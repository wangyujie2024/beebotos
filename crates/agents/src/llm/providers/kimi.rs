//! Kimi LLM Provider
//!
//! Implementation for Moonshot AI's Kimi API.
//! Kimi uses an OpenAI-compatible API format.

use async_trait::async_trait;
use reqwest::header::{self, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};

use tokio::sync::mpsc;
use tracing::{debug, error, info, trace, warn};

use crate::llm::http_client::{LLMHttpClient, OpenAIRequestBuilder, ProviderConfig};
use crate::llm::traits::*;
use crate::llm::types::*;

// Re-export models for public access
pub use crate::llm::types::kimi_models;

/// 🔧 P1 FIX: Provider mode for multi-provider configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ProviderMode {
    /// Merge mode: Allow multiple providers, system automatically selects
    #[default]
    #[serde(rename = "merge")]
    Merge,
    /// Replace mode: Only use this provider
    #[serde(rename = "replace")]
    Replace,
}

impl std::fmt::Display for ProviderMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderMode::Merge => write!(f, "merge"),
            ProviderMode::Replace => write!(f, "replace"),
        }
    }
}

/// Kimi API configuration
#[derive(Debug, Clone)]
pub struct KimiConfig {
    /// API base URL
    pub base_url: String,
    /// API key
    pub api_key: String,
    /// Default model
    pub default_model: String,
    /// Request timeout
    pub timeout: std::time::Duration,
    /// Retry policy
    pub retry_policy: RetryPolicy,
    /// 🔧 P1 FIX: Provider mode for multi-provider configuration
    pub mode: ProviderMode,
}

impl Default for KimiConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.moonshot.cn/v1".to_string(),
            api_key: String::new(),
            default_model: kimi_models::KIMI_LATEST.to_string(),
            timeout: std::time::Duration::from_secs(60),
            retry_policy: RetryPolicy::default(),
            mode: ProviderMode::default(), // merge by default
        }
    }
}

impl KimiConfig {
    /// Create from environment variables
    pub fn from_env() -> Result<Self, String> {
        use std::env;
        
        let api_key = env::var("KIMI_API_KEY")
            .or_else(|_| env::var("MOONSHOT_API_KEY"))
            .map_err(|_| "KIMI_API_KEY or MOONSHOT_API_KEY not set".to_string())?;

        let base_url = env::var("KIMI_BASE_URL")
            .unwrap_or_else(|_| "https://api.moonshot.cn/v1".to_string());

        let default_model = env::var("KIMI_DEFAULT_MODEL")
            .unwrap_or_else(|_| kimi_models::KIMI_LATEST.to_string());

        let mode = env::var("KIMI_MODE")
            .ok()
            .and_then(|m| match m.to_lowercase().as_str() {
                "merge" => Some(ProviderMode::Merge),
                "replace" => Some(ProviderMode::Replace),
                _ => None,
            })
            .unwrap_or_default();

        Ok(Self {
            base_url,
            api_key,
            default_model,
            timeout: std::time::Duration::from_secs(60),
            retry_policy: RetryPolicy::default(),
            mode,
        })
    }

    /// 🔧 P1 FIX: Set provider mode
    pub fn with_mode(mut self, mode: ProviderMode) -> Self {
        self.mode = mode;
        self
    }

    /// 🔧 P1 FIX: Check if this provider should be used exclusively
    pub fn is_exclusive(&self) -> bool {
        self.mode == ProviderMode::Replace
    }
}

impl ProviderConfig for KimiConfig {
    fn base_url(&self) -> &str {
        &self.base_url
    }

    fn api_key(&self) -> &str {
        &self.api_key
    }

    fn timeout(&self) -> std::time::Duration {
        self.timeout
    }

    fn retry_policy(&self) -> &RetryPolicy {
        &self.retry_policy
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    fn build_headers(&self) -> Result<HeaderMap, LLMError> {
        let mut headers = HeaderMap::new();
        
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.api_key))
                .map_err(|e| LLMError::InvalidRequest(e.to_string()))?,
        );
        
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );

        Ok(headers)
    }
}

/// Kimi LLM Provider
pub struct KimiProvider {
    config: KimiConfig,
    http_client: LLMHttpClient,
    request_builder: OpenAIRequestBuilder,
    capabilities: ProviderCapabilities,
}

impl KimiProvider {
    /// Create new Kimi provider
    pub fn new(config: KimiConfig) -> Result<Self, LLMError> {
        if config.api_key.is_empty() {
            return Err(LLMError::Auth("API key is required".to_string()));
        }

        let http_client = LLMHttpClient::new(config.timeout)?;
        let request_builder = OpenAIRequestBuilder::new(config.default_model.clone());

        let capabilities = ProviderCapabilities {
            streaming: true,
            function_calling: true,
            vision: true,
            json_mode: true,
            system_messages: true,
            max_context_length: 256_000, // Kimi supports very long context
            max_output_tokens: 8_192,
        };

        info!("Kimi provider initialized with model: {}", config.default_model);

        Ok(Self {
            config,
            http_client,
            request_builder,
            capabilities,
        })
    }

    /// Create from environment
    pub fn from_env() -> Result<Self, LLMError> {
        let config = KimiConfig::from_env()
            .map_err(|e| LLMError::InvalidRequest(e))?;
        Self::new(config)
    }
}

#[async_trait]
impl LLMProvider for KimiProvider {
    fn name(&self) -> &str {
        "kimi"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.capabilities.clone()
    }

    async fn complete(&self, request: LLMRequest) -> LLMResult<LLMResponse> {
        debug!("Sending completion request to Kimi");

        let body = self.request_builder.build_body(request);
        let response = self.http_client.execute_with_retry(
            &self.config,
            "/chat/completions",
            body
        ).await?;
        
        let llm_response: LLMResponse = response
            .json()
            .await
            .map_err(|e| LLMError::Serialization(e.to_string()))?;

        debug!(
            "Received response from Kimi: {} tokens used",
            llm_response.usage.as_ref().map(|u| u.total_tokens).unwrap_or(0)
        );

        Ok(llm_response)
    }

    async fn complete_stream(&self, request: LLMRequest) -> LLMResult<mpsc::Receiver<StreamChunk>> {
        debug!("Sending streaming request to Kimi");

        let (tx, rx) = mpsc::channel(100);

        let mut request = request;
        request.config.stream = Some(true);

        let body = self.request_builder.build_body(request);
        let response = self.http_client.stream_with_retry(
            &self.config,
            "/chat/completions",
            body
        ).await?;
        
        let mut stream = response.bytes_stream();

        tokio::spawn(async move {
            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);

                        for line in text.lines() {
                            if line.starts_with("data: ") {
                                let data = &line[6..];

                                if data == "[DONE]" {
                                    return;
                                }

                                match serde_json::from_str::<StreamChunk>(data) {
                                    Ok(chunk) => {
                                        if tx.send(chunk).await.is_err() {
                                            return;
                                        }
                                    }
                                    Err(e) => {
                                        warn!("Failed to parse Kimi chunk: {} | data: {}", e, &data[..data.len().min(200)]);
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Stream error: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(rx)
    }

    async fn health_check(&self) -> LLMResult<()> {
        let _response = self.http_client
            .get_with_retry(&self.config, "/models")
            .await?;
        Ok(())
    }

    async fn list_models(&self) -> LLMResult<Vec<ModelInfo>> {
        Ok(vec![
            ModelInfo {
                id: kimi_models::KIMI_LATEST.to_string(),
                name: "Kimi Latest".to_string(),
                description: Some("Latest Kimi model with best performance".to_string()),
                context_window: 256_000,
                max_tokens: 8_192,
                capabilities: ModelCapabilities {
                    vision: true,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.006, 0.012)), // USD per 1K tokens
            },
            ModelInfo {
                id: kimi_models::KIMI_FLASH.to_string(),
                name: "Kimi Flash".to_string(),
                description: Some("Fast and cost-effective".to_string()),
                context_window: 128_000,
                max_tokens: 4_096,
                capabilities: ModelCapabilities {
                    vision: true,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.001, 0.002)),
            },
        ])
    }
}

use futures::StreamExt;
