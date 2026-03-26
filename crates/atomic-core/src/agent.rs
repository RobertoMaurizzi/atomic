//! Chat agent loop with tool calling and streaming
//!
//! Provides the agentic chat loop that searches the knowledge base,
//! retrieves atoms, and generates responses with citations.
//! Uses a callback-based event system (same pattern as EmbeddingEvent).

use crate::models::{ChatCitation, ChatMessage, ChatMessageWithContext, ChatToolCall, SemanticSearchResult};
use crate::chunking::count_tokens;
use crate::providers::traits::LlmConfig;
use crate::providers::types::{GenerationParams, Message, MessageRole, StreamDelta, ToolDefinition};
use crate::providers::{create_streaming_llm_provider, ProviderConfig, ProviderType};
use crate::search::{SearchMode, SearchOptions};
use crate::storage::StorageBackend;
use chrono::Utc;
use serde::Serialize;
use serde_json::json;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

// ==================== Chat Events ====================

/// Events emitted during the chat agent loop.
/// Consumers (Tauri, HTTP server) bridge these to their own event systems.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ChatEvent {
    /// Streaming content delta (accumulated)
    StreamDelta {
        conversation_id: String,
        content: String,
    },
    /// Tool execution started
    ToolStart {
        conversation_id: String,
        tool_call_id: String,
        tool_name: String,
        tool_input: serde_json::Value,
    },
    /// Tool execution completed
    ToolComplete {
        conversation_id: String,
        tool_call_id: String,
        results_count: i32,
    },
    /// Full message completed
    Complete {
        conversation_id: String,
        message: ChatMessageWithContext,
    },
    /// Error during chat
    Error {
        conversation_id: String,
        error: String,
    },
}

// ==================== Tool Definitions ====================

fn get_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition::new(
            "search_atoms",
            "Search for relevant atoms using hybrid keyword and semantic search. Use this to find information related to a specific topic or question.",
            json!({
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
        ),
        ToolDefinition::new(
            "get_atom",
            "Get the full content of a specific atom by its ID. Use this when you need more detail about an atom returned from search.",
            json!({
                "type": "object",
                "properties": {
                    "atom_id": {
                        "type": "string",
                        "description": "The ID of the atom to retrieve"
                    }
                },
                "required": ["atom_id"]
            }),
        ),
    ]
}

// ==================== Tool Execution ====================

async fn execute_search_atoms(
    storage: &StorageBackend,
    query: &str,
    limit: i32,
    scope_tag_ids: &[String],
    external_settings: Option<std::collections::HashMap<String, String>>,
) -> Result<Vec<SemanticSearchResult>, String> {
    // Try SQLite path first (uses full search module with settings resolution)
    if let Some(sqlite) = storage.as_sqlite() {
        let options = SearchOptions::new(query, SearchMode::Hybrid, limit)
            .with_threshold(0.3)
            .with_scope(scope_tag_ids.to_vec());
        return crate::search::search_atoms_with_settings(&sqlite.db, options, external_settings).await;
    }

    // Postgres path: use storage dispatch methods
    let settings = match external_settings {
        Some(s) => s,
        None => storage.get_all_settings_sync().map_err(|e| e.to_string())?,
    };
    let config = ProviderConfig::from_settings(&settings);
    let tag_id = scope_tag_ids.first().map(|s| s.as_str());

    // Generate query embedding
    let provider = crate::providers::get_embedding_provider(&config)
        .map_err(|e| e.to_string())?;
    let embed_config = crate::providers::EmbeddingConfig::new(config.embedding_model());
    let embeddings = provider.embed_batch(&[query.to_string()], &embed_config)
        .await.map_err(|e| e.to_string())?;

    let keyword = storage.keyword_search_sync(query, limit * 2, tag_id)
        .map_err(|e| e.to_string())?;
    let semantic = if !embeddings.is_empty() && !embeddings[0].is_empty() {
        storage.vector_search_sync(&embeddings[0], limit * 2, 0.3, tag_id)
            .map_err(|e| e.to_string())?
    } else { vec![] };

    Ok(crate::search::merge_search_results_rrf(semantic, keyword, limit))
}

fn execute_get_atom(storage: &StorageBackend, atom_id: &str) -> Result<Option<String>, String> {
    storage.get_atom_content_impl(atom_id).map_err(|e| e.to_string())
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

// ==================== Context Window Management ====================

/// Estimate token count for a message, including tool call content.
fn estimate_message_tokens(m: &Message) -> usize {
    let content_tokens = count_tokens(m.content.as_deref().unwrap_or(""));
    let tool_call_tokens = m.tool_calls.as_ref().map_or(0, |tcs| {
        tcs.iter()
            .map(|tc| {
                let args = tc.get_arguments().unwrap_or("");
                let name = tc.get_name().unwrap_or("");
                count_tokens(name) + count_tokens(args) + 10
            })
            .sum()
    });
    content_tokens + tool_call_tokens
}

/// A group of messages that must be kept together for API validity.
/// Either a single user/system message, or an assistant message followed
/// by its tool-result messages.
struct MessageGroup {
    start: usize,
    end: usize, // exclusive
    tokens: usize,
}

/// Group messages into atomic units that can't be split.
/// An assistant message with tool_calls and its subsequent tool-result messages
/// form one group. Everything else is its own group.
fn group_messages(messages: &[Message], message_tokens: &[usize]) -> Vec<MessageGroup> {
    let mut groups = Vec::new();
    let mut i = 0;
    while i < messages.len() {
        if messages[i].role == MessageRole::Assistant && messages[i].tool_calls.is_some() {
            // Start of a tool round: assistant + following tool results
            let start = i;
            let mut tokens = message_tokens[i];
            i += 1;
            while i < messages.len() && messages[i].role == MessageRole::Tool {
                tokens += message_tokens[i];
                i += 1;
            }
            groups.push(MessageGroup { start, end: i, tokens });
        } else {
            groups.push(MessageGroup {
                start: i,
                end: i + 1,
                tokens: message_tokens[i],
            });
            i += 1;
        }
    }
    groups
}

/// Truncate message history to fit within the provider's context window.
/// Keeps the system prompt (first group) and the most recent group,
/// then includes as many recent groups as fit in the remaining budget.
/// Never splits assistant+tool-result pairs to maintain API validity.
/// Reserves ~30% of context for the assistant's response and tool results.
fn truncate_messages_to_context(messages: Vec<Message>, context_length: Option<usize>) -> Vec<Message> {
    let max_tokens = match context_length {
        Some(ctx_len) => (ctx_len as f64 * 0.7) as usize,
        None => return messages,
    };

    if messages.len() <= 2 {
        return messages;
    }

    let message_tokens: Vec<usize> = messages.iter().map(estimate_message_tokens).collect();
    let total: usize = message_tokens.iter().sum();
    if total <= max_tokens {
        return messages;
    }

    let groups = group_messages(&messages, &message_tokens);
    if groups.len() <= 2 {
        return messages; // System + one group, nothing safe to drop
    }

    // Always keep first group (system) and last group (most recent)
    let first_tokens = groups[0].tokens;
    let last_tokens = groups[groups.len() - 1].tokens;
    let mut budget = max_tokens.saturating_sub(first_tokens + last_tokens);

    // Work backwards through middle groups, keeping as many as fit
    let mut keep_from_group = groups.len() - 1;
    for gi in (1..groups.len() - 1).rev() {
        if groups[gi].tokens > budget {
            break;
        }
        budget -= groups[gi].tokens;
        keep_from_group = gi;
    }

    // Build result from kept groups
    let mut result: Vec<Message> = messages[groups[0].start..groups[0].end].to_vec();
    for g in &groups[keep_from_group..] {
        result.extend(messages[g.start..g.end].to_vec());
    }

    eprintln!(
        "[chat] Truncated message history from {} to {} messages ({} groups kept) to fit context window ({} tokens)",
        messages.len(),
        result.len(),
        groups.len() - keep_from_group + 1,
        max_tokens
    );

    result
}

// ==================== Helper: Convert stored messages to provider format ====================

/// Convert ChatMessage models from storage into provider Message format for the API.
fn chat_messages_to_provider_messages(messages: Vec<crate::models::ChatMessageWithContext>) -> Vec<Message> {
    messages
        .into_iter()
        .map(|m| {
            let role = match m.message.role.as_str() {
                "system" => MessageRole::System,
                "assistant" => MessageRole::Assistant,
                "tool" => MessageRole::Tool,
                _ => MessageRole::User,
            };
            Message {
                role,
                content: Some(m.message.content),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }
        })
        .collect()
}

// ==================== Agent Loop ====================

struct AgentContext {
    conversation_id: String,
    scope_tag_ids: Vec<String>,
    messages: Vec<Message>,
    citations: Vec<(String, i32, String)>, // (atom_id, chunk_index, excerpt)
    tool_calls_record: Vec<ChatToolCall>,
}

async fn run_agent_loop<F>(
    on_event: &F,
    storage: StorageBackend,
    provider_config: ProviderConfig,
    model: String,
    mut ctx: AgentContext,
    external_settings: Option<std::collections::HashMap<String, String>>,
) -> Result<ChatMessageWithContext, String>
where
    F: Fn(ChatEvent) + Send + Sync,
{
    let provider = create_streaming_llm_provider(&provider_config)
        .map_err(|e| format!("Failed to create streaming provider: {}", e))?;
    let tools = get_tools();
    let max_iterations = 10;

    for _iteration in 0..max_iterations {
        let config = LlmConfig::new(&model).with_params(
            GenerationParams::new()
                .with_temperature(0.7)
                .with_max_tokens(4000),
        );

        // Accumulate streaming content. The Box callback captures an Arc<Mutex<String>>
        // because we can't capture `on_event` (lifetime/Send issues with Box<dyn Fn>).
        // We emit the accumulated content as a StreamDelta after the call completes.
        let accumulated_content = Arc::new(Mutex::new(String::new()));
        let accumulated_clone = Arc::clone(&accumulated_content);

        let on_delta = Box::new(move |delta: StreamDelta| {
            if let StreamDelta::Content(text) = delta {
                let mut content = accumulated_clone.lock().unwrap();
                content.push_str(&text);
            }
        });

        // Truncate messages if they've grown beyond context window (from tool results)
        let call_messages = truncate_messages_to_context(
            ctx.messages.clone(),
            provider_config.context_length_for_model(&model),
        );

        let response = provider
            .complete_streaming_with_tools(&call_messages, &tools, &config, on_delta)
            .await
            .map_err(|e| format!("API request failed: {}", e))?;

        // Emit the accumulated content as a stream delta
        if let Ok(content) = accumulated_content.lock() {
            if !content.is_empty() {
                on_event(ChatEvent::StreamDelta {
                    conversation_id: ctx.conversation_id.clone(),
                    content: content.clone(),
                });
            }
        }

        // Check if there are tool calls
        if let Some(tool_calls) = &response.tool_calls {
            // Add assistant message with tool calls to history
            if response.content.is_empty() {
                ctx.messages
                    .push(Message::assistant_with_tool_calls(tool_calls.clone()));
            } else {
                let mut msg = Message::assistant(&response.content);
                msg.tool_calls = Some(tool_calls.clone());
                ctx.messages.push(msg);
            }

            // Execute each tool call
            for tool_call in tool_calls {
                let tool_name = tool_call.get_name().unwrap_or_default();
                let tool_args_str = tool_call.get_arguments().unwrap_or_default();
                let tool_args: serde_json::Value =
                    serde_json::from_str(tool_args_str).unwrap_or(serde_json::Value::Null);

                // Emit tool start event
                on_event(ChatEvent::ToolStart {
                    conversation_id: ctx.conversation_id.clone(),
                    tool_call_id: tool_call.id.clone(),
                    tool_name: tool_name.to_string(),
                    tool_input: tool_args.clone(),
                });

                // Execute tool
                let (tool_result, results_count) = match tool_name {
                    "search_atoms" => {
                        let query = tool_args["query"].as_str().unwrap_or("");
                        let limit = tool_args["limit"].as_i64().unwrap_or(5) as i32;
                        match execute_search_atoms(&storage, query, limit, &ctx.scope_tag_ids, external_settings.clone()).await {
                            Ok(results) => {
                                let count = results.len() as i32;
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
                        match execute_get_atom(&storage, atom_id) {
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
                    message_id: String::new(), // Set when saving
                    tool_name: tool_name.to_string(),
                    tool_input: tool_args,
                    tool_output: Some(serde_json::Value::String(tool_result.clone())),
                    status: "complete".to_string(),
                    created_at: Utc::now().to_rfc3339(),
                    completed_at: Some(Utc::now().to_rfc3339()),
                });

                // Emit tool complete event
                on_event(ChatEvent::ToolComplete {
                    conversation_id: ctx.conversation_id.clone(),
                    tool_call_id: tool_call.id.clone(),
                    results_count,
                });

                // Add tool result to messages
                ctx.messages
                    .push(Message::tool_result(&tool_call.id, tool_result));
            }
        } else {
            // No tool calls - we have the final answer
            let content = response.content;

            // Build citations from collected data
            let citations: Vec<ChatCitation> = ctx
                .citations
                .iter()
                .enumerate()
                .map(|(i, (atom_id, chunk_index, excerpt))| ChatCitation {
                    id: Uuid::new_v4().to_string(),
                    message_id: String::new(), // Set when saving
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
                    message_index: 0, // Set when saving
                },
                tool_calls: ctx.tool_calls_record,
                citations,
            });
        }
    }

    Err("Max iterations reached without completing".to_string())
}

// ==================== Public API ====================

/// Send a chat message and run the agent loop.
///
/// The `on_event` callback is invoked with streaming deltas, tool call events,
/// and completion/error events. This is the same pattern as `EmbeddingEvent`.
///
/// Returns the final assistant message with tool calls and citations.
pub async fn send_chat_message<F>(
    storage: StorageBackend,
    conversation_id: &str,
    content: &str,
    on_event: F,
) -> Result<ChatMessageWithContext, String>
where
    F: Fn(ChatEvent) + Send + Sync,
{
    send_chat_message_with_settings(storage, conversation_id, content, on_event, None).await
}

/// Like `send_chat_message` but with externally-provided settings (from registry).
pub async fn send_chat_message_with_settings<F>(
    storage: StorageBackend,
    conversation_id: &str,
    content: &str,
    on_event: F,
    external_settings: Option<std::collections::HashMap<String, String>>,
) -> Result<ChatMessageWithContext, String>
where
    F: Fn(ChatEvent) + Send + Sync,
{
    // Resolve settings (from registry if provided, otherwise from storage)
    let settings_map = match external_settings {
        Some(s) => s,
        None => {
            storage.get_all_settings_sync()
                .map_err(|e| e.to_string())?
        }
    };

    // Get provider config and model from settings
    let (provider_config, model) = {
        let provider_config = ProviderConfig::from_settings(&settings_map);

        if provider_config.provider_type == ProviderType::OpenRouter
            && provider_config.openrouter_api_key.is_none()
        {
            return Err(
                "OpenRouter API key not configured. Please set it in Settings.".to_string(),
            );
        }

        let model = match provider_config.provider_type {
            ProviderType::Ollama => provider_config.llm_model().to_string(),
            ProviderType::OpenAICompat => provider_config.llm_model().to_string(),
            ProviderType::OpenRouter => settings_map
                .get("chat_model")
                .cloned()
                .unwrap_or_else(|| "anthropic/claude-sonnet-4".to_string()),
        };

        (provider_config, model)
    };

    // Save user message
    storage.save_message_sync(conversation_id, "user", content)
        .map_err(|e| e.to_string())?;

    // Get conversation context
    let scope_tag_ids = storage.get_scope_tag_ids_sync(conversation_id)
        .map_err(|e| e.to_string())?;
    let scope_description = storage.get_scope_description_sync(&scope_tag_ids)
        .map_err(|e| e.to_string())?;

    // Get conversation messages via get_conversation_sync and convert to provider format
    let conversation = storage.get_conversation_sync(conversation_id)
        .map_err(|e| e.to_string())?;
    let messages = match conversation {
        Some(conv) => chat_messages_to_provider_messages(conv.messages),
        None => Vec::new(),
    };

    // Build message history for API
    let mut api_messages = vec![Message::system(get_system_prompt(&scope_description))];
    api_messages.extend(messages);

    // Truncate to fit context window for providers with limited context
    let api_messages = truncate_messages_to_context(api_messages, provider_config.context_length_for_model(&model));

    // Create agent context
    let ctx = AgentContext {
        conversation_id: conversation_id.to_string(),
        scope_tag_ids,
        messages: api_messages,
        citations: Vec::new(),
        tool_calls_record: Vec::new(),
    };

    // Run agent loop (storage is Clone, so no separate connection needed)
    let mut result =
        run_agent_loop(&on_event, storage.clone(), provider_config, model, ctx, Some(settings_map)).await?;

    // Save assistant message
    {
        let saved_msg = storage.save_message_sync(conversation_id, "assistant", &result.message.content)
            .map_err(|e| e.to_string())?;

        result.message.id = saved_msg.id.clone();
        result.message.message_index = saved_msg.message_index;

        for tool_call in &mut result.tool_calls {
            tool_call.message_id = saved_msg.id.clone();
        }
        storage.save_tool_calls_sync(&saved_msg.id, &result.tool_calls)
            .map_err(|e| e.to_string())?;

        for citation in &mut result.citations {
            citation.message_id = saved_msg.id.clone();
        }
        storage.save_citations_sync(&saved_msg.id, &result.citations)
            .map_err(|e| e.to_string())?;
    }

    // Emit completion event
    on_event(ChatEvent::Complete {
        conversation_id: conversation_id.to_string(),
        message: result.clone(),
    });

    Ok(result)
}
