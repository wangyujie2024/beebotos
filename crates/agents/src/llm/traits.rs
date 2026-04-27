//! LLM Provider Traits
//!
//! Defines the interface that all LLM providers must implement,
//! supporting both synchronous and streaming responses.

use async_trait::async_trait;
use tokio::sync::mpsc;

use super::types::*;

/// Core LLM provider trait
///
/// All LLM providers (Kimi, OpenAI, Anthropic, etc.) implement this trait.
/// It provides methods for both standard and streaming completions.
#[async_trait]
pub trait LLMProvider: Send + Sync {
    /// Get provider name
    fn name(&self) -> &str;

    /// Get provider capabilities
    fn capabilities(&self) -> ProviderCapabilities;

    /// Complete a chat request (non-streaming)
    ///
    /// # Arguments
    /// * `request` - The completion request with messages and config
    ///
    /// # Returns
    /// The LLM response or an error
    async fn complete(&self, request: LLMRequest) -> LLMResult<LLMResponse>;

    /// Complete a chat request with streaming
    ///
    /// # Arguments
    /// * `request` - The completion request
    ///
    /// # Returns
    /// A channel receiver that streams response chunks
    async fn complete_stream(&self, request: LLMRequest) -> LLMResult<mpsc::Receiver<StreamChunk>>;

    /// Check if the provider is healthy/available
    async fn health_check(&self) -> LLMResult<()>;

    /// Get list of available models
    async fn list_models(&self) -> LLMResult<Vec<ModelInfo>>;
}

/// Provider capabilities
#[derive(Debug, Clone, Default)]
pub struct ProviderCapabilities {
    /// Supports streaming responses
    pub streaming: bool,
    /// Supports function/tool calling
    pub function_calling: bool,
    /// Supports vision/multimodal inputs
    pub vision: bool,
    /// Supports JSON mode
    pub json_mode: bool,
    /// Supports system messages
    pub system_messages: bool,
    /// Maximum context length
    pub max_context_length: usize,
    /// Maximum output tokens
    pub max_output_tokens: usize,
}

/// Model information
#[derive(Debug, Clone)]
pub struct ModelInfo {
    /// Model ID
    pub id: String,
    /// Model name (human-readable)
    pub name: String,
    /// Model description
    pub description: Option<String>,
    /// Context window size
    pub context_window: usize,
    /// Max output tokens
    pub max_tokens: usize,
    /// Capabilities
    pub capabilities: ModelCapabilities,
    /// Pricing per 1K tokens (input, output)
    pub pricing: Option<(f64, f64)>,
}

/// Model-specific capabilities
#[derive(Debug, Clone, Default)]
pub struct ModelCapabilities {
    /// Supports vision
    pub vision: bool,
    /// Supports function calling
    pub function_calling: bool,
    /// Supports JSON mode
    pub json_mode: bool,
}

/// Extended interface for providers supporting tool execution
#[async_trait]
pub trait ToolExecutor: LLMProvider {
    /// Execute a tool and return the result
    ///
    /// # Arguments
    /// * `tool_call` - The tool call from the model
    ///
    /// # Returns
    /// The tool execution result
    async fn execute_tool(&self, tool_call: &ToolCall) -> LLMResult<ToolResult>;
}

/// Context manager for conversation state
///
/// Manages conversation history and context window
pub trait ContextManager: Send + Sync {
    /// Add a message to the context
    fn add_message(&mut self, message: Message);

    /// Get all messages in context
    fn get_messages(&self) -> &[Message];

    /// Clear the context
    fn clear(&mut self);

    /// Trim context to fit within token limit
    ///
    /// Returns the number of messages removed
    fn trim_to_fit(&mut self, max_tokens: usize) -> usize;

    /// Get estimated token count
    fn estimated_tokens(&self) -> usize;
}

/// Retry policy for failed requests
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of retries
    pub max_retries: u32,
    /// Base delay between retries
    pub base_delay: std::time::Duration,
    /// Maximum delay between retries
    pub max_delay: std::time::Duration,
    /// Multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// Whether to retry on rate limit
    pub retry_on_rate_limit: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: std::time::Duration::from_millis(500),
            max_delay: std::time::Duration::from_secs(30),
            backoff_multiplier: 2.0,
            retry_on_rate_limit: true,
        }
    }
}

impl RetryPolicy {
    /// Create a no-retry policy
    pub fn none() -> Self {
        Self {
            max_retries: 0,
            ..Default::default()
        }
    }

    /// Calculate delay for a specific retry attempt
    pub fn delay_for_attempt(&self, attempt: u32) -> std::time::Duration {
        let exponential =
            self.base_delay.as_millis() as f64 * self.backoff_multiplier.powi(attempt as i32);
        let jitter = rand::random::<f64>() * 0.1 * exponential; // 10% jitter
        let delay_ms = (exponential + jitter) as u64;
        std::time::Duration::from_millis(delay_ms.min(self.max_delay.as_millis() as u64))
    }

    /// Check if we should retry based on error
    pub fn should_retry(&self, error: &LLMError, attempt: u32) -> bool {
        if attempt >= self.max_retries {
            return false;
        }

        match error {
            LLMError::RateLimit { .. } => self.retry_on_rate_limit,
            LLMError::Network(_) | LLMError::Timeout => true,
            LLMError::Api { code, .. } => {
                // Retry on server errors (5xx) and rate limits (429)
                matches!(code, 429 | 500..=599)
            }
            _ => false,
        }
    }
}

/// Metrics collector for LLM calls
#[async_trait]
pub trait MetricsCollector: Send + Sync {
    /// Record a successful request
    async fn record_success(
        &self,
        provider: &str,
        model: &str,
        latency: std::time::Duration,
        input_tokens: u32,
        output_tokens: u32,
    );

    /// Record a failed request
    async fn record_failure(
        &self,
        provider: &str,
        model: &str,
        error: &LLMError,
        latency: std::time::Duration,
    );

    /// Record a streaming chunk
    async fn record_stream_chunk(&self, provider: &str, model: &str, chunk_size: usize);
}
