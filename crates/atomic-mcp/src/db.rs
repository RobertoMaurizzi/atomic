//! Database access for the standalone MCP server
//!
//! This module provides a convenience function for opening an AtomicCore instance.

pub use atomic_core::AtomicCore;

use std::path::Path;

/// Open the database at the given path and return an AtomicCore instance
pub fn open_database(path: &Path) -> Result<AtomicCore, String> {
    AtomicCore::open(path).map_err(|e| format!("Failed to open database: {}", e))
}
