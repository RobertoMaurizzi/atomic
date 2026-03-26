//! Atomic MCP Server (Standalone)
//!
//! A standalone MCP server that provides access to the Atomic knowledge base
//! via stdio transport. This allows MCP clients like Claude Desktop to interact
//! with Atomic without requiring the main Tauri app to be running.
//!
//! The server connects directly to the Atomic database and provides tools for:
//! - Semantic search across atoms
//! - Reading atom content
//! - Creating new atoms

mod db;
mod server;
mod types;

use std::path::PathBuf;

#[tokio::main]
async fn main() {
    // Find database path
    let db_path = get_database_path();

    eprintln!("Atomic MCP Server v{}", env!("CARGO_PKG_VERSION"));
    eprintln!("Database: {}", db_path.display());

    // Check if database exists
    if !db_path.exists() {
        eprintln!("Error: Database not found at {}", db_path.display());
        eprintln!("Please run Atomic at least once to initialize the database.");
        eprintln!("Hint: Set ATOMIC_DATA_DIR or ATOMIC_DB_PATH to specify the data directory or database file.");
        std::process::exit(1);
    }

    // Open database
    let core = match db::open_database(&db_path) {
        Ok(core) => core,
        Err(e) => {
            eprintln!("Error opening database: {}", e);
            std::process::exit(1);
        }
    };

    eprintln!("Database opened successfully");

    // Create MCP server
    let server = server::AtomicMcpServer::new(core);

    // Run with stdio transport
    eprintln!("Starting MCP server on stdio...");

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let running_server = match rmcp::serve_server(server, (stdin, stdout)).await {
        Ok(server) => server,
        Err(e) => {
            eprintln!("Server initialization error: {}", e);
            std::process::exit(1);
        }
    };

    // Wait for the server to complete (runs until client disconnects)
    let _ = running_server.waiting().await;
}

/// Get the path to the Atomic database.
///
/// Resolution order:
/// 1. `ATOMIC_DB_PATH` env var — direct path to a .db file (backwards compat)
/// 2. `ATOMIC_DATA_DIR` env var — data directory with registry.db + databases/
/// 3. Standard Tauri app data directory (tries new layout first, falls back to atomic.db)
fn get_database_path() -> PathBuf {
    // Direct database path (backwards compat)
    if let Ok(path) = std::env::var("ATOMIC_DB_PATH") {
        return PathBuf::from(path);
    }

    // Data directory with registry layout
    if let Ok(dir) = std::env::var("ATOMIC_DATA_DIR") {
        let data_dir = PathBuf::from(dir);
        if let Some(path) = resolve_from_data_dir(&data_dir) {
            return path;
        }
    }

    // Standard app data directory
    let app_data_dir = get_app_data_dir();
    if let Some(ref dir) = app_data_dir {
        if let Some(path) = resolve_from_data_dir(dir) {
            return path;
        }
        // Legacy fallback: atomic.db in the app data dir
        return dir.join("atomic.db");
    }

    // Final fallback
    PathBuf::from("atomic.db")
}

/// Try to resolve the default database path from a data directory.
/// Returns Some if registry.db exists (new layout), None otherwise.
fn resolve_from_data_dir(data_dir: &std::path::Path) -> Option<PathBuf> {
    let registry_path = data_dir.join("registry.db");
    if registry_path.exists() {
        // Use the registry to find the default database
        match atomic_core::Registry::open_or_create(data_dir) {
            Ok(registry) => {
                // Check ATOMIC_DB env var for specific database selection
                let db_id = std::env::var("ATOMIC_DB").ok();
                let target_id = db_id.as_deref().unwrap_or("default");

                let databases = registry.list_databases().ok()?;
                if databases.iter().any(|d| d.id == target_id) {
                    return Some(registry.database_path(target_id));
                }
                // Fall back to default
                let default_id = registry.get_default_database_id().ok()?;
                Some(registry.database_path(&default_id))
            }
            Err(_) => None,
        }
    } else {
        None
    }
}

fn get_app_data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|h| h.join("Library/Application Support/com.atomic.app"))
    }

    #[cfg(target_os = "linux")]
    {
        dirs::data_local_dir().map(|d| d.join("com.atomic.app"))
    }

    #[cfg(target_os = "windows")]
    {
        dirs::data_local_dir().map(|d| d.join("com.atomic.app"))
    }
}
