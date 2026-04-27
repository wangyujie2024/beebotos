//! LLM Adapter - Bridge between old and new LLM interfaces
//!
//! Provides compatibility between the legacy `LLMCallInterface` and
//! the new comprehensive `LLMClient`.

use std::collections::HashMap;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::communication::{LLMCallInterface, Message as CommMessage};
use crate::error::Result as AgentResult;
use crate::llm::{LLMClient, Message as LLMMessage, Role};

/// Adapter that wraps the new LLMClient to implement the legacy
/// LLMCallInterface
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
        let response = if let Some(ref ctx) = context {
            if let Some(max_tokens_str) = ctx.get("max_tokens") {
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

        let rx = self
            .client
            .chat_stream(&last_msg)
            .await
            .map_err(|e| crate::error::AgentError::Execution(e.to_string()))?;

        Ok(rx)
    }
}

/// Builder for creating LLMClient with legacy interface
pub struct LegacyLLMClientBuilder;

impl LegacyLLMClientBuilder {
    /// Build from environment (OpenAI-compatible)
    pub async fn from_env() -> AgentResult<Box<dyn LLMCallInterface>> {
        let client = crate::llm::create_openai_client()
            .await
            .map_err(|e| crate::error::AgentError::InvalidConfig(e.to_string()))?;

        Ok(Box::new(LLMClientAdapter::new(client)))
    }

    /// Build with custom client
    pub fn with_client(client: LLMClient) -> Box<dyn LLMCallInterface> {
        Box::new(LLMClientAdapter::new(client))
    }
}
