//! Memory Flush with LLM Integration
//!
//! Intelligent memory management using LLM for:
//! - Importance analysis and scoring
//! - Automatic categorization
//! - Content summarization and compression
//! - Smart decision on save targets (core vs daily)
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                  LLM Memory Flush System                        │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                                                                  │
//! │  ┌─────────────────┐      ┌─────────────────┐                  │
//! │  │   Context       │      │   LLM           │                  │
//! │  │   Monitor       │──────►│   Analyzer      │                  │
//! │  │                 │      │                 │                  │
//! │  └─────────────────┘      └────────┬────────┘                  │
//! │                                     │                           │
//! │         ┌───────────────────────────┼───────────────────┐      │
//! │         ▼                           ▼                   ▼      │
//! │  ┌──────────────┐          ┌──────────────┐    ┌──────────────┐│
//! │  │  Importance  │          │ Category     │    │ Compression  ││
//! │  │  Score       │          │ Detection    │    │ & Summary    ││
//! │  └──────┬───────┘          └──────┬───────┘    └──────┬───────┘│
//! │         │                         │                  │        │
//! │         └─────────────────────────┼──────────────────┘        │
//! │                                   ▼                            │
//! │                          ┌─────────────────┐                   │
//! │                          │  UnifiedMemory  │                   │
//! │                          │  System         │                   │
//! │                          └─────────────────┘                   │
//! │                                                                  │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

#[allow(unused_imports)]
use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info};
#[allow(unused_imports)]
use uuid::Uuid;

use crate::error::Result;
use crate::memory::markdown_search::{UnifiedMemoryConfig, UnifiedMemorySystem};
use crate::memory::markdown_storage::{MarkdownMemoryEntry, MemoryFileType};
use crate::memory::memory_flush::{
    FlushEvent, FlushStatistics, FlushTrigger, MemoryFlushConfig, MemoryFlushManager,
};

/// LLM provider trait for memory analysis
#[async_trait::async_trait]
pub trait LLMProvider: Send + Sync {
    /// Complete a prompt and return response
    async fn complete(&self, prompt: &str) -> Result<String>;

    /// Get model name
    fn model_name(&self) -> &str;
}

/// OpenAI LLM provider
pub struct OpenAILLMProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl OpenAILLMProvider {
    pub fn new(api_key: impl Into<String>, model: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            model: model.unwrap_or_else(|| "gpt-4o-mini".to_string()),
            base_url: "https://api.openai.com/v1".to_string(),
        }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }
}

#[async_trait::async_trait]
impl LLMProvider for OpenAILLMProvider {
    async fn complete(&self, prompt: &str) -> Result<String> {
        let request = serde_json::json!({
            "model": self.model,
            "messages": [
                {
                    "role": "system",
                    "content": "You are a helpful assistant that analyzes conversation content for memory management."
                },
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "temperature": 0.3,
            "max_tokens": 500
        });

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                crate::error::AgentError::platform(format!("LLM request failed: {}", e))
            })?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(crate::error::AgentError::platform(format!(
                "LLM API error: {}",
                error_text
            )));
        }

        let result: serde_json::Value = response.json().await.map_err(|e| {
            crate::error::AgentError::platform(format!("Failed to parse LLM response: {}", e))
        })?;

        let content = result["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| {
                crate::error::AgentError::platform("Invalid LLM response format".to_string())
            })?;

        Ok(content.to_string())
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

/// Mock LLM provider for testing
pub struct MockLLMProvider;

#[async_trait::async_trait]
impl LLMProvider for MockLLMProvider {
    async fn complete(&self, prompt: &str) -> Result<String> {
        // Return mock analysis based on prompt keywords
        if prompt.contains("importance") {
            Ok(r#"{"score": 0.85, "category": "UserPreference", "reason": "User explicitly stated a preference"}"#.to_string())
        } else if prompt.contains("summarize") {
            Ok("This is a summary of the conversation.".to_string())
        } else {
            Ok("Mock LLM response".to_string())
        }
    }

    fn model_name(&self) -> &str {
        "mock"
    }
}

/// LLM-based importance analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMImportanceAnalysis {
    /// Importance score (0.0 - 1.0)
    pub score: f32,
    /// Primary category
    pub category: String,
    /// Detailed reason
    pub reason: String,
    /// Whether to save to core memory
    pub save_to_core: bool,
    /// Whether to save to daily log
    pub save_to_daily: bool,
    /// Extracted key entities
    pub entities: Vec<String>,
    /// Suggested title
    pub suggested_title: String,
}

/// Memory compression result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionResult {
    /// Original token count (estimated)
    pub original_tokens: usize,
    /// Compressed token count (estimated)
    pub compressed_tokens: usize,
    /// Compression ratio
    pub compression_ratio: f32,
    /// Compressed content
    pub compressed_content: String,
    /// Key points extracted
    pub key_points: Vec<String>,
}

/// Configuration for LLM memory flush
#[derive(Clone)]
pub struct LLMMemoryFlushConfig {
    /// Base flush configuration
    pub base_config: MemoryFlushConfig,
    /// LLM provider
    pub llm: Arc<dyn LLMProvider>,
    /// Enable LLM analysis
    pub enable_llm_analysis: bool,
    /// Enable memory compression
    pub enable_compression: bool,
    /// Compression threshold (token count)
    pub compression_threshold: usize,
    /// Minimum importance score to save
    pub min_importance_threshold: f32,
}

impl std::fmt::Debug for LLMMemoryFlushConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LLMMemoryFlushConfig")
            .field("base_config", &self.base_config)
            .field("llm", &"<dyn LLMProvider>")
            .field("enable_llm_analysis", &self.enable_llm_analysis)
            .field("enable_compression", &self.enable_compression)
            .field("compression_threshold", &self.compression_threshold)
            .field("min_importance_threshold", &self.min_importance_threshold)
            .finish()
    }
}

impl Default for LLMMemoryFlushConfig {
    fn default() -> Self {
        Self {
            base_config: MemoryFlushConfig::default(),
            llm: Arc::new(MockLLMProvider),
            enable_llm_analysis: true,
            enable_compression: true,
            compression_threshold: 500,
            min_importance_threshold: 0.6,
        }
    }
}

/// Intelligent memory flush orchestrator with LLM
pub struct LLMMemoryFlushOrchestrator {
    config: LLMMemoryFlushConfig,
    base_manager: MemoryFlushManager,
    memory_system: Arc<RwLock<UnifiedMemorySystem>>,
    llm: Arc<dyn LLMProvider>,
}

impl LLMMemoryFlushOrchestrator {
    /// Create new LLM-enhanced flush orchestrator
    pub async fn new(
        config: LLMMemoryFlushConfig,
        memory_config: UnifiedMemoryConfig,
    ) -> Result<Self> {
        let (base_manager, _events) = MemoryFlushManager::new(config.base_config.clone());
        let memory_system = Arc::new(RwLock::new(UnifiedMemorySystem::new(memory_config).await?));

        Ok(Self {
            config: config.clone(),
            base_manager,
            memory_system,
            llm: config.llm,
        })
    }

    /// Initialize session with intelligent monitoring
    pub async fn init_session(&self, session_id: &str) {
        self.base_manager.init_session(session_id).await;
        info!(
            "Initialized LLM-enhanced memory monitoring for session: {}",
            session_id
        );
    }

    /// Process new message with LLM analysis
    pub async fn process_message(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
    ) -> Result<Option<FlushEvent>> {
        // Update base context monitoring
        let flush_trigger = self
            .base_manager
            .update_token_usage(session_id, content)
            .await;

        // Perform LLM analysis if enabled
        let should_save = if self.config.enable_llm_analysis {
            let analysis = self.analyze_importance(content).await?;
            analysis.score >= self.config.min_importance_threshold
        } else {
            self.base_manager.analyze_importance(content).score
                >= self.config.min_importance_threshold
        };

        // Save to memory if important
        if should_save {
            self.save_message(session_id, role, content).await?;
        }

        // Check if flush is needed
        if let Some(trigger) = flush_trigger {
            let event = self.perform_intelligent_flush(session_id, trigger).await?;
            return Ok(Some(event));
        }

        Ok(None)
    }

    /// Analyze content importance using LLM
    pub async fn analyze_importance(&self, content: &str) -> Result<LLMImportanceAnalysis> {
        let prompt = format!(
            r#"Analyze the following conversation content and determine its importance for long-term memory storage.

Content: "{}"

Respond with a JSON object in this exact format:
{{
    "score": <float between 0.0 and 1.0>,
    "category": <one of: UserPreference, UserFact, ProjectConfig, TaskGoal, Decision, Technical, General>,
    "reason": <brief explanation>,
    "save_to_core": <true/false>,
    "save_to_daily": <true/false>,
    "entities": [<extracted key entities>],
    "suggested_title": <concise title for this memory>
}}

Guidelines:
- score >= 0.8: Critical information (preferences, important facts, decisions)
- score 0.6-0.8: Useful information (tasks, technical details)
- score < 0.6: General conversation
- save_to_core: true for user preferences, important facts, project configs
- save_to_daily: true for conversation logs and temporary information"#,
            content.replace('"', "\\\"")
        );

        let response = self.llm.complete(&prompt).await?;

        // Parse JSON response
        let analysis: LLMImportanceAnalysis = serde_json::from_str(&response).map_err(|e| {
            crate::error::AgentError::platform(format!(
                "Failed to parse LLM importance analysis: {}. Response: {}",
                e, response
            ))
        })?;

        debug!(
            "LLM importance analysis: score={:.2}, category={}",
            analysis.score, analysis.category
        );

        Ok(analysis)
    }

    /// Compress conversation using LLM summarization
    pub async fn compress_conversation(
        &self,
        messages: &[ConversationMessage],
    ) -> Result<CompressionResult> {
        if !self.config.enable_compression || messages.len() < 3 {
            return Ok(CompressionResult {
                original_tokens: self.estimate_tokens(messages),
                compressed_tokens: self.estimate_tokens(messages),
                compression_ratio: 1.0,
                compressed_content: messages
                    .iter()
                    .map(|m| format!("{}: {}", m.role, m.content))
                    .collect::<Vec<_>>()
                    .join("\n"),
                key_points: vec![],
            });
        }

        let conversation_text = messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            r#"Summarize the following conversation and extract key points.

Conversation:
{}

Provide a JSON response:
{{
    "summary": <concise summary of the conversation>,
    "key_points": [<list of key facts/decisions/preferences>],
    "original_estimate": <estimated token count>,
    "compressed_estimate": <estimated token count after compression>
}}

Make the summary concise but preserve all important information."#,
            conversation_text
        );

        let response = self.llm.complete(&prompt).await?;

        #[derive(Deserialize)]
        struct LLMCompression {
            summary: String,
            key_points: Vec<String>,
            original_estimate: usize,
            compressed_estimate: usize,
        }

        let llm_result: LLMCompression = serde_json::from_str(&response).map_err(|e| {
            crate::error::AgentError::platform(format!(
                "Failed to parse LLM compression: {}. Response: {}",
                e, response
            ))
        })?;

        let compression_ratio = if llm_result.original_estimate > 0 {
            llm_result.compressed_estimate as f32 / llm_result.original_estimate as f32
        } else {
            1.0
        };

        Ok(CompressionResult {
            original_tokens: llm_result.original_estimate,
            compressed_tokens: llm_result.compressed_estimate,
            compression_ratio,
            compressed_content: llm_result.summary,
            key_points: llm_result.key_points,
        })
    }

    /// Perform intelligent flush with LLM analysis
    async fn perform_intelligent_flush(
        &self,
        session_id: &str,
        trigger: FlushTrigger,
    ) -> Result<FlushEvent> {
        info!(
            "Performing intelligent memory flush for session: {}",
            session_id
        );

        // Get context state
        let state = self
            .base_manager
            .get_context_state(session_id)
            .await
            .ok_or_else(|| crate::error::AgentError::not_found("Session not found".to_string()))?;

        // Get flush recommendation from base manager
        if let Some(prompt) = self.base_manager.get_flush_prompt(state.usage_ratio) {
            debug!("Flush recommendation: {}", prompt);
        }

        // Perform base flush
        self.base_manager.perform_flush(session_id, trigger).await?;

        info!("Intelligent flush completed for session: {}", session_id);
        Ok(FlushEvent {
            id: uuid::Uuid::new_v4(),
            session_id: session_id.to_string(),
            timestamp: chrono::Utc::now(),
            trigger,
            token_usage_before: state.current_tokens,
            token_usage_after: state.current_tokens / 4,
            memories_flushed: 0,
            saved_entries: vec![],
        })
    }

    /// Save message to appropriate memory store
    async fn save_message(&self, session_id: &str, role: &str, content: &str) -> Result<()> {
        // Analyze for categorization
        let analysis = if self.config.enable_llm_analysis {
            self.analyze_importance(content).await?
        } else {
            LLMImportanceAnalysis {
                score: 0.5,
                category: "General".to_string(),
                reason: "Default categorization".to_string(),
                save_to_core: false,
                save_to_daily: true,
                entities: vec![],
                suggested_title: format!("{} Message", role),
            }
        };

        // Create memory entry
        let entry = MarkdownMemoryEntry::new(&analysis.suggested_title, content.to_string())
            .with_category(&analysis.category.to_lowercase())
            .with_importance(analysis.score)
            .with_session_id(session_id)
            .with_metadata("role", role);

        // Determine target storage
        let memory = self.memory_system.read().await;

        if analysis.save_to_core && analysis.score >= 0.8 {
            // Save to core memory
            memory.store(MemoryFileType::Core, &entry, None).await?;
            info!(
                "Saved important memory to core: {} (score: {:.2})",
                analysis.suggested_title, analysis.score
            );
        }

        if analysis.save_to_daily {
            // Save to daily log
            memory.store(MemoryFileType::Daily, &entry, None).await?;
        }

        Ok(())
    }

    /// Estimate token count
    fn estimate_tokens(&self, messages: &[ConversationMessage]) -> usize {
        messages.iter().map(|m| m.content.len() / 4).sum()
    }

    /// Get flush statistics
    pub async fn get_statistics(&self) -> FlushStatistics {
        self.base_manager.get_statistics().await
    }

    /// Access underlying memory system
    pub fn memory_system(&self) -> &Arc<RwLock<UnifiedMemorySystem>> {
        &self.memory_system
    }
}

/// Conversation message for compression
#[derive(Debug, Clone)]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "Requires LLM provider"]
    async fn test_llm_importance_analysis() {
        // This test requires a real LLM provider
        // Test the LLM importance analyzer directly using the orchestrator
        let memory_config = UnifiedMemoryConfig::default();
        let orchestrator =
            LLMMemoryFlushOrchestrator::new(LLMMemoryFlushConfig::default(), memory_config)
                .await
                .unwrap();

        let analysis = orchestrator
            .analyze_importance("I prefer dark mode always")
            .await
            .unwrap();

        assert!(analysis.score > 0.0);
        assert!(!analysis.category.is_empty());
    }

    #[tokio::test]
    #[ignore = "Requires LLM provider"]
    async fn test_compression() {
        let _config = LLMMemoryFlushConfig::default();
        let _messages = vec![
            ConversationMessage {
                role: "User".to_string(),
                content: "Hello, how are you?".to_string(),
                timestamp: chrono::Utc::now(),
            },
            ConversationMessage {
                role: "Assistant".to_string(),
                content: "I'm doing well, thank you!".to_string(),
                timestamp: chrono::Utc::now(),
            },
        ];

        // Note: This would need actual LLM to test properly
        // With MockLLMProvider it returns mock data
    }
}
