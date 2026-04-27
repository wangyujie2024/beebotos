//! Ollama LLM Provider
//!
//! Implementation for local LLM models via Ollama API.
//! https://github.com/ollama/ollama

use async_trait::async_trait;
use reqwest::header::{self, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::mpsc;
use tracing::{debug, info, trace, warn};

use crate::llm::traits::*;
use crate::llm::types::*;

/// Ollama API configuration
#[derive(Debug, Clone)]
pub struct OllamaConfig {
    pub base_url: String,
    pub timeout: std::time::Duration,
    pub default_model: String,
    pub retry_policy: RetryPolicy,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".to_string(),
            timeout: std::time::Duration::from_secs(300), // Local models can be slow
            default_model: "llama2".to_string(),
            retry_policy: RetryPolicy::default(),
        }
    }
}

impl OllamaConfig {
    pub fn from_env() -> Result<Self, String> {
        use std::env;

        let base_url =
            env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://localhost:11434".to_string());

        let default_model =
            env::var("OLLAMA_DEFAULT_MODEL").unwrap_or_else(|_| "llama2".to_string());

        Ok(Self {
            base_url,
            timeout: std::time::Duration::from_secs(300),
            default_model,
            retry_policy: RetryPolicy::default(),
        })
    }
}

pub struct OllamaProvider {
    config: OllamaConfig,
    http_client: reqwest::Client,
    capabilities: ProviderCapabilities,
}

impl OllamaProvider {
    pub fn new(config: OllamaConfig) -> Result<Self, LLMError> {
        let http_client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| LLMError::Network(e.to_string()))?;

        let capabilities = ProviderCapabilities {
            streaming: true,
            function_calling: false, // Limited support depending on model
            vision: false,           // Some models support vision
            json_mode: true,
            system_messages: true,
            max_context_length: 32_768, // Depends on model
            max_output_tokens: 4_096,
        };

        info!(
            "Ollama provider initialized with base URL: {}",
            config.base_url
        );

        Ok(Self {
            config,
            http_client,
            capabilities,
        })
    }

    pub fn from_env() -> Result<Self, LLMError> {
        let config = OllamaConfig::from_env().map_err(|e| LLMError::InvalidRequest(e))?;
        Self::new(config)
    }

    fn build_headers(&self) -> Result<HeaderMap, LLMError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        Ok(headers)
    }

    fn convert_messages(&self, messages: Vec<Message>) -> Vec<OllamaMessage> {
        messages
            .into_iter()
            .map(|msg| {
                let role = match msg.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                };

                OllamaMessage {
                    role: role.to_string(),
                    content: msg.text_content(),
                    images: None, // Could be extended to support vision
                }
            })
            .collect()
    }

    fn convert_tools(&self, tools: Option<Vec<Tool>>) -> Option<Vec<OllamaTool>> {
        tools.map(|tools| {
            tools
                .into_iter()
                .map(|tool| OllamaTool {
                    function: OllamaFunction {
                        name: tool.function.name,
                        description: tool.function.description.unwrap_or_default(),
                        parameters: tool.function.parameters,
                    },
                })
                .collect()
        })
    }

    fn build_request_body(&self, request: LLMRequest) -> serde_json::Value {
        let model = if request.config.model.is_empty() {
            self.config.default_model.clone()
        } else {
            request.config.model
        };

        let messages = self.convert_messages(request.messages);

        let mut options = serde_json::Map::new();
        if let Some(temp) = request.config.temperature {
            options.insert("temperature".to_string(), json!(temp));
        }
        if let Some(max_tokens) = request.config.max_tokens {
            options.insert("num_predict".to_string(), json!(max_tokens as i32));
        }
        if let Some(top_p) = request.config.top_p {
            options.insert("top_p".to_string(), json!(top_p));
        }

        let mut body = json!({
            "model": model,
            "messages": messages,
            "stream": request.config.stream.unwrap_or(false),
        });

        if !options.is_empty() {
            body["options"] = json!(options);
        }

        if let Some(tools) = self.convert_tools(request.config.tools) {
            body["tools"] = json!(tools);
        }

        body
    }

    async fn execute_with_retry(&self, request: LLMRequest) -> LLMResult<reqwest::Response> {
        let url = format!("{}/api/chat", self.config.base_url);
        let headers = self.build_headers()?;
        let body = self.build_request_body(request);

        let mut attempt = 0u32;
        loop {
            trace!("Sending request to Ollama API (attempt {})", attempt + 1);

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
                        LLMError::Network(format!(
                            "Failed to connect to Ollama at {}. Is Ollama running? Error: {}",
                            self.config.base_url, e
                        ))
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
                reqwest::StatusCode::NOT_FOUND => LLMError::ModelNotFound(format!(
                    "Model not found. Run 'ollama pull <model>' to download it."
                )),
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

    fn generate_id(&self) -> String {
        format!("ollama-{}", uuid::Uuid::new_v4())
    }
}

#[async_trait]
impl LLMProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.capabilities.clone()
    }

    async fn complete(&self, request: LLMRequest) -> LLMResult<LLMResponse> {
        debug!("Sending completion request to Ollama");

        let response = self.execute_with_retry(request).await?;

        let ollama_resp: OllamaResponse = response
            .json()
            .await
            .map_err(|e| LLMError::Serialization(e.to_string()))?;

        // Ollama doesn't return token usage directly, estimate from character count
        let prompt_estimate: usize = ollama_resp.message.content.len(); // Simplified
        let completion_estimate = ollama_resp.message.content.len();
        // Rough estimate: ~4 characters per token
        let prompt_tokens = (prompt_estimate / 4) as u32;
        let completion_tokens = (completion_estimate / 4) as u32;

        debug!("Received response from Ollama model: {}", ollama_resp.model);

        let tool_calls = ollama_resp.message.tool_calls.map(|tcs| {
            tcs.into_iter()
                .map(|tc| ToolCall {
                    id: self.generate_id(),
                    r#type: "function".to_string(),
                    function: FunctionCall {
                        name: tc.function.name,
                        arguments: serde_json::to_string(&tc.function.arguments)
                            .unwrap_or_default(),
                    },
                })
                .collect()
        });

        Ok(LLMResponse {
            id: self.generate_id(),
            object: "chat.completion".to_string(),
            created: chrono::Utc::now().timestamp() as u64,
            model: ollama_resp.model,
            choices: vec![Choice {
                index: 0,
                message: if let Some(calls) = tool_calls {
                    Message::assistant(ollama_resp.message.content).with_tool_calls(calls)
                } else {
                    Message::assistant(ollama_resp.message.content)
                },
                finish_reason: if ollama_resp.done {
                    Some("stop".to_string())
                } else {
                    None
                },
                logprobs: None,
            }],
            usage: Some(Usage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            }),
        })
    }

    async fn complete_stream(&self, request: LLMRequest) -> LLMResult<mpsc::Receiver<StreamChunk>> {
        let (tx, rx) = mpsc::channel(100);

        let mut request = request;
        request.config.stream = Some(true);

        let response = self.execute_with_retry(request).await?;

        let mut stream = response.bytes_stream();
        let event_id = self.generate_id();

        tokio::spawn(async move {
            let mut buffer = String::new();

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        buffer.push_str(&text);

                        // Ollama streaming returns JSON lines
                        while let Some(pos) = buffer.find('\n') {
                            let line = buffer.split_off(pos + 1);
                            let json_str = std::mem::replace(&mut buffer, line).trim().to_string();

                            if json_str.is_empty() {
                                continue;
                            }

                            match serde_json::from_str::<OllamaStreamChunk>(&json_str) {
                                Ok(chunk) => {
                                    let finish_reason = if chunk.done {
                                        Some("stop".to_string())
                                    } else {
                                        None
                                    };

                                    if tx
                                        .send(StreamChunk::new(
                                            event_id.clone(),
                                            chunk.message.content.clone(),
                                            finish_reason,
                                        ))
                                        .await
                                        .is_err()
                                    {
                                        return;
                                    }

                                    if chunk.done {
                                        return;
                                    }
                                }
                                Err(e) => {
                                    trace!("Failed to parse Ollama stream chunk: {}", e);
                                    // Continue on parse error, might be partial data
                                    if json_str.len() > 10000 {
                                        // Prevent infinite buffering
                                        return;
                                    }
                                    buffer = json_str + &buffer;
                                    break;
                                }
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
        let url = format!("{}/api/tags", self.config.base_url);

        match self
            .http_client
            .get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => Ok(()),
            _ => Err(LLMError::Provider("Ollama is not running".to_string())),
        }
    }

    async fn list_models(&self) -> LLMResult<Vec<ModelInfo>> {
        let url = format!("{}/api/tags", self.config.base_url);

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| LLMError::Network(e.to_string()))?;

        if !response.status().is_success() {
            return Err(LLMError::Api {
                code: response.status().as_u16(),
                message: response.text().await.unwrap_or_default(),
            });
        }

        let tags: OllamaTagsResponse = response
            .json()
            .await
            .map_err(|e| LLMError::Serialization(e.to_string()))?;

        Ok(tags
            .models
            .into_iter()
            .map(|m| {
                ModelInfo {
                    id: m.name.clone(),
                    name: m.name,
                    description: None,
                    context_window: m.details.num_ctx.unwrap_or(2048) as usize,
                    max_tokens: 4_096,
                    capabilities: ModelCapabilities {
                        vision: false,
                        function_calling: false,
                        json_mode: true,
                    },
                    pricing: Some((0.0, 0.0)), // Local models are free
                }
            })
            .collect())
    }
}

use futures::StreamExt;

#[derive(Debug, Serialize)]
struct OllamaMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    images: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
struct OllamaTool {
    function: OllamaFunction,
}

#[derive(Debug, Serialize)]
struct OllamaFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    model: String,
    message: OllamaResponseMessage,
    done: bool,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OllamaResponseMessage {
    role: String,
    content: String,
    #[serde(default)]
    tool_calls: Option<Vec<OllamaToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OllamaToolCall {
    function: OllamaToolCallFunction,
}

#[derive(Debug, Deserialize)]
struct OllamaToolCallFunction {
    name: String,
    arguments: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Debug, Deserialize)]
struct OllamaModel {
    name: String,
    #[serde(default)]
    details: OllamaModelDetails,
}

#[derive(Debug, Deserialize, Default)]
struct OllamaModelDetails {
    #[serde(default)]
    num_ctx: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OllamaStreamChunk {
    model: String,
    message: OllamaStreamMessage,
    done: bool,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OllamaStreamMessage {
    role: String,
    content: String,
}

/// Ollama model names - popular models
pub mod ollama_models {
    pub const LLAMA2: &str = "llama2";
    pub const LLAMA3: &str = "llama3";
    pub const LLAMA3_1: &str = "llama3.1";
    pub const LLAMA3_2: &str = "llama3.2";
    pub const MISTRAL: &str = "mistral";
    pub const MIXTRAL: &str = "mixtral";
    pub const CODELLAMA: &str = "codellama";
    pub const PHI3: &str = "phi3";
    pub const GEMMA: &str = "gemma";
    pub const GEMMA2: &str = "gemma2";
}
