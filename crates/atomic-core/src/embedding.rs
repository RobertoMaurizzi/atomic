//! Embedding generation pipeline with callback-based events
//!
//! This module handles:
//! - Embedding generation via provider abstraction
//! - Tag extraction via LLM
//! - Semantic edge computation
//! - Callback-based event notification

use crate::chunking::chunk_content;
use crate::db::Database;
use crate::extraction::{
    build_tag_info_for_consolidation, cleanup_orphaned_parents, consolidate_atom_tags,
    extract_tags_from_chunk, get_or_create_tag, get_tag_tree_for_llm, link_tags_to_atom,
    tag_names_to_ids,
};
use crate::providers::models::{
    fetch_and_return_capabilities, get_cached_capabilities_sync, save_capabilities_cache,
};
use crate::providers::traits::EmbeddingConfig;
use crate::providers::{create_embedding_provider, ProviderConfig, ProviderType};
use crate::settings;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, LazyLock};
use tokio::sync::Semaphore;
use uuid::Uuid;

// Limit concurrent embedding tasks to prevent thread exhaustion
const MAX_CONCURRENT_EMBEDDINGS: usize = 1;
const MAX_CONCURRENT_TAGGING: usize = 1;

static EMBEDDING_SEMAPHORE: LazyLock<Semaphore> =
    LazyLock::new(|| Semaphore::new(MAX_CONCURRENT_EMBEDDINGS));

static TAGGING_SEMAPHORE: LazyLock<Semaphore> =
    LazyLock::new(|| Semaphore::new(MAX_CONCURRENT_TAGGING));

/// Events emitted during the embedding/tagging pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EmbeddingEvent {
    /// Embedding generation started for an atom
    Started { atom_id: String },
    /// Embedding generation completed successfully
    EmbeddingComplete { atom_id: String },
    /// Embedding generation failed
    EmbeddingFailed { atom_id: String, error: String },
    /// Tag extraction completed
    TaggingComplete {
        atom_id: String,
        tags_extracted: Vec<String>,
        new_tags_created: Vec<String>,
    },
    /// Tag extraction failed
    TaggingFailed { atom_id: String, error: String },
    /// Tag extraction was skipped (disabled or no API key)
    TaggingSkipped { atom_id: String },
}

/// Generate embeddings via provider abstraction (batch support)
/// Uses ProviderConfig to determine which provider to use
pub async fn generate_embeddings_with_config(
    config: &ProviderConfig,
    texts: &[String],
) -> Result<Vec<Vec<f32>>, String> {
    let provider = create_embedding_provider(config).map_err(|e| e.to_string())?;
    let embed_config = EmbeddingConfig::new(config.embedding_model());

    provider
        .embed_batch(texts, &embed_config)
        .await
        .map_err(|e| e.to_string())
}

/// Generate embeddings via OpenRouter API (batch support)
/// DEPRECATED: Use generate_embeddings_with_config instead
/// Kept for backward compatibility with existing code
pub async fn generate_openrouter_embeddings_public(
    _client: &Client,
    api_key: &str,
    texts: &[String],
) -> Result<Vec<Vec<f32>>, String> {
    use crate::providers::openrouter::OpenRouterProvider;
    use crate::providers::traits::EmbeddingProvider;

    let provider = OpenRouterProvider::new(api_key.to_string());
    let config = EmbeddingConfig::new("openai/text-embedding-3-small");

    provider
        .embed_batch(texts, &config)
        .await
        .map_err(|e| e.to_string())
}

/// Convert f32 vector to binary blob for sqlite-vec
pub fn f32_vec_to_blob_public(vec: &[f32]) -> Vec<u8> {
    vec.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Process ONLY embedding generation for an atom (no tag extraction)
/// This is the fast phase - just embedding API calls
///
/// Steps:
/// 1. Set embedding_status to 'processing'
/// 2. Delete existing chunks
/// 3. Chunk content
/// 4. Generate embeddings via provider
/// 5. Store chunks and embeddings
/// 6. Compute semantic edges
/// 7. Set embedding_status to 'complete'
pub async fn process_embedding_only(
    db: &Database,
    atom_id: &str,
    content: &str,
) -> Result<(), String> {
    // Scope for initial DB operations
    let (provider_config, chunks) = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;

        // Set embedding status to processing
        conn.execute(
            "UPDATE atoms SET embedding_status = 'processing' WHERE id = ?1",
            [atom_id],
        )
        .map_err(|e| e.to_string())?;

        // Get settings for embeddings
        let settings_map = settings::get_all_settings(&conn).map_err(|e| e.to_string())?;
        let provider_config = ProviderConfig::from_settings(&settings_map);

        // Validate provider configuration
        if provider_config.provider_type == ProviderType::OpenRouter
            && provider_config.openrouter_api_key.is_none()
        {
            return Err(
                "OpenRouter API key not configured. Please set it in Settings.".to_string(),
            );
        }

        // Delete existing chunks for this atom
        let existing_chunk_ids: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT id FROM atom_chunks WHERE atom_id = ?1")
                .map_err(|e| format!("Failed to prepare chunk query: {}", e))?;
            let ids = stmt
                .query_map([atom_id], |row| row.get(0))
                .map_err(|e| format!("Failed to query chunks: {}", e))?
                .collect::<Result<Vec<String>, _>>()
                .map_err(|e| format!("Failed to collect chunk IDs: {}", e))?;
            ids
        };

        for chunk_id in &existing_chunk_ids {
            conn.execute("DELETE FROM vec_chunks WHERE chunk_id = ?1", [chunk_id])
                .ok();
        }
        conn.execute("DELETE FROM atom_chunks WHERE atom_id = ?1", [atom_id])
            .map_err(|e| e.to_string())?;

        // Also delete from FTS5 table
        conn.execute(
            "DELETE FROM atom_chunks_fts WHERE atom_id = ?1",
            [atom_id],
        )
        .ok();

        // Chunk content
        let chunks = chunk_content(content);

        if chunks.is_empty() {
            // No chunks to process, mark embedding as complete, tagging as skipped
            conn.execute(
                "UPDATE atoms SET embedding_status = 'complete', tagging_status = 'skipped' WHERE id = ?1",
                [atom_id],
            )
            .map_err(|e| e.to_string())?;
            return Ok(());
        }

        (provider_config, chunks)
    }; // Connection dropped here

    // Generate all embeddings in one batch (async, no lock)
    let chunk_texts: Vec<String> = chunks.iter().map(|s| s.to_string()).collect();
    let embeddings = generate_embeddings_with_config(&provider_config, &chunk_texts)
        .await
        .map_err(|e| format!("Failed to generate embeddings: {}", e))?;

    // Store chunks and embeddings
    {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;

        for (index, chunk_content) in chunks.iter().enumerate() {
            let chunk_id = Uuid::new_v4().to_string();
            let embedding_blob = f32_vec_to_blob_public(&embeddings[index]);

            // Insert into atom_chunks
            conn.execute(
                "INSERT INTO atom_chunks (id, atom_id, chunk_index, content, embedding) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![&chunk_id, atom_id, index as i32, chunk_content, &embedding_blob],
            )
            .map_err(|e| format!("Failed to insert chunk: {}", e))?;

            // Insert into vec_chunks for similarity search
            conn.execute(
                "INSERT INTO vec_chunks (chunk_id, embedding) VALUES (?1, ?2)",
                rusqlite::params![&chunk_id, &embedding_blob],
            )
            .map_err(|e| format!("Failed to insert vec_chunk: {}", e))?;

            // Insert into FTS5 table for keyword search
            conn.execute(
                "INSERT INTO atom_chunks_fts (chunk_id, atom_id, content, chunk_index) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![&chunk_id, atom_id, chunk_content, index as i32],
            )
            .map_err(|e| format!("Failed to insert into FTS5: {}", e))?;
        }

        // Compute semantic edges for this atom
        match compute_semantic_edges_for_atom(&conn, atom_id, 0.5, 15) {
            Ok(edge_count) => {
                if edge_count > 0 {
                    eprintln!(
                        "Created {} semantic edges for atom {}",
                        edge_count, atom_id
                    );
                }
            }
            Err(e) => {
                eprintln!(
                    "Warning: Failed to compute semantic edges for atom {}: {}",
                    atom_id, e
                );
            }
        }

        // Set embedding status to complete
        conn.execute(
            "UPDATE atoms SET embedding_status = 'complete' WHERE id = ?1",
            [atom_id],
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Process ONLY tag extraction for an atom (no embedding)
/// Assumes atom_chunks already exist with content
///
/// Steps:
/// 1. Check embedding_status is 'complete' (skip if not)
/// 2. Set tagging_status to 'processing'
/// 3. Check auto_tagging_enabled (skip if disabled)
/// 4. Read chunks from atom_chunks table
/// 5. Extract tags via LLM for each chunk
/// 6. Run consolidation pass for multi-chunk atoms
/// 7. Set tagging_status to 'complete'
pub async fn process_tagging_only(
    db: &Database,
    atom_id: &str,
) -> Result<(Vec<String>, Vec<String>), String> {
    // Get settings and validate state
    let (auto_tagging_enabled, provider_config, tagging_model, chunks) = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;

        // Check embedding status - can't tag without complete embeddings
        let embedding_status: String = conn
            .query_row(
                "SELECT COALESCE(embedding_status, 'pending') FROM atoms WHERE id = ?1",
                [atom_id],
                |row| row.get(0),
            )
            .map_err(|e| format!("Atom not found: {}", e))?;

        if embedding_status != "complete" {
            conn.execute(
                "UPDATE atoms SET tagging_status = 'skipped' WHERE id = ?1",
                [atom_id],
            )
            .map_err(|e| e.to_string())?;
            return Ok((Vec::new(), Vec::new()));
        }

        // Set tagging status to processing
        conn.execute(
            "UPDATE atoms SET tagging_status = 'processing' WHERE id = ?1",
            [atom_id],
        )
        .map_err(|e| e.to_string())?;

        // Get settings
        let settings_map = settings::get_all_settings(&conn).map_err(|e| e.to_string())?;
        let auto_tagging_enabled = settings_map
            .get("auto_tagging_enabled")
            .map(|v| v == "true")
            .unwrap_or(true);

        if !auto_tagging_enabled {
            conn.execute(
                "UPDATE atoms SET tagging_status = 'skipped' WHERE id = ?1",
                [atom_id],
            )
            .map_err(|e| e.to_string())?;
            return Ok((Vec::new(), Vec::new()));
        }

        let provider_config = ProviderConfig::from_settings(&settings_map);

        // Validate provider for LLM
        if provider_config.provider_type == ProviderType::OpenRouter
            && provider_config.openrouter_api_key.is_none()
        {
            conn.execute(
                "UPDATE atoms SET tagging_status = 'skipped' WHERE id = ?1",
                [atom_id],
            )
            .map_err(|e| e.to_string())?;
            return Ok((Vec::new(), Vec::new()));
        }

        let tagging_model = provider_config.llm_model().to_string();

        // Get chunks from database (already created during embedding)
        let mut stmt = conn
            .prepare("SELECT content FROM atom_chunks WHERE atom_id = ?1 ORDER BY chunk_index")
            .map_err(|e| format!("Failed to prepare chunk query: {}", e))?;
        let chunks: Vec<String> = stmt
            .query_map([atom_id], |row| row.get(0))
            .map_err(|e| format!("Failed to query chunks: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect chunks: {}", e))?;

        if chunks.is_empty() {
            conn.execute(
                "UPDATE atoms SET tagging_status = 'skipped' WHERE id = ?1",
                [atom_id],
            )
            .map_err(|e| e.to_string())?;
            return Ok((Vec::new(), Vec::new()));
        }

        (auto_tagging_enabled, provider_config, tagging_model, chunks)
    }; // Connection dropped

    // Load model capabilities for OpenRouter
    let supported_params: Option<Vec<String>> =
        if provider_config.provider_type == ProviderType::OpenRouter {
            let client = Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| Client::new());

            let (cached, is_stale) = {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;
                match get_cached_capabilities_sync(&conn) {
                    Ok(Some(cache)) => {
                        let stale = cache.is_stale();
                        (Some(cache), stale)
                    }
                    Ok(None) => (None, true),
                    Err(_) => (None, true),
                }
            };

            let capabilities = if is_stale {
                match fetch_and_return_capabilities(&client).await {
                    Ok(fresh_cache) => {
                        if let Ok(conn) = db.new_connection() {
                            let _ = save_capabilities_cache(&conn, &fresh_cache);
                        }
                        fresh_cache
                    }
                    Err(_) => cached.unwrap_or_default(),
                }
            } else {
                cached.unwrap_or_default()
            };

            capabilities.get_supported_params(&tagging_model).cloned()
        } else {
            None
        };

    // Track all tags
    let mut all_tag_ids: Vec<String> = Vec::new();
    let mut all_new_tag_ids: Vec<String> = Vec::new();

    // Process each chunk for tag extraction
    for (index, chunk_content) in chunks.iter().enumerate() {
        // Get fresh tag tree that includes tags from previous chunks
        let tag_tree_json = {
            let conn = db.conn.lock().map_err(|e| e.to_string())?;
            get_tag_tree_for_llm(&conn)?
        };

        match extract_tags_from_chunk(
            &provider_config,
            chunk_content,
            &tag_tree_json,
            &tagging_model,
            supported_params.clone(),
        )
        .await
        {
            Ok(result) => {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;
                let mut chunk_tag_ids = Vec::new();

                for tag_application in result.tags {
                    let trimmed_name = tag_application.name.trim();
                    if trimmed_name.is_empty() || trimmed_name.eq_ignore_ascii_case("null") {
                        continue;
                    }

                    match get_or_create_tag(&conn, &tag_application.name, &tag_application.parent_name)
                    {
                        Ok(tag_id) => chunk_tag_ids.push(tag_id),
                        Err(e) => eprintln!(
                            "Failed to get/create tag '{}': {}",
                            tag_application.name, e
                        ),
                    }
                }

                if !chunk_tag_ids.is_empty() {
                    link_tags_to_atom(&conn, atom_id, &chunk_tag_ids)?;
                }

                all_tag_ids.extend(chunk_tag_ids.clone());
                all_new_tag_ids.extend(chunk_tag_ids);
            }
            Err(e) => {
                eprintln!("Tag extraction failed for chunk {}: {}", index, e);
            }
        }
    }

    // Consolidation pass for multi-chunk atoms
    if chunks.len() > 1 && auto_tagging_enabled && !all_tag_ids.is_empty() {
        all_tag_ids.sort();
        all_tag_ids.dedup();

        let tag_info = {
            let conn = db.conn.lock().map_err(|e| e.to_string())?;
            build_tag_info_for_consolidation(&conn, &all_tag_ids)?
        };

        match consolidate_atom_tags(&provider_config, tag_info, &tagging_model, supported_params)
            .await
        {
            Ok(consolidation) => {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;

                let lookup_result = tag_names_to_ids(&conn, &consolidation.tags_to_remove)?;
                if !lookup_result.missing_names.is_empty() {
                    eprintln!(
                        "Warning: Consolidation recommended removing non-existent tags: {:?}",
                        lookup_result.missing_names
                    );
                }
                let remove_ids = lookup_result.found_ids;

                for tag_id in &remove_ids {
                    conn.execute(
                        "DELETE FROM atom_tags WHERE atom_id = ?1 AND tag_id = ?2",
                        rusqlite::params![atom_id, tag_id],
                    )
                    .map_err(|e| e.to_string())?;

                    let usage_count: i64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM atom_tags WHERE tag_id = ?1",
                            [tag_id],
                            |row| row.get(0),
                        )
                        .map_err(|e| e.to_string())?;

                    if usage_count == 0 {
                        let has_wiki: bool = conn
                            .query_row(
                                "SELECT 1 FROM wiki_articles WHERE tag_id = ?1",
                                [tag_id],
                                |_| Ok(true),
                            )
                            .unwrap_or(false);

                        if has_wiki {
                            eprintln!(
                                "Skipping deletion of tag {} - has associated wiki article",
                                tag_id
                            );
                        } else {
                            conn.execute("DELETE FROM tags WHERE id = ?1", [tag_id])
                                .map_err(|e| e.to_string())?;
                            cleanup_orphaned_parents(&conn, tag_id)?;
                        }
                    }
                }

                let mut new_tag_ids = Vec::new();
                for tag_application in consolidation.tags_to_add {
                    let trimmed_name = tag_application.name.trim();
                    if trimmed_name.is_empty() || trimmed_name.eq_ignore_ascii_case("null") {
                        continue;
                    }

                    match get_or_create_tag(
                        &conn,
                        &tag_application.name,
                        &tag_application.parent_name,
                    ) {
                        Ok(tag_id) => new_tag_ids.push(tag_id),
                        Err(e) => eprintln!(
                            "Failed to get/create consolidation tag '{}': {}",
                            tag_application.name, e
                        ),
                    }
                }

                if !new_tag_ids.is_empty() {
                    link_tags_to_atom(&conn, atom_id, &new_tag_ids)?;
                }

                all_tag_ids.retain(|id| !remove_ids.contains(id));
                let added_count = new_tag_ids.len();
                all_tag_ids.extend(new_tag_ids.clone());
                all_new_tag_ids.extend(new_tag_ids);

                eprintln!(
                    "Tag consolidation complete for atom {}: removed {}, added {}",
                    atom_id,
                    remove_ids.len(),
                    added_count
                );
            }
            Err(e) => {
                eprintln!("Tag consolidation failed for atom {}: {}", atom_id, e);
            }
        }
    }

    // Set tagging status to complete
    {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE atoms SET tagging_status = 'complete' WHERE id = ?1",
            [atom_id],
        )
        .map_err(|e| e.to_string())?;
    }

    all_tag_ids.sort();
    all_tag_ids.dedup();
    all_new_tag_ids.sort();
    all_new_tag_ids.dedup();

    Ok((all_tag_ids, all_new_tag_ids))
}

/// Process tagging for multiple atoms concurrently with semaphore-based limiting
/// Used by process_pending_tagging for bulk operations
pub async fn process_tagging_batch<F>(db: Arc<Database>, atom_ids: Vec<String>, on_event: F)
where
    F: Fn(EmbeddingEvent) + Send + Sync + Clone + 'static,
{
    let mut tasks = Vec::with_capacity(atom_ids.len());

    for atom_id in atom_ids {
        let db = Arc::clone(&db);
        let on_event = on_event.clone();

        let task = tokio::spawn(async move {
            // Acquire semaphore permit
            let _permit = TAGGING_SEMAPHORE
                .acquire()
                .await
                .expect("Semaphore closed unexpectedly");

            let result = process_tagging_only(&db, &atom_id).await;

            let event = match result {
                Ok((tags_extracted, new_tags_created)) => EmbeddingEvent::TaggingComplete {
                    atom_id: atom_id.clone(),
                    tags_extracted,
                    new_tags_created,
                },
                Err(e) => {
                    if let Ok(conn) = db.conn.lock() {
                        let _ = conn.execute(
                            "UPDATE atoms SET tagging_status = 'failed' WHERE id = ?1",
                            [&atom_id],
                        );
                    }
                    EmbeddingEvent::TaggingFailed {
                        atom_id: atom_id.clone(),
                        error: e,
                    }
                }
            };

            on_event(event);
        });

        tasks.push(task);
    }

    // Wait for all tasks to complete
    for task in tasks {
        let _ = task.await;
    }
}

/// Process embeddings and tagging for a SINGLE atom (used by create_atom/update_atom)
/// This spawns a background task that runs embedding first, then tagging.
///
/// Phase 1 (Embedding):
/// 1. Sets embedding_status to 'processing'
/// 2. Chunks the content
/// 3. Generates embeddings via provider
/// 4. Stores chunks and embeddings in database
/// 5. Computes semantic edges
/// 6. Sets embedding_status to 'complete' or 'failed'
/// 7. Invokes callback with EmbeddingComplete or EmbeddingFailed event
///
/// Phase 2 (Tagging - only if embedding succeeded):
/// 1. Sets tagging_status to 'processing'
/// 2. Extracts tags using LLM (if enabled)
/// 3. Links extracted tags to the atom
/// 4. Runs consolidation for multi-chunk atoms
/// 5. Sets tagging_status to 'complete', 'skipped', or 'failed'
/// 6. Invokes callback with TaggingComplete, TaggingSkipped, or TaggingFailed event
pub fn spawn_embedding_task_single<F>(
    db: Arc<Database>,
    atom_id: String,
    content: String,
    on_event: F,
) where
    F: Fn(EmbeddingEvent) + Send + Sync + 'static,
{
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

        // Emit started event
        on_event(EmbeddingEvent::Started {
            atom_id: atom_id.clone(),
        });

        // Phase 1: Embedding only
        let embedding_result = rt.block_on(process_embedding_only(&db, &atom_id, &content));

        match &embedding_result {
            Ok(()) => {
                on_event(EmbeddingEvent::EmbeddingComplete {
                    atom_id: atom_id.clone(),
                });
            }
            Err(e) => {
                if let Ok(conn) = db.conn.lock() {
                    let _ = conn.execute(
                        "UPDATE atoms SET embedding_status = 'failed', tagging_status = 'skipped' WHERE id = ?1",
                        [&atom_id],
                    );
                }
                on_event(EmbeddingEvent::EmbeddingFailed {
                    atom_id: atom_id.clone(),
                    error: e.clone(),
                });
            }
        };

        // Phase 2: Tagging only (if embedding succeeded)
        if embedding_result.is_ok() {
            let tagging_result = rt.block_on(process_tagging_only(&db, &atom_id));

            match tagging_result {
                Ok((tags_extracted, new_tags_created)) => {
                    on_event(EmbeddingEvent::TaggingComplete {
                        atom_id: atom_id.clone(),
                        tags_extracted,
                        new_tags_created,
                    });
                }
                Err(e) => {
                    if let Ok(conn) = db.conn.lock() {
                        let _ = conn.execute(
                            "UPDATE atoms SET tagging_status = 'failed' WHERE id = ?1",
                            [&atom_id],
                        );
                    }
                    on_event(EmbeddingEvent::TaggingFailed {
                        atom_id: atom_id.clone(),
                        error: e,
                    });
                }
            };
        }
    });
}

/// Process embeddings for multiple atoms with semaphore-based limiting
/// After ALL embeddings complete, runs tagging for successfully embedded atoms (unless skip_tagging is true)
/// This ensures fast embedding phase completes first, then slower tagging runs
/// Set skip_tagging=true when re-embedding due to model/provider change (tags are preserved)
pub async fn process_embedding_batch<F>(
    db: Arc<Database>,
    atoms: Vec<(String, String)>,
    skip_tagging: bool,
    on_event: F,
) where
    F: Fn(EmbeddingEvent) + Send + Sync + Clone + 'static,
{
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc as StdArc;

    // Track successfully embedded atom IDs for tagging phase
    let successful_ids: StdArc<std::sync::Mutex<Vec<String>>> =
        StdArc::new(std::sync::Mutex::new(Vec::new()));
    let total_count = atoms.len();
    let completed_count = StdArc::new(AtomicUsize::new(0));

    // Phase 1: All embeddings
    eprintln!("Starting embedding phase for {} atoms...", total_count);
    let mut tasks = Vec::with_capacity(atoms.len());

    for (atom_id, content) in atoms {
        let db = Arc::clone(&db);
        let successful_ids = StdArc::clone(&successful_ids);
        let completed_count = StdArc::clone(&completed_count);
        let on_event = on_event.clone();

        let task = tokio::spawn(async move {
            // Acquire semaphore permit
            let _permit = EMBEDDING_SEMAPHORE
                .acquire()
                .await
                .expect("Semaphore closed unexpectedly");

            // Process embedding only (no tagging yet)
            let result = process_embedding_only(&db, &atom_id, &content).await;

            let event = match &result {
                Ok(()) => {
                    // Track successful embedding for tagging phase
                    if let Ok(mut ids) = successful_ids.lock() {
                        ids.push(atom_id.clone());
                    }
                    EmbeddingEvent::EmbeddingComplete {
                        atom_id: atom_id.clone(),
                    }
                }
                Err(e) => {
                    if let Ok(conn) = db.conn.lock() {
                        let _ = conn.execute(
                            "UPDATE atoms SET embedding_status = 'failed', tagging_status = 'skipped' WHERE id = ?1",
                            [&atom_id],
                        );
                    }
                    EmbeddingEvent::EmbeddingFailed {
                        atom_id: atom_id.clone(),
                        error: e.clone(),
                    }
                }
            };

            let count = completed_count.fetch_add(1, Ordering::SeqCst) + 1;
            eprintln!("Embedding {}/{} complete: {}", count, total_count, atom_id);
            on_event(event);
        });

        tasks.push(task);
    }

    // Wait for all embedding tasks to complete
    for task in tasks {
        let _ = task.await;
    }

    // Phase 2: All tagging (after all embeddings complete) - unless skip_tagging is true
    if skip_tagging {
        eprintln!("Embedding phase complete. Skipping tagging phase (re-embedding only).");
    } else {
        let atoms_to_tag: Vec<String> = successful_ids
            .lock()
            .map(|ids| ids.clone())
            .unwrap_or_default();

        if !atoms_to_tag.is_empty() {
            eprintln!(
                "Embedding phase complete. Starting tagging phase for {} atoms...",
                atoms_to_tag.len()
            );
            process_tagging_batch(db, atoms_to_tag, on_event).await;
        } else {
            eprintln!("Embedding phase complete. No atoms to tag.");
        }
    }
}

/// Convert L2 distance to cosine similarity for normalized vectors
/// Formula: cosine_similarity = 1 - (L2_distance² / 2)
/// This derives from: L2² = 2(1 - cos(θ)) for unit vectors
pub fn distance_to_similarity(distance: f32) -> f32 {
    (1.0 - (distance * distance / 2.0)).clamp(-1.0, 1.0)
}

/// Compute semantic edges for an atom after embedding generation
/// Finds similar atoms based on vector similarity and stores edges in semantic_edges table
pub fn compute_semantic_edges_for_atom(
    conn: &rusqlite::Connection,
    atom_id: &str,
    threshold: f32, // Default: 0.5 - lower than UI threshold to capture more relationships
    max_edges: i32, // Default: 15 per atom
) -> Result<i32, String> {
    use std::collections::HashMap;

    // First, delete existing edges for this atom (bidirectional)
    conn.execute(
        "DELETE FROM semantic_edges WHERE source_atom_id = ?1 OR target_atom_id = ?1",
        [atom_id],
    )
    .map_err(|e| format!("Failed to delete existing edges: {}", e))?;

    // Get all chunks for the given atom
    let mut stmt = conn
        .prepare("SELECT id, chunk_index, embedding FROM atom_chunks WHERE atom_id = ?1")
        .map_err(|e| format!("Failed to prepare chunk query: {}", e))?;

    let source_chunks: Vec<(String, i32, Vec<u8>)> = stmt
        .query_map([atom_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(|e| format!("Failed to query chunks: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect chunks: {}", e))?;

    if source_chunks.is_empty() {
        return Ok(0);
    }

    // Map to store best similarity per target atom_id
    // Value: (similarity, source_chunk_index, target_chunk_index)
    let mut atom_similarities: HashMap<String, (f32, i32, i32)> = HashMap::new();

    // For each source chunk, find similar chunks
    for (_, source_chunk_index, embedding_blob) in &source_chunks {
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
            .query_map(rusqlite::params![embedding_blob, max_edges * 5], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(|e| format!("Failed to query similar chunks: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect similar chunks: {}", e))?;

        // For each similar chunk, check threshold and track best match per atom
        for (chunk_id, distance) in similar_chunks {
            let similarity = distance_to_similarity(distance);

            if similarity < threshold {
                continue;
            }

            // Get the parent atom_id and chunk index for this chunk
            let chunk_info: Result<(String, i32), _> = conn.query_row(
                "SELECT atom_id, chunk_index FROM atom_chunks WHERE id = ?1",
                [&chunk_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            );

            if let Ok((target_atom_id, target_chunk_index)) = chunk_info {
                // Exclude the source atom itself
                if target_atom_id == atom_id {
                    continue;
                }

                // Keep highest similarity per target atom
                let entry = atom_similarities.entry(target_atom_id.clone());
                match entry {
                    std::collections::hash_map::Entry::Occupied(mut e) => {
                        if similarity > e.get().0 {
                            e.insert((similarity, *source_chunk_index, target_chunk_index));
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(e) => {
                        e.insert((similarity, *source_chunk_index, target_chunk_index));
                    }
                }
            }
        }
    }

    // Sort by similarity and take top N
    let mut edges: Vec<(String, f32, i32, i32)> = atom_similarities
        .into_iter()
        .map(|(target_id, (sim, src_idx, tgt_idx))| (target_id, sim, src_idx, tgt_idx))
        .collect();

    edges.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    edges.truncate(max_edges as usize);

    // Insert edges (store bidirectionally with consistent ordering)
    let now = chrono::Utc::now().to_rfc3339();
    let mut edges_created = 0;

    for (target_atom_id, similarity, source_chunk_index, target_chunk_index) in edges {
        // Use consistent ordering: smaller ID is source
        let (src_id, tgt_id, src_chunk, tgt_chunk) = if atom_id < target_atom_id.as_str() {
            (
                atom_id.to_string(),
                target_atom_id.clone(),
                source_chunk_index,
                target_chunk_index,
            )
        } else {
            (
                target_atom_id.clone(),
                atom_id.to_string(),
                target_chunk_index,
                source_chunk_index,
            )
        };

        let edge_id = Uuid::new_v4().to_string();

        // Insert or update (using INSERT OR REPLACE due to UNIQUE constraint)
        let result = conn.execute(
            "INSERT OR REPLACE INTO semantic_edges
             (id, source_atom_id, target_atom_id, similarity_score, source_chunk_index, target_chunk_index, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![&edge_id, &src_id, &tgt_id, similarity, src_chunk, tgt_chunk, &now,],
        );

        if result.is_ok() {
            edges_created += 1;
        }
    }

    Ok(edges_created)
}

/// Process all atoms with 'pending' embedding status
///
/// Fetches all pending atoms, marks them as 'processing', and processes them in batch.
/// Returns the number of atoms queued for processing.
pub fn process_pending_embeddings<F>(
    db: Arc<Database>,
    on_event: F,
) -> Result<i32, String>
where
    F: Fn(EmbeddingEvent) + Send + Sync + Clone + 'static,
{
    // Atomically fetch and mark pending atoms as 'processing' in a single statement
    // This prevents race conditions from duplicate calls
    let pending_atoms: Vec<(String, String)> = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "UPDATE atoms SET embedding_status = 'processing'
                 WHERE embedding_status = 'pending'
                 RETURNING id, content",
            )
            .map_err(|e| format!("Failed to prepare query: {}", e))?;
        let results: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| format!("Failed to query pending atoms: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect pending atoms: {}", e))?;
        results
    };

    let count = pending_atoms.len() as i32;

    if count > 0 {
        // Process batch asynchronously in a separate thread
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(process_embedding_batch(
                db,
                pending_atoms,
                false, // don't skip tagging - normal flow
                on_event,
            ));
        });
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distance_to_similarity() {
        // Formula: 1.0 - (distance² / 2.0), clamped to [-1.0, 1.0]
        assert!((distance_to_similarity(0.0) - 1.0).abs() < 0.001); // 1 - 0 = 1.0
        assert!((distance_to_similarity(1.0) - 0.5).abs() < 0.001); // 1 - 0.5 = 0.5
        // distance = √2 gives 1.0 - 1.0 = 0.0
        assert!((distance_to_similarity(std::f32::consts::SQRT_2) - 0.0).abs() < 0.001);
        // distance = 2.0 gives 1.0 - 2.0 = -1.0 (clamped)
        assert!((distance_to_similarity(2.0) - (-1.0)).abs() < 0.001);
    }
}
