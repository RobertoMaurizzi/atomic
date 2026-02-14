//! Settings and model discovery operations

use crate::db::{Database, SharedDatabase};
use crate::providers::{
    fetch_and_return_capabilities, get_cached_capabilities_sync, save_capabilities_cache,
    AvailableModel,
};
use crate::settings;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn get_settings(db: State<Database>) -> Result<HashMap<String, String>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    settings::get_all_settings(&conn)
}

#[tauri::command]
pub async fn set_setting(
    app_handle: AppHandle,
    db: State<'_, Database>,
    shared_db: State<'_, SharedDatabase>,
    key: String,
    value: String,
) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    // Check if this setting change affects embedding dimensions
    // This includes: provider, embedding_model, ollama_embedding_model
    let dimension_affecting_keys = ["provider", "embedding_model", "ollama_embedding_model"];

    let mut dimension_changed = false;

    if dimension_affecting_keys.contains(&key.as_str()) {
        let (will_change, new_dim) = atomic_core::db::will_dimension_change(&conn, &key, &value);

        if will_change {
            let current_dim = atomic_core::db::get_current_embedding_dimension(&conn);
            eprintln!(
                "Embedding dimension changing from {} to {} due to {} change - recreating vec_chunks",
                current_dim, new_dim, key
            );
            atomic_core::db::recreate_vec_chunks_with_dimension(&conn, new_dim)
                .map_err(|e| e.to_string())?;
            dimension_changed = true;
        }
    }

    settings::set_setting(&conn, &key, &value)?;

    // If dimension changed, emit event and trigger re-embedding
    if dimension_changed {
        // Count how many atoms need re-embedding
        let pending_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM atoms WHERE embedding_status = 'pending'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        eprintln!(
            "Dimension changed - {} atoms marked as pending, emitting event and triggering re-embedding",
            pending_count
        );

        // Emit event to notify frontend that atoms need to be re-fetched
        let _ = app_handle.emit(
            "embeddings-reset",
            serde_json::json!({
                "pending_count": pending_count,
                "reason": format!("{} changed", key)
            }),
        );

        // Get pending atoms and trigger re-embedding
        if pending_count > 0 {
            let mut stmt = conn
                .prepare(
                    "UPDATE atoms SET embedding_status = 'processing'
                     WHERE embedding_status IN ('pending', 'processing')
                     RETURNING id, content",
                )
                .map_err(|e| e.to_string())?;

            let pending_atoms: Vec<(String, String)> = stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;

            drop(stmt);
            drop(conn);

            // Spawn embedding batch processing (skip tagging - tags are preserved)
            tokio::spawn(crate::embedding::process_embedding_batch(
                app_handle,
                Arc::clone(&shared_db),
                pending_atoms,
                true, // skip tagging - re-embedding only, tags preserved
            ));
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn test_openrouter_connection(api_key: String) -> Result<bool, String> {
    let client = reqwest::Client::new();

    let response = client
        .post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": "anthropic/claude-haiku-4.5",
            "messages": [
                {
                    "role": "user",
                    "content": "Hi"
                }
            ],
            "max_tokens": 5
        }))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if response.status().is_success() {
        Ok(true)
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(format!("API error ({}): {}", status, body))
    }
}

/// Get available LLM models that support structured outputs
/// Uses cached capabilities if fresh, otherwise fetches from OpenRouter API
#[tauri::command]
pub async fn get_available_llm_models(
    db: State<'_, Database>,
) -> Result<Vec<AvailableModel>, String> {
    // Check cache first (sync DB access)
    let (cached, is_stale) = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        match get_cached_capabilities_sync(&conn) {
            Ok(Some(cache)) => (Some(cache.clone()), cache.is_stale()),
            Ok(None) => (None, true),
            Err(_) => (None, true),
        }
    };

    // If cache is fresh, return from cache
    if let Some(ref cache) = cached {
        if !is_stale {
            return Ok(cache.get_models_with_structured_outputs());
        }
    }

    // Fetch fresh capabilities from API
    let client = reqwest::Client::new();
    match fetch_and_return_capabilities(&client).await {
        Ok(fresh_cache) => {
            // Save to database
            if let Ok(conn) = db.new_connection() {
                let _ = save_capabilities_cache(&conn, &fresh_cache);
            }
            Ok(fresh_cache.get_models_with_structured_outputs())
        }
        Err(e) => {
            // If we have a stale cache, use it as fallback
            if let Some(cache) = cached {
                Ok(cache.get_models_with_structured_outputs())
            } else {
                Err(format!("Failed to fetch models: {}", e))
            }
        }
    }
}
