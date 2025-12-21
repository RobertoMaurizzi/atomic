//! MCP Server implementation for Atomic

use crate::db::Database;
use crate::types::*;
use rmcp::{
    handler::server::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ErrorData, ServerHandler,
};
use std::sync::Arc;
use uuid::Uuid;
use chrono::Utc;

/// MCP Server for Atomic knowledge base
#[derive(Clone)]
pub struct AtomicMcpServer {
    db: Arc<Database>,
    tool_router: ToolRouter<Self>,
}

impl AtomicMcpServer {
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            db,
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

        let conn = self
            .db
            .new_connection()
            .map_err(|e| ErrorData::internal_error(format!("{}", e), None))?;

        // Escape query for FTS5
        let escaped_query: String = params.query
            .split_whitespace()
            .map(|word| {
                let cleaned = word.replace('"', "");
                if cleaned.is_empty() {
                    String::new()
                } else {
                    format!("\"{}\"", cleaned)
                }
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");

        if escaped_query.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text("[]")]));
        }

        // Keyword search using FTS5
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT ac.atom_id, a.content, ac.content as chunk_content
                 FROM atom_chunks_fts fts
                 JOIN atom_chunks ac ON fts.chunk_id = ac.id
                 JOIN atoms a ON ac.atom_id = a.id
                 WHERE atom_chunks_fts MATCH ?1
                 ORDER BY bm25(atom_chunks_fts)
                 LIMIT ?2",
            )
            .map_err(|e| ErrorData::internal_error(format!("Query error: {}", e), None))?;

        let results: Vec<SearchResult> = stmt
            .query_map(rusqlite::params![&escaped_query, limit], |row| {
                let atom_id: String = row.get(0)?;
                let content: String = row.get(1)?;
                let chunk_content: String = row.get(2)?;
                Ok(SearchResult {
                    atom_id,
                    content_preview: content.chars().take(200).collect(),
                    similarity_score: 0.8, // FTS doesn't give similarity, use placeholder
                    matching_chunk: Some(chunk_content.chars().take(300).collect()),
                })
            })
            .map_err(|e| ErrorData::internal_error(format!("Query error: {}", e), None))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ErrorData::internal_error(format!("Result error: {}", e), None))?;

        let response_text = serde_json::to_string_pretty(&results)
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

        let conn = self
            .db
            .new_connection()
            .map_err(|e| ErrorData::internal_error(format!("{}", e), None))?;

        let atom_result: Result<(String, String, String, String), rusqlite::Error> = conn
            .query_row(
                "SELECT id, content, created_at, updated_at FROM atoms WHERE id = ?1",
                [&params.atom_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            );

        match atom_result {
            Ok((id, content, created_at, updated_at)) => {
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
                    atom_id: id,
                    content: paginated_content,
                    total_lines,
                    returned_lines,
                    offset: offset as i32,
                    has_more,
                    created_at,
                    updated_at,
                };

                let response_text = serde_json::to_string_pretty(&response)
                    .map_err(|e| ErrorData::internal_error(format!("Serialization error: {}", e), None))?;

                Ok(CallToolResult::success(vec![Content::text(response_text)]))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(CallToolResult::success(vec![
                Content::text(format!("Atom not found: {}", params.atom_id)),
            ])),
            Err(e) => Err(ErrorData::internal_error(e.to_string(), None)),
        }
    }

    /// Create a new atom with markdown content
    #[tool(
        description = "Create a new atom with markdown content. The atom will be processed for embeddings when the Atomic app runs. Returns the created atom ID."
    )]
    async fn create_atom(
        &self,
        Parameters(params): Parameters<CreateAtomParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let conn = self
            .db
            .new_connection()
            .map_err(|e| ErrorData::internal_error(format!("{}", e), None))?;

        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        // Insert atom with pending embedding status
        conn.execute(
            "INSERT INTO atoms (id, content, source_url, created_at, updated_at, embedding_status, tagging_status)
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending', 'pending')",
            rusqlite::params![&id, &params.content, &params.source_url, &now, &now],
        )
        .map_err(|e| ErrorData::internal_error(format!("Failed to create atom: {}", e), None))?;

        // Add tags if provided
        if let Some(tag_ids) = &params.tag_ids {
            for tag_id in tag_ids {
                conn.execute(
                    "INSERT OR IGNORE INTO atom_tags (atom_id, tag_id) VALUES (?1, ?2)",
                    rusqlite::params![&id, tag_id],
                )
                .ok(); // Ignore errors for invalid tag IDs
            }
        }

        let response = CreatedAtom {
            atom_id: id,
            content_preview: params.content.chars().take(200).collect(),
            embedding_status: "pending".to_string(),
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
