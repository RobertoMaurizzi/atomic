//! Token management endpoints

use crate::error::ok_or_error;
use crate::state::AppState;
use actix_web::{web, HttpResponse};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateTokenBody {
    pub name: String,
}

/// POST /api/auth/tokens — Create a new named API token
pub async fn create_token(
    state: web::Data<AppState>,
    body: web::Json<CreateTokenBody>,
) -> HttpResponse {
    match state.core.create_api_token(&body.name) {
        Ok((info, raw_token)) => HttpResponse::Created().json(serde_json::json!({
            "id": info.id,
            "name": info.name,
            "token": raw_token,
            "prefix": info.token_prefix,
            "created_at": info.created_at,
        })),
        Err(e) => crate::error::error_response(e),
    }
}

/// GET /api/auth/tokens — List all tokens (metadata only, no raw token values)
pub async fn list_tokens(state: web::Data<AppState>) -> HttpResponse {
    ok_or_error(state.core.list_api_tokens())
}

/// DELETE /api/auth/tokens/{id} — Revoke a token
pub async fn revoke_token(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    let token_id = path.into_inner();
    ok_or_error(state.core.revoke_api_token(&token_id))
}
