//! Database access for the standalone MCP server
//!
//! This module re-exports the Database type from atomic-core.

pub use atomic_core::Database;

use std::path::Path;
use std::sync::Arc;

/// Open the database at the given path
/// This is a convenience function that wraps atomic-core's Database::open
pub fn open_database(path: &Path) -> Result<Arc<Database>, String> {
    let db = Database::open(path).map_err(|e| format!("Failed to open database: {}", e))?;
    Ok(Arc::new(db))
}
