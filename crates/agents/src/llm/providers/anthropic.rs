//! Anthropic (Claude) LLM Provider
//!
//! Implementation for Anthropic's Claude API (Claude 3 Opus, Claude 3.5 Sonnet,
//! Claude 3 Haiku, etc.) Claude is known for its strong reasoning capabilities.

use async_trait::async_trait;
use reqwest::header::{self, HeaderMap, HeaderValue};
use serde_json::json;
use tokio::sync::mpsc;
use tracing::{debug, info, trace, warn};

use crate::llm::traits::*;
use crate::llm::types::*;

/// Anthropic API configuration
#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    pub base_url: String,
    pub api_key: String,
    pub version: String, // API version
    pub default_model: String,
    pub timeout: std::time::Duration,
    pub retry_policy: RetryPolicy,
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.anthropic.com".to_string(),
            api_key: String::new(),
            version: "2023-06-01".to_string(),
            default_model: anthropic_models::CLAUDE_3_5_SONNET.to_string(),
            timeout: std::time::Duration::from_secs(120),
            retry_policy: RetryPolicy::default(),
        }
    }
}

impl AnthropicConfig {
    pub fn from_env() -> Result<Self, String> {
        use std::env;

        let api_key = env::var("ANTHROPIC_API_KEY")
            .or_else(|_| env::var("CLAUDE_API_KEY"))
            .map_err(|_| "ANTHROPIC_API_KEY not set".to_string())?;

        let base_url = env::var("ANTHROPIC_BASE_URL")
            .unwrap_or_else(|_| "https://api.anthropic.com".to_string());

        let default_model = env::var("CLAUDE_DEFAULT_MODEL")
            .unwrap_or_else(|_| anthropic_models::CLAUDE_3_5_SONNET.to_string());

        Ok(Self {
            base_url,
            api_key,
            version: "2023-06-01".to_string(),
            default_model,
            timeout: std::time::Duration::from_secs(120),
            retry_policy: RetryPolicy::default(),
        })
    }
}

pub struct AnthropicProvider {
    config: AnthropicConfig,
    http_client: reqwest::Client,
    capabilities: ProviderCapabilities,
}

impl AnthropicProvider {
    pub fn new(config: AnthropicConfig) -> Result<Self, LLMError> {
        if config.api_key.is_empty() {
            return Err(LLMError::Auth("API key is required".to_string()));
        }

        let http_client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| LLMError::Network(e.to_string()))?;

        let capabilities = ProviderCapabilities {
            streaming: true,
            function_calling: true,
            vision: true,
            json_mode: true,
            system_messages: true,
            max_context_length: 200_000,
            max_output_tokens: 8_192,
        };

        info!(
            "Anthropic provider initialized with model: {}",
            config.default_model
        );

        Ok(Self {
            config,
            http_client,
            capabilities,
        })
    }

    pub fn from_env() -> Result<Self, LLMError> {
        let config = AnthropicConfig::from_env().map_err(|e| LLMError::InvalidRequest(e))?;
        Self::new(config)
    }

    fn build_headers(&self) -> Result<HeaderMap, LLMError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(&self.config.api_key)
                .map_err(|e| LLMError::InvalidRequest(e.to_string()))?,
        );
        headers.insert(
            "anthropic-version",
            HeaderValue::from_str(&self.config.version)
                .map_err(|e| LLMError::InvalidRequest(e.to_string()))?,
        );
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        Ok(headers)
    }

    fn convert_messages(&self, messages: Vec<Message>) -> (Option<String>, Vec<serde_json::Value>) {
        let mut system_prompt = None;
        let mut anthropic_messages = Vec::new();

        for msg in messages {
            match msg.role {
                Role::System => {
                    system_prompt = Some(msg.text_content());
                }
                Role::User | Role::Tool => {
                    anthropic_messages.push(json!({
                        "role": "user",
                        "content": msg.text_content()
                    }));
                }
                Role::Assistant => {
                    anthropic_messages.push(json!({
                        "role": "assistant",
                        "content": msg.text_content()
                    }));
                }
            }
        }

        (system_prompt, anthropic_messages)
    }

    fn convert_tools(&self, tools: Option<Vec<Tool>>) -> Option<Vec<serde_json::Value>> {
        tools.map(|tools| {
            tools
                .into_iter()
                .map(|tool| {
                    json!({
                        "name": tool.function.name,
                        "description": tool.function.description,
                        "input_schema": tool.function.parameters
                    })
                })
                .collect()
        })
    }

    fn build_request_body(&self, request: LLMRequest) -> serde_json::Value {
        let (system, messages) = self.convert_messages(request.messages);
        let model = if request.config.model.is_empty() {
            self.config.default_model.clone()
        } else {
            request.config.model
        };

        let max_tokens = request.config.max_tokens.unwrap_or(4096);

        let mut body = json!({
            "model": model,
            "messages": messages,
            "max_tokens": max_tokens,
        });

        if let Some(sys) = system {
            body["system"] = json!(sys);
        }

        if let Some(temp) = request.config.temperature {
            body["temperature"] = json!(temp);
        }

        if let Some(top_p) = request.config.top_p {
            body["top_p"] = json!(top_p);
        }

        if let Some(stream) = request.config.stream {
            body["stream"] = json!(stream);
        }

        if let Some(tools) = self.convert_tools(request.config.tools) {
            body["tools"] = json!(tools);
        }

        body
    }

    async fn execute_with_retry(&self, request: LLMRequest) -> LLMResult<reqwest::Response> {
        let url = format!("{}/v1/messages", self.config.base_url);
        let headers = self.build_headers()?;
        let body = self.build_request_body(request);

        let mut attempt = 0u32;
        loop {
            trace!("Sending request to Anthropic API (attempt {})", attempt + 1);

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

            if response.status().is_success() {
                return Ok(response);
            }

            let status = response.status();
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());

            let error = match status {
                reqwest::StatusCode::UNAUTHORIZED => LLMError::Auth("Invalid API key".to_string()),
                reqwest::StatusCode::TOO_MANY_REQUESTS => LLMError::RateLimit { retry_after: None },
                reqwest::StatusCode::BAD_REQUEST => {
                    if error_body.contains("context_length") {
                        LLMError::ContextLengthExceeded(error_body)
                    } else {
                        LLMError::InvalidRequest(error_body)
                    }
                }
                _ => LLMError::Api {
                    code: status.as_u16(),
                    message: error_body,
                },
            };

            if !self.config.retry_policy.should_retry(&error, attempt) {
                return Err(error);
            }

            attempt += 1;
            let delay = self.config.retry_policy.delay_for_attempt(attempt);
            warn!(
                "Request failed, retrying in {:?} (attempt {}/{})",
                delay, attempt, self.config.retry_policy.max_retries
            );
            tokio::time::sleep(delay).await;
        }
    }
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.capabilities.clone()
    }

    async fn complete(&self, request: LLMRequest) -> LLMResult<LLMResponse> {
        debug!("Sending completion request to Anthropic");

        let response = self.execute_with_retry(request).await?;

        let anthropic_resp: AnthropicResponse = response
            .json()
            .await
            .map_err(|e| LLMError::Serialization(e.to_string()))?;

        let content = anthropic_resp
            .content
            .first()
            .map(|c| c.text.clone())
            .unwrap_or_default();

        let tool_calls = anthropic_resp
            .content
            .iter()
            .filter_map(|c| {
                if let Some(name) = &c.name {
                    Some(ToolCall {
                        id: format!("call_{}", uuid::Uuid::new_v4()),
                        r#type: "function".to_string(),
                        function: FunctionCall {
                            name: name.clone(),
                            arguments: c.input.as_ref()?.to_string(),
                        },
                    })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        debug!(
            "Received response from Anthropic: {} input_tokens, {} output_tokens",
            anthropic_resp.usage.input_tokens, anthropic_resp.usage.output_tokens
        );

        Ok(LLMResponse {
            id: anthropic_resp.id,
            object: "chat.completion".to_string(),
            created: chrono::Utc::now().timestamp() as u64,
            model: anthropic_resp.model,
            choices: vec![Choice {
                index: 0,
                message: if tool_calls.is_empty() {
                    Message::assistant(content)
                } else {
                    Message::assistant(content).with_tool_calls(tool_calls)
                },
                finish_reason: Some(
                    anthropic_resp
                        .stop_reason
                        .unwrap_or_else(|| "stop".to_string()),
                ),
                logprobs: None,
            }],
            usage: Some(Usage {
                prompt_tokens: anthropic_resp.usage.input_tokens,
                completion_tokens: anthropic_resp.usage.output_tokens,
                total_tokens: anthropic_resp.usage.input_tokens
                    + anthropic_resp.usage.output_tokens,
            }),
        })
    }

    async fn complete_stream(&self, request: LLMRequest) -> LLMResult<mpsc::Receiver<StreamChunk>> {
        let (tx, rx) = mpsc::channel(100);

        let mut request = request;
        request.config.stream = Some(true);

        let response = self.execute_with_retry(request).await?;

        let mut stream = response.bytes_stream();

        tokio::spawn(async move {
            let mut event_id = String::new();
            let mut buffer = String::new();

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        buffer.push_str(&text);

                        // Process SSE events
                        while let Some(pos) = buffer.find("\n\n") {
                            let chunk = buffer.split_off(pos + 2);
                            let event = std::mem::replace(&mut buffer, chunk);

                            let mut event_type = String::new();
                            let mut data = String::new();

                            for line in event.lines() {
                                if let Some(t) = line.strip_prefix("event: ") {
                                    event_type = t.to_string();
                                } else if let Some(d) = line.strip_prefix("data: ") {
                                    data = d.to_string();
                                }
                            }

                            match event_type.as_str() {
                                "message_start" => {
                                    if let Ok(start) =
                                        serde_json::from_str::<AnthropicStreamStart>(&data)
                                    {
                                        event_id = start.message.id.clone();
                                    }
                                }
                                "content_block_delta" => {
                                    if let Ok(delta) =
                                        serde_json::from_str::<AnthropicStreamDelta>(&data)
                                    {
                                        if let Some(text) = delta.delta.text {
                                            let _ = tx
                                                .send(StreamChunk::new(
                                                    event_id.clone(),
                                                    text,
                                                    None,
                                                ))
                                                .await;
                                        }
                                    }
                                }
                                "message_delta" => {
                                    if let Ok(delta) =
                                        serde_json::from_str::<AnthropicMessageDelta>(&data)
                                    {
                                        let _ = tx
                                            .send(StreamChunk::new(
                                                event_id.clone(),
                                                String::new(),
                                                delta.delta.stop_reason.clone(),
                                            ))
                                            .await;
                                    }
                                }
                                "message_stop" => break,
                                _ => {}
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Stream error: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(rx)
    }

    async fn health_check(&self) -> LLMResult<()> {
        let test = LLMRequest {
            messages: vec![Message::user("Hi")],
            config: RequestConfig {
                max_tokens: Some(1),
                ..Default::default()
            },
        };
        self.complete(test)
            .await
            .map(|_| ())
            .map_err(|e| LLMError::Provider(format!("Health check: {}", e)))
    }

    async fn list_models(&self) -> LLMResult<Vec<ModelInfo>> {
        Ok(vec![
            ModelInfo {
                id: anthropic_models::CLAUDE_3_5_SONNET.to_string(),
                name: "Claude 3.5 Sonnet".to_string(),
                description: Some("Best balance of intelligence and speed".to_string()),
                context_window: 200_000,
                max_tokens: 8_192,
                capabilities: ModelCapabilities {
                    vision: true,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.003, 0.015)),
            },
            ModelInfo {
                id: anthropic_models::CLAUDE_3_OPUS.to_string(),
                name: "Claude 3 Opus".to_string(),
                description: Some("Most powerful model for complex tasks".to_string()),
                context_window: 200_000,
                max_tokens: 4_096,
                capabilities: ModelCapabilities {
                    vision: true,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.015, 0.075)),
            },
            ModelInfo {
                id: anthropic_models::CLAUDE_3_HAIKU.to_string(),
                name: "Claude 3 Haiku".to_string(),
                description: Some("Fastest model for lightweight actions".to_string()),
                context_window: 200_000,
                max_tokens: 4_096,
                capabilities: ModelCapabilities {
                    vision: true,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.00025, 0.00125)),
            },
            ModelInfo {
                id: anthropic_models::CLAUDE_3_5_HAIKU.to_string(),
                name: "Claude 3.5 Haiku".to_string(),
                description: Some("Updated fast model".to_string()),
                context_window: 200_000,
                max_tokens: 8_192,
                capabilities: ModelCapabilities {
                    vision: false,
                    function_calling: true,
                    json_mode: true,
                },
                pricing: Some((0.0008, 0.004)),
            },
        ])
    }
}

use futures::StreamExt;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    id: String,
    model: String,
    content: Vec<AnthropicContentBlock>,
    stop_reason: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    content_type: String,
    text: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    input: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

// Streaming structures
#[derive(Debug, Deserialize)]
struct AnthropicStreamStart {
    message: AnthropicStreamMessage,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AnthropicStreamMessage {
    id: String,
    #[serde(rename = "type")]
    msg_type: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamDelta {
    delta: AnthropicDeltaContent,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AnthropicDeltaContent {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicMessageDelta {
    delta: AnthropicStopDelta,
}

#[derive(Debug, Deserialize)]
struct AnthropicStopDelta {
    stop_reason: Option<String>,
}

/// Anthropic model names
pub mod anthropic_models {
    pub const CLAUDE_3_5_SONNET: &str = "claude-3-5-sonnet-20241022";
    pub const CLAUDE_3_OPUS: &str = "claude-3-opus-20240229";
    pub const CLAUDE_3_SONNET: &str = "claude-3-sonnet-20240229";
    pub const CLAUDE_3_HAIKU: &str = "claude-3-haiku-20240307";
    pub const CLAUDE_3_5_HAIKU: &str = "claude-3-5-haiku-20241022";
}
