//! Clustering routes

use crate::error::ok_or_error;
use crate::state::AppState;
use actix_web::{web, HttpResponse};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct ComputeClustersBody {
    pub min_similarity: Option<f32>,
    pub min_cluster_size: Option<i32>,
}

pub async fn compute_clusters(
    state: web::Data<AppState>,
    body: web::Json<ComputeClustersBody>,
) -> HttpResponse {
    let min_similarity = body.min_similarity.unwrap_or(0.6);
    let min_cluster_size = body.min_cluster_size.unwrap_or(2);

    match state
        .core
        .compute_clusters(min_similarity, min_cluster_size)
    {
        Ok(clusters) => {
            if let Err(e) = state.core.save_clusters(&clusters) {
                return crate::error::error_response(e);
            }
            HttpResponse::Ok().json(clusters)
        }
        Err(e) => crate::error::error_response(e),
    }
}

pub async fn get_clusters(state: web::Data<AppState>) -> HttpResponse {
    // Try cached clusters first, then compute fresh
    let db = state.core.database();
    let cached = {
        let conn = match db.conn.lock() {
            Ok(c) => c,
            Err(e) => {
                return HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": e.to_string()}));
            }
        };
        atomic_core::clustering::get_cached_clusters(&conn).unwrap_or_default()
    };

    if !cached.is_empty() {
        return HttpResponse::Ok().json(cached);
    }

    // No cached clusters — compute fresh
    match state.core.compute_clusters(0.6, 2) {
        Ok(clusters) => {
            let _ = state.core.save_clusters(&clusters);
            HttpResponse::Ok().json(clusters)
        }
        Err(e) => crate::error::error_response(e),
    }
}

#[derive(Deserialize)]
pub struct ConnectionCountsQuery {
    pub min_similarity: Option<f32>,
}

pub async fn get_connection_counts(
    state: web::Data<AppState>,
    query: web::Query<ConnectionCountsQuery>,
) -> HttpResponse {
    let min_similarity = query.min_similarity.unwrap_or(0.5);
    ok_or_error(state.core.get_connection_counts(min_similarity))
}
