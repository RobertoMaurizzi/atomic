//! atomic-core: Knowledge base library for Atomic
//!
//! This library provides the core RAG pipeline for the Atomic knowledge base:
//! - Atom CRUD operations
//! - Embedding generation with callback-based events
//! - Unified search (semantic, keyword, hybrid)
//! - Wiki article synthesis
//! - Tag extraction and compaction
//!
//! # Example
//!
//! ```rust,ignore
//! use atomic_core::{AtomicCore, CreateAtomRequest, EmbeddingEvent};
//!
//! let core = AtomicCore::open_or_create("/path/to/db")?;
//!
//! // Create an atom with embedding callback
//! let atom = core.create_atom(
//!     CreateAtomRequest {
//!         content: "My note content".to_string(),
//!         source_url: None,
//!         tag_ids: vec![],
//!     },
//!     |event| match event {
//!         EmbeddingEvent::EmbeddingComplete { atom_id } => println!("Done: {}", atom_id),
//!         _ => {}
//!     },
//! )?;
//! ```

pub mod chunking;
pub mod clustering;
pub mod compaction;
pub mod db;
pub mod embedding;
pub mod error;
pub mod extraction;
pub mod import;
pub mod models;
pub mod providers;
pub mod search;
pub mod settings;
pub mod wiki;

// Re-exports for convenience
pub use db::Database;
pub use embedding::EmbeddingEvent;
pub use error::AtomicCoreError;
pub use models::*;
pub use providers::{ProviderConfig, ProviderType};
pub use search::{SearchMode, SearchOptions};

use chrono::Utc;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

/// Request to create a new atom
#[derive(Debug, Clone)]
pub struct CreateAtomRequest {
    pub content: String,
    pub source_url: Option<String>,
    pub tag_ids: Vec<String>,
}

/// Request to update an existing atom
#[derive(Debug, Clone)]
pub struct UpdateAtomRequest {
    pub content: String,
    pub source_url: Option<String>,
    pub tag_ids: Vec<String>,
}

/// Main library facade providing high-level operations
pub struct AtomicCore {
    db: Arc<Database>,
}

impl AtomicCore {
    /// Open an existing database
    pub fn open(db_path: impl AsRef<Path>) -> Result<Self, AtomicCoreError> {
        let db = Database::open(db_path)?;
        Ok(Self { db: Arc::new(db) })
    }

    /// Open an existing database or create a new one
    pub fn open_or_create(db_path: impl AsRef<Path>) -> Result<Self, AtomicCoreError> {
        let db = Database::open_or_create(db_path)?;
        Ok(Self { db: Arc::new(db) })
    }

    /// Get the database path (for external code to open its own connection)
    pub fn db_path(&self) -> &Path {
        &self.db.db_path
    }

    /// Get a reference to the database
    pub fn database(&self) -> Arc<Database> {
        Arc::clone(&self.db)
    }

    // ==================== Settings ====================

    /// Get all settings
    pub fn get_settings(
        &self,
    ) -> Result<std::collections::HashMap<String, String>, AtomicCoreError> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        settings::get_all_settings(&conn)
    }

    /// Set a setting value
    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), AtomicCoreError> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        settings::set_setting(&conn, key, value)
    }

    // ==================== Atom Operations ====================

    /// Get all atoms with their tags
    pub fn get_all_atoms(&self) -> Result<Vec<AtomWithTags>, AtomicCoreError> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, content, source_url, created_at, updated_at,
                 COALESCE(embedding_status, 'pending'), COALESCE(tagging_status, 'pending')
                 FROM atoms ORDER BY updated_at DESC",
            )?;

        let atoms: Vec<Atom> = stmt
            .query_map([], |row| {
                Ok(Atom {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    source_url: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    embedding_status: row.get(5)?,
                    tagging_status: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut result = Vec::new();
        for atom in atoms {
            let tags = get_tags_for_atom(&conn, &atom.id)?;
            result.push(AtomWithTags { atom, tags });
        }

        Ok(result)
    }

    /// Get a single atom by ID
    pub fn get_atom(&self, id: &str) -> Result<Option<AtomWithTags>, AtomicCoreError> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        let atom_result = conn.query_row(
            "SELECT id, content, source_url, created_at, updated_at,
             COALESCE(embedding_status, 'pending'), COALESCE(tagging_status, 'pending')
             FROM atoms WHERE id = ?1",
            [id],
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
        );

        match atom_result {
            Ok(atom) => {
                let tags = get_tags_for_atom(&conn, id)?;
                Ok(Some(AtomWithTags { atom, tags }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AtomicCoreError::Database(e)),
        }
    }

    /// Create a new atom and trigger embedding generation
    ///
    /// The `on_event` callback will be invoked with progress events during
    /// embedding generation and tag extraction (which happens asynchronously).
    pub fn create_atom<F>(
        &self,
        request: CreateAtomRequest,
        on_event: F,
    ) -> Result<AtomWithTags, AtomicCoreError>
    where
        F: Fn(EmbeddingEvent) + Send + Sync + 'static,
    {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let embedding_status = "pending";

        {
            let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

            conn.execute(
                "INSERT INTO atoms (id, content, source_url, created_at, updated_at, embedding_status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                (&id, &request.content, &request.source_url, &now, &now, &embedding_status),
            )
            ?;

            // Add tags
            for tag_id in &request.tag_ids {
                conn.execute(
                    "INSERT INTO atom_tags (atom_id, tag_id) VALUES (?1, ?2)",
                    (&id, tag_id),
                )
                ?;
            }
        }

        // Get the created atom with tags
        let atom = Atom {
            id: id.clone(),
            content: request.content.clone(),
            source_url: request.source_url,
            created_at: now.clone(),
            updated_at: now,
            embedding_status: embedding_status.to_string(),
            tagging_status: "pending".to_string(),
        };

        let tags = {
            let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
            get_tags_for_atom(&conn, &id)?
        };

        // Spawn embedding task (non-blocking)
        embedding::spawn_embedding_task_single(
            Arc::clone(&self.db),
            id,
            request.content,
            on_event,
        );

        Ok(AtomWithTags { atom, tags })
    }

    /// Update an existing atom and trigger re-embedding
    pub fn update_atom<F>(
        &self,
        id: &str,
        request: UpdateAtomRequest,
        on_event: F,
    ) -> Result<AtomWithTags, AtomicCoreError>
    where
        F: Fn(EmbeddingEvent) + Send + Sync + 'static,
    {
        let now = Utc::now().to_rfc3339();
        let embedding_status = "pending";

        {
            let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

            conn.execute(
                "UPDATE atoms SET content = ?1, source_url = ?2, updated_at = ?3, embedding_status = ?4
                 WHERE id = ?5",
                (&request.content, &request.source_url, &now, &embedding_status, id),
            )
            ?;

            // Remove existing tags and add new ones
            conn.execute("DELETE FROM atom_tags WHERE atom_id = ?1", [id])
                ?;

            for tag_id in &request.tag_ids {
                conn.execute(
                    "INSERT INTO atom_tags (atom_id, tag_id) VALUES (?1, ?2)",
                    (id, tag_id),
                )
                ?;
            }
        }

        // Get the updated atom
        let atom = {
            let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
            conn.query_row(
                "SELECT id, content, source_url, created_at, updated_at,
                 COALESCE(embedding_status, 'pending'), COALESCE(tagging_status, 'pending')
                 FROM atoms WHERE id = ?1",
                [id],
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
            ?
        };

        let tags = {
            let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
            get_tags_for_atom(&conn, id)?
        };

        // Spawn embedding task (non-blocking)
        embedding::spawn_embedding_task_single(
            Arc::clone(&self.db),
            id.to_string(),
            request.content,
            on_event,
        );

        Ok(AtomWithTags { atom, tags })
    }

    /// Delete an atom
    pub fn delete_atom(&self, id: &str) -> Result<(), AtomicCoreError> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        conn.execute("DELETE FROM atoms WHERE id = ?1", [id])
            ?;

        Ok(())
    }

    /// Get atoms by tag (includes atoms with descendant tags)
    pub fn get_atoms_by_tag(&self, tag_id: &str) -> Result<Vec<AtomWithTags>, AtomicCoreError> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        // Get all descendant tag IDs (including the tag itself)
        let all_tag_ids = get_descendant_tag_ids(&conn, tag_id)?;

        // Query atoms with any of these tags (deduplicated)
        let placeholders = all_tag_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT DISTINCT a.id, a.content, a.source_url, a.created_at, a.updated_at,
             COALESCE(a.embedding_status, 'pending'), COALESCE(a.tagging_status, 'pending')
             FROM atoms a
             INNER JOIN atom_tags at ON a.id = at.atom_id
             WHERE at.tag_id IN ({})
             ORDER BY a.updated_at DESC",
            placeholders
        );

        let mut stmt = conn.prepare(&query)?;

        let atoms: Vec<Atom> = stmt
            .query_map(rusqlite::params_from_iter(all_tag_ids.iter()), |row| {
                Ok(Atom {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    source_url: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    embedding_status: row.get(5)?,
                    tagging_status: row.get(6)?,
                })
            })
            ?
            .collect::<Result<Vec<_>, _>>()
            ?;

        let mut result = Vec::new();
        for atom in atoms {
            let tags = get_tags_for_atom(&conn, &atom.id)?;
            result.push(AtomWithTags { atom, tags });
        }

        Ok(result)
    }

    // ==================== Tag Operations ====================

    /// Get all tags with counts (hierarchical tree)
    pub fn get_all_tags(&self) -> Result<Vec<TagWithCount>, AtomicCoreError> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        let mut stmt = conn
            .prepare("SELECT id, name, parent_id, created_at FROM tags ORDER BY name")
            ?;

        let all_tags: Vec<Tag> = stmt
            .query_map([], |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    parent_id: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })
            ?
            .collect::<Result<Vec<_>, _>>()
            ?;

        Ok(build_tag_tree(&all_tags, None, &conn))
    }

    /// Create a new tag
    pub fn create_tag(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> Result<Tag, AtomicCoreError> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO tags (id, name, parent_id, created_at) VALUES (?1, ?2, ?3, ?4)",
            (&id, name, &parent_id, &now),
        )
        ?;

        Ok(Tag {
            id,
            name: name.to_string(),
            parent_id: parent_id.map(String::from),
            created_at: now,
        })
    }

    /// Update a tag
    pub fn update_tag(
        &self,
        id: &str,
        name: &str,
        parent_id: Option<&str>,
    ) -> Result<Tag, AtomicCoreError> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        conn.execute(
            "UPDATE tags SET name = ?1, parent_id = ?2 WHERE id = ?3",
            (name, &parent_id, id),
        )
        ?;

        let tag = conn
            .query_row(
                "SELECT id, name, parent_id, created_at FROM tags WHERE id = ?1",
                [id],
                |row| {
                    Ok(Tag {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        parent_id: row.get(2)?,
                        created_at: row.get(3)?,
                    })
                },
            )
            ?;

        Ok(tag)
    }

    /// Delete a tag
    pub fn delete_tag(&self, id: &str) -> Result<(), AtomicCoreError> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        conn.execute("DELETE FROM tags WHERE id = ?1", [id])
            ?;

        Ok(())
    }

    // ==================== Search Operations ====================

    /// Search atoms using the configured search mode
    pub async fn search(
        &self,
        options: SearchOptions,
    ) -> Result<Vec<SemanticSearchResult>, AtomicCoreError> {
        search::search_atoms(&self.db, options)
            .await
            .map_err(|e| AtomicCoreError::Search(e))
    }

    /// Find atoms similar to a given atom
    pub fn find_similar(
        &self,
        atom_id: &str,
        limit: i32,
        threshold: f32,
    ) -> Result<Vec<SimilarAtomResult>, AtomicCoreError> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        search::find_similar_atoms(&conn, atom_id, limit, threshold)
            .map_err(|e| AtomicCoreError::Search(e))
    }

    // ==================== Wiki Operations ====================

    /// Generate a wiki article for a tag
    pub async fn generate_wiki(
        &self,
        tag_id: &str,
        tag_name: &str,
    ) -> Result<WikiArticleWithCitations, AtomicCoreError> {
        // Get settings for provider config
        let (provider_config, wiki_model) = {
            let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
            let settings_map = settings::get_all_settings(&conn)?;
            let config = ProviderConfig::from_settings(&settings_map);
            let model = settings_map
                .get("wiki_model")
                .cloned()
                .unwrap_or_else(|| "anthropic/claude-sonnet-4.5".to_string());
            (config, model)
        };

        // Prepare sources using async function
        let input = wiki::prepare_wiki_generation(&self.db, &provider_config, tag_id, tag_name)
            .await
            .map_err(|e| AtomicCoreError::Wiki(e))?;

        // Generate content
        let result = wiki::generate_wiki_content(&provider_config, &input, &wiki_model)
            .await
            .map_err(|e| AtomicCoreError::Wiki(e))?;

        // Save to database
        {
            let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
            wiki::save_wiki_article(&conn, &result.article, &result.citations)
                .map_err(|e| AtomicCoreError::Wiki(e))?;
        }

        Ok(result)
    }

    /// Get an existing wiki article
    pub fn get_wiki(&self, tag_id: &str) -> Result<Option<WikiArticleWithCitations>, AtomicCoreError> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        wiki::load_wiki_article(&conn, tag_id).map_err(|e| AtomicCoreError::Wiki(e))
    }

    /// Get wiki article status (for checking if update is needed)
    pub fn get_wiki_status(&self, tag_id: &str) -> Result<WikiArticleStatus, AtomicCoreError> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        wiki::get_article_status(&conn, tag_id).map_err(|e| AtomicCoreError::Wiki(e))
    }

    /// Delete a wiki article
    pub fn delete_wiki(&self, tag_id: &str) -> Result<(), AtomicCoreError> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        wiki::delete_article(&conn, tag_id).map_err(|e| AtomicCoreError::Wiki(e))
    }

    // ==================== Embedding Management ====================

    /// Process all pending embeddings
    pub fn process_pending_embeddings<F>(&self, on_event: F) -> Result<i32, AtomicCoreError>
    where
        F: Fn(EmbeddingEvent) + Send + Sync + Clone + 'static,
    {
        embedding::process_pending_embeddings(Arc::clone(&self.db), on_event)
            .map_err(|e| AtomicCoreError::Embedding(e))
    }

    /// Reset atoms stuck in 'processing' state back to 'pending'
    pub fn reset_stuck_processing(&self) -> Result<i32, AtomicCoreError> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        let count = conn
            .execute(
                "UPDATE atoms SET embedding_status = 'pending' WHERE embedding_status = 'processing'",
                [],
            )
            ?;

        Ok(count as i32)
    }

    /// Retry embedding for a specific atom
    pub fn retry_embedding<F>(&self, atom_id: &str, on_event: F) -> Result<(), AtomicCoreError>
    where
        F: Fn(EmbeddingEvent) + Send + Sync + 'static,
    {
        let content = {
            let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
            conn.query_row("SELECT content FROM atoms WHERE id = ?1", [atom_id], |row| {
                row.get::<_, String>(0)
            })
            ?
        };

        embedding::spawn_embedding_task_single(
            Arc::clone(&self.db),
            atom_id.to_string(),
            content,
            on_event,
        );

        Ok(())
    }

    // ==================== Clustering ====================

    /// Compute atom clusters based on semantic similarity
    pub fn compute_clusters(
        &self,
        min_similarity: f32,
        min_cluster_size: i32,
    ) -> Result<Vec<AtomCluster>, AtomicCoreError> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        clustering::compute_atom_clusters(&conn, min_similarity, min_cluster_size)
            .map_err(|e| AtomicCoreError::Clustering(e))
    }

    /// Save cluster assignments to the database
    pub fn save_clusters(&self, clusters: &[AtomCluster]) -> Result<(), AtomicCoreError> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        clustering::save_cluster_assignments(&conn, clusters)
            .map_err(|e| AtomicCoreError::Clustering(e))
    }

    /// Get connection counts for hub identification
    pub fn get_connection_counts(
        &self,
        min_similarity: f32,
    ) -> Result<std::collections::HashMap<String, i32>, AtomicCoreError> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        clustering::get_connection_counts(&conn, min_similarity)
            .map_err(|e| AtomicCoreError::Clustering(e))
    }

    // ==================== Compaction ====================

    /// Get all tags formatted for LLM analysis
    pub fn get_tags_for_compaction(&self) -> Result<String, AtomicCoreError> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        compaction::read_all_tags(&conn).map_err(|e| AtomicCoreError::Compaction(e))
    }

    /// Apply tag merge operations
    pub fn apply_tag_merges(
        &self,
        merges: &[compaction::TagMerge],
    ) -> Result<compaction::CompactionResult, AtomicCoreError> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        let (tags_merged, atoms_retagged, errors) = compaction::apply_merge_operations(&conn, merges);

        if !errors.is_empty() {
            eprintln!("Merge errors: {:?}", errors);
        }

        Ok(compaction::CompactionResult {
            tags_merged,
            atoms_retagged,
        })
    }
}

// ==================== Helper Functions ====================

/// Get tags for a specific atom
fn get_tags_for_atom(conn: &Connection, atom_id: &str) -> Result<Vec<Tag>, AtomicCoreError> {
    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.name, t.parent_id, t.created_at
             FROM tags t
             INNER JOIN atom_tags at ON t.id = at.tag_id
             WHERE at.atom_id = ?1",
        )
        ?;

    let tags = stmt
        .query_map([atom_id], |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
                parent_id: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        ?
        .collect::<Result<Vec<_>, _>>()
        ?;

    Ok(tags)
}

/// Get all descendant tag IDs (including the tag itself)
fn get_descendant_tag_ids(conn: &Connection, tag_id: &str) -> Result<Vec<String>, AtomicCoreError> {
    let mut all_tag_ids = vec![tag_id.to_string()];
    let mut to_process = vec![tag_id.to_string()];

    while let Some(current_id) = to_process.pop() {
        let mut child_stmt = conn
            .prepare("SELECT id FROM tags WHERE parent_id = ?1")
            ?;

        let children: Vec<String> = child_stmt
            .query_map([&current_id], |row| row.get(0))
            ?
            .collect::<Result<Vec<_>, _>>()
            ?;

        for child_id in children {
            all_tag_ids.push(child_id.clone());
            to_process.push(child_id);
        }
    }

    Ok(all_tag_ids)
}

/// Helper function to get all descendant tag IDs recursively
fn get_descendant_ids(tag_id: &str, all_tags: &[Tag]) -> Vec<String> {
    let mut result = vec![tag_id.to_string()];
    let children: Vec<&Tag> = all_tags
        .iter()
        .filter(|t| t.parent_id.as_deref() == Some(tag_id))
        .collect();
    for child in children {
        result.extend(get_descendant_ids(&child.id, all_tags));
    }
    result
}

/// Build hierarchical tag tree with counts
fn build_tag_tree(
    all_tags: &[Tag],
    parent_id: Option<&str>,
    conn: &Connection,
) -> Vec<TagWithCount> {
    all_tags
        .iter()
        .filter(|tag| tag.parent_id.as_deref() == parent_id)
        .map(|tag| {
            let children = build_tag_tree(all_tags, Some(&tag.id), conn);

            // Get all descendant tag IDs including this tag
            let descendant_ids = get_descendant_ids(&tag.id, all_tags);

            // Count distinct atoms across this tag and all descendants
            let placeholders = descendant_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let query = format!(
                "SELECT COUNT(DISTINCT atom_id) FROM atom_tags WHERE tag_id IN ({})",
                placeholders
            );

            let atom_count: i32 = conn
                .query_row(&query, rusqlite::params_from_iter(descendant_ids.iter()), |row| {
                    row.get(0)
                })
                .unwrap_or(0);

            TagWithCount {
                tag: tag.clone(),
                atom_count,
                children,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    /// Test utility: Create a test database
    fn create_test_db() -> (AtomicCore, NamedTempFile) {
        let temp_file = NamedTempFile::new().unwrap();
        let db = AtomicCore::open_or_create(temp_file.path()).unwrap();
        (db, temp_file)
    }

    /// Test utility: Create a test atom
    fn create_test_atom(db: &AtomicCore, content: &str) -> AtomWithTags {
        db.create_atom(
            CreateAtomRequest {
                content: content.to_string(),
                source_url: None,
                tag_ids: vec![],
            },
            |_| {}, // no-op callback
        )
        .unwrap()
    }

    // ==================== Atom CRUD Tests ====================

    #[test]
    fn test_create_atom_returns_atom() {
        let (db, _temp) = create_test_db();

        let atom = create_test_atom(&db, "Test content for atom");

        assert!(!atom.atom.id.is_empty());
        assert_eq!(atom.atom.content, "Test content for atom");
        assert_eq!(atom.atom.embedding_status, "pending");
        assert!(atom.tags.is_empty());
    }

    #[test]
    fn test_get_atom_by_id() {
        let (db, _temp) = create_test_db();

        let created = create_test_atom(&db, "Content to retrieve");
        let retrieved = db.get_atom(&created.atom.id).unwrap();

        assert!(retrieved.is_some());
        let atom = retrieved.unwrap();
        assert_eq!(atom.atom.id, created.atom.id);
        assert_eq!(atom.atom.content, "Content to retrieve");
    }

    #[test]
    fn test_get_atom_not_found() {
        let (db, _temp) = create_test_db();

        let result = db.get_atom("nonexistent-id-12345").unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn test_get_all_atoms() {
        let (db, _temp) = create_test_db();

        // Create multiple atoms
        create_test_atom(&db, "First atom");
        create_test_atom(&db, "Second atom");
        create_test_atom(&db, "Third atom");

        let all_atoms = db.get_all_atoms().unwrap();

        assert_eq!(all_atoms.len(), 3);
    }

    #[test]
    fn test_delete_atom() {
        let (db, _temp) = create_test_db();

        let atom = create_test_atom(&db, "Atom to delete");
        let atom_id = atom.atom.id.clone();

        // Verify it exists
        assert!(db.get_atom(&atom_id).unwrap().is_some());

        // Delete it
        db.delete_atom(&atom_id).unwrap();

        // Verify it's gone
        assert!(db.get_atom(&atom_id).unwrap().is_none());
    }

    // ==================== Tag CRUD Tests ====================

    #[test]
    fn test_create_tag_root() {
        let (db, _temp) = create_test_db();

        let tag = db.create_tag("Topics", None).unwrap();

        assert!(!tag.id.is_empty());
        assert_eq!(tag.name, "Topics");
        assert!(tag.parent_id.is_none());
    }

    #[test]
    fn test_create_tag_with_parent() {
        let (db, _temp) = create_test_db();

        // Create parent tag
        let parent = db.create_tag("Topics", None).unwrap();

        // Create child tag
        let child = db.create_tag("AI", Some(&parent.id)).unwrap();

        assert_eq!(child.name, "AI");
        assert_eq!(child.parent_id, Some(parent.id));
    }

    #[test]
    fn test_get_all_tags_hierarchical() {
        let (db, _temp) = create_test_db();

        // Create a hierarchy: Topics -> AI -> Machine Learning
        let topics = db.create_tag("Topics", None).unwrap();
        let ai = db.create_tag("AI", Some(&topics.id)).unwrap();
        let _ml = db.create_tag("Machine Learning", Some(&ai.id)).unwrap();

        let all_tags = db.get_all_tags().unwrap();

        // Should have one root tag (Topics) with nested children
        assert_eq!(all_tags.len(), 1);
        assert_eq!(all_tags[0].tag.name, "Topics");
        assert_eq!(all_tags[0].children.len(), 1);
        assert_eq!(all_tags[0].children[0].tag.name, "AI");
        assert_eq!(all_tags[0].children[0].children.len(), 1);
        assert_eq!(all_tags[0].children[0].children[0].tag.name, "Machine Learning");
    }

    #[test]
    fn test_delete_tag() {
        let (db, _temp) = create_test_db();

        let tag = db.create_tag("ToDelete", None).unwrap();
        let tag_id = tag.id.clone();

        // Verify it exists in get_all_tags
        let tags_before = db.get_all_tags().unwrap();
        assert!(tags_before.iter().any(|t| t.tag.id == tag_id));

        // Delete it
        db.delete_tag(&tag_id).unwrap();

        // Verify it's gone
        let tags_after = db.get_all_tags().unwrap();
        assert!(!tags_after.iter().any(|t| t.tag.id == tag_id));
    }

    // ==================== Atom-Tag Relationship Tests ====================

    #[test]
    fn test_create_atom_with_tags() {
        let (db, _temp) = create_test_db();

        // Create tags first
        let tag1 = db.create_tag("Tag1", None).unwrap();
        let tag2 = db.create_tag("Tag2", None).unwrap();

        // Create atom with tags
        let atom = db
            .create_atom(
                CreateAtomRequest {
                    content: "Tagged content".to_string(),
                    source_url: None,
                    tag_ids: vec![tag1.id.clone(), tag2.id.clone()],
                },
                |_| {},
            )
            .unwrap();

        // Verify tags are attached
        assert_eq!(atom.tags.len(), 2);
        let tag_names: Vec<&str> = atom.tags.iter().map(|t| t.name.as_str()).collect();
        assert!(tag_names.contains(&"Tag1"));
        assert!(tag_names.contains(&"Tag2"));
    }

    #[test]
    fn test_get_atoms_by_tag_includes_descendants() {
        let (db, _temp) = create_test_db();

        // Create hierarchy: Topics -> AI
        let topics = db.create_tag("Topics", None).unwrap();
        let ai = db.create_tag("AI", Some(&topics.id)).unwrap();

        // Create atom tagged with AI (child)
        let atom = db
            .create_atom(
                CreateAtomRequest {
                    content: "AI content".to_string(),
                    source_url: None,
                    tag_ids: vec![ai.id.clone()],
                },
                |_| {},
            )
            .unwrap();

        // Query by parent tag (Topics) should include atoms tagged with AI
        let atoms = db.get_atoms_by_tag(&topics.id).unwrap();

        assert_eq!(atoms.len(), 1);
        assert_eq!(atoms[0].atom.id, atom.atom.id);
    }

    #[test]
    fn test_atom_tag_counts() {
        let (db, _temp) = create_test_db();

        // Create parent tag
        let topics = db.create_tag("Topics", None).unwrap();

        // Create 3 atoms with this tag
        for i in 0..3 {
            db.create_atom(
                CreateAtomRequest {
                    content: format!("Atom {}", i),
                    source_url: None,
                    tag_ids: vec![topics.id.clone()],
                },
                |_| {},
            )
            .unwrap();
        }

        // Get tags and check count
        let all_tags = db.get_all_tags().unwrap();
        let topics_tag = all_tags.iter().find(|t| t.tag.name == "Topics").unwrap();

        assert_eq!(topics_tag.atom_count, 3);
    }
}
