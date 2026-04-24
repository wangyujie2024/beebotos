//! LLM HTTP Client
//!
//! Provides a reusable HTTP client for LLM providers with:
//! - Automatic retry with exponential backoff
//! - Standardized error handling
//! - Request/response logging
//! - Header management

use reqwest::header::{self, HeaderMap, HeaderValue};
use serde_json::json;
use tracing::{trace, warn};

use crate::llm::types::{LLMError, LLMRequest, LLMResult};
use crate::llm::RetryPolicy;

/// Trait for provider-specific configuration
pub trait ProviderConfig: Send + Sync + Clone {
    /// Get the base URL for API requests
    fn base_url(&self) -> &str;

    /// Get the API key
    fn api_key(&self) -> &str;

    /// Get the request timeout
    fn timeout(&self) -> std::time::Duration;

    /// Get the retry policy
    fn retry_policy(&self) -> &RetryPolicy;

    /// Get the default model
    fn default_model(&self) -> &str;

    /// Build provider-specific headers
    fn build_headers(&self) -> Result<HeaderMap, LLMError> {
        let mut headers = HeaderMap::new();

        // Standard Authorization header
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.api_key()))
                .map_err(|e| LLMError::InvalidRequest(e.to_string()))?,
        );

        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );

        Ok(headers)
    }
}

/// Standard HTTP client for LLM providers
pub struct LLMHttpClient {
    http_client: reqwest::Client,
}

impl LLMHttpClient {
    /// Create a new HTTP client with the given timeout
    pub fn new(timeout: std::time::Duration) -> LLMResult<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| LLMError::Network(e.to_string()))?;

        Ok(Self { http_client })
    }

    /// Execute a request with retry logic
    pub async fn execute_with_retry<C: ProviderConfig>(
        &self,
        config: &C,
        endpoint: &str,
        body: serde_json::Value,
    ) -> LLMResult<reqwest::Response> {
        let url = format!("{}{}", config.base_url(), endpoint);
        let headers = config.build_headers()?;
        let retry_policy = config.retry_policy();

        let mut attempt = 0u32;

        loop {
            trace!("Sending request to {} (attempt {})", url, attempt + 1);

            let response = self
                .http_client
                .post(&url)
                .headers(headers.clone())
                .json(&body)
                .send()
                .await
                .map_err(|e| {
                    if e.is_timeout() {
                        LLMError::Timeout
                    } else {
                        LLMError::Network(e.to_string())
                    }
                })?;

            let status = response.status();

            if status.is_success() {
                return Ok(response);
            }

            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            let error = Self::map_http_error(status, error_body);

            if !retry_policy.should_retry(&error, attempt) {
                return Err(error);
            }

            attempt += 1;
            let delay = retry_policy.delay_for_attempt(attempt);
            warn!(
                "Request failed, retrying in {:?} (attempt {}/{})",
                delay, attempt, retry_policy.max_retries
            );
            tokio::time::sleep(delay).await;
        }
    }

    /// Stream a request with retry logic
    pub async fn stream_with_retry<C: ProviderConfig>(
        &self,
        config: &C,
        endpoint: &str,
        body: serde_json::Value,
    ) -> LLMResult<reqwest::Response> {
        // For streaming, we use the same logic but the caller handles the response
        // differently
        self.execute_with_retry(config, endpoint, body).await
    }

    /// Execute a GET request with retry logic
    pub async fn get_with_retry<C: ProviderConfig>(
        &self,
        config: &C,
        endpoint: &str,
    ) -> LLMResult<reqwest::Response> {
        let url = format!("{}{}", config.base_url(), endpoint);
        let headers = config.build_headers()?;
        let retry_policy = config.retry_policy();

        let mut attempt = 0u32;

        loop {
            trace!("Sending GET request to {} (attempt {})", url, attempt + 1);

            let response = self
                .http_client
                .get(&url)
                .headers(headers.clone())
                .send()
                .await
                .map_err(|e| {
                    if e.is_timeout() {
                        LLMError::Timeout
                    } else {
                        LLMError::Network(e.to_string())
                    }
                })?;

            let status = response.status();

            if status.is_success() {
                return Ok(response);
            }

            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            let error = Self::map_http_error(status, error_body);

            if !retry_policy.should_retry(&error, attempt) {
                return Err(error);
            }

            attempt += 1;
            let delay = retry_policy.delay_for_attempt(attempt);
            warn!(
                "GET request failed, retrying in {:?} (attempt {}/{})",
                delay, attempt, retry_policy.max_retries
            );
            tokio::time::sleep(delay).await;
        }
    }

    /// Map HTTP status codes to LLM errors
    fn map_http_error(status: reqwest::StatusCode, error_body: String) -> LLMError {
        match status {
            reqwest::StatusCode::UNAUTHORIZED => LLMError::Auth("Invalid API key".to_string()),
            reqwest::StatusCode::TOO_MANY_REQUESTS => LLMError::RateLimit { retry_after: None },
            reqwest::StatusCode::BAD_REQUEST => {
                if error_body.contains("context_length") {
                    LLMError::ContextLengthExceeded(error_body)
                } else {
                    LLMError::InvalidRequest(error_body)
                }
            }
            _ if status.is_server_error() => LLMError::Api {
                code: status.as_u16(),
                message: error_body,
            },
            _ => LLMError::Api {
                code: status.as_u16(),
                message: error_body,
            },
        }
    }
}

/// Trait for building OpenAI-compatible request bodies
pub trait OpenAICompatibleBody {
    fn build_request_body(&self, request: LLMRequest) -> serde_json::Value;
}

/// Default implementation for OpenAI-compatible providers
pub struct OpenAIRequestBuilder {
    pub default_model: String,
}

impl OpenAIRequestBuilder {
    pub fn new(default_model: String) -> Self {
        Self { default_model }
    }

    pub fn build_body(&self, request: LLMRequest) -> serde_json::Value {
        let model = if request.config.model.is_empty() {
            self.default_model.clone()
        } else {
            request.config.model
        };

        let messages: Vec<serde_json::Value> = request
            .messages
            .into_iter()
            .map(|m| {
                use crate::llm::types::{Content, ImageUrlContent};

                let content_json = if m.content.len() == 1 {
                    match &m.content[0] {
                        Content::Text { text } => json!(text),
                        Content::ImageUrl {
                            image_url: ImageUrlContent { url, detail },
                        } => {
                            let mut arr = vec![json!({
                                "type": "image_url",
                                "image_url": {
                                    "url": url,
                                }
                            })];
                            if let Some(detail) = detail {
                                arr[0]["image_url"]["detail"] = json!(detail);
                            }
                            json!(arr)
                        }
                        Content::File { name, source } => json!([{
                            "type": "file",
                            "name": name,
                            "source": source,
                        }]),
                    }
                } else {
                    let arr: Vec<serde_json::Value> = m
                        .content
                        .iter()
                        .map(|c| match c {
                            Content::Text { text } => json!({
                                "type": "text",
                                "text": text,
                            }),
                            Content::ImageUrl {
                                image_url: ImageUrlContent { url, detail },
                            } => {
                                let mut obj = json!({
                                    "type": "image_url",
                                    "image_url": {
                                        "url": url,
                                    }
                                });
                                if let Some(detail) = detail {
                                    obj["image_url"]["detail"] = json!(detail);
                                }
                                obj
                            }
                            Content::File { name, source } => json!({
                                "type": "file",
                                "name": name,
                                "source": source,
                            }),
                        })
                        .collect();
                    json!(arr)
                };

                let mut obj = json!({
                    "role": m.role,
                    "content": content_json,
                });

                if let Some(name) = m.name {
                    obj["name"] = json!(name);
                }

                if let Some(tool_calls) = m.tool_calls {
                    obj["tool_calls"] = json!(tool_calls);
                }

                if let Some(tool_call_id) = m.tool_call_id {
                    obj["tool_call_id"] = json!(tool_call_id);
                }

                obj
            })
            .collect();

        let mut body = json!({
            "model": model,
            "messages": messages,
        });

        if let Some(temp) = request.config.temperature {
            body["temperature"] = json!(temp);
        }

        if let Some(top_p) = request.config.top_p {
            body["top_p"] = json!(top_p);
        }

        if let Some(max_tokens) = request.config.max_tokens {
            body["max_tokens"] = json!(max_tokens);
        }

        if let Some(stop) = request.config.stop {
            body["stop"] = json!(stop);
        }

        if let Some(stream) = request.config.stream {
            body["stream"] = json!(stream);
        }

        if let Some(response_format) = request.config.response_format {
            body["response_format"] = json!(response_format);
        }

        if let Some(tools) = request.config.tools {
            body["tools"] = json!(tools);
        }

        if let Some(tool_choice) = request.config.tool_choice {
            body["tool_choice"] = json!(tool_choice);
        }

        if let Some(presence_penalty) = request.config.presence_penalty {
            body["presence_penalty"] = json!(presence_penalty);
        }

        if let Some(frequency_penalty) = request.config.frequency_penalty {
            body["frequency_penalty"] = json!(frequency_penalty);
        }

        if let Some(seed) = request.config.seed {
            body["seed"] = json!(seed);
        }

        for (key, value) in request.config.extra {
            body[key] = value;
        }

        body
    }
}

/// Macro to implement ProviderConfig for standard providers
#[macro_export]
macro_rules! impl_standard_provider_config {
    ($config_type:ty, $base_url_fn:expr, $api_key_fn:expr, $timeout_fn:expr, $retry_policy_fn:expr, $default_model_fn:expr) => {
        impl $crate::llm::http_client::ProviderConfig for $config_type {
            fn base_url(&self) -> &str {
                $base_url_fn(self)
            }

            fn api_key(&self) -> &str {
                $api_key_fn(self)
            }

            fn timeout(&self) -> std::time::Duration {
                $timeout_fn(self)
            }

            fn retry_policy(&self) -> &$crate::llm::traits::RetryPolicy {
                $retry_policy_fn(self)
            }

            fn default_model(&self) -> &str {
                $default_model_fn(self)
            }
        }
    };
}

/// Common provider initialization logic
pub struct ProviderInitParams<C> {
    pub config: C,
    pub http_client: LLMHttpClient,
}

impl<C: ProviderConfig> ProviderInitParams<C> {
    pub fn new(config: C) -> LLMResult<Self> {
        if config.api_key().is_empty() {
            return Err(LLMError::Auth("API key is required".to_string()));
        }

        let http_client = LLMHttpClient::new(config.timeout())?;

        Ok(Self {
            config,
            http_client,
        })
    }
}
