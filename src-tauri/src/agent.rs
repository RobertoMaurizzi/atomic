//! Chat agent Tauri command — thin wrapper around atomic-core::agent

use crate::db::Database;
use crate::models::ChatMessageWithContext;
use atomic_core::agent::ChatEvent;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[derive(serde::Serialize, Clone)]
struct ChatStreamDelta {
    conversation_id: String,
    content: String,
}

#[derive(serde::Serialize, Clone)]
struct ChatToolStart {
    conversation_id: String,
    tool_call_id: String,
    tool_name: String,
    tool_input: serde_json::Value,
}

#[derive(serde::Serialize, Clone)]
struct ChatToolComplete {
    conversation_id: String,
    tool_call_id: String,
    results_count: i32,
}

#[derive(serde::Serialize, Clone)]
struct ChatComplete {
    conversation_id: String,
    message: ChatMessageWithContext,
}

#[tauri::command]
pub async fn send_chat_message(
    app_handle: AppHandle,
    db: State<'_, Database>,
    conversation_id: String,
    content: String,
) -> Result<ChatMessageWithContext, String> {
    // Create a separate DB connection for the async agent loop
    let agent_db = Arc::new(
        atomic_core::Database::open(&db.db_path)
            .map_err(|e| format!("Failed to create agent DB connection: {}", e))?,
    );

    let app = app_handle.clone();

    let result = atomic_core::agent::send_chat_message(
        agent_db,
        &conversation_id,
        &content,
        move |event| {
            match event {
                ChatEvent::StreamDelta {
                    conversation_id,
                    content,
                } => {
                    let _ = app.emit(
                        "chat-stream-delta",
                        ChatStreamDelta {
                            conversation_id,
                            content,
                        },
                    );
                }
                ChatEvent::ToolStart {
                    conversation_id,
                    tool_call_id,
                    tool_name,
                    tool_input,
                } => {
                    let _ = app.emit(
                        "chat-tool-start",
                        ChatToolStart {
                            conversation_id,
                            tool_call_id,
                            tool_name,
                            tool_input,
                        },
                    );
                }
                ChatEvent::ToolComplete {
                    conversation_id,
                    tool_call_id,
                    results_count,
                } => {
                    let _ = app.emit(
                        "chat-tool-complete",
                        ChatToolComplete {
                            conversation_id,
                            tool_call_id,
                            results_count,
                        },
                    );
                }
                ChatEvent::Complete {
                    conversation_id,
                    message,
                } => {
                    let _ = app.emit(
                        "chat-complete",
                        ChatComplete {
                            conversation_id,
                            message,
                        },
                    );
                }
                ChatEvent::Error {
                    conversation_id,
                    error,
                } => {
                    let _ = app.emit(
                        "chat-error",
                        serde_json::json!({
                            "conversation_id": conversation_id,
                            "error": error,
                        }),
                    );
                }
            }
        },
    )
    .await?;

    Ok(result)
}
