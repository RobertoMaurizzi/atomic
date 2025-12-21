//! Data models for Atomic Tauri app
//!
//! This module re-exports types from atomic-core and defines Tauri-specific types.

// Re-export all KB types from atomic-core
pub use atomic_core::{
    Atom, AtomCluster, AtomPosition, AtomWithEmbedding, AtomWithTags,
    EmbeddingCompletePayload, NeighborhoodAtom, NeighborhoodEdge, NeighborhoodGraph,
    SemanticEdge, SemanticSearchResult, SimilarAtomResult, Tag, TagWithCount,
    TaggingCompletePayload, WikiArticleStatus, WikiArticleSummary,
    WikiArticleWithCitations,
};

// Re-export chat types from atomic-core (they're defined there for convenience)
pub use atomic_core::{
    ChatCitation, ChatMessage, ChatMessageWithContext, ChatToolCall, Conversation,
    ConversationWithMessages, ConversationWithTags,
};

// Note: CreateAtomRequest is defined in atomic-core lib.rs as a facade type
// We define a local version for the Tauri command compatibility
use serde::{Deserialize, Serialize};

/// Request payload for creating an atom (used by both Tauri commands and HTTP API)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAtomRequest {
    pub content: String,
    pub source_url: Option<String>,
    pub tag_ids: Vec<String>,
}

impl From<CreateAtomRequest> for atomic_core::CreateAtomRequest {
    fn from(req: CreateAtomRequest) -> Self {
        atomic_core::CreateAtomRequest {
            content: req.content,
            source_url: req.source_url,
            tag_ids: req.tag_ids,
        }
    }
}
