//! Search routes

use crate::error::ok_or_error;
use crate::state::AppState;
use actix_web::{web, HttpResponse};
use atomic_core::{SearchMode, SearchOptions};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub mode: String, // "keyword", "semantic", "hybrid"
    pub limit: Option<i32>,
    pub threshold: Option<f32>,
}

pub async fn search(state: web::Data<AppState>, body: web::Json<SearchRequest>) -> HttpResponse {
    let req = body.into_inner();
    let mode = match req.mode.as_str() {
        "keyword" => SearchMode::Keyword,
        "semantic" => SearchMode::Semantic,
        "hybrid" => SearchMode::Hybrid,
        _ => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": "Invalid search mode. Use 'keyword', 'semantic', or 'hybrid'."
            }));
        }
    };

    let mut options = SearchOptions::new(req.query, mode, req.limit.unwrap_or(20));
    if let Some(threshold) = req.threshold {
        options = options.with_threshold(threshold);
    }

    let result = state.core.search(options).await;
    ok_or_error(result)
}

#[derive(Deserialize)]
pub struct FindSimilarQuery {
    pub limit: Option<i32>,
    pub threshold: Option<f32>,
}

pub async fn find_similar(
    state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<FindSimilarQuery>,
) -> HttpResponse {
    let atom_id = path.into_inner();
    let limit = query.limit.unwrap_or(10);
    let threshold = query.threshold.unwrap_or(0.7);
    ok_or_error(state.core.find_similar(&atom_id, limit, threshold))
}
