//! Chat / Conversation routes

use crate::error::ok_or_error;
use crate::event_bridge::chat_event_callback;
use crate::state::AppState;
use actix_web::{web, HttpResponse};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateConversationBody {
    pub tag_ids: Vec<String>,
    pub title: Option<String>,
}

pub async fn create_conversation(
    state: web::Data<AppState>,
    body: web::Json<CreateConversationBody>,
) -> HttpResponse {
    let req = body.into_inner();
    match state
        .core
        .create_conversation(&req.tag_ids, req.title.as_deref())
    {
        Ok(conv) => HttpResponse::Created().json(conv),
        Err(e) => crate::error::error_response(e),
    }
}

#[derive(Deserialize)]
pub struct GetConversationsQuery {
    pub filter_tag_id: Option<String>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
}

pub async fn get_conversations(
    state: web::Data<AppState>,
    query: web::Query<GetConversationsQuery>,
) -> HttpResponse {
    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);
    ok_or_error(
        state
            .core
            .get_conversations(query.filter_tag_id.as_deref(), limit, offset),
    )
}

pub async fn get_conversation(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    let id = path.into_inner();
    match state.core.get_conversation(&id) {
        Ok(Some(conv)) => HttpResponse::Ok().json(conv),
        Ok(None) => {
            HttpResponse::NotFound().json(serde_json::json!({"error": "Conversation not found"}))
        }
        Err(e) => crate::error::error_response(e),
    }
}

#[derive(Deserialize)]
pub struct UpdateConversationBody {
    pub title: Option<String>,
    pub is_archived: Option<bool>,
}

pub async fn update_conversation(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<UpdateConversationBody>,
) -> HttpResponse {
    let id = path.into_inner();
    let req = body.into_inner();
    ok_or_error(
        state
            .core
            .update_conversation(&id, req.title.as_deref(), req.is_archived),
    )
}

pub async fn delete_conversation(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    let id = path.into_inner();
    ok_or_error(state.core.delete_conversation(&id))
}

#[derive(Deserialize)]
pub struct SetScopeBody {
    pub tag_ids: Vec<String>,
}

pub async fn set_conversation_scope(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<SetScopeBody>,
) -> HttpResponse {
    let id = path.into_inner();
    ok_or_error(state.core.set_conversation_scope(&id, &body.tag_ids))
}

#[derive(Deserialize)]
pub struct AddTagBody {
    pub tag_id: String,
}

pub async fn add_tag_to_scope(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<AddTagBody>,
) -> HttpResponse {
    let id = path.into_inner();
    ok_or_error(state.core.add_tag_to_scope(&id, &body.tag_id))
}

pub async fn remove_tag_from_scope(
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> HttpResponse {
    let (id, tag_id) = path.into_inner();
    ok_or_error(state.core.remove_tag_from_scope(&id, &tag_id))
}

#[derive(Deserialize)]
pub struct SendMessageBody {
    pub content: String,
}

pub async fn send_chat_message(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<SendMessageBody>,
) -> HttpResponse {
    let conversation_id = path.into_inner();
    let on_event = chat_event_callback(state.event_tx.clone());

    match state
        .core
        .send_chat_message(&conversation_id, &body.content, on_event)
        .await
    {
        Ok(message) => HttpResponse::Ok().json(message),
        Err(e) => crate::error::error_response(e),
    }
}
