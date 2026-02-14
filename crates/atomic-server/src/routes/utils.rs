//! Utility routes

use crate::state::AppState;
use actix_web::{web, HttpResponse};

pub async fn check_sqlite_vec(state: web::Data<AppState>) -> HttpResponse {
    let db = state.core.database();
    let conn = match db.conn.lock() {
        Ok(c) => c,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()}));
        }
    };

    match conn.query_row("SELECT vec_version()", [], |row| row.get::<_, String>(0)) {
        Ok(version) => HttpResponse::Ok().json(serde_json::json!({"version": version})),
        Err(e) => HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": format!("sqlite-vec not loaded: {}", e)})),
    }
}

pub async fn compact_tags(state: web::Data<AppState>) -> HttpResponse {
    let db = state.core.database();

    let (provider_config, model) = {
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
        let model = match provider_config.provider_type {
            atomic_core::ProviderType::Ollama => provider_config.llm_model().to_string(),
            atomic_core::ProviderType::OpenRouter => settings_map
                .get("tagging_model")
                .cloned()
                .unwrap_or_else(|| "openai/gpt-4o-mini".to_string()),
        };
        (provider_config, model)
    };

    // Get supported params for OpenRouter
    let supported_params: Option<Vec<String>> =
        if provider_config.provider_type == atomic_core::ProviderType::OpenRouter {
            use atomic_core::providers::models::{
                fetch_and_return_capabilities, get_cached_capabilities_sync, save_capabilities_cache,
            };

            let (cached, is_stale) = {
                let conn = match db.conn.lock() {
                    Ok(c) => c,
                    Err(_) => return HttpResponse::InternalServerError().finish(),
                };
                match get_cached_capabilities_sync(&conn) {
                    Ok(Some(cache)) => {
                        let stale = cache.is_stale();
                        (Some(cache), stale)
                    }
                    Ok(None) => (None, true),
                    Err(_) => (None, true),
                }
            };

            let capabilities = if is_stale {
                let client = reqwest::Client::new();
                match fetch_and_return_capabilities(&client).await {
                    Ok(fresh) => {
                        if let Ok(conn) = db.new_connection() {
                            let _ = save_capabilities_cache(&conn, &fresh);
                        }
                        fresh
                    }
                    Err(_) => cached.unwrap_or_default(),
                }
            } else {
                cached.unwrap_or_default()
            };

            capabilities.get_supported_params(&model).cloned()
        } else {
            None
        };

    let all_tags = {
        let conn = match db.conn.lock() {
            Ok(c) => c,
            Err(e) => {
                return HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": e.to_string()}));
            }
        };
        match atomic_core::compaction::read_all_tags(&conn) {
            Ok(t) => t,
            Err(e) => {
                return HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": e}));
            }
        }
    };

    if all_tags == "(no existing tags)" {
        return HttpResponse::Ok().json(serde_json::json!({
            "tags_merged": 0,
            "atoms_retagged": 0
        }));
    }

    match atomic_core::compaction::fetch_merge_suggestions(
        &provider_config,
        &all_tags,
        &model,
        supported_params,
    )
    .await
    {
        Ok(merge_suggestions) => {
            let conn = match db.conn.lock() {
                Ok(c) => c,
                Err(e) => {
                    return HttpResponse::InternalServerError()
                        .json(serde_json::json!({"error": e.to_string()}));
                }
            };
            let (merged, retagged, errors) =
                atomic_core::compaction::apply_merge_operations(&conn, &merge_suggestions.merges);

            for err in &errors {
                eprintln!("Merge error: {}", err);
            }

            HttpResponse::Ok().json(serde_json::json!({
                "tags_merged": merged,
                "atoms_retagged": retagged
            }))
        }
        Err(e) => HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": e})),
    }
}
