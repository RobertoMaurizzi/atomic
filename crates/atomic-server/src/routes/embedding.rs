//! Embedding management routes

use crate::event_bridge::embedding_event_callback;
use crate::state::AppState;
use actix_web::{web, HttpResponse};

pub async fn process_pending_embeddings(state: web::Data<AppState>) -> HttpResponse {
    let on_event = embedding_event_callback(state.event_tx.clone());
    match state.core.process_pending_embeddings(on_event) {
        Ok(count) => HttpResponse::Ok().json(serde_json::json!({"count": count})),
        Err(e) => crate::error::error_response(e),
    }
}

pub async fn process_pending_tagging(state: web::Data<AppState>) -> HttpResponse {
    // Process tagging for atoms with complete embeddings but pending tagging
    let db = state.core.database();
    let conn = match db.conn.lock() {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()}));
        }
    };

    let mut stmt = match conn.prepare(
        "UPDATE atoms SET tagging_status = 'processing'
         WHERE embedding_status = 'complete'
         AND tagging_status = 'pending'
         RETURNING id",
    ) {
        Ok(s) => s,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()}));
        }
    };

    let pending_atoms: Vec<String> = match stmt
        .query_map([], |row| row.get(0))
        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
    {
        Ok(atoms) => atoms,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()}));
        }
    };

    let count = pending_atoms.len() as i32;
    drop(stmt);
    drop(conn);

    if count > 0 {
        let on_event = embedding_event_callback(state.event_tx.clone());
        let db_clone = state.core.database();
        tokio::spawn(atomic_core::embedding::process_tagging_batch(
            db_clone,
            pending_atoms,
            on_event,
        ));
    }

    HttpResponse::Ok().json(serde_json::json!({"count": count}))
}

pub async fn retry_embedding(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    let atom_id = path.into_inner();
    let on_event = embedding_event_callback(state.event_tx.clone());
    match state.core.retry_embedding(&atom_id, on_event) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"status": "ok"})),
        Err(e) => crate::error::error_response(e),
    }
}

pub async fn reset_stuck_processing(state: web::Data<AppState>) -> HttpResponse {
    match state.core.reset_stuck_processing() {
        Ok(count) => HttpResponse::Ok().json(serde_json::json!({"count": count})),
        Err(e) => crate::error::error_response(e),
    }
}

pub async fn get_embedding_status(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    let atom_id = path.into_inner();
    let db = state.core.database();
    let conn = match db.conn.lock() {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()}));
        }
    };

    match conn.query_row(
        "SELECT COALESCE(embedding_status, 'pending') FROM atoms WHERE id = ?1",
        [&atom_id],
        |row| row.get::<_, String>(0),
    ) {
        Ok(status) => HttpResponse::Ok().json(serde_json::json!({"status": status})),
        Err(e) => HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": e.to_string()})),
    }
}
