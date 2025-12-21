//! Ollama embedding implementation

use crate::providers::error::ProviderError;
use crate::providers::ollama::OllamaProvider;
use crate::providers::traits::EmbeddingConfig;
use serde::{Deserialize, Serialize};

/// Ollama Embeddings API request
#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
}

/// Ollama Embeddings API response
#[derive(Deserialize)]
struct EmbeddingResponse {
    embeddings: Vec<Vec<f32>>,
}

/// Generate embeddings for multiple texts via Ollama API
pub async fn embed_batch(
    provider: &OllamaProvider,
    texts: &[String],
    config: &EmbeddingConfig,
) -> Result<Vec<Vec<f32>>, ProviderError> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }

    let request = EmbeddingRequest {
        model: config.model.clone(),
        input: texts.to_vec(),
    };

    let response = provider
        .client()
        .post(format!("{}/api/embed", provider.base_url()))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();

        return Err(ProviderError::Api {
            status,
            message: body,
        });
    }

    let embedding_response: EmbeddingResponse = response.json().await?;

    Ok(embedding_response.embeddings)
}
