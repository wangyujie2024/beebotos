//! Message persistence layer for replay and recovery
//!
//! This module provides message persistence capabilities, allowing messages
//! to be stored and replayed. It supports both in-memory and file-based
//! storage.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use crate::error::{MessageBusError, Result};
use crate::Message;

/// Persisted message record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedMessage {
    /// Sequence number (monotonically increasing)
    pub sequence: u64,
    /// Timestamp when the message was persisted
    pub persisted_at: DateTime<Utc>,
    /// The message itself
    pub message: Message,
    /// Topic the message was published to
    pub topic: String,
}

/// Persistence configuration
#[derive(Debug, Clone)]
pub struct PersistenceConfig {
    /// Maximum number of messages to keep in memory
    pub max_in_memory_messages: usize,
    /// Enable file-based persistence
    pub enable_file_persistence: bool,
    /// Directory for file persistence
    pub persistence_dir: String,
    /// Maximum file size in MB
    pub max_file_size_mb: u32,
    /// Retention period in hours
    pub retention_hours: u32,
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self {
            max_in_memory_messages: 100_000,
            enable_file_persistence: false,
            persistence_dir: "./message_persistence".to_string(),
            max_file_size_mb: 100,
            retention_hours: 24,
        }
    }
}

/// Message persistence trait
#[async_trait]
pub trait MessagePersistence: Send + Sync {
    /// Persist a message
    async fn persist(&self, topic: &str, message: &Message) -> Result<()>;

    /// Get messages for a topic since a sequence number
    async fn get_messages_since(&self, topic: &str, sequence: u64)
        -> Result<Vec<PersistedMessage>>;

    /// Get messages for a topic within a time range
    async fn get_messages_in_range(
        &self,
        topic: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<PersistedMessage>>;

    /// Get the latest sequence number for a topic
    async fn get_latest_sequence(&self, topic: &str) -> Result<u64>;

    /// Create a snapshot of current state
    async fn snapshot(&self) -> Result<PersistenceSnapshot>;

    /// Restore from snapshot
    async fn restore(&self, snapshot: PersistenceSnapshot) -> Result<()>;

    /// Clean up old messages
    async fn cleanup(&self, max_age: chrono::Duration) -> Result<u64>;

    /// Replay messages to a callback
    async fn replay<F>(&self, topic: &str, filter: ReplayFilter, callback: F) -> Result<u64>
    where
        F: FnMut(&PersistedMessage) -> Result<()> + Send;
}

/// Replay filter criteria
#[derive(Debug, Clone, Default)]
pub struct ReplayFilter {
    /// Start sequence number (inclusive)
    pub from_sequence: Option<u64>,
    /// End sequence number (inclusive)
    pub to_sequence: Option<u64>,
    /// Start time (inclusive)
    pub from_time: Option<DateTime<Utc>>,
    /// End time (inclusive)
    pub to_time: Option<DateTime<Utc>>,
    /// Maximum number of messages
    pub limit: Option<usize>,
}

impl ReplayFilter {
    /// Create a filter for all messages
    pub fn all() -> Self {
        Self::default()
    }

    /// Filter from sequence
    pub fn from_sequence(mut self, seq: u64) -> Self {
        self.from_sequence = Some(seq);
        self
    }

    /// Filter to sequence
    pub fn to_sequence(mut self, seq: u64) -> Self {
        self.to_sequence = Some(seq);
        self
    }

    /// Filter by time range
    pub fn time_range(mut self, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        self.from_time = Some(start);
        self.to_time = Some(end);
        self
    }

    /// Limit number of messages
    pub fn limit(mut self, count: usize) -> Self {
        self.limit = Some(count);
        self
    }

    /// Check if a message matches the filter
    fn matches(&self, msg: &PersistedMessage) -> bool {
        if let Some(from_seq) = self.from_sequence {
            if msg.sequence < from_seq {
                return false;
            }
        }

        if let Some(to_seq) = self.to_sequence {
            if msg.sequence > to_seq {
                return false;
            }
        }

        if let Some(from_time) = self.from_time {
            if msg.persisted_at < from_time {
                return false;
            }
        }

        if let Some(to_time) = self.to_time {
            if msg.persisted_at > to_time {
                return false;
            }
        }

        true
    }
}

/// In-memory message persistence
pub struct InMemoryPersistence {
    config: PersistenceConfig,
    /// Topic -> Messages mapping
    messages: Arc<RwLock<HashMap<String, VecDeque<PersistedMessage>>>>,
    /// Sequence counter per topic
    sequences: Arc<RwLock<HashMap<String, AtomicU64>>>,
    /// Total messages stored
    total_messages: AtomicU64,
}

impl InMemoryPersistence {
    /// Create new in-memory persistence
    pub fn new(config: PersistenceConfig) -> Self {
        Self {
            config,
            messages: Arc::new(RwLock::new(HashMap::new())),
            sequences: Arc::new(RwLock::new(HashMap::new())),
            total_messages: AtomicU64::new(0),
        }
    }

    /// Get or create sequence counter for a topic
    async fn get_sequence(&self, topic: &str) -> u64 {
        let sequences = self.sequences.read().await;
        if let Some(counter) = sequences.get(topic) {
            counter.fetch_add(1, Ordering::SeqCst) + 1
        } else {
            drop(sequences);
            let mut sequences = self.sequences.write().await;
            let counter = sequences
                .entry(topic.to_string())
                .or_insert_with(|| AtomicU64::new(0));
            counter.fetch_add(1, Ordering::SeqCst) + 1
        }
    }

    /// Enforce retention policy
    async fn enforce_retention(&self) {
        let mut messages = self.messages.write().await;
        let max = self.config.max_in_memory_messages;

        for (_, queue) in messages.iter_mut() {
            while queue.len() > max {
                if queue.pop_front().is_some() {
                    self.total_messages.fetch_sub(1, Ordering::Relaxed);
                }
            }
        }
    }
}

#[async_trait]
impl MessagePersistence for InMemoryPersistence {
    async fn persist(&self, topic: &str, message: &Message) -> Result<()> {
        let sequence = self.get_sequence(topic).await;

        let persisted = PersistedMessage {
            sequence,
            persisted_at: Utc::now(),
            message: message.clone(),
            topic: topic.to_string(),
        };

        let mut messages = self.messages.write().await;
        let queue = messages
            .entry(topic.to_string())
            .or_insert_with(VecDeque::new);
        queue.push_back(persisted);

        self.total_messages.fetch_add(1, Ordering::Relaxed);

        // Enforce retention in background
        if self.total_messages.load(Ordering::Relaxed) as usize > self.config.max_in_memory_messages
        {
            drop(messages);
            self.enforce_retention().await;
        }

        debug!("Persisted message {} to topic '{}'", sequence, topic);
        Ok(())
    }

    async fn get_messages_since(
        &self,
        topic: &str,
        sequence: u64,
    ) -> Result<Vec<PersistedMessage>> {
        let messages = self.messages.read().await;

        if let Some(queue) = messages.get(topic) {
            let result: Vec<_> = queue
                .iter()
                .filter(|m| m.sequence >= sequence)
                .cloned()
                .collect();
            Ok(result)
        } else {
            Ok(Vec::new())
        }
    }

    async fn get_messages_in_range(
        &self,
        topic: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<PersistedMessage>> {
        let messages = self.messages.read().await;

        if let Some(queue) = messages.get(topic) {
            let result: Vec<_> = queue
                .iter()
                .filter(|m| m.persisted_at >= start && m.persisted_at <= end)
                .cloned()
                .collect();
            Ok(result)
        } else {
            Ok(Vec::new())
        }
    }

    async fn get_latest_sequence(&self, topic: &str) -> Result<u64> {
        let sequences = self.sequences.read().await;
        Ok(sequences
            .get(topic)
            .map(|s| s.load(Ordering::Relaxed))
            .unwrap_or(0))
    }

    async fn snapshot(&self) -> Result<PersistenceSnapshot> {
        let messages = self.messages.read().await;
        let snapshot_data: HashMap<String, Vec<PersistedMessage>> = messages
            .iter()
            .map(|(k, v)| (k.clone(), v.iter().cloned().collect()))
            .collect();

        Ok(PersistenceSnapshot {
            created_at: Utc::now(),
            messages: snapshot_data,
        })
    }

    async fn restore(&self, snapshot: PersistenceSnapshot) -> Result<()> {
        let mut messages = self.messages.write().await;
        let mut sequences = self.sequences.write().await;
        messages.clear();
        sequences.clear();

        let mut total = 0;
        for (topic, msgs) in snapshot.messages {
            let queue: VecDeque<_> = msgs.iter().cloned().collect();
            // Restore sequence number from the last message
            if let Some(last_msg) = queue.back() {
                sequences.insert(topic.clone(), AtomicU64::new(last_msg.sequence));
            }
            total += queue.len();
            messages.insert(topic, queue);
        }

        self.total_messages.store(total as u64, Ordering::Relaxed);
        info!("Restored {} messages from snapshot", total);

        Ok(())
    }

    async fn cleanup(&self, max_age: chrono::Duration) -> Result<u64> {
        let cutoff = Utc::now() - max_age;
        let mut messages = self.messages.write().await;
        let mut removed = 0;

        for (_, queue) in messages.iter_mut() {
            let initial_len = queue.len();
            queue.retain(|m| m.persisted_at > cutoff);
            removed += initial_len - queue.len();
        }

        self.total_messages
            .fetch_sub(removed as u64, Ordering::Relaxed);
        info!("Cleaned up {} old messages", removed);

        Ok(removed as u64)
    }

    async fn replay<F>(&self, topic: &str, filter: ReplayFilter, mut callback: F) -> Result<u64>
    where
        F: FnMut(&PersistedMessage) -> Result<()> + Send,
    {
        let messages = self.messages.read().await;
        let mut count = 0;

        if let Some(queue) = messages.get(topic) {
            for msg in queue.iter() {
                if !filter.matches(msg) {
                    continue;
                }

                if let Some(limit) = filter.limit {
                    if count >= limit {
                        break;
                    }
                }

                callback(msg)?;
                count += 1;
            }
        }

        Ok(count as u64)
    }
}

/// File-based persistence
pub struct FilePersistence {
    config: PersistenceConfig,
    inner: InMemoryPersistence,
}

impl FilePersistence {
    /// Create new file-based persistence
    pub async fn new(config: PersistenceConfig) -> Result<Self> {
        // Create persistence directory
        if config.enable_file_persistence {
            fs::create_dir_all(&config.persistence_dir)
                .await
                .map_err(|e| {
                    MessageBusError::Internal(format!("Failed to create persistence dir: {}", e))
                })?;
        }

        let inner = InMemoryPersistence::new(config.clone());

        // Load existing data
        let snapshot = Self::load_from_disk(&config).await?;
        inner.restore(snapshot).await?;

        Ok(Self { config, inner })
    }

    /// Load snapshot from disk
    async fn load_from_disk(config: &PersistenceConfig) -> Result<PersistenceSnapshot> {
        if !config.enable_file_persistence {
            return Ok(PersistenceSnapshot::default());
        }

        let snapshot_path = format!("{}/snapshot.json", config.persistence_dir);

        match fs::read_to_string(&snapshot_path).await {
            Ok(content) => {
                let snapshot: PersistenceSnapshot =
                    serde_json::from_str(&content).map_err(|e| {
                        MessageBusError::Deserialization(format!("Failed to parse snapshot: {}", e))
                    })?;
                info!("Loaded snapshot with {} topics", snapshot.messages.len());
                Ok(snapshot)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                info!("No existing snapshot found, starting fresh");
                Ok(PersistenceSnapshot::default())
            }
            Err(e) => Err(MessageBusError::Internal(format!(
                "Failed to read snapshot: {}",
                e
            ))),
        }
    }

    /// Save snapshot to disk
    async fn save_to_disk(&self) -> Result<()> {
        if !self.config.enable_file_persistence {
            return Ok(());
        }

        let snapshot = self.inner.snapshot().await?;
        let content = serde_json::to_string(&snapshot).map_err(|e| {
            MessageBusError::Serialization(format!("Failed to serialize snapshot: {}", e))
        })?;

        let snapshot_path = format!("{}/snapshot.json", self.config.persistence_dir);
        fs::write(&snapshot_path, content)
            .await
            .map_err(|e| MessageBusError::Internal(format!("Failed to write snapshot: {}", e)))?;

        debug!("Saved snapshot to {}", snapshot_path);
        Ok(())
    }

    /// Start background persistence task
    pub fn start_persistence_task(&self) -> tokio::task::JoinHandle<()> {
        let this = Self {
            config: self.config.clone(),
            inner: InMemoryPersistence::new(self.config.clone()),
        };

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));

            loop {
                interval.tick().await;

                if let Err(e) = this.save_to_disk().await {
                    error!("Failed to persist to disk: {}", e);
                }
            }
        })
    }
}

#[async_trait]
impl MessagePersistence for FilePersistence {
    async fn persist(&self, topic: &str, message: &Message) -> Result<()> {
        self.inner.persist(topic, message).await
    }

    async fn get_messages_since(
        &self,
        topic: &str,
        sequence: u64,
    ) -> Result<Vec<PersistedMessage>> {
        self.inner.get_messages_since(topic, sequence).await
    }

    async fn get_messages_in_range(
        &self,
        topic: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<PersistedMessage>> {
        self.inner.get_messages_in_range(topic, start, end).await
    }

    async fn get_latest_sequence(&self, topic: &str) -> Result<u64> {
        self.inner.get_latest_sequence(topic).await
    }

    async fn snapshot(&self) -> Result<PersistenceSnapshot> {
        self.inner.snapshot().await
    }

    async fn restore(&self, snapshot: PersistenceSnapshot) -> Result<()> {
        self.inner.restore(snapshot).await
    }

    async fn cleanup(&self, max_age: chrono::Duration) -> Result<u64> {
        let removed = self.inner.cleanup(max_age).await?;
        self.save_to_disk().await?;
        Ok(removed)
    }

    async fn replay<F>(&self, topic: &str, filter: ReplayFilter, callback: F) -> Result<u64>
    where
        F: FnMut(&PersistedMessage) -> Result<()> + Send,
    {
        self.inner.replay(topic, filter, callback).await
    }
}

/// Persistence snapshot for backup/restore
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistenceSnapshot {
    pub created_at: DateTime<Utc>,
    pub messages: HashMap<String, Vec<PersistedMessage>>,
}

/// Persistent message bus wrapper
pub struct PersistentMessageBus<B, P> {
    bus: B,
    persistence: P,
}

impl<B: crate::MessageBus, P: MessagePersistence> PersistentMessageBus<B, P> {
    /// Create a new persistent message bus
    pub fn new(bus: B, persistence: P) -> Self {
        Self { bus, persistence }
    }

    /// Publish and persist a message
    pub async fn publish_persistent(&self, topic: &str, message: &Message) -> Result<()> {
        // Persist first
        self.persistence.persist(topic, message).await?;

        // Then publish
        self.bus.publish(topic, message.clone()).await
    }

    /// Replay messages to a subscriber
    pub async fn replay_and_subscribe(
        &self,
        topic: &str,
        filter: ReplayFilter,
    ) -> Result<(crate::SubscriptionId, crate::MessageStream)> {
        // Get current sequence
        let _latest = self.persistence.get_latest_sequence(topic).await?;

        // Replay historical messages
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

        self.persistence
            .replay(topic, filter, |msg| {
                let _ = tx.send(msg.message.clone());
                Ok(())
            })
            .await?;

        // Subscribe to new messages
        let (sub_id, stream) = self.bus.subscribe(topic).await?;

        // Combine streams (this is a simplified version)
        // In production, you'd use a proper stream merging mechanism

        Ok((sub_id, stream))
    }

    /// Get inner bus
    pub fn inner(&self) -> &B {
        &self.bus
    }

    /// Get persistence
    pub fn persistence(&self) -> &P {
        &self.persistence
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_persistence() {
        let config = PersistenceConfig::default();
        let persistence = InMemoryPersistence::new(config);

        // Persist some messages
        for i in 0..5 {
            let msg = Message::new("test/topic", format!("data{}", i).into_bytes());
            persistence.persist("test/topic", &msg).await.unwrap();
        }

        // Get messages since sequence 3
        let messages = persistence
            .get_messages_since("test/topic", 3)
            .await
            .unwrap();
        assert_eq!(messages.len(), 3); // sequences 3, 4, 5

        // Get latest sequence
        let latest = persistence.get_latest_sequence("test/topic").await.unwrap();
        assert_eq!(latest, 5);
    }

    #[tokio::test]
    async fn test_replay_filter() {
        let config = PersistenceConfig::default();
        let persistence = InMemoryPersistence::new(config);

        // Persist messages
        for i in 0..10 {
            let msg = Message::new("test/topic", format!("data{}", i).into_bytes());
            persistence.persist("test/topic", &msg).await.unwrap();
        }

        // Replay with filter
        let mut received = Vec::new();
        let filter = ReplayFilter::all().from_sequence(5).limit(3);

        persistence
            .replay("test/topic", filter, |msg| {
                received.push(msg.sequence);
                Ok(())
            })
            .await
            .unwrap();

        assert_eq!(received, vec![5, 6, 7]);
    }

    #[tokio::test]
    async fn test_snapshot_restore() {
        let config = PersistenceConfig::default();
        let persistence1 = InMemoryPersistence::new(config.clone());

        // Add data
        for i in 0..5 {
            let msg = Message::new("test/topic", format!("data{}", i).into_bytes());
            persistence1.persist("test/topic", &msg).await.unwrap();
        }

        // Create snapshot
        let snapshot = persistence1.snapshot().await.unwrap();

        // Restore to new persistence
        let persistence2 = InMemoryPersistence::new(config);
        persistence2.restore(snapshot).await.unwrap();

        // Verify
        let latest = persistence2
            .get_latest_sequence("test/topic")
            .await
            .unwrap();
        assert_eq!(latest, 5);
    }

    #[tokio::test]
    async fn test_retention_policy() {
        let config = PersistenceConfig {
            max_in_memory_messages: 3,
            ..Default::default()
        };
        let persistence = InMemoryPersistence::new(config);

        // Add more messages than limit
        for i in 0..5 {
            let msg = Message::new("test/topic", format!("data{}", i).into_bytes());
            persistence.persist("test/topic", &msg).await.unwrap();
        }

        // Enforce retention
        persistence.enforce_retention().await;

        // Should only have 3 messages (latest)
        let messages = persistence
            .get_messages_since("test/topic", 0)
            .await
            .unwrap();
        assert_eq!(messages.len(), 3);
    }
}
