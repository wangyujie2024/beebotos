//! Transport layer implementations

pub mod memory;

pub mod grpc;

use std::time::Duration;

use async_trait::async_trait;

use crate::error::Result;
use crate::{Message, MessageStream, SubscriptionId};

/// Transport trait for message bus backends
///
/// This trait abstracts different transport mechanisms (memory, gRPC, etc.)
/// and provides a unified interface for the message bus.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Publish a message to a topic
    ///
    /// # Arguments
    /// * `topic` - The topic to publish to
    /// * `message` - The message to publish
    async fn publish(&self, topic: &str, message: Message) -> Result<()>;

    /// Subscribe to a topic pattern
    ///
    /// # Arguments
    /// * `topic_pattern` - The topic pattern to subscribe to
    ///
    /// # Returns
    /// A tuple of (subscription_id, message_stream)
    async fn subscribe(&self, topic_pattern: &str) -> Result<(SubscriptionId, MessageStream)>;

    /// Unsubscribe from a topic
    ///
    /// # Arguments
    /// * `id` - The subscription ID to cancel
    async fn unsubscribe(&self, id: SubscriptionId) -> Result<()>;

    /// Send a request and wait for a reply
    ///
    /// # Arguments
    /// * `topic` - The topic to send the request to
    /// * `request` - The request message
    /// * `timeout` - Maximum time to wait for a response
    async fn request(&self, topic: &str, request: Message, timeout: Duration) -> Result<Message>;

    /// Check if the transport is connected/healthy
    async fn is_healthy(&self) -> bool {
        true
    }

    /// Get transport name
    fn name(&self) -> &'static str;
}

/// Transport configuration
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// Maximum message size in bytes
    pub max_message_size: usize,

    /// Connection timeout
    pub connection_timeout: Duration,

    /// Publish timeout
    pub publish_timeout: Duration,

    /// Subscribe buffer size
    pub subscribe_buffer_size: usize,

    /// Enable message persistence
    pub persistent: bool,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            max_message_size: 10 * 1024 * 1024, // 10 MB
            connection_timeout: Duration::from_secs(10),
            publish_timeout: Duration::from_secs(5),
            subscribe_buffer_size: 1000,
            persistent: false,
        }
    }
}

/// Transport statistics
#[derive(Debug, Clone, Default)]
pub struct TransportStats {
    /// Total messages published
    pub messages_published: u64,

    /// Total messages received
    pub messages_received: u64,

    /// Total bytes published
    pub bytes_published: u64,

    /// Total bytes received
    pub bytes_received: u64,

    /// Number of active subscriptions
    pub active_subscriptions: usize,

    /// Number of connection errors
    pub connection_errors: u64,

    /// Average publish latency in microseconds
    pub avg_publish_latency_us: u64,
}

/// Transport metrics trait
#[async_trait]
pub trait TransportMetrics: Send + Sync {
    /// Record a publish operation
    async fn record_publish(&self, topic: &str, bytes: usize, latency: Duration);

    /// Record a receive operation
    async fn record_receive(&self, topic: &str, bytes: usize);

    /// Record a subscription
    async fn record_subscribe(&self, topic_pattern: &str);

    /// Record an unsubscribe
    async fn record_unsubscribe(&self);

    /// Record a connection error
    async fn record_connection_error(&self);

    /// Get current stats
    async fn get_stats(&self) -> TransportStats;
}

/// No-op metrics implementation
pub struct NoopMetrics;

#[async_trait]
impl TransportMetrics for NoopMetrics {
    async fn record_publish(&self, _topic: &str, _bytes: usize, _latency: Duration) {}
    async fn record_receive(&self, _topic: &str, _bytes: usize) {}
    async fn record_subscribe(&self, _topic_pattern: &str) {}
    async fn record_unsubscribe(&self) {}
    async fn record_connection_error(&self) {}
    async fn get_stats(&self) -> TransportStats {
        TransportStats::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_config_default() {
        let config = TransportConfig::default();
        assert_eq!(config.max_message_size, 10 * 1024 * 1024);
        assert_eq!(config.connection_timeout, Duration::from_secs(10));
        assert_eq!(config.subscribe_buffer_size, 1000);
        assert!(!config.persistent);
    }

    #[test]
    fn test_transport_stats_default() {
        let stats = TransportStats::default();
        assert_eq!(stats.messages_published, 0);
        assert_eq!(stats.active_subscriptions, 0);
    }
}
