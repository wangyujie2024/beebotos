//! Context Assembler Module
//!
//! Provides functionality for assembling conversation context with attachments,
//! managing token limits, and retrieving historical messages for LLM
//! interactions.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::media::attachment::ParsedAttachment;
use crate::media::formatter::{FormattedMessage, MessageFormatter, MessageRole};

/// Assembled context ready for LLM consumption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssembledContext {
    /// Context ID
    pub id: String,
    /// Session or conversation ID
    pub session_id: String,
    /// Assembled messages
    pub messages: Vec<ContextMessage>,
    /// Total token count estimate
    pub total_tokens: usize,
    /// Context window limit
    pub context_window: usize,
    /// Metadata about the assembly
    pub metadata: ContextMetadata,
    /// Timestamp of assembly
    pub assembled_at: chrono::DateTime<chrono::Utc>,
}

/// Context message with full content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMessage {
    /// Message ID
    pub id: String,
    /// Message role
    pub role: MessageRole,
    /// Message content
    pub content: String,
    /// Attachments
    pub attachments: Vec<ParsedAttachment>,
    /// Token count estimate
    pub token_count: usize,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Context assembly metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMetadata {
    /// Number of messages included
    pub message_count: usize,
    /// Number of messages truncated
    pub truncated_count: usize,
    /// Number of attachments
    pub attachment_count: usize,
    /// Assembly strategy used
    pub strategy: AssemblyStrategy,
    /// Whether context was truncated
    pub was_truncated: bool,
}

impl Default for ContextMetadata {
    fn default() -> Self {
        Self {
            message_count: 0,
            truncated_count: 0,
            attachment_count: 0,
            strategy: AssemblyStrategy::RecentFirst,
            was_truncated: false,
        }
    }
}

/// Assembly strategy for handling context window limits
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssemblyStrategy {
    /// Keep most recent messages, truncate older ones
    RecentFirst,
    /// Keep oldest messages, truncate newer ones
    OldestFirst,
    /// Keep system message and most recent, truncate middle
    SystemAndRecent,
    /// Keep messages with highest priority
    PriorityBased,
    /// Summarize older messages instead of truncating
    SummarizeOld,
}

impl std::fmt::Display for AssemblyStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssemblyStrategy::RecentFirst => write!(f, "recent_first"),
            AssemblyStrategy::OldestFirst => write!(f, "oldest_first"),
            AssemblyStrategy::SystemAndRecent => write!(f, "system_and_recent"),
            AssemblyStrategy::PriorityBased => write!(f, "priority_based"),
            AssemblyStrategy::SummarizeOld => write!(f, "summarize_old"),
        }
    }
}

/// Context assembler configuration
#[derive(Debug, Clone)]
pub struct AssemblerConfig {
    /// Maximum context window size in tokens
    pub context_window: usize,
    /// Reserve tokens for response
    pub response_reserve: usize,
    /// Assembly strategy
    pub strategy: AssemblyStrategy,
    /// Maximum messages to include
    pub max_messages: usize,
    /// Whether to include attachments
    pub include_attachments: bool,
    /// Token estimation ratio (characters per token)
    pub chars_per_token: f32,
}

impl Default for AssemblerConfig {
    fn default() -> Self {
        Self {
            context_window: 128_000, // Default to 128k context window
            response_reserve: 4096,
            strategy: AssemblyStrategy::SystemAndRecent,
            max_messages: 100,
            include_attachments: true,
            chars_per_token: 4.0, // Approximate ratio
        }
    }
}

impl AssemblerConfig {
    /// Create new config with context window size
    pub fn with_context_window(mut self, window: usize) -> Self {
        self.context_window = window;
        self
    }

    /// Set response reserve tokens
    pub fn with_response_reserve(mut self, reserve: usize) -> Self {
        self.response_reserve = reserve;
        self
    }

    /// Set assembly strategy
    pub fn with_strategy(mut self, strategy: AssemblyStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Set maximum messages
    pub fn with_max_messages(mut self, max: usize) -> Self {
        self.max_messages = max;
        self
    }

    /// Get available tokens for context
    pub fn available_tokens(&self) -> usize {
        self.context_window.saturating_sub(self.response_reserve)
    }
}

/// Context assembler for building LLM context
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ContextAssembler {
    config: AssemblerConfig,
    message_formatter: MessageFormatter,
}

impl ContextAssembler {
    /// Create a new context assembler with default config
    pub fn new() -> Self {
        Self {
            config: AssemblerConfig::default(),
            message_formatter: MessageFormatter::new(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: AssemblerConfig) -> Self {
        Self {
            config,
            message_formatter: MessageFormatter::new(),
        }
    }

    /// Get the assembler configuration
    pub fn config(&self) -> &AssemblerConfig {
        &self.config
    }

    /// Assemble context with attachments
    ///
    /// # Arguments
    /// * `session_id` - The session or conversation ID
    /// * `messages` - Historical messages to assemble
    /// * `new_message` - The new message to add
    /// * `attachments` - Optional attachments for the new message
    ///
    /// # Returns
    /// Assembled context ready for LLM consumption
    pub async fn assemble_with_attachments(
        &self,
        session_id: impl Into<String>,
        messages: Vec<ContextMessage>,
        new_message: impl Into<String>,
        attachments: Vec<ParsedAttachment>,
    ) -> Result<AssembledContext> {
        let session_id = session_id.into();
        let new_content: String = new_message.into();

        // Create new context message
        let token_count = self.estimate_tokens(&new_content, &attachments);
        let new_context_msg = ContextMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::User,
            content: new_content,
            attachments: attachments.clone(),
            token_count,
            timestamp: chrono::Utc::now(),
            metadata: HashMap::new(),
        };

        // Combine with historical messages
        let mut all_messages = messages;
        all_messages.push(new_context_msg);

        // Apply context window limits
        let (selected_messages, metadata) = self.apply_context_limits(all_messages).await?;

        // Calculate total tokens
        let total_tokens: usize = selected_messages.iter().map(|m| m.token_count).sum();

        Ok(AssembledContext {
            id: uuid::Uuid::new_v4().to_string(),
            session_id,
            messages: selected_messages,
            total_tokens,
            context_window: self.config.context_window,
            metadata,
            assembled_at: chrono::Utc::now(),
        })
    }

    /// Assemble context from formatted messages
    ///
    /// # Arguments
    /// * `session_id` - The session or conversation ID
    /// * `formatted_messages` - Pre-formatted messages
    ///
    /// # Returns
    /// Assembled context
    pub async fn assemble_from_formatted(
        &self,
        session_id: impl Into<String>,
        formatted_messages: Vec<FormattedMessage>,
    ) -> Result<AssembledContext> {
        let session_id = session_id.into();

        // Convert formatted messages to context messages
        let context_messages: Vec<ContextMessage> = formatted_messages
            .into_iter()
            .map(|fm| ContextMessage {
                id: fm.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                role: fm.role,
                content: fm.text.clone(),
                attachments: Vec::new(), // Attachments already processed in formatted messages
                token_count: self.estimate_tokens(&fm.text, &[]),
                timestamp: fm.timestamp,
                metadata: fm.metadata,
            })
            .collect();

        // Apply context limits
        let (selected_messages, metadata) = self.apply_context_limits(context_messages).await?;

        // Calculate total tokens
        let total_tokens: usize = selected_messages.iter().map(|m| m.token_count).sum();

        Ok(AssembledContext {
            id: uuid::Uuid::new_v4().to_string(),
            session_id,
            messages: selected_messages,
            total_tokens,
            context_window: self.config.context_window,
            metadata,
            assembled_at: chrono::Utc::now(),
        })
    }

    /// Retrieve historical messages based on criteria
    ///
    /// # Arguments
    /// * `session_id` - The session ID to retrieve from
    /// * `limit` - Maximum number of messages to retrieve
    /// * `before` - Optional timestamp to retrieve messages before
    ///
    /// # Returns
    /// List of historical context messages
    pub async fn retrieve_history(
        &self,
        _session_id: &str,
        limit: usize,
        before: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<ContextMessage>> {
        // This is a placeholder implementation
        // In a real implementation, this would query a database or memory store
        // For now, return an empty vector
        tracing::info!(
            "Retrieving history for session with limit {} before {:?}",
            limit,
            before
        );

        // TODO: Implement actual history retrieval from memory store
        Ok(Vec::new())
    }

    /// Estimate token count for text and attachments
    ///
    /// Uses a simple character-to-token ratio for estimation.
    /// More sophisticated tokenizers can be integrated here.
    ///
    /// # Arguments
    /// * `text` - The text content
    /// * `attachments` - List of attachments
    ///
    /// # Returns
    /// Estimated token count
    pub fn estimate_tokens(&self, text: &str, attachments: &[ParsedAttachment]) -> usize {
        // Estimate text tokens
        let text_tokens = (text.len() as f32 / self.config.chars_per_token).ceil() as usize;

        // Estimate attachment tokens (simplified)
        let attachment_tokens: usize = attachments
            .iter()
            .map(|att| match att.attachment_type {
                crate::media::attachment::AttachmentType::Image => 1000, /* Image description */
                // estimate
                crate::media::attachment::AttachmentType::Audio => 500,
                crate::media::attachment::AttachmentType::Video => 2000,
                crate::media::attachment::AttachmentType::Document => {
                    (att.file_size.unwrap_or(0) as f32 / self.config.chars_per_token).ceil()
                        as usize
                }
                _ => 100,
            })
            .sum();

        text_tokens + attachment_tokens
    }

    /// Apply context window limits to messages
    async fn apply_context_limits(
        &self,
        messages: Vec<ContextMessage>,
    ) -> Result<(Vec<ContextMessage>, ContextMetadata)> {
        let available_tokens = self.config.available_tokens();
        let mut metadata = ContextMetadata {
            message_count: 0,
            truncated_count: 0,
            attachment_count: messages.iter().map(|m| m.attachments.len()).sum(),
            strategy: self.config.strategy,
            was_truncated: false,
        };

        // If within limits, return all messages
        let total_tokens: usize = messages.iter().map(|m| m.token_count).sum();
        if total_tokens <= available_tokens && messages.len() <= self.config.max_messages {
            metadata.message_count = messages.len();
            return Ok((messages, metadata));
        }

        // Apply strategy
        let selected = match self.config.strategy {
            AssemblyStrategy::RecentFirst => {
                self.select_recent_first(messages, available_tokens, &mut metadata)
            }
            AssemblyStrategy::OldestFirst => {
                self.select_oldest_first(messages, available_tokens, &mut metadata)
            }
            AssemblyStrategy::SystemAndRecent => {
                self.select_system_and_recent(messages, available_tokens, &mut metadata)
            }
            AssemblyStrategy::PriorityBased => {
                self.select_priority_based(messages, available_tokens, &mut metadata)
            }
            AssemblyStrategy::SummarizeOld => {
                self.select_with_summarization(messages, available_tokens, &mut metadata)
                    .await?
            }
        };

        metadata.was_truncated = metadata.truncated_count > 0;
        metadata.message_count = selected.len();

        Ok((selected, metadata))
    }

    /// Select most recent messages first
    fn select_recent_first(
        &self,
        messages: Vec<ContextMessage>,
        available_tokens: usize,
        metadata: &mut ContextMetadata,
    ) -> Vec<ContextMessage> {
        let mut selected = Vec::new();
        let mut token_count = 0;

        // Iterate in reverse (most recent first)
        for msg in messages.into_iter().rev() {
            if selected.len() >= self.config.max_messages
                || token_count + msg.token_count > available_tokens
            {
                metadata.truncated_count += 1;
                continue;
            }

            token_count += msg.token_count;
            selected.push(msg);
        }

        // Reverse back to chronological order
        selected.reverse();
        selected
    }

    /// Select oldest messages first
    fn select_oldest_first(
        &self,
        messages: Vec<ContextMessage>,
        available_tokens: usize,
        metadata: &mut ContextMetadata,
    ) -> Vec<ContextMessage> {
        let mut selected = Vec::new();
        let mut token_count = 0;

        for msg in messages {
            if selected.len() >= self.config.max_messages
                || token_count + msg.token_count > available_tokens
            {
                metadata.truncated_count += 1;
                continue;
            }

            token_count += msg.token_count;
            selected.push(msg);
        }

        selected
    }

    /// Select system message and most recent
    fn select_system_and_recent(
        &self,
        messages: Vec<ContextMessage>,
        available_tokens: usize,
        metadata: &mut ContextMetadata,
    ) -> Vec<ContextMessage> {
        let mut selected = Vec::new();
        let mut token_count = 0;

        // First, find and include system message
        let system_messages: Vec<_> = messages
            .iter()
            .filter(|m| matches!(m.role, MessageRole::System))
            .cloned()
            .collect();

        for msg in &system_messages {
            if token_count + msg.token_count <= available_tokens {
                token_count += msg.token_count;
                selected.push(msg.clone());
            } else {
                metadata.truncated_count += 1;
            }
        }

        // Then add recent messages
        let non_system: Vec<_> = messages
            .into_iter()
            .filter(|m| !matches!(m.role, MessageRole::System))
            .rev()
            .collect();

        for msg in non_system {
            if selected.len() >= self.config.max_messages
                || token_count + msg.token_count > available_tokens
            {
                metadata.truncated_count += 1;
                continue;
            }

            token_count += msg.token_count;
            selected.push(msg);
        }

        // Reverse to maintain order
        let system_count = system_messages.len();
        let non_system_selected = selected.split_off(system_count);
        selected.extend(non_system_selected.into_iter().rev());

        selected
    }

    /// Select based on priority (placeholder implementation)
    fn select_priority_based(
        &self,
        messages: Vec<ContextMessage>,
        available_tokens: usize,
        metadata: &mut ContextMetadata,
    ) -> Vec<ContextMessage> {
        // For now, fall back to recent first
        // TODO: Implement priority scoring based on message content and metadata
        self.select_recent_first(messages, available_tokens, metadata)
    }

    /// Select with summarization of old messages
    async fn select_with_summarization(
        &self,
        messages: Vec<ContextMessage>,
        available_tokens: usize,
        metadata: &mut ContextMetadata,
    ) -> Result<Vec<ContextMessage>> {
        // For now, fall back to system and recent
        // TODO: Implement actual summarization using LLM
        tracing::info!(
            "Summarization strategy not yet implemented, falling back to system_and_recent"
        );
        Ok(self.select_system_and_recent(messages, available_tokens, metadata))
    }
}

impl Default for ContextAssembler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_message(role: MessageRole, content: &str, tokens: usize) -> ContextMessage {
        ContextMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role,
            content: content.to_string(),
            attachments: Vec::new(),
            token_count: tokens,
            timestamp: chrono::Utc::now(),
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_assembler_config() {
        let config = AssemblerConfig::default()
            .with_context_window(8192)
            .with_response_reserve(1024);

        assert_eq!(config.context_window, 8192);
        assert_eq!(config.response_reserve, 1024);
        assert_eq!(config.available_tokens(), 7168);
    }

    #[tokio::test]
    async fn test_assemble_with_attachments() {
        let assembler = ContextAssembler::new();
        let history = vec![
            create_test_message(MessageRole::System, "You are a helpful assistant", 10),
            create_test_message(MessageRole::User, "Hello", 5),
            create_test_message(MessageRole::Assistant, "Hi there!", 5),
        ];

        let context = assembler
            .assemble_with_attachments("session_1", history, "How are you?", vec![])
            .await
            .unwrap();

        assert_eq!(context.session_id, "session_1");
        assert_eq!(context.messages.len(), 4);
        assert!(context.total_tokens > 0);
    }

    #[test]
    fn test_estimate_tokens() {
        let assembler = ContextAssembler::new();
        let text = "Hello world"; // ~11 chars
        let tokens = assembler.estimate_tokens(text, &[]);

        // With 4 chars per token, should be ~3 tokens
        assert!(tokens >= 2 && tokens <= 5);
    }

    #[test]
    fn test_select_recent_first() {
        let assembler = ContextAssembler::new();
        let config = AssemblerConfig::default()
            .with_context_window(100)
            .with_strategy(AssemblyStrategy::RecentFirst);
        let assembler = ContextAssembler::with_config(config);

        let messages = vec![
            create_test_message(MessageRole::User, "Message 1", 50),
            create_test_message(MessageRole::User, "Message 2", 40),
            create_test_message(MessageRole::User, "Message 3", 30),
        ];

        let mut metadata = ContextMetadata::default();
        let selected = assembler.select_recent_first(messages, 80, &mut metadata);

        // Should include most recent messages that fit
        assert!(!selected.is_empty());
    }

    #[test]
    fn test_assembly_strategy_display() {
        assert_eq!(AssemblyStrategy::RecentFirst.to_string(), "recent_first");
        assert_eq!(
            AssemblyStrategy::SystemAndRecent.to_string(),
            "system_and_recent"
        );
    }
}
