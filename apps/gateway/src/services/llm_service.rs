//! LLM Service
//!
//! Handles LLM interactions for incoming messages from various platforms.
//! Providers are loaded from database at startup and can be hot-reloaded.
//! Supports fallback chain: if primary provider fails, try next in chain.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use beebotos_agents::communication::Message as ChannelMessage;
use beebotos_agents::llm::{
    AnthropicConfig, AnthropicProvider, Content, FailoverProvider, FailoverProviderBuilder,
    LLMProvider, Message as LLMMessage, OpenAIConfig, OpenAIProvider, RequestConfig, RetryPolicy,
    Role,
};
use beebotos_agents::media::multimodal::{MultimodalContent, MultimodalProcessor};
use sqlx::SqlitePool;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::error::GatewayError;
use crate::services::encryption_service::EncryptionService;
use crate::services::llm_provider_db as db;

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
/// Loads providers from database at startup. Supports hot-reload when
/// provider configuration changes via the admin API.
pub struct LlmService {
    db: Arc<SqlitePool>,
    encryption: Arc<EncryptionService>,
    multimodal_processor: MultimodalProcessor,
    failover_provider: Arc<RwLock<Arc<FailoverProvider>>>,
    metrics: Arc<LlmMetrics>,
}

impl LlmService {
    /// Create a new LLM service from database
    pub async fn new(
        db: Arc<SqlitePool>,
        encryption: Arc<EncryptionService>,
    ) -> Result<Self, GatewayError> {
        // Seed preset providers on first startup
        db::seed_providers(&db)
            .await
            .map_err(|e| GatewayError::internal(format!("Failed to seed providers: {}", e)))?;

        // Load providers from database
        let failover = Self::build_failover_provider(&db, &encryption).await?;

        Ok(Self {
            db,
            encryption,
            multimodal_processor: MultimodalProcessor::new(),
            failover_provider: Arc::new(RwLock::new(Arc::new(failover))),
            metrics: Arc::new(LlmMetrics::default()),
        })
    }

    /// Reload providers from database (hot reload)
    pub async fn reload_providers(&self) -> Result<(), GatewayError> {
        let new_failover = Self::build_failover_provider(&self.db, &self.encryption).await?;
        let mut guard = self.failover_provider.write().await;
        *guard = Arc::new(new_failover);
        info!("LLM providers reloaded from database");
        Ok(())
    }

    /// Build failover provider from database configuration
    async fn build_failover_provider(
        db: &SqlitePool,
        encryption: &EncryptionService,
    ) -> Result<FailoverProvider, GatewayError> {
        let providers = db::list_providers_with_models(db)
            .await
            .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

        if providers.is_empty() {
            return Err(GatewayError::internal(
                "No LLM providers configured. Please configure providers via the Web UI."
                    .to_string(),
            ));
        }

        // Find default provider index
        let default_idx = providers
            .iter()
            .position(|(p, _)| p.is_default_provider)
            .unwrap_or(0);

        let mut primary: Option<Arc<dyn LLMProvider>> = None;
        let mut fallbacks: Vec<Arc<dyn LLMProvider>> = Vec::new();

        // Build provider list: default first, then enabled others
        let mut ordered = Vec::new();
        ordered.push(providers[default_idx].clone());
        for (i, (p, models)) in providers.iter().enumerate() {
            if i != default_idx && p.enabled {
                ordered.push((p.clone(), models.clone()));
            }
        }

        for (idx, (provider, models)) in ordered.iter().enumerate() {
            let api_key = match &provider.api_key_encrypted {
                Some(encrypted) => match encryption.decrypt(encrypted) {
                    Ok(key) => key,
                    Err(e) => {
                        warn!(
                            "Failed to decrypt API key for provider '{}': {}",
                            provider.provider_id, e
                        );
                        continue;
                    }
                },
                None => {
                    if provider.provider_id != "ollama" {
                        warn!(
                            "Provider '{}' has no API key configured, skipping",
                            provider.provider_id
                        );
                        continue;
                    }
                    String::new()
                }
            };

            let default_model = models
                .iter()
                .find(|m| m.is_default_model)
                .map(|m| m.name.clone())
                .or_else(|| models.first().map(|m| m.name.clone()))
                .unwrap_or_else(|| match provider.protocol.as_str() {
                    "anthropic" => "claude-3-sonnet-20240229".to_string(),
                    _ => "gpt-4o-mini".to_string(),
                });

            let base_url = provider
                .base_url
                .clone()
                .unwrap_or_else(|| match provider.protocol.as_str() {
                    "anthropic" => "https://api.anthropic.com/v1".to_string(),
                    _ => "https://api.openai.com/v1".to_string(),
                });

            match Self::create_provider_from_db(
                &provider.protocol,
                base_url,
                api_key,
                default_model,
            ) {
                Ok(p) => {
                    if idx == 0 {
                        primary = Some(p);
                        info!("Primary provider '{}' initialized", provider.provider_id);
                    } else {
                        fallbacks.push(p);
                        info!("Fallback provider '{}' initialized", provider.provider_id);
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to initialize provider '{}': {}",
                        provider.provider_id, e
                    );
                }
            }
        }

        let primary = primary.ok_or_else(|| {
            GatewayError::internal("No primary LLM provider available".to_string())
        })?;

        // Build failover provider
        let mut builder = FailoverProviderBuilder::new()
            .primary(primary)
            .timeout_secs(90);

        for fallback in fallbacks {
            builder = builder.fallback(fallback);
        }

        builder.build().map_err(|e| {
            GatewayError::internal(format!("Failed to build failover provider: {}", e))
        })
    }

    /// Create a single provider from database configuration
    fn create_provider_from_db(
        protocol: &str,
        base_url: String,
        api_key: String,
        default_model: String,
    ) -> Result<Arc<dyn LLMProvider>, String> {
        match protocol {
            "openai-compatible" => {
                let config = OpenAIConfig {
                    base_url,
                    api_key,
                    default_model,
                    timeout: Duration::from_secs(90),
                    retry_policy: RetryPolicy::default(),
                    organization: None,
                };
                let provider = OpenAIProvider::new(config)
                    .map_err(|e| format!("Failed to create OpenAI provider: {}", e))?;
                Ok(Arc::new(provider))
            }
            "anthropic" => {
                let config = AnthropicConfig {
                    base_url,
                    api_key,
                    default_model,
                    timeout: Duration::from_secs(90),
                    retry_policy: RetryPolicy::default(),
                    version: "2023-06-01".to_string(),
                };
                let provider = AnthropicProvider::new(config)
                    .map_err(|e| format!("Failed to create Anthropic provider: {}", e))?;
                Ok(Arc::new(provider))
            }
            _ => Err(format!("Unknown protocol: {}", protocol)),
        }
    }

    /// Get metrics reference
    pub fn metrics(&self) -> Arc<LlmMetrics> {
        self.metrics.clone()
    }

    /// Get metrics summary
    pub fn get_metrics_summary(&self) -> MetricsSummary {
        self.metrics.get_summary()
    }

    /// Process a message with optional custom image download function
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
        let multimodal_content = self
            .multimodal_processor
            .process_message(message, message.platform, None)
            .await;
        self.execute_llm_request(multimodal_content, message.content.clone(), true)
            .await
    }

    /// Execute a chat completion with pre-built messages
    pub async fn chat(&self, messages: Vec<LLMMessage>) -> Result<String, GatewayError> {
        let start_time = std::time::Instant::now();

        let request_config = RequestConfig {
            model: self.get_default_model().await,
            temperature: Some(0.7),
            max_tokens: Some(4096),
            stream: Some(false),
            ..Default::default()
        };

        let request = beebotos_agents::llm::types::LLMRequest {
            messages,
            config: request_config,
        };

        let failover = self.failover_provider.read().await.clone();
        let result = failover.complete(request).await;
        let latency_ms = start_time.elapsed().as_millis() as u64;

        match result {
            Ok(response) => {
                let content = response
                    .choices
                    .first()
                    .map(|choice| choice.message.text_content())
                    .unwrap_or_default();

                let (input_tokens, output_tokens) =
                    response.usage.as_ref().map_or((0, 0), |u| {
                        (u.prompt_tokens, u.completion_tokens)
                    });

                self.metrics
                    .record_success(latency_ms, input_tokens, output_tokens)
                    .await;

                info!(
                    "Received LLM response: length={}, latency={}ms, tokens={}/{}",
                    content.len(),
                    latency_ms,
                    input_tokens,
                    output_tokens
                );
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

    /// Execute LLM request with processed multimodal content
    async fn execute_llm_request(
        &self,
        multimodal_result: Result<MultimodalContent, beebotos_agents::error::AgentError>,
        fallback_text: String,
        _include_system_prompt: bool,
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
            "Processing LLM request: text='{}...', images={}",
            multimodal_content
                .text
                .chars()
                .take(50)
                .collect::<String>(),
            multimodal_content.images.len()
        );

        // Build LLM contents from multimodal content
        let mut contents: Vec<Content> = vec![Content::Text {
            text: multimodal_content.text,
        }];

        // Add images as content
        for image in &multimodal_content.images {
            let data_url = format!("data:{};base64, {}", image.mime_type, image.base64_data);
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

        let messages = vec![user_message];

        // Build request config
        let request_config = RequestConfig {
            model: self.get_default_model().await,
            temperature: Some(0.7),
            max_tokens: Some(4096),
            stream: Some(false),
            ..Default::default()
        };

        let request = beebotos_agents::llm::types::LLMRequest {
            messages,
            config: request_config,
        };

        // Execute request through failover provider
        let failover = self.failover_provider.read().await.clone();
        let result = failover.complete(request).await;
        let latency_ms = start_time.elapsed().as_millis() as u64;

        match result {
            Ok(response) => {
                let content = response
                    .choices
                    .first()
                    .map(|choice| choice.message.text_content())
                    .unwrap_or_default();

                let (input_tokens, output_tokens) =
                    response.usage.as_ref().map_or((0, 0), |u| {
                        (u.prompt_tokens, u.completion_tokens)
                    });

                self.metrics
                    .record_success(latency_ms, input_tokens, output_tokens)
                    .await;

                info!(
                    "Received LLM response: length={}, latency={}ms, tokens={}/{}",
                    content.len(),
                    latency_ms,
                    input_tokens,
                    output_tokens
                );
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

    /// Process a message with streaming response
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
            let data_url = format!("data:{};base64, {}", image.mime_type, image.base64_data);
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

        let messages = vec![user_message];

        // Build streaming request config
        let request_config = RequestConfig {
            model: self.get_default_model().await,
            temperature: Some(0.7),
            max_tokens: Some(4096),
            stream: Some(true),
            ..Default::default()
        };

        let request = beebotos_agents::llm::types::LLMRequest {
            messages,
            config: request_config,
        };

        // Execute streaming request
        let failover = self.failover_provider.read().await.clone();
        let mut stream_rx = failover.complete_stream(request).await.map_err(|e| {
            GatewayError::Internal {
                message: format!("LLM streaming request failed: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            }
        })?;

        // Create output channel
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let metrics = self.metrics.clone();

        // Spawn task to handle streaming
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

        info!("Started LLM streaming response");
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
        let failover = self.failover_provider.read().await.clone();
        failover
            .health_check()
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("LLM health check failed: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            })
    }

    /// Get provider status
    pub async fn get_provider_status(&self) -> Vec<(String, bool, u32)> {
        let failover = self.failover_provider.read().await.clone();
        failover.get_provider_status().await
    }

    /// Get default model from database
    async fn get_default_model(&self) -> String {
        match db::get_default_provider(&self.db).await.ok().flatten() {
            Some(provider) => {
                match db::get_default_model(&self.db, provider.id).await.ok().flatten() {
                    Some(model) => model.name,
                    None => "gpt-4o-mini".to_string(),
                }
            }
            None => "gpt-4o-mini".to_string(),
        }
    }
}
