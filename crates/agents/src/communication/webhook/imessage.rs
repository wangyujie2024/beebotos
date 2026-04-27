//! iMessage Webhook Handler
//!
//! Handles incoming webhooks from iMessage via BlueBubbles server.
//! Supports message events, typing indicators, and delivery receipts.

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::communication::webhook::{
    SignatureVerification, WebhookConfig, WebhookEvent, WebhookEventType, WebhookHandler,
};
use crate::communication::{Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

/// BlueBubbles webhook payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueBubblesWebhookPayload {
    /// Event type
    #[serde(rename = "type")]
    pub event_type: String,
    /// Event data
    pub data: serde_json::Value,
}

/// BlueBubbles message data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueBubblesMessage {
    /// Message GUID
    pub guid: String,
    /// Chat GUID
    #[serde(rename = "chatGuid")]
    pub chat_guid: String,
    /// Sender handle (phone/email)
    pub handle: Option<String>,
    /// Is from me
    #[serde(rename = "isFromMe")]
    pub is_from_me: bool,
    /// Message text
    pub text: Option<String>,
    /// Message date (Unix timestamp in milliseconds)
    pub date: i64,
    /// Attachments
    pub attachments: Option<Vec<BlueBubblesAttachment>>,
    /// Associated message GUID (for tapbacks/reactions)
    #[serde(rename = "associatedMessageGuid")]
    pub associated_message_guid: Option<String>,
    /// Associated message type (for tapbacks)
    #[serde(rename = "associatedMessageType")]
    pub associated_message_type: Option<i32>,
    /// Thread originator GUID (for replies)
    #[serde(rename = "threadOriginatorGuid")]
    pub thread_originator_guid: Option<String>,
    /// Has been delivered
    #[serde(rename = "dateDelivered")]
    pub date_delivered: Option<i64>,
    /// Has been read
    #[serde(rename = "dateRead")]
    pub date_read: Option<i64>,
    /// Is delivered
    #[serde(default, rename = "isDelivered")]
    pub is_delivered: bool,
    /// Is read
    #[serde(default, rename = "isRead")]
    pub is_read: bool,
}

/// BlueBubbles attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueBubblesAttachment {
    /// Attachment GUID
    pub guid: String,
    /// Transfer name (filename)
    #[serde(rename = "transferName")]
    pub transfer_name: String,
    /// MIME type
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    /// File size
    #[serde(rename = "totalBytes")]
    pub total_bytes: i64,
    /// Is downloaded
    #[serde(rename = "isDownloaded")]
    pub is_downloaded: bool,
}

/// BlueBubbles chat data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueBubblesChat {
    /// Chat GUID
    pub guid: String,
    /// Display name (for group chats)
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    /// Participants
    pub participants: Vec<String>,
    /// Chat style (0 = individual, 1 = group)
    pub style: i32,
}

/// BlueBubbles typing indicator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueBubblesTypingIndicator {
    /// Chat GUID
    #[serde(rename = "chatGuid")]
    pub chat_guid: String,
    /// Display name
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    /// Is typing
    #[serde(rename = "isTyping")]
    pub is_typing: bool,
}

/// Tapback type mapping
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapbackType {
    Love = 2000,
    Like = 2001,
    Dislike = 2002,
    Laugh = 2003,
    Emphasize = 2004,
    Question = 2005,
}

impl TapbackType {
    /// Get tapback type from integer
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            2000 => Some(Self::Love),
            2001 => Some(Self::Like),
            2002 => Some(Self::Dislike),
            2003 => Some(Self::Laugh),
            2004 => Some(Self::Emphasize),
            2005 => Some(Self::Question),
            _ => None,
        }
    }

    /// Get emoji representation
    pub fn to_emoji(&self) -> &'static str {
        match self {
            Self::Love => "❤️",
            Self::Like => "👍",
            Self::Dislike => "👎",
            Self::Laugh => "😂",
            Self::Emphasize => "‼️",
            Self::Question => "❓",
        }
    }
}

/// iMessage webhook handler
#[allow(dead_code)]
pub struct IMessageWebhookHandler {
    config: WebhookConfig,
    /// BlueBubbles server password (optional)
    password: Option<String>,
}

impl IMessageWebhookHandler {
    /// Create a new iMessage webhook handler
    pub fn new(config: WebhookConfig) -> Self {
        let password = config.secret.clone();
        Self { config, password }
    }

    /// Parse message event
    fn parse_message(&self, data: &serde_json::Value) -> Result<WebhookEvent> {
        let msg: BlueBubblesMessage = serde_json::from_value(data.clone())
            .map_err(|e| AgentError::platform(format!("Failed to parse iMessage: {}", e)))?;

        // Skip messages from self
        if msg.is_from_me {
            return Ok(WebhookEvent {
                event_type: WebhookEventType::System,
                platform: PlatformType::IMessage,
                event_id: msg.guid.clone(),
                timestamp: chrono::DateTime::from_timestamp_millis(msg.date)
                    .unwrap_or_else(|| chrono::Utc::now()),
                payload: data.clone(),
                message: None,
                metadata: {
                    let mut m = HashMap::new();
                    m.insert("from_me".to_string(), "true".to_string());
                    m
                },
            });
        }

        // Check if it's a tapback (reaction)
        if let (Some(assoc_guid), Some(assoc_type)) =
            (&msg.associated_message_guid, msg.associated_message_type)
        {
            return self.parse_tapback(&msg, assoc_guid, assoc_type, data);
        }

        let message_type = if let Some(attachments) = &msg.attachments {
            if attachments.is_empty() {
                MessageType::Text
            } else {
                let mime = &attachments[0].mime_type;
                if mime.starts_with("image/") {
                    MessageType::Image
                } else if mime.starts_with("video/") {
                    MessageType::Video
                } else if mime.starts_with("audio/") {
                    MessageType::Voice
                } else {
                    MessageType::File
                }
            }
        } else {
            MessageType::Text
        };

        let mut metadata = HashMap::new();
        metadata.insert("chat_guid".to_string(), msg.chat_guid.clone());
        if let Some(handle) = &msg.handle {
            metadata.insert("sender_handle".to_string(), handle.clone());
        }
        if let Some(attachments) = &msg.attachments {
            metadata.insert(
                "attachment_count".to_string(),
                attachments.len().to_string(),
            );
            if let Some(first) = attachments.first() {
                metadata.insert("attachment_guid".to_string(), first.guid.clone());
            }
        }
        metadata.insert("is_delivered".to_string(), msg.is_delivered.to_string());
        metadata.insert("is_read".to_string(), msg.is_read.to_string());

        let message = Message {
            id: Uuid::parse_str(&msg.guid).unwrap_or_else(|_| uuid::Uuid::new_v4()),
            thread_id: Uuid::parse_str(&msg.chat_guid).unwrap_or_else(|_| uuid::Uuid::new_v4()),
            platform: PlatformType::IMessage,
            content: msg.text.clone().unwrap_or_default(),
            message_type,
            timestamp: chrono::DateTime::from_timestamp_millis(msg.date)
                .unwrap_or_else(|| chrono::Utc::now()),
            metadata,
        };

        Ok(WebhookEvent {
            event_type: WebhookEventType::MessageReceived,
            platform: PlatformType::IMessage,
            event_id: msg.guid.clone(),
            timestamp: chrono::DateTime::from_timestamp_millis(msg.date)
                .unwrap_or_else(|| chrono::Utc::now()),
            payload: data.clone(),
            message: Some(message),
            metadata: HashMap::new(),
        })
    }

    /// Parse tapback (reaction) event
    fn parse_tapback(
        &self,
        msg: &BlueBubblesMessage,
        original_guid: &str,
        tapback_type: i32,
        data: &serde_json::Value,
    ) -> Result<WebhookEvent> {
        let tapback = TapbackType::from_i32(tapback_type.abs()).ok_or_else(|| {
            AgentError::platform(format!("Unknown tapback type: {}", tapback_type))
        })?;

        let is_add = tapback_type > 0;
        let emoji = tapback.to_emoji();

        let mut metadata = HashMap::new();
        metadata.insert("chat_guid".to_string(), msg.chat_guid.clone());
        metadata.insert(
            "original_message_guid".to_string(),
            original_guid.to_string(),
        );
        metadata.insert("tapback_type".to_string(), tapback_type.to_string());
        metadata.insert("tapback_emoji".to_string(), emoji.to_string());
        metadata.insert("is_add".to_string(), is_add.to_string());

        let message = Message {
            id: Uuid::parse_str(&msg.guid).unwrap_or_else(|_| uuid::Uuid::new_v4()),
            thread_id: Uuid::parse_str(&msg.chat_guid).unwrap_or_else(|_| uuid::Uuid::new_v4()),
            platform: PlatformType::IMessage,
            content: if is_add {
                format!("[Reaction: {}]", emoji)
            } else {
                "[Removed reaction]".to_string()
            },
            message_type: MessageType::System,
            timestamp: chrono::DateTime::from_timestamp_millis(msg.date)
                .unwrap_or_else(|| chrono::Utc::now()),
            metadata: metadata.clone(),
        };

        Ok(WebhookEvent {
            event_type: WebhookEventType::MessageReceived,
            platform: PlatformType::IMessage,
            event_id: msg.guid.clone(),
            timestamp: chrono::DateTime::from_timestamp_millis(msg.date)
                .unwrap_or_else(|| chrono::Utc::now()),
            payload: data.clone(),
            message: Some(message),
            metadata,
        })
    }

    /// Parse message update event
    fn parse_message_update(&self, data: &serde_json::Value) -> Result<WebhookEvent> {
        // Message updates include delivery/read receipts
        let msg: BlueBubblesMessage = serde_json::from_value(data.clone())
            .map_err(|e| AgentError::platform(format!("Failed to parse message update: {}", e)))?;

        let event_type = if msg.is_read {
            WebhookEventType::MessageEdited // Using as proxy for read receipt
        } else if msg.is_delivered {
            WebhookEventType::System // Delivery receipt
        } else {
            WebhookEventType::MessageEdited
        };

        let mut metadata = HashMap::new();
        metadata.insert("chat_guid".to_string(), msg.chat_guid.clone());
        metadata.insert("is_delivered".to_string(), msg.is_delivered.to_string());
        metadata.insert("is_read".to_string(), msg.is_read.to_string());

        Ok(WebhookEvent {
            event_type,
            platform: PlatformType::IMessage,
            event_id: msg.guid.clone(),
            timestamp: chrono::DateTime::from_timestamp_millis(msg.date)
                .unwrap_or_else(|| chrono::Utc::now()),
            payload: data.clone(),
            message: None,
            metadata,
        })
    }

    /// Parse typing indicator event
    fn parse_typing_indicator(&self, data: &serde_json::Value) -> Result<WebhookEvent> {
        let indicator: BlueBubblesTypingIndicator =
            serde_json::from_value(data.clone()).map_err(|e| {
                AgentError::platform(format!("Failed to parse typing indicator: {}", e))
            })?;

        let mut metadata = HashMap::new();
        metadata.insert("chat_guid".to_string(), indicator.chat_guid.clone());
        metadata.insert("is_typing".to_string(), indicator.is_typing.to_string());
        if let Some(name) = &indicator.display_name {
            metadata.insert("display_name".to_string(), name.clone());
        }

        Ok(WebhookEvent {
            event_type: WebhookEventType::System,
            platform: PlatformType::IMessage,
            event_id: format!("typing_{}", chrono::Utc::now().timestamp()),
            timestamp: chrono::Utc::now(),
            payload: data.clone(),
            message: None,
            metadata,
        })
    }
}

#[async_trait]
impl WebhookHandler for IMessageWebhookHandler {
    fn platform_type(&self) -> PlatformType {
        PlatformType::IMessage
    }

    async fn verify_signature(
        &self,
        _body: &[u8],
        _signature: Option<&str>,
        _timestamp: Option<&str>,
    ) -> Result<SignatureVerification> {
        // BlueBubbles doesn't use signature verification by default
        // If password is configured, it should be checked in the Authorization header
        Ok(SignatureVerification::Skipped)
    }

    async fn parse_payload(&self, body: &[u8]) -> Result<Vec<WebhookEvent>> {
        let payload: BlueBubblesWebhookPayload = serde_json::from_slice(body).map_err(|e| {
            AgentError::platform(format!("Failed to parse BlueBubbles webhook: {}", e))
        })?;

        debug!("Received BlueBubbles webhook: {}", payload.event_type);

        let event = match payload.event_type.as_str() {
            "new-message" => self.parse_message(&payload.data)?,
            "message-updated" => self.parse_message_update(&payload.data)?,
            "message-deleted" => {
                let mut event = self.parse_message(&payload.data)?;
                event.event_type = WebhookEventType::MessageDeleted;
                event
            }
            "typing" => self.parse_typing_indicator(&payload.data)?,
            "participant-removed" | "participant-added" => {
                // Group membership changes
                WebhookEvent {
                    event_type: WebhookEventType::System,
                    platform: PlatformType::IMessage,
                    event_id: format!("member_{}", chrono::Utc::now().timestamp()),
                    timestamp: chrono::Utc::now(),
                    payload: serde_json::to_value(&payload).map_err(|e| {
                        AgentError::platform(format!("Failed to serialize payload: {}", e))
                    })?,
                    message: None,
                    metadata: {
                        let mut m = HashMap::new();
                        m.insert("event_type".to_string(), payload.event_type.clone());
                        m
                    },
                }
            }
            _ => {
                warn!("Unknown BlueBubbles event type: {}", payload.event_type);
                WebhookEvent {
                    event_type: WebhookEventType::Unknown,
                    platform: PlatformType::IMessage,
                    event_id: format!("unknown_{}", chrono::Utc::now().timestamp()),
                    timestamp: chrono::Utc::now(),
                    payload: serde_json::to_value(&payload).map_err(|e| {
                        AgentError::platform(format!("Failed to serialize payload: {}", e))
                    })?,
                    message: None,
                    metadata: HashMap::new(),
                }
            }
        };

        Ok(vec![event])
    }

    async fn handle_event(&self, event: WebhookEvent) -> Result<()> {
        info!(
            "Handling iMessage event: {:?} - {}",
            event.event_type, event.event_id
        );
        Ok(())
    }

    fn get_config(&self) -> &WebhookConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_text_message() {
        let config = WebhookConfig {
            platform: PlatformType::IMessage,
            endpoint_path: "/webhook/imessage".to_string(),
            ..Default::default()
        };
        let handler = IMessageWebhookHandler::new(config);

        let data = serde_json::json!({
            "guid": "msg123",
            "chatGuid": "chat456",
            "handle": "+1234567890",
            "isFromMe": false,
            "text": "Hello iMessage!",
            "date": 1234567890000i64,
            "attachments": [],
        });

        let event = handler.parse_message(&data).unwrap();
        assert_eq!(event.event_type, WebhookEventType::MessageReceived);
        assert!(event.message.is_some());

        let message = event.message.unwrap();
        assert_eq!(message.content, "Hello iMessage!");
        assert_eq!(message.message_type, MessageType::Text);
        // sender info is stored in metadata
        assert_eq!(
            message.metadata.get("sender_handle"),
            Some(&"+1234567890".to_string())
        );
    }

    #[test]
    fn test_parse_image_message() {
        let config = WebhookConfig {
            platform: PlatformType::IMessage,
            endpoint_path: "/webhook/imessage".to_string(),
            ..Default::default()
        };
        let handler = IMessageWebhookHandler::new(config);

        let data = serde_json::json!({
            "guid": "msg789",
            "chatGuid": "chat456",
            "handle": "user@example.com",
            "isFromMe": false,
            "text": "Check out this photo",
            "date": 1234567890000i64,
            "attachments": [{
                "guid": "att123",
                "transferName": "photo.jpg",
                "mimeType": "image/jpeg",
                "totalBytes": 1024000,
                "isDownloaded": true
            }],
        });

        let event = handler.parse_message(&data).unwrap();
        let message = event.message.unwrap();
        assert_eq!(message.message_type, MessageType::Image);
        assert_eq!(message.metadata.get("attachment_count").unwrap(), "1");
    }

    #[test]
    fn test_parse_tapback() {
        let config = WebhookConfig {
            platform: PlatformType::IMessage,
            endpoint_path: "/webhook/imessage".to_string(),
            ..Default::default()
        };
        let handler = IMessageWebhookHandler::new(config);

        let msg = BlueBubblesMessage {
            guid: "tapback123".to_string(),
            chat_guid: "chat456".to_string(),
            handle: Some("+1234567890".to_string()),
            is_from_me: false,
            text: None,
            date: 1234567890000,
            attachments: None,
            associated_message_guid: Some("original_msg_456".to_string()),
            associated_message_type: Some(2000), // Love
            thread_originator_guid: None,
            date_delivered: None,
            date_read: None,
            is_delivered: true,
            is_read: false,
        };

        let data = serde_json::to_value(&msg).unwrap();
        let event = handler
            .parse_tapback(&msg, "original_msg_456", 2000, &data)
            .unwrap();

        assert!(event.message.is_some());
        let message = event.message.unwrap();
        // Reaction messages are mapped to System type
        assert_eq!(message.message_type, MessageType::System);
        assert_eq!(message.content, "[Reaction: ❤️]");
        assert_eq!(message.metadata.get("tapback_emoji").unwrap(), "❤️");
    }

    #[test]
    fn test_skip_from_me() {
        let config = WebhookConfig {
            platform: PlatformType::IMessage,
            endpoint_path: "/webhook/imessage".to_string(),
            ..Default::default()
        };
        let handler = IMessageWebhookHandler::new(config);

        let data = serde_json::json!({
            "guid": "msg999",
            "chatGuid": "chat456",
            "handle": "+1234567890",
            "isFromMe": true,
            "text": "My own message",
            "date": 1234567890000i64,
        });

        let event = handler.parse_message(&data).unwrap();
        assert_eq!(event.event_type, WebhookEventType::System);
        assert!(event.message.is_none());
    }
}
