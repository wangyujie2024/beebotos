//! Message types for the message bus

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{MessageBusError, Result};

/// Unique message identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub uuid::Uuid);

impl MessageId {
    /// Create a new unique message ID
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    /// Create a message ID from a UUID
    pub fn from_uuid(uuid: uuid::Uuid) -> Self {
        Self(uuid)
    }

    /// Get the underlying UUID
    pub fn as_uuid(&self) -> &uuid::Uuid {
        &self.0
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Message metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMetadata {
    /// Unique message identifier
    pub message_id: MessageId,

    /// Topic the message was published to
    pub topic: String,

    /// Timestamp when the message was created
    pub timestamp: DateTime<Utc>,

    /// Correlation ID for request-reply patterns
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,

    /// Custom headers
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,

    /// Message priority (0-9, higher is more urgent)
    #[serde(default = "default_priority")]
    pub priority: u8,

    /// Time-to-live in seconds (None means no expiration)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl_secs: Option<u64>,
}

fn default_priority() -> u8 {
    5
}

impl MessageMetadata {
    /// Create new metadata for a topic
    pub fn new(topic: impl Into<String>) -> Self {
        Self {
            message_id: MessageId::new(),
            topic: topic.into(),
            timestamp: Utc::now(),
            correlation_id: None,
            headers: HashMap::new(),
            priority: default_priority(),
            ttl_secs: None,
        }
    }

    /// Set the correlation ID
    pub fn with_correlation_id(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }

    /// Add a header
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Set the priority
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority.min(9);
        self
    }

    /// Set the time-to-live
    pub fn with_ttl(mut self, ttl_secs: u64) -> Self {
        self.ttl_secs = Some(ttl_secs);
        self
    }

    /// Check if the message has expired
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl_secs {
            let elapsed = Utc::now().signed_duration_since(self.timestamp);
            elapsed.num_seconds() > ttl as i64
        } else {
            false
        }
    }

    /// Get a header value
    pub fn get_header(&self, key: &str) -> Option<&str> {
        self.headers.get(key).map(|s| s.as_str())
    }
}

/// Message structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message metadata
    pub metadata: MessageMetadata,

    /// Message payload (binary data)
    pub payload: Vec<u8>,
}

impl Message {
    /// Create a new message with raw payload
    pub fn new(topic: impl Into<String>, payload: Vec<u8>) -> Self {
        Self {
            metadata: MessageMetadata::new(topic),
            payload,
        }
    }

    /// Create a message with a serializable payload
    pub fn with_payload<T: Serialize>(topic: impl Into<String>, payload: &T) -> Result<Self> {
        let payload_bytes = serde_json::to_vec(payload)
            .map_err(|e| MessageBusError::Serialization(e.to_string()))?;
        Ok(Self::new(topic, payload_bytes))
    }

    /// Create a message with metadata
    pub fn with_metadata(metadata: MessageMetadata, payload: Vec<u8>) -> Self {
        Self { metadata, payload }
    }

    /// Set the correlation ID
    pub fn with_correlation_id(mut self, id: impl Into<String>) -> Self {
        self.metadata.correlation_id = Some(id.into());
        self
    }

    /// Add a header
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.headers.insert(key.into(), value.into());
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.metadata.priority = priority.min(9);
        self
    }

    /// Decode the payload to a specific type
    pub fn decode_payload<T: for<'de> Deserialize<'de>>(&self) -> Result<T> {
        serde_json::from_slice(&self.payload)
            .map_err(|e| MessageBusError::Deserialization(e.to_string()))
    }

    /// Get the message ID
    pub fn id(&self) -> &MessageId {
        &self.metadata.message_id
    }

    /// Get the topic
    pub fn topic(&self) -> &str {
        &self.metadata.topic
    }

    /// Get the payload size in bytes
    pub fn payload_size(&self) -> usize {
        self.payload.len()
    }

    /// Check if this is a reply message (has correlation ID)
    pub fn is_reply(&self) -> bool {
        self.metadata.correlation_id.is_some()
    }

    /// Create a reply to this message
    pub fn create_reply(&self, payload: Vec<u8>) -> Self {
        let correlation_id = self.metadata.correlation_id.clone().unwrap_or_default();
        let reply_topic = format!("{}/reply", self.metadata.topic);

        Self {
            metadata: MessageMetadata {
                message_id: MessageId::new(),
                topic: reply_topic,
                timestamp: Utc::now(),
                correlation_id: Some(correlation_id),
                headers: self.metadata.headers.clone(),
                priority: self.metadata.priority,
                ttl_secs: None,
            },
            payload,
        }
    }

    /// Check if the message has expired based on TTL
    pub fn is_expired(&self) -> bool {
        self.metadata.is_expired()
    }
}

/// Message builder for convenient message construction
pub struct MessageBuilder {
    metadata: MessageMetadata,
    payload: Option<Vec<u8>>,
}

impl MessageBuilder {
    /// Create a new message builder for a topic
    pub fn new(topic: impl Into<String>) -> Self {
        Self {
            metadata: MessageMetadata::new(topic),
            payload: None,
        }
    }

    /// Set the payload from serializable data
    pub fn payload<T: Serialize>(mut self, payload: &T) -> Result<Self> {
        self.payload = Some(
            serde_json::to_vec(payload)
                .map_err(|e| MessageBusError::Serialization(e.to_string()))?,
        );
        Ok(self)
    }

    /// Set raw payload
    pub fn raw_payload(mut self, payload: Vec<u8>) -> Self {
        self.payload = Some(payload);
        self
    }

    /// Set correlation ID
    pub fn correlation_id(mut self, id: impl Into<String>) -> Self {
        self.metadata.correlation_id = Some(id.into());
        self
    }

    /// Add a header
    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.headers.insert(key.into(), value.into());
        self
    }

    /// Set priority
    pub fn priority(mut self, priority: u8) -> Self {
        self.metadata.priority = priority.min(9);
        self
    }

    /// Set TTL
    pub fn ttl(mut self, ttl_secs: u64) -> Self {
        self.metadata.ttl_secs = Some(ttl_secs);
        self
    }

    /// Build the message
    pub fn build(self) -> Result<Message> {
        let payload = self
            .payload
            .ok_or_else(|| MessageBusError::Serialization("Payload not set".to_string()))?;

        Ok(Message {
            metadata: self.metadata,
            payload,
        })
    }
}

/// Message envelope for internal routing
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct MessageEnvelope {
    pub message: Message,
    pub received_at: DateTime<Utc>,
    pub delivery_attempt: u32,
}

impl MessageEnvelope {
    #[allow(dead_code)]
    pub fn new(message: Message) -> Self {
        Self {
            message,
            received_at: Utc::now(),
            delivery_attempt: 0,
        }
    }

    #[allow(dead_code)]
    pub fn increment_attempt(&mut self) {
        self.delivery_attempt += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestPayload {
        name: String,
        value: i32,
    }

    #[test]
    fn test_message_creation() {
        let msg = Message::new("test/topic", b"hello".to_vec());
        assert_eq!(msg.topic(), "test/topic");
        assert_eq!(msg.payload, b"hello");
        assert!(!msg.is_expired());
    }

    #[test]
    fn test_message_with_payload() {
        let payload = TestPayload {
            name: "test".to_string(),
            value: 42,
        };

        let msg = Message::with_payload("test/topic", &payload).unwrap();
        let decoded: TestPayload = msg.decode_payload().unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn test_message_builder() {
        let payload = TestPayload {
            name: "builder_test".to_string(),
            value: 100,
        };

        let msg = MessageBuilder::new("test/builder")
            .payload(&payload)
            .unwrap()
            .correlation_id("corr-123")
            .header("x-custom", "value")
            .priority(8)
            .ttl(60)
            .build()
            .unwrap();

        assert_eq!(msg.topic(), "test/builder");
        assert_eq!(msg.metadata.correlation_id, Some("corr-123".to_string()));
        assert_eq!(msg.metadata.get_header("x-custom"), Some("value"));
        assert_eq!(msg.metadata.priority, 8);
        assert_eq!(msg.metadata.ttl_secs, Some(60));
    }

    #[test]
    fn test_message_expiration() {
        let metadata = MessageMetadata::new("test/expiry").with_ttl(1);
        let msg = Message::with_metadata(metadata, b"test".to_vec());

        // Should not be expired immediately
        assert!(!msg.is_expired());

        // Simulate expiration by waiting
        std::thread::sleep(std::time::Duration::from_secs(2));
        assert!(msg.is_expired());
    }

    #[test]
    fn test_message_reply() {
        let original =
            Message::new("test/request", b"request data".to_vec()).with_correlation_id("req-123");

        let reply = original.create_reply(b"response data".to_vec());

        assert_eq!(reply.metadata.correlation_id, Some("req-123".to_string()));
        assert!(reply.topic().ends_with("/reply"));
        assert_eq!(reply.payload, b"response data");
    }

    #[test]
    fn test_priority_clamping() {
        let msg = Message::new("test/priority", b"test".to_vec()).with_priority(15);

        assert_eq!(msg.metadata.priority, 9);
    }
}
