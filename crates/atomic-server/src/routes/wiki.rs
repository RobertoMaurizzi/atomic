//! Wiki article routes

use crate::error::ok_or_error;
use crate::state::AppState;
use actix_web::{web, HttpResponse};
use serde::Deserialize;

pub async fn get_all_wiki_articles(state: web::Data<AppState>) -> HttpResponse {
    let db = state.core.database();
    let conn = match db.conn.lock() {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()}));
        }
    };
    match atomic_core::wiki::load_all_wiki_articles(&conn) {
        Ok(articles) => HttpResponse::Ok().json(articles),
        Err(e) => HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": e})),
    }
}

pub async fn get_wiki(state: web::Data<AppState>, path: web::Path<String>) -> HttpResponse {
    let tag_id = path.into_inner();
    ok_or_error(state.core.get_wiki(&tag_id))
}

pub async fn get_wiki_status(state: web::Data<AppState>, path: web::Path<String>) -> HttpResponse {
    let tag_id = path.into_inner();
    ok_or_error(state.core.get_wiki_status(&tag_id))
}

#[derive(Deserialize)]
pub struct GenerateWikiBody {
    pub tag_name: String,
}

pub async fn generate_wiki(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<GenerateWikiBody>,
) -> HttpResponse {
    let tag_id = path.into_inner();
    match state.core.generate_wiki(&tag_id, &body.tag_name).await {
        Ok(article) => HttpResponse::Ok().json(article),
        Err(e) => crate::error::error_response(e),
    }
}

pub async fn update_wiki(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<GenerateWikiBody>,
) -> HttpResponse {
    let tag_id = path.into_inner();

    // Get provider config and existing article
    let db = state.core.database();
    let (provider_config, wiki_model, existing, update_input) = {
        let conn = match db.conn.lock() {
            Ok(c) => c,
            Err(e) => {
                return HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": e.to_string()}));
            }
        };
        let settings_map = match atomic_core::settings::get_all_settings(&conn) {
            Ok(s) => s,
            Err(e) => {
                return HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": e.to_string()}));
            }
        };
        let provider_config = atomic_core::ProviderConfig::from_settings(&settings_map);
        let wiki_model = match provider_config.provider_type {
            atomic_core::ProviderType::Ollama => provider_config.llm_model().to_string(),
            atomic_core::ProviderType::OpenRouter => settings_map
                .get("wiki_model")
                .cloned()
                .unwrap_or_else(|| "anthropic/claude-sonnet-4.5".to_string()),
        };
        let existing = match atomic_core::wiki::load_wiki_article(&conn, &tag_id) {
            Ok(e) => e,
            Err(e) => {
                return HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": e}));
            }
        };
        let update_input = if let Some(ref ex) = existing {
            match atomic_core::wiki::prepare_wiki_update(
                &conn,
                &tag_id,
                &body.tag_name,
                &ex.article,
                &ex.citations,
            ) {
                Ok(input) => input,
                Err(e) => {
                    return HttpResponse::InternalServerError()
                        .json(serde_json::json!({"error": e}));
                }
            }
        } else {
            None
        };
        (provider_config, wiki_model, existing, update_input)
    };

    let existing = match existing {
        Some(e) => e,
        None => {
            return HttpResponse::NotFound()
                .json(serde_json::json!({"error": "No existing article to update"}));
        }
    };

    let input = match update_input {
        Some(input) => input,
        None => return HttpResponse::Ok().json(existing),
    };

    match atomic_core::wiki::update_wiki_content(&provider_config, &input, &wiki_model).await {
        Ok(result) => {
            let conn = match db.conn.lock() {
                Ok(c) => c,
                Err(e) => {
                    return HttpResponse::InternalServerError()
                        .json(serde_json::json!({"error": e.to_string()}));
                }
            };
            if let Err(e) =
                atomic_core::wiki::save_wiki_article(&conn, &result.article, &result.citations)
            {
                return HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": e}));
            }
            HttpResponse::Ok().json(result)
        }
        Err(e) => HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": e})),
    }
}

pub async fn delete_wiki(state: web::Data<AppState>, path: web::Path<String>) -> HttpResponse {
    let tag_id = path.into_inner();
    ok_or_error(state.core.delete_wiki(&tag_id))
}
