//! Database management for Atomic Tauri app
//!
//! This module provides a Database wrapper that wraps atomic-core's Database.
//! All migrations (including chat tables) are handled by atomic-core.

use rusqlite::Connection;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

/// Tauri-specific Database wrapper
/// Wraps atomic-core::Database
pub struct Database {
    inner: atomic_core::Database,
}

/// Thread-safe wrapper around Database using Arc
pub type SharedDatabase = Arc<Database>;

impl Database {
    /// Create a new database connection
    /// Uses atomic-core for all tables (KB + chat)
    pub fn new(app_data_dir: PathBuf, _resource_dir: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&app_data_dir)
            .map_err(|e| format!("Failed to create app data directory: {}", e))?;

        let db_name = std::env::var("ATOMIC_DB_NAME")
            .map(|name| format!("{}.db", name))
            .unwrap_or_else(|_| "atomic.db".to_string());

        let db_path = app_data_dir.join(&db_name);
        eprintln!("Using database: {:?}", db_path);

        let inner = atomic_core::Database::open_or_create(&db_path)
            .map_err(|e| format!("Failed to initialize database: {}", e))?;

        Ok(Database { inner })
    }

    /// Create a new connection to the same database
    pub fn new_connection(&self) -> Result<Connection, String> {
        self.inner.new_connection().map_err(|e| e.to_string())
    }

    /// Create a new Database wrapper with a fresh connection to the same database file
    pub fn with_new_connection(&self) -> Result<Self, String> {
        let new_inner = atomic_core::Database::open(&self.inner.db_path)
            .map_err(|e| format!("Failed to create new connection: {}", e))?;
        Ok(Database { inner: new_inner })
    }

    /// Get reference to the underlying atomic-core Database
    pub fn as_core(&self) -> &atomic_core::Database {
        &self.inner
    }
}

// Implement Deref to allow using Database as atomic_core::Database
impl Deref for Database {
    type Target = atomic_core::Database;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
