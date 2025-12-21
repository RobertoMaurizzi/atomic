//! Database management for Atomic Tauri app
//!
//! This module provides a Database wrapper that:
//! - Wraps atomic-core's Database for KB operations
//! - Adds chat table migrations (not part of atomic-core's KB scope)
//! - Supports custom database names via environment variable

use rusqlite::Connection;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

// Re-export atomic-core's Database for direct use where needed

/// Tauri-specific Database wrapper
/// Wraps atomic-core::Database and adds chat functionality
pub struct Database {
    inner: atomic_core::Database,
}

/// Thread-safe wrapper around Database using Arc
pub type SharedDatabase = Arc<Database>;

impl Database {
    /// Create a new database connection
    /// Uses atomic-core for KB tables and adds chat tables locally
    pub fn new(app_data_dir: PathBuf, _resource_dir: PathBuf) -> Result<Self, String> {
        // Create database directory if it doesn't exist
        std::fs::create_dir_all(&app_data_dir)
            .map_err(|e| format!("Failed to create app data directory: {}", e))?;

        // Check for custom database name via environment variable
        let db_name = std::env::var("ATOMIC_DB_NAME")
            .map(|name| format!("{}.db", name))
            .unwrap_or_else(|_| "atomic.db".to_string());

        let db_path = app_data_dir.join(&db_name);
        eprintln!("Using database: {:?}", db_path);

        // Use atomic-core to create/open the database with KB migrations
        let inner = atomic_core::Database::open_or_create(&db_path)
            .map_err(|e| format!("Failed to initialize database: {}", e))?;

        // Run chat migrations
        {
            let conn = inner.conn.lock().map_err(|_| "Failed to lock connection")?;
            Self::run_chat_migrations(&conn)?;
        }

        Ok(Database { inner })
    }

    /// Create a new connection to the same database
    pub fn new_connection(&self) -> Result<Connection, String> {
        self.inner.new_connection()
            .map_err(|e| e.to_string())
    }

    /// Create a new Database wrapper with a fresh connection to the same database file
    /// Useful for creating shared database instances for async operations
    pub fn with_new_connection(&self) -> Result<Self, String> {
        let new_inner = atomic_core::Database::open(&self.inner.db_path)
            .map_err(|e| format!("Failed to create new connection: {}", e))?;
        Ok(Database { inner: new_inner })
    }

    /// Get reference to the underlying atomic-core Database
    pub fn as_core(&self) -> &atomic_core::Database {
        &self.inner
    }

    /// Run chat-specific migrations (not part of atomic-core)
    fn run_chat_migrations(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            r#"
            -- Chat conversations
            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                is_archived INTEGER DEFAULT 0
            );

            -- Many-to-many: conversation tag scope
            CREATE TABLE IF NOT EXISTS conversation_tags (
                conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
                tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
                PRIMARY KEY (conversation_id, tag_id)
            );

            -- Chat messages
            CREATE TABLE IF NOT EXISTS chat_messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL,
                message_index INTEGER NOT NULL
            );

            -- Tool calls for transparency
            CREATE TABLE IF NOT EXISTS chat_tool_calls (
                id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL REFERENCES chat_messages(id) ON DELETE CASCADE,
                tool_name TEXT NOT NULL,
                tool_input TEXT NOT NULL,
                tool_output TEXT,
                status TEXT NOT NULL DEFAULT 'pending',
                created_at TEXT NOT NULL,
                completed_at TEXT
            );

            -- Chat citations
            CREATE TABLE IF NOT EXISTS chat_citations (
                id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL REFERENCES chat_messages(id) ON DELETE CASCADE,
                citation_index INTEGER NOT NULL,
                atom_id TEXT NOT NULL REFERENCES atoms(id) ON DELETE CASCADE,
                chunk_index INTEGER,
                excerpt TEXT NOT NULL,
                relevance_score REAL
            );

            -- Indexes for chat tables
            CREATE INDEX IF NOT EXISTS idx_conversations_updated ON conversations(updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_conversation_tags_conv ON conversation_tags(conversation_id);
            CREATE INDEX IF NOT EXISTS idx_conversation_tags_tag ON conversation_tags(tag_id);
            CREATE INDEX IF NOT EXISTS idx_chat_messages_conversation ON chat_messages(conversation_id, message_index);
            CREATE INDEX IF NOT EXISTS idx_chat_tool_calls_message ON chat_tool_calls(message_id);
            CREATE INDEX IF NOT EXISTS idx_chat_citations_message ON chat_citations(message_id);
            CREATE INDEX IF NOT EXISTS idx_chat_citations_atom ON chat_citations(atom_id);
            "#,
        )
        .map_err(|e| format!("Failed to run chat migrations: {}", e))?;

        Ok(())
    }
}

// Implement Deref to allow using Database as atomic_core::Database
impl Deref for Database {
    type Target = atomic_core::Database;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// Get embedding dimension based on current settings
pub fn get_current_embedding_dimension(conn: &Connection) -> usize {
    use atomic_core::providers::ProviderConfig;

    let settings_map = atomic_core::settings::get_all_settings(conn).unwrap_or_default();
    let config = ProviderConfig::from_settings(&settings_map);
    config.embedding_dimension()
}

/// Check if dimension will change with new settings
pub fn will_dimension_change(conn: &Connection, key: &str, new_value: &str) -> (bool, usize) {
    use atomic_core::providers::ProviderConfig;

    let current_dim = get_current_embedding_dimension(conn);

    // Get current settings and apply the change
    let mut settings_map = atomic_core::settings::get_all_settings(conn).unwrap_or_default();
    settings_map.insert(key.to_string(), new_value.to_string());

    let new_config = ProviderConfig::from_settings(&settings_map);
    let new_dim = new_config.embedding_dimension();

    (current_dim != new_dim, new_dim)
}

/// Recreate vec_chunks table with a new dimension and reset embedding status
pub fn recreate_vec_chunks_with_dimension(conn: &Connection, dimension: usize) -> Result<(), String> {
    // Drop existing vec_chunks table
    conn.execute("DROP TABLE IF EXISTS vec_chunks", [])
        .map_err(|e| format!("Failed to drop vec_chunks table: {}", e))?;

    // Create new vec_chunks table with the specified dimension
    let create_sql = format!(
        "CREATE VIRTUAL TABLE vec_chunks USING vec0(chunk_id TEXT PRIMARY KEY, embedding float[{}])",
        dimension
    );
    conn.execute(&create_sql, [])
        .map_err(|e| format!("Failed to create vec_chunks table: {}", e))?;

    // Reset ONLY embedding status to pending
    conn.execute("UPDATE atoms SET embedding_status = 'pending'", [])
        .map_err(|e| format!("Failed to reset atom embedding status: {}", e))?;

    // Set tagging_status to 'skipped' - existing tags are preserved
    conn.execute("UPDATE atoms SET tagging_status = 'skipped'", [])
        .map_err(|e| format!("Failed to update atom tagging status: {}", e))?;

    // Clear all existing chunk data
    conn.execute("DELETE FROM atom_chunks", [])
        .map_err(|e| format!("Failed to clear atom_chunks: {}", e))?;

    // Clear FTS5 table
    conn.execute("DELETE FROM atom_chunks_fts", [])
        .map_err(|e| format!("Failed to clear atom_chunks_fts: {}", e))?;

    // Clear semantic edges
    conn.execute("DELETE FROM semantic_edges", [])
        .map_err(|e| format!("Failed to clear semantic_edges: {}", e))?;

    // Clear canvas positions
    conn.execute("DELETE FROM atom_positions", [])
        .map_err(|e| format!("Failed to clear atom_positions: {}", e))?;

    Ok(())
}
