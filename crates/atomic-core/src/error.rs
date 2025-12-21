//! Error types for atomic-core

use thiserror::Error;

/// Main error type for atomic-core operations
#[derive(Error, Debug)]
pub enum AtomicCoreError {
    /// Database error from rusqlite
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// Provider error (embedding, LLM)
    #[error("Provider error: {0}")]
    Provider(#[from] crate::providers::ProviderError),

    /// Configuration error (missing settings, invalid values)
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Resource not found
    #[error("Not found: {0}")]
    NotFound(String),

    /// Validation error (invalid input)
    #[error("Validation error: {0}")]
    Validation(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Lock acquisition error
    #[error("Lock error: {0}")]
    Lock(String),

    /// Embedding generation error
    #[error("Embedding error: {0}")]
    Embedding(String),

    /// Search error
    #[error("Search error: {0}")]
    Search(String),

    /// Wiki generation error
    #[error("Wiki error: {0}")]
    Wiki(String),

    /// Clustering error
    #[error("Clustering error: {0}")]
    Clustering(String),

    /// Compaction error
    #[error("Compaction error: {0}")]
    Compaction(String),

    /// General database operation error (string-based)
    #[error("Database operation error: {0}")]
    DatabaseOperation(String),
}

/// Result type alias for atomic-core operations
pub type Result<T> = std::result::Result<T, AtomicCoreError>;
