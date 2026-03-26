//! Utility routes

use crate::db_extractor::Db;
use actix_web::HttpResponse;

#[utoipa::path(get, path = "/api/utils/sqlite-vec", responses((status = 200, description = "sqlite-vec version")), tag = "utils")]
pub async fn check_sqlite_vec(db: Db) -> HttpResponse {
    match db.0.check_sqlite_vec() {
        Ok(version) => HttpResponse::Ok().json(serde_json::json!({"version": version})),
        Err(e) => HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": format!("sqlite-vec not loaded: {}", e)})),
    }
}

#[utoipa::path(post, path = "/api/utils/compact-tags", responses((status = 200, description = "Tag compaction results")), tag = "utils")]
pub async fn compact_tags(db: Db) -> HttpResponse {
    let core = &db.0;

    let (provider_config, model) = {
        let settings_map = match core.get_settings() {
            Ok(s) => s,
            Err(e) => {
                return HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": e.to_string()}));
            }
        };
        let provider_config = atomic_core::ProviderConfig::from_settings(&settings_map);
        let model = match provider_config.provider_type {
            atomic_core::ProviderType::Ollama => provider_config.llm_model().to_string(),
            atomic_core::ProviderType::OpenAICompat => provider_config.llm_model().to_string(),
            atomic_core::ProviderType::OpenRouter => settings_map
                .get("tagging_model")
                .cloned()
                .unwrap_or_else(|| "openai/gpt-4o-mini".to_string()),
        };
        (provider_config, model)
    };

    let supported_params: Option<Vec<String>> =
        if provider_config.provider_type == atomic_core::ProviderType::OpenRouter {
            use atomic_core::providers::models::fetch_and_return_capabilities;

            let (cached, is_stale) = match core.get_cached_capabilities() {
                Ok(Some(cache)) => {
                    let stale = cache.is_stale();
                    (Some(cache), stale)
                }
                Ok(None) => (None, true),
                Err(_) => (None, true),
            };

            let capabilities = if is_stale {
                let client = reqwest::Client::new();
                match fetch_and_return_capabilities(&client).await {
                    Ok(fresh) => {
                        let _ = core.save_capabilities_cache(&fresh);
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

    let all_tags = match core.get_tags_for_compaction() {
        Ok(t) => t,
        Err(e) => {
            return HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()}));
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
            let result = match core.apply_tag_merges(&merge_suggestions.merges) {
                Ok(r) => r,
                Err(e) => {
                    return HttpResponse::InternalServerError()
                        .json(serde_json::json!({"error": e.to_string()}));
                }
            };

            HttpResponse::Ok().json(serde_json::json!({
                "tags_merged": result.tags_merged,
                "atoms_retagged": result.atoms_retagged
            }))
        }
        Err(e) => HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": e})),
    }
}
