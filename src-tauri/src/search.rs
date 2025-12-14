//! Unified search module for Atomic
//!
//! Provides a single search implementation that supports keyword (BM25), semantic (vector),
//! and hybrid (RRF-combined) search modes. Used by UI, chat agent, MCP, and wiki generation.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::embedding::{distance_to_similarity, f32_vec_to_blob_public};
use crate::models::{Atom, AtomWithTags, SemanticSearchResult, Tag};
use crate::providers::{create_embedding_provider, EmbeddingConfig, ProviderConfig};
use crate::settings::get_all_settings;

/// Search mode - determines which search algorithm(s) to use
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchMode {
    /// BM25 keyword search using FTS5
    Keyword,
    /// Vector similarity search using embeddings
    Semantic,
    /// Combines keyword and semantic using Reciprocal Rank Fusion
    Hybrid,
}

/// Options for search queries
#[derive(Debug, Clone)]
pub struct SearchOptions {
    /// The search query text
    pub query: String,
    /// Search algorithm to use
    pub mode: SearchMode,
    /// Maximum number of results to return
    pub limit: i32,
    /// Minimum similarity threshold (0.0-1.0) for semantic/hybrid modes
    pub threshold: f32,
    /// Optional tag IDs to filter results (only return atoms with these tags)
    pub scope_tag_ids: Vec<String>,
}

impl SearchOptions {
    pub fn new(query: impl Into<String>, mode: SearchMode, limit: i32) -> Self {
        Self {
            query: query.into(),
            mode,
            limit,
            threshold: 0.3,
            scope_tag_ids: vec![],
        }
    }

    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold;
        self
    }

    pub fn with_scope(mut self, tag_ids: Vec<String>) -> Self {
        self.scope_tag_ids = tag_ids;
        self
    }
}

/// A single chunk result from search
#[derive(Debug, Clone)]
pub struct ChunkResult {
    pub chunk_id: String,
    pub atom_id: String,
    pub content: String,
    pub chunk_index: i32,
    /// Normalized score (0.0-1.0), higher is better
    pub score: f32,
}

/// RRF constant - standard value that prevents high ranks from dominating
const RRF_K: f32 = 60.0;

/// Escape special characters for FTS5 MATCH query
/// Wraps each word in quotes to treat them as literal terms
fn escape_fts5_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|word| {
            let cleaned = word.replace('"', "");
            if cleaned.is_empty() {
                String::new()
            } else {
                format!("\"{}\"", cleaned)
            }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Normalize BM25 score to 0-1 range
/// BM25 scores are negative (lower = better), typically -30 to 0
fn normalize_bm25_score(score: f64) -> f32 {
    let clamped = score.clamp(-30.0, 0.0);
    (1.0 - (clamped / -30.0) * 0.7) as f32
}

/// Core search function - returns raw chunks without atom deduplication
///
/// Use this when you need multiple chunks per atom (e.g., wiki generation).
/// For most UI cases, use `search_atoms()` instead.
pub async fn search_chunks(
    db: &Database,
    options: SearchOptions,
) -> Result<Vec<ChunkResult>, String> {
    match options.mode {
        SearchMode::Keyword => search_keyword_chunks(db, &options).await,
        SearchMode::Semantic => search_semantic_chunks(db, &options).await,
        SearchMode::Hybrid => search_hybrid_chunks(db, &options).await,
    }
}

/// Search and return deduplicated atoms with full data
///
/// This is the main entry point for UI/chat/MCP search. Returns one result per atom
/// with the best-matching chunk info attached.
pub async fn search_atoms(
    db: &Database,
    options: SearchOptions,
) -> Result<Vec<SemanticSearchResult>, String> {
    // Get raw chunk results
    let chunks = search_chunks(db, options.clone()).await?;

    // Deduplicate by atom_id, keeping highest score per atom
    let mut atom_best: HashMap<String, ChunkResult> = HashMap::new();
    for chunk in chunks {
        let entry = atom_best.entry(chunk.atom_id.clone());
        match entry {
            std::collections::hash_map::Entry::Occupied(mut e) => {
                if chunk.score > e.get().score {
                    e.insert(chunk);
                }
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(chunk);
            }
        }
    }

    // Sort by score descending and limit
    let mut deduped: Vec<ChunkResult> = atom_best.into_values().collect();
    deduped.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    deduped.truncate(options.limit as usize);

    // Fetch full atom data
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let mut results = Vec::with_capacity(deduped.len());

    for chunk in deduped {
        let atom: Atom = conn
            .query_row(
                "SELECT id, content, source_url, created_at, updated_at,
                 COALESCE(embedding_status, 'pending'), COALESCE(tagging_status, 'pending')
                 FROM atoms WHERE id = ?1",
                [&chunk.atom_id],
                |row| {
                    Ok(Atom {
                        id: row.get(0)?,
                        content: row.get(1)?,
                        source_url: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                        embedding_status: row.get(5)?,
                        tagging_status: row.get(6)?,
                    })
                },
            )
            .map_err(|e| format!("Failed to get atom: {}", e))?;

        let tags = get_tags_for_atom(&conn, &chunk.atom_id)?;

        results.push(SemanticSearchResult {
            atom: AtomWithTags { atom, tags },
            similarity_score: chunk.score,
            matching_chunk_content: chunk.content,
            matching_chunk_index: chunk.chunk_index,
        });
    }

    Ok(results)
}

/// Get tags for an atom
fn get_tags_for_atom(conn: &rusqlite::Connection, atom_id: &str) -> Result<Vec<Tag>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.name, t.parent_id, t.created_at
             FROM tags t
             INNER JOIN atom_tags at ON t.id = at.tag_id
             WHERE at.atom_id = ?1",
        )
        .map_err(|e| e.to_string())?;

    let tags = stmt
        .query_map([atom_id], |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
                parent_id: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(tags)
}

/// Keyword search using FTS5/BM25
async fn search_keyword_chunks(
    db: &Database,
    options: &SearchOptions,
) -> Result<Vec<ChunkResult>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    let mut fts_stmt = conn
        .prepare(
            "SELECT chunk_id, atom_id, content, chunk_index, bm25(atom_chunks_fts) as score
             FROM atom_chunks_fts
             WHERE atom_chunks_fts MATCH ?1
             ORDER BY bm25(atom_chunks_fts)
             LIMIT ?2",
        )
        .map_err(|e| format!("Failed to prepare FTS query: {}", e))?;

    let escaped_query = escape_fts5_query(&options.query);
    let fetch_limit = options.limit * 5; // Fetch extra for filtering

    let raw_results: Vec<(String, String, String, i32, f64)> = fts_stmt
        .query_map(rusqlite::params![&escaped_query, fetch_limit], |row| {
            Ok((
                row.get(0)?, // chunk_id
                row.get(1)?, // atom_id
                row.get(2)?, // content
                row.get(3)?, // chunk_index
                row.get(4)?, // BM25 score
            ))
        })
        .map_err(|e| format!("Failed to query FTS: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect FTS results: {}", e))?;

    // Apply tag scope filter if specified
    let filtered = filter_by_scope(&conn, raw_results, &options.scope_tag_ids)?;

    // Convert to ChunkResult with normalized scores
    Ok(filtered
        .into_iter()
        .map(|(chunk_id, atom_id, content, chunk_index, bm25_score)| ChunkResult {
            chunk_id,
            atom_id,
            content,
            chunk_index,
            score: normalize_bm25_score(bm25_score),
        })
        .collect())
}

/// Semantic search using vector similarity
async fn search_semantic_chunks(
    db: &Database,
    options: &SearchOptions,
) -> Result<Vec<ChunkResult>, String> {
    // Get provider config from settings
    let provider_config = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let settings_map = get_all_settings(&conn)?;
        ProviderConfig::from_settings(&settings_map)
    };

    // Create embedding provider and generate query embedding
    let provider = create_embedding_provider(&provider_config)
        .map_err(|e| format!("Failed to create embedding provider: {}", e))?;
    let embedding_config = EmbeddingConfig::new(provider_config.embedding_model());
    let embeddings = provider
        .embed_batch(&[options.query.clone()], &embedding_config)
        .await
        .map_err(|e| format!("Failed to generate query embedding: {}", e))?;

    let query_blob = f32_vec_to_blob_public(&embeddings[0]);

    // Query vec_chunks
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let fetch_limit = options.limit * 10;

    let mut vec_stmt = conn
        .prepare(
            "SELECT chunk_id, distance
             FROM vec_chunks
             WHERE embedding MATCH ?1
             ORDER BY distance
             LIMIT ?2",
        )
        .map_err(|e| format!("Failed to prepare vec query: {}", e))?;

    let similar_chunks: Vec<(String, f32)> = vec_stmt
        .query_map(rusqlite::params![&query_blob, fetch_limit], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(|e| format!("Failed to query similar chunks: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect similar chunks: {}", e))?;

    // Get chunk details and filter
    let mut results = Vec::new();
    for (chunk_id, distance) in similar_chunks {
        let similarity = distance_to_similarity(distance);

        if similarity < options.threshold {
            continue;
        }

        let chunk_info: Result<(String, String, i32), _> = conn.query_row(
            "SELECT atom_id, content, chunk_index FROM atom_chunks WHERE id = ?1",
            [&chunk_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        );

        if let Ok((atom_id, content, chunk_index)) = chunk_info {
            // Check tag scope if specified
            if !options.scope_tag_ids.is_empty() && !atom_has_scope_tag(&conn, &atom_id, &options.scope_tag_ids)? {
                continue;
            }

            results.push(ChunkResult {
                chunk_id,
                atom_id,
                content,
                chunk_index,
                score: similarity,
            });
        }
    }

    Ok(results)
}

/// Hybrid search combining keyword and semantic with RRF
async fn search_hybrid_chunks(
    db: &Database,
    options: &SearchOptions,
) -> Result<Vec<ChunkResult>, String> {
    // Get provider config from settings
    let provider_config = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let settings_map = get_all_settings(&conn)?;
        ProviderConfig::from_settings(&settings_map)
    };

    let fetch_limit = options.limit * 5;

    // Phase 1: Keyword search
    let keyword_results: Vec<(String, String, String, i32)> = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;

        let mut fts_stmt = conn
            .prepare(
                "SELECT chunk_id, atom_id, content, chunk_index
                 FROM atom_chunks_fts
                 WHERE atom_chunks_fts MATCH ?1
                 ORDER BY bm25(atom_chunks_fts)
                 LIMIT ?2",
            )
            .map_err(|e| format!("Failed to prepare FTS query: {}", e))?;

        let escaped_query = escape_fts5_query(&options.query);

        let results: Vec<(String, String, String, i32)> = fts_stmt
            .query_map(rusqlite::params![&escaped_query, fetch_limit], |row| {
                Ok((
                    row.get::<_, String>(0)?, // chunk_id
                    row.get::<_, String>(1)?, // atom_id
                    row.get::<_, String>(2)?, // content
                    row.get::<_, i32>(3)?,    // chunk_index
                ))
            })
            .map_err(|e| format!("Failed to query FTS: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect FTS results: {}", e))?;

        // Filter by scope
        if !options.scope_tag_ids.is_empty() {
            results
                .into_iter()
                .filter(|(_, atom_id, _, _)| {
                    atom_has_scope_tag(&conn, atom_id, &options.scope_tag_ids).unwrap_or(false)
                })
                .collect()
        } else {
            results
        }
    };

    // Phase 2: Semantic search
    let provider = create_embedding_provider(&provider_config)
        .map_err(|e| format!("Failed to create embedding provider: {}", e))?;
    let embedding_config = EmbeddingConfig::new(provider_config.embedding_model());
    let embeddings = provider
        .embed_batch(&[options.query.clone()], &embedding_config)
        .await
        .map_err(|e| format!("Failed to generate query embedding: {}", e))?;

    let query_blob = f32_vec_to_blob_public(&embeddings[0]);

    let semantic_results: Vec<(String, String, String, i32, f32)> = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;

        let mut vec_stmt = conn
            .prepare(
                "SELECT chunk_id, distance
                 FROM vec_chunks
                 WHERE embedding MATCH ?1
                 ORDER BY distance
                 LIMIT ?2",
            )
            .map_err(|e| format!("Failed to prepare vec query: {}", e))?;

        let similar_chunks: Vec<(String, f32)> = vec_stmt
            .query_map(rusqlite::params![&query_blob, fetch_limit], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(|e| format!("Failed to query similar chunks: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect similar chunks: {}", e))?;

        let mut results = Vec::new();
        for (chunk_id, distance) in similar_chunks {
            let similarity = distance_to_similarity(distance);

            if similarity < options.threshold {
                continue;
            }

            let chunk_info: Result<(String, String, i32), _> = conn.query_row(
                "SELECT atom_id, content, chunk_index FROM atom_chunks WHERE id = ?1",
                [&chunk_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            );

            if let Ok((atom_id, content, chunk_index)) = chunk_info {
                if !options.scope_tag_ids.is_empty()
                    && !atom_has_scope_tag(&conn, &atom_id, &options.scope_tag_ids)?
                {
                    continue;
                }

                results.push((chunk_id, atom_id, content, chunk_index, similarity));
            }
        }
        results
    };

    // Phase 3: Combine with RRF
    // RRF score = 1/(k + rank) summed across result sets
    let mut chunk_scores: HashMap<String, (f32, String, String, i32)> = HashMap::new();

    // Add keyword results with RRF scores
    for (rank, (chunk_id, atom_id, content, chunk_index)) in keyword_results.iter().enumerate() {
        let rrf = 1.0 / (RRF_K + (rank + 1) as f32);
        chunk_scores.insert(
            chunk_id.clone(),
            (rrf, atom_id.clone(), content.clone(), *chunk_index),
        );
    }

    // Add semantic results with RRF scores
    for (rank, (chunk_id, atom_id, content, chunk_index, _)) in semantic_results.iter().enumerate() {
        let rrf = 1.0 / (RRF_K + (rank + 1) as f32);
        chunk_scores
            .entry(chunk_id.clone())
            .and_modify(|(score, _, _, _)| *score += rrf)
            .or_insert((rrf, atom_id.clone(), content.clone(), *chunk_index));
    }

    // Sort by RRF score and convert to ChunkResult
    let mut combined: Vec<ChunkResult> = chunk_scores
        .into_iter()
        .map(|(chunk_id, (score, atom_id, content, chunk_index))| {
            // Normalize RRF score to 0-1 range
            let max_rrf = 2.0 / (RRF_K + 1.0);
            ChunkResult {
                chunk_id,
                atom_id,
                content,
                chunk_index,
                score: (score / max_rrf).clamp(0.0, 1.0),
            }
        })
        .collect();

    combined.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    Ok(combined)
}

/// Check if an atom has any of the specified scope tags
fn atom_has_scope_tag(
    conn: &rusqlite::Connection,
    atom_id: &str,
    scope_tag_ids: &[String],
) -> Result<bool, String> {
    if scope_tag_ids.is_empty() {
        return Ok(true);
    }

    let placeholders: Vec<&str> = scope_tag_ids.iter().map(|_| "?").collect();
    let query = format!(
        "SELECT EXISTS(SELECT 1 FROM atom_tags WHERE atom_id = ?1 AND tag_id IN ({}))",
        placeholders.join(",")
    );

    let mut params: Vec<&dyn rusqlite::ToSql> = vec![&atom_id];
    for tag_id in scope_tag_ids {
        params.push(tag_id);
    }

    conn.query_row(&query, rusqlite::params_from_iter(params), |row| row.get(0))
        .map_err(|e| format!("Failed to check tag scope: {}", e))
}

/// Filter results by tag scope (for keyword search batch filtering)
fn filter_by_scope<T>(
    conn: &rusqlite::Connection,
    results: Vec<(String, String, String, i32, T)>,
    scope_tag_ids: &[String],
) -> Result<Vec<(String, String, String, i32, T)>, String> {
    if scope_tag_ids.is_empty() {
        return Ok(results);
    }

    let mut filtered = Vec::new();
    for result in results {
        if atom_has_scope_tag(conn, &result.1, scope_tag_ids)? {
            filtered.push(result);
        }
    }
    Ok(filtered)
}
