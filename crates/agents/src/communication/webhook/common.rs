//! Common Webhook Components
//!
//! Provides shared functionality for all webhook handlers:
//! - Signature verification traits and implementations
//! - Message mapping utilities
//! - Common response builders
//! - Utility functions

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::communication::channel::ChannelEvent;
use crate::communication::{PlatformMessage, PlatformType};
use crate::error::{AgentError, Result};

// =============================================================================
// Signature Verification
// =============================================================================

/// Compute HMAC-SHA256 signature with optional prefix
pub fn compute_hmac_sha256(secret: &str, message: &[u8], prefix: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    if !prefix.is_empty() {
        mac.update(prefix.as_bytes());
    }
    mac.update(message);
    let result = mac.finalize();
    hex::encode(result.into_bytes())
}

/// Verify HMAC-SHA256 signature
pub fn verify_hmac_sha256(secret: &str, message: &[u8], signature: &str) -> bool {
    let computed = compute_hmac_sha256(secret, message, "");
    computed.eq_ignore_ascii_case(signature)
}

/// Signature verification result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureVerification {
    Valid,
    Invalid,
    Skipped,
}

/// Signature verifier trait
#[async_trait]
pub trait SignatureVerifier: Send + Sync {
    /// Verify request signature
    async fn verify(
        &self,
        body: &[u8],
        signature: Option<&str>,
        timestamp: Option<&str>,
    ) -> Result<SignatureVerification>;
}

/// HMAC-SHA256 signature verifier
pub struct HmacSha256Verifier {
    secret: String,
}

impl HmacSha256Verifier {
    pub fn new(secret: impl Into<String>) -> Self {
        Self {
            secret: secret.into(),
        }
    }

    /// Compute HMAC-SHA256 signature
    pub fn compute_signature(&self, body: &[u8]) -> String {
        compute_hmac_sha256(&self.secret, body, "")
    }

    /// Compute HMAC-SHA256 signature with custom prefix
    pub fn compute_signature_with_prefix(&self, body: &[u8], prefix: &str) -> String {
        compute_hmac_sha256(&self.secret, body, prefix)
    }
}

#[async_trait]
impl SignatureVerifier for HmacSha256Verifier {
    async fn verify(
        &self,
        body: &[u8],
        signature: Option<&str>,
        _timestamp: Option<&str>,
    ) -> Result<SignatureVerification> {
        let signature = match signature {
            Some(s) => s,
            None => return Ok(SignatureVerification::Skipped),
        };

        let computed = self.compute_signature(body);

        if computed.eq_ignore_ascii_case(signature) {
            Ok(SignatureVerification::Valid)
        } else {
            Ok(SignatureVerification::Invalid)
        }
    }
}

/// Simple token verifier
pub struct TokenVerifier {
    token: String,
}

impl TokenVerifier {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }
}

#[async_trait]
impl SignatureVerifier for TokenVerifier {
    async fn verify(
        &self,
        _body: &[u8],
        signature: Option<&str>,
        _timestamp: Option<&str>,
    ) -> Result<SignatureVerification> {
        match signature {
            Some(s) if s == self.token => Ok(SignatureVerification::Valid),
            Some(_) => Ok(SignatureVerification::Invalid),
            None => Ok(SignatureVerification::Skipped),
        }
    }
}

/// No-op verifier (for development or unsigned webhooks)
pub struct NoopVerifier;

#[async_trait]
impl SignatureVerifier for NoopVerifier {
    async fn verify(
        &self,
        _body: &[u8],
        _signature: Option<&str>,
        _timestamp: Option<&str>,
    ) -> Result<SignatureVerification> {
        Ok(SignatureVerification::Skipped)
    }
}

// =============================================================================
// Message Mapping
// =============================================================================

/// Event type from webhook
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookEventType {
    MessageReceived,
    UserJoined,
    UserLeft,
    ChannelCreated,
    ChannelUpdated,
    ChannelDeleted,
    ReactionAdded,
    ReactionRemoved,
    FileShared,
    Unknown,
}

/// Message mapper trait
pub trait MessageMapper<Payload> {
    /// Map platform-specific payload to event type
    fn map_event_type(&self, payload: &Payload) -> WebhookEventType;

    /// Map platform-specific payload to message
    fn map_to_message(&self, payload: &Payload) -> Option<PlatformMessage>;

    /// Extract sender ID from payload
    fn extract_sender_id(&self, payload: &Payload) -> Option<String>;

    /// Extract channel/chat ID from payload
    fn extract_channel_id(&self, payload: &Payload) -> Option<String>;
}

/// Metadata builder for messages
pub struct MetadataBuilder {
    metadata: HashMap<String, String>,
}

impl MetadataBuilder {
    pub fn new() -> Self {
        Self {
            metadata: HashMap::new(),
        }
    }

    pub fn add(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn add_optional(
        mut self,
        key: impl Into<String>,
        value: Option<impl Into<String>>,
    ) -> Self {
        if let Some(v) = value {
            self.metadata.insert(key.into(), v.into());
        }
        self
    }

    pub fn build(self) -> HashMap<String, String> {
        self.metadata
    }
}

impl Default for MetadataBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Webhook Response
// =============================================================================

/// Webhook response trait
pub trait WebhookResponse: Serialize {
    /// Create success response
    fn success() -> Self;

    /// Create error response
    fn error(message: impl Into<String>) -> Self;

    /// Create text response
    fn text(content: impl Into<String>) -> Self;
}

/// Standard JSON webhook response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonWebhookResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl WebhookResponse for JsonWebhookResponse {
    fn success() -> Self {
        Self {
            status: "ok".to_string(),
            message: None,
            data: None,
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            status: "error".to_string(),
            message: Some(message.into()),
            data: None,
        }
    }

    fn text(content: impl Into<String>) -> Self {
        Self {
            status: "ok".to_string(),
            message: Some(content.into()),
            data: None,
        }
    }
}

// =============================================================================
// Common Utilities
// =============================================================================

/// Parse timestamp string to datetime
pub fn parse_timestamp(timestamp: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    timestamp
        .parse::<i64>()
        .ok()
        .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
        .or_else(|| {
            chrono::DateTime::parse_from_rfc3339(timestamp)
                .ok()
                .map(|dt| dt.into())
        })
}

/// Extract message type from string
pub fn parse_message_type(msg_type: &str) -> crate::communication::MessageType {
    match msg_type {
        "text" => crate::communication::MessageType::Text,
        "image" | "photo" => crate::communication::MessageType::Image,
        "video" => crate::communication::MessageType::Video,
        "audio" | "voice" => crate::communication::MessageType::Voice,
        "file" | "document" => crate::communication::MessageType::File,
        "location" => crate::communication::MessageType::Text, // Location maps to Text
        "sticker" => crate::communication::MessageType::Sticker,
        "system" => crate::communication::MessageType::System,
        _ => crate::communication::MessageType::Text, // Default to Text for unknown
    }
}

/// Create channel event from message
pub fn create_message_event(
    platform: PlatformType,
    channel_id: String,
    message: PlatformMessage,
) -> ChannelEvent {
    ChannelEvent::MessageReceived {
        platform,
        channel_id,
        message,
    }
}

/// Safe JSON extraction helper
pub fn get_json_string<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for key in path {
        current = current.get(key)?;
    }
    current.as_str()
}

/// Safe JSON extraction for nested objects
pub fn get_json_value<'a>(
    value: &'a serde_json::Value,
    path: &[&str],
) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for key in path {
        current = current.get(key)?;
    }
    Some(current)
}

/// Convert HTTP header value to string
///
/// Takes a string representation of a header value
pub fn header_to_string(value: Option<&str>) -> Option<String> {
    value.map(|s| s.to_string())
}

/// Generate webhook path for platform
pub fn webhook_path(platform: &str) -> String {
    format!("/webhook/{}", platform.to_lowercase())
}

/// Validate webhook secret length
pub fn validate_secret(secret: &str, min_length: usize) -> Result<()> {
    if secret.len() < min_length {
        return Err(AgentError::platform(format!(
            "Secret too short: {} chars (min: {})",
            secret.len(),
            min_length
        )));
    }
    Ok(())
}

// =============================================================================
// Encryption/Decryption utilities (for WeChat-style encryption)
// =============================================================================

/// AES encryption/decryption utility
pub mod encryption {
    use crate::error::{AgentError, Result};

    /// Decrypt AES-CBC encrypted message
    ///
    /// WeChat uses a specific format:
    /// - First 16 bytes: random prefix
    /// - Next 4 bytes: content length (big-endian)
    /// - Content bytes
    /// - App ID at the end
    pub fn aes_cbc_decrypt(encrypted_data: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>> {
        use aes::cipher::block_padding::NoPadding;
        use aes::cipher::{BlockDecryptMut, KeyIvInit};

        type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;

        if key.len() != 32 {
            return Err(AgentError::platform("AES key must be 32 bytes for AES-256"));
        }

        let iv = if iv.len() == 16 {
            iv
        } else {
            key[..16].as_ref()
        };

        tracing::info!(
            "AES decrypt: key_len={}, iv_len={}, data_len={}",
            key.len(),
            iv.len(),
            encrypted_data.len()
        );
        tracing::info!("First 4 bytes of key: {:?}", &key[..4.min(key.len())]);
        tracing::info!("First 4 bytes of IV: {:?}", &iv[..4.min(iv.len())]);

        // Allocate buffer for decrypted data (same size as encrypted data)
        let mut buffer = encrypted_data.to_vec();

        // Decrypt without padding first (like Python)
        Aes256CbcDec::new(key.into(), iv.into())
            .decrypt_padded_mut::<NoPadding>(&mut buffer)
            .map_err(|e| AgentError::platform(format!("AES decryption failed: {:?}", e)))?;

        // Manual PKCS7 unpadding
        if buffer.is_empty() {
            return Err(AgentError::platform("Decrypted buffer is empty"));
        }
        let pad_len = buffer[buffer.len() - 1] as usize;
        tracing::info!("PKCS7 pad_len: {}", pad_len);

        if pad_len > 0 && pad_len <= 32 && pad_len <= buffer.len() {
            buffer.truncate(buffer.len() - pad_len);
        }

        Ok(buffer)
    }

    /// Encrypt message with AES-CBC
    pub fn aes_cbc_encrypt(data: &[u8], key: &[u8], iv: &[u8]) -> Result<Vec<u8>> {
        use aes::cipher::block_padding::Pkcs7;
        use aes::cipher::{BlockEncryptMut, KeyIvInit};

        type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;

        if key.len() != 32 {
            return Err(AgentError::platform("AES key must be 32 bytes for AES-256"));
        }

        let iv = if iv.len() == 16 {
            iv
        } else {
            key[..16].as_ref()
        };

        // Allocate buffer with space for padding
        let mut buffer = vec![0u8; data.len() + 16];
        buffer[..data.len()].copy_from_slice(data);

        let encrypted = Aes256CbcEnc::new(key.into(), iv.into())
            .encrypt_padded_mut::<Pkcs7>(&mut buffer, data.len())
            .map_err(|e| AgentError::platform(format!("AES encryption failed: {:?}", e)))?;

        Ok(encrypted.to_vec())
    }

    /// Extract content from WeChat encrypted message format
    /// Format: random(16) + msg_len(4) + msg + appid
    pub fn extract_wechat_content(decrypted: &[u8]) -> Result<(String, String)> {
        if decrypted.len() < 20 {
            return Err(AgentError::platform(
                "Decrypted data too short for WeChat format",
            ));
        }

        // Skip random prefix (16 bytes)
        let content_len =
            u32::from_be_bytes([decrypted[16], decrypted[17], decrypted[18], decrypted[19]])
                as usize;

        if content_len + 20 > decrypted.len() {
            return Err(AgentError::platform(
                "Content length exceeds decrypted data",
            ));
        }

        let content = String::from_utf8(decrypted[20..20 + content_len].to_vec())
            .map_err(|e| AgentError::platform(format!("Invalid UTF-8 in content: {}", e)))?;

        // app_id is after content, before padding
        // Find corp_id by looking for non-null bytes after content
        let remaining = &decrypted[20 + content_len..];
        // Remove PKCS7 padding (last byte is padding length)
        let app_id_len = if remaining.is_empty() {
            0
        } else {
            let pad_len = remaining[remaining.len() - 1] as usize;
            if pad_len > 0 && pad_len <= remaining.len() {
                remaining.len() - pad_len
            } else {
                remaining.len()
            }
        };
        let app_id = String::from_utf8(remaining[..app_id_len].to_vec())
            .map_err(|e| AgentError::platform(format!("Invalid UTF-8 in app ID: {}", e)))?;

        Ok((content, app_id))
    }
}

// =============================================================================
// Re-exports for convenience
// =============================================================================

pub use super::utils::*;
