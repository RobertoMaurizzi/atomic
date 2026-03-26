//! Database manager for multi-database support.
//!
//! `DatabaseManager` holds the registry and a lazy-loaded map of `AtomicCore`
//! instances. It provides the main entry point for server and desktop code
//! to resolve which database to operate on.

use crate::error::AtomicCoreError;
use crate::registry::{DatabaseInfo, Registry};
use crate::AtomicCore;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

/// Manages multiple knowledge-base databases with a shared registry.
pub struct DatabaseManager {
    registry: Arc<Registry>,
    cores: RwLock<HashMap<String, AtomicCore>>,
    active_id: RwLock<String>,
}

impl DatabaseManager {
    /// Create a new manager, opening or creating the registry in `data_dir`.
    pub fn new(data_dir: impl AsRef<Path>) -> Result<Self, AtomicCoreError> {
        let registry = Arc::new(Registry::open_or_create(&data_dir)?);
        let default_id = registry.get_default_database_id()?;

        Ok(DatabaseManager {
            registry,
            cores: RwLock::new(HashMap::new()),
            active_id: RwLock::new(default_id),
        })
    }

    /// Create a manager that uses Postgres for data storage.
    /// Registry stays as local SQLite for settings, tokens, OAuth, and DB metadata.
    #[cfg(feature = "postgres")]
    pub async fn new_postgres(
        data_dir: impl AsRef<Path>,
        database_url: &str,
    ) -> Result<Self, AtomicCoreError> {
        let registry = Arc::new(Registry::open_or_create(&data_dir)?);
        let default_id = registry.get_default_database_id()?;

        let core = AtomicCore::open_postgres(
            database_url,
            Some(Arc::clone(&registry)),
        ).await?;

        let mut cores_map = HashMap::new();
        cores_map.insert(default_id.clone(), core);

        Ok(DatabaseManager {
            registry,
            cores: RwLock::new(cores_map),
            active_id: RwLock::new(default_id),
        })
    }

    /// Get a core for a specific database, loading it lazily if needed.
    pub fn get_core(&self, id: &str) -> Result<AtomicCore, AtomicCoreError> {
        // Fast path: already loaded
        {
            let cores = self.cores.read().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
            if let Some(core) = cores.get(id) {
                return Ok(core.clone());
            }
        }

        // Slow path: load from disk
        let db_path = self.registry.database_path(id);
        let core = AtomicCore::open_for_server_with_registry(
            &db_path,
            Some(Arc::clone(&self.registry)),
        )?;

        self.registry.touch_database(id)?;

        let mut cores = self.cores.write().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        cores.insert(id.to_string(), core.clone());
        Ok(core)
    }

    /// Get the active (current) database core.
    pub fn active_core(&self) -> Result<AtomicCore, AtomicCoreError> {
        let id = self.active_id.read().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        self.get_core(&id)
    }

    /// Get the active database ID.
    pub fn active_id(&self) -> Result<String, AtomicCoreError> {
        let id = self.active_id.read().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        Ok(id.clone())
    }

    /// Switch the active database.
    pub fn set_active(&self, id: &str) -> Result<(), AtomicCoreError> {
        // Validate the database exists
        let databases = self.registry.list_databases()?;
        if !databases.iter().any(|d| d.id == id) {
            return Err(AtomicCoreError::NotFound(format!("Database '{}'", id)));
        }

        // Ensure it's loaded
        self.get_core(id)?;

        let mut active = self.active_id.write().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        *active = id.to_string();
        Ok(())
    }

    /// Get a reference to the registry for settings/token/database CRUD.
    pub fn registry(&self) -> &Arc<Registry> {
        &self.registry
    }

    /// Create a new database and register it.
    pub fn create_database(&self, name: &str) -> Result<DatabaseInfo, AtomicCoreError> {
        let info = self.registry.create_database(name)?;

        // Create the actual SQLite file
        let db_path = self.registry.database_path(&info.id);
        let core = AtomicCore::open_for_server_with_registry(
            &db_path,
            Some(Arc::clone(&self.registry)),
        )?;

        let mut cores = self.cores.write().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        cores.insert(info.id.clone(), core);

        Ok(info)
    }

    /// Delete a database (cannot delete default). Removes from cache and disk.
    pub fn delete_database(&self, id: &str) -> Result<(), AtomicCoreError> {
        // Registry validates it's not the default
        self.registry.delete_database(id)?;

        // Remove from cache
        {
            let mut cores =
                self.cores.write().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
            if let Some(core) = cores.remove(id) {
                core.optimize();
            }
        }

        // If this was the active database, switch to default
        {
            let active = self.active_id.read().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
            if *active == id {
                drop(active);
                let default_id = self.registry.get_default_database_id()?;
                let mut active =
                    self.active_id.write().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
                *active = default_id;
            }
        }

        // Delete the file
        let db_path = self.registry.database_path(id);
        if db_path.exists() {
            std::fs::remove_file(&db_path).ok();
            // Also remove WAL/SHM
            std::fs::remove_file(db_path.with_extension("db-wal")).ok();
            std::fs::remove_file(db_path.with_extension("db-shm")).ok();
        }

        Ok(())
    }

    /// List all databases with their info, plus which is active.
    pub fn list_databases(&self) -> Result<(Vec<DatabaseInfo>, String), AtomicCoreError> {
        let databases = self.registry.list_databases()?;
        let active = self.active_id.read().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        Ok((databases, active.clone()))
    }

    /// Optimize all loaded cores (call on shutdown).
    pub fn optimize_all(&self) {
        if let Ok(cores) = self.cores.read() {
            for core in cores.values() {
                core.optimize();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_new_manager() {
        let dir = TempDir::new().unwrap();
        let manager = DatabaseManager::new(dir.path()).unwrap();

        let (databases, active_id) = manager.list_databases().unwrap();
        assert_eq!(databases.len(), 1);
        assert_eq!(active_id, "default");
    }

    #[test]
    fn test_get_active_core() {
        let dir = TempDir::new().unwrap();
        let manager = DatabaseManager::new(dir.path()).unwrap();

        let core = manager.active_core().unwrap();
        // Should be able to query the core
        let settings = core.get_settings().unwrap();
        assert!(settings.contains_key("provider"));
    }

    #[test]
    fn test_create_and_switch_database() {
        let dir = TempDir::new().unwrap();
        let manager = DatabaseManager::new(dir.path()).unwrap();

        let info = manager.create_database("Work").unwrap();
        assert_eq!(info.name, "Work");

        manager.set_active(&info.id).unwrap();
        let active = manager.active_id().unwrap();
        assert_eq!(active, info.id);
    }

    #[test]
    fn test_delete_database() {
        let dir = TempDir::new().unwrap();
        let manager = DatabaseManager::new(dir.path()).unwrap();

        let info = manager.create_database("Temp").unwrap();
        manager.delete_database(&info.id).unwrap();

        let (databases, _) = manager.list_databases().unwrap();
        assert_eq!(databases.len(), 1); // only default
    }

    #[test]
    fn test_delete_active_switches_to_default() {
        let dir = TempDir::new().unwrap();
        let manager = DatabaseManager::new(dir.path()).unwrap();

        let info = manager.create_database("Temp").unwrap();
        manager.set_active(&info.id).unwrap();
        manager.delete_database(&info.id).unwrap();

        let active = manager.active_id().unwrap();
        assert_eq!(active, "default");
    }

    #[test]
    fn test_cannot_delete_default() {
        let dir = TempDir::new().unwrap();
        let manager = DatabaseManager::new(dir.path()).unwrap();

        let result = manager.delete_database("default");
        assert!(result.is_err());
    }
}
