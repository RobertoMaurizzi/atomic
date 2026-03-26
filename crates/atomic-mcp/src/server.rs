//! MCP Server implementation for Atomic

use crate::types::*;
use atomic_core::{AtomicCore, CreateAtomRequest, SearchMode, SearchOptions};
use rmcp::{
    handler::server::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ErrorData, ServerHandler,
};

/// MCP Server for Atomic knowledge base
#[derive(Clone)]
pub struct AtomicMcpServer {
    core: AtomicCore,
    tool_router: ToolRouter<Self>,
}

impl AtomicMcpServer {
    pub fn new(core: AtomicCore) -> Self {
        Self {
            core,
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl AtomicMcpServer {
    /// Search for atoms using keyword search (BM25)
    #[tool(
        description = "Search for atoms using keyword search. Returns atoms with content matching the query, ranked by relevance. Use this to find information in the knowledge base."
    )]
    async fn search(
        &self,
        Parameters(params): Parameters<SemanticSearchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let limit = params.limit.unwrap_or(10).min(50);

        let options = SearchOptions::new(
            params.query,
            SearchMode::Keyword,
            limit,
        );

        let results = self
            .core
            .search(options)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let search_results: Vec<SearchResult> = results
            .into_iter()
            .map(|r| SearchResult {
                atom_id: r.atom.atom.id.clone(),
                content_preview: r.atom.atom.content.chars().take(200).collect(),
                similarity_score: r.similarity_score,
                matching_chunk: Some(r.matching_chunk_content),
            })
            .collect();

        let response_text = serde_json::to_string_pretty(&search_results)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(response_text)]))
    }

    /// Read a single atom with optional line-based pagination
    #[tool(
        description = "Read the full content of a specific atom by its ID. Supports line-based pagination for large atoms. Returns the atom content and metadata."
    )]
    async fn read_atom(
        &self,
        Parameters(params): Parameters<ReadAtomParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let limit = params.limit.unwrap_or(100).min(500) as usize;
        let offset = params.offset.unwrap_or(0).max(0) as usize;

        let atom_with_tags = match self.core.get_atom(&params.atom_id) {
            Ok(Some(a)) => a,
            Ok(None) => {
                return Ok(CallToolResult::success(vec![
                    Content::text(format!("Atom not found: {}", params.atom_id)),
                ]));
            }
            Err(e) => return Err(ErrorData::internal_error(e.to_string(), None)),
        };

        let content = &atom_with_tags.atom.content;
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len() as i32;
        let start = offset.min(lines.len());
        let end = (start + limit).min(lines.len());
        let paginated_lines = &lines[start..end];
        let returned_lines = paginated_lines.len() as i32;
        let has_more = end < lines.len();

        let mut paginated_content = paginated_lines.join("\n");

        if has_more {
            paginated_content.push_str(&format!(
                "\n\n(Content continues. Use offset {} to read more lines.)",
                end
            ));
        }

        let response = AtomContent {
            atom_id: atom_with_tags.atom.id,
            content: paginated_content,
            total_lines,
            returned_lines,
            offset: offset as i32,
            has_more,
            created_at: atom_with_tags.atom.created_at,
            updated_at: atom_with_tags.atom.updated_at,
        };

        let response_text = serde_json::to_string_pretty(&response)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(response_text)]))
    }

    /// Create a new atom with markdown content
    #[tool(
        description = "Create a new atom with markdown content. The atom will be processed for embeddings when the Atomic app runs. Returns the created atom ID."
    )]
    async fn create_atom(
        &self,
        Parameters(params): Parameters<CreateAtomParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let request = CreateAtomRequest {
            content: params.content.clone(),
            source_url: params.source_url,
            published_at: None,
            tag_ids: params.tag_ids.unwrap_or_default(),
        };

        // Use a no-op callback since standalone MCP server doesn't have event broadcasting
        let result = self
            .core
            .create_atom(request, |_| {})
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let response = CreatedAtom {
            atom_id: result.atom.id.clone(),
            content_preview: result.atom.content.chars().take(200).collect(),
            embedding_status: result.atom.embedding_status.clone(),
        };

        let response_text = serde_json::to_string_pretty(&response)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(response_text)]))
    }
}

#[tool_handler]
impl ServerHandler for AtomicMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Atomic is a personal knowledge base. \
                 Use search to find relevant information by keywords, \
                 read_atom to get full content of a specific atom, \
                 and create_atom to add new notes. \
                 Note: New atoms will be processed for semantic search when the Atomic app runs."
                    .to_string(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
