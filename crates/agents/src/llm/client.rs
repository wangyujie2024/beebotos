//! High-level LLM Client
//!
//! Provides a user-friendly interface for interacting with LLM providers,
//! with features like conversation management, tool execution, and metrics.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{info, instrument, warn};

use crate::llm::traits::*;
use crate::llm::types::*;
use crate::rate_limit::RateLimiter;

/// Outcome of a single LLM turn when tools are enabled.
enum TurnOutcome {
    /// Model returned text — conversation is done.
    Text(String),
    /// Model requested tool calls — need to execute and continue.
    /// Includes the assistant message with tool_calls so the caller
    /// can append it to the conversation history.
    ToolResults(Vec<ToolResult>, Message),
}

/// High-level LLM client
///
/// Wraps a provider with additional functionality like:
/// - Conversation context management
/// - Tool execution
/// - Metrics collection
/// - Request/response logging
/// - ARCHITECTURE FIX: Rate limiting
pub struct LLMClient {
    provider: Arc<dyn LLMProvider>,
    context: Arc<RwLock<Vec<Message>>>,
    config: RequestConfig,
    tools: Arc<RwLock<HashMap<String, Box<dyn ToolHandler>>>>,
    metrics: Arc<RwLock<ClientMetrics>>,
    /// ARCHITECTURE FIX: Rate limiter for API requests
    rate_limiter: Option<Arc<RateLimiter>>,
    /// ARCHITECTURE FIX: Rate limit key (e.g., "llm_requests")
    rate_limit_key: String,
}

/// Tool handler trait
#[async_trait::async_trait]
pub trait ToolHandler: Send + Sync {
    /// Get tool definition
    fn definition(&self) -> Tool;

    /// Execute the tool
    async fn execute(&self, arguments: &str) -> Result<String, String>;
}

/// Client metrics
#[derive(Debug, Clone, Default)]
pub struct ClientMetrics {
    /// Total requests
    pub total_requests: u64,
    /// Successful requests
    pub successful_requests: u64,
    /// Failed requests
    pub failed_requests: u64,
    /// Total tokens used
    pub total_tokens: u64,
    /// Total latency
    pub total_latency_ms: u64,
}

impl LLMClient {
    /// Create new client with provider
    pub fn new(provider: Arc<dyn LLMProvider>) -> Self {
        Self {
            provider,
            context: Arc::new(RwLock::new(Vec::new())),
            config: RequestConfig::default(),
            tools: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(ClientMetrics::default())),
            rate_limiter: None,
            rate_limit_key: "llm_requests".to_string(),
        }
    }

    /// Create with custom config
    pub fn with_config(mut self, config: RequestConfig) -> Self {
        self.config = config;
        self
    }

    /// ARCHITECTURE FIX: Enable rate limiting
    pub fn with_rate_limiter(mut self, rate_limiter: Arc<RateLimiter>) -> Self {
        self.rate_limiter = Some(rate_limiter);
        self
    }

    /// ARCHITECTURE FIX: Set custom rate limit key
    pub fn with_rate_limit_key(mut self, key: impl Into<String>) -> Self {
        self.rate_limit_key = key.into();
        self
    }

    /// ARCHITECTURE FIX: Check rate limit before making request
    fn check_rate_limit(&self) -> LLMResult<()> {
        if let Some(ref limiter) = self.rate_limiter {
            if !limiter.check(&self.rate_limit_key) {
                return Err(LLMError::RateLimitExceeded(
                    "LLM request rate limit exceeded. Please try again later.".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// Set system message
    pub async fn set_system_message(&self, content: impl Into<String>) {
        let mut context = self.context.write().await;

        // Remove existing system message
        context.retain(|m| m.role != Role::System);

        // Add new system message at the beginning
        context.insert(0, Message::system(content));
    }

    /// CODE QUALITY FIX: Get current context size in tokens (estimated)
    pub async fn context_size(&self) -> usize {
        let context = self.context.read().await;
        context.iter().map(|m| m.text_content().len() / 4).sum() // Rough estimate: 1 token ~= 4 chars
    }

    /// CODE QUALITY FIX: Limit context window by removing oldest non-system
    /// messages
    pub async fn limit_context_window(&self, max_messages: usize) {
        let mut context = self.context.write().await;

        // Keep system messages and the most recent messages
        let system_messages: Vec<_> = context
            .iter()
            .filter(|m| m.role == Role::System)
            .cloned()
            .collect();

        let non_system_messages: Vec<_> = context
            .iter()
            .filter(|m| m.role != Role::System)
            .cloned()
            .collect();

        // If we have more non-system messages than max, truncate
        if non_system_messages.len() > max_messages {
            let start_index = non_system_messages.len() - max_messages;
            let recent_messages: Vec<_> = non_system_messages[start_index..].to_vec();

            *context = system_messages;
            context.extend(recent_messages);

            tracing::info!(
                "Context window truncated to {} messages (removed {} old messages)",
                context.len(),
                start_index
            );
        }
    }

    /// CODE QUALITY FIX: Truncate context if it exceeds a token limit
    pub async fn truncate_context(&self, max_tokens: usize) {
        let mut context = self.context.write().await;

        // Calculate total tokens (rough estimate)
        let total_tokens: usize = context.iter().map(|m| m.text_content().len() / 4).sum();

        if total_tokens <= max_tokens {
            return; // No truncation needed
        }

        // Keep system messages
        let system_messages: Vec<_> = context
            .iter()
            .filter(|m| m.role == Role::System)
            .cloned()
            .collect();

        let system_tokens: usize = system_messages
            .iter()
            .map(|m| m.text_content().len() / 4)
            .sum();

        // Calculate remaining token budget for non-system messages
        let remaining_budget = max_tokens.saturating_sub(system_tokens);

        // Get non-system messages in reverse order (newest first)
        let non_system: Vec<_> = context
            .iter()
            .filter(|m| m.role != Role::System)
            .rev()
            .cloned()
            .collect();

        // Keep adding messages until we hit the budget
        let mut kept_messages = Vec::new();
        let mut current_tokens = 0;

        for msg in non_system {
            let msg_tokens = msg.text_content().len() / 4;
            if current_tokens + msg_tokens > remaining_budget && !kept_messages.is_empty() {
                break; // Don't add more if we'd exceed budget
            }
            kept_messages.push(msg);
            current_tokens += msg_tokens;
        }

        // Reverse to restore chronological order
        kept_messages.reverse();

        // Rebuild context
        *context = system_messages;
        context.extend(kept_messages);

        tracing::info!(
            "Context truncated to ~{} tokens ({} messages)",
            max_tokens,
            context.len()
        );
    }

    /// Add a user message and get assistant response
    #[instrument(skip(self, message))]
    pub async fn chat(&self, message: impl Into<String>) -> LLMResult<String> {
        let user_msg = Message::user(message);

        // Add to context
        {
            let mut context = self.context.write().await;
            context.push(user_msg);
        }

        // Get response
        let response = self.execute_with_context().await?;

        // Add assistant response to context
        {
            let mut context = self.context.write().await;
            context.push(Message::assistant(&response));
        }

        Ok(response)
    }

    /// Chat with multimodal content
    pub async fn chat_multimodal(&self, contents: Vec<Content>) -> LLMResult<String> {
        let user_msg = Message::multimodal(Role::User, contents);

        {
            let mut context = self.context.write().await;
            context.push(user_msg);
        }

        let response = self.execute_with_context().await?;

        {
            let mut context = self.context.write().await;
            context.push(Message::assistant(&response));
        }

        Ok(response)
    }

    /// 🟢 P2 FIX: Chat with optional max_tokens override for dynamic token
    /// limiting
    pub async fn chat_with_max_tokens(
        &self,
        message: impl Into<String>,
        max_tokens: u32,
    ) -> LLMResult<String> {
        let user_msg = Message::user(message);

        {
            let mut context = self.context.write().await;
            context.push(user_msg);
        }

        let messages = self.context.read().await.clone();
        let mut request = LLMRequest {
            messages,
            config: RequestConfig {
                max_tokens: Some(max_tokens),
                ..self.config.clone()
            },
        };

        // Add tools if registered
        let tools = self.tools.read().await;
        if !tools.is_empty() {
            request.config.tools = Some(tools.values().map(|t| t.definition()).collect());
        }
        drop(tools);

        let start = std::time::Instant::now();
        let response = self.provider.complete(request).await?;
        let latency = start.elapsed();

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.total_requests += 1;
            metrics.successful_requests += 1;
            if let Some(usage) = &response.usage {
                metrics.total_tokens += usage.total_tokens as u64;
            }
            metrics.total_latency_ms += latency.as_millis() as u64;
        }

        // Handle tool calls if present (single-shot, no loop)
        let assistant_text = if let Some(choice) = response.choices.first() {
            if let Some(tool_calls) = &choice.message.tool_calls {
                let results = self.handle_tool_calls(tool_calls).await?;
                return Ok(results
                    .iter()
                    .map(|r| format!("{}: {}", r.tool_call_id, r.content))
                    .collect::<Vec<_>>()
                    .join("\n"));
            }
            choice.message.text_content()
        } else {
            return Err(LLMError::Provider("Empty response".to_string()));
        };

        // Add assistant response to context
        {
            let mut context = self.context.write().await;
            context.push(Message::assistant(&assistant_text));
        }

        Ok(assistant_text)
    }

    /// Execute with current context
    ///
    /// ARCHITECTURE FIX: Enforces rate limiting before making request.
    async fn execute_with_context(&self) -> LLMResult<String> {
        // Check rate limit
        self.check_rate_limit()?;

        let messages = self.context.read().await.clone();
        let mut request = LLMRequest {
            messages,
            config: self.config.clone(),
        };

        // Add tools if registered
        let tools = self.tools.read().await;
        if !tools.is_empty() {
            request.config.tools = Some(tools.values().map(|t| t.definition()).collect());
        }
        drop(tools);

        let start = std::time::Instant::now();

        let response = self.provider.complete(request).await?;

        let latency = start.elapsed();

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.total_requests += 1;
            metrics.successful_requests += 1;
            if let Some(usage) = &response.usage {
                metrics.total_tokens += usage.total_tokens as u64;
            }
            metrics.total_latency_ms += latency.as_millis() as u64;
        }

        // Handle tool calls if present
        if let Some(choice) = response.choices.first() {
            if let Some(tool_calls) = &choice.message.tool_calls {
                let results = self.handle_tool_calls(tool_calls).await?;
                return Ok(results
                    .iter()
                    .map(|r| format!("{}: {}", r.tool_call_id, r.content))
                    .collect::<Vec<_>>()
                    .join("\n"));
            }

            return Ok(choice.message.text_content());
        }

        Err(LLMError::Provider("Empty response".to_string()))
    }

    /// Handle tool calls and return structured results for tool-loop use.
    async fn handle_tool_calls(
        &self,
        tool_calls: &[ToolCall],
    ) -> LLMResult<Vec<ToolResult>> {
        let tools = self.tools.read().await;
        let mut results = Vec::with_capacity(tool_calls.len());

        for tool_call in tool_calls {
            tracing::info!(
                "Executing tool '{}' with {} bytes of arguments",
                tool_call.function.name,
                tool_call.function.arguments.len()
            );
            if let Some(handler) = tools.get(&tool_call.function.name) {
                match handler.execute(&tool_call.function.arguments).await {
                    Ok(result) => {
                        results.push(ToolResult {
                            tool_call_id: tool_call.id.clone(),
                            content: result,
                        });
                    }
                    Err(e) => {
                        warn!(
                            "Tool '{}' execution failed: {} (args: {})",
                            tool_call.function.name, e, tool_call.function.arguments
                        );
                        results.push(ToolResult {
                            tool_call_id: tool_call.id.clone(),
                            content: format!("Error: {}", e),
                        });
                    }
                }
            } else {
                results.push(ToolResult {
                    tool_call_id: tool_call.id.clone(),
                    content: format!(
                        "Error: Tool '{}' not found",
                        tool_call.function.name
                    ),
                });
            }
        }

        Ok(results)
    }

    /// Execute a single LLM request with tool support, returning either the
    /// assistant text or a Vec of ToolResults if the model requested tools.
    async fn execute_once_with_tools(
        &self,
        messages: Vec<Message>,
    ) -> LLMResult<TurnOutcome> {
        self.check_rate_limit()?;

        let mut request = LLMRequest {
            messages,
            config: self.config.clone(),
        };

        let tools = self.tools.read().await;
        if !tools.is_empty() {
            request.config.tools = Some(tools.values().map(|t| t.definition()).collect());
            // 🆕 FIX: Explicitly set tool_choice to "auto" so the model knows
            // it can use tools. Some providers (e.g. Kimi) require this.
            request.config.tool_choice = Some(super::types::ToolChoice::Auto("auto".to_string()));
        }
        drop(tools);

        let start = std::time::Instant::now();
        info!("[LLM-TRACE] LLMClient calling provider.complete with {} tools", request.config.tools.as_ref().map(|t| t.len()).unwrap_or(0));
        let response = self.provider.complete(request).await?;
        let latency = start.elapsed();
        info!("[LLM-TRACE] Provider.complete returned in {:?}", latency);

        {
            let mut metrics = self.metrics.write().await;
            metrics.total_requests += 1;
            metrics.successful_requests += 1;
            if let Some(usage) = &response.usage {
                metrics.total_tokens += usage.total_tokens as u64;
            }
            metrics.total_latency_ms += latency.as_millis() as u64;
        }

        if let Some(choice) = response.choices.first() {
            if let Some(tool_calls) = &choice.message.tool_calls {
                let results = self.handle_tool_calls(tool_calls).await?;
                // 🆕 FIX: Include the assistant message with tool_calls
                // so run_tool_loop can append it to the conversation.
                let assistant_msg = choice.message.clone();
                return Ok(TurnOutcome::ToolResults(results, assistant_msg));
            }
            return Ok(TurnOutcome::Text(choice.message.text_content()));
        }

        Err(LLMError::Provider("Empty response".to_string()))
    }

    /// Chat with automatic tool-call loop.
    ///
    /// Adds the user message to the internal context, then runs the
    /// conversation loop: if the model requests tools, execute them and
    /// feed results back, repeat until the model returns text or max
    /// iterations is reached.
    pub async fn chat_with_tools(&self, message: impl Into<String>) -> LLMResult<String> {
        let user_msg = Message::user(message);
        {
            let mut context = self.context.write().await;
            context.push(user_msg);
        }

        let response = self.run_tool_loop_with_context().await?;

        {
            let mut context = self.context.write().await;
            context.push(Message::assistant(&response));
        }

        Ok(response)
    }

    /// Chat with automatic tool-call loop using the provided messages.
    ///
    /// Unlike `chat_with_tools`, this does NOT modify the internal context.
    /// Use this when the caller has already built the full message list
    /// (e.g. the Agent with persona + memory + skills prompt).
    pub async fn chat_with_tools_and_messages(
        &self,
        messages: Vec<Message>,
    ) -> LLMResult<String> {
        self.run_tool_loop(messages).await
    }

    /// Core tool-call loop. Reusable for both internal-context and
    /// external-message modes.
    async fn run_tool_loop_with_context(&self) -> LLMResult<String> {
        let messages = self.context.read().await.clone();
        self.run_tool_loop(messages).await
    }

    /// Core tool-call loop implementation.
    ///
    /// 1. Send messages + tool definitions to LLM
    /// 2. If tool_calls present: execute each tool in parallel, append
    ///    results as `Role::Tool` messages, go to step 1
    /// 3. If text response: return it
    /// 4. Max 10 iterations to prevent infinite loops
    async fn run_tool_loop(&self, mut messages: Vec<Message>) -> LLMResult<String> {
        const MAX_ITERATIONS: usize = 10;

        for iteration in 0..MAX_ITERATIONS {
            match self.execute_once_with_tools(messages.clone()).await? {
                TurnOutcome::Text(text) => {
                    return Ok(text);
                }
                TurnOutcome::ToolResults(tool_results, assistant_msg) => {
                    // 🆕 FIX: Append the assistant message that requested the
                    // tool calls, so the API can match tool_call_ids.
                    messages.push(assistant_msg);

                    // Add each tool result as a Tool message
                    for result in tool_results {
                        messages.push(Message::tool(&result.tool_call_id, &result.content));
                    }

                    tracing::info!(
                        "Tool loop iteration {}/{}, continuing conversation",
                        iteration + 1,
                        MAX_ITERATIONS
                    );
                }
            }
        }

        Err(LLMError::Provider(
            "Tool-call loop exceeded maximum iterations".to_string(),
        ))
    }

    /// Peek at the internal context to find the last assistant message
    /// that contains tool_calls. Used to reconstruct the conversation
    /// for the tool loop.
    async fn find_last_assistant_with_tools(&self) -> Option<Message> {
        let context = self.context.read().await;
        context
            .iter()
            .rev()
            .find(|m| m.role == Role::Assistant && m.tool_calls.is_some())
            .cloned()
    }

    /// Stream chat response
    pub async fn chat_stream(
        &self,
        message: impl Into<String>,
    ) -> LLMResult<mpsc::Receiver<String>> {
        let user_msg = Message::user(message);

        {
            let mut context = self.context.write().await;
            context.push(user_msg);
        }

        let messages = self.context.read().await.clone();
        let request = LLMRequest {
            messages,
            config: RequestConfig {
                stream: Some(true),
                ..self.config.clone()
            },
        };

        let mut chunk_rx = self.provider.complete_stream(request).await?;
        let (tx, rx) = mpsc::channel(100);

        // Convert StreamChunk to String
        tokio::spawn(async move {
            let mut full_response = String::new();

            loop {
                match chunk_rx.recv().await {
                    Some(chunk) => {
                        for choice in chunk.choices {
                            if let Some(content) = choice.delta.content {
                                full_response.push_str(&content);
                                if tx.send(content).await.is_err() {
                                    return;
                                }
                            }

                            if choice.finish_reason.is_some() {
                                return;
                            }
                        }
                    }
                    None => break,
                }
            }
        });

        Ok(rx)
    }

    /// Register a tool
    pub async fn register_tool(&self, name: impl Into<String>, handler: Box<dyn ToolHandler>) {
        let mut tools = self.tools.write().await;
        tools.insert(name.into(), handler);
    }

    /// Clear conversation context
    pub async fn clear_context(&self) {
        let mut context = self.context.write().await;
        context.clear();
    }

    /// Get conversation history
    pub async fn get_history(&self) -> Vec<Message> {
        self.context.read().await.clone()
    }

    /// Get metrics
    pub async fn get_metrics(&self) -> ClientMetrics {
        self.metrics.read().await.clone()
    }

    /// Get provider capabilities
    pub fn capabilities(&self) -> ProviderCapabilities {
        self.provider.capabilities()
    }

    /// Health check
    pub async fn health_check(&self) -> LLMResult<()> {
        self.provider.health_check().await
    }
}

/// Builder for LLMClient
pub struct LLMClientBuilder {
    provider: Option<Arc<dyn LLMProvider>>,
    config: RequestConfig,
    system_message: Option<String>,
}

impl LLMClientBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            provider: None,
            config: RequestConfig::default(),
            system_message: None,
        }
    }

    /// Set provider
    pub fn provider(mut self, provider: Arc<dyn LLMProvider>) -> Self {
        self.provider = Some(provider);
        self
    }

    /// Set model
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.config.model = model.into();
        self
    }

    /// Set temperature
    pub fn temperature(mut self, temp: f32) -> Self {
        self.config.temperature = Some(temp);
        self
    }

    /// Set max tokens
    pub fn max_tokens(mut self, tokens: u32) -> Self {
        self.config.max_tokens = Some(tokens);
        self
    }

    /// Set system message
    pub fn system_message(mut self, message: impl Into<String>) -> Self {
        self.system_message = Some(message.into());
        self
    }

    /// Build the client
    pub async fn build(self) -> LLMResult<LLMClient> {
        let provider = self
            .provider
            .ok_or_else(|| LLMError::InvalidRequest("Provider required".to_string()))?;

        let client = LLMClient::new(provider).with_config(self.config);

        if let Some(sys_msg) = self.system_message {
            client.set_system_message(sys_msg).await;
        }

        Ok(client)
    }
}

impl Default for LLMClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

use tokio::sync::mpsc;
