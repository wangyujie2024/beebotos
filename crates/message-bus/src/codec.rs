//! Message codec implementations for serialization/deserialization

use crate::error::{MessageBusError, Result};

/// Message codec trait
///
/// Implement this trait to provide custom serialization for messages.
pub trait MessageCodec: Send + Sync {
    /// Encode a message to bytes
    fn encode(&self, message: &crate::Message) -> Result<Vec<u8>>;

    /// Decode bytes to a message
    fn decode(&self, data: &[u8]) -> Result<crate::Message>;

    /// Get codec name
    fn name(&self) -> &'static str;

    /// Get content type
    fn content_type(&self) -> &'static str;
}

/// JSON codec implementation
#[derive(Debug, Clone, Copy, Default)]
pub struct JsonCodec;

impl JsonCodec {
    /// Create a new JSON codec
    pub fn new() -> Self {
        Self
    }
}

impl MessageCodec for JsonCodec {
    fn encode(&self, message: &crate::Message) -> Result<Vec<u8>> {
        serde_json::to_vec(message)
            .map_err(|e| MessageBusError::Serialization(format!("JSON encode error: {}", e)))
    }

    fn decode(&self, data: &[u8]) -> Result<crate::Message> {
        serde_json::from_slice(data)
            .map_err(|e| MessageBusError::Deserialization(format!("JSON decode error: {}", e)))
    }

    fn name(&self) -> &'static str {
        "json"
    }

    fn content_type(&self) -> &'static str {
        "application/json"
    }
}

/// MessagePack codec implementation (optional feature)
#[cfg(feature = "msgpack-codec")]
#[derive(Debug, Clone, Copy, Default)]
pub struct MsgPackCodec;

#[cfg(feature = "msgpack-codec")]
impl MsgPackCodec {
    /// Create a new MessagePack codec
    pub fn new() -> Self {
        Self
    }
}

#[cfg(feature = "msgpack-codec")]
impl MessageCodec for MsgPackCodec {
    fn encode(&self, message: &crate::Message) -> Result<Vec<u8>> {
        rmp_serde::to_vec(message)
            .map_err(|e| MessageBusError::Serialization(format!("MessagePack encode error: {}", e)))
    }

    fn decode(&self, data: &[u8]) -> Result<crate::Message> {
        rmp_serde::from_slice(data).map_err(|e| {
            MessageBusError::Deserialization(format!("MessagePack decode error: {}", e))
        })
    }

    fn name(&self) -> &'static str {
        "msgpack"
    }

    fn content_type(&self) -> &'static str {
        "application/msgpack"
    }
}

/// Binary codec (raw bytes, minimal overhead)
///
/// Note: This is a placeholder for future binary protocol implementation.
/// Currently falls back to JSON for metadata.
#[derive(Debug, Clone, Copy, Default)]
pub struct BinaryCodec;

impl BinaryCodec {
    /// Create a new binary codec
    pub fn new() -> Self {
        Self
    }
}

impl MessageCodec for BinaryCodec {
    fn encode(&self, message: &crate::Message) -> Result<Vec<u8>> {
        // For now, use JSON encoding for the full message
        // In a full implementation, this would use a binary format like protobuf
        JsonCodec::new().encode(message)
    }

    fn decode(&self, data: &[u8]) -> Result<crate::Message> {
        JsonCodec::new().decode(data)
    }

    fn name(&self) -> &'static str {
        "binary"
    }

    fn content_type(&self) -> &'static str {
        "application/octet-stream"
    }
}

/// Codec registry for managing multiple codecs
pub struct CodecRegistry {
    codecs: std::collections::HashMap<String, Box<dyn MessageCodec>>,
    default_codec: String,
}

impl CodecRegistry {
    /// Create a new codec registry with default codecs
    pub fn new() -> Self {
        let mut registry = Self {
            codecs: std::collections::HashMap::new(),
            default_codec: "json".to_string(),
        };

        // Register default codecs
        registry.register("json", Box::new(JsonCodec::new()));
        registry.register("binary", Box::new(BinaryCodec::new()));

        #[cfg(feature = "msgpack-codec")]
        registry.register("msgpack", Box::new(MsgPackCodec::new()));

        registry
    }

    /// Register a codec
    pub fn register(&mut self, name: &str, codec: Box<dyn MessageCodec>) {
        self.codecs.insert(name.to_string(), codec);
    }

    /// Get a codec by name
    pub fn get(&self, name: &str) -> Option<&dyn MessageCodec> {
        self.codecs.get(name).map(|c| c.as_ref())
    }

    /// Set the default codec
    pub fn set_default(&mut self, name: &str) -> Result<()> {
        if self.codecs.contains_key(name) {
            self.default_codec = name.to_string();
            Ok(())
        } else {
            Err(MessageBusError::CodecNotAvailable(format!(
                "Codec '{}' not registered",
                name
            )))
        }
    }

    /// Get the default codec
    pub fn default_codec(&self) -> &dyn MessageCodec {
        self.codecs
            .get(&self.default_codec)
            .map(|c| c.as_ref())
            .expect("Default codec must exist")
    }

    /// Get content type for a codec
    pub fn content_type(&self, name: &str) -> Option<&'static str> {
        self.get(name).map(|c| c.content_type())
    }

    /// List available codecs
    pub fn available_codecs(&self) -> Vec<&str> {
        self.codecs.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for CodecRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Message, MessageMetadata};

    #[test]
    fn test_json_codec() {
        let codec = JsonCodec::new();
        let message = Message::new("test/topic", b"hello world".to_vec());

        // Encode
        let encoded = codec.encode(&message).unwrap();
        assert!(!encoded.is_empty());
        assert_eq!(codec.content_type(), "application/json");

        // Decode
        let decoded = codec.decode(&encoded).unwrap();
        assert_eq!(decoded.topic(), "test/topic");
        assert_eq!(decoded.payload, b"hello world");
    }

    #[test]
    fn test_binary_codec() {
        let codec = BinaryCodec::new();
        let message = Message::new("test/topic", vec![0u8, 1u8, 2u8, 3u8]);

        let encoded = codec.encode(&message).unwrap();
        let decoded = codec.decode(&encoded).unwrap();

        assert_eq!(decoded.payload, vec![0u8, 1u8, 2u8, 3u8]);
    }

    #[test]
    fn test_codec_registry() {
        let registry = CodecRegistry::new();

        // Check default codecs are registered
        let codecs = registry.available_codecs();
        assert!(codecs.contains(&"json"));
        assert!(codecs.contains(&"binary"));

        // Get codec
        let codec = registry.get("json").unwrap();
        assert_eq!(codec.name(), "json");

        // Get default
        let default = registry.default_codec();
        assert_eq!(default.name(), "json");
    }

    #[test]
    fn test_codec_registry_custom() {
        let mut registry = CodecRegistry::new();

        // Register custom codec
        registry.register("custom", Box::new(BinaryCodec::new()));

        // Set as default
        registry.set_default("custom").unwrap();
        assert_eq!(registry.default_codec().name(), "binary");

        // Try to set non-existent codec
        assert!(registry.set_default("nonexistent").is_err());
    }

    #[test]
    fn test_complex_message_roundtrip() {
        let codec = JsonCodec::new();

        let mut message = Message::new("agent/123/task/start", b"task data".to_vec())
            .with_correlation_id("corr-123")
            .with_priority(8);

        message
            .metadata
            .headers
            .insert("x-custom".to_string(), "value".to_string());

        let encoded = codec.encode(&message).unwrap();
        let decoded = codec.decode(&encoded).unwrap();

        assert_eq!(decoded.topic(), "agent/123/task/start");
        assert_eq!(
            decoded.metadata.correlation_id,
            Some("corr-123".to_string())
        );
        assert_eq!(decoded.metadata.priority, 8);
        assert_eq!(decoded.metadata.get_header("x-custom"), Some("value"));
    }
}
