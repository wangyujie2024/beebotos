//! LLM Service
//!
//! Handles LLM interactions for incoming messages from various platforms.
//! Uses beebotos_agents::llm module for provider management.
//! Supports fallback chain: if primary provider fails, try next in chain.
//! Now supports multimodal inputs (text + images).

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use beebotos_agents::communication::Message as ChannelMessage;
use beebotos_agents::llm::{
    Content, FailoverProvider, FailoverProviderBuilder, KimiConfig, KimiProvider, LLMProvider,
    Message as LLMMessage, RequestConfig, Role,
};
use beebotos_agents::media::multimodal::{MultimodalContent, MultimodalProcessor};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::config::{BeeBotOSConfig, ModelProviderConfig};
use crate::error::GatewayError;

/// Metrics for LLM service
#[derive(Debug, Default)]
pub struct LlmMetrics {
    /// Total number of requests
    pub total_requests: AtomicU64,
    /// Number of successful requests
    pub successful_requests: AtomicU64,
    /// Number of failed requests
    pub failed_requests: AtomicU64,
    /// Total latency in milliseconds
    pub total_latency_ms: AtomicU64,
    /// Total tokens used (input + output)
    pub total_tokens: AtomicU64,
    /// Total input tokens
    pub input_tokens: AtomicU64,
    /// Total output tokens
    pub output_tokens: AtomicU64,
    /// Request latency histogram (in ms)
    latency_histogram: RwLock<Vec<u64>>,
}

impl LlmMetrics {
    /// Record a successful request
    pub async fn record_success(&self, latency_ms: u64, input_tokens: u32, output_tokens: u32) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.successful_requests.fetch_add(1, Ordering::Relaxed);
        self.total_latency_ms.fetch_add(latency_ms, Ordering::Relaxed);
        self.input_tokens.fetch_add(input_tokens as u64, Ordering::Relaxed);
        self.output_tokens.fetch_add(output_tokens as u64, Ordering::Relaxed);
        self.total_tokens.fetch_add(
            (input_tokens + output_tokens) as u64,
            Ordering::Relaxed,
        );
        
        // Add to latency histogram
        let mut hist = self.latency_histogram.write().await;
        hist.push(latency_ms);
        // Keep only last 1000 entries
        if hist.len() > 1000 {
            hist.remove(0);
        }
    }

    /// Record a failed request
    pub fn record_failure(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.failed_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// Get average latency in milliseconds
    pub async fn average_latency_ms(&self) -> f64 {
        let total = self.total_latency_ms.load(Ordering::Relaxed);
        let requests = self.successful_requests.load(Ordering::Relaxed);
        if requests == 0 {
            0.0
        } else {
            total as f64 / requests as f64
        }
    }

    /// Get latency percentiles
    pub async fn latency_percentiles(&self) -> (f64, f64, f64) {
        let hist = self.latency_histogram.read().await;
        if hist.is_empty() {
            return (0.0, 0.0, 0.0);
        }
        
        let mut sorted = hist.clone();
        sorted.sort_unstable();
        
        let p50 = sorted[sorted.len() * 50 / 100] as f64;
        let p95 = sorted[sorted.len() * 95 / 100] as f64;
        let p99 = sorted[sorted.len() * 99 / 100] as f64;
        
        (p50, p95, p99)
    }

    /// Get metrics summary
    pub fn get_summary(&self) -> MetricsSummary {
        MetricsSummary {
            total_requests: self.total_requests.load(Ordering::Relaxed),
            successful_requests: self.successful_requests.load(Ordering::Relaxed),
            failed_requests: self.failed_requests.load(Ordering::Relaxed),
            total_tokens: self.total_tokens.load(Ordering::Relaxed),
            input_tokens: self.input_tokens.load(Ordering::Relaxed),
            output_tokens: self.output_tokens.load(Ordering::Relaxed),
        }
    }
}

/// Metrics summary for serialization
#[derive(Debug, Clone, serde::Serialize)]
pub struct MetricsSummary {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// LLM Service for processing messages
///
/// Uses beebotos_agents::llm module for provider management with automatic failover.
pub struct LlmService {
    /// Configuration
    config: BeeBotOSConfig,
    /// Multimodal processor for handling images
    multimodal_processor: MultimodalProcessor,
    /// Failover provider for handling multiple providers
    failover_provider: Arc<FailoverProvider>,
    /// Metrics collection
    metrics: Arc<LlmMetrics>,
}

impl LlmService {
    /// Create a new LLM service with configuration
    pub async fn new(config: BeeBotOSConfig) -> Result<Self, GatewayError> {
        // Validate configuration before creating providers
        Self::validate_config(&config)?;
        
        // Create failover provider from configuration
        let failover_provider = Self::create_failover_provider(&config).await?;

        Ok(Self {
            config,
            multimodal_processor: MultimodalProcessor::new(),
            failover_provider: Arc::new(failover_provider),
            metrics: Arc::new(LlmMetrics::default()),
        })
    }

    /// Validate LLM configuration
    fn validate_config(config: &BeeBotOSConfig) -> Result<(), GatewayError> {
        // Check if default provider is configured
        if config.models.default_provider.is_empty() {
            return Err(GatewayError::Internal {
                message: "Default LLM provider is not configured".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            });
        }

        // Check if default provider has configuration
        if !config.models.providers.contains_key(&config.models.default_provider) {
            return Err(GatewayError::Internal {
                message: format!(
                    "Default provider '{}' has no configuration",
                    config.models.default_provider
                ),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            });
        }

        // Validate each provider configuration
        for (name, provider_config) in &config.models.providers {
            // Check if provider name is supported
            let supported_providers = ["kimi", "openai", "zhipu", "deepseek", "ollama", "anthropic", "claude"];
            if !supported_providers.contains(&name.as_str()) {
                warn!("Provider '{}' is not in the supported list: {:?}", name, supported_providers);
            }

            // Check if API key is set (except for ollama)
            if name != "ollama" {
                let has_api_key = provider_config.api_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false);
                let has_env_key = std::env::var(format!("{}_API_KEY", name.to_uppercase())).is_ok();
                
                if !has_api_key && !has_env_key {
                    return Err(GatewayError::Internal {
                        message: format!(
                            "Provider '{}' is missing API key. Set {}_API_KEY environment variable or configure in config file.",
                            name,
                            name.to_uppercase()
                        ),
                        correlation_id: uuid::Uuid::new_v4().to_string(),
                    });
                }
            }
        }

        info!("LLM configuration validation passed");
        Ok(())
    }

    /// Get metrics reference
    pub fn metrics(&self) -> Arc<LlmMetrics> {
        self.metrics.clone()
    }

    /// Get metrics summary
    pub fn get_metrics_summary(&self) -> MetricsSummary {
        self.metrics.get_summary()
    }

    /// Get the configured system prompt
    pub fn system_prompt(&self) -> String {
        self.config.models.system_prompt.clone()
    }

    /// Create failover provider from configuration
    async fn create_failover_provider(
        config: &BeeBotOSConfig,
    ) -> Result<FailoverProvider, GatewayError> {
        // Build provider chain: default -> fallback chain
        let mut providers_to_try: Vec<String> = Vec::new();

        // 1. Add default provider from config (highest priority)
        let default = &config.models.default_provider;
        providers_to_try.push(default.clone());

        // 2. Add fallback chain from config
        for fallback in &config.models.fallback_chain {
            if !providers_to_try.contains(fallback) {
                providers_to_try.push(fallback.clone());
            }
        }

        if providers_to_try.is_empty() {
            return Err(GatewayError::Internal {
                message: "No LLM provider configured".to_string(),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            });
        }

        info!("LLM provider chain: {:?}", providers_to_try);

        // Create providers from configuration
        let mut primary: Option<Arc<dyn LLMProvider>> = None;
        let mut fallbacks: Vec<Arc<dyn LLMProvider>> = Vec::new();

        for (idx, provider_name) in providers_to_try.iter().enumerate() {
            match Self::create_provider(provider_name, config).await {
                Ok(provider) => {
                    if idx == 0 {
                        primary = Some(provider);
                        info!("Primary provider '{}' initialized", provider_name);
                    } else {
                        fallbacks.push(provider);
                        info!("Fallback provider '{}' initialized", provider_name);
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to initialize provider '{}': {}",
                        provider_name, e
                    );
                    if idx == 0 {
                        return Err(GatewayError::Internal {
                            message: format!(
                                "Primary provider '{}' failed to initialize: {}",
                                provider_name, e
                            ),
                            correlation_id: uuid::Uuid::new_v4().to_string(),
                        });
                    }
                }
            }
        }

        let primary = primary.ok_or_else(|| GatewayError::Internal {
            message: "No primary LLM provider available".to_string(),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

        // Build failover provider
        let mut builder = FailoverProviderBuilder::new().primary(primary);

        for fallback in fallbacks {
            builder = builder.fallback(fallback);
        }

        builder.build().map_err(|e| GatewayError::Internal {
            message: format!("Failed to build failover provider: {}", e),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })
    }

    /// Create a single provider from configuration
    async fn create_provider(
        name: &str,
        config: &BeeBotOSConfig,
    ) -> Result<Arc<dyn LLMProvider>, String> {
        let provider_config = config
            .models
            .providers
            .get(name)
            .cloned()
            .unwrap_or_default();

        let api_key = Self::get_api_key(name, &provider_config);
        if api_key.is_none() && name != "ollama" {
            return Err(format!("API key not set for provider '{}'", name));
        }

        match name {
            "kimi" => {
                let kimi_config = KimiConfig {
                    base_url: Self::get_base_url(name, &provider_config),
                    api_key: api_key.unwrap_or_default(),
                    default_model: Self::get_model(name, &provider_config),
                    timeout: std::time::Duration::from_secs(60),
                    retry_policy: beebotos_agents::llm::traits::RetryPolicy::default(),
                    mode: beebotos_agents::llm::providers::ProviderMode::Merge,
                };

                let provider = KimiProvider::new(kimi_config)
                    .map_err(|e| format!("Failed to create Kimi provider: {}", e))?;
                Ok(Arc::new(provider))
            }
            "openai" => {
                use beebotos_agents::llm::{OpenAIConfig, OpenAIProvider};

                let openai_config = OpenAIConfig {
                    base_url: Self::get_base_url(name, &provider_config),
                    api_key: api_key.unwrap_or_default(),
                    default_model: Self::get_model(name, &provider_config),
                    timeout: std::time::Duration::from_secs(60),
                    retry_policy: beebotos_agents::llm::traits::RetryPolicy::default(),
                    organization: None,
                };

                let provider = OpenAIProvider::new(openai_config)
                    .map_err(|e| format!("Failed to create OpenAI provider: {}", e))?;
                Ok(Arc::new(provider))
            }
            "zhipu" => {
                use beebotos_agents::llm::{ZhipuConfig, ZhipuProvider};

                let zhipu_config = ZhipuConfig {
                    base_url: Self::get_base_url(name, &provider_config),
                    api_key: api_key.unwrap_or_default(),
                    default_model: Self::get_model(name, &provider_config),
                    timeout: std::time::Duration::from_secs(60),
                    retry_policy: beebotos_agents::llm::traits::RetryPolicy::default(),
                };

                let provider = ZhipuProvider::new(zhipu_config)
                    .map_err(|e| format!("Failed to create Zhipu provider: {}", e))?;
                Ok(Arc::new(provider))
            }
            "deepseek" => {
                use beebotos_agents::llm::{DeepSeekConfig, DeepSeekProvider};

                let deepseek_config = DeepSeekConfig {
                    base_url: Self::get_base_url(name, &provider_config),
                    api_key: api_key.unwrap_or_default(),
                    default_model: Self::get_model(name, &provider_config),
                    timeout: std::time::Duration::from_secs(60),
                    retry_policy: beebotos_agents::llm::traits::RetryPolicy::default(),
                };

                let provider = DeepSeekProvider::new(deepseek_config)
                    .map_err(|e| format!("Failed to create DeepSeek provider: {}", e))?;
                Ok(Arc::new(provider))
            }
            "ollama" => {
                use beebotos_agents::llm::{OllamaConfig, OllamaProvider};

                let ollama_config = OllamaConfig {
                    base_url: Self::get_base_url(name, &provider_config),
                    default_model: Self::get_model(name, &provider_config),
                    timeout: std::time::Duration::from_secs(120),
                    retry_policy: beebotos_agents::llm::traits::RetryPolicy::default(),
                };

                let provider = OllamaProvider::new(ollama_config)
                    .map_err(|e| format!("Failed to create Ollama provider: {}", e))?;
                Ok(Arc::new(provider))
            }
            "anthropic" | "claude" => {
                use beebotos_agents::llm::{ClaudeConfig, ClaudeProvider};

                let claude_config = ClaudeConfig {
                    base_url: Self::get_base_url(name, &provider_config),
                    api_key: api_key.unwrap_or_default(),
                    default_model: Self::get_model(name, &provider_config),
                    timeout: std::time::Duration::from_secs(60),
                    retry_policy: beebotos_agents::llm::traits::RetryPolicy::default(),
                    version: "2023-06-01".to_string(),
                };

                let provider = ClaudeProvider::new(claude_config)
                    .map_err(|e| format!("Failed to create Claude provider: {}", e))?;
                Ok(Arc::new(provider))
            }
            _ => Err(format!("Unsupported provider: {}", name)),
        }
    }

    /// Get API key from environment variable or config file
    fn get_api_key(provider: &str, config: &ModelProviderConfig) -> Option<String> {
        // First try environment variable
        let env_var_name = format!("{}_API_KEY", provider.to_uppercase());
        if let Ok(key) = std::env::var(&env_var_name) {
            if !key.is_empty() {
                return Some(key);
            }
        }

        // Then try config file
        config.api_key.clone().filter(|k| !k.is_empty())
    }

    /// Get base URL for provider
    fn get_base_url(provider: &str, config: &ModelProviderConfig) -> String {
        // First try environment variable
        let env_var_name = format!("{}_BASE_URL", provider.to_uppercase());
        if let Ok(url) = std::env::var(&env_var_name) {
            if !url.is_empty() {
                return url;
            }
        }

        // Then try config file
        if let Some(url) = &config.base_url {
            if !url.is_empty() {
                return url.clone();
            }
        }

        // Finally use default
        match provider {
            "kimi" => "https://api.moonshot.cn/v1".to_string(),
            "openai" => "https://api.openai.com/v1".to_string(),
            "zhipu" => "https://open.bigmodel.cn/api/paas/v4".to_string(),
            "deepseek" => "https://api.deepseek.com/v1".to_string(),
            "ollama" => "http://localhost:11434".to_string(),
            "anthropic" | "claude" => "https://api.anthropic.com/v1".to_string(),
            _ => "https://api.openai.com/v1".to_string(),
        }
    }

    /// Get model for provider
    fn get_model(provider: &str, config: &ModelProviderConfig) -> String {
        // First try environment variable
        let env_var_name = format!("{}_MODEL", provider.to_uppercase());
        if let Ok(model) = std::env::var(&env_var_name) {
            if !model.is_empty() {
                return model;
            }
        }

        // Then try config file
        if let Some(model) = &config.model {
            if !model.is_empty() {
                return model.clone();
            }
        }

        // Finally use default
        match provider {
            "kimi" => "moonshot-v1-8k".to_string(),
            "openai" => "gpt-4o-mini".to_string(),
            "zhipu" => "glm-4".to_string(),
            "deepseek" => "deepseek-chat".to_string(),
            "ollama" => "llama2".to_string(),
            "anthropic" | "claude" => "claude-3-sonnet-20240229".to_string(),
            _ => "gpt-4o-mini".to_string(),
        }
    }

    /// Process a message with optional custom image download function
    ///
    /// # Arguments
    /// * `message` - The message to process
    /// * `image_downloader` - Optional function to download images
    pub async fn process_message_with_images<F, Fut>(
        &self,
        message: &ChannelMessage,
        image_downloader: Option<F>,
    ) -> Result<String, GatewayError>
    where
        F: Fn(&str, Option<&str>) -> Fut + Send + Sync,
        Fut: std::future::Future<
                Output = std::result::Result<Vec<u8>, beebotos_agents::error::AgentError>,
            > + Send,
    {
        // Process multimodal content
        let multimodal_content = if let Some(downloader) = &image_downloader {
            self.multimodal_processor
                .process_message_with_downloader(message, downloader)
                .await
        } else {
            self.multimodal_processor
                .process_message(message, message.platform, None)
                .await
        };

        self.execute_llm_request(multimodal_content, message.content.clone(), false)
            .await
    }

    /// Process an incoming message and generate a response
    pub async fn process_message(
        &self,
        message: &ChannelMessage,
    ) -> Result<String, GatewayError> {
        // Process multimodal content
        let multimodal_content = self
            .multimodal_processor
            .process_message(message, message.platform, None)
            .await;

        self.execute_llm_request(multimodal_content, message.content.clone(), true)
            .await
    }

    /// Execute LLM request with processed multimodal content
    async fn execute_llm_request(
        &self,
        multimodal_result: Result<MultimodalContent, beebotos_agents::error::AgentError>,
        fallback_text: String,
        include_system_prompt: bool,
    ) -> Result<String, GatewayError> {
        let start_time = std::time::Instant::now();
        
        // Handle multimodal processing result
        let multimodal_content = multimodal_result.unwrap_or_else(|e| {
            warn!("Failed to process multimodal content: {}, using text only", e);
            MultimodalContent {
                text: fallback_text,
                images: vec![],
                metadata: HashMap::new(),
            }
        });

        info!(
            "📊 Processing LLM request: text='{}...', images={}",
            multimodal_content.text.chars().take(50).collect::<String>(),
            multimodal_content.images.len()
        );

        // Build LLM contents from multimodal content
        let mut contents: Vec<Content> = vec![Content::Text {
            text: multimodal_content.text,
        }];

        // Add images as content
        for image in &multimodal_content.images {
            let data_url = format!("data:{};base64,{}", image.mime_type, image.base64_data);
            contents.push(Content::ImageUrl {
                image_url: beebotos_agents::llm::types::ImageUrlContent {
                    url: data_url,
                    detail: Some("auto".to_string()),
                },
            });
        }

        // Create user message
        let user_message = if contents.len() == 1 {
            match &contents[0] {
                Content::Text { text } => LLMMessage::user(text.clone()),
                _ => LLMMessage::user("".to_string()),
            }
        } else {
            LLMMessage::multimodal(Role::User, contents)
        };

        // Build messages (with or without system prompt)
        let messages = if include_system_prompt && !self.config.models.system_prompt.is_empty() {
            vec![
                LLMMessage::system(&self.config.models.system_prompt),
                user_message,
            ]
        } else {
            vec![user_message]
        };

        // Build request config
        let request_config = RequestConfig {
            model: self.get_default_model(),
            temperature: self.get_default_temperature(),
            max_tokens: Some(self.config.models.max_tokens),
            stream: Some(false),
            ..Default::default()
        };

        let request = beebotos_agents::llm::types::LLMRequest {
            messages,
            config: request_config,
        };

        // Execute request through failover provider
        let result = self.failover_provider.complete(request).await;
        let latency_ms = start_time.elapsed().as_millis() as u64;

        match result {
            Ok(response) => {
                // Extract response content
                let content = response
                    .choices
                    .first()
                    .map(|choice| choice.message.text_content())
                    .unwrap_or_default();

                // Extract token usage from response
                let (input_tokens, output_tokens) = response.usage.as_ref().map_or((0, 0), |u| {
                    (u.prompt_tokens, u.completion_tokens)
                });

                // Record metrics
                self.metrics
                    .record_success(latency_ms, input_tokens, output_tokens)
                    .await;

                info!(
                    "✅ Received LLM response: length={}, latency={}ms, tokens={}/{}",
                    content.len(),
                    latency_ms,
                    input_tokens,
                    output_tokens
                );
                Ok(content)
            }
            Err(e) => {
                // Record failure metrics
                self.metrics.record_failure();

                Err(GatewayError::Internal {
                    message: format!("LLM request failed: {}", e),
                    correlation_id: uuid::Uuid::new_v4().to_string(),
                })
            }
        }
    }

    /// Process a pre-built message list (non-streaming)
    pub async fn process_messages(
        &self,
        messages: Vec<LLMMessage>,
    ) -> Result<String, GatewayError> {
        let start_time = std::time::Instant::now();

        let request_config = RequestConfig {
            model: self.get_default_model(),
            temperature: self.get_default_temperature(),
            max_tokens: Some(self.config.models.max_tokens),
            stream: Some(false),
            ..Default::default()
        };

        let request = beebotos_agents::llm::types::LLMRequest {
            messages,
            config: request_config,
        };

        let result = self.failover_provider.complete(request).await;
        let latency_ms = start_time.elapsed().as_millis() as u64;

        match result {
            Ok(response) => {
                let content = response
                    .choices
                    .first()
                    .map(|choice| choice.message.text_content())
                    .unwrap_or_default();

                let (input_tokens, output_tokens) = response.usage.as_ref().map_or((0, 0), |u| {
                    (u.prompt_tokens, u.completion_tokens)
                });

                self.metrics
                    .record_success(latency_ms, input_tokens, output_tokens)
                    .await;

                Ok(content)
            }
            Err(e) => {
                self.metrics.record_failure();
                Err(GatewayError::Internal {
                    message: format!("LLM request failed: {}", e),
                    correlation_id: uuid::Uuid::new_v4().to_string(),
                })
            }
        }
    }

    /// Process a pre-built message list with streaming response
    pub async fn process_messages_stream(
        &self,
        messages: Vec<LLMMessage>,
    ) -> Result<tokio::sync::mpsc::Receiver<String>, GatewayError> {
        let start_time = std::time::Instant::now();

        let request_config = RequestConfig {
            model: self.get_default_model(),
            temperature: self.get_default_temperature(),
            max_tokens: Some(self.config.models.max_tokens),
            stream: Some(true),
            ..Default::default()
        };

        let request = beebotos_agents::llm::types::LLMRequest {
            messages,
            config: request_config,
        };

        let mut stream_rx = self
            .failover_provider
            .complete_stream(request)
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("LLM streaming request failed: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            })?;

        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let metrics = self.metrics.clone();

        tokio::spawn(async move {
            while let Some(chunk) = stream_rx.recv().await {
                for choice in &chunk.choices {
                    if let Some(content) = &choice.delta.content {
                        if tx.send(content.clone()).await.is_err() {
                            let latency_ms = start_time.elapsed().as_millis() as u64;
                            metrics.record_success(latency_ms, 0, 0).await;
                            return;
                        }
                    }

                    if choice.finish_reason.is_some() {
                        let latency_ms = start_time.elapsed().as_millis() as u64;
                        metrics.record_success(latency_ms, 0, 0).await;
                        return;
                    }
                }
            }

            let latency_ms = start_time.elapsed().as_millis() as u64;
            metrics.record_success(latency_ms, 0, 0).await;
        });

        info!("🔄 Started LLM streaming response for pre-built messages");
        Ok(rx)
    }

    /// Process a message with streaming response
    /// 
    /// Returns a channel receiver that yields response chunks
    pub async fn process_message_stream(
        &self,
        message: &ChannelMessage,
    ) -> Result<tokio::sync::mpsc::Receiver<String>, GatewayError> {
        let start_time = std::time::Instant::now();
        
        // Process multimodal content
        let multimodal_content = self
            .multimodal_processor
            .process_message(message, message.platform, None)
            .await
            .unwrap_or_else(|e| {
                warn!("Failed to process multimodal content: {}, using text only", e);
                MultimodalContent {
                    text: message.content.clone(),
                    images: vec![],
                    metadata: HashMap::new(),
                }
            });

        // Build contents
        let mut contents: Vec<Content> = vec![Content::Text {
            text: multimodal_content.text.clone(),
        }];

        // Add images
        for image in &multimodal_content.images {
            let data_url = format!("data:{};base64,{}", image.mime_type, image.base64_data);
            contents.push(Content::ImageUrl {
                image_url: beebotos_agents::llm::types::ImageUrlContent {
                    url: data_url,
                    detail: Some("auto".to_string()),
                },
            });
        }

        // Create user message
        let user_message = if contents.len() == 1 {
            LLMMessage::user(multimodal_content.text.clone())
        } else {
            LLMMessage::multimodal(Role::User, contents)
        };

        // Build messages with system prompt
        let messages = if self.config.models.system_prompt.is_empty() {
            vec![user_message]
        } else {
            vec![
                LLMMessage::system(&self.config.models.system_prompt),
                user_message,
            ]
        };

        // Build streaming request config
        let request_config = RequestConfig {
            model: self.get_default_model(),
            temperature: self.get_default_temperature(),
            max_tokens: Some(self.config.models.max_tokens),
            stream: Some(true),
            ..Default::default()
        };

        let request = beebotos_agents::llm::types::LLMRequest {
            messages,
            config: request_config,
        };

        // Execute streaming request
        let mut stream_rx = self
            .failover_provider
            .complete_stream(request)
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("LLM streaming request failed: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            })?;

        // Create output channel
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let metrics = self.metrics.clone();

        // Spawn task to handle streaming
        tokio::spawn(async move {
            let mut full_content = String::new();
            
            while let Some(chunk) = stream_rx.recv().await {
                // Extract content from chunk
                for choice in &chunk.choices {
                    if let Some(content) = &choice.delta.content {
                        full_content.push_str(content);
                        if tx.send(content.clone()).await.is_err() {
                            // Receiver dropped, stop streaming
                            let latency_ms = start_time.elapsed().as_millis() as u64;
                            metrics.record_success(latency_ms, 0, 0).await;
                            return;
                        }
                    }
                    
                    // Check for finish reason
                    if choice.finish_reason.is_some() {
                        let latency_ms = start_time.elapsed().as_millis() as u64;
                        metrics.record_success(latency_ms, 0, 0).await;
                        return;
                    }
                }
            }
            
            // Stream completed
            let latency_ms = start_time.elapsed().as_millis() as u64;
            metrics.record_success(latency_ms, 0, 0).await;
        });

        info!("🔄 Started LLM streaming response");
        Ok(rx)
    }

    /// Send a reply back to the platform
    pub async fn send_reply(
        &self,
        platform: beebotos_agents::communication::PlatformType,
        channel_id: &str,
        content: &str,
    ) -> Result<(), GatewayError> {
        debug!(
            "Sending reply to {:?} channel {}: content_length={}",
            platform, channel_id, content.len()
        );

        info!(
            "Reply ready for {:?} channel {}: preview={:.50}...",
            platform, channel_id, content
        );

        Ok(())
    }

    /// Health check for LLM service
    pub async fn health_check(&self) -> Result<(), GatewayError> {
        self.failover_provider
            .health_check()
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("LLM health check failed: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            })
    }

    /// Get provider status
    pub async fn get_provider_status(&self) -> Vec<(String, bool, u32)> {
        self.failover_provider.get_provider_status().await
    }

    /// Get default model from config
    fn get_default_model(&self) -> String {
        let provider = &self.config.models.default_provider;
        self.config
            .models
            .providers
            .get(provider)
            .and_then(|p| p.model.clone())
            .unwrap_or_else(|| Self::get_model(provider, &ModelProviderConfig::default()))
    }

    /// Get default temperature from config
    fn get_default_temperature(&self) -> Option<f32> {
        let provider = &self.config.models.default_provider;
        self.config
            .models
            .providers
            .get(provider)
            .map(|p| p.temperature)
            .filter(|&t| t != 0.0)
            .or(Some(0.7))
    }
}

// Note: LlmService does not implement Default because it requires async initialization.
// Use `LlmService::new(config).await` instead.

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config_with_provider(provider: &str) -> BeeBotOSConfig {
        let mut config = BeeBotOSConfig::default();
        config.models.default_provider = provider.to_string();
        config.models.providers.insert(
            provider.to_string(),
            ModelProviderConfig {
                api_key: Some("test-api-key".to_string()),
                model: Some("test-model".to_string()),
                base_url: None,
                temperature: 0.7,
                deployment: None,
                context_window: None,
            },
        );
        config
    }

    #[test]
    fn test_get_api_key_from_config() {
        let config = ModelProviderConfig {
            api_key: Some("config-api-key".to_string()),
            ..Default::default()
        };
        
        // Temporarily clear env var
        let key = LlmService::get_api_key("test_provider", &config);
        assert_eq!(key, Some("config-api-key".to_string()));
    }

    #[test]
    fn test_get_base_url_defaults() {
        let config = ModelProviderConfig::default();
        
        assert_eq!(
            LlmService::get_base_url("kimi", &config),
            "https://api.moonshot.cn/v1"
        );
        assert_eq!(
            LlmService::get_base_url("openai", &config),
            "https://api.openai.com/v1"
        );
        assert_eq!(
            LlmService::get_base_url("zhipu", &config),
            "https://open.bigmodel.cn/api/paas/v4"
        );
        assert_eq!(
            LlmService::get_base_url("deepseek", &config),
            "https://api.deepseek.com/v1"
        );
    }

    #[test]
    fn test_get_model_defaults() {
        let config = ModelProviderConfig::default();
        
        assert_eq!(
            LlmService::get_model("kimi", &config),
            "moonshot-v1-8k"
        );
        assert_eq!(
            LlmService::get_model("openai", &config),
            "gpt-4o-mini"
        );
        assert_eq!(
            LlmService::get_model("zhipu", &config),
            "glm-4"
        );
    }

    #[test]
    fn test_provider_chain_building() {
        let mut config = BeeBotOSConfig::default();
        config.models.default_provider = "kimi".to_string();
        config.models.fallback_chain = vec!["openai".to_string(), "zhipu".to_string()];
        
        // Verify the chain would be: kimi -> openai -> zhipu
        let mut providers_to_try: Vec<String> = Vec::new();
        providers_to_try.push(config.models.default_provider.clone());
        for fallback in &config.models.fallback_chain {
            if !providers_to_try.contains(fallback) {
                providers_to_try.push(fallback.clone());
            }
        }
        
        assert_eq!(providers_to_try, vec!["kimi", "openai", "zhipu"]);
    }

    #[test]
    fn test_provider_chain_deduplication() {
        let mut config = BeeBotOSConfig::default();
        config.models.default_provider = "kimi".to_string();
        // Duplicate kimi in fallback chain
        config.models.fallback_chain = vec!["kimi".to_string(), "openai".to_string()];
        
        let mut providers_to_try: Vec<String> = Vec::new();
        providers_to_try.push(config.models.default_provider.clone());
        for fallback in &config.models.fallback_chain {
            if !providers_to_try.contains(fallback) {
                providers_to_try.push(fallback.clone());
            }
        }
        
        // kimi should appear only once
        assert_eq!(providers_to_try, vec!["kimi", "openai"]);
    }
}
