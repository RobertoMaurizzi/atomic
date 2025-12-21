//! Types for MCP tool parameters and responses

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Parameters for semantic search
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SemanticSearchParams {
    /// The search query
    pub query: String,
    /// Maximum number of results (default: 10, max: 50)
    #[serde(default)]
    pub limit: Option<i32>,
}

/// A search result
#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub atom_id: String,
    pub content_preview: String,
    pub similarity_score: f32,
    pub matching_chunk: Option<String>,
}

/// Parameters for reading an atom
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadAtomParams {
    /// The ID of the atom to read
    pub atom_id: String,
    /// Line offset for pagination (default: 0)
    #[serde(default)]
    pub offset: Option<i32>,
    /// Maximum lines to return (default: 100, max: 500)
    #[serde(default)]
    pub limit: Option<i32>,
}

/// Atom content response
#[derive(Debug, Serialize)]
pub struct AtomContent {
    pub atom_id: String,
    pub content: String,
    pub total_lines: i32,
    pub returned_lines: i32,
    pub offset: i32,
    pub has_more: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Parameters for creating an atom
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateAtomParams {
    /// The markdown content of the atom
    pub content: String,
    /// Optional source URL
    #[serde(default)]
    pub source_url: Option<String>,
    /// Optional tag IDs to attach
    #[serde(default)]
    pub tag_ids: Option<Vec<String>>,
}

/// Created atom response
#[derive(Debug, Serialize)]
pub struct CreatedAtom {
    pub atom_id: String,
    pub content_preview: String,
    pub embedding_status: String,
}
