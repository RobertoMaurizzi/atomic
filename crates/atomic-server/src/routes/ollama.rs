//! Ollama and provider routes

use crate::state::AppState;
use actix_web::{web, HttpResponse};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct TestOllamaBody {
    pub host: String,
}

pub async fn test_ollama(body: web::Json<TestOllamaBody>) -> HttpResponse {
    match atomic_core::providers::models::test_ollama_connection(&body.host).await {
        Ok(true) => HttpResponse::Ok().json(serde_json::json!({"success": true})),
        Ok(false) => HttpResponse::Ok().json(serde_json::json!({"success": false})),
        Err(e) => HttpResponse::BadGateway()
            .json(serde_json::json!({"error": e})),
    }
}

#[derive(Deserialize)]
pub struct OllamaHostQuery {
    pub host: Option<String>,
}

pub async fn get_ollama_models(query: web::Query<OllamaHostQuery>) -> HttpResponse {
    let host = query
        .host
        .as_deref()
        .unwrap_or("http://127.0.0.1:11434");
    match atomic_core::providers::models::fetch_ollama_models(host).await {
        Ok(models) => HttpResponse::Ok().json(models),
        Err(e) => HttpResponse::BadGateway()
            .json(serde_json::json!({"error": e})),
    }
}

pub async fn get_ollama_embedding_models(query: web::Query<OllamaHostQuery>) -> HttpResponse {
    let host = query
        .host
        .as_deref()
        .unwrap_or("http://127.0.0.1:11434");
    match atomic_core::providers::models::get_ollama_embedding_models(host).await {
        Ok(models) => HttpResponse::Ok().json(models),
        Err(e) => HttpResponse::BadGateway()
            .json(serde_json::json!({"error": e})),
    }
}

pub async fn get_ollama_llm_models(query: web::Query<OllamaHostQuery>) -> HttpResponse {
    let host = query
        .host
        .as_deref()
        .unwrap_or("http://127.0.0.1:11434");
    match atomic_core::providers::models::get_ollama_llm_models(host).await {
        Ok(models) => HttpResponse::Ok().json(models),
        Err(e) => HttpResponse::BadGateway()
            .json(serde_json::json!({"error": e})),
    }
}

pub async fn verify_provider_configured(state: web::Data<AppState>) -> HttpResponse {
    let settings = match state.core.get_settings() {
        Ok(s) => s,
        Err(e) => return crate::error::error_response(e),
    };

    let config = atomic_core::ProviderConfig::from_settings(&settings);

    let configured = match config.provider_type {
        atomic_core::ProviderType::OpenRouter => config
            .openrouter_api_key
            .as_ref()
            .map(|k| !k.is_empty())
            .unwrap_or(false),
        atomic_core::ProviderType::Ollama => !config.ollama_host.is_empty(),
    };

    HttpResponse::Ok().json(serde_json::json!({"configured": configured}))
}
