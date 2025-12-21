//! Import commands for importing notes from external applications

use crate::db::Database;
use crate::embedding::process_embedding_batch;
use crate::obsidian::{discover_notes, parse_obsidian_note, DEFAULT_EXCLUDES};
use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

/// Result of an import operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub imported: i32,
    pub skipped: i32,
    pub errors: i32,
    pub tags_created: i32,
    pub tags_linked: i32,
}

/// Progress event payload for import operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportProgressPayload {
    pub current: i32,
    pub total: i32,
    pub current_file: String,
    pub status: String, // "importing", "skipped", "error"
}

/// Import notes from an Obsidian vault (native Rust implementation)
///
/// This command discovers markdown files in the vault, parses YAML frontmatter,
/// extracts tags from both frontmatter and folder structure, and imports them
/// as atoms with pending embedding status.
#[tauri::command]
pub async fn import_obsidian_vault(
    app: AppHandle,
    db: State<'_, Arc<Database>>,
    vault_path: String,
    max_notes: Option<i32>,
) -> Result<ImportResult, String> {
    let vault_path = Path::new(&vault_path);

    // Validate vault path
    if !vault_path.exists() {
        return Err(format!("Vault not found at {:?}", vault_path));
    }

    // Get vault name from path
    let vault_name = vault_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Vault".to_string());

    // Discover notes
    let exclude_patterns: Vec<&str> = DEFAULT_EXCLUDES.to_vec();
    let mut note_files = discover_notes(vault_path, &exclude_patterns)?;

    if note_files.is_empty() {
        return Ok(ImportResult {
            imported: 0,
            skipped: 0,
            errors: 0,
            tags_created: 0,
            tags_linked: 0,
        });
    }

    // Limit number of notes if specified
    if let Some(max) = max_notes {
        note_files.truncate(max as usize);
    }

    let total = note_files.len() as i32;
    let mut stats = ImportResult {
        imported: 0,
        skipped: 0,
        errors: 0,
        tags_created: 0,
        tags_linked: 0,
    };

    // Tag cache for deduplication
    // Key: (lowercase name, parent_id as Option<String>) -> tag id
    // This allows proper hierarchical lookup where "Work" under "Projects" is different from root "Work"
    let mut tag_cache: HashMap<(String, Option<String>), String> = HashMap::new();

    // Track imported atoms for embedding processing
    let mut imported_atoms: Vec<(String, String)> = Vec::new();

    // Process each note
    for (index, file_path) in note_files.iter().enumerate() {
        let relative_path = file_path.strip_prefix(vault_path).unwrap_or(file_path);
        let relative_str = relative_path.to_string_lossy().to_string();

        // Parse the note
        let note = match parse_obsidian_note(file_path, relative_path, &vault_name) {
            Ok(n) => n,
            Err(e) => {
                eprintln!("Error parsing {}: {}", relative_str, e);
                stats.errors += 1;
                let _ = app.emit(
                    "import-progress",
                    ImportProgressPayload {
                        current: index as i32 + 1,
                        total,
                        current_file: relative_str,
                        status: "error".to_string(),
                    },
                );
                continue;
            }
        };

        // Skip empty notes (< 10 chars after title)
        if note.content.trim().len() < 10 {
            stats.skipped += 1;
            let _ = app.emit(
                "import-progress",
                ImportProgressPayload {
                    current: index as i32 + 1,
                    total,
                    current_file: relative_str,
                    status: "skipped".to_string(),
                },
            );
            continue;
        }

        // Database operations
        let conn = db.conn.lock().map_err(|e| e.to_string())?;

        // Check for duplicates by source_url
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM atoms WHERE source_url = ?1 LIMIT 1",
                [&note.source_url],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if exists {
            stats.skipped += 1;
            let _ = app.emit(
                "import-progress",
                ImportProgressPayload {
                    current: index as i32 + 1,
                    total,
                    current_file: relative_str,
                    status: "skipped".to_string(),
                },
            );
            drop(conn);
            continue;
        }

        // Insert atom
        let atom_id = Uuid::new_v4().to_string();
        match conn.execute(
            "INSERT INTO atoms (id, content, source_url, created_at, updated_at, embedding_status, tagging_status)
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending', 'pending')",
            params![
                &atom_id,
                &note.content,
                &note.source_url,
                &note.created_at,
                &note.updated_at,
            ],
        ) {
            Ok(_) => {
                // Track for embedding processing
                imported_atoms.push((atom_id.clone(), note.content.clone()));
            }
            Err(e) => {
                eprintln!("Error inserting atom for {}: {}", relative_str, e);
                stats.errors += 1;
                let _ = app.emit(
                    "import-progress",
                    ImportProgressPayload {
                        current: index as i32 + 1,
                        total,
                        current_file: relative_str,
                        status: "error".to_string(),
                    },
                );
                drop(conn);
                continue;
            }
        }

        // Helper closure to get or create a tag with optional parent
        let get_or_create_tag = |conn: &rusqlite::Connection,
                                  tag_cache: &mut HashMap<(String, Option<String>), String>,
                                  name: &str,
                                  parent_id: Option<&str>,
                                  stats: &mut ImportResult|
         -> Option<String> {
            let cache_key = (name.to_lowercase(), parent_id.map(|s| s.to_string()));

            if let Some(cached_id) = tag_cache.get(&cache_key) {
                return Some(cached_id.clone());
            }

            // Check if tag exists (case-insensitive, with matching parent)
            let existing: Option<String> = if let Some(pid) = parent_id {
                conn.query_row(
                    "SELECT id FROM tags WHERE LOWER(name) = LOWER(?1) AND parent_id = ?2 LIMIT 1",
                    params![name, pid],
                    |row| row.get(0),
                )
                .ok()
            } else {
                conn.query_row(
                    "SELECT id FROM tags WHERE LOWER(name) = LOWER(?1) AND parent_id IS NULL LIMIT 1",
                    [name],
                    |row| row.get(0),
                )
                .ok()
            };

            let id = match existing {
                Some(id) => id,
                None => {
                    // Create new tag
                    let new_id = Uuid::new_v4().to_string();
                    let now = Utc::now().to_rfc3339();
                    if let Err(e) = conn.execute(
                        "INSERT INTO tags (id, name, parent_id, created_at) VALUES (?1, ?2, ?3, ?4)",
                        params![&new_id, name, parent_id, &now],
                    ) {
                        eprintln!("Error creating tag '{}': {}", name, e);
                        return None;
                    }
                    stats.tags_created += 1;
                    new_id
                }
            };

            tag_cache.insert(cache_key, id.clone());
            Some(id)
        };

        // Process hierarchical folder tags first (to establish parent relationships)
        // We need to process them in order to build up the hierarchy
        let mut folder_tag_ids: Vec<String> = Vec::new();
        for htag in &note.folder_tags {
            // Find the parent_id by looking up the immediate parent in the path
            let parent_id = if htag.parent_path.is_empty() {
                None
            } else {
                // The parent is the last element in parent_path
                // We need to find its ID - it should already be in folder_tag_ids
                // since we process tags in order
                let parent_index = htag.parent_path.len() - 1;
                folder_tag_ids.get(parent_index).map(|s| s.as_str())
            };

            if let Some(tag_id) =
                get_or_create_tag(&conn, &mut tag_cache, &htag.name, parent_id, &mut stats)
            {
                folder_tag_ids.push(tag_id.clone());

                // Link tag to atom
                if let Err(e) = conn.execute(
                    "INSERT OR IGNORE INTO atom_tags (atom_id, tag_id) VALUES (?1, ?2)",
                    params![&atom_id, &tag_id],
                ) {
                    eprintln!("Error linking folder tag '{}' to atom: {}", htag.name, e);
                    continue;
                }
                stats.tags_linked += 1;
            }
        }

        // Process flat frontmatter tags (no parent)
        for tag_name in &note.frontmatter_tags {
            if let Some(tag_id) =
                get_or_create_tag(&conn, &mut tag_cache, tag_name, None, &mut stats)
            {
                // Link tag to atom
                if let Err(e) = conn.execute(
                    "INSERT OR IGNORE INTO atom_tags (atom_id, tag_id) VALUES (?1, ?2)",
                    params![&atom_id, &tag_id],
                ) {
                    eprintln!("Error linking tag '{}' to atom: {}", tag_name, e);
                    continue;
                }
                stats.tags_linked += 1;
            }
        }

        stats.imported += 1;
        let _ = app.emit(
            "import-progress",
            ImportProgressPayload {
                current: index as i32 + 1,
                total,
                current_file: relative_str,
                status: "importing".to_string(),
            },
        );

        drop(conn);
    }

    // Trigger embedding processing for all imported atoms
    if !imported_atoms.is_empty() {
        // Mark atoms as 'processing' before spawning the batch
        {
            let conn = db.conn.lock().map_err(|e| e.to_string())?;
            for (atom_id, _) in &imported_atoms {
                let _ = conn.execute(
                    "UPDATE atoms SET embedding_status = 'processing' WHERE id = ?1",
                    [atom_id],
                );
            }
        }

        // Spawn embedding batch processing (non-blocking)
        let app_clone = app.clone();
        let db_clone = Arc::clone(&db);
        tokio::spawn(async move {
            process_embedding_batch(app_clone, db_clone, imported_atoms, false).await;
        });
    }

    Ok(stats)
}
