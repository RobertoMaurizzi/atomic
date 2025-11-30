use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Atom {
    pub id: String,
    pub content: String,
    pub source_url: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub embedding_status: String, // 'pending', 'processing', 'complete', 'failed'
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomWithTags {
    #[serde(flatten)]
    pub atom: Atom,
    pub tags: Vec<Tag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagWithCount {
    #[serde(flatten)]
    pub tag: Tag,
    pub atom_count: i32,
    pub children: Vec<TagWithCount>,
}

/// Result struct for similar atom search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarAtomResult {
    #[serde(flatten)]
    pub atom: AtomWithTags,
    pub similarity_score: f32,
    pub matching_chunk_content: String,
    pub matching_chunk_index: i32,
}

/// Result struct for semantic search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchResult {
    #[serde(flatten)]
    pub atom: AtomWithTags,
    pub similarity_score: f32,
    pub matching_chunk_content: String,
    pub matching_chunk_index: i32,
}

/// Payload for embedding-complete event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingCompletePayload {
    pub atom_id: String,
    pub status: String, // "complete" or "failed"
    pub error: Option<String>,
    pub tags_extracted: Vec<String>,      // IDs of all tags applied
    pub new_tags_created: Vec<String>,    // IDs of newly created tags
}

/// Chunk data for internal use
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ChunkData {
    pub id: String,
    pub atom_id: String,
    pub chunk_index: i32,
    pub content: String,
}

/// Wiki article for a tag
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiArticle {
    pub id: String,
    pub tag_id: String,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
    pub atom_count: i32,
}

/// Citation linking article content to source atom/chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiCitation {
    pub id: String,
    pub citation_index: i32,
    pub atom_id: String,
    pub chunk_index: Option<i32>,
    pub excerpt: String,
}

/// Wiki article with all its citations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiArticleWithCitations {
    pub article: WikiArticle,
    pub citations: Vec<WikiCitation>,
}

/// Status of a wiki article for quick checks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiArticleStatus {
    pub has_article: bool,
    pub article_atom_count: i32,
    pub current_atom_count: i32,
    pub new_atoms_available: i32,
    pub updated_at: Option<String>,
}

/// Chunk with context for wiki generation
#[derive(Debug, Clone)]
pub struct ChunkWithContext {
    pub atom_id: String,
    pub chunk_index: i32,
    pub content: String,
    pub similarity_score: f32,
}

/// Position of an atom on the canvas
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomPosition {
    pub atom_id: String,
    pub x: f64,
    pub y: f64,
}

/// Atom with its average embedding vector for similarity calculations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomWithEmbedding {
    #[serde(flatten)]
    pub atom: AtomWithTags,
    pub embedding: Option<Vec<f32>>,  // Average of chunk embeddings, None if not yet embedded
}

/// Request payload for creating an atom (used by both Tauri commands and HTTP API)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAtomRequest {
    pub content: String,
    pub source_url: Option<String>,
    pub tag_ids: Vec<String>,
}

// ==================== Chat Types ====================

/// Chat conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub is_archived: bool,
}

/// Conversation with its tag scope and summary info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationWithTags {
    #[serde(flatten)]
    pub conversation: Conversation,
    pub tags: Vec<Tag>,
    pub message_count: i32,
    pub last_message_preview: Option<String>,
}

/// Conversation with full message history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationWithMessages {
    #[serde(flatten)]
    pub conversation: Conversation,
    pub tags: Vec<Tag>,
    pub messages: Vec<ChatMessageWithContext>,
}

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub conversation_id: String,
    pub role: String, // "user", "assistant", "system", "tool"
    pub content: String,
    pub created_at: String,
    pub message_index: i32,
}

/// Message with tool calls and citations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageWithContext {
    #[serde(flatten)]
    pub message: ChatMessage,
    pub tool_calls: Vec<ChatToolCall>,
    pub citations: Vec<ChatCitation>,
}

/// Tool call record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatToolCall {
    pub id: String,
    pub message_id: String,
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub tool_output: Option<serde_json::Value>,
    pub status: String, // "pending", "running", "complete", "failed"
    pub created_at: String,
    pub completed_at: Option<String>,
}

/// Citation in a chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCitation {
    pub id: String,
    pub message_id: String,
    pub citation_index: i32,
    pub atom_id: String,
    pub chunk_index: Option<i32>,
    pub excerpt: String,
    pub relevance_score: Option<f32>,
}

// ==================== Semantic Graph Types ====================

/// Pre-computed semantic edge between two atoms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticEdge {
    pub id: String,
    pub source_atom_id: String,
    pub target_atom_id: String,
    pub similarity_score: f32,
    pub source_chunk_index: Option<i32>,
    pub target_chunk_index: Option<i32>,
    pub created_at: String,
}

/// Neighborhood graph for local graph view
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeighborhoodGraph {
    pub center_atom_id: String,
    pub atoms: Vec<NeighborhoodAtom>,
    pub edges: Vec<NeighborhoodEdge>,
}

/// Atom in a neighborhood graph with depth info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeighborhoodAtom {
    #[serde(flatten)]
    pub atom: AtomWithTags,
    pub depth: i32, // 0 = center, 1 = direct connection, 2 = friend-of-friend
}

/// Edge in a neighborhood graph (combines tag and semantic connections)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeighborhoodEdge {
    pub source_id: String,
    pub target_id: String,
    pub edge_type: String, // "tag", "semantic", "both"
    pub strength: f32,     // Combined strength (0-1)
    pub shared_tag_count: i32,
    pub similarity_score: Option<f32>,
}

/// Atom cluster assignment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomCluster {
    pub cluster_id: i32,
    pub atom_ids: Vec<String>,
    pub dominant_tags: Vec<String>,
}


