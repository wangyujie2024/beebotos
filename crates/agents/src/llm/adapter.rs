//! LLM Adapter - Bridge between old and new LLM interfaces
//!
//! Provides compatibility between the legacy `LLMCallInterface` and
//! the new comprehensive `LLMClient`.

use std::collections::HashMap;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::communication::{LLMCallInterface, Message as CommMessage};
use crate::llm::{LLMClient, Message as LLMMessage, Role};
use crate::error::Result as AgentResult;

/// Adapter that wraps the new LLMClient to implement the legacy LLMCallInterface
pub struct LLMClientAdapter {
    client: LLMClient,
}

impl LLMClientAdapter {
    /// Create new adapter
    pub fn new(client: LLMClient) -> Self {
        Self { client }
    }

    /// Convert communication message to LLM message
    #[allow(dead_code)]
    fn convert_message(msg: CommMessage) -> LLMMessage {
        let role = match msg.platform {
            crate::communication::PlatformType::Custom => Role::System,
            _ => Role::User,
        };

        let mut message = LLMMessage::text(role, msg.content);

        if let Some(image_urls_json) = msg.metadata.get("image_urls") {
            if let Ok(image_urls) = serde_json::from_str::<Vec<String>>(image_urls_json) {
                for url in image_urls {
                    message = message.with_image(url);
                }
            }
        }

        message
    }
}

#[async_trait]
impl LLMCallInterface for LLMClientAdapter {
    async fn call_llm(
        &self,
        messages: Vec<CommMessage>,
        context: Option<HashMap<String, String>>,
    ) -> AgentResult<String> {
        // Build a combined prompt from all messages to preserve context.
        let prompt = messages
            .into_iter()
            .map(|m| m.content)
            .collect::<Vec<_>>()
            .join("\n\n");

        // 🟢 P2 FIX: Support dynamic max_tokens from context
        // 🟢 P1 OPTIMIZE: Use chat_one_shot when 'one_shot' flag is set to avoid
        // carrying heavy conversation history into skill command generation.
        let response = if let Some(ref ctx) = context {
            if ctx.get("one_shot").map(|s| s.as_str()) == Some("true") {
                self.client.chat_one_shot(&prompt).await
            } else if let Some(max_tokens_str) = ctx.get("max_tokens") {
                if let Ok(max_tokens) = max_tokens_str.parse::<u32>() {
                    self.client.chat_with_max_tokens(&prompt, max_tokens).await
                } else {
                    self.client.chat(&prompt).await
                }
            } else {
                self.client.chat(&prompt).await
            }
        } else {
            self.client.chat(&prompt).await
        }
        .map_err(|e| crate::error::AgentError::Execution(e.to_string()))?;

        Ok(response)
    }

    async fn call_llm_stream(
        &self,
        messages: Vec<CommMessage>,
        _context: Option<HashMap<String, String>>,
    ) -> AgentResult<mpsc::Receiver<String>> {
        // Convert messages and stream
        let last_msg = messages
            .into_iter()
            .last()
            .map(|m| m.content)
            .unwrap_or_default();

        let rx = self.client.chat_stream(&last_msg).await
            .map_err(|e| crate::error::AgentError::Execution(e.to_string()))?;

        Ok(rx)
    }

    fn supports_native_tools(&self) -> bool {
        self.client.capabilities().function_calling
    }

    async fn call_llm_with_tools(
        &self,
        messages: Vec<CommMessage>,
        tools: Vec<crate::communication::ToolDefinition>,
        _context: Option<HashMap<String, String>>,
    ) -> AgentResult<String> {
        // Preserve message roles by separating system context from user query.
        // System messages become the prompt prefix; the last user message is the actual input.
        let mut system_parts = Vec::new();
        let mut user_parts = Vec::new();
        for m in &messages {
            if m.metadata.get("role").map(|s| s.as_str()) == Some("system")
                || m.platform == crate::communication::PlatformType::Custom
            {
                system_parts.push(m.content.as_str());
            } else {
                user_parts.push(m.content.as_str());
            }
        }

        let prompt = if !system_parts.is_empty() {
            format!(
                "{system}\n\n{user}",
                system = system_parts.join("\n\n"),
                user = user_parts.join("\n\n")
            )
        } else {
            user_parts.join("\n\n")
        };

        let tool_handlers: Vec<Box<dyn crate::llm::ToolHandler>> = tools
            .into_iter()
            .map(|t| {
                Box::new(NativeToolAdapter {
                    name: t.name,
                    description: t.description,
                    parameters: t.parameters,
                }) as Box<dyn crate::llm::ToolHandler>
            })
            .collect();

        let result = self
            .client
            .chat_with_tools_react(&prompt, tool_handlers, 10)
            .await
            .map_err(|e| crate::error::AgentError::Execution(e.to_string()))?;

        Ok(result)
    }
}

/// Adapter that bridges communication::ToolDefinition to llm::traits::ToolHandler
struct NativeToolAdapter {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[async_trait::async_trait]
impl crate::llm::ToolHandler for NativeToolAdapter {
    fn definition(&self) -> crate::llm::types::Tool {
        crate::llm::types::Tool {
            r#type: "function".to_string(),
            function: crate::llm::types::FunctionDefinition {
                name: self.name.clone(),
                description: Some(self.description.clone()),
                parameters: self.parameters.clone(),
            },
        }
    }

    async fn execute(&self, _arguments: &str) -> Result<String, String> {
        Err(format!(
            "NativeToolAdapter for '{}' is a definition-only stub. \
             Real tool execution must be provided by the caller.",
            self.name
        ))
    }
}

/// Builder for creating LLMClient with legacy interface
pub struct LegacyLLMClientBuilder;

impl LegacyLLMClientBuilder {
    /// Build from environment (Kimi)
    pub async fn from_env() -> AgentResult<Box<dyn LLMCallInterface>> {
        let client = crate::llm::create_kimi_client().await
            .map_err(|e| crate::error::AgentError::InvalidConfig(e.to_string()))?;

        Ok(Box::new(LLMClientAdapter::new(client)))
    }

    /// Build with custom client
    pub fn with_client(client: LLMClient) -> Box<dyn LLMCallInterface> {
        Box::new(LLMClientAdapter::new(client))
    }
}
