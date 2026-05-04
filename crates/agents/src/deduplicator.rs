//! Message Deduplicator
//!
//! Prevents duplicate message processing due to WebSocket reconnection
//! or network jitter. Based on beebot's implementation.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Message processing status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageStatus {
    /// Message is being processed
    Processing,
    /// Message processing completed successfully
    Completed,
    /// Message processing failed
    Failed(String),
}

/// Message record for deduplication
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct MessageRecord {
    /// Message unique identifier
    message_id: String,
    /// Processing status
    status: MessageStatus,
    /// Timestamp when record was created
    created_at: DateTime<Utc>,
    /// Timestamp when processing completed (if applicable)
    completed_at: Option<DateTime<Utc>>,
}

/// Message key for deduplication
/// Combines platform type and message ID to avoid conflicts
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct MessageKey {
    pub platform: String,
    pub message_id: String,
}

impl MessageKey {
    /// Create a new message key
    pub fn new(platform: impl Into<String>, message_id: impl Into<String>) -> Self {
        Self {
            platform: platform.into(),
            message_id: message_id.into(),
        }
    }

    /// Convert to string representation
    pub fn to_string(&self) -> String {
        format!("{}:{}", self.platform, self.message_id)
    }
}

/// Message deduplicator
///
/// Prevents duplicate processing of messages by tracking message IDs
/// with a time-to-live (TTL) mechanism.
///
/// # Platform Isolation
/// Messages from different platforms are isolated using composite keys
/// (platform:message_id) to avoid ID conflicts.
pub struct MessageDeduplicator {
    /// In-memory message cache (key format: "platform:message_id")
    messages: Arc<RwLock<HashMap<String, MessageRecord>>>,
    /// Maximum number of messages to track
    max_size: usize,
    /// Time-to-live for message records (seconds)
    ttl_seconds: i64,
}

impl MessageDeduplicator {
    /// Create a new deduplicator
    ///
    /// # Arguments
    /// * `max_size` - Maximum number of messages to track in memory
    /// * `ttl_seconds` - Time-to-live for message records
    ///
    /// # Example
    /// ```ignore
    /// use beebotos_agents::deduplicator::MessageDeduplicator;
    /// let dedup = MessageDeduplicator::new(50000, 3600);
    /// ```
    pub fn new(max_size: usize, ttl_seconds: i64) -> Self {
        let messages = Arc::new(RwLock::new(HashMap::with_capacity(max_size)));

        // Start cleanup task
        let messages_clone = messages.clone();
        let ttl = ttl_seconds;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                Self::cleanup_expired(&messages_clone, ttl).await;
            }
        });

        Self {
            messages,
            max_size,
            ttl_seconds,
        }
    }

    /// Create with default settings
    ///
    /// Defaults: max_size=50000, ttl_seconds=3600 (1 hour)
    pub fn default() -> Self {
        Self::new(50000, 3600)
    }

    /// Check if a message should be processed (legacy method)
    ///
    /// Returns `true` if the message should be processed (not a duplicate),
    /// `false` if it's a duplicate or already being processed.
    ///
    /// # Arguments
    /// * `message_id` - Unique message identifier
    ///
    /// # Note
    /// This method uses a generic "default" platform. For multi-platform
    /// scenarios, use `should_process_key` instead.
    pub async fn should_process(&self, message_id: &str) -> bool {
        self.should_process_key("default", message_id).await
    }

    /// Check if a message should be processed with platform isolation
    ///
    /// Returns `true` if the message should be processed (not a duplicate),
    /// `false` if it's a duplicate or already being processed.
    ///
    /// # Arguments
    /// * `platform` - Platform type (e.g., "lark", "dingtalk", "telegram")
    /// * `message_id` - Unique message identifier
    ///
    /// # Example
    /// ```ignore
    /// # use beebotos_agents::deduplicator::MessageDeduplicator;
    /// # tokio::runtime::Runtime::new().unwrap().block_on(async {
    /// let dedup = MessageDeduplicator::new(50000, 3600);
    /// let should = dedup.should_process_key("lark", "om_12345").await;
    /// # });
    /// ```
    pub async fn should_process_key(&self, platform: &str, message_id: &str) -> bool {
        let key = format!("{}:{}", platform, message_id);
        let mut messages = self.messages.write().await;

        // Check if message exists
        if let Some(record) = messages.get(&key) {
            // Check if record has expired
            if Utc::now()
                .signed_duration_since(record.created_at)
                .num_seconds()
                > self.ttl_seconds
            {
                // Record expired, remove it and allow reprocessing
                messages.remove(&key);
                debug!("Message record expired: {}", key);
                return true;
            }

            // Message exists and hasn't expired
            match &record.status {
                MessageStatus::Processing => {
                    warn!("Message {} is already being processed", message_id);
                    false
                }
                MessageStatus::Completed => {
                    debug!("Message {} already processed", message_id);
                    false
                }
                MessageStatus::Failed(_) => {
                    // Failed messages can be retried - remove old record and create new
                    debug!("Retrying failed message: {}", key);
                    messages.remove(&key);
                    true
                }
            }
        } else {
            // New message, record it
            let record = MessageRecord {
                message_id: key.clone(),
                status: MessageStatus::Processing,
                created_at: Utc::now(),
                completed_at: None,
            };

            // Check if we need to evict old records
            if messages.len() >= self.max_size {
                // Remove oldest records (simple eviction)
                let keys_to_remove: Vec<String> = messages
                    .iter()
                    .take(messages.len() - self.max_size + 1)
                    .map(|(k, _)| k.clone())
                    .collect();

                for key in keys_to_remove {
                    messages.remove(&key);
                }
            }

            messages.insert(key, record);
            debug!("New message recorded: {}", message_id);
            true
        }
    }

    /// Mark a message as completed (legacy method)
    pub async fn mark_completed(&self, message_id: &str) {
        self.mark_completed_key("default", message_id).await;
    }

    /// Mark a message as completed with platform isolation
    ///
    /// # Arguments
    /// * `platform` - Platform type
    /// * `message_id` - Unique message identifier
    pub async fn mark_completed_key(&self, platform: &str, message_id: &str) {
        let key = format!("{}:{}", platform, message_id);
        let mut messages = self.messages.write().await;

        if let Some(record) = messages.get_mut(&key) {
            record.status = MessageStatus::Completed;
            record.completed_at = Some(Utc::now());
            debug!("Message marked as completed: {}:{}", platform, message_id);
        }
    }

    /// Mark a message as failed (legacy method)
    pub async fn mark_failed(&self, message_id: &str, error: &str) {
        self.mark_failed_key("default", message_id, error).await;
    }

    /// Mark a message as failed with platform isolation
    ///
    /// # Arguments
    /// * `platform` - Platform type
    /// * `message_id` - Unique message identifier
    /// * `error` - Error message
    pub async fn mark_failed_key(&self, platform: &str, message_id: &str, error: &str) {
        let key = format!("{}:{}", platform, message_id);
        let mut messages = self.messages.write().await;

        if let Some(record) = messages.get_mut(&key) {
            record.status = MessageStatus::Failed(error.to_string());
            record.completed_at = Some(Utc::now());
            debug!(
                "Message marked as failed: {}:{} - {}",
                platform, message_id, error
            );
        }
    }

    /// Get current cache size
    pub async fn cache_size(&self) -> usize {
        let messages = self.messages.read().await;
        messages.len()
    }

    /// Cleanup expired records
    async fn cleanup_expired(
        messages: &Arc<RwLock<HashMap<String, MessageRecord>>>,
        ttl_seconds: i64,
    ) {
        let mut messages = messages.write().await;
        let now = Utc::now();

        let expired_keys: Vec<String> = messages
            .iter()
            .filter(|(_, record)| {
                now.signed_duration_since(record.created_at).num_seconds() > ttl_seconds
            })
            .map(|(k, _)| k.clone())
            .collect();

        for key in expired_keys {
            messages.remove(&key);
        }

        if !messages.is_empty() {
            debug!("Cleaned up {} expired message records", messages.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_should_process_new_message() {
        let dedup = MessageDeduplicator::new(100, 3600);

        assert!(dedup.should_process("msg_1").await);
        assert!(!dedup.should_process("msg_1").await); // Duplicate
    }

    #[tokio::test]
    async fn test_mark_completed() {
        let dedup = MessageDeduplicator::new(100, 3600);

        assert!(dedup.should_process("msg_1").await);
        dedup.mark_completed("msg_1").await;

        // Completed message should not be reprocessed
        assert!(!dedup.should_process("msg_1").await);
    }

    #[tokio::test]
    async fn test_failed_message_can_retry() {
        let dedup = MessageDeduplicator::new(100, 3600);

        assert!(dedup.should_process("msg_1").await);
        dedup.mark_failed("msg_1", "Network error").await;

        // Failed message can be retried
        assert!(dedup.should_process("msg_1").await);
    }

    #[tokio::test]
    async fn test_cache_size() {
        let dedup = MessageDeduplicator::new(100, 3600);

        dedup.should_process("msg_1").await;
        dedup.should_process("msg_2").await;

        assert_eq!(dedup.cache_size().await, 2);
    }
}
