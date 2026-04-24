//! LLM Module - Comprehensive LLM integration
//!
//! Provides a complete interface for interacting with Large Language Models,
//! with support for:
//! - Multiple providers (OpenAI-compatible, Anthropic, Ollama)
//! - Streaming responses
//! - Tool/function calling
//! - Multimodal inputs (text + images)
//! - Conversation management
//! - Retry and error handling
//!
//! # Quick Start - Simple Chat
//!
//! ```ignore
//! use beebotos_agents::llm::{create_openai_client, Message};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = create_openai_client().await?;
//!     let response = client.chat("Hello, how are you?").await?;
//!     println!("{}", response);
//!     Ok(())
//! }
//! ```

pub mod adapter;
pub mod client;
pub mod failover;
pub mod http_client;
pub mod providers;
pub mod traits;
pub mod types;

// Re-export HTTP client types
// Re-export main types
pub use adapter::{LLMClientAdapter, LegacyLLMClientBuilder};
pub use client::{ClientMetrics, LLMClient, LLMClientBuilder, ToolHandler};
// Re-export failover types
pub use failover::{FailoverConfig, FailoverProvider, FailoverProviderBuilder};
pub use http_client::{LLMHttpClient, OpenAIRequestBuilder, ProviderConfig, ProviderInitParams};
// Re-export model name modules
pub use providers::{anthropic_models, ollama_models, openai_models};
// Re-export providers
pub use providers::{
    AnthropicConfig, AnthropicProvider, OllamaConfig, OllamaProvider, OpenAIConfig, OpenAIProvider,
    ProviderFactory,
};
pub use traits::{
    ContextManager, LLMProvider, MetricsCollector, ModelCapabilities, ModelInfo,
    ProviderCapabilities, RetryPolicy, ToolExecutor,
};
pub use types::{
    Choice, Content, Delta, FunctionCall, FunctionChoice, FunctionDefinition, ImageUrlContent,
    LLMError, LLMRequest, LLMResponse, LLMResult, Message, RequestConfig, ResponseFormat, Role,
    StreamChoice, StreamChunk, Tool, ToolCall, ToolChoice, ToolResult, Usage,
};

/// Create an OpenAI-compatible client from environment variables
///
/// Expects OPENAI_API_KEY to be set
pub async fn create_openai_client() -> LLMResult<LLMClient> {
    let provider = OpenAIProvider::from_env()?;
    Ok(LLMClient::new(std::sync::Arc::new(provider)))
}

/// Create an Anthropic client from environment variables
///
/// Expects ANTHROPIC_API_KEY to be set
pub async fn create_anthropic_client() -> LLMResult<LLMClient> {
    let provider = AnthropicProvider::from_env()?;
    Ok(LLMClient::new(std::sync::Arc::new(provider)))
}

/// Create an Ollama client from environment variables
///
/// Expects OLLAMA_BASE_URL (optional, defaults to http://localhost:11434)
pub async fn create_ollama_client() -> LLMResult<LLMClient> {
    let provider = OllamaProvider::from_env()?;
    Ok(LLMClient::new(std::sync::Arc::new(provider)))
}

/// Version of the LLM module
pub const VERSION: &str = "1.0.0";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = Message::user("Hello");
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.text_content(), "Hello");
    }

    #[test]
    fn test_message_with_image() {
        let msg =
            Message::user("What's in this image?").with_image("https://example.com/image.png");

        assert_eq!(msg.content.len(), 2);
    }

    #[test]
    fn test_retry_policy() {
        let policy = RetryPolicy::default();

        // Should retry network errors
        let error = LLMError::Network("timeout".to_string());
        assert!(policy.should_retry(&error, 0));

        // Should not retry after max attempts
        assert!(!policy.should_retry(&error, 3));
    }
}
