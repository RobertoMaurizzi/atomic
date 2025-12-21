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
        std::process::exit(1);
    }

    // Open database
    let db = match db::open_database(&db_path) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Error opening database: {}", e);
            std::process::exit(1);
        }
    };

    eprintln!("Database opened successfully");

    // Create MCP server
    let server = server::AtomicMcpServer::new(db);

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

/// Get the path to the Atomic database
fn get_database_path() -> PathBuf {
    // Check environment variable first (for testing/custom paths)
    if let Ok(path) = std::env::var("ATOMIC_DB_PATH") {
        return PathBuf::from(path);
    }

    // Use standard Tauri app data directory
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            return home
                .join("Library/Application Support/com.atomic.app")
                .join("atomic.db");
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(data_dir) = dirs::data_local_dir() {
            return data_dir.join("com.atomic.app").join("atomic.db");
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(data_dir) = dirs::data_local_dir() {
            return data_dir.join("com.atomic.app").join("atomic.db");
        }
    }

    // Fallback
    PathBuf::from("atomic.db")
}
