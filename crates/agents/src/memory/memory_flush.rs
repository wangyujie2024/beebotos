//! Memory Flush Mechanism
//!
//! Prevents "silent forgetting" in long conversations by automatically
//! triggering memory persistence when the context window approaches its limit.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Memory Flush System                          │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                                                                  │
//! │   Context Window Monitor          Flush Decision Engine          │
//! │   ┌──────────────────┐           ┌──────────────────┐          │
//! │   │ Track token      │──────────▶│ Analyze content  │          │
//! │   │ usage & growth   │           │ importance       │          │
//! │   └──────────────────┘           └────────┬─────────┘          │
//! │            │                              │                    │
//! │            │ Token threshold              │ Trigger            │
//! │            │ reached (80%)                │ flush              │
//! │            ▼                              ▼                    │
//! │   ┌──────────────────┐           ┌──────────────────┐          │
//! │   │ Alert AI to      │           │ Selective        │          │
//! │   │ save important   │◀──────────│ persistence      │          │
//! │   │ memories         │           │ (important only) │          │
//! │   └──────────────────┘           └──────────────────┘          │
//! │            │                                                     │
//! │            ▼                                                     │
//! │   ┌──────────────────┐                                           │
//! │   │ Write to daily   │  → memory/YYYY-MM-DD.md                    │
//! │   │ log & core       │  → MEMORY.md (if critical)                 │
//! │   │ memory files     │                                           │
//! │   └──────────────────┘                                           │
//! │                                                                  │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Flush Triggers
//! - Token threshold: 80% of context window
//! - Time-based: Every 30 minutes of active conversation
//! - Manual: User explicit trigger
//! - Content-based: Important entity detection

use std::collections::HashMap;
use std::sync::Arc;
#[allow(unused_imports)]
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration};
use tracing::{info, warn};
use uuid::Uuid;

use crate::error::Result;

/// Default token threshold percentage (80%)
pub const DEFAULT_TOKEN_THRESHOLD: f32 = 0.8;
/// Default time-based flush interval (30 minutes)
pub const DEFAULT_FLUSH_INTERVAL_SECS: u64 = 1800;
/// Default minimum content importance score
pub const DEFAULT_IMPORTANCE_THRESHOLD: f32 = 0.6;
/// Maximum tokens in context window (default: 4K for most models)
pub const DEFAULT_MAX_CONTEXT_TOKENS: usize = 4096;

/// Flush trigger types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlushTrigger {
    /// Token threshold reached
    TokenThreshold,
    /// Time-based periodic flush
    TimeBased,
    /// Manual user trigger
    Manual,
    /// Content importance detection
    ContentImportant,
    /// Context window full
    ContextFull,
}

impl std::fmt::Display for FlushTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlushTrigger::TokenThreshold => write!(f, "token_threshold"),
            FlushTrigger::TimeBased => write!(f, "time_based"),
            FlushTrigger::Manual => write!(f, "manual"),
            FlushTrigger::ContentImportant => write!(f, "content_important"),
            FlushTrigger::ContextFull => write!(f, "context_full"),
        }
    }
}

/// Memory flush configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFlushConfig {
    /// Token usage threshold (0.0 - 1.0)
    pub token_threshold: f32,
    /// Time-based flush interval in seconds
    pub flush_interval_secs: u64,
    /// Minimum importance score for selective flush
    pub importance_threshold: f32,
    /// Maximum context window tokens
    pub max_context_tokens: usize,
    /// Enable automatic flush
    pub enable_auto_flush: bool,
    /// Enable selective flush (only important content)
    pub enable_selective_flush: bool,
    /// Days to keep in daily logs
    pub daily_log_retention_days: usize,
}

impl Default for MemoryFlushConfig {
    fn default() -> Self {
        Self {
            token_threshold: DEFAULT_TOKEN_THRESHOLD,
            flush_interval_secs: DEFAULT_FLUSH_INTERVAL_SECS,
            importance_threshold: DEFAULT_IMPORTANCE_THRESHOLD,
            max_context_tokens: DEFAULT_MAX_CONTEXT_TOKENS,
            enable_auto_flush: true,
            enable_selective_flush: true,
            daily_log_retention_days: 30,
        }
    }
}

/// Importance analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportanceAnalysis {
    /// Overall importance score (0.0 - 1.0)
    pub score: f32,
    /// Categories detected
    pub categories: Vec<MemoryCategory>,
    /// Key entities extracted
    pub entities: Vec<String>,
    /// Should be saved to core memory (MEMORY.md)
    pub save_to_core: bool,
    /// Should be saved to daily log
    pub save_to_daily: bool,
}

/// Memory category for classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryCategory {
    /// User preferences and settings
    UserPreference,
    /// Important facts about user
    UserFact,
    /// Project configuration
    ProjectConfig,
    /// Task or goal information
    TaskGoal,
    /// Decision made during conversation
    Decision,
    /// Code or technical information
    Technical,
    /// General conversation
    General,
}

impl MemoryCategory {
    /// Get default importance for category
    pub fn default_importance(&self) -> f32 {
        match self {
            MemoryCategory::UserPreference => 0.9,
            MemoryCategory::UserFact => 0.85,
            MemoryCategory::ProjectConfig => 0.85,
            MemoryCategory::TaskGoal => 0.8,
            MemoryCategory::Decision => 0.75,
            MemoryCategory::Technical => 0.7,
            MemoryCategory::General => 0.4,
        }
    }
}

/// Flush event data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlushEvent {
    /// Event ID
    pub id: Uuid,
    /// Session ID
    pub session_id: String,
    /// Trigger type
    pub trigger: FlushTrigger,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Token usage before flush
    pub token_usage_before: usize,
    /// Token usage after flush (estimated)
    pub token_usage_after: usize,
    /// Number of memories flushed
    pub memories_flushed: usize,
    /// Memory entries that were saved
    pub saved_entries: Vec<FlushedMemoryEntry>,
}

/// Flushed memory entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlushedMemoryEntry {
    /// Entry ID
    pub id: Uuid,
    /// Content
    pub content: String,
    /// Category
    pub category: MemoryCategory,
    /// Importance score
    pub importance: f32,
    /// Saved to core memory
    pub saved_to_core: bool,
    /// Saved to daily log
    pub saved_to_daily: bool,
    /// Metadata
    pub metadata: HashMap<String, String>,
}

/// Context window monitor
#[derive(Debug, Clone)]
pub struct ContextWindowState {
    /// Current token count
    pub current_tokens: usize,
    /// Maximum tokens allowed
    pub max_tokens: usize,
    /// Usage ratio (0.0 - 1.0)
    pub usage_ratio: f32,
    /// Last flush timestamp
    pub last_flush: Option<chrono::DateTime<chrono::Utc>>,
    /// Conversation start timestamp
    pub conversation_start: chrono::DateTime<chrono::Utc>,
    /// Number of messages in context
    pub message_count: usize,
}

impl ContextWindowState {
    pub fn new(max_tokens: usize) -> Self {
        let now = chrono::Utc::now();
        Self {
            current_tokens: 0,
            max_tokens,
            usage_ratio: 0.0,
            last_flush: None,
            conversation_start: now,
            message_count: 0,
        }
    }

    /// Update token count and usage ratio
    pub fn update_tokens(&mut self, tokens: usize) {
        self.current_tokens = tokens;
        self.usage_ratio = tokens as f32 / self.max_tokens as f32;
        self.message_count += 1;
    }

    /// Check if token threshold is reached
    pub fn is_threshold_reached(&self, threshold: f32) -> bool {
        self.usage_ratio >= threshold
    }

    /// Check if context is near full
    pub fn is_near_full(&self) -> bool {
        self.usage_ratio >= 0.95
    }

    /// Estimate tokens from text (rough approximation)
    pub fn estimate_tokens(text: &str) -> usize {
        // Rough estimate: 1 token ≈ 4 characters for English
        text.len() / 4
    }
}

/// Memory flush manager
pub struct MemoryFlushManager {
    config: MemoryFlushConfig,
    /// Context window states per session
    context_states: Arc<RwLock<HashMap<String, ContextWindowState>>>,
    /// Flush event sender
    flush_sender: mpsc::Sender<FlushEvent>,
    /// Flush history
    flush_history: Arc<RwLock<Vec<FlushEvent>>>,
}

impl MemoryFlushManager {
    /// Create new memory flush manager
    pub fn new(config: MemoryFlushConfig) -> (Self, mpsc::Receiver<FlushEvent>) {
        let (flush_sender, flush_receiver) = mpsc::channel(100);

        let manager = Self {
            config,
            context_states: Arc::new(RwLock::new(HashMap::new())),
            flush_sender,
            flush_history: Arc::new(RwLock::new(Vec::new())),
        };

        (manager, flush_receiver)
    }

    /// Create with default configuration
    pub fn default() -> (Self, mpsc::Receiver<FlushEvent>) {
        Self::new(MemoryFlushConfig::default())
    }

    /// Initialize context monitoring for a session
    pub async fn init_session(&self, session_id: &str) {
        let mut states = self.context_states.write().await;
        states.insert(
            session_id.to_string(),
            ContextWindowState::new(self.config.max_context_tokens),
        );
        info!(
            "Initialized memory flush monitoring for session: {}",
            session_id
        );
    }

    /// Remove session monitoring
    pub async fn remove_session(&self, session_id: &str) {
        let mut states = self.context_states.write().await;
        states.remove(session_id);
        info!(
            "Removed memory flush monitoring for session: {}",
            session_id
        );
    }

    /// Update token usage for a session
    pub async fn update_token_usage(
        &self,
        session_id: &str,
        content: &str,
    ) -> Option<FlushTrigger> {
        let estimated_tokens = ContextWindowState::estimate_tokens(content);

        let mut states = self.context_states.write().await;
        let state = states.get_mut(session_id)?;

        state.update_tokens(state.current_tokens + estimated_tokens);

        // Check for flush triggers
        if state.is_near_full() {
            Some(FlushTrigger::ContextFull)
        } else if state.is_threshold_reached(self.config.token_threshold) {
            Some(FlushTrigger::TokenThreshold)
        } else {
            None
        }
    }

    /// Check if flush is needed
    pub async fn check_flush_needed(&self, session_id: &str) -> Option<FlushTrigger> {
        let states = self.context_states.read().await;
        let state = states.get(session_id)?;

        if state.is_near_full() {
            Some(FlushTrigger::ContextFull)
        } else if state.is_threshold_reached(self.config.token_threshold) {
            Some(FlushTrigger::TokenThreshold)
        } else {
            None
        }
    }

    /// Trigger manual flush
    pub async fn trigger_manual_flush(&self, session_id: &str) -> Result<()> {
        self.perform_flush(session_id, FlushTrigger::Manual).await
    }

    /// Analyze content importance
    pub fn analyze_importance(&self, content: &str) -> ImportanceAnalysis {
        let mut score: f32 = 0.0;
        let mut categories = Vec::new();
        let mut entities = Vec::new();
        let lower = content.to_lowercase();

        // Check for user preferences
        if lower.contains("prefer")
            || lower.contains("like")
            || lower.contains("always")
            || lower.contains("never")
            || lower.contains("favorite")
        {
            score += 0.9;
            categories.push(MemoryCategory::UserPreference);
        }

        // Check for important facts
        if lower.contains("i am")
            || lower.contains("my name")
            || lower.contains("i work")
            || lower.contains("my job")
            || lower.contains("i live")
        {
            score += 0.85;
            categories.push(MemoryCategory::UserFact);
        }

        // Check for project config
        if lower.contains("project")
            || lower.contains("config")
            || lower.contains("setup")
            || lower.contains("database")
            || lower.contains("api key")
        {
            score += 0.8;
            categories.push(MemoryCategory::ProjectConfig);
        }

        // Check for tasks/goals
        if lower.contains("task")
            || lower.contains("goal")
            || lower.contains("todo")
            || lower.contains("deadline")
            || lower.contains("plan")
        {
            score += 0.75;
            categories.push(MemoryCategory::TaskGoal);
        }

        // Check for decisions
        if lower.contains("decide")
            || lower.contains("decision")
            || lower.contains("choose")
            || lower.contains("agreed")
            || lower.contains("conclusion")
        {
            score += 0.7;
            categories.push(MemoryCategory::Decision);
        }

        // Check for technical info
        if lower.contains("code")
            || lower.contains("function")
            || lower.contains("script")
            || lower.contains("implementation")
            || lower.contains("algorithm")
        {
            score += 0.65;
            categories.push(MemoryCategory::Technical);
        }

        // Extract simple entities (quoted strings)
        for word in content.split_whitespace() {
            if word.starts_with('\'') || word.starts_with('"') {
                entities.push(word.trim_matches(&['"', '\''][..]).to_string());
            }
        }

        // Normalize score
        score = score.min(1.0_f32);

        // Determine save targets
        let save_to_core = score >= 0.8;
        let save_to_daily = score >= self.config.importance_threshold;

        if categories.is_empty() {
            categories.push(MemoryCategory::General);
        }

        ImportanceAnalysis {
            score,
            categories,
            entities,
            save_to_core,
            save_to_daily,
        }
    }

    /// Perform memory flush
    pub async fn perform_flush(&self, session_id: &str, trigger: FlushTrigger) -> Result<()> {
        info!(
            "Performing memory flush for session: {} (trigger: {})",
            session_id, trigger
        );

        let mut states = self.context_states.write().await;
        let state = states.get_mut(session_id).ok_or_else(|| {
            crate::error::AgentError::not_found(format!("Session not found: {}", session_id))
        })?;

        let token_usage_before = state.current_tokens;

        // In real implementation, this would:
        // 1. Retrieve conversation history
        // 2. Analyze importance of each message
        // 3. Save important ones to appropriate storage
        // 4. Clear or compress context

        // Reset token count (simulating flush)
        state.current_tokens = state.current_tokens / 4; // Keep 25% as summary
        state.last_flush = Some(chrono::Utc::now());
        state.update_tokens(state.current_tokens);

        drop(states);

        // Create flush event
        let event = FlushEvent {
            id: Uuid::new_v4(),
            session_id: session_id.to_string(),
            trigger,
            timestamp: chrono::Utc::now(),
            token_usage_before,
            token_usage_after: token_usage_before / 4,
            memories_flushed: 0, // Would be calculated from actual content
            saved_entries: Vec::new(),
        };

        // Store in history
        {
            let mut history = self.flush_history.write().await;
            history.push(event.clone());
        }

        // Send event
        let _ = self.flush_sender.send(event).await;

        info!("Memory flush completed for session: {}", session_id);
        Ok(())
    }

    /// Get flush recommendation for AI
    pub fn get_flush_prompt(&self, usage_ratio: f32) -> Option<String> {
        if usage_ratio >= 0.95 {
            Some(format!(
                "⚠️ **CRITICAL**: Context window is {}% full!\nImportant information may be lost. \
                 Please immediately:\n1. Identify and save critical user preferences\n2. Save \
                 project configurations\n3. Summarize key decisions made\n4. Store to appropriate \
                 memory files",
                (usage_ratio * 100.0) as u32
            ))
        } else if usage_ratio >= self.config.token_threshold {
            Some(format!(
                "📝 **Memory Flush Recommended**: Context window is {}% full.\nTo prevent silent \
                 forgetting, please:\n1. Review important information from this conversation\n2. \
                 Save user preferences to MEMORY.md\n3. Log today's activities to memory/{}.md",
                (usage_ratio * 100.0) as u32,
                chrono::Local::now().format("%Y-%m-%d")
            ))
        } else {
            None
        }
    }

    /// Start background monitoring task
    pub fn start_monitoring(self: Arc<Self>) {
        if !self.config.enable_auto_flush {
            return;
        }

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(self.config.flush_interval_secs));

            loop {
                interval.tick().await;

                let states = self.context_states.read().await;
                let session_ids: Vec<String> = states.keys().cloned().collect();
                drop(states);

                for session_id in session_ids {
                    // Check time-based flush
                    let should_flush = {
                        let states = self.context_states.read().await;
                        if let Some(state) = states.get(&session_id) {
                            if let Some(last_flush) = state.last_flush {
                                let elapsed = chrono::Utc::now() - last_flush;
                                elapsed.num_seconds() >= self.config.flush_interval_secs as i64
                            } else {
                                // Never flushed, check if conversation has been long
                                let elapsed = chrono::Utc::now() - state.conversation_start;
                                elapsed.num_seconds() >= self.config.flush_interval_secs as i64
                            }
                        } else {
                            false
                        }
                    };

                    if should_flush {
                        if let Err(e) = self
                            .perform_flush(&session_id, FlushTrigger::TimeBased)
                            .await
                        {
                            warn!(
                                "Failed to perform time-based flush for {}: {}",
                                session_id, e
                            );
                        }
                    }
                }
            }
        });
    }

    /// Get flush history
    pub async fn get_flush_history(&self, session_id: Option<&str>) -> Vec<FlushEvent> {
        let history = self.flush_history.read().await;

        match session_id {
            Some(sid) => history
                .iter()
                .filter(|e| e.session_id == sid)
                .cloned()
                .collect(),
            None => history.clone(),
        }
    }

    /// Get context window state
    pub async fn get_context_state(&self, session_id: &str) -> Option<ContextWindowState> {
        let states = self.context_states.read().await;
        states.get(session_id).cloned()
    }

    /// Get flush statistics
    pub async fn get_statistics(&self) -> FlushStatistics {
        let history = self.flush_history.read().await;
        let states = self.context_states.read().await;

        let total_flushes = history.len();
        let token_threshold_flushes = history
            .iter()
            .filter(|e| e.trigger == FlushTrigger::TokenThreshold)
            .count();
        let time_based_flushes = history
            .iter()
            .filter(|e| e.trigger == FlushTrigger::TimeBased)
            .count();
        let manual_flushes = history
            .iter()
            .filter(|e| e.trigger == FlushTrigger::Manual)
            .count();

        let total_tokens_saved: usize = history
            .iter()
            .map(|e| e.token_usage_before - e.token_usage_after)
            .sum();

        FlushStatistics {
            total_flushes,
            token_threshold_flushes,
            time_based_flushes,
            manual_flushes,
            active_sessions: states.len(),
            total_tokens_saved,
            avg_tokens_per_flush: if total_flushes > 0 {
                total_tokens_saved / total_flushes
            } else {
                0
            },
        }
    }
}

/// Flush statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlushStatistics {
    pub total_flushes: usize,
    pub token_threshold_flushes: usize,
    pub time_based_flushes: usize,
    pub manual_flushes: usize,
    pub active_sessions: usize,
    pub total_tokens_saved: usize,
    pub avg_tokens_per_flush: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_flush_config_default() {
        let config = MemoryFlushConfig::default();
        assert_eq!(config.token_threshold, 0.8);
        assert_eq!(config.flush_interval_secs, 1800);
        assert!(config.enable_auto_flush);
    }

    #[test]
    fn test_context_window_state() {
        let mut state = ContextWindowState::new(4096);
        assert_eq!(state.current_tokens, 0);
        assert_eq!(state.usage_ratio, 0.0);

        state.update_tokens(3200);
        assert_eq!(state.current_tokens, 3200);
        assert!((state.usage_ratio - 0.78).abs() < 0.01);

        assert!(state.is_threshold_reached(0.7));
        assert!(!state.is_near_full());
    }

    #[test]
    fn test_flush_trigger_display() {
        assert_eq!(
            format!("{}", FlushTrigger::TokenThreshold),
            "token_threshold"
        );
        assert_eq!(format!("{}", FlushTrigger::Manual), "manual");
    }

    #[tokio::test]
    async fn test_memory_flush_manager() {
        // Use smaller max_context to make threshold easier to reach
        let config = MemoryFlushConfig {
            max_context_tokens: 2000, // Smaller context
            token_threshold: 0.5,     // Lower threshold (50%)
            ..Default::default()
        };
        let (manager, _receiver) = MemoryFlushManager::new(config);

        // Initialize session
        manager.init_session("test-session").await;

        // Update token usage - 4000 chars / 4 = 1000 tokens, which is 50% of 2000
        let trigger = manager
            .update_token_usage("test-session", "a".repeat(4000).as_str())
            .await;
        assert!(trigger.is_some(), "Expected trigger but got None");

        // Check importance analysis
        let analysis = manager.analyze_importance("I prefer dark mode always");
        assert!(analysis.score > 0.8);
        assert!(analysis.save_to_core);

        // Remove session
        manager.remove_session("test-session").await;
    }

    #[test]
    fn test_memory_category_importance() {
        assert!(
            MemoryCategory::UserPreference.default_importance()
                > MemoryCategory::General.default_importance()
        );
        assert!(
            MemoryCategory::ProjectConfig.default_importance()
                > MemoryCategory::Technical.default_importance()
        );
    }

    #[test]
    fn test_get_flush_prompt() {
        let config = MemoryFlushConfig::default();
        let (manager, _) = MemoryFlushManager::new(config);

        let critical = manager.get_flush_prompt(0.96);
        assert!(critical.is_some());
        assert!(critical.unwrap().contains("CRITICAL"));

        let recommended = manager.get_flush_prompt(0.85);
        assert!(recommended.is_some());
        assert!(recommended.unwrap().contains("Recommended"));

        let none = manager.get_flush_prompt(0.5);
        assert!(none.is_none());
    }
}
