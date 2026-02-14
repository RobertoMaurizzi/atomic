//! Chat Tauri commands — thin wrappers around atomic-core::chat

use crate::db::Database;
use crate::models::{
    Conversation, ConversationWithMessages, ConversationWithTags,
};
use tauri::State;

#[tauri::command]
pub fn create_conversation(
    db: State<Database>,
    tag_ids: Vec<String>,
    title: Option<String>,
) -> Result<ConversationWithTags, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    atomic_core::chat::create_conversation(&conn, &tag_ids, title.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_conversations(
    db: State<Database>,
    filter_tag_id: Option<String>,
    limit: i32,
    offset: i32,
) -> Result<Vec<ConversationWithTags>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    atomic_core::chat::get_conversations(&conn, filter_tag_id.as_deref(), limit, offset)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_conversation(
    db: State<Database>,
    conversation_id: String,
) -> Result<Option<ConversationWithMessages>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    atomic_core::chat::get_conversation(&conn, &conversation_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_conversation(
    db: State<Database>,
    id: String,
    title: Option<String>,
    is_archived: Option<bool>,
) -> Result<Conversation, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    atomic_core::chat::update_conversation(&conn, &id, title.as_deref(), is_archived)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_conversation(db: State<Database>, id: String) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    atomic_core::chat::delete_conversation(&conn, &id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_conversation_scope(
    db: State<Database>,
    conversation_id: String,
    tag_ids: Vec<String>,
) -> Result<ConversationWithTags, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    atomic_core::chat::set_conversation_scope(&conn, &conversation_id, &tag_ids)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_tag_to_scope(
    db: State<Database>,
    conversation_id: String,
    tag_id: String,
) -> Result<ConversationWithTags, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    atomic_core::chat::add_tag_to_scope(&conn, &conversation_id, &tag_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_tag_from_scope(
    db: State<Database>,
    conversation_id: String,
    tag_id: String,
) -> Result<ConversationWithTags, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    atomic_core::chat::remove_tag_from_scope(&conn, &conversation_id, &tag_id)
        .map_err(|e| e.to_string())
}
