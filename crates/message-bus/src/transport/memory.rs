//! In-memory transport implementation
//!
//! This transport is suitable for single-process deployments where
//! all components run in the same application. It provides the lowest
//! latency and highest throughput.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use dashmap::DashMap;
use tokio::sync::mpsc;
use tracing::{debug, trace, warn};

use super::{Transport, TransportConfig, TransportStats};
use crate::error::{MessageBusError, Result};
use crate::{Message, MessageStream, SubscriptionId};

/// In-memory transport implementation
///
/// Uses a DashMap for concurrent subscription management and
/// Tokio mpsc channels for message delivery.
#[derive(Clone)]
pub struct MemoryTransport {
    /// Configuration
    config: TransportConfig,

    /// Topic subscriptions: topic_pattern -> list of (subscription_id, sender)
    subscriptions: Arc<DashMap<String, Vec<(SubscriptionId, mpsc::UnboundedSender<Message>)>>>,

    /// Reverse mapping: subscription_id -> topic_pattern
    subscription_topics: Arc<DashMap<SubscriptionId, String>>,

    /// Statistics
    stats: Arc<MemoryTransportStats>,
}

/// Statistics for memory transport
#[derive(Debug)]
struct MemoryTransportStats {
    messages_published: AtomicU64,
    messages_received: AtomicU64,
    bytes_published: AtomicU64,
    bytes_received: AtomicU64,
    connection_errors: AtomicU64,
}

impl MemoryTransportStats {
    fn new() -> Self {
        Self {
            messages_published: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
            bytes_published: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            connection_errors: AtomicU64::new(0),
        }
    }

    fn record_publish(&self, bytes: usize) {
        self.messages_published.fetch_add(1, Ordering::Relaxed);
        self.bytes_published
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }

    #[allow(dead_code)]
    fn record_receive(&self, bytes: usize) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
        self.bytes_received
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }
}

impl MemoryTransport {
    /// Create a new memory transport with default configuration
    pub fn new() -> Self {
        Self::with_config(TransportConfig::default())
    }

    /// Create a new memory transport with custom configuration
    pub fn with_config(config: TransportConfig) -> Self {
        Self {
            config,
            subscriptions: Arc::new(DashMap::new()),
            subscription_topics: Arc::new(DashMap::new()),
            stats: Arc::new(MemoryTransportStats::new()),
        }
    }

    /// Match a topic against a pattern
    ///
    /// Supports:
    /// - Exact match: "agent/123/task"
    /// - Single-level wildcard (+): "agent/+/task"
    /// - Multi-level wildcard (#): "agent/#"
    fn topic_matches(pattern: &str, topic: &str) -> bool {
        // Handle empty cases
        if pattern.is_empty() || topic.is_empty() {
            return false;
        }

        let pattern_parts: Vec<&str> = pattern.split('/').collect();
        let topic_parts: Vec<&str> = topic.split('/').collect();

        let mut p_idx = 0;
        let mut t_idx = 0;

        while p_idx < pattern_parts.len() && t_idx < topic_parts.len() {
            match pattern_parts[p_idx] {
                // Multi-level wildcard matches everything remaining
                "#" => return true,
                // Single-level wildcard matches any single segment
                "+" => {
                    p_idx += 1;
                    t_idx += 1;
                }
                // Exact match required
                part => {
                    if part != topic_parts[t_idx] {
                        return false;
                    }
                    p_idx += 1;
                    t_idx += 1;
                }
            }
        }

        // Handle trailing # wildcard
        if p_idx < pattern_parts.len() && pattern_parts[p_idx] == "#" {
            return true;
        }

        // Both should be fully consumed
        p_idx == pattern_parts.len() && t_idx == topic_parts.len()
    }

    /// Get current statistics
    pub fn stats(&self) -> TransportStats {
        TransportStats {
            messages_published: self.stats.messages_published.load(Ordering::Relaxed),
            messages_received: self.stats.messages_received.load(Ordering::Relaxed),
            bytes_published: self.stats.bytes_published.load(Ordering::Relaxed),
            bytes_received: self.stats.bytes_received.load(Ordering::Relaxed),
            active_subscriptions: self.subscriptions.len(),
            connection_errors: self.stats.connection_errors.load(Ordering::Relaxed),
            avg_publish_latency_us: 0, // Not tracked in memory transport
        }
    }

    /// Get number of active subscriptions
    pub fn subscription_count(&self) -> usize {
        self.subscriptions.len()
    }

    /// Clear all subscriptions (useful for testing)
    pub fn clear(&self) {
        self.subscriptions.clear();
        self.subscription_topics.clear();
    }
}

impl Default for MemoryTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Transport for MemoryTransport {
    async fn publish(&self, topic: &str, message: Message) -> Result<()> {
        // Check message size
        let message_size = message.payload_size();
        if message_size > self.config.max_message_size {
            return Err(MessageBusError::Transport(format!(
                "Message size {} exceeds maximum {}",
                message_size, self.config.max_message_size
            )));
        }

        trace!(
            topic = %topic,
            message_id = %message.metadata.message_id,
            "Publishing message"
        );

        let mut delivered_count = 0;

        // Find matching subscriptions
        for entry in self.subscriptions.iter() {
            let pattern = entry.key();
            let subs = entry.value();

            if Self::topic_matches(pattern, topic) {
                for (sub_id, tx) in subs {
                    match tx.send(message.clone()) {
                        Ok(_) => {
                            delivered_count += 1;
                            trace!("Delivered to subscription {}", sub_id.0);
                        }
                        Err(_) => {
                            warn!(
                                "Failed to deliver to subscription {} (receiver dropped)",
                                sub_id.0
                            );
                        }
                    }
                }
            }
        }

        // Update stats
        self.stats.record_publish(message_size);

        debug!(
            topic = %topic,
            delivered = delivered_count,
            "Message published"
        );

        Ok(())
    }

    async fn subscribe(&self, topic_pattern: &str) -> Result<(SubscriptionId, MessageStream)> {
        // Validate pattern
        if topic_pattern.is_empty() {
            return Err(MessageBusError::InvalidTopic(
                "Topic pattern cannot be empty".to_string(),
            ));
        }

        let id = SubscriptionId::new();
        let (tx, rx) = mpsc::unbounded_channel();

        // Add subscription
        self.subscriptions
            .entry(topic_pattern.to_string())
            .or_insert_with(Vec::new)
            .push((id, tx));

        // Store reverse mapping
        self.subscription_topics
            .insert(id, topic_pattern.to_string());

        debug!(
            subscription_id = %id.0,
            pattern = %topic_pattern,
            "New subscription created"
        );

        Ok((id, rx))
    }

    async fn unsubscribe(&self, id: SubscriptionId) -> Result<()> {
        // Find the topic pattern for this subscription
        if let Some((_, topic_pattern)) = self.subscription_topics.remove(&id) {
            // Remove from subscriptions
            if let Some(mut entry) = self.subscriptions.get_mut(&topic_pattern) {
                entry.retain(|(sub_id, _)| *sub_id != id);

                // Clean up empty subscription lists
                if entry.is_empty() {
                    drop(entry);
                    self.subscriptions.remove(&topic_pattern);
                }
            }

            debug!(subscription_id = %id.0, "Unsubscribed");
            Ok(())
        } else {
            Err(MessageBusError::TopicNotFound(format!(
                "Subscription {} not found",
                id.0
            )))
        }
    }

    async fn request(&self, topic: &str, request: Message, timeout: Duration) -> Result<Message> {
        // Generate response topic
        let correlation_id = request
            .metadata
            .correlation_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let response_topic = format!("{}/response/{}", topic, correlation_id);

        // Subscribe to response topic
        let (sub_id, mut rx) = self.subscribe(&response_topic).await?;

        // Publish request with correlation ID
        let request = request.with_correlation_id(&correlation_id);
        self.publish(topic, request).await?;

        // Wait for response
        let result = tokio::time::timeout(timeout, rx.recv()).await;

        // Clean up subscription
        let _ = self.unsubscribe(sub_id).await;

        match result {
            Ok(Some(response)) => Ok(response),
            Ok(None) => Err(MessageBusError::SubscriptionClosed),
            Err(_) => Err(MessageBusError::RequestTimeout),
        }
    }

    fn name(&self) -> &'static str {
        "memory"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topic_matching() {
        // Exact match
        assert!(MemoryTransport::topic_matches(
            "agent/123/task",
            "agent/123/task"
        ));
        assert!(!MemoryTransport::topic_matches(
            "agent/123/task",
            "agent/123/other"
        ));

        // Single-level wildcard
        assert!(MemoryTransport::topic_matches(
            "agent/+/task",
            "agent/123/task"
        ));
        assert!(MemoryTransport::topic_matches(
            "agent/+/task",
            "agent/456/task"
        ));
        assert!(!MemoryTransport::topic_matches(
            "agent/+/task",
            "agent/123/other"
        ));
        assert!(!MemoryTransport::topic_matches(
            "agent/+/task",
            "agent/123/task/extra"
        ));

        // Multi-level wildcard
        assert!(MemoryTransport::topic_matches("agent/#", "agent/123"));
        assert!(MemoryTransport::topic_matches("agent/#", "agent/123/task"));
        assert!(MemoryTransport::topic_matches(
            "agent/#",
            "agent/123/task/start"
        ));
        assert!(!MemoryTransport::topic_matches("agent/#", "other/123"));

        // Mixed wildcards
        assert!(MemoryTransport::topic_matches(
            "agent/+/task/#",
            "agent/123/task/start"
        ));
        assert!(MemoryTransport::topic_matches(
            "agent/+/task/#",
            "agent/123/task/start/progress"
        ));

        // Edge cases
        assert!(!MemoryTransport::topic_matches("", "agent/123"));
        assert!(!MemoryTransport::topic_matches("agent/#", ""));
    }

    #[tokio::test]
    async fn test_basic_pub_sub() {
        let transport = MemoryTransport::new();

        // Subscribe
        let (sub_id, mut rx) = transport.subscribe("test/topic").await.unwrap();

        // Publish
        let msg = Message::new("test/topic", b"hello".to_vec());
        transport.publish("test/topic", msg.clone()).await.unwrap();

        // Receive
        let received = rx.recv().await.unwrap();
        assert_eq!(received.payload, b"hello");
        assert_eq!(received.metadata.topic, "test/topic");

        // Unsubscribe
        transport.unsubscribe(sub_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_wildcard_subscription() {
        let transport = MemoryTransport::new();

        // Subscribe with wildcard
        let (_sub_id, mut rx) = transport.subscribe("agent/+/task/+").await.unwrap();

        // Publish matching message
        let msg = Message::new("agent/123/task/start", b"data1".to_vec());
        transport
            .publish("agent/123/task/start", msg)
            .await
            .unwrap();

        // Should receive
        let received = rx.recv().await.unwrap();
        assert_eq!(received.payload, b"data1");

        // Publish another matching message with different agent ID
        let msg2 = Message::new("agent/456/task/complete", b"data2".to_vec());
        transport
            .publish("agent/456/task/complete", msg2)
            .await
            .unwrap();

        let received2 = rx.recv().await.unwrap();
        assert_eq!(received2.payload, b"data2");
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let transport = MemoryTransport::new();

        // Create two subscriptions
        let (_sub1, mut rx1) = transport.subscribe("test/broadcast").await.unwrap();
        let (_sub2, mut rx2) = transport.subscribe("test/broadcast").await.unwrap();

        // Publish
        let msg = Message::new("test/broadcast", b"broadcast message".to_vec());
        transport.publish("test/broadcast", msg).await.unwrap();

        // Both should receive
        let received1 = rx1.recv().await.unwrap();
        let received2 = rx2.recv().await.unwrap();
        assert_eq!(received1.payload, b"broadcast message");
        assert_eq!(received2.payload, b"broadcast message");
    }

    #[tokio::test]
    async fn test_request_reply() {
        let transport = Arc::new(MemoryTransport::new());
        let transport_clone = transport.clone();

        // Spawn a responder
        tokio::spawn(async move {
            let (_sub_id, mut rx) = transport_clone.subscribe("test/request").await.unwrap();

            while let Some(request) = rx.recv().await {
                // Extract correlation ID from request
                let correlation_id = request
                    .metadata
                    .correlation_id
                    .clone()
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                // Send reply to the expected response topic
                let reply_topic = format!("test/request/response/{}", correlation_id);
                let reply = Message::new(&reply_topic, b"response".to_vec())
                    .with_correlation_id(&correlation_id);
                transport_clone.publish(&reply_topic, reply).await.unwrap();
            }
        });

        // Give the responder time to subscribe
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Send request
        let request = Message::new("test/request", b"request data".to_vec());
        let response = transport
            .request("test/request", request, Duration::from_secs(5))
            .await
            .unwrap();

        assert_eq!(response.payload, b"response");
    }

    #[tokio::test]
    async fn test_message_size_limit() {
        let config = TransportConfig {
            max_message_size: 100,
            ..Default::default()
        };
        let transport = MemoryTransport::with_config(config);

        // Small message should succeed
        let small_msg = Message::new("test/topic", vec![0u8; 50]);
        assert!(transport.publish("test/topic", small_msg).await.is_ok());

        // Large message should fail
        let large_msg = Message::new("test/topic", vec![0u8; 200]);
        assert!(transport.publish("test/topic", large_msg).await.is_err());
    }

    #[tokio::test]
    async fn test_stats() {
        let transport = MemoryTransport::new();

        // Initial stats
        let stats = transport.stats();
        assert_eq!(stats.messages_published, 0);

        // Publish some messages
        for i in 0..5 {
            let msg = Message::new("test/topic", format!("data{}", i).into_bytes());
            transport.publish("test/topic", msg).await.unwrap();
        }

        // Check stats
        let stats = transport.stats();
        assert_eq!(stats.messages_published, 5);
        assert!(stats.bytes_published > 0);
    }

    #[tokio::test]
    async fn test_unsubscribe_not_found() {
        let transport = MemoryTransport::new();

        let fake_id = SubscriptionId::new();
        let result = transport.unsubscribe(fake_id).await;
        assert!(result.is_err());
    }
}
