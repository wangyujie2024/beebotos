//! BeeBotOS Unified Message Bus - Phase 1 Implementation
//!
//! A high-performance, type-safe message bus for decoupled inter-module
//! communication.
//!
//! # Features
//! - Topic-based publish/subscribe messaging
//! - Request-reply pattern support
//! - Multiple transport backends (memory, grpc)
//! - Type-safe message serialization
//! - Built-in observability (metrics, tracing)
//! - Wildcard topic subscriptions
//! - Cluster federation support (gRPC)
//!
//! # Example
//! ```rust,ignore
//! use beebotos_message_bus::{MessageBus, DefaultMessageBus, JsonCodec, MemoryTransport, Message};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create message bus with in-memory transport
//!     let bus = DefaultMessageBus::new(MemoryTransport::new(), Box::new(JsonCodec::new()), None);
//!
//!     // Subscribe to events
//!     let (sub_id, mut stream) = bus.subscribe("agent/+/task/+").await?;
//!
//!     // Publish a message
//!     let msg = Message::new("agent/123/task/start", b"task data".to_vec());
//!     bus.publish("agent/123/task/start", msg).await?;
//!
//!     // Receive the message
//!     if let Some(message) = stream.recv().await {
//!         println!("Received: {:?}", message);
//!     }
//!
//!     Ok(())
//! }
//! ```

pub mod codec;
pub mod compat;
pub mod config;
pub mod error;
pub mod message;
pub mod metrics;
pub mod persistence;
pub mod router;
pub mod tracing;
pub mod transport;

/// Message bus version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// Re-export core types
#[cfg(feature = "msgpack-codec")]
pub use codec::MsgPackCodec;
pub use codec::{JsonCodec, MessageCodec};
pub use compat::{
    AgentsEventAdapter, ChainEventAdapter, CoreEventBusAdapter, KernelEventAdapter, MigrationGuide,
};

/// Gateway module compatibility
pub mod gateway {
    pub use super::compat::gateway::*;
}
use std::sync::Arc;

use async_trait::async_trait;
pub use config::{ConfigError, MessageBusConfig};
pub use error::{MessageBusError, Result};
pub use message::{Message, MessageId, MessageMetadata};
pub use metrics::MessageBusMetrics;
pub use router::{RouteRule, Router, TopicMatcher};
use tokio::sync::mpsc;
#[cfg(feature = "memory")]
pub use transport::memory::MemoryTransport;
pub use transport::Transport;

/// Subscription identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriptionId(pub uuid::Uuid);

impl SubscriptionId {
    /// Create a new unique subscription ID
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for SubscriptionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Message stream for receiving messages
pub type MessageStream = mpsc::UnboundedReceiver<Message>;

/// Core message bus trait
///
/// This trait defines the fundamental operations for a message bus:
/// - Publishing messages to topics
/// - Subscribing to topics with wildcard support
/// - Request-reply pattern
#[async_trait]
pub trait MessageBus: Send + Sync {
    /// Publish a message to a topic
    ///
    /// The message will be delivered to all subscribers matching the topic.
    ///
    /// # Arguments
    /// * `topic` - The topic to publish to (e.g., "agent/123/task/start")
    /// * `message` - The message to publish
    ///
    /// # Errors
    /// Returns an error if the transport fails to publish
    async fn publish(&self, topic: &str, message: Message) -> Result<()>;

    /// Subscribe to a topic pattern
    ///
    /// Supports wildcard subscriptions:
    /// - `agent/123/task/start` - exact match
    /// - `agent/+/task/start` - match any agent ID
    /// - `agent/#` - match any topic under agent/
    ///
    /// # Arguments
    /// * `topic_pattern` - The topic pattern to subscribe to
    ///
    /// # Returns
    /// A tuple of (subscription_id, message_stream)
    ///
    /// # Errors
    /// Returns an error if the subscription fails
    async fn subscribe(&self, topic_pattern: &str) -> Result<(SubscriptionId, MessageStream)>;

    /// Unsubscribe from a topic
    ///
    /// # Arguments
    /// * `id` - The subscription ID to cancel
    ///
    /// # Errors
    /// Returns an error if the subscription doesn't exist
    async fn unsubscribe(&self, id: SubscriptionId) -> Result<()>;

    /// Send a request and wait for a reply
    ///
    /// Implements the request-reply pattern. The request is published to the
    /// topic, and the method waits for a response on a temporary topic.
    ///
    /// # Arguments
    /// * `topic` - The topic to send the request to
    /// * `request` - The request message
    /// * `timeout` - Maximum time to wait for a response
    ///
    /// # Returns
    /// The response message
    ///
    /// # Errors
    /// Returns an error on timeout or transport failure
    async fn request(
        &self,
        topic: &str,
        request: Message,
        timeout: std::time::Duration,
    ) -> Result<Message>;
}

/// Auto-implement MessageBus for Arc<T> where T: MessageBus
#[async_trait]
impl<T: MessageBus> MessageBus for Arc<T> {
    async fn publish(&self, topic: &str, message: Message) -> Result<()> {
        self.as_ref().publish(topic, message).await
    }

    async fn subscribe(&self, topic_pattern: &str) -> Result<(SubscriptionId, MessageStream)> {
        self.as_ref().subscribe(topic_pattern).await
    }

    async fn unsubscribe(&self, id: SubscriptionId) -> Result<()> {
        self.as_ref().unsubscribe(id).await
    }

    async fn request(
        &self,
        topic: &str,
        request: Message,
        timeout: std::time::Duration,
    ) -> Result<Message> {
        self.as_ref().request(topic, request, timeout).await
    }
}

/// Message bus builder for convenient configuration
pub struct MessageBusBuilder<T: Transport> {
    transport: T,
    codec: Option<Box<dyn MessageCodec>>,
    metrics: Option<MessageBusMetrics>,
}

impl<T: Transport> MessageBusBuilder<T> {
    /// Create a new builder with the given transport
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            codec: None,
            metrics: None,
        }
    }

    /// Set the message codec
    pub fn with_codec(mut self, codec: Box<dyn MessageCodec>) -> Self {
        self.codec = Some(codec);
        self
    }

    /// Enable metrics collection
    pub fn with_metrics(mut self, metrics: MessageBusMetrics) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Build the message bus
    pub fn build(self) -> DefaultMessageBus<T> {
        DefaultMessageBus::new(
            self.transport,
            self.codec.unwrap_or_else(|| Box::new(JsonCodec)),
            self.metrics,
        )
    }
}

/// Default message bus implementation
#[derive(Clone)]
pub struct DefaultMessageBus<T: Transport> {
    transport: T,
    codec: Arc<dyn MessageCodec>,
    metrics: Option<Arc<MessageBusMetrics>>,
}

impl<T: Transport> DefaultMessageBus<T> {
    /// Create a new message bus
    pub fn new(
        transport: T,
        codec: Box<dyn MessageCodec>,
        metrics: Option<MessageBusMetrics>,
    ) -> Self {
        Self {
            transport,
            codec: Arc::from(codec),
            metrics: metrics.map(Arc::new),
        }
    }

    /// Get the codec
    pub fn codec(&self) -> &dyn MessageCodec {
        self.codec.as_ref()
    }

    /// Record metrics if enabled
    fn record_publish(&self, topic: &str, latency: std::time::Duration) {
        if let Some(ref metrics) = self.metrics {
            metrics.record_publish(topic, latency);
        }
    }
}

#[async_trait]
impl<T: Transport> MessageBus for DefaultMessageBus<T> {
    async fn publish(&self, topic: &str, message: Message) -> Result<()> {
        let start = std::time::Instant::now();
        let result = self.transport.publish(topic, message).await;
        self.record_publish(topic, start.elapsed());
        result
    }

    async fn subscribe(&self, topic_pattern: &str) -> Result<(SubscriptionId, MessageStream)> {
        self.transport.subscribe(topic_pattern).await
    }

    async fn unsubscribe(&self, id: SubscriptionId) -> Result<()> {
        self.transport.unsubscribe(id).await
    }

    async fn request(
        &self,
        topic: &str,
        request: Message,
        timeout: std::time::Duration,
    ) -> Result<Message> {
        self.transport.request(topic, request, timeout).await
    }
}

/// Utility module for common operations
pub mod utils {
    use super::*;

    /// Validate a topic string
    ///
    /// Topics must:
    /// - Not be empty
    /// - Not start with /
    /// - Only contain alphanumeric characters, /, -, _, and *
    pub fn validate_topic(topic: &str) -> Result<()> {
        if topic.is_empty() {
            return Err(MessageBusError::InvalidTopic(
                "Topic cannot be empty".to_string(),
            ));
        }

        if topic.starts_with('/') {
            return Err(MessageBusError::InvalidTopic(
                "Topic cannot start with /".to_string(),
            ));
        }

        // Check for valid characters
        for c in topic.chars() {
            if !c.is_alphanumeric() && !matches!(c, '/' | '-' | '_' | '*' | '#') {
                return Err(MessageBusError::InvalidTopic(format!(
                    "Invalid character '{}' in topic",
                    c
                )));
            }
        }

        Ok(())
    }

    /// Match a topic against a pattern
    ///
    /// Supports:
    /// - Exact match: "agent/123/task" matches "agent/123/task"
    /// - Single-level wildcard (+): "agent/+/task" matches "agent/123/task"
    /// - Multi-level wildcard (#): "agent/#" matches "agent/123/task/start"
    pub fn topic_matches(pattern: &str, topic: &str) -> bool {
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

        // Check if we've consumed all parts
        // Pattern might end with # which matches empty
        if p_idx < pattern_parts.len() && pattern_parts[p_idx] == "#" {
            return true;
        }

        p_idx == pattern_parts.len() && t_idx == topic_parts.len()
    }
}

#[cfg(test)]
mod tests {
    use super::utils::*;
    use super::*;

    #[test]
    fn test_topic_validation() {
        assert!(validate_topic("agent/123/task").is_ok());
        assert!(validate_topic("agent-123_task").is_ok());
        assert!(validate_topic("agent/*/task").is_ok());
        assert!(validate_topic("").is_err());
        assert!(validate_topic("/agent").is_err());
    }

    #[test]
    fn test_topic_matching() {
        // Exact match
        assert!(topic_matches("agent/123/task", "agent/123/task"));
        assert!(!topic_matches("agent/123/task", "agent/123/other"));

        // Single-level wildcard
        assert!(topic_matches("agent/+/task", "agent/123/task"));
        assert!(topic_matches("agent/+/task", "agent/456/task"));
        assert!(!topic_matches("agent/+/task", "agent/123/other"));
        assert!(!topic_matches("agent/+/task", "agent/123/task/extra"));

        // Multi-level wildcard
        assert!(topic_matches("agent/#", "agent/123"));
        assert!(topic_matches("agent/#", "agent/123/task"));
        assert!(topic_matches("agent/#", "agent/123/task/start"));
        assert!(!topic_matches("agent/#", "other/123"));

        // Mixed wildcards
        assert!(topic_matches("agent/+/task/#", "agent/123/task/start"));
        assert!(topic_matches(
            "agent/+/task/#",
            "agent/123/task/start/progress"
        ));
    }

    #[tokio::test]
    async fn test_message_bus_basic() {
        let transport = MemoryTransport::new();
        let bus = DefaultMessageBus::new(transport, Box::new(JsonCodec), None);

        // Subscribe
        let (sub_id, mut stream) = bus.subscribe("test/topic").await.unwrap();

        // Publish
        let msg = Message::new("test/topic", b"hello".to_vec());
        bus.publish("test/topic", msg).await.unwrap();

        // Receive
        let received = stream.recv().await.unwrap();
        assert_eq!(received.payload, b"hello");

        // Unsubscribe
        bus.unsubscribe(sub_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_wildcard_subscription() {
        let transport = MemoryTransport::new();
        let bus = DefaultMessageBus::new(transport, Box::new(JsonCodec), None);

        // Subscribe with wildcard
        let (_sub_id, mut stream) = bus.subscribe("agent/+/task/+").await.unwrap();

        // Publish matching message
        let msg = Message::new("agent/123/task/start", b"data".to_vec());
        bus.publish("agent/123/task/start", msg).await.unwrap();

        // Should receive
        let received = stream.recv().await.unwrap();
        assert_eq!(received.metadata.topic, "agent/123/task/start");

        // Publish non-matching message
        let msg2 = Message::new("agent/123/other/start", b"other".to_vec());
        bus.publish("agent/123/other/start", msg2).await.unwrap();

        // Should timeout (no matching subscriber)
        let result =
            tokio::time::timeout(std::time::Duration::from_millis(100), stream.recv()).await;
        assert!(result.is_err());
    }
}

/// Re-export common types for convenience
pub mod prelude {
    pub use crate::codec::{JsonCodec, MessageCodec};
    pub use crate::config::{ConfigError, MessageBusConfig};
    pub use crate::error::{MessageBusError, Result};
    pub use crate::tracing::TraceContext;
    pub use crate::transport::grpc::GrpcTransport;
    #[cfg(feature = "memory")]
    pub use crate::transport::memory::MemoryTransport;
    pub use crate::{
        Message, MessageBus, MessageId, MessageMetadata, MessageStream, SubscriptionId,
    };
}
