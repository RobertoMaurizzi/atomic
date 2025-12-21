//! Provider trait definitions

use crate::providers::error::ProviderError;
use crate::providers::types::{
    CompletionResponse, GenerationParams, Message, StreamDelta, ToolDefinition,
};
use async_trait::async_trait;

/// Configuration for embedding requests
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    pub model: String,
}

impl EmbeddingConfig {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
        }
    }
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model: "openai/text-embedding-3-small".to_string(),
        }
    }
}

/// Configuration for LLM requests
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub model: String,
    pub params: GenerationParams,
}

impl LlmConfig {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            params: GenerationParams::default(),
        }
    }

    pub fn with_params(mut self, params: GenerationParams) -> Self {
        self.params = params;
        self
    }
}

/// Provider that can generate text embeddings
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embeddings for multiple texts (batch)
    async fn embed_batch(
        &self,
        texts: &[String],
        config: &EmbeddingConfig,
    ) -> Result<Vec<Vec<f32>>, ProviderError>;
}

/// Provider that can generate text completions
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Generate a completion for the given messages
    async fn complete(
        &self,
        messages: &[Message],
        config: &LlmConfig,
    ) -> Result<CompletionResponse, ProviderError>;
}

/// Callback type for streaming deltas
pub type StreamCallback = Box<dyn Fn(StreamDelta) + Send + Sync>;

/// Provider that supports streaming completions
#[async_trait]
pub trait StreamingLlmProvider: LlmProvider {
    /// Generate a streaming completion with tools
    async fn complete_streaming_with_tools(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        config: &LlmConfig,
        on_delta: StreamCallback,
    ) -> Result<CompletionResponse, ProviderError>;
}
