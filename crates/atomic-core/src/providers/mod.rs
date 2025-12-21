//! Provider abstraction layer for AI services (embeddings, LLM completion)
//! Enables pluggable providers (OpenRouter, Ollama, etc.)

pub mod error;
pub mod models;
pub mod ollama;
pub mod openrouter;
pub mod traits;
pub mod types;

use std::collections::HashMap;
use std::sync::Arc;

pub use error::ProviderError;
pub use models::{fetch_and_return_capabilities, get_cached_capabilities_sync, save_capabilities_cache, AvailableModel};
pub use ollama::OllamaProvider;
pub use openrouter::OpenRouterProvider;
pub use traits::{EmbeddingConfig, EmbeddingProvider, LlmConfig, LlmProvider, StreamingLlmProvider};

/// Provider type enum
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderType {
    OpenRouter,
    Ollama,
}

impl ProviderType {
    pub fn from_string(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "ollama" => ProviderType::Ollama,
            _ => ProviderType::OpenRouter,
        }
    }
}

/// Provider configuration extracted from settings
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub provider_type: ProviderType,
    // OpenRouter settings
    pub openrouter_api_key: Option<String>,
    pub openrouter_embedding_model: String,
    pub openrouter_llm_model: String,
    // Ollama settings
    pub ollama_host: String,
    pub ollama_embedding_model: String,
    pub ollama_llm_model: String,
}

impl ProviderConfig {
    pub fn from_settings(settings: &HashMap<String, String>) -> Self {
        let provider_type = ProviderType::from_string(
            settings.get("provider").map(|s| s.as_str()).unwrap_or("openrouter")
        );

        ProviderConfig {
            provider_type,
            openrouter_api_key: settings.get("openrouter_api_key").cloned(),
            openrouter_embedding_model: settings.get("embedding_model")
                .cloned()
                .unwrap_or_else(|| "openai/text-embedding-3-small".to_string()),
            openrouter_llm_model: settings.get("tagging_model")
                .cloned()
                .unwrap_or_else(|| "openai/gpt-4o-mini".to_string()),
            ollama_host: settings.get("ollama_host")
                .cloned()
                .unwrap_or_else(|| "http://127.0.0.1:11434".to_string()),
            ollama_embedding_model: settings.get("ollama_embedding_model")
                .cloned()
                .unwrap_or_else(|| "nomic-embed-text".to_string()),
            ollama_llm_model: settings.get("ollama_llm_model")
                .cloned()
                .unwrap_or_else(|| "llama3.2".to_string()),
        }
    }

    /// Get the embedding model for the current provider
    pub fn embedding_model(&self) -> &str {
        match self.provider_type {
            ProviderType::OpenRouter => &self.openrouter_embedding_model,
            ProviderType::Ollama => &self.ollama_embedding_model,
        }
    }

    /// Get the LLM model for the current provider
    pub fn llm_model(&self) -> &str {
        match self.provider_type {
            ProviderType::OpenRouter => &self.openrouter_llm_model,
            ProviderType::Ollama => &self.ollama_llm_model,
        }
    }

    /// Get the embedding dimension for the current embedding model
    pub fn embedding_dimension(&self) -> usize {
        match self.provider_type {
            ProviderType::OpenRouter => {
                match self.openrouter_embedding_model.as_str() {
                    "openai/text-embedding-3-small" => 1536,
                    "openai/text-embedding-3-large" => 3072,
                    _ => 1536, // Default
                }
            }
            ProviderType::Ollama => {
                ollama::get_embedding_dimension(&self.ollama_embedding_model)
            }
        }
    }
}

/// Create an embedding provider based on configuration
pub fn create_embedding_provider(config: &ProviderConfig) -> Result<Arc<dyn EmbeddingProvider>, ProviderError> {
    match config.provider_type {
        ProviderType::OpenRouter => {
            let api_key = config.openrouter_api_key.clone()
                .ok_or_else(|| ProviderError::Configuration("OpenRouter API key not configured".to_string()))?;
            Ok(Arc::new(OpenRouterProvider::new(api_key)))
        }
        ProviderType::Ollama => {
            Ok(Arc::new(OllamaProvider::new(Some(config.ollama_host.clone()))))
        }
    }
}

/// Create an LLM provider based on configuration
pub fn create_llm_provider(config: &ProviderConfig) -> Result<Arc<dyn LlmProvider>, ProviderError> {
    match config.provider_type {
        ProviderType::OpenRouter => {
            let api_key = config.openrouter_api_key.clone()
                .ok_or_else(|| ProviderError::Configuration("OpenRouter API key not configured".to_string()))?;
            Ok(Arc::new(OpenRouterProvider::new(api_key)))
        }
        ProviderType::Ollama => {
            Ok(Arc::new(OllamaProvider::new(Some(config.ollama_host.clone()))))
        }
    }
}

/// Create a streaming LLM provider based on configuration
pub fn create_streaming_llm_provider(config: &ProviderConfig) -> Result<Arc<dyn StreamingLlmProvider>, ProviderError> {
    match config.provider_type {
        ProviderType::OpenRouter => {
            let api_key = config.openrouter_api_key.clone()
                .ok_or_else(|| ProviderError::Configuration("OpenRouter API key not configured".to_string()))?;
            Ok(Arc::new(OpenRouterProvider::new(api_key)))
        }
        ProviderType::Ollama => {
            Ok(Arc::new(OllamaProvider::new(Some(config.ollama_host.clone()))))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_error_is_retryable() {
        // Retryable errors
        let rate_limited = ProviderError::RateLimited {
            retry_after_secs: Some(30),
        };
        assert!(rate_limited.is_retryable(), "Rate limited should be retryable");

        let network_error = ProviderError::Network("connection refused".to_string());
        assert!(network_error.is_retryable(), "Network errors should be retryable");

        // Non-retryable errors
        let config_error = ProviderError::Configuration("missing API key".to_string());
        assert!(!config_error.is_retryable(), "Config errors should not be retryable");

        let api_error = ProviderError::Api {
            status: 400,
            message: "bad request".to_string(),
        };
        assert!(!api_error.is_retryable(), "API errors should not be retryable");

        let model_error = ProviderError::ModelNotFound("gpt-5".to_string());
        assert!(!model_error.is_retryable(), "Model not found should not be retryable");
    }

    #[test]
    fn test_provider_config_from_settings_openrouter() {
        let mut settings: HashMap<String, String> = HashMap::new();
        settings.insert("provider".to_string(), "openrouter".to_string());
        settings.insert("openrouter_api_key".to_string(), "test-key".to_string());
        settings.insert("embedding_model".to_string(), "openai/text-embedding-3-large".to_string());

        let config = ProviderConfig::from_settings(&settings);

        assert_eq!(config.provider_type, ProviderType::OpenRouter);
        assert_eq!(config.openrouter_api_key, Some("test-key".to_string()));
        assert_eq!(config.embedding_model(), "openai/text-embedding-3-large");
        assert_eq!(config.embedding_dimension(), 3072); // text-embedding-3-large
    }

    #[test]
    fn test_provider_config_from_settings_ollama() {
        let mut settings: HashMap<String, String> = HashMap::new();
        settings.insert("provider".to_string(), "ollama".to_string());
        settings.insert("ollama_host".to_string(), "http://localhost:11434".to_string());
        settings.insert("ollama_embedding_model".to_string(), "nomic-embed-text".to_string());
        settings.insert("ollama_llm_model".to_string(), "llama3.2".to_string());

        let config = ProviderConfig::from_settings(&settings);

        assert_eq!(config.provider_type, ProviderType::Ollama);
        assert_eq!(config.ollama_host, "http://localhost:11434");
        assert_eq!(config.embedding_model(), "nomic-embed-text");
        assert_eq!(config.llm_model(), "llama3.2");
    }

    #[test]
    fn test_provider_config_defaults() {
        // Empty settings should use defaults
        let settings: HashMap<String, String> = HashMap::new();
        let config = ProviderConfig::from_settings(&settings);

        assert_eq!(config.provider_type, ProviderType::OpenRouter); // Default
        assert_eq!(config.openrouter_embedding_model, "openai/text-embedding-3-small");
        assert_eq!(config.ollama_host, "http://127.0.0.1:11434");
    }
}
