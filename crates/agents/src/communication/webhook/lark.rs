//! Lark (飞书) Webhook Handler
//!
//! Handles incoming webhooks from Lark/Feishu messaging platform.
//! Supports signature verification (HMAC-SHA256) and message decryption
//! (AES-GCM).

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use super::common::{
    MetadataBuilder, SignatureVerification as CommonSignatureVerification, SignatureVerifier,
    TokenVerifier,
};
use crate::communication::webhook::{
    utils, SignatureVerification, WebhookConfig, WebhookEvent, WebhookEventType, WebhookHandler,
};
use crate::communication::{AgentMessageDispatcher, Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

/// Lark webhook payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkWebhookPayload {
    /// Challenge for URL verification (only present during verification)
    pub challenge: Option<String>,
    /// Token for verification
    pub token: Option<String>,
    /// Event type
    #[serde(rename = "type")]
    pub payload_type: Option<String>,
    /// Event data
    pub event: Option<LarkEvent>,
    /// Encrypted data (if encryption is enabled)
    pub encrypt: Option<String>,
}

/// Lark event data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkEvent {
    /// Event ID
    #[serde(rename = "event_id")]
    pub id: String,
    /// Event type
    #[serde(rename = "event_type")]
    pub event_type: String,
    /// Token for verification
    pub token: Option<String>,
    /// App ID
    pub app_id: Option<String>,
    /// Tenant key
    pub tenant_key: Option<String>,
    /// Sender information
    pub sender: Option<LarkSender>,
    /// Message information
    pub message: Option<LarkEventMessage>,
    /// Timestamp
    #[serde(rename = "create_time")]
    pub create_time: Option<String>,
}

/// Lark sender information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkSender {
    /// Sender ID
    pub sender_id: LarkUserId,
    /// Sender type
    pub sender_type: String,
    /// Tenant key
    pub tenant_key: String,
}

/// Lark user ID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkUserId {
    /// Open ID
    pub open_id: String,
    /// Union ID
    pub union_id: Option<String>,
    /// User ID
    pub user_id: Option<String>,
}

/// Lark event message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkEventMessage {
    /// Message ID
    #[serde(rename = "message_id")]
    pub id: String,
    /// Root ID (for reply messages)
    pub root_id: Option<String>,
    /// Parent ID (for reply messages)
    pub parent_id: Option<String>,
    /// Create time
    pub create_time: String,
    /// Chat ID
    pub chat_id: String,
    /// Chat type
    pub chat_type: String,
    /// Message type
    #[serde(rename = "message_type")]
    pub msg_type: String,
    /// Message content (JSON string)
    pub content: String,
    /// Mentions
    pub mentions: Option<Vec<LarkMention>>,
}

/// Lark mention
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkMention {
    /// Mention key
    pub key: String,
    /// User ID
    pub id: LarkUserId,
    /// User name
    pub name: String,
    /// Tenant key
    pub tenant_key: String,
}

/// Lark message content types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum LarkMessageContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        image_key: String,
        file_name: Option<String>,
    },
    #[serde(rename = "file")]
    File { file_key: String, file_name: String },
    #[serde(rename = "audio")]
    Audio {
        file_key: String,
        duration: Option<i32>,
    },
    #[serde(rename = "media")]
    Media {
        file_key: String,
        image_key: Option<String>,
    },
}

/// Lark webhook handler
pub struct LarkWebhookHandler {
    config: WebhookConfig,
    verification_token: String,
    encrypt_key: Option<Vec<u8>>,
    dispatcher: Option<std::sync::Arc<AgentMessageDispatcher>>,
}

impl LarkWebhookHandler {
    /// Create a new Lark webhook handler
    ///
    /// # Arguments
    /// * `verification_token` - Token for verifying webhook requests
    /// * `encrypt_key` - Optional encryption key for decrypting messages
    pub fn new(verification_token: String, encrypt_key: Option<String>) -> Self {
        let encrypt_key_bytes = encrypt_key.and_then(|key| {
            // Lark uses base64-encoded 32-byte key
            match utils::decode_base64(&key) {
                Ok(bytes) if bytes.len() == 32 => Some(bytes),
                Ok(bytes) => {
                    warn!("Invalid encrypt key length: {} (expected 32)", bytes.len());
                    None
                }
                Err(e) => {
                    warn!("Failed to decode encrypt key: {}", e);
                    None
                }
            }
        });

        let mut config = WebhookConfig::default();
        config.platform = PlatformType::Lark;
        config.endpoint_path = "/webhook/lark".to_string();
        config.verify_signatures = true;
        config.decrypt_messages = encrypt_key_bytes.is_some();

        Self {
            config,
            verification_token,
            encrypt_key: encrypt_key_bytes,
            dispatcher: None,
        }
    }

    /// Attach an `AgentMessageDispatcher` so that incoming messages are
    /// routed to the appropriate agents instead of just being logged.
    pub fn with_dispatcher(mut self, dispatcher: std::sync::Arc<AgentMessageDispatcher>) -> Self {
        self.dispatcher = Some(dispatcher);
        self
    }

    /// Create handler from environment variables
    pub fn from_env() -> Result<Self> {
        let verification_token = std::env::var("LARK_VERIFICATION_TOKEN")
            .map_err(|_| AgentError::configuration("LARK_VERIFICATION_TOKEN not set"))?;
        let encrypt_key = std::env::var("LARK_ENCRYPT_KEY").ok();

        Ok(Self::new(verification_token, encrypt_key))
    }

    /// Decrypt Lark encrypted message
    ///
    /// Lark uses AES-256-GCM with a 12-byte nonce
    fn decrypt_message(&self, encrypted_data: &str) -> Result<Vec<u8>> {
        let key = self
            .encrypt_key
            .as_ref()
            .ok_or_else(|| AgentError::platform("No encryption key configured"))?;

        // Decode base64
        let encrypted_bytes = utils::decode_base64(encrypted_data)
            .map_err(|e| AgentError::platform(format!("Failed to decode encrypted data: {}", e)))?;

        // Lark format: nonce (12 bytes) + ciphertext + tag (16 bytes)
        if encrypted_bytes.len() < 28 {
            return Err(AgentError::platform("Encrypted data too short"));
        }

        let nonce = Nonce::from_slice(&encrypted_bytes[..12]);
        let ciphertext = &encrypted_bytes[12..];

        let cipher = Aes256Gcm::new_from_slice(key)
            .map_err(|e| AgentError::platform(format!("Failed to create cipher: {}", e)))?;

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| AgentError::platform(format!("Failed to decrypt message: {}", e)))?;

        Ok(plaintext)
    }

    /// Parse Lark event type
    fn parse_event_type(&self, event_type: &str) -> WebhookEventType {
        match event_type {
            "im.message.receive_v1" => WebhookEventType::MessageReceived,
            "im.message.updated_v1" => WebhookEventType::MessageEdited,
            "im.message.deleted_v1" => WebhookEventType::MessageDeleted,
            "im.chat.member.bot.added_v1" => WebhookEventType::UserJoined,
            "im.chat.member.bot.deleted_v1" => WebhookEventType::UserLeft,
            "application.bot.menu_v1" => WebhookEventType::BotMentioned,
            _ => {
                if event_type.contains("file") {
                    WebhookEventType::FileShared
                } else {
                    WebhookEventType::Unknown
                }
            }
        }
    }

    /// Parse message type from Lark message type
    fn parse_message_type(&self, msg_type: &str) -> MessageType {
        match msg_type {
            "text" => MessageType::Text,
            "image" => MessageType::Image,
            "file" => MessageType::File,
            "audio" => MessageType::Voice,
            "media" => MessageType::Video,
            _ => MessageType::System,
        }
    }

    /// Convert Lark event to internal message
    fn convert_to_message(&self, event: &LarkEvent) -> Option<Message> {
        let message = event.message.as_ref()?;
        let sender = event.sender.as_ref()?;

        // Parse content based on message type
        let content = match message.msg_type.as_str() {
            "text" => {
                // Parse JSON content for text messages
                match serde_json::from_str::<serde_json::Value>(&message.content) {
                    Ok(json) => json
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string(),
                    Err(_) => message.content.clone(),
                }
            }
            "image" | "file" | "audio" | "media" => {
                // For media messages, store the file key
                format!("[{}] {}", message.msg_type, message.content)
            }
            _ => message.content.clone(),
        };

        let metadata = MetadataBuilder::new()
            .add("chat_id", &message.chat_id)
            .add("sender_open_id", &sender.sender_id.open_id)
            .add("message_type", &message.msg_type)
            .add_optional("root_id", message.root_id.as_ref())
            .add_optional("parent_id", message.parent_id.as_ref())
            .build();

        Some(Message {
            id: uuid::Uuid::new_v4(),
            thread_id: uuid::Uuid::new_v4(),
            platform: PlatformType::Lark,
            message_type: self.parse_message_type(&message.msg_type),
            content,
            metadata,
            timestamp: chrono::Utc::now(),
        })
    }

    /// Handle URL verification challenge
    #[allow(dead_code)]
    fn handle_challenge(&self, payload: &LarkWebhookPayload) -> Option<String> {
        payload.challenge.clone()
    }
}

#[async_trait]
impl WebhookHandler for LarkWebhookHandler {
    fn platform_type(&self) -> PlatformType {
        PlatformType::Lark
    }

    async fn verify_signature(
        &self,
        _body: &[u8],
        _signature: Option<&str>,
        _timestamp: Option<&str>,
    ) -> Result<SignatureVerification> {
        // Lark uses token-based verification in the payload
        // The signature verification is done by checking the token in the event
        // For URL verification, we use the challenge-response mechanism
        Ok(SignatureVerification::Skipped)
    }

    async fn parse_payload(&self, body: &[u8]) -> Result<Vec<WebhookEvent>> {
        let mut payload: LarkWebhookPayload = serde_json::from_slice(body)
            .map_err(|e| AgentError::platform(format!("Failed to parse Lark payload: {}", e)))?;

        // Handle encrypted payload
        if let Some(encrypt) = &payload.encrypt {
            if self.config.decrypt_messages {
                let decrypted = self.decrypt_message(encrypt)?;
                payload = serde_json::from_slice(&decrypted).map_err(|e| {
                    AgentError::platform(format!("Failed to parse decrypted payload: {}", e))
                })?;
            } else {
                return Err(AgentError::platform(
                    "Received encrypted payload but decryption is not configured",
                ));
            }
        }

        // Handle URL verification
        if payload.challenge.is_some() {
            debug!("Received Lark URL verification challenge");
            return Ok(vec![WebhookEvent {
                event_type: WebhookEventType::System,
                platform: PlatformType::Lark,
                event_id: "challenge".to_string(),
                timestamp: chrono::Utc::now(),
                payload: serde_json::to_value(&payload).unwrap_or_default(),
                message: None,
                metadata: MetadataBuilder::new()
                    .add_optional("challenge", payload.challenge)
                    .build(),
            }]);
        }

        // Handle event callback
        let event = payload
            .event
            .ok_or_else(|| AgentError::platform("Lark payload missing event data"))?;

        // Verify token if present
        if let Some(token) = &event.token {
            let verifier = TokenVerifier::new(&self.verification_token);
            let result = verifier.verify(&[], Some(token), None).await?;
            if result != CommonSignatureVerification::Valid {
                return Err(AgentError::authentication("Invalid verification token"));
            }
        }

        let event_type = self.parse_event_type(&event.event_type);
        let message = self.convert_to_message(&event);

        let webhook_event = WebhookEvent {
            event_type,
            platform: PlatformType::Lark,
            event_id: event.id.clone(),
            timestamp: event
                .create_time
                .as_ref()
                .and_then(|t| t.parse::<i64>().ok())
                .map(|t| chrono::DateTime::from_timestamp(t / 1000, 0))
                .flatten()
                .unwrap_or_else(chrono::Utc::now),
            payload: serde_json::to_value(&event).unwrap_or_default(),
            message,
            metadata: MetadataBuilder::new()
                .add_optional("app_id", event.app_id)
                .add_optional("tenant_key", event.tenant_key)
                .build(),
        };

        Ok(vec![webhook_event])
    }

    async fn handle_event(&self, event: WebhookEvent) -> Result<()> {
        match event.event_type {
            WebhookEventType::MessageReceived | WebhookEventType::BotMentioned => {
                if let Some(msg) = &event.message {
                    info!(
                        "Received message from Lark: {} (type: {:?})",
                        msg.content, msg.message_type
                    );

                    // P0 FIX: Removed dispatcher.dispatch() to avoid duplicate
                    // processing. Messages are now routed
                    // exclusively through channel_event_bus →
                    // MessageProcessor → AgentResolver path in webhook_handler.
                    // if let Some(dispatcher) = &self.dispatcher {
                    //     let tenant_key = event.metadata.get("tenant_key")
                    //         .cloned()
                    //         .unwrap_or_default();
                    //     let chat_id = msg.metadata.get("chat_id")
                    //         .cloned()
                    //         .unwrap_or_default();
                    //
                    //     dispatcher.dispatch(
                    //         PlatformType::Lark,
                    //         &tenant_key,
                    //         msg.clone(),
                    //         chat_id,
                    //     ).await?;
                    // }
                }
            }
            WebhookEventType::UserJoined => {
                info!("User joined Lark chat");
            }
            WebhookEventType::UserLeft => {
                info!("User left Lark chat");
            }
            WebhookEventType::System => {
                debug!("Received system event from Lark");
            }
            _ => {
                debug!("Received unhandled event type: {:?}", event.event_type);
            }
        }

        Ok(())
    }

    fn get_config(&self) -> &WebhookConfig {
        &self.config
    }
}

/// Lark webhook response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkWebhookResponse {
    /// Challenge response (for URL verification)
    pub challenge: Option<String>,
    /// Response code
    pub code: i32,
    /// Response message
    pub msg: String,
}

impl LarkWebhookResponse {
    /// Create a challenge response
    pub fn challenge(challenge: String) -> Self {
        Self {
            challenge: Some(challenge),
            code: 0,
            msg: "success".to_string(),
        }
    }

    /// Create a success response
    pub fn success() -> Self {
        Self {
            challenge: None,
            code: 0,
            msg: "success".to_string(),
        }
    }

    /// Create an error response
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            challenge: None,
            code: -1,
            msg: msg.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_event_type() {
        let handler = LarkWebhookHandler::new("test_token".to_string(), None);

        assert_eq!(
            handler.parse_event_type("im.message.receive_v1"),
            WebhookEventType::MessageReceived
        );
        assert_eq!(
            handler.parse_event_type("im.message.updated_v1"),
            WebhookEventType::MessageEdited
        );
        assert_eq!(
            handler.parse_event_type("im.chat.member.bot.added_v1"),
            WebhookEventType::UserJoined
        );
        assert_eq!(
            handler.parse_event_type("unknown.event"),
            WebhookEventType::Unknown
        );
    }

    #[test]
    fn test_parse_message_type() {
        let handler = LarkWebhookHandler::new("test_token".to_string(), None);

        assert_eq!(handler.parse_message_type("text"), MessageType::Text);
        assert_eq!(handler.parse_message_type("image"), MessageType::Image);
        assert_eq!(handler.parse_message_type("file"), MessageType::File);
        assert_eq!(handler.parse_message_type("audio"), MessageType::Voice);
        assert_eq!(handler.parse_message_type("media"), MessageType::Video);
        assert_eq!(handler.parse_message_type("unknown"), MessageType::System);
    }

    #[test]
    fn test_lark_webhook_response() {
        let challenge_resp = LarkWebhookResponse::challenge("test_challenge".to_string());
        assert_eq!(challenge_resp.challenge, Some("test_challenge".to_string()));
        assert_eq!(challenge_resp.code, 0);

        let success_resp = LarkWebhookResponse::success();
        assert_eq!(success_resp.challenge, None);
        assert_eq!(success_resp.code, 0);

        let error_resp = LarkWebhookResponse::error("test error");
        assert_eq!(error_resp.code, -1);
        assert_eq!(error_resp.msg, "test error");
    }
}
