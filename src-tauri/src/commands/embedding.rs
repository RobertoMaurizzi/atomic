//! Embedding and similarity search operations

use crate::db::{Database, SharedDatabase};
use crate::embedding::{distance_to_similarity, spawn_embedding_task_single};
use crate::models::{Atom, AtomWithTags, SemanticSearchResult, SimilarAtomResult};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;


/// Find similar atoms based on vector similarity
/// 1. Get all chunks for the given atom
/// 2. For each chunk, find similar chunks in vec_chunks
/// 3. Filter by threshold (convert distance to similarity)
/// 4. Deduplicate by parent atom_id, keep highest similarity
/// 5. Exclude the source atom itself
/// 6. Return up to `limit` results
#[tauri::command]
pub fn find_similar_atoms(
    db: State<Database>,
    atom_id: String,
    limit: i32,
    threshold: f32,
) -> Result<Vec<SimilarAtomResult>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    // 1. Get all chunks for the given atom
    let mut stmt = conn
        .prepare("SELECT id, embedding FROM atom_chunks WHERE atom_id = ?1")
        .map_err(|e| format!("Failed to prepare chunk query: {}", e))?;

    let source_chunks: Vec<(String, Vec<u8>)> = stmt
        .query_map([&atom_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| format!("Failed to query chunks: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect chunks: {}", e))?;

    if source_chunks.is_empty() {
        return Ok(Vec::new());
    }

    // Map to store best similarity per atom_id
    let mut atom_similarities: HashMap<String, (f32, String, i32)> = HashMap::new();

    // 2. For each source chunk, find similar chunks
    for (_, embedding_blob) in &source_chunks {
        // Query vec_chunks for similar chunks
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
            .query_map(rusqlite::params![embedding_blob, limit * 10], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(|e| format!("Failed to query similar chunks: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect similar chunks: {}", e))?;

        // 3. For each similar chunk, get its parent atom and check threshold
        for (chunk_id, distance) in similar_chunks {
            let similarity = distance_to_similarity(distance);

            if similarity < threshold {
                continue;
            }

            // Get the parent atom_id and chunk info for this chunk
            let chunk_info: Result<(String, String, i32), _> = conn.query_row(
                "SELECT atom_id, content, chunk_index FROM atom_chunks WHERE id = ?1",
                [&chunk_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            );

            if let Ok((parent_atom_id, chunk_content, chunk_index)) = chunk_info {
                // 5. Exclude the source atom itself
                if parent_atom_id == atom_id {
                    continue;
                }

                // 4. Keep highest similarity per atom
                let entry = atom_similarities.entry(parent_atom_id.clone());
                match entry {
                    std::collections::hash_map::Entry::Occupied(mut e) => {
                        if similarity > e.get().0 {
                            e.insert((similarity, chunk_content, chunk_index));
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(e) => {
                        e.insert((similarity, chunk_content, chunk_index));
                    }
                }
            }
        }
    }

    // 6. Build results, sorted by similarity
    let mut results: Vec<(String, f32, String, i32)> = atom_similarities
        .into_iter()
        .map(|(atom_id, (sim, content, idx))| (atom_id, sim, content, idx))
        .collect();

    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit as usize);

    // Fetch atom data for results (truncated content since UI only shows 100 chars)
    let mut final_results = Vec::new();
    for (result_atom_id, similarity, chunk_content, chunk_index) in results {
        let atom: Atom = conn
            .query_row(
                "SELECT id, SUBSTR(content, 1, 150) as content, source_url, created_at, updated_at, COALESCE(embedding_status, 'pending'), COALESCE(tagging_status, 'pending') FROM atoms WHERE id = ?1",
                [&result_atom_id],
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

        // Tags not needed - RelatedAtoms UI doesn't display them
        final_results.push(SimilarAtomResult {
            atom: AtomWithTags { atom, tags: vec![] },
            similarity_score: similarity,
            matching_chunk_content: chunk_content,
            matching_chunk_index: chunk_index,
        });
    }

    Ok(final_results)
}

/// Search atoms using vector similarity (semantic search)
/// Delegates to the unified search module
#[tauri::command]
pub async fn search_atoms_semantic(
    db: State<'_, Database>,
    query: String,
    limit: i32,
    threshold: f32,
) -> Result<Vec<SemanticSearchResult>, String> {
    let options =
        crate::search::SearchOptions::new(query, crate::search::SearchMode::Semantic, limit)
            .with_threshold(threshold);
    crate::search::search_atoms(db.as_core(), options).await
}

/// Search atoms using BM25 keyword search (FTS5)
/// Delegates to the unified search module
#[tauri::command]
pub async fn search_atoms_keyword(
    db: State<'_, Database>,
    query: String,
    limit: i32,
) -> Result<Vec<SemanticSearchResult>, String> {
    let options =
        crate::search::SearchOptions::new(query, crate::search::SearchMode::Keyword, limit);
    crate::search::search_atoms(db.as_core(), options).await
}

/// Search atoms using hybrid search (combines BM25 keyword + vector semantic)
/// Delegates to the unified search module
#[tauri::command]
pub async fn search_atoms_hybrid(
    db: State<'_, Database>,
    query: String,
    limit: i32,
    threshold: f32,
) -> Result<Vec<SemanticSearchResult>, String> {
    let options =
        crate::search::SearchOptions::new(query, crate::search::SearchMode::Hybrid, limit)
            .with_threshold(threshold);
    crate::search::search_atoms(db.as_core(), options).await
}

/// Retry embedding generation for a failed atom
/// Reset status to 'pending' and trigger embedding again
#[tauri::command]
pub fn retry_embedding(
    app_handle: tauri::AppHandle,
    db: State<Database>,
    shared_db: State<SharedDatabase>,
    atom_id: String,
) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    // Get the atom content
    let content: String = conn
        .query_row(
            "SELECT content FROM atoms WHERE id = ?1",
            [&atom_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to get atom: {}", e))?;

    // Reset status to pending
    conn.execute(
        "UPDATE atoms SET embedding_status = 'pending' WHERE id = ?1",
        [&atom_id],
    )
    .map_err(|e| e.to_string())?;

    // Drop the connection lock before spawning the embedding task
    drop(conn);

    // Spawn embedding task
    spawn_embedding_task_single(app_handle, Arc::clone(&shared_db), atom_id, content);

    Ok(())
}

/// Reset atoms stuck in 'processing' state back to 'pending'
/// Call this on app startup to recover from interrupted sessions
#[tauri::command]
pub fn reset_stuck_processing(db: State<Database>) -> Result<i32, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    // Reset embedding_status from 'processing' to 'pending'
    let embedding_reset = conn
        .execute(
            "UPDATE atoms SET embedding_status = 'pending' WHERE embedding_status = 'processing'",
            [],
        )
        .map_err(|e| format!("Failed to reset stuck embeddings: {}", e))?;

    // Reset tagging_status from 'processing' to 'pending'
    let tagging_reset = conn
        .execute(
            "UPDATE atoms SET tagging_status = 'pending' WHERE tagging_status = 'processing'",
            [],
        )
        .map_err(|e| format!("Failed to reset stuck tagging: {}", e))?;

    let total = (embedding_reset + tagging_reset) as i32;
    if total > 0 {
        eprintln!(
            "Reset {} stuck atoms (embedding: {}, tagging: {})",
            total, embedding_reset, tagging_reset
        );
    }

    Ok(total)
}

/// Trigger embedding generation for all atoms with 'pending' status
/// Uses async batch processing with semaphore to prevent thread exhaustion
#[tauri::command]
pub async fn process_pending_embeddings(
    app_handle: tauri::AppHandle,
    db: State<'_, Database>,
    shared_db: State<'_, SharedDatabase>,
) -> Result<i32, String> {
    // Atomically fetch and mark pending atoms as 'processing' in a single statement
    // This prevents race conditions from duplicate calls (e.g., React StrictMode)
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "UPDATE atoms SET embedding_status = 'processing'
             WHERE embedding_status = 'pending'
             RETURNING id, content",
        )
        .map_err(|e| format!("Failed to prepare query: {}", e))?;
    let pending_atoms: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| format!("Failed to query pending atoms: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect pending atoms: {}", e))?;
    drop(stmt);
    drop(conn);

    let count = pending_atoms.len() as i32;

    // Process batch asynchronously (with tagging)
    tokio::spawn(crate::embedding::process_embedding_batch(
        app_handle,
        Arc::clone(&shared_db),
        pending_atoms,
        false, // don't skip tagging - normal flow
    ));

    Ok(count)
}

/// Get the embedding status for an atom
#[tauri::command]
pub fn get_embedding_status(db: State<Database>, atom_id: String) -> Result<String, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    let status: String = conn
        .query_row(
            "SELECT COALESCE(embedding_status, 'pending') FROM atoms WHERE id = ?1",
            [&atom_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to get embedding status: {}", e))?;

    Ok(status)
}

/// Trigger tag extraction for all atoms with completed embeddings but pending tagging
/// Uses async batch processing with semaphore to prevent thread exhaustion
#[tauri::command]
pub async fn process_pending_tagging(
    app_handle: tauri::AppHandle,
    db: State<'_, Database>,
    shared_db: State<'_, SharedDatabase>,
) -> Result<i32, String> {
    // Atomically fetch and mark pending tagging atoms as 'processing' in a single statement
    // This prevents race conditions from duplicate calls (e.g., React StrictMode)
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "UPDATE atoms SET tagging_status = 'processing'
             WHERE embedding_status = 'complete'
             AND tagging_status = 'pending'
             RETURNING id",
        )
        .map_err(|e| format!("Failed to prepare query: {}", e))?;
    let pending_atoms: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| format!("Failed to query pending tagging atoms: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect pending tagging atoms: {}", e))?;
    drop(stmt);
    drop(conn);

    let count = pending_atoms.len() as i32;

    // Process batch asynchronously
    tokio::spawn(crate::embedding::process_tagging_batch(
        app_handle,
        Arc::clone(&shared_db),
        pending_atoms,
    ));

    Ok(count)
}
