//! Wiki article operations

use crate::db::Database;
use crate::models::{WikiArticleSummary, WikiArticleWithCitations};
use crate::providers::{ProviderConfig, ProviderType};
use crate::settings;
use crate::wiki;
use tauri::State;

/// Get all wiki articles (for list view)
#[tauri::command]
pub fn get_all_wiki_articles(db: State<Database>) -> Result<Vec<WikiArticleSummary>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    wiki::load_all_wiki_articles(&conn)
}

/// Get a wiki article for a tag (if it exists)
#[tauri::command]
pub fn get_wiki_article(
    db: State<Database>,
    tag_id: String,
) -> Result<Option<WikiArticleWithCitations>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    wiki::load_wiki_article(&conn, &tag_id)
}

/// Get the status of a wiki article for a tag
#[tauri::command]
pub fn get_wiki_article_status(
    db: State<Database>,
    tag_id: String,
) -> Result<crate::models::WikiArticleStatus, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    wiki::get_article_status(&conn, &tag_id)
}

/// Generate a new wiki article for a tag
#[tauri::command]
pub async fn generate_wiki_article(
    db: State<'_, Database>,
    tag_id: String,
    tag_name: String,
) -> Result<WikiArticleWithCitations, String> {
    // Get settings and prepare data
    let (provider_config, wiki_model) = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let settings_map = settings::get_all_settings(&conn)?;
        let provider_config = ProviderConfig::from_settings(&settings_map);

        // Validate provider configuration
        if provider_config.provider_type == ProviderType::OpenRouter
            && provider_config.openrouter_api_key.is_none()
        {
            return Err(
                "OpenRouter API key not configured. Please set it in Settings.".to_string(),
            );
        }

        // Use provider-appropriate model: Ollama uses its configured LLM, OpenRouter uses wiki_model setting
        let wiki_model = match provider_config.provider_type {
            ProviderType::Ollama => provider_config.llm_model().to_string(),
            ProviderType::OpenRouter => settings_map
                .get("wiki_model")
                .cloned()
                .unwrap_or_else(|| "anthropic/claude-sonnet-4".to_string()),
        };
        (provider_config, wiki_model)
    };

    let input = wiki::prepare_wiki_generation(db.as_core(), &provider_config, &tag_id, &tag_name).await?;

    // Generate article via API (async, no db lock needed)
    let result = wiki::generate_wiki_content(&provider_config, &input, &wiki_model).await?;

    // Save to database (sync, with db lock)
    {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        wiki::save_wiki_article(&conn, &result.article, &result.citations)?;
    }

    Ok(result)
}

/// Update an existing wiki article with new atoms
#[tauri::command]
pub async fn update_wiki_article(
    db: State<'_, Database>,
    tag_id: String,
    tag_name: String,
) -> Result<WikiArticleWithCitations, String> {
    // Get settings, existing article, and prepare update data (sync, with db lock)
    let (provider_config, wiki_model, existing, update_input) = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let settings_map = settings::get_all_settings(&conn)?;
        let provider_config = ProviderConfig::from_settings(&settings_map);

        // Validate provider configuration
        if provider_config.provider_type == ProviderType::OpenRouter
            && provider_config.openrouter_api_key.is_none()
        {
            return Err(
                "OpenRouter API key not configured. Please set it in Settings.".to_string(),
            );
        }

        // Use provider-appropriate model: Ollama uses its configured LLM, OpenRouter uses wiki_model setting
        let wiki_model = match provider_config.provider_type {
            ProviderType::Ollama => provider_config.llm_model().to_string(),
            ProviderType::OpenRouter => settings_map
                .get("wiki_model")
                .cloned()
                .unwrap_or_else(|| "anthropic/claude-sonnet-4".to_string()),
        };
        let existing = wiki::load_wiki_article(&conn, &tag_id)?;

        let update_input = if let Some(ref ex) = existing {
            wiki::prepare_wiki_update(&conn, &tag_id, &tag_name, &ex.article, &ex.citations)?
        } else {
            None
        };

        (provider_config, wiki_model, existing, update_input)
    };
    // Lock released here

    let existing = existing.ok_or("No existing article to update")?;

    // Check if there are new atoms to incorporate
    let input = match update_input {
        Some(input) => input,
        None => {
            // No new atoms, return existing article unchanged
            return Ok(existing);
        }
    };

    // Update article via API (async, no db lock needed)
    let result = wiki::update_wiki_content(&provider_config, &input, &wiki_model).await?;

    // Save to database (sync, with db lock)
    {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        wiki::save_wiki_article(&conn, &result.article, &result.citations)?;
    }

    Ok(result)
}

/// Delete a wiki article for a tag
#[tauri::command]
pub fn delete_wiki_article(db: State<Database>, tag_id: String) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    wiki::delete_article(&conn, &tag_id)
}
