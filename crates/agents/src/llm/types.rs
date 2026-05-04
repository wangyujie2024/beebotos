//! LLM Types - Comprehensive type definitions for LLM interactions
//!
//! Supports OpenAI-compatible APIs including Kimi, with full support for:
//! - Text and multimodal (vision) inputs
//! - Tool/function calling
//! - Streaming responses
//! - Structured outputs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Role in a conversation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// OpenAI-compatible image URL content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrlContent {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Content types for messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Content {
    /// Text content
    Text { text: String },
    /// Image URL content (OpenAI-compatible format)
    ImageUrl { image_url: ImageUrlContent },
    /// File content
    File {
        /// File name
        name: String,
        /// File content or URL
        source: String,
    },
}

/// A message in the LLM conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMMessage {
    /// Message role
    pub role: Role,
    /// Message content (can be string for simple text, or array for multimodal)
    #[serde(with = "serde_content")]
    pub content: Vec<Content>,
    /// Optional name for the message sender (for multi-user conversations)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Tool calls (for assistant messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Tool call ID (for tool messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// 🆕 FIX: Kimi k2.6 reasoning_content — model may consume all tokens here leaving content empty
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

/// Type alias for backward compatibility
pub type Message = LLMMessage;

impl LLMMessage {
    /// Create a text message
    pub fn text(role: Role, text: impl Into<String>) -> Self {
        Self {
            role,
            content: vec![Content::Text { text: text.into() }],
            name: None,
            tool_calls: None,
            tool_call_id: None,
            reasoning_content: None,
        }
    }

    /// Create a system message
    pub fn system(text: impl Into<String>) -> Self {
        Self::text(Role::System, text)
    }

    /// Create a user message
    pub fn user(text: impl Into<String>) -> Self {
        Self::text(Role::User, text)
    }

    /// Create an assistant message
    pub fn assistant(text: impl Into<String>) -> Self {
        Self::text(Role::Assistant, text)
    }

    /// Create a multimodal message with text and image
    pub fn multimodal(role: Role, contents: Vec<Content>) -> Self {
        Self {
            role,
            content: contents,
            name: None,
            tool_calls: None,
            tool_call_id: None,
            reasoning_content: None,
        }
    }

    /// Add an image URL to the message
    pub fn with_image(mut self, url: impl Into<String>) -> Self {
        self.content.push(Content::ImageUrl {
            image_url: ImageUrlContent {
                url: url.into(),
                detail: None,
            },
        });
        self
    }

    /// Set name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Add tool calls to the message
    pub fn with_tool_calls(mut self, tool_calls: Vec<ToolCall>) -> Self {
        self.tool_calls = Some(tool_calls);
        self
    }

    /// Get text content (concatenates all text parts)
    pub fn text_content(&self) -> String {
        let text = self.content
            .iter()
            .filter_map(|c| match c {
                Content::Text { text } => Some(text.as_str()),
                Content::ImageUrl { .. } | Content::File { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("");
        // 🆕 FIX: Fallback to reasoning_content when content is empty (Kimi k2.6)
        if text.is_empty() {
            if let Some(ref reasoning) = self.reasoning_content {
                return reasoning.clone();
            }
        }
        text
    }
}

/// Custom serializer for content that supports both string and array formats
mod serde_content {
    use super::{Content, Serialize, Deserialize};
    use serde::{Deserializer, Serializer};
    use serde::de::{self, Visitor};
    use std::fmt;

    pub fn serialize<S: Serializer>(content: &Vec<Content>, serializer: S) -> Result<S::Ok, S::Error> {
        // Serialize single text as string for backward compatibility,
        // otherwise serialize as array
        if content.len() == 1 {
            if let Some(Content::Text { text }) = content.first() {
                return serializer.serialize_str(text);
            }
        }
        content.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Content>, D::Error> {
        struct ContentVisitor;

        impl<'de> Visitor<'de> for ContentVisitor {
            type Value = Vec<Content>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string or an array of content objects")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(vec![Content::Text { text: value.to_string() }])
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(vec![Content::Text { text: value }])
            }

            fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                Vec::<Content>::deserialize(de::value::SeqAccessDeserializer::new(seq))
            }
        }

        deserializer.deserialize_any(ContentVisitor)
    }
}

/// Tool definition for function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    /// Tool type (usually "function")
    pub r#type: String,
    /// Function definition
    pub function: FunctionDefinition,
}

/// Function definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// Function name
    pub name: String,
    /// Function description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema for parameters
    pub parameters: serde_json::Value,
}

/// A tool call from the assistant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool call ID
    pub id: String,
    /// Tool type
    pub r#type: String,
    /// Function call
    pub function: FunctionCall,
}

/// Function call details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// Function name
    pub name: String,
    /// Function arguments as JSON string
    pub arguments: String,
}

impl FunctionCall {
    /// Parse arguments as JSON
    pub fn parse_arguments<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_str(&self.arguments)
    }
}

/// Tool result to send back
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Tool call ID
    pub tool_call_id: String,
    /// Tool output
    pub content: String,
}

/// Response format configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseFormat {
    /// Plain text (default)
    Text,
    /// JSON object
    JsonObject {
        /// Optional JSON schema
        #[serde(skip_serializing_if = "Option::is_none")]
        schema: Option<serde_json::Value>,
    },
}

/// LLM request configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestConfig {
    /// Model to use
    pub model: String,
    /// Temperature (0.0 - 2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Top-p sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Maximum tokens to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Stop sequences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    /// Whether to stream the response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Response format
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
    /// Tools available for function calling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    /// Tool choice (auto, none, or specific tool)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// Presence penalty
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    /// Frequency penalty
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    /// Seed for deterministic sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Additional parameters
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl Default for RequestConfig {
    fn default() -> Self {
        Self {
            model: "kimi-latest".to_string(),
            temperature: Some(0.7),
            top_p: None,
            max_tokens: Some(2048),
            stop: None,
            stream: Some(false),
            response_format: None,
            tools: None,
            tool_choice: None,
            presence_penalty: None,
            frequency_penalty: None,
            seed: None,
            extra: HashMap::new(),
        }
    }
}

/// Tool choice options
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    /// Auto - let the model decide
    Auto(String),
    /// None - don't use tools
    None(String),
    /// Required - must use a tool
    Required(String),
    /// Specific tool
    Specific { 
        r#type: String, 
        function: FunctionChoice 
    },
}

/// Function choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionChoice {
    pub name: String,
}

/// Full LLM request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMRequest {
    /// Conversation messages
    pub messages: Vec<Message>,
    /// Request configuration
    #[serde(flatten)]
    pub config: RequestConfig,
}

/// LLM response
#[derive(Debug, Clone, Deserialize)]
pub struct LLMResponse {
    /// Response ID
    pub id: String,
    /// Object type
    pub object: String,
    /// Creation timestamp
    pub created: u64,
    /// Model used
    pub model: String,
    /// Response choices
    pub choices: Vec<Choice>,
    /// Usage statistics
    pub usage: Option<Usage>,
}

/// Response choice
#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    /// Choice index
    pub index: u32,
    /// Message
    pub message: Message,
    /// Finish reason
    pub finish_reason: Option<String>,
    /// Log probabilities (if requested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<serde_json::Value>,
}

/// Token usage statistics
#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    /// Prompt tokens
    pub prompt_tokens: u32,
    /// Completion tokens
    pub completion_tokens: u32,
    /// Total tokens
    pub total_tokens: u32,
}

/// Streaming response chunk
#[derive(Debug, Clone, Deserialize)]
pub struct StreamChunk {
    /// Chunk ID
    pub id: String,
    /// Object type
    pub object: String,
    /// Creation timestamp
    pub created: u64,
    /// Model
    pub model: String,
    /// Choices in this chunk
    pub choices: Vec<StreamChoice>,
}

impl StreamChunk {
    /// Create a new stream chunk with content
    pub fn new(id: String, content: String, finish_reason: Option<String>) -> Self {
        Self {
            id: id.clone(),
            object: "chat.completion.chunk".to_string(),
            created: chrono::Utc::now().timestamp() as u64,
            model: String::new(),
            choices: vec![StreamChoice {
                index: 0,
                delta: Delta {
                    role: None,
                    content: Some(content),
                    tool_calls: None,
                },
                finish_reason,
            }],
        }
    }

    /// Get content from the first choice's delta
    pub fn content(&self) -> Option<&str> {
        self.choices.first()?.delta.content.as_deref()
    }

    /// Get finish reason from the first choice
    pub fn finish_reason(&self) -> Option<&str> {
        self.choices.first()?.finish_reason.as_deref()
    }
}

/// Streaming choice
#[derive(Debug, Clone, Deserialize)]
pub struct StreamChoice {
    /// Choice index
    pub index: u32,
    /// Delta (content update)
    pub delta: Delta,
    /// Finish reason
    pub finish_reason: Option<String>,
}

/// Delta content in streaming response
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Delta {
    /// Role (usually only in first chunk)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    /// Content text
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Tool calls
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// LLM error types
#[derive(Debug, thiserror::Error)]
pub enum LLMError {
    #[error("API error: {code} - {message}")]
    Api { code: u16, message: String },
    #[error("Rate limit exceeded. Retry after: {retry_after:?}")]
    RateLimit { retry_after: Option<u64> },
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    #[error("Authentication error: {0}")]
    Auth(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Timeout")]
    Timeout,
    #[error("Streaming error: {0}")]
    Stream(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Provider error: {0}")]
    Provider(String),
    #[error("Not implemented: {0}")]
    NotImplemented(String),
    #[error("Context length exceeded: {0}")]
    ContextLengthExceeded(String),
    #[error("Model not found: {0}")]
    ModelNotFound(String),
    /// ARCHITECTURE FIX: Client-side rate limit exceeded
    #[error("Client rate limit exceeded: {0}")]
    RateLimitExceeded(String),
}

/// Result type for LLM operations
pub type LLMResult<T> = Result<T, LLMError>;

/// Kimi-specific model names
pub mod kimi_models {
    pub const KIMI_LATEST: &str = "kimi-latest";
    pub const KIMI_FLASH: &str = "kimi-flash";
    pub const KIMI_PRO: &str = "kimi-k2.6";
    pub const KIMI_K2_5: &str = "kimi-k2.6";
}

/// OpenAI model names (for compatibility)
pub mod openai_models {
    pub const GPT_4O: &str = "gpt-4o";
    pub const GPT_4O_MINI: &str = "gpt-4o-mini";
    pub const GPT_4_TURBO: &str = "gpt-4-turbo";
    pub const O1: &str = "o1";
    pub const O3_MINI: &str = "o3-mini";
}
