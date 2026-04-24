//! LLM Providers
//!
//! Protocol-based provider implementations.

pub mod anthropic;
pub mod ollama;
pub mod openai;

// Re-export providers
pub use anthropic::{anthropic_models, AnthropicConfig, AnthropicProvider};
pub use ollama::{ollama_models, OllamaConfig, OllamaProvider};
pub use openai::{openai_models, OpenAIConfig, OpenAIProvider};

/// Provider factory - creates providers by name from environment
///
/// NOTE: Gateway app no longer uses this. Kept for other modules.
pub struct ProviderFactory;

impl ProviderFactory {
    pub fn from_env(name: &str) -> Result<Box<dyn super::traits::LLMProvider>, String> {
        match name.to_lowercase().as_str() {
            "openai" | "chatgpt" | "kimi" | "moonshot" | "deepseek" | "zhipu" | "doubao"
            | "qwen" | "gemini" => {
                let provider = OpenAIProvider::from_env()
                    .map_err(|e| format!("Failed to create OpenAI-compatible provider: {}", e))?;
                Ok(Box::new(provider))
            }
            "claude" | "anthropic" => {
                let provider = AnthropicProvider::from_env()
                    .map_err(|e| format!("Failed to create Anthropic provider: {}", e))?;
                Ok(Box::new(provider))
            }
            "ollama" | "local" => {
                let provider = OllamaProvider::from_env()
                    .map_err(|e| format!("Failed to create Ollama provider: {}", e))?;
                Ok(Box::new(provider))
            }
            _ => Err(format!("Unknown provider: {}", name)),
        }
    }

    pub fn available_providers() -> Vec<&'static str> {
        vec!["openai", "anthropic", "ollama"]
    }
}
