//! Chat conversation CRUD operations
//!
//! Provides all conversation, message, tool call, and citation management
//! for the chat system. Used by both Tauri and standalone server.

use crate::error::AtomicCoreError;
use crate::models::{
    ChatCitation, ChatMessage, ChatMessageWithContext, ChatToolCall, Conversation,
    ConversationWithMessages, ConversationWithTags, Tag,
};
use rusqlite::Connection;

// ==================== Helper Functions ====================

/// Get tags for a conversation
pub fn get_conversation_tags(
    conn: &Connection,
    conversation_id: &str,
) -> Result<Vec<Tag>, AtomicCoreError> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, t.parent_id, t.created_at
         FROM tags t
         JOIN conversation_tags ct ON ct.tag_id = t.id
         WHERE ct.conversation_id = ?1
         ORDER BY t.name",
    )?;

    let tags = stmt
        .query_map([conversation_id], |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
                parent_id: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(tags)
}

/// Get message count and last message preview for a conversation
pub fn get_conversation_summary(
    conn: &Connection,
    conversation_id: &str,
) -> Result<(i32, Option<String>), AtomicCoreError> {
    let message_count: i32 = conn.query_row(
        "SELECT COUNT(*) FROM chat_messages WHERE conversation_id = ?1",
        [conversation_id],
        |row| row.get(0),
    )?;

    let last_message_preview: Option<String> = conn
        .query_row(
            "SELECT content FROM chat_messages
             WHERE conversation_id = ?1
             ORDER BY message_index DESC
             LIMIT 1",
            [conversation_id],
            |row| {
                let content: String = row.get(0)?;
                Ok(if content.len() > 100 {
                    format!("{}...", &content[..100])
                } else {
                    content
                })
            },
        )
        .ok();

    Ok((message_count, last_message_preview))
}

/// Get tool calls for a message
pub fn get_message_tool_calls(
    conn: &Connection,
    message_id: &str,
) -> Result<Vec<ChatToolCall>, AtomicCoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, message_id, tool_name, tool_input, tool_output, status, created_at, completed_at
         FROM chat_tool_calls
         WHERE message_id = ?1
         ORDER BY created_at",
    )?;

    let tool_calls = stmt
        .query_map([message_id], |row| {
            let tool_input_str: String = row.get(3)?;
            let tool_output_str: Option<String> = row.get(4)?;

            Ok(ChatToolCall {
                id: row.get(0)?,
                message_id: row.get(1)?,
                tool_name: row.get(2)?,
                tool_input: serde_json::from_str(&tool_input_str).unwrap_or(serde_json::Value::Null),
                tool_output: tool_output_str
                    .map(|s| serde_json::from_str(&s).unwrap_or(serde_json::Value::Null)),
                status: row.get(5)?,
                created_at: row.get(6)?,
                completed_at: row.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(tool_calls)
}

/// Get citations for a message
pub fn get_message_citations(
    conn: &Connection,
    message_id: &str,
) -> Result<Vec<ChatCitation>, AtomicCoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, message_id, citation_index, atom_id, chunk_index, excerpt, relevance_score
         FROM chat_citations
         WHERE message_id = ?1
         ORDER BY citation_index",
    )?;

    let citations = stmt
        .query_map([message_id], |row| {
            Ok(ChatCitation {
                id: row.get(0)?,
                message_id: row.get(1)?,
                citation_index: row.get(2)?,
                atom_id: row.get(3)?,
                chunk_index: row.get(4)?,
                excerpt: row.get(5)?,
                relevance_score: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(citations)
}

/// Get messages with context for a conversation
pub fn get_messages_with_context(
    conn: &Connection,
    conversation_id: &str,
) -> Result<Vec<ChatMessageWithContext>, AtomicCoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, conversation_id, role, content, created_at, message_index
         FROM chat_messages
         WHERE conversation_id = ?1
         ORDER BY message_index",
    )?;

    let messages: Vec<ChatMessage> = stmt
        .query_map([conversation_id], |row| {
            Ok(ChatMessage {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
                message_index: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut messages_with_context = Vec::new();
    for message in messages {
        let tool_calls = get_message_tool_calls(conn, &message.id)?;
        let citations = get_message_citations(conn, &message.id)?;
        messages_with_context.push(ChatMessageWithContext {
            message,
            tool_calls,
            citations,
        });
    }

    Ok(messages_with_context)
}

// ==================== CRUD Operations ====================

/// Create a new conversation
pub fn create_conversation(
    conn: &Connection,
    tag_ids: &[String],
    title: Option<&str>,
) -> Result<ConversationWithTags, AtomicCoreError> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = uuid::Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO conversations (id, title, created_at, updated_at, is_archived)
         VALUES (?1, ?2, ?3, ?4, 0)",
        rusqlite::params![&id, &title, &now, &now],
    )?;

    for tag_id in tag_ids {
        conn.execute(
            "INSERT INTO conversation_tags (conversation_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![&id, tag_id],
        )?;
    }

    let tags = get_conversation_tags(conn, &id)?;

    Ok(ConversationWithTags {
        conversation: Conversation {
            id,
            title: title.map(String::from),
            created_at: now.clone(),
            updated_at: now,
            is_archived: false,
        },
        tags,
        message_count: 0,
        last_message_preview: None,
    })
}

/// Get all conversations, optionally filtered by tag
pub fn get_conversations(
    conn: &Connection,
    filter_tag_id: Option<&str>,
    limit: i32,
    offset: i32,
) -> Result<Vec<ConversationWithTags>, AtomicCoreError> {
    let conversations: Vec<Conversation> = if let Some(tag_id) = filter_tag_id {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT c.id, c.title, c.created_at, c.updated_at, c.is_archived
             FROM conversations c
             JOIN conversation_tags ct ON ct.conversation_id = c.id
             WHERE ct.tag_id = ?1 AND c.is_archived = 0
             ORDER BY c.updated_at DESC
             LIMIT ?2 OFFSET ?3",
        )?;

        let results = stmt
            .query_map(rusqlite::params![tag_id, limit, offset], |row| {
                Ok(Conversation {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                    is_archived: row.get::<_, i32>(4)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        results
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, title, created_at, updated_at, is_archived
             FROM conversations
             WHERE is_archived = 0
             ORDER BY updated_at DESC
             LIMIT ?1 OFFSET ?2",
        )?;

        let results = stmt
            .query_map(rusqlite::params![limit, offset], |row| {
                Ok(Conversation {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                    is_archived: row.get::<_, i32>(4)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        results
    };

    let mut result = Vec::new();
    for conversation in conversations {
        let tags = get_conversation_tags(conn, &conversation.id)?;
        let (message_count, last_message_preview) =
            get_conversation_summary(conn, &conversation.id)?;

        result.push(ConversationWithTags {
            conversation,
            tags,
            message_count,
            last_message_preview,
        });
    }

    Ok(result)
}

/// Get a single conversation with all messages
pub fn get_conversation(
    conn: &Connection,
    conversation_id: &str,
) -> Result<Option<ConversationWithMessages>, AtomicCoreError> {
    let conversation: Option<Conversation> = conn
        .query_row(
            "SELECT id, title, created_at, updated_at, is_archived
             FROM conversations
             WHERE id = ?1",
            [conversation_id],
            |row| {
                Ok(Conversation {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                    is_archived: row.get::<_, i32>(4)? != 0,
                })
            },
        )
        .ok();

    match conversation {
        Some(conv) => {
            let tags = get_conversation_tags(conn, &conv.id)?;
            let messages = get_messages_with_context(conn, &conv.id)?;

            Ok(Some(ConversationWithMessages {
                conversation: conv,
                tags,
                messages,
            }))
        }
        None => Ok(None),
    }
}

/// Update a conversation (title, archive status)
pub fn update_conversation(
    conn: &Connection,
    id: &str,
    title: Option<&str>,
    is_archived: Option<bool>,
) -> Result<Conversation, AtomicCoreError> {
    let now = chrono::Utc::now().to_rfc3339();

    if let Some(t) = title {
        conn.execute(
            "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![t, &now, id],
        )?;
    }

    if let Some(archived) = is_archived {
        conn.execute(
            "UPDATE conversations SET is_archived = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![if archived { 1 } else { 0 }, &now, id],
        )?;
    }

    conn.query_row(
        "SELECT id, title, created_at, updated_at, is_archived
         FROM conversations
         WHERE id = ?1",
        [id],
        |row| {
            Ok(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                is_archived: row.get::<_, i32>(4)? != 0,
            })
        },
    )
    .map_err(|_| AtomicCoreError::NotFound(format!("Conversation not found: {}", id)))
}

/// Delete a conversation
pub fn delete_conversation(conn: &Connection, id: &str) -> Result<(), AtomicCoreError> {
    conn.execute("DELETE FROM conversations WHERE id = ?1", [id])?;
    Ok(())
}

// ==================== Scope Management ====================

/// Set the full scope (replace all tags)
pub fn set_conversation_scope(
    conn: &Connection,
    conversation_id: &str,
    tag_ids: &[String],
) -> Result<ConversationWithTags, AtomicCoreError> {
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "DELETE FROM conversation_tags WHERE conversation_id = ?1",
        [conversation_id],
    )?;

    for tag_id in tag_ids {
        conn.execute(
            "INSERT INTO conversation_tags (conversation_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![conversation_id, tag_id],
        )?;
    }

    conn.execute(
        "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
        rusqlite::params![&now, conversation_id],
    )?;

    let conversation = conn
        .query_row(
            "SELECT id, title, created_at, updated_at, is_archived
             FROM conversations WHERE id = ?1",
            [conversation_id],
            |row| {
                Ok(Conversation {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                    is_archived: row.get::<_, i32>(4)? != 0,
                })
            },
        )
        .map_err(|_| {
            AtomicCoreError::NotFound(format!("Conversation not found: {}", conversation_id))
        })?;

    let tags = get_conversation_tags(conn, conversation_id)?;
    let (message_count, last_message_preview) =
        get_conversation_summary(conn, conversation_id)?;

    Ok(ConversationWithTags {
        conversation,
        tags,
        message_count,
        last_message_preview,
    })
}

/// Add a single tag to scope
pub fn add_tag_to_scope(
    conn: &Connection,
    conversation_id: &str,
    tag_id: &str,
) -> Result<ConversationWithTags, AtomicCoreError> {
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT OR IGNORE INTO conversation_tags (conversation_id, tag_id) VALUES (?1, ?2)",
        rusqlite::params![conversation_id, tag_id],
    )?;

    conn.execute(
        "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
        rusqlite::params![&now, conversation_id],
    )?;

    let conversation = conn
        .query_row(
            "SELECT id, title, created_at, updated_at, is_archived
             FROM conversations WHERE id = ?1",
            [conversation_id],
            |row| {
                Ok(Conversation {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                    is_archived: row.get::<_, i32>(4)? != 0,
                })
            },
        )
        .map_err(|_| {
            AtomicCoreError::NotFound(format!("Conversation not found: {}", conversation_id))
        })?;

    let tags = get_conversation_tags(conn, conversation_id)?;
    let (message_count, last_message_preview) =
        get_conversation_summary(conn, conversation_id)?;

    Ok(ConversationWithTags {
        conversation,
        tags,
        message_count,
        last_message_preview,
    })
}

/// Remove a single tag from scope
pub fn remove_tag_from_scope(
    conn: &Connection,
    conversation_id: &str,
    tag_id: &str,
) -> Result<ConversationWithTags, AtomicCoreError> {
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "DELETE FROM conversation_tags WHERE conversation_id = ?1 AND tag_id = ?2",
        rusqlite::params![conversation_id, tag_id],
    )?;

    conn.execute(
        "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
        rusqlite::params![&now, conversation_id],
    )?;

    let conversation = conn
        .query_row(
            "SELECT id, title, created_at, updated_at, is_archived
             FROM conversations WHERE id = ?1",
            [conversation_id],
            |row| {
                Ok(Conversation {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                    is_archived: row.get::<_, i32>(4)? != 0,
                })
            },
        )
        .map_err(|_| {
            AtomicCoreError::NotFound(format!("Conversation not found: {}", conversation_id))
        })?;

    let tags = get_conversation_tags(conn, conversation_id)?;
    let (message_count, last_message_preview) =
        get_conversation_summary(conn, conversation_id)?;

    Ok(ConversationWithTags {
        conversation,
        tags,
        message_count,
        last_message_preview,
    })
}

// ==================== Message DB Operations ====================

/// Save a message and return (message_id, message_index)
pub fn save_message(
    conn: &Connection,
    conversation_id: &str,
    role: &str,
    content: &str,
) -> Result<(String, i32), AtomicCoreError> {
    let message_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let message_index: i32 = conn.query_row(
        "SELECT COALESCE(MAX(message_index), -1) + 1 FROM chat_messages WHERE conversation_id = ?1",
        [conversation_id],
        |row| row.get(0),
    )?;

    conn.execute(
        "INSERT INTO chat_messages (id, conversation_id, role, content, created_at, message_index)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![&message_id, conversation_id, role, content, &now, message_index],
    )?;

    conn.execute(
        "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
        rusqlite::params![&now, conversation_id],
    )?;

    Ok((message_id, message_index))
}

/// Save tool calls for a message
pub fn save_tool_calls(
    conn: &Connection,
    message_id: &str,
    tool_calls: &[ChatToolCall],
) -> Result<(), AtomicCoreError> {
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
        )?;
    }
    Ok(())
}

/// Save citations for a message
pub fn save_citations(
    conn: &Connection,
    message_id: &str,
    citations: &[ChatCitation],
) -> Result<(), AtomicCoreError> {
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
        )?;
    }
    Ok(())
}

/// Get scope tag IDs for a conversation
pub fn get_scope_tag_ids(
    conn: &Connection,
    conversation_id: &str,
) -> Result<Vec<String>, AtomicCoreError> {
    let mut stmt =
        conn.prepare("SELECT tag_id FROM conversation_tags WHERE conversation_id = ?1")?;

    let tag_ids = stmt
        .query_map([conversation_id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(tag_ids)
}

/// Get scope description for system prompt
pub fn get_scope_description(conn: &Connection, tag_ids: &[String]) -> String {
    if tag_ids.is_empty() {
        return "You have access to ALL atoms in the knowledge base.".to_string();
    }

    let placeholders: Vec<String> = (0..tag_ids.len()).map(|i| format!("?{}", i + 1)).collect();
    let query = format!(
        "SELECT name FROM tags WHERE id IN ({})",
        placeholders.join(", ")
    );

    let mut stmt = match conn.prepare(&query) {
        Ok(s) => s,
        Err(_) => return "You have access to a scoped set of atoms.".to_string(),
    };

    let params: Vec<&dyn rusqlite::ToSql> =
        tag_ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
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

/// Get conversation messages in provider Message format
pub fn get_conversation_messages(
    conn: &Connection,
    conversation_id: &str,
) -> Result<Vec<crate::providers::types::Message>, AtomicCoreError> {
    use crate::providers::types::{Message, MessageRole};

    let mut stmt = conn.prepare(
        "SELECT role, content FROM chat_messages WHERE conversation_id = ?1 ORDER BY message_index",
    )?;

    let messages = stmt
        .query_map([conversation_id], |row| {
            let role_str: String = row.get(0)?;
            let content: Option<String> = row.get(1)?;
            let role = match role_str.as_str() {
                "system" => MessageRole::System,
                "assistant" => MessageRole::Assistant,
                "tool" => MessageRole::Tool,
                _ => MessageRole::User,
            };
            Ok(Message {
                role,
                content,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use tempfile::NamedTempFile;

    fn setup_db() -> (Database, NamedTempFile) {
        let temp_file = NamedTempFile::new().unwrap();
        let db = Database::open_or_create(temp_file.path()).unwrap();
        (db, temp_file)
    }

    #[test]
    fn test_create_conversation() {
        let (db, _temp) = setup_db();
        let conn = db.conn.lock().unwrap();

        let result = create_conversation(&conn, &[], Some("Test Chat")).unwrap();
        assert_eq!(result.conversation.title, Some("Test Chat".to_string()));
        assert_eq!(result.message_count, 0);
        assert!(result.tags.is_empty());
    }

    #[test]
    fn test_create_conversation_with_tags() {
        let (db, _temp) = setup_db();
        let conn = db.conn.lock().unwrap();

        // Create a tag first
        let tag_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO tags (id, name, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![&tag_id, "TestTag", &now],
        )
        .unwrap();

        let result =
            create_conversation(&conn, &[tag_id.clone()], Some("Tagged Chat")).unwrap();
        assert_eq!(result.tags.len(), 1);
        assert_eq!(result.tags[0].name, "TestTag");
    }

    #[test]
    fn test_get_conversations() {
        let (db, _temp) = setup_db();
        let conn = db.conn.lock().unwrap();

        create_conversation(&conn, &[], Some("Chat 1")).unwrap();
        create_conversation(&conn, &[], Some("Chat 2")).unwrap();

        let conversations = get_conversations(&conn, None, 10, 0).unwrap();
        assert_eq!(conversations.len(), 2);
    }

    #[test]
    fn test_get_conversation_with_messages() {
        let (db, _temp) = setup_db();
        let conn = db.conn.lock().unwrap();

        let conv = create_conversation(&conn, &[], Some("Chat")).unwrap();
        save_message(&conn, &conv.conversation.id, "user", "Hello").unwrap();
        save_message(&conn, &conv.conversation.id, "assistant", "Hi there!").unwrap();

        let result = get_conversation(&conn, &conv.conversation.id).unwrap();
        assert!(result.is_some());
        let conv_with_msgs = result.unwrap();
        assert_eq!(conv_with_msgs.messages.len(), 2);
        assert_eq!(conv_with_msgs.messages[0].message.role, "user");
        assert_eq!(conv_with_msgs.messages[1].message.role, "assistant");
    }

    #[test]
    fn test_update_conversation() {
        let (db, _temp) = setup_db();
        let conn = db.conn.lock().unwrap();

        let conv = create_conversation(&conn, &[], Some("Original")).unwrap();
        let updated =
            update_conversation(&conn, &conv.conversation.id, Some("Updated"), None).unwrap();
        assert_eq!(updated.title, Some("Updated".to_string()));
    }

    #[test]
    fn test_delete_conversation() {
        let (db, _temp) = setup_db();
        let conn = db.conn.lock().unwrap();

        let conv = create_conversation(&conn, &[], Some("ToDelete")).unwrap();
        delete_conversation(&conn, &conv.conversation.id).unwrap();

        let result = get_conversation(&conn, &conv.conversation.id).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_scope_management() {
        let (db, _temp) = setup_db();
        let conn = db.conn.lock().unwrap();

        // Create tags
        let tag1_id = uuid::Uuid::new_v4().to_string();
        let tag2_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO tags (id, name, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![&tag1_id, "Tag1", &now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tags (id, name, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![&tag2_id, "Tag2", &now],
        )
        .unwrap();

        let conv = create_conversation(&conn, &[], None).unwrap();

        // Add tag
        let result = add_tag_to_scope(&conn, &conv.conversation.id, &tag1_id).unwrap();
        assert_eq!(result.tags.len(), 1);

        // Add another tag
        let result = add_tag_to_scope(&conn, &conv.conversation.id, &tag2_id).unwrap();
        assert_eq!(result.tags.len(), 2);

        // Remove tag
        let result = remove_tag_from_scope(&conn, &conv.conversation.id, &tag1_id).unwrap();
        assert_eq!(result.tags.len(), 1);

        // Set scope (replace all)
        let result = set_conversation_scope(
            &conn,
            &conv.conversation.id,
            &[tag1_id.clone(), tag2_id.clone()],
        )
        .unwrap();
        assert_eq!(result.tags.len(), 2);
    }

    #[test]
    fn test_save_message() {
        let (db, _temp) = setup_db();
        let conn = db.conn.lock().unwrap();

        let conv = create_conversation(&conn, &[], None).unwrap();
        let (msg_id, msg_idx) =
            save_message(&conn, &conv.conversation.id, "user", "Hello").unwrap();

        assert!(!msg_id.is_empty());
        assert_eq!(msg_idx, 0);

        let (_, msg_idx2) =
            save_message(&conn, &conv.conversation.id, "assistant", "Hi!").unwrap();
        assert_eq!(msg_idx2, 1);
    }
}
