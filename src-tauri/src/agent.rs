use crate::db::Database;
use crate::models::{
    ChatCitation, ChatMessage, ChatMessageWithContext, ChatToolCall, SemanticSearchResult,
};
use chrono::Utc;
use futures::StreamExt;
use reqwest::Client;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

// ==================== OpenRouter API Types ====================

#[derive(Serialize, Debug)]
struct OpenRouterRequest {
    model: String,
    messages: Vec<Message>,
    tools: Vec<Tool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
    temperature: f32,
    max_tokens: u32,
    stream: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Message {
    role: String,
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCallResponse>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Serialize, Debug, Clone)]
struct Tool {
    #[serde(rename = "type")]
    tool_type: String,
    function: FunctionDef,
}

#[derive(Serialize, Debug, Clone)]
struct FunctionDef {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ToolCallResponse {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: FunctionCall,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct FunctionCall {
    name: String,
    arguments: String,
}

// ==================== Streaming Response Types ====================

#[derive(Deserialize, Debug)]
struct StreamingResponse {
    choices: Vec<StreamingChoice>,
}

#[derive(Deserialize, Debug)]
struct StreamingChoice {
    delta: StreamingDelta,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
struct StreamingDelta {
    #[allow(dead_code)]
    role: Option<String>,
    content: Option<String>,
    tool_calls: Option<Vec<StreamingToolCall>>,
}

#[derive(Deserialize, Debug)]
struct StreamingToolCall {
    index: usize,
    id: Option<String>,
    #[serde(rename = "type")]
    call_type: Option<String>,
    function: Option<StreamingFunction>,
}

#[derive(Deserialize, Debug)]
struct StreamingFunction {
    name: Option<String>,
    arguments: Option<String>,
}

// Accumulator for building complete tool calls from streaming deltas
#[derive(Debug, Default, Clone)]
struct ToolCallAccumulator {
    id: String,
    call_type: String,
    name: String,
    arguments: String,
}

// ==================== Event Payloads ====================

#[derive(Serialize, Clone)]
struct ChatStreamDelta {
    conversation_id: String,
    content: String,
}

#[derive(Serialize, Clone)]
struct ChatToolStart {
    conversation_id: String,
    tool_call_id: String,
    tool_name: String,
    tool_input: serde_json::Value,
}

#[derive(Serialize, Clone)]
struct ChatToolComplete {
    conversation_id: String,
    tool_call_id: String,
    results_count: i32,
}

#[derive(Serialize, Clone)]
struct ChatComplete {
    conversation_id: String,
    message: ChatMessageWithContext,
}

// ==================== Tool Definitions ====================

fn get_tools() -> Vec<Tool> {
    vec![
        Tool {
            tool_type: "function".to_string(),
            function: FunctionDef {
                name: "search_atoms".to_string(),
                description: "Search for relevant atoms using semantic similarity. Use this to find information related to a specific topic or question.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The search query to find relevant atoms"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of results to return (default: 5)",
                            "default": 5
                        }
                    },
                    "required": ["query"]
                }),
            },
        },
        Tool {
            tool_type: "function".to_string(),
            function: FunctionDef {
                name: "get_atom".to_string(),
                description: "Get the full content of a specific atom by its ID. Use this when you need more detail about an atom returned from search.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "atom_id": {
                            "type": "string",
                            "description": "The ID of the atom to retrieve"
                        }
                    },
                    "required": ["atom_id"]
                }),
            },
        },
    ]
}

// ==================== Tool Execution ====================

async fn execute_search_atoms(
    db: &Database,
    query: &str,
    limit: i32,
    scope_tag_ids: &[String],
) -> Result<Vec<SemanticSearchResult>, String> {
    crate::commands::search_atoms_semantic_impl(db, query, limit, 0.3, scope_tag_ids).await
}

fn execute_get_atom(db: &Database, atom_id: &str) -> Result<Option<String>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT content FROM atoms WHERE id = ?1",
        [atom_id],
        |row| row.get::<_, String>(0),
    )
    .map(Some)
    .or_else(|e| {
        if matches!(e, rusqlite::Error::QueryReturnedNoRows) {
            Ok(None)
        } else {
            Err(format!("Failed to get atom: {}", e))
        }
    })
}

// ==================== Streaming Response Processing ====================

/// Result of processing a streaming response
struct StreamingResult {
    content: String,
    tool_calls: Option<Vec<ToolCallResponse>>,
}

/// Process SSE stream from OpenRouter and emit deltas
async fn process_streaming_response(
    response: reqwest::Response,
    app_handle: &AppHandle,
    conversation_id: &str,
) -> Result<StreamingResult, String> {
    let mut content = String::new();
    let mut tool_call_accumulators: Vec<ToolCallAccumulator> = Vec::new();
    let mut buffer = String::new();

    let mut stream = response.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("Stream error: {}", e))?;
        let chunk_str = String::from_utf8_lossy(&chunk);
        buffer.push_str(&chunk_str);

        // Process complete lines from buffer
        while let Some(line_end) = buffer.find('\n') {
            let line = buffer[..line_end].trim().to_string();
            buffer = buffer[line_end + 1..].to_string();

            // Skip empty lines
            if line.is_empty() {
                continue;
            }

            // Check for stream end
            if line == "data: [DONE]" {
                break;
            }

            // Parse SSE data line
            if let Some(json_str) = line.strip_prefix("data: ") {
                match serde_json::from_str::<StreamingResponse>(json_str) {
                    Ok(response) => {
                        if let Some(choice) = response.choices.first() {
                            // Handle content delta
                            if let Some(delta_content) = &choice.delta.content {
                                content.push_str(delta_content);
                                // Emit full accumulated content to frontend
                                // (more robust than deltas - avoids duplication from race conditions)
                                let _ = app_handle.emit(
                                    "chat-stream-delta",
                                    ChatStreamDelta {
                                        conversation_id: conversation_id.to_string(),
                                        content: content.clone(),
                                    },
                                );
                            }

                            // Handle tool call deltas
                            if let Some(tool_calls) = &choice.delta.tool_calls {
                                for tc in tool_calls {
                                    // Ensure accumulator exists for this index
                                    while tool_call_accumulators.len() <= tc.index {
                                        tool_call_accumulators.push(ToolCallAccumulator::default());
                                    }

                                    let acc = &mut tool_call_accumulators[tc.index];

                                    // Accumulate fields
                                    if let Some(id) = &tc.id {
                                        acc.id = id.clone();
                                    }
                                    if let Some(call_type) = &tc.call_type {
                                        acc.call_type = call_type.clone();
                                    }
                                    if let Some(func) = &tc.function {
                                        if let Some(name) = &func.name {
                                            acc.name = name.clone();
                                        }
                                        if let Some(args) = &func.arguments {
                                            acc.arguments.push_str(args);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        // Log parse errors but continue (some lines may be malformed)
                        eprintln!("Failed to parse streaming response: {} - {}", e, json_str);
                    }
                }
            }
        }
    }

    // Convert accumulators to ToolCallResponse
    let tool_calls = if tool_call_accumulators.is_empty() {
        None
    } else {
        Some(
            tool_call_accumulators
                .into_iter()
                .map(|acc| ToolCallResponse {
                    id: acc.id,
                    call_type: acc.call_type,
                    function: FunctionCall {
                        name: acc.name,
                        arguments: acc.arguments,
                    },
                })
                .collect(),
        )
    };

    Ok(StreamingResult { content, tool_calls })
}

// ==================== System Prompt ====================

fn get_system_prompt(scope_description: &str) -> String {
    format!(
        r#"You are a helpful AI assistant with access to the user's personal knowledge base. Your role is to answer questions by searching through and referencing the user's stored information.

{}

Guidelines:
- ALWAYS use the search_atoms tool first to find relevant information before answering
- If the initial search doesn't find enough, try different search queries
- When you find relevant information, cite it using [N] notation where N is a sequential number
- Be honest if you cannot find information - do not make things up
- Keep responses concise but informative
- If the user asks about something not in their knowledge base, say so

When citing sources:
- Use [1], [2], etc. for each unique source
- Place citations immediately after the relevant claim
- You can cite the same source multiple times if needed"#,
        scope_description
    )
}

// ==================== Agent Loop ====================

struct AgentContext {
    conversation_id: String,
    scope_tag_ids: Vec<String>,
    messages: Vec<Message>,
    citations: Vec<(String, i32, String)>, // (atom_id, chunk_index, excerpt)
    tool_calls_record: Vec<ChatToolCall>,
}

async fn run_agent_loop(
    app_handle: AppHandle,
    db: Arc<Database>,
    api_key: String,
    model: String,
    mut ctx: AgentContext,
) -> Result<ChatMessageWithContext, String> {
    let client = Client::new();
    let tools = get_tools();
    let max_iterations = 10;

    for iteration in 0..max_iterations {
        // Build request - use streaming for final answers, non-streaming for tool calls iteration
        // We always use streaming now to get token-by-token output
        let request = OpenRouterRequest {
            model: model.clone(),
            messages: ctx.messages.clone(),
            tools: tools.clone(),
            tool_choice: if iteration == max_iterations - 1 {
                Some("none".to_string()) // Force final answer on last iteration
            } else {
                None
            },
            temperature: 0.7,
            max_tokens: 4000,
            stream: true, // Enable streaming for real-time token output
        };

        // Call OpenRouter
        let response = client
            .post("https://openrouter.ai/api/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("API request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("API error {}: {}", status, text));
        }

        // Process the streaming response
        let streaming_result = process_streaming_response(
            response,
            &app_handle,
            &ctx.conversation_id,
        )
        .await?;

        // Check if there are tool calls
        if let Some(tool_calls) = &streaming_result.tool_calls {
            // Add assistant message with tool calls to history
            let assistant_content = if streaming_result.content.is_empty() {
                None
            } else {
                Some(streaming_result.content.clone())
            };
            ctx.messages.push(Message {
                role: "assistant".to_string(),
                content: assistant_content,
                tool_calls: Some(tool_calls.clone()),
                tool_call_id: None,
                name: None,
            });

            // Execute each tool call
            for tool_call in tool_calls {
                let tool_name = &tool_call.function.name;
                let tool_args: serde_json::Value =
                    serde_json::from_str(&tool_call.function.arguments)
                        .unwrap_or(serde_json::Value::Null);

                // Emit tool start event
                let _ = app_handle.emit(
                    "chat-tool-start",
                    ChatToolStart {
                        conversation_id: ctx.conversation_id.clone(),
                        tool_call_id: tool_call.id.clone(),
                        tool_name: tool_name.clone(),
                        tool_input: tool_args.clone(),
                    },
                );

                // Execute tool
                let (tool_result, results_count) = match tool_name.as_str() {
                    "search_atoms" => {
                        let query = tool_args["query"].as_str().unwrap_or("");
                        let limit = tool_args["limit"].as_i64().unwrap_or(5) as i32;
                        match execute_search_atoms(&db, query, limit, &ctx.scope_tag_ids).await {
                            Ok(results) => {
                                let count = results.len() as i32;
                                // Store citation info
                                for result in results.iter() {
                                    ctx.citations.push((
                                        result.atom.atom.id.clone(),
                                        result.matching_chunk_index,
                                        result.matching_chunk_content.chars().take(200).collect(),
                                    ));
                                }
                                let result_text = results
                                    .iter()
                                    .enumerate()
                                    .map(|(i, r)| {
                                        format!(
                                            "[{}] (atom_id: {}, similarity: {:.2})\n{}",
                                            ctx.citations.len() - results.len() + i + 1,
                                            r.atom.atom.id,
                                            r.similarity_score,
                                            r.matching_chunk_content
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n\n");
                                (result_text, count)
                            }
                            Err(e) => (format!("Error: {}", e), 0),
                        }
                    }
                    "get_atom" => {
                        let atom_id = tool_args["atom_id"].as_str().unwrap_or("");
                        match execute_get_atom(&db, atom_id) {
                            Ok(Some(content)) => (content, 1),
                            Ok(None) => ("Atom not found".to_string(), 0),
                            Err(e) => (format!("Error: {}", e), 0),
                        }
                    }
                    _ => (format!("Unknown tool: {}", tool_name), 0),
                };

                // Record tool call
                ctx.tool_calls_record.push(ChatToolCall {
                    id: tool_call.id.clone(),
                    message_id: String::new(), // Will be set when saving
                    tool_name: tool_name.clone(),
                    tool_input: tool_args,
                    tool_output: Some(serde_json::Value::String(tool_result.clone())),
                    status: "complete".to_string(),
                    created_at: Utc::now().to_rfc3339(),
                    completed_at: Some(Utc::now().to_rfc3339()),
                });

                // Emit tool complete event
                let _ = app_handle.emit(
                    "chat-tool-complete",
                    ChatToolComplete {
                        conversation_id: ctx.conversation_id.clone(),
                        tool_call_id: tool_call.id.clone(),
                        results_count,
                    },
                );

                // Add tool result to messages
                ctx.messages.push(Message {
                    role: "tool".to_string(),
                    content: Some(tool_result),
                    tool_calls: None,
                    tool_call_id: Some(tool_call.id.clone()),
                    name: Some(tool_name.clone()),
                });
            }
        } else {
            // No tool calls - we have the final answer
            // Content was already streamed to frontend via chat-stream-delta events
            let content = streaming_result.content;

            // Build citations from collected data
            let citations: Vec<ChatCitation> = ctx
                .citations
                .iter()
                .enumerate()
                .map(|(i, (atom_id, chunk_index, excerpt))| ChatCitation {
                    id: Uuid::new_v4().to_string(),
                    message_id: String::new(), // Will be set when saving
                    citation_index: (i + 1) as i32,
                    atom_id: atom_id.clone(),
                    chunk_index: Some(*chunk_index),
                    excerpt: excerpt.clone(),
                    relevance_score: None,
                })
                .collect();

            return Ok(ChatMessageWithContext {
                message: ChatMessage {
                    id: Uuid::new_v4().to_string(),
                    conversation_id: ctx.conversation_id.clone(),
                    role: "assistant".to_string(),
                    content,
                    created_at: Utc::now().to_rfc3339(),
                    message_index: 0, // Will be set when saving
                },
                tool_calls: ctx.tool_calls_record,
                citations,
            });
        }
    }

    Err("Max iterations reached without completing".to_string())
}

// ==================== Database Operations ====================

fn save_message(
    conn: &Connection,
    conversation_id: &str,
    role: &str,
    content: &str,
) -> Result<(String, i32), String> {
    let message_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    // Get next message index
    let message_index: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(message_index), -1) + 1 FROM chat_messages WHERE conversation_id = ?1",
            [conversation_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to get message index: {}", e))?;

    conn.execute(
        "INSERT INTO chat_messages (id, conversation_id, role, content, created_at, message_index)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![&message_id, conversation_id, role, content, &now, message_index],
    )
    .map_err(|e| format!("Failed to save message: {}", e))?;

    // Update conversation timestamp
    conn.execute(
        "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
        rusqlite::params![&now, conversation_id],
    )
    .map_err(|e| format!("Failed to update conversation: {}", e))?;

    Ok((message_id, message_index))
}

fn save_tool_calls(
    conn: &Connection,
    message_id: &str,
    tool_calls: &[ChatToolCall],
) -> Result<(), String> {
    for tool_call in tool_calls {
        conn.execute(
            "INSERT INTO chat_tool_calls (id, message_id, tool_name, tool_input, tool_output, status, created_at, completed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                &tool_call.id,
                message_id,
                &tool_call.tool_name,
                serde_json::to_string(&tool_call.tool_input).unwrap_or_default(),
                tool_call.tool_output.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default()),
                &tool_call.status,
                &tool_call.created_at,
                &tool_call.completed_at,
            ],
        )
        .map_err(|e| format!("Failed to save tool call: {}", e))?;
    }
    Ok(())
}

fn save_citations(
    conn: &Connection,
    message_id: &str,
    citations: &[ChatCitation],
) -> Result<(), String> {
    for citation in citations {
        conn.execute(
            "INSERT INTO chat_citations (id, message_id, citation_index, atom_id, chunk_index, excerpt, relevance_score)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                &citation.id,
                message_id,
                citation.citation_index,
                &citation.atom_id,
                citation.chunk_index,
                &citation.excerpt,
                citation.relevance_score,
            ],
        )
        .map_err(|e| format!("Failed to save citation: {}", e))?;
    }
    Ok(())
}

fn get_conversation_messages(conn: &Connection, conversation_id: &str) -> Result<Vec<Message>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT role, content FROM chat_messages WHERE conversation_id = ?1 ORDER BY message_index",
        )
        .map_err(|e| format!("Failed to prepare query: {}", e))?;

    let messages = stmt
        .query_map([conversation_id], |row| {
            Ok(Message {
                role: row.get(0)?,
                content: row.get(1)?,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            })
        })
        .map_err(|e| format!("Failed to query messages: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect messages: {}", e))?;

    Ok(messages)
}

fn get_scope_tag_ids(conn: &Connection, conversation_id: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT tag_id FROM conversation_tags WHERE conversation_id = ?1")
        .map_err(|e| format!("Failed to prepare query: {}", e))?;

    let tag_ids = stmt
        .query_map([conversation_id], |row| row.get(0))
        .map_err(|e| format!("Failed to query tags: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect tags: {}", e))?;

    Ok(tag_ids)
}

fn get_scope_description(conn: &Connection, tag_ids: &[String]) -> String {
    if tag_ids.is_empty() {
        return "You have access to ALL atoms in the knowledge base.".to_string();
    }

    // Get tag names
    let placeholders: Vec<String> = (0..tag_ids.len()).map(|i| format!("?{}", i + 1)).collect();
    let query = format!(
        "SELECT name FROM tags WHERE id IN ({})",
        placeholders.join(", ")
    );

    let mut stmt = match conn.prepare(&query) {
        Ok(s) => s,
        Err(_) => return "You have access to a scoped set of atoms.".to_string(),
    };

    let params: Vec<&dyn rusqlite::ToSql> = tag_ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    let names: Vec<String> = stmt
        .query_map(params.as_slice(), |row| row.get(0))
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();

    if names.is_empty() {
        "You have access to a scoped set of atoms.".to_string()
    } else {
        format!(
            "You have access to atoms tagged with: {}. Focus your search on these topics.",
            names.join(", ")
        )
    }
}

// ==================== Tauri Command ====================

#[tauri::command]
pub async fn send_chat_message(
    app_handle: AppHandle,
    db: State<'_, Database>,
    conversation_id: String,
    content: String,
) -> Result<ChatMessageWithContext, String> {
    // Get API key and model from settings
    let (api_key, model) = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let api_key: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'openrouter_api_key'",
                [],
                |row| row.get(0),
            )
            .map_err(|_| "OpenRouter API key not configured. Please set it in Settings.")?;

        let model: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'chat_model'",
                [],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| "anthropic/claude-sonnet-4".to_string());

        (api_key, model)
    };

    // Save user message
    {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        save_message(&conn, &conversation_id, "user", &content)?;
    }

    // Get conversation context
    let (messages, scope_tag_ids, scope_description) = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let messages = get_conversation_messages(&conn, &conversation_id)?;
        let scope_tag_ids = get_scope_tag_ids(&conn, &conversation_id)?;
        let scope_description = get_scope_description(&conn, &scope_tag_ids);
        (messages, scope_tag_ids, scope_description)
    };

    // Build message history for API
    let mut api_messages = vec![Message {
        role: "system".to_string(),
        content: Some(get_system_prompt(&scope_description)),
        tool_calls: None,
        tool_call_id: None,
        name: None,
    }];
    api_messages.extend(messages);

    // Create agent context
    let ctx = AgentContext {
        conversation_id: conversation_id.clone(),
        scope_tag_ids,
        messages: api_messages,
        citations: Vec::new(),
        tool_calls_record: Vec::new(),
    };

    // Create a new database reference for the async agent loop
    let db_arc = Arc::new(Database {
        conn: std::sync::Mutex::new(db.new_connection().map_err(|e| e.to_string())?),
        db_path: db.db_path.clone(),
        resource_dir: db.resource_dir.clone(),
    });

    // Run agent loop
    let mut result = run_agent_loop(app_handle.clone(), db_arc, api_key, model, ctx).await?;

    // Save assistant message
    {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let (msg_id, msg_idx) = save_message(&conn, &conversation_id, "assistant", &result.message.content)?;

        // Update message with correct id and index
        result.message.id = msg_id.clone();
        result.message.message_index = msg_idx;

        // Save tool calls with correct message_id
        for tool_call in &mut result.tool_calls {
            tool_call.message_id = msg_id.clone();
        }
        save_tool_calls(&conn, &msg_id, &result.tool_calls)?;

        // Save citations with correct message_id
        for citation in &mut result.citations {
            citation.message_id = msg_id.clone();
        }
        save_citations(&conn, &msg_id, &result.citations)?;
    }

    // Emit completion event
    let _ = app_handle.emit(
        "chat-complete",
        ChatComplete {
            conversation_id: conversation_id.clone(),
            message: result.clone(),
        },
    );

    Ok(result)
}
