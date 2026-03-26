//! Storage trait definitions for atomic-core.
//!
//! These traits define the storage abstraction layer. All database operations
//! go through these traits, allowing different backends (SQLite, Postgres, etc.)
//! to be plugged in.
//!
//! All trait methods are async to support both sync backends (SQLite via
//! spawn_blocking) and natively async backends (Postgres via sqlx).

use async_trait::async_trait;

use crate::models::AtomCluster;
use crate::compaction::{CompactionResult, TagMerge};
use crate::error::AtomicCoreError;
use crate::models::*;
use crate::{CreateAtomRequest, ListAtomsParams, UpdateAtomRequest};

/// Result type alias for storage operations.
pub type StorageResult<T> = Result<T, AtomicCoreError>;

// ==================== Atom Storage ====================

/// Storage operations for atoms (the fundamental unit of the knowledge base).
#[async_trait]
pub trait AtomStore: Send + Sync {
    /// Get all atoms with their tags.
    async fn get_all_atoms(&self) -> StorageResult<Vec<AtomWithTags>>;

    /// Get a single atom by ID with its tags.
    async fn get_atom(&self, id: &str) -> StorageResult<Option<AtomWithTags>>;

    /// Insert a new atom into the database. Returns the created atom with tags.
    /// Does NOT trigger embedding — that's handled by AtomicCore.
    async fn insert_atom(
        &self,
        id: &str,
        request: &CreateAtomRequest,
        created_at: &str,
    ) -> StorageResult<AtomWithTags>;

    /// Insert multiple atoms in a single transaction. Returns the created atoms.
    async fn insert_atoms_bulk(
        &self,
        atoms: &[(String, CreateAtomRequest, String)], // (id, request, created_at)
    ) -> StorageResult<Vec<AtomWithTags>>;

    /// Update an existing atom. Returns the updated atom with tags.
    async fn update_atom(
        &self,
        id: &str,
        request: &UpdateAtomRequest,
        updated_at: &str,
    ) -> StorageResult<AtomWithTags>;

    /// Delete an atom and all associated data (tags, chunks, embeddings, edges).
    async fn delete_atom(&self, id: &str) -> StorageResult<()>;

    /// Get all atoms with a specific tag (including descendants of that tag).
    async fn get_atoms_by_tag(&self, tag_id: &str) -> StorageResult<Vec<AtomWithTags>>;

    /// List atoms with pagination, filtering, and sorting.
    async fn list_atoms(&self, params: &ListAtomsParams) -> StorageResult<PaginatedAtoms>;

    /// Get all unique sources with atom counts.
    async fn get_source_list(&self) -> StorageResult<Vec<SourceInfo>>;

    /// Get embedding status for a specific atom.
    async fn get_embedding_status(&self, atom_id: &str) -> StorageResult<String>;

    /// Get all atom canvas positions.
    async fn get_atom_positions(&self) -> StorageResult<Vec<AtomPosition>>;

    /// Save atom canvas positions (replaces all).
    async fn save_atom_positions(&self, positions: &[AtomPosition]) -> StorageResult<()>;

    /// Get all atoms with their average embedding vectors.
    async fn get_atoms_with_embeddings(&self) -> StorageResult<Vec<AtomWithEmbedding>>;
}

// ==================== Tag Storage ====================

/// Storage operations for tags (hierarchical organizational units).
#[async_trait]
pub trait TagStore: Send + Sync {
    /// Get all tags with atom counts, organized hierarchically.
    async fn get_all_tags(&self) -> StorageResult<Vec<TagWithCount>>;

    /// Get all tags filtered by minimum atom count.
    async fn get_all_tags_filtered(&self, min_count: i32) -> StorageResult<Vec<TagWithCount>>;

    /// Get children of a tag with pagination.
    async fn get_tag_children(
        &self,
        parent_id: &str,
        min_count: i32,
        limit: i32,
        offset: i32,
    ) -> StorageResult<PaginatedTagChildren>;

    /// Create a new tag.
    async fn create_tag(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> StorageResult<Tag>;

    /// Update a tag's name and/or parent.
    async fn update_tag(
        &self,
        id: &str,
        name: &str,
        parent_id: Option<&str>,
    ) -> StorageResult<Tag>;

    /// Delete a tag. If recursive, also deletes child tags.
    async fn delete_tag(&self, id: &str, recursive: bool) -> StorageResult<()>;

    /// Get tags semantically related to a given tag (via centroid similarity).
    async fn get_related_tags(
        &self,
        tag_id: &str,
        limit: usize,
    ) -> StorageResult<Vec<RelatedTag>>;

    /// Read all tags formatted for compaction LLM input.
    async fn get_tags_for_compaction(&self) -> StorageResult<String>;

    /// Apply tag merge operations (merge source tags into targets).
    async fn apply_tag_merges(
        &self,
        merges: &[TagMerge],
    ) -> StorageResult<CompactionResult>;
}

// ==================== Chunk/Embedding Storage ====================

/// Storage operations for chunks, embeddings, and semantic edges.
#[async_trait]
pub trait ChunkStore: Send + Sync {
    /// Get atoms with pending embedding status (limit batch size).
    async fn get_pending_embeddings(&self, limit: i32) -> StorageResult<Vec<(String, String)>>; // (atom_id, content)

    /// Mark an atom's embedding status (pending, processing, complete, failed).
    async fn set_embedding_status(
        &self,
        atom_id: &str,
        status: &str,
    ) -> StorageResult<()>;

    /// Mark an atom's tagging status.
    async fn set_tagging_status(
        &self,
        atom_id: &str,
        status: &str,
    ) -> StorageResult<()>;

    /// Save chunks and their embeddings for an atom (replaces existing).
    async fn save_chunks_and_embeddings(
        &self,
        atom_id: &str,
        chunks: &[(String, Vec<f32>)], // (chunk_content, embedding)
    ) -> StorageResult<()>;

    /// Delete all chunks and embeddings for an atom.
    async fn delete_chunks(&self, atom_id: &str) -> StorageResult<()>;

    /// Reset atoms stuck in 'processing' status back to 'pending'.
    async fn reset_stuck_processing(&self) -> StorageResult<i32>;

    /// Rebuild semantic edges between all atoms with embeddings.
    async fn rebuild_semantic_edges(&self) -> StorageResult<i32>;

    /// Get semantic edges above a similarity threshold.
    async fn get_semantic_edges(
        &self,
        min_similarity: f32,
    ) -> StorageResult<Vec<SemanticEdge>>;

    /// Get the local neighborhood graph around an atom.
    async fn get_atom_neighborhood(
        &self,
        atom_id: &str,
        depth: i32,
        min_similarity: f32,
    ) -> StorageResult<NeighborhoodGraph>;

    /// Get connection counts for all atoms (tag connections + semantic edges).
    async fn get_connection_counts(
        &self,
        min_similarity: f32,
    ) -> StorageResult<std::collections::HashMap<String, i32>>;

    /// Save tag centroid embedding.
    async fn save_tag_centroid(
        &self,
        tag_id: &str,
        embedding: &[f32],
    ) -> StorageResult<()>;

    /// Recompute all tag centroid embeddings from their atoms' embeddings.
    async fn recompute_all_tag_embeddings(&self) -> StorageResult<i32>;

    /// Check sqlite-vec or equivalent vector extension version.
    async fn check_vector_extension(&self) -> StorageResult<String>;
}

// ==================== Search Storage ====================

/// Storage operations for search (semantic, keyword, hybrid).
#[async_trait]
pub trait SearchStore: Send + Sync {
    /// Perform vector similarity search using embeddings.
    async fn vector_search(
        &self,
        query_embedding: &[f32],
        limit: i32,
        threshold: f32,
        tag_id: Option<&str>,
    ) -> StorageResult<Vec<SemanticSearchResult>>;

    /// Perform keyword search using full-text search.
    async fn keyword_search(
        &self,
        query: &str,
        limit: i32,
        tag_id: Option<&str>,
    ) -> StorageResult<Vec<SemanticSearchResult>>;

    /// Find atoms similar to a given atom.
    async fn find_similar(
        &self,
        atom_id: &str,
        limit: i32,
        threshold: f32,
    ) -> StorageResult<Vec<SimilarAtomResult>>;
}

// ==================== Chat Storage ====================

/// Storage operations for chat conversations and messages.
#[async_trait]
pub trait ChatStore: Send + Sync {
    /// Create a new conversation with optional tag scope.
    async fn create_conversation(
        &self,
        tag_ids: &[String],
        title: Option<&str>,
    ) -> StorageResult<ConversationWithTags>;

    /// List conversations with optional tag filter and pagination.
    async fn get_conversations(
        &self,
        filter_tag_id: Option<&str>,
        limit: i32,
        offset: i32,
    ) -> StorageResult<Vec<ConversationWithTags>>;

    /// Get a conversation with its full message history.
    async fn get_conversation(
        &self,
        conversation_id: &str,
    ) -> StorageResult<Option<ConversationWithMessages>>;

    /// Update conversation metadata.
    async fn update_conversation(
        &self,
        id: &str,
        title: Option<&str>,
        is_archived: Option<bool>,
    ) -> StorageResult<Conversation>;

    /// Delete a conversation and all its messages.
    async fn delete_conversation(&self, id: &str) -> StorageResult<()>;

    /// Set the tag scope for a conversation (replaces existing scope).
    async fn set_conversation_scope(
        &self,
        conversation_id: &str,
        tag_ids: &[String],
    ) -> StorageResult<ConversationWithTags>;

    /// Add a tag to a conversation's scope.
    async fn add_tag_to_scope(
        &self,
        conversation_id: &str,
        tag_id: &str,
    ) -> StorageResult<ConversationWithTags>;

    /// Remove a tag from a conversation's scope.
    async fn remove_tag_from_scope(
        &self,
        conversation_id: &str,
        tag_id: &str,
    ) -> StorageResult<ConversationWithTags>;

    /// Save a chat message (user, assistant, system, or tool).
    async fn save_message(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str,
    ) -> StorageResult<ChatMessage>;

    /// Save tool calls associated with a message.
    async fn save_tool_calls(
        &self,
        message_id: &str,
        tool_calls: &[ChatToolCall],
    ) -> StorageResult<()>;

    /// Save citations for a message.
    async fn save_citations(
        &self,
        message_id: &str,
        citations: &[ChatCitation],
    ) -> StorageResult<()>;
}

// ==================== Wiki Storage ====================

/// Storage operations for wiki articles and their metadata.
#[async_trait]
pub trait WikiStore: Send + Sync {
    /// Get a wiki article with its citations for a tag.
    async fn get_wiki(
        &self,
        tag_id: &str,
    ) -> StorageResult<Option<WikiArticleWithCitations>>;

    /// Get wiki article status (exists, atom count, etc.).
    async fn get_wiki_status(
        &self,
        tag_id: &str,
    ) -> StorageResult<WikiArticleStatus>;

    /// Save or update a wiki article with citations.
    async fn save_wiki(
        &self,
        tag_id: &str,
        content: &str,
        citations: &[WikiCitation],
        atom_count: i32,
    ) -> StorageResult<WikiArticleWithCitations>;

    /// Delete a wiki article and its citations.
    async fn delete_wiki(&self, tag_id: &str) -> StorageResult<()>;

    /// Get cross-reference links from a wiki article to other wiki articles.
    async fn get_wiki_links(
        &self,
        tag_id: &str,
    ) -> StorageResult<Vec<WikiLink>>;

    /// List all versions of a wiki article.
    async fn list_wiki_versions(
        &self,
        tag_id: &str,
    ) -> StorageResult<Vec<WikiVersionSummary>>;

    /// Get a specific wiki article version.
    async fn get_wiki_version(
        &self,
        version_id: &str,
    ) -> StorageResult<Option<WikiArticleVersion>>;

    /// Get all wiki articles (summaries for list view).
    async fn get_all_wiki_articles(&self) -> StorageResult<Vec<WikiArticleSummary>>;

    /// Get tags that would benefit from having wiki articles.
    async fn get_suggested_wiki_articles(
        &self,
        limit: i32,
    ) -> StorageResult<Vec<SuggestedArticle>>;
}

// ==================== Feed Storage ====================

/// Storage operations for RSS/Atom feed subscriptions.
#[async_trait]
pub trait FeedStore: Send + Sync {
    /// Create a new feed subscription.
    async fn create_feed(
        &self,
        url: &str,
        title: Option<&str>,
        site_url: Option<&str>,
        poll_interval: i32,
        tag_ids: &[String],
    ) -> StorageResult<Feed>;

    /// List all feed subscriptions.
    async fn list_feeds(&self) -> StorageResult<Vec<Feed>>;

    /// Get a single feed by ID.
    async fn get_feed(&self, id: &str) -> StorageResult<Feed>;

    /// Update a feed subscription.
    async fn update_feed(
        &self,
        id: &str,
        title: Option<&str>,
        poll_interval: Option<i32>,
        is_paused: Option<bool>,
        tag_ids: Option<&[String]>,
    ) -> StorageResult<Feed>;

    /// Delete a feed subscription.
    async fn delete_feed(&self, id: &str) -> StorageResult<()>;

    /// Get feeds that are due for polling.
    async fn get_due_feeds(&self) -> StorageResult<Vec<Feed>>;

    /// Record that a feed was polled (update timestamp and error).
    async fn mark_feed_polled(
        &self,
        id: &str,
        error: Option<&str>,
    ) -> StorageResult<()>;

    /// Atomically claim a feed item GUID. Returns true if this call claimed it.
    async fn claim_feed_item(
        &self,
        feed_id: &str,
        guid: &str,
    ) -> StorageResult<bool>;

    /// Mark a claimed feed item as successfully ingested with its atom_id.
    async fn complete_feed_item(
        &self,
        feed_id: &str,
        guid: &str,
        atom_id: &str,
    ) -> StorageResult<()>;

    /// Mark a claimed feed item as skipped with a reason.
    async fn mark_feed_item_skipped(
        &self,
        feed_id: &str,
        guid: &str,
        reason: &str,
    ) -> StorageResult<()>;
}

// ==================== Clustering Storage ====================

/// Storage operations for atom clustering.
#[async_trait]
pub trait ClusterStore: Send + Sync {
    /// Compute clusters from atom embeddings.
    async fn compute_clusters(
        &self,
        min_similarity: f32,
        min_cluster_size: i32,
    ) -> StorageResult<Vec<AtomCluster>>;

    /// Save computed clusters (replaces existing).
    async fn save_clusters(&self, clusters: &[AtomCluster]) -> StorageResult<()>;

    /// Get cached clusters (recomputes if stale).
    async fn get_clusters(&self) -> StorageResult<Vec<AtomCluster>>;

    /// Get the hierarchical canvas level for a given parent.
    async fn get_canvas_level(
        &self,
        parent_id: Option<&str>,
        children_hint: Option<Vec<String>>,
    ) -> StorageResult<CanvasLevel>;
}

// ==================== Supertrait ====================

/// Combined storage trait. Every storage backend must implement all sub-traits.
///
/// This is the main trait that `AtomicCore` holds as `Arc<dyn Storage>`.
#[async_trait]
pub trait Storage:
    AtomStore
    + TagStore
    + ChunkStore
    + SearchStore
    + ChatStore
    + WikiStore
    + FeedStore
    + ClusterStore
    + Send
    + Sync
{
    /// Initialize the storage backend (run migrations, create tables, etc.).
    async fn initialize(&self) -> StorageResult<()>;

    /// Graceful shutdown (optimize, flush, etc.).
    async fn shutdown(&self) -> StorageResult<()>;

    /// Get the database/storage path (for display purposes).
    fn storage_path(&self) -> &std::path::Path;
}
