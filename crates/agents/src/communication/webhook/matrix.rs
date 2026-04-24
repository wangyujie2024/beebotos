//! Matrix Webhook Handler
//!
//! Handles incoming webhooks from Matrix homeserver via Application Service
//! API. Supports message events, membership events, and room state events.

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};
use uuid::Uuid;

use crate::communication::webhook::common::MetadataBuilder;
use crate::communication::webhook::{
    SignatureVerification, WebhookConfig, WebhookEvent, WebhookEventType, WebhookHandler,
};
use crate::communication::{Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

/// Matrix transaction payload (Application Service API)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixTransaction {
    /// Transaction ID
    #[serde(rename = "txnId")]
    pub txn_id: String,
    /// Events
    pub events: Vec<MatrixEvent>,
}

/// Matrix event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixEvent {
    /// Event type
    #[serde(rename = "type")]
    pub event_type: String,
    /// Event content
    pub content: serde_json::Value,
    /// Room ID
    #[serde(rename = "room_id")]
    pub room_id: String,
    /// Sender
    pub sender: String,
    /// Event ID
    #[serde(rename = "event_id")]
    pub event_id: String,
    /// Origin server timestamp
    #[serde(rename = "origin_server_ts")]
    pub origin_server_ts: i64,
    /// Unsigned data
    pub unsigned: Option<serde_json::Value>,
    /// State key (for state events)
    #[serde(rename = "state_key")]
    pub state_key: Option<String>,
}

/// Matrix message content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixMessageContent {
    /// Message type
    #[serde(rename = "msgtype")]
    pub msg_type: String,
    /// Body (plain text)
    pub body: String,
    /// Formatted body (HTML)
    #[serde(rename = "formatted_body")]
    pub formatted_body: Option<String>,
    /// Format
    pub format: Option<String>,
    /// URL (for media)
    pub url: Option<String>,
    /// File info
    pub info: Option<serde_json::Value>,
    /// Filename
    pub filename: Option<String>,
}

/// Matrix membership content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixMembershipContent {
    /// Membership state
    pub membership: String,
    /// Display name
    pub displayname: Option<String>,
    /// Avatar URL
    pub avatar_url: Option<String>,
}

/// Matrix room member
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixRoomMember {
    /// User ID
    pub user_id: String,
    /// Display name
    pub display_name: Option<String>,
    /// Avatar URL
    pub avatar_url: Option<String>,
}

/// Matrix webhook handler
#[allow(dead_code)]
pub struct MatrixWebhookHandler {
    config: WebhookConfig,
    /// Homeserver token for verification
    hs_token: Option<String>,
}

impl MatrixWebhookHandler {
    /// Create a new Matrix webhook handler
    pub fn new(config: WebhookConfig) -> Self {
        let hs_token = config.secret.clone();
        Self { config, hs_token }
    }

    /// Parse message event
    fn parse_message(&self, event: &MatrixEvent) -> Result<WebhookEvent> {
        let content: MatrixMessageContent = serde_json::from_value(event.content.clone())
            .map_err(|e| AgentError::platform(format!("Failed to parse Matrix message: {}", e)))?;

        let message_type = match content.msg_type.as_str() {
            "m.text" => MessageType::Text,
            "m.image" => MessageType::Image,
            "m.video" => MessageType::Video,
            "m.audio" => MessageType::Voice,
            "m.file" => MessageType::File,
            "m.location" => MessageType::Text,
            "m.sticker" => MessageType::Image,
            "m.notice" => MessageType::System,
            "m.emote" => MessageType::Text,
            _ => MessageType::System,
        };

        let metadata = MetadataBuilder::new()
            .add("room_id", &event.room_id)
            .add("sender_id", &event.sender)
            .add_optional("media_url", content.url.as_ref())
            .add_optional("filename", content.filename.as_ref())
            .add_optional("media_info", content.info.as_ref().map(|v| v.to_string()))
            .build();

        let message = Message {
            id: Uuid::parse_str(&event.event_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
            thread_id: Uuid::parse_str(&event.room_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
            platform: PlatformType::Matrix,
            content: content.body.clone(),
            message_type,
            timestamp: chrono::DateTime::from_timestamp_millis(event.origin_server_ts)
                .unwrap_or_else(|| chrono::Utc::now()),
            metadata,
        };

        Ok(WebhookEvent {
            event_type: WebhookEventType::MessageReceived,
            platform: PlatformType::Matrix,
            event_id: event.event_id.clone(),
            timestamp: chrono::DateTime::from_timestamp_millis(event.origin_server_ts)
                .unwrap_or_else(|| chrono::Utc::now()),
            payload: serde_json::to_value(event)
                .map_err(|e| AgentError::platform(format!("JSON serialization error: {}", e)))?,
            message: Some(message),
            metadata: HashMap::new(),
        })
    }

    /// Parse membership event
    fn parse_membership(&self, event: &MatrixEvent) -> Result<WebhookEvent> {
        let content: MatrixMembershipContent = serde_json::from_value(event.content.clone())
            .map_err(|e| AgentError::platform(format!("Failed to parse membership: {}", e)))?;

        let event_type = match content.membership.as_str() {
            "join" => WebhookEventType::UserJoined,
            "leave" => WebhookEventType::UserLeft,
            "invite" => WebhookEventType::System,
            "ban" => WebhookEventType::System,
            _ => WebhookEventType::System,
        };

        let metadata = MetadataBuilder::new()
            .add("room_id", &event.room_id)
            .add("membership", &content.membership)
            .add_optional("display_name", content.displayname.as_ref())
            .build();

        Ok(WebhookEvent {
            event_type,
            platform: PlatformType::Matrix,
            event_id: event.event_id.clone(),
            timestamp: chrono::DateTime::from_timestamp_millis(event.origin_server_ts)
                .unwrap_or_else(|| chrono::Utc::now()),
            payload: serde_json::to_value(event)
                .map_err(|e| AgentError::platform(format!("JSON serialization error: {}", e)))?,
            message: None,
            metadata,
        })
    }

    /// Parse reaction event (m.reaction)
    fn parse_reaction(&self, event: &MatrixEvent) -> Result<WebhookEvent> {
        #[derive(Deserialize)]
        struct ReactionContent {
            #[serde(rename = "m.relates_to")]
            relates_to: RelatesTo,
        }
        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct RelatesTo {
            #[allow(dead_code)]
            rel_type: String,
            event_id: String,
            key: String,
        }

        let content: ReactionContent = serde_json::from_value(event.content.clone())
            .map_err(|e| AgentError::platform(format!("Failed to parse reaction: {}", e)))?;

        let metadata = MetadataBuilder::new()
            .add("room_id", &event.room_id)
            .add("target_event_id", &content.relates_to.event_id)
            .add("reaction_key", &content.relates_to.key)
            .build();

        let message = Message {
            id: Uuid::parse_str(&event.event_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
            thread_id: Uuid::parse_str(&event.room_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
            platform: PlatformType::Matrix,
            content: format!("[Reaction: {}]", content.relates_to.key),
            message_type: MessageType::Text,
            timestamp: chrono::DateTime::from_timestamp_millis(event.origin_server_ts)
                .unwrap_or_else(|| chrono::Utc::now()),
            metadata: metadata.clone(),
        };

        Ok(WebhookEvent {
            event_type: WebhookEventType::MessageReceived,
            platform: PlatformType::Matrix,
            event_id: event.event_id.clone(),
            timestamp: chrono::DateTime::from_timestamp_millis(event.origin_server_ts)
                .unwrap_or_else(|| chrono::Utc::now()),
            payload: serde_json::to_value(event)
                .map_err(|e| AgentError::platform(format!("JSON serialization error: {}", e)))?,
            message: Some(message),
            metadata,
        })
    }

    /// Extract reply-to message ID from Matrix fallback text
    #[allow(dead_code)]
    fn extract_reply_to(&self, _body: &str) -> Option<String> {
        // Matrix reply fallback format: "> <@user:server> message\n\nreply text"
        // The actual reply-to is in the m.relates_to field, but we don't have access
        // here This is a simplified version
        None
    }
}

#[async_trait]
impl WebhookHandler for MatrixWebhookHandler {
    fn platform_type(&self) -> PlatformType {
        PlatformType::Matrix
    }

    async fn verify_signature(
        &self,
        _body: &[u8],
        _signature: Option<&str>,
        _timestamp: Option<&str>,
    ) -> Result<SignatureVerification> {
        // Matrix Application Service API uses token-based auth via query parameter
        // The token should be verified in the HTTP handler layer
        Ok(SignatureVerification::Skipped)
    }

    async fn parse_payload(&self, body: &[u8]) -> Result<Vec<WebhookEvent>> {
        let transaction: MatrixTransaction = serde_json::from_slice(body).map_err(|e| {
            AgentError::platform(format!("Failed to parse Matrix transaction: {}", e))
        })?;

        debug!(
            "Received Matrix transaction: {} with {} events",
            transaction.txn_id,
            transaction.events.len()
        );

        let mut events = Vec::new();

        for event in &transaction.events {
            // Skip events from the bot itself
            if event.sender.contains(":beebot.") {
                continue;
            }

            let parsed = match event.event_type.as_str() {
                "m.room.message" => self.parse_message(event)?,
                "m.room.member" => self.parse_membership(event)?,
                "m.reaction" => self.parse_reaction(event)?,
                "m.room.redaction" => {
                    // Message deleted
                    let mut e = self.parse_message(event)?;
                    e.event_type = WebhookEventType::MessageDeleted;
                    e
                }
                _ => {
                    debug!("Skipping unknown Matrix event type: {}", event.event_type);
                    continue;
                }
            };

            events.push(parsed);
        }

        Ok(events)
    }

    async fn handle_event(&self, event: WebhookEvent) -> Result<()> {
        info!(
            "Handling Matrix event: {:?} - {}",
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
            platform: PlatformType::Matrix,
            endpoint_path: "/_matrix/app/v1/transactions".to_string(),
            ..Default::default()
        };
        let handler = MatrixWebhookHandler::new(config);

        let event = MatrixEvent {
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({
                "msgtype": "m.text",
                "body": "Hello Matrix!"
            }),
            room_id: "!test:example.com".to_string(),
            sender: "@user:example.com".to_string(),
            event_id: "$event123".to_string(),
            origin_server_ts: 1234567890000,
            unsigned: None,
            state_key: None,
        };

        let webhook_event = handler.parse_message(&event).unwrap();
        assert_eq!(webhook_event.event_type, WebhookEventType::MessageReceived);
        assert!(webhook_event.message.is_some());

        let message = webhook_event.message.unwrap();
        assert_eq!(message.content, "Hello Matrix!");
        assert_eq!(message.message_type, MessageType::Text);
        // sender info is stored in metadata
        assert_eq!(
            message.metadata.get("sender_id"),
            Some(&"@user:example.com".to_string())
        );
    }

    #[test]
    fn test_parse_image_message() {
        let config = WebhookConfig {
            platform: PlatformType::Matrix,
            endpoint_path: "/_matrix/app/v1/transactions".to_string(),
            ..Default::default()
        };
        let handler = MatrixWebhookHandler::new(config);

        let event = MatrixEvent {
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({
                "msgtype": "m.image",
                "body": "image.png",
                "url": "mxc://example.com/abc123"
            }),
            room_id: "!test:example.com".to_string(),
            sender: "@user:example.com".to_string(),
            event_id: "$event456".to_string(),
            origin_server_ts: 1234567890000,
            unsigned: None,
            state_key: None,
        };

        let webhook_event = handler.parse_message(&event).unwrap();
        let message = webhook_event.message.unwrap();
        assert_eq!(message.message_type, MessageType::Image);
        assert_eq!(
            message.metadata.get("media_url").unwrap(),
            "mxc://example.com/abc123"
        );
    }

    #[test]
    fn test_parse_membership_join() {
        let config = WebhookConfig {
            platform: PlatformType::Matrix,
            endpoint_path: "/_matrix/app/v1/transactions".to_string(),
            ..Default::default()
        };
        let handler = MatrixWebhookHandler::new(config);

        let event = MatrixEvent {
            event_type: "m.room.member".to_string(),
            content: serde_json::json!({
                "membership": "join",
                "displayname": "Test User"
            }),
            room_id: "!test:example.com".to_string(),
            sender: "@user:example.com".to_string(),
            event_id: "$event789".to_string(),
            origin_server_ts: 1234567890000,
            unsigned: None,
            state_key: Some("@user:example.com".to_string()),
        };

        let webhook_event = handler.parse_membership(&event).unwrap();
        assert_eq!(webhook_event.event_type, WebhookEventType::UserJoined);
        assert_eq!(
            webhook_event.metadata.get("display_name").unwrap(),
            "Test User"
        );
    }
}
