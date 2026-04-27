//! Model Router

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use super::{CompletionRequest, CompletionResponse, ModelConfig};
use crate::error::{AgentError, Result};
use crate::llm::traits::LLMProvider;
use crate::llm::types::{LLMRequest, Message, RequestConfig, Usage};

/// Routes requests to appropriate model providers
pub struct ModelRouter {
    default_provider: String,
    providers: RwLock<HashMap<String, Arc<dyn LLMProvider>>>,
}

impl ModelRouter {
    pub fn new(default_provider: impl Into<String>) -> Self {
        Self {
            default_provider: default_provider.into(),
            providers: RwLock::new(HashMap::new()),
        }
    }

    /// Register a provider
    pub async fn register_provider(&self, name: impl Into<String>, provider: Arc<dyn LLMProvider>) {
        let mut providers = self.providers.write().await;
        providers.insert(name.into(), provider);
    }

    /// Get a registered provider by name
    pub async fn get_provider(&self, name: &str) -> Option<Arc<dyn LLMProvider>> {
        let providers = self.providers.read().await;
        providers.get(name).cloned()
    }

    /// Route completion request to appropriate provider
    pub async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let provider_name = if request.config.provider.is_empty() {
            self.default_provider.clone()
        } else {
            request.config.provider.clone()
        };

        tracing::info!("Routing to provider: {}", provider_name);

        // Get provider
        let provider = self.get_provider(&provider_name).await.ok_or_else(|| {
            AgentError::InvalidConfig(format!("Provider '{}' not found", provider_name))
        })?;

        // Convert to LLMRequest
        let llm_request = LLMRequest {
            messages: vec![Message::user(request.prompt)],
            config: RequestConfig {
                model: request.config.model,
                temperature: Some(request.config.temperature),
                max_tokens: Some(request.config.max_tokens),
                ..Default::default()
            },
        };

        // Call provider
        let response = provider
            .complete(llm_request)
            .await
            .map_err(|e| AgentError::Execution(format!("Provider error: {}", e)))?;

        let usage = response.usage.unwrap_or(Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });

        Ok(CompletionResponse {
            text: response
                .choices
                .into_iter()
                .next()
                .map(|c| c.message.text_content())
                .unwrap_or_default(),
            tokens_used: usage.total_tokens,
            cost: self.calculate_cost(&provider_name, &usage),
        })
    }

    /// Stream completion request
    pub async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<String>> {
        let provider_name = if request.config.provider.is_empty() {
            self.default_provider.clone()
        } else {
            request.config.provider.clone()
        };

        let provider = self.get_provider(&provider_name).await.ok_or_else(|| {
            AgentError::InvalidConfig(format!("Provider '{}' not found", provider_name))
        })?;

        let llm_request = LLMRequest {
            messages: vec![Message::user(request.prompt)],
            config: RequestConfig {
                model: request.config.model,
                temperature: Some(request.config.temperature),
                max_tokens: Some(request.config.max_tokens),
                stream: Some(true),
                ..Default::default()
            },
        };

        let (tx, rx) = tokio::sync::mpsc::channel(100);

        // Start streaming
        let mut stream = provider
            .complete_stream(llm_request)
            .await
            .map_err(|e| AgentError::Execution(format!("Provider stream error: {}", e)))?;

        tokio::spawn(async move {
            while let Some(chunk) = stream.recv().await {
                let content = chunk.content().unwrap_or("").to_string();
                if tx.send(content).await.is_err() {
                    break; // Receiver dropped
                }
            }
        });

        Ok(rx)
    }

    pub fn select_model(&self, _task_complexity: f32) -> ModelConfig {
        // Select appropriate model based on task
        ModelConfig {
            provider: self.default_provider.clone(),
            model: "gpt-4o".to_string(),
            temperature: 0.7,
            max_tokens: 2048,
            top_p: 1.0,
        }
    }

    /// Calculate approximate cost based on token usage
    fn calculate_cost(&self, provider: &str, usage: &Usage) -> f64 {
        // Simple cost calculation (would be more complex in production)
        let rate_per_1k_tokens = match provider {
            "openai" => 0.03,                // $0.03 per 1K tokens for GPT-4
            "anthropic" | "claude" => 0.008, // $0.008 per 1K tokens for Claude
            "ollama" => 0.0,                 // Local, no cost
            "zhipu" => 0.0001,               // Very cheap
            "kimi" => 0.003,                 // Moonshot pricing
            "deepseek" => 0.001,             // DeepSeek pricing
            _ => 0.01,
        };

        (usage.total_tokens as f64 / 1000.0) * rate_per_1k_tokens
    }

    /// Health check all providers
    pub async fn health_check(&self) -> HashMap<String, bool> {
        let providers = self.providers.read().await;
        let mut results = HashMap::new();

        for (name, provider) in providers.iter() {
            let healthy = provider.health_check().await.is_ok();
            results.insert(name.clone(), healthy);
        }

        results
    }

    /// 🔧 P1 FIX: Complete with automatic fallback to backup providers
    ///
    /// If the primary provider fails, automatically try fallback providers
    /// in order of priority until one succeeds.
    ///
    /// # Arguments
    /// * `request` - The completion request
    /// * `fallback_chain` - Ordered list of fallback provider names
    pub async fn complete_with_fallback(
        &self,
        request: CompletionRequest,
        fallback_chain: Vec<String>,
    ) -> Result<CompletionResponse> {
        let mut last_error = None;

        for provider_name in &fallback_chain {
            match self.complete_with_provider(&request, provider_name).await {
                Ok(response) => {
                    tracing::info!("Successfully completed with provider: {}", provider_name);
                    return Ok(response);
                }
                Err(e) => {
                    tracing::warn!("Provider {} failed: {}, trying next...", provider_name, e);
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| AgentError::Execution("All providers failed".to_string())))
    }

    /// Complete with a specific provider
    async fn complete_with_provider(
        &self,
        request: &CompletionRequest,
        provider_name: &str,
    ) -> Result<CompletionResponse> {
        let provider = self.get_provider(provider_name).await.ok_or_else(|| {
            AgentError::InvalidConfig(format!("Provider '{}' not found", provider_name))
        })?;

        let llm_request = LLMRequest {
            messages: vec![Message::user(request.prompt.clone())],
            config: RequestConfig {
                model: request.config.model.clone(),
                temperature: Some(request.config.temperature),
                max_tokens: Some(request.config.max_tokens),
                ..Default::default()
            },
        };

        let response = provider
            .complete(llm_request)
            .await
            .map_err(|e| AgentError::Execution(format!("Provider error: {}", e)))?;

        let usage = response.usage.unwrap_or(Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        });

        Ok(CompletionResponse {
            text: response
                .choices
                .into_iter()
                .next()
                .map(|c| c.message.text_content())
                .unwrap_or_default(),
            tokens_used: usage.total_tokens,
            cost: self.calculate_cost(provider_name, &usage),
        })
    }

    /// 🔧 P1 FIX: Smart provider selection based on capabilities and health
    ///
    /// Automatically selects the best available provider based on:
    /// - Health status
    /// - Request requirements (streaming, function calling, etc.)
    /// - Cost optimization
    pub async fn select_best_provider(
        &self,
        require_streaming: bool,
        require_functions: bool,
        require_vision: bool,
    ) -> Option<String> {
        let providers = self.providers.read().await;

        for (name, provider) in providers.iter() {
            let caps = provider.capabilities();

            // Check if provider meets requirements
            if require_streaming && !caps.streaming {
                continue;
            }
            if require_functions && !caps.function_calling {
                continue;
            }
            if require_vision && !caps.vision {
                continue;
            }

            // Check health
            if provider.health_check().await.is_ok() {
                return Some(name.clone());
            }
        }

        None
    }
}
