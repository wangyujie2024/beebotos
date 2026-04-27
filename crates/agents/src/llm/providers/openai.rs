//! OpenAI LLM Provider
//!
//! Implementation for OpenAI API (GPT-4, GPT-3.5, etc.)
//! OpenAI-compatible API format used by many providers.

use async_trait::async_trait;
use reqwest::header::{self, HeaderMap, HeaderValue};
use tokio::sync::mpsc;
use tracing::{debug, error, info, trace};

use crate::llm::http_client::{LLMHttpClient, OpenAIRequestBuilder, ProviderConfig};
use crate::llm::traits::*;
// Re-export models for public access
pub use crate::llm::types::openai_models;
use crate::llm::types::*;

/// OpenAI API configuration
#[derive(Debug, Clone)]
pub struct OpenAIConfig {
    /// API base URL
    pub base_url: String,
    /// API key
    pub api_key: String,
    /// Organization ID (optional)
    pub organization: Option<String>,
    /// Default model
    pub default_model: String,
    /// Request timeout
    pub timeout: std::time::Duration,
    /// Retry policy
    pub retry_policy: RetryPolicy,
}

impl Default for OpenAIConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            organization: None,
            default_model: openai_models::GPT_4O.to_string(),
            timeout: std::time::Duration::from_secs(120),
            retry_policy: RetryPolicy::default(),
        }
    }
}

impl OpenAIConfig {
    /// Create from environment variables
    pub fn from_env() -> Result<Self, String> {
        use std::env;

        let api_key =
            env::var("OPENAI_API_KEY").map_err(|_| "OPENAI_API_KEY not set".to_string())?;

        let base_url =
            env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".to_string());

        let default_model =
            env::var("OPENAI_DEFAULT_MODEL").unwrap_or_else(|_| openai_models::GPT_4O.to_string());

        let organization = env::var("OPENAI_ORGANIZATION").ok();

        Ok(Self {
            base_url,
            api_key,
            organization,
            default_model,
            timeout: std::time::Duration::from_secs(120),
            retry_policy: RetryPolicy::default(),
        })
    }
}

impl ProviderConfig for OpenAIConfig {
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

        if let Some(org) = &self.organization {
            headers.insert(
                "OpenAI-Organization",
                HeaderValue::from_str(org).map_err(|e| LLMError::InvalidRequest(e.to_string()))?,
            );
        }

        Ok(headers)
    }
}

/// OpenAI LLM Provider
pub struct OpenAIProvider {
    config: OpenAIConfig,
    http_client: LLMHttpClient,
    request_builder: OpenAIRequestBuilder,
    capabilities: ProviderCapabilities,
}

impl OpenAIProvider {
    /// Create new OpenAI provider
    pub fn new(config: OpenAIConfig) -> Result<Self, LLMError> {
        let http_client = LLMHttpClient::new(config.timeout)?;
        let request_builder = OpenAIRequestBuilder::new(config.default_model.clone());

        let capabilities = ProviderCapabilities {
            streaming: true,
            function_calling: true,
            vision: true,
            json_mode: true,
            system_messages: true,
            max_context_length: 128_000, // GPT-4 Turbo context
            max_output_tokens: 4_096,
        };

        info!(
            "OpenAI provider initialized with model: {}",
            config.default_model
        );

        Ok(Self {
            config,
            http_client,
            request_builder,
            capabilities,
        })
    }

    /// Create from environment
    pub fn from_env() -> Result<Self, LLMError> {
        let config = OpenAIConfig::from_env().map_err(|e| LLMError::InvalidRequest(e))?;
        Self::new(config)
    }
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.capabilities.clone()
    }

    async fn complete(&self, request: LLMRequest) -> LLMResult<LLMResponse> {
        let start = std::time::Instant::now();
        info!("[LLM-TRACE] OpenAIProvider::complete started, model={}, messages={}",
            request.config.model, request.messages.len());

        let body = self.request_builder.build_body(request);
        let body_size = body.to_string().len();
        info!("[LLM-TRACE] Request body built, size={} bytes", body_size);

        let response = self
            .http_client
            .execute_with_retry(&self.config, "/chat/completions", body)
            .await?;

        info!("[LLM-TRACE] HTTP response received after {:?}, status={}",
            start.elapsed(), response.status());

        let llm_response: LLMResponse = response
            .json()
            .await
            .map_err(|e| LLMError::Serialization(e.to_string()))?;

        info!("[LLM-TRACE] OpenAIProvider::complete finished in {:?}", start.elapsed());

        debug!(
            "Received response from OpenAI: {} tokens used",
            llm_response
                .usage
                .as_ref()
                .map(|u| u.total_tokens)
                .unwrap_or(0)
        );

        Ok(llm_response)
    }

    async fn complete_stream(&self, request: LLMRequest) -> LLMResult<mpsc::Receiver<StreamChunk>> {
        debug!("Sending streaming request to OpenAI");

        let (tx, rx) = mpsc::channel(100);

        let mut request = request;
        request.config.stream = Some(true);

        let body = self.request_builder.build_body(request);
        let response = self
            .http_client
            .stream_with_retry(&self.config, "/chat/completions", body)
            .await?;

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
                                    break;
                                }

                                match serde_json::from_str::<StreamChunk>(data) {
                                    Ok(chunk) => {
                                        if tx.send(chunk).await.is_err() {
                                            return;
                                        }
                                    }
                                    Err(e) => {
                                        trace!("Failed to parse chunk: {}", e);
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
        let _response = self
            .http_client
            .get_with_retry(&self.config, "/models")
            .await?;
        Ok(())
    }

    async fn list_models(&self) -> LLMResult<Vec<ModelInfo>> {
        Ok(vec![
            ModelInfo {
                id: openai_models::GPT_4O.to_string(),
                name: "GPT-4o".to_string(),
                description: Some("Latest GPT-4 model, multimodal".to_string()),
                context_window: 128_000,
                max_tokens: 4_096,
                capabilities: ModelCapabilities {
                    vision: true,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.005, 0.015)),
            },
            ModelInfo {
                id: openai_models::GPT_4O_MINI.to_string(),
                name: "GPT-4o Mini".to_string(),
                description: Some("Fast and affordable".to_string()),
                context_window: 128_000,
                max_tokens: 4_096,
                capabilities: ModelCapabilities {
                    vision: true,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.00015, 0.0006)),
            },
            ModelInfo {
                id: openai_models::GPT_4_TURBO.to_string(),
                name: "GPT-4 Turbo".to_string(),
                description: Some("High performance".to_string()),
                context_window: 128_000,
                max_tokens: 4_096,
                capabilities: ModelCapabilities {
                    vision: true,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.01, 0.03)),
            },
            ModelInfo {
                id: openai_models::O1.to_string(),
                name: "o1".to_string(),
                description: Some("Reasoning model".to_string()),
                context_window: 128_000,
                max_tokens: 32_768,
                capabilities: ModelCapabilities {
                    vision: true,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.015, 0.06)),
            },
            ModelInfo {
                id: openai_models::O3_MINI.to_string(),
                name: "o3-mini".to_string(),
                description: Some("Fast reasoning model".to_string()),
                context_window: 200_000,
                max_tokens: 100_000,
                capabilities: ModelCapabilities {
                    vision: false,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.0011, 0.0044)),
            },
        ])
    }
}

use futures::StreamExt;
