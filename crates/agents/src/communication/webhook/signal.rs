//! Signal Webhook Handler
//!
//! Handles incoming webhooks from Signal via signal-cli HTTP JSON-RPC + SSE.
//! Supports message events, receipt events, and sync events.

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::communication::webhook::{
    SignatureVerification, WebhookConfig, WebhookEvent, WebhookEventType, WebhookHandler,
};
use crate::communication::{Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

/// Signal webhook payload from signal-cli
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalWebhookPayload {
    /// JSON-RPC version
    pub jsonrpc: String,
    /// Method name
    pub method: String,
    /// Parameters
    pub params: SignalParams,
}

/// Signal parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalParams {
    /// Account (phone number)
    pub account: String,
    /// Envelope containing message data
    pub envelope: SignalEnvelope,
}

/// Signal envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalEnvelope {
    /// Source (sender phone number)
    pub source: Option<String>,
    /// Source UUID
    pub source_uuid: Option<String>,
    /// Source name (if in contacts)
    pub source_name: Option<String>,
    /// Source device
    pub source_device: Option<i32>,
    /// Timestamp
    pub timestamp: i64,
    /// Receipt message
    pub receipt_message: Option<SignalReceiptMessage>,
    /// Data message
    pub data_message: Option<SignalDataMessage>,
    /// Sync message
    pub sync_message: Option<SignalSyncMessage>,
    /// Call message
    pub call_message: Option<serde_json::Value>,
}

/// Signal receipt message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalReceiptMessage {
    /// Receipt type (DELIVERY, READ, VIEWED)
    #[serde(rename = "type")]
    pub receipt_type: String,
    /// Timestamps of messages this receipt is for
    pub timestamps: Vec<i64>,
    /// When the receipt was sent
    pub when: i64,
}

/// Signal data message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalDataMessage {
    /// Timestamp
    pub timestamp: i64,
    /// Message text
    pub message: Option<String>,
    /// Attachments
    pub attachments: Option<Vec<SignalAttachment>>,
    /// Quote (reply to)
    pub quote: Option<SignalQuote>,
    /// Mentions
    pub mentions: Option<Vec<SignalMention>>,
    /// Group info
    pub group_info: Option<SignalGroupInfo>,
    /// Sticker
    pub sticker: Option<SignalSticker>,
    /// Reaction
    pub reaction: Option<SignalReaction>,
}

/// Signal attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalAttachment {
    /// Content type
    pub content_type: String,
    /// Filename
    pub filename: Option<String>,
    /// ID
    pub id: String,
    /// Size
    pub size: i64,
}

/// Signal quote (reply)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalQuote {
    /// Quoted message ID
    pub id: i64,
    /// Author
    pub author: String,
    /// Text
    pub text: Option<String>,
    /// Attachments
    pub attachments: Option<Vec<serde_json::Value>>,
}

/// Signal mention
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalMention {
    /// Mentioned user
    pub uuid: String,
    /// Name
    pub name: Option<String>,
    /// Start position in text
    pub start: i32,
    /// Length
    pub length: i32,
}

/// Signal group info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalGroupInfo {
    /// Group ID
    pub group_id: String,
    /// Group name
    pub group_name: Option<String>,
    /// Revision
    pub revision: Option<i32>,
}

/// Signal sticker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalSticker {
    /// Sticker ID
    pub sticker_id: i64,
    /// Pack ID
    pub pack_id: i64,
}

/// Signal reaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalReaction {
    /// Emoji
    pub emoji: String,
    /// Target author
    pub target_author: String,
    /// Target timestamp
    pub target_timestamp: i64,
    /// Is removal
    pub is_remove: bool,
}

/// Signal sync message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalSyncMessage {
    /// Sent transcript
    pub sent_transcript: Option<SignalSentTranscript>,
}

/// Signal sent transcript
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalSentTranscript {
    /// Destination
    pub destination: Option<String>,
    /// Timestamp
    pub timestamp: i64,
    /// Message
    pub message: Option<String>,
    /// Group info
    pub group_info: Option<SignalGroupInfo>,
}

/// Signal webhook handler
pub struct SignalWebhookHandler {
    config: WebhookConfig,
}

impl SignalWebhookHandler {
    /// Create a new Signal webhook handler
    pub fn new(config: WebhookConfig) -> Self {
        Self { config }
    }

    /// Parse data message
    fn parse_data_message(
        &self,
        envelope: &SignalEnvelope,
        data_msg: &SignalDataMessage,
    ) -> Result<WebhookEvent> {
        let message_type = if let Some(_sticker) = &data_msg.sticker {
            MessageType::Image
        } else if let Some(_reaction) = &data_msg.reaction {
            MessageType::System
        } else if let Some(attachments) = &data_msg.attachments {
            if attachments.len() == 1 {
                let content_type = attachments[0].content_type.clone();
                if content_type.starts_with("image/") {
                    MessageType::Image
                } else if content_type.starts_with("video/") {
                    MessageType::Video
                } else if content_type.starts_with("audio/") {
                    MessageType::Voice
                } else {
                    MessageType::File
                }
            } else {
                MessageType::File
            }
        } else {
            MessageType::Text
        };

        let content = if let Some(reaction) = &data_msg.reaction {
            format!("[Reaction: {}]", reaction.emoji)
        } else {
            data_msg.message.clone().unwrap_or_default()
        };

        let mut metadata = HashMap::new();
        if let Some(group) = &data_msg.group_info {
            metadata.insert("group_id".to_string(), group.group_id.clone());
            if let Some(name) = &group.group_name {
                metadata.insert("group_name".to_string(), name.clone());
            }
        }
        if let Some(attachments) = &data_msg.attachments {
            metadata.insert(
                "attachment_count".to_string(),
                attachments.len().to_string(),
            );
        }
        if let Some(reaction) = &data_msg.reaction {
            metadata.insert("reaction_emoji".to_string(), reaction.emoji.clone());
            metadata.insert(
                "target_timestamp".to_string(),
                reaction.target_timestamp.to_string(),
            );
        }

        let sender_id = envelope.source.clone().unwrap_or_default();
        let sender_name = envelope.source_name.clone();

        let mut metadata_with_sender = metadata;
        metadata_with_sender.insert("sender_id".to_string(), sender_id.clone());
        if let Some(name) = &sender_name {
            metadata_with_sender.insert("sender_name".to_string(), name.clone());
        }
        if let Some(quote) = &data_msg.quote {
            metadata_with_sender.insert("reply_to".to_string(), quote.id.to_string());
        }

        let message = Message {
            id: uuid::Uuid::parse_str(&data_msg.timestamp.to_string())
                .unwrap_or_else(|_| uuid::Uuid::new_v4()),
            thread_id: uuid::Uuid::new_v4(),
            platform: PlatformType::Signal,
            message_type,
            content,
            metadata: metadata_with_sender,
            timestamp: chrono::DateTime::from_timestamp_millis(data_msg.timestamp)
                .unwrap_or_else(|| chrono::Utc::now()),
        };

        Ok(WebhookEvent {
            event_type: WebhookEventType::MessageReceived,
            platform: PlatformType::Signal,
            event_id: data_msg.timestamp.to_string(),
            timestamp: chrono::DateTime::from_timestamp_millis(data_msg.timestamp)
                .unwrap_or_else(|| chrono::Utc::now()),
            payload: serde_json::to_value(envelope)
                .map_err(|e| AgentError::platform(format!("JSON serialization error: {}", e)))?,
            message: Some(message),
            metadata: HashMap::new(),
        })
    }

    /// Parse receipt message
    fn parse_receipt_message(
        &self,
        envelope: &SignalEnvelope,
        receipt: &SignalReceiptMessage,
    ) -> Result<WebhookEvent> {
        let event_type = match receipt.receipt_type.as_str() {
            "READ" => WebhookEventType::MessageEdited, // Using as proxy for read receipt
            "DELIVERY" => WebhookEventType::System,
            "VIEWED" => WebhookEventType::System,
            _ => WebhookEventType::System,
        };

        let mut metadata = HashMap::new();
        metadata.insert("receipt_type".to_string(), receipt.receipt_type.clone());
        metadata.insert(
            "original_timestamps".to_string(),
            receipt
                .timestamps
                .iter()
                .map(|t| t.to_string())
                .collect::<Vec<_>>()
                .join(","),
        );

        Ok(WebhookEvent {
            event_type,
            platform: PlatformType::Signal,
            event_id: format!("receipt_{}", envelope.timestamp),
            timestamp: chrono::DateTime::from_timestamp_millis(receipt.when)
                .unwrap_or_else(|| chrono::Utc::now()),
            payload: serde_json::to_value(envelope)
                .map_err(|e| AgentError::platform(format!("JSON serialization error: {}", e)))?,
            message: None,
            metadata,
        })
    }
}

#[async_trait]
impl WebhookHandler for SignalWebhookHandler {
    fn platform_type(&self) -> PlatformType {
        PlatformType::Signal
    }

    async fn verify_signature(
        &self,
        _body: &[u8],
        _signature: Option<&str>,
        _timestamp: Option<&str>,
    ) -> Result<SignatureVerification> {
        // signal-cli HTTP JSON-RPC doesn't use signature verification
        Ok(SignatureVerification::Skipped)
    }

    async fn parse_payload(&self, body: &[u8]) -> Result<Vec<WebhookEvent>> {
        let payload: SignalWebhookPayload = serde_json::from_slice(body)
            .map_err(|e| AgentError::platform(format!("Failed to parse Signal webhook: {}", e)))?;

        debug!("Received Signal webhook: method={}", payload.method);

        let envelope = &payload.params.envelope;

        let event = if let Some(data_msg) = &envelope.data_message {
            self.parse_data_message(envelope, data_msg)?
        } else if let Some(receipt) = &envelope.receipt_message {
            self.parse_receipt_message(envelope, receipt)?
        } else if let Some(_sync) = &envelope.sync_message {
            // Sync messages are from user's other devices, typically ignored
            debug!("Received Signal sync message, ignoring");
            WebhookEvent {
                event_type: WebhookEventType::System,
                platform: PlatformType::Signal,
                event_id: envelope.timestamp.to_string(),
                timestamp: chrono::Utc::now(),
                payload: serde_json::to_value(envelope).map_err(|e| {
                    AgentError::platform(format!("JSON serialization error: {}", e))
                })?,
                message: None,
                metadata: HashMap::new(),
            }
        } else {
            warn!("Unknown Signal envelope type");
            WebhookEvent {
                event_type: WebhookEventType::Unknown,
                platform: PlatformType::Signal,
                event_id: envelope.timestamp.to_string(),
                timestamp: chrono::Utc::now(),
                payload: serde_json::to_value(envelope).map_err(|e| {
                    AgentError::platform(format!("JSON serialization error: {}", e))
                })?,
                message: None,
                metadata: HashMap::new(),
            }
        };

        Ok(vec![event])
    }

    async fn handle_event(&self, event: WebhookEvent) -> Result<()> {
        info!(
            "Handling Signal event: {:?} - {}",
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
            platform: PlatformType::Signal,
            endpoint_path: "/webhook/signal".to_string(),
            ..Default::default()
        };
        let handler = SignalWebhookHandler::new(config);

        let envelope = SignalEnvelope {
            source: Some("+1234567890".to_string()),
            source_uuid: Some("uuid-123".to_string()),
            source_name: Some("Test User".to_string()),
            source_device: Some(1),
            timestamp: 1234567890000,
            receipt_message: None,
            data_message: Some(SignalDataMessage {
                timestamp: 1234567890000,
                message: Some("Hello Signal!".to_string()),
                attachments: None,
                quote: None,
                mentions: None,
                group_info: None,
                sticker: None,
                reaction: None,
            }),
            sync_message: None,
            call_message: None,
        };

        let data_msg = envelope.data_message.as_ref().unwrap();
        let event = handler.parse_data_message(&envelope, data_msg).unwrap();

        assert_eq!(event.event_type, WebhookEventType::MessageReceived);
        assert!(event.message.is_some());

        let message = event.message.unwrap();
        assert_eq!(message.content, "Hello Signal!");
        assert_eq!(message.message_type, MessageType::Text);
        // sender info is stored in metadata
        assert_eq!(
            message.metadata.get("sender_id"),
            Some(&"+1234567890".to_string())
        );
    }

    #[test]
    fn test_parse_reaction() {
        let config = WebhookConfig {
            platform: PlatformType::Signal,
            endpoint_path: "/webhook/signal".to_string(),
            ..Default::default()
        };
        let handler = SignalWebhookHandler::new(config);

        let envelope = SignalEnvelope {
            source: Some("+1234567890".to_string()),
            source_uuid: Some("uuid-123".to_string()),
            source_name: None,
            source_device: Some(1),
            timestamp: 1234567890000,
            receipt_message: None,
            data_message: Some(SignalDataMessage {
                timestamp: 1234567890000,
                message: None,
                attachments: None,
                quote: None,
                mentions: None,
                group_info: None,
                sticker: None,
                reaction: Some(SignalReaction {
                    emoji: "❤️".to_string(),
                    target_author: "+0987654321".to_string(),
                    target_timestamp: 1234567880000,
                    is_remove: false,
                }),
            }),
            sync_message: None,
            call_message: None,
        };

        let data_msg = envelope.data_message.as_ref().unwrap();
        let event = handler.parse_data_message(&envelope, data_msg).unwrap();

        assert!(event.message.is_some());
        let message = event.message.unwrap();
        // Reaction messages are mapped to System type
        assert_eq!(message.message_type, MessageType::System);
        assert_eq!(message.content, "[Reaction: ❤️]");
    }
}
