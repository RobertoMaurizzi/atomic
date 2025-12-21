//! Ollama provider implementation

mod embedding;
mod llm;

use crate::providers::error::ProviderError;
use crate::providers::traits::{
    EmbeddingConfig, EmbeddingProvider, LlmConfig, LlmProvider, StreamCallback,
    StreamingLlmProvider,
};
use crate::providers::types::{CompletionResponse, Message, ToolDefinition};
use async_trait::async_trait;
use reqwest::Client;
use std::time::Duration;

/// Default Ollama server URL
pub const DEFAULT_OLLAMA_HOST: &str = "http://127.0.0.1:11434";

/// Ollama provider implementation
/// Supports embeddings, chat completions, streaming, tool calling, and structured outputs
pub struct OllamaProvider {
    client: Client,
    base_url: String,
}

impl OllamaProvider {
    pub fn new(base_url: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            client,
            base_url: base_url.unwrap_or_else(|| DEFAULT_OLLAMA_HOST.to_string()),
        }
    }

    /// Get the HTTP client
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Get the base URL
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Test connection to Ollama server
    pub async fn test_connection(&self) -> Result<bool, ProviderError> {
        let response = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await?;

        Ok(response.status().is_success())
    }
}

#[async_trait]
impl EmbeddingProvider for OllamaProvider {
    async fn embed_batch(
        &self,
        texts: &[String],
        config: &EmbeddingConfig,
    ) -> Result<Vec<Vec<f32>>, ProviderError> {
        embedding::embed_batch(self, texts, config).await
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn complete(
        &self,
        messages: &[Message],
        config: &LlmConfig,
    ) -> Result<CompletionResponse, ProviderError> {
        llm::complete(self, messages, config).await
    }
}

#[async_trait]
impl StreamingLlmProvider for OllamaProvider {
    async fn complete_streaming_with_tools(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        config: &LlmConfig,
        on_delta: StreamCallback,
    ) -> Result<CompletionResponse, ProviderError> {
        llm::complete_streaming_with_tools(self, messages, tools, config, on_delta).await
    }
}

/// Get the embedding dimension for a given Ollama model
pub fn get_embedding_dimension(model: &str) -> usize {
    // Normalize model name (remove :tag suffix for matching)
    let base_model = model.split(':').next().unwrap_or(model);

    match base_model {
        "nomic-embed-text" => 768,
        "mxbai-embed-large" => 1024,
        "all-minilm" => 384,
        "snowflake-arctic-embed" => 1024,
        "bge-m3" => 1024,
        "bge-large" => 1024,
        // Default to 768 for unknown embedding models
        _ => 768,
    }
}

/// Known embedding models in Ollama
pub fn is_embedding_model(model: &str) -> bool {
    let base_model = model.split(':').next().unwrap_or(model).to_lowercase();

    matches!(
        base_model.as_str(),
        "nomic-embed-text"
            | "mxbai-embed-large"
            | "all-minilm"
            | "snowflake-arctic-embed"
            | "bge-m3"
            | "bge-large"
    ) || base_model.contains("embed")
}
