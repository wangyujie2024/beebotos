//! Models Module
//!
//! LLM provider management and routing.
//!
//! # Providers
//! Providers are now located in `crate::llm::providers`.
//! Use `crate::llm::providers` for direct provider access.

use serde::{Deserialize, Serialize};

pub mod converter;
pub mod cost;
pub mod failover;
pub mod router;

// Re-export providers from llm module for backward compatibility
pub use converter::{
    ConverterConfig, FunctionCall, LLMContent, LLMMessage, ModelInputConverter, ToolCall,
};
pub use cost::CostTracker;
pub use failover::FailoverManager;
pub use router::ModelRouter;

pub use crate::llm::providers::*;

/// Model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelConfig {
    pub provider: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub top_p: f32,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            temperature: 0.7,
            max_tokens: 2048,
            top_p: 1.0,
        }
    }
}

impl ModelConfig {
    /// 🟡 P1 FIX: Validate model configuration
    pub fn validate(&self) -> Result<(), crate::error::AgentError> {
        if self.temperature < 0.0 || self.temperature > 2.0 {
            return Err(crate::error::AgentError::InvalidConfig(format!(
                "Temperature must be between 0.0 and 2.0, got {}",
                self.temperature
            )));
        }
        if self.max_tokens == 0 {
            return Err(crate::error::AgentError::InvalidConfig(
                "max_tokens cannot be 0".to_string(),
            ));
        }
        Ok(())
    }
}

/// Completion request
#[derive(Debug)]
pub struct CompletionRequest {
    pub prompt: String,
    pub config: ModelConfig,
}

/// Completion response
#[derive(Debug)]
pub struct CompletionResponse {
    pub text: String,
    pub tokens_used: u32,
    pub cost: f64,
}
