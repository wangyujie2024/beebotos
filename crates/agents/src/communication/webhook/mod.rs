//! Webhook Handler Module
//!
//! Provides a unified framework for handling incoming webhooks from various
//! platforms. Supports signature verification, message routing, and event
//! processing.

pub mod common;
use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
pub use common::*;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::communication::{Message, PlatformType};
use crate::error::{AgentError, Result};

pub mod lark;
pub use lark::LarkWebhookHandler;

pub mod dingtalk;
pub use dingtalk::DingTalkWebhookHandler;

pub mod telegram;
pub use telegram::{TelegramWebhookConfig, TelegramWebhookHandler};

pub mod discord;
pub use discord::DiscordWebhookHandler;

pub mod slack;
pub use slack::SlackWebhookHandler;

pub mod twitter;
pub use twitter::TwitterWebhookHandler;

pub mod wechat;
pub use wechat::WeChatWebhookHandler;

pub mod teams;
pub use teams::TeamsWebhookHandler;

pub mod whatsapp;
pub use whatsapp::WhatsAppWebhookHandler;

pub mod signal;
pub use signal::SignalWebhookHandler;

pub mod matrix;
pub use matrix::MatrixWebhookHandler;

pub mod imessage;
pub use imessage::IMessageWebhookHandler;

/// Webhook event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WebhookEventType {
    /// Message received event
    MessageReceived,
    /// Message edited event
    MessageEdited,
    /// Message deleted event
    MessageDeleted,
    /// User joined event
    UserJoined,
    /// User left event
    UserLeft,
    /// Bot mentioned event
    BotMentioned,
    /// File shared event
    FileShared,
    /// Voice message event
    VoiceMessage,
    /// Video message event
    VideoMessage,
    /// System event
    System,
    /// Unknown event
    Unknown,
}

/// Webhook event payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEvent {
    /// Event type
    pub event_type: WebhookEventType,
    /// Platform type
    pub platform: PlatformType,
    /// Event ID (unique per platform)
    pub event_id: String,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Raw payload
    pub payload: serde_json::Value,
    /// Parsed message (if applicable)
    pub message: Option<Message>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Webhook signature verification result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureVerification {
    /// Signature is valid
    Valid,
    /// Signature is invalid
    Invalid,
    /// Signature verification skipped (no signature provided)
    Skipped,
}

/// Webhook handler trait
///
/// Implement this trait to create custom webhook handlers for different
/// platforms.
#[async_trait]
pub trait WebhookHandler: Send + Sync {
    /// Get the platform type this handler supports
    fn platform_type(&self) -> PlatformType;

    /// Verify webhook signature
    ///
    /// # Arguments
    /// * `body` - Raw request body
    /// * `signature` - Signature from request header
    /// * `timestamp` - Timestamp from request header (if available)
    ///
    /// # Returns
    /// Signature verification result
    async fn verify_signature(
        &self,
        body: &[u8],
        signature: Option<&str>,
        timestamp: Option<&str>,
    ) -> Result<SignatureVerification>;

    /// Parse webhook payload into events
    ///
    /// # Arguments
    /// * `body` - Raw request body
    ///
    /// # Returns
    /// Parsed webhook events
    async fn parse_payload(&self, body: &[u8]) -> Result<Vec<WebhookEvent>>;

    /// Handle webhook event
    ///
    /// # Arguments
    /// * `event` - Webhook event to handle
    ///
    /// # Returns
    /// Result of handling the event
    async fn handle_event(&self, event: WebhookEvent) -> Result<()>;

    /// Get webhook configuration
    fn get_config(&self) -> &WebhookConfig;
}

/// Webhook configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// Platform type
    pub platform: PlatformType,
    /// Webhook endpoint path
    pub endpoint_path: String,
    /// Secret for signature verification
    pub secret: Option<String>,
    /// Encryption key (if message encryption is enabled)
    pub encryption_key: Option<String>,
    /// Whether to verify signatures
    pub verify_signatures: bool,
    /// Whether to decrypt messages
    pub decrypt_messages: bool,
    /// Allowed IP ranges (CIDR notation)
    pub allowed_ips: Vec<String>,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// Maximum body size in bytes
    pub max_body_size: usize,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            platform: PlatformType::Custom,
            endpoint_path: "/webhook".to_string(),
            secret: None,
            encryption_key: None,
            verify_signatures: true,
            decrypt_messages: false,
            allowed_ips: vec![],
            timeout_secs: 30,
            max_body_size: 10 * 1024 * 1024, // 10MB
        }
    }
}

/// Webhook registration
#[derive(Clone)]
pub struct WebhookRegistration {
    /// Handler instance
    pub handler: Arc<dyn WebhookHandler>,
    /// Configuration
    pub config: WebhookConfig,
}

/// Webhook manager
///
/// Manages multiple webhook handlers and routes incoming requests to the
/// appropriate handler.
pub struct WebhookManager {
    /// Registered handlers by endpoint path
    handlers: Arc<RwLock<HashMap<String, WebhookRegistration>>>,
    /// Event callbacks
    event_callbacks: Arc<RwLock<Vec<Box<dyn Fn(WebhookEvent) + Send + Sync>>>>,
}

impl WebhookManager {
    /// Create a new webhook manager
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            event_callbacks: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register a webhook handler
    ///
    /// # Arguments
    /// * `handler` - Webhook handler implementation
    ///
    /// # Returns
    /// Result indicating success or failure
    pub async fn register_handler(&self, handler: Arc<dyn WebhookHandler>) -> Result<()> {
        let config = handler.get_config();
        let path = config.endpoint_path.clone();
        let config_clone = config.clone();

        let mut handlers = self.handlers.write().await;
        if handlers.contains_key(&path) {
            return Err(AgentError::configuration(format!(
                "Webhook handler already registered for path: {}",
                path
            )));
        }

        info!(
            "Registering webhook handler for platform {:?} at path: {}",
            config.platform, path
        );

        handlers.insert(
            path,
            WebhookRegistration {
                handler,
                config: config_clone,
            },
        );

        Ok(())
    }

    /// Unregister a webhook handler
    ///
    /// # Arguments
    /// * `path` - Endpoint path
    pub async fn unregister_handler(&self, path: &str) -> Result<()> {
        let mut handlers = self.handlers.write().await;
        if handlers.remove(path).is_some() {
            info!("Unregistered webhook handler for path: {}", path);
            Ok(())
        } else {
            Err(AgentError::not_found(format!(
                "No webhook handler found for path: {}",
                path
            )))
        }
    }

    /// Handle incoming webhook request
    ///
    /// # Arguments
    /// * `path` - Request path
    /// * `body` - Request body
    /// * `signature` - Signature header value
    /// * `timestamp` - Timestamp header value
    ///
    /// # Returns
    /// Result indicating success or failure
    pub async fn handle_request(
        &self,
        path: &str,
        body: &[u8],
        signature: Option<&str>,
        timestamp: Option<&str>,
    ) -> Result<Vec<WebhookEvent>> {
        let handlers = self.handlers.read().await;
        let registration = handlers.get(path).ok_or_else(|| {
            AgentError::not_found(format!("No webhook handler found for path: {}", path))
        })?;

        let handler = &registration.handler;
        let config = &registration.config;

        // Check body size
        if body.len() > config.max_body_size {
            return Err(AgentError::platform(format!(
                "Request body too large: {} bytes (max: {})",
                body.len(),
                config.max_body_size
            )));
        }

        // Verify signature if required
        if config.verify_signatures {
            let verification = handler.verify_signature(body, signature, timestamp).await?;
            match verification {
                SignatureVerification::Valid => {
                    debug!("Webhook signature verified for path: {}", path);
                }
                SignatureVerification::Invalid => {
                    error!("Invalid webhook signature for path: {}", path);
                    return Err(AgentError::authentication("Invalid webhook signature"));
                }
                SignatureVerification::Skipped => {
                    warn!("Webhook signature verification skipped for path: {}", path);
                }
            }
        }

        // Parse payload
        let events = handler.parse_payload(body).await?;
        debug!("Parsed {} events from webhook payload", events.len());

        // Process events
        let mut processed_events = Vec::new();
        for event in events {
            // Notify callbacks
            self.notify_callbacks(&event).await;

            // Handle event
            if let Err(e) = handler.handle_event(event.clone()).await {
                error!("Failed to handle webhook event: {}", e);
                // Continue processing other events even if one fails
            } else {
                processed_events.push(event);
            }
        }

        Ok(processed_events)
    }

    /// Register event callback
    ///
    /// # Arguments
    /// * `callback` - Callback function
    pub async fn on_event<F>(&self, callback: F)
    where
        F: Fn(WebhookEvent) + Send + Sync + 'static,
    {
        let mut callbacks = self.event_callbacks.write().await;
        callbacks.push(Box::new(callback));
    }

    /// Notify all registered callbacks
    async fn notify_callbacks(&self, event: &WebhookEvent) {
        let callbacks = self.event_callbacks.read().await;
        for callback in callbacks.iter() {
            callback(event.clone());
        }
    }

    /// Get registered handler for a path
    pub async fn get_handler(&self, path: &str) -> Option<WebhookRegistration> {
        let handlers = self.handlers.read().await;
        handlers.get(path).cloned()
    }

    /// List all registered webhook paths
    pub async fn list_registered_paths(&self) -> Vec<String> {
        let handlers = self.handlers.read().await;
        handlers.keys().cloned().collect()
    }
}

impl Default for WebhookManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Utility functions for webhook handlers
pub mod utils {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    /// Compute HMAC-SHA256 signature
    ///
    /// # Arguments
    /// * `secret` - Secret key
    /// * `message` - Message to sign
    ///
    /// # Returns
    /// Hex-encoded signature
    pub fn compute_hmac_sha256(secret: &str, message: &[u8]) -> String {
        let mut mac =
            HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
        mac.update(message);
        let result = mac.finalize();
        hex::encode(result.into_bytes())
    }

    /// Verify HMAC-SHA256 signature
    ///
    /// # Arguments
    /// * `secret` - Secret key
    /// * `message` - Message that was signed
    /// * `signature` - Expected signature (hex-encoded)
    ///
    /// # Returns
    /// True if signature is valid
    pub fn verify_hmac_sha256(secret: &str, message: &[u8], signature: &str) -> bool {
        let computed = compute_hmac_sha256(secret, message);
        computed.eq_ignore_ascii_case(signature)
    }

    /// Decode base64 string
    pub fn decode_base64(input: &str) -> Result<Vec<u8>, base64::DecodeError> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.decode(input)
    }

    /// Encode bytes to base64
    pub fn encode_base64(input: &[u8]) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hmac_sha256() {
        let secret = "test_secret";
        let message = b"test message";
        let signature = utils::compute_hmac_sha256(secret, message);
        assert!(utils::verify_hmac_sha256(secret, message, &signature));
        assert!(!utils::verify_hmac_sha256(
            secret,
            message,
            "invalid_signature"
        ));
    }

    #[test]
    fn test_signature_verification_enum() {
        assert_eq!(SignatureVerification::Valid, SignatureVerification::Valid);
        assert_ne!(SignatureVerification::Valid, SignatureVerification::Invalid);
    }
}
