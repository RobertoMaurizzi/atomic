use rusqlite::ffi::sqlite3_auto_extension;
use rusqlite::Connection;
use sqlite_vec::sqlite3_vec_init;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct Database {
    pub conn: Mutex<Connection>,
    pub db_path: PathBuf,
    pub resource_dir: PathBuf,
}

/// Thread-safe wrapper around Database using Arc
pub type SharedDatabase = Arc<Database>;

impl Database {
    pub fn new(app_data_dir: PathBuf, resource_dir: PathBuf) -> Result<Self, String> {
        // Register sqlite-vec extension
        unsafe {
            #[allow(clippy::missing_transmute_annotations)]
            sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
        }

        // Create database directory if it doesn't exist
        std::fs::create_dir_all(&app_data_dir)
            .map_err(|e| format!("Failed to create app data directory: {}", e))?;

        let db_path = app_data_dir.join("atomic.db");
        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Failed to open database: {}", e))?;

        // Enable extension loading and load sqlite-lembed
        Self::load_lembed_extension(&conn, &resource_dir)?;

        // Run migrations
        Self::run_migrations(&conn)?;

        // Register the embedding model
        Self::register_embedding_model(&conn, &resource_dir)?;

        Ok(Database {
            conn: Mutex::new(conn),
            db_path,
            resource_dir,
        })
    }

    /// Load the sqlite-lembed extension into a connection
    fn load_lembed_extension(conn: &Connection, resource_dir: &PathBuf) -> Result<(), String> {
        // Enable extension loading and load sqlite-lembed extension
        // Both operations are unsafe as they involve loading external code
        unsafe {
            conn.load_extension_enable()
                .map_err(|e| format!("Failed to enable extension loading: {}", e))?;

            // Load sqlite-lembed extension
            let lembed_path = resource_dir.join("lembed0");
            conn.load_extension(&lembed_path, None)
                .map_err(|e| format!("Failed to load sqlite-lembed extension: {}", e))?;
        }

        Ok(())
    }

    /// Register the embedding model for a connection
    /// The model is registered in temp.lembed_models which is a temporary table,
    /// so it needs to be re-registered for each new connection
    fn register_embedding_model(conn: &Connection, resource_dir: &PathBuf) -> Result<(), String> {
        let model_path = resource_dir.join("all-MiniLM-L6-v2.q8_0.gguf");
        let model_path_str = model_path
            .to_str()
            .ok_or("Invalid model path")?;

        conn.execute(
            "INSERT INTO temp.lembed_models(name, model) SELECT 'all-MiniLM-L6-v2', lembed_model_from_file(?1)",
            [model_path_str],
        )
        .map_err(|e| format!("Failed to register embedding model: {}", e))?;

        Ok(())
    }

    /// Create a new connection to the same database
    /// This is useful for background tasks that need their own connection
    /// The connection will have sqlite-lembed loaded and the model registered
    pub fn new_connection(&self) -> Result<Connection, String> {
        let conn = Connection::open(&self.db_path)
            .map_err(|e| format!("Failed to open database connection: {}", e))?;

        // Load sqlite-lembed extension and register model for this connection
        Self::load_lembed_extension(&conn, &self.resource_dir)?;
        Self::register_embedding_model(&conn, &self.resource_dir)?;

        Ok(conn)
    }

    fn run_migrations(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            r#"
            -- Atoms are the core content units
            CREATE TABLE IF NOT EXISTS atoms (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                source_url TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            -- Hierarchical tags
            CREATE TABLE IF NOT EXISTS tags (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                parent_id TEXT REFERENCES tags(id) ON DELETE SET NULL,
                created_at TEXT NOT NULL
            );

            -- Many-to-many relationship
            CREATE TABLE IF NOT EXISTS atom_tags (
                atom_id TEXT REFERENCES atoms(id) ON DELETE CASCADE,
                tag_id TEXT REFERENCES tags(id) ON DELETE CASCADE,
                PRIMARY KEY (atom_id, tag_id)
            );

            -- For Phase 2 embeddings
            CREATE TABLE IF NOT EXISTS atom_chunks (
                id TEXT PRIMARY KEY,
                atom_id TEXT REFERENCES atoms(id) ON DELETE CASCADE,
                chunk_index INTEGER NOT NULL,
                content TEXT NOT NULL,
                embedding BLOB
            );

            -- Indexes
            CREATE INDEX IF NOT EXISTS idx_atom_chunks_atom_id ON atom_chunks(atom_id);
            CREATE INDEX IF NOT EXISTS idx_atom_tags_atom_id ON atom_tags(atom_id);
            CREATE INDEX IF NOT EXISTS idx_atom_tags_tag_id ON atom_tags(tag_id);
            CREATE INDEX IF NOT EXISTS idx_tags_parent_id ON tags(parent_id);
            "#,
        )
        .map_err(|e| format!("Failed to run migrations: {}", e))?;

        // Add embedding_status column to atoms table if it doesn't exist
        Self::add_embedding_status_column(conn)?;

        // Create vec_chunks virtual table for sqlite-vec similarity search
        Self::create_vec_chunks_table(conn)?;

        Ok(())
    }

    fn add_embedding_status_column(conn: &Connection) -> Result<(), String> {
        // Check if embedding_status column exists
        let column_exists: bool = conn
            .prepare("SELECT 1 FROM pragma_table_info('atoms') WHERE name = 'embedding_status'")
            .map_err(|e| format!("Failed to prepare column check: {}", e))?
            .exists([])
            .map_err(|e| format!("Failed to check column existence: {}", e))?;

        if !column_exists {
            conn.execute(
                "ALTER TABLE atoms ADD COLUMN embedding_status TEXT DEFAULT 'pending'",
                [],
            )
            .map_err(|e| format!("Failed to add embedding_status column: {}", e))?;
        }

        Ok(())
    }

    fn create_vec_chunks_table(conn: &Connection) -> Result<(), String> {
        // Create vec_chunks virtual table for sqlite-vec similarity search
        // This uses the vec0 module from sqlite-vec for vector similarity
        conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS vec_chunks USING vec0(
                chunk_id TEXT PRIMARY KEY,
                embedding float[384]
            )",
            [],
        )
        .map_err(|e| format!("Failed to create vec_chunks table: {}", e))?;

        Ok(())
    }
}

