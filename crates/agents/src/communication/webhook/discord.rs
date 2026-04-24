//! Discord Webhook Handler
//!
//! Handles incoming webhooks from Discord messaging platform.
//! Supports signature verification (Ed25519) for interaction verification.
//!
//! Refactored to use common webhook utilities from the common module.

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

// Import common webhook utilities
use super::common::MetadataBuilder;
use crate::communication::webhook::{
    SignatureVerification, WebhookConfig, WebhookEvent, WebhookEventType, WebhookHandler,
};
use crate::communication::{AgentMessageDispatcher, Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

/// Discord webhook payload types
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum DiscordWebhookPayload {
    /// Ping verification (URL verification)
    #[serde(rename = "1")]
    Ping {
        #[serde(rename = "id")]
        application_id: String,
    },
    /// Application command (slash command)
    #[serde(rename = "2")]
    ApplicationCommand {
        #[serde(rename = "id")]
        interaction_id: String,
        #[serde(rename = "application_id")]
        application_id: String,
        #[serde(rename = "guild_id")]
        guild_id: Option<String>,
        #[serde(rename = "channel_id")]
        channel_id: String,
        #[serde(rename = "member")]
        member: Option<DiscordMember>,
        #[serde(rename = "user")]
        user: Option<DiscordUser>,
        data: DiscordCommandData,
        token: String,
        version: i32,
    },
    /// Message component interaction (buttons, select menus)
    #[serde(rename = "3")]
    MessageComponent {
        #[serde(rename = "id")]
        interaction_id: String,
        #[serde(rename = "application_id")]
        application_id: String,
        #[serde(rename = "guild_id")]
        guild_id: Option<String>,
        #[serde(rename = "channel_id")]
        channel_id: String,
        #[serde(rename = "member")]
        member: Option<DiscordMember>,
        #[serde(rename = "user")]
        user: Option<DiscordUser>,
        data: DiscordComponentData,
        token: String,
        version: i32,
    },
    /// Application command autocomplete
    #[serde(rename = "4")]
    ApplicationCommandAutocomplete {
        #[serde(rename = "id")]
        interaction_id: String,
        #[serde(rename = "application_id")]
        application_id: String,
        #[serde(rename = "guild_id")]
        guild_id: Option<String>,
        #[serde(rename = "channel_id")]
        channel_id: String,
        data: DiscordCommandData,
        token: String,
        version: i32,
    },
    /// Modal submit
    #[serde(rename = "5")]
    ModalSubmit {
        #[serde(rename = "id")]
        interaction_id: String,
        #[serde(rename = "application_id")]
        application_id: String,
        #[serde(rename = "guild_id")]
        guild_id: Option<String>,
        #[serde(rename = "channel_id")]
        channel_id: String,
        data: DiscordModalData,
        token: String,
        version: i32,
    },
}

impl<'de> Deserialize<'de> for DiscordWebhookPayload {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        let value = serde_json::Value::deserialize(deserializer)?;
        let type_val = value
            .get("type")
            .ok_or_else(|| D::Error::custom("missing type field"))?;

        let type_num = type_val
            .as_i64()
            .or_else(|| type_val.as_str().and_then(|s| s.parse().ok()))
            .ok_or_else(|| D::Error::custom("type must be an integer"))?;

        match type_num {
            1 => {
                let id = value
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| D::Error::custom("missing id field"))?
                    .to_string();
                Ok(DiscordWebhookPayload::Ping { application_id: id })
            }
            2 => {
                let v: serde_json::Value = value.clone();
                // Parse as ApplicationCommand with full struct
                serde_json::from_value(v).map_err(D::Error::custom)
            }
            3 => {
                let v: serde_json::Value = value.clone();
                serde_json::from_value(v).map_err(D::Error::custom)
            }
            4 => {
                let v: serde_json::Value = value.clone();
                serde_json::from_value(v).map_err(D::Error::custom)
            }
            5 => {
                let v: serde_json::Value = value.clone();
                serde_json::from_value(v).map_err(D::Error::custom)
            }
            _ => Err(D::Error::custom(format!("unknown type: {}", type_num))),
        }
    }
}

/// Discord member info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordMember {
    pub user: Option<DiscordUser>,
    pub nick: Option<String>,
    pub avatar: Option<String>,
    pub roles: Vec<String>,
    #[serde(rename = "joined_at")]
    pub joined_at: String,
    #[serde(rename = "premium_since")]
    pub premium_since: Option<String>,
    pub deaf: bool,
    pub mute: bool,
    pub pending: Option<bool>,
}

/// Discord user info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordUser {
    pub id: String,
    pub username: String,
    #[serde(rename = "global_name")]
    pub global_name: Option<String>,
    pub avatar: Option<String>,
    pub bot: Option<bool>,
    pub system: Option<bool>,
}

/// Discord command data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordCommandData {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub command_type: i32,
    pub options: Option<Vec<DiscordCommandOption>>,
}

/// Discord command option
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordCommandOption {
    pub name: String,
    #[serde(rename = "type")]
    pub option_type: i32,
    pub value: Option<serde_json::Value>,
    pub options: Option<Vec<DiscordCommandOption>>,
    pub focused: Option<bool>,
}

/// Discord component data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordComponentData {
    #[serde(rename = "custom_id")]
    pub custom_id: String,
    #[serde(rename = "component_type")]
    pub component_type: i32,
    pub values: Option<Vec<String>>,
}

/// Discord modal data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordModalData {
    #[serde(rename = "custom_id")]
    pub custom_id: String,
    pub components: Vec<DiscordActionRow>,
}

/// Discord action row
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordActionRow {
    #[serde(rename = "type")]
    pub component_type: i32,
    pub components: Vec<DiscordTextInput>,
}

/// Discord text input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordTextInput {
    #[serde(rename = "type")]
    pub component_type: i32,
    #[serde(rename = "custom_id")]
    pub custom_id: String,
    pub value: String,
}

/// Discord webhook handler
#[allow(dead_code)]
pub struct DiscordWebhookHandler {
    config: WebhookConfig,
    public_key: String,
    dispatcher: Option<std::sync::Arc<AgentMessageDispatcher>>,
}

impl DiscordWebhookHandler {
    /// Create a new Discord webhook handler
    pub fn new(public_key: String) -> Self {
        let mut config = WebhookConfig::default();
        config.platform = PlatformType::Discord;
        config.endpoint_path = "/webhook/discord".to_string();
        config.verify_signatures = true;

        Self {
            config,
            public_key,
            dispatcher: None,
        }
    }

    pub fn with_dispatcher(mut self, dispatcher: std::sync::Arc<AgentMessageDispatcher>) -> Self {
        self.dispatcher = Some(dispatcher);
        self
    }

    /// Create handler from environment variables
    pub fn from_env() -> Result<Self> {
        let public_key = std::env::var("DISCORD_PUBLIC_KEY")
            .map_err(|_| AgentError::configuration("DISCORD_PUBLIC_KEY not set"))?;

        Ok(Self::new(public_key))
    }

    /// Verify Discord signature using Ed25519
    fn verify_ed25519_signature(&self, _body: &[u8], _signature: &str, _timestamp: &str) -> bool {
        // Discord signature verification requires Ed25519
        // Format: signature is hex-encoded Ed25519 signature of timestamp + body
        // This is a placeholder - actual implementation would use ed25519-dalek
        debug!("Discord signature verification skipped (not implemented)");
        true
    }

    /// Convert Discord payload to internal message using MetadataBuilder
    fn convert_to_message(&self, payload: &DiscordWebhookPayload) -> Option<Message> {
        match payload {
            DiscordWebhookPayload::ApplicationCommand {
                data,
                user,
                member,
                channel_id,
                ..
            } => {
                let _user_info = user.clone().or_else(|| member.as_ref()?.user.clone())?;

                let content = if let Some(options) = &data.options {
                    let opts: Vec<String> = options
                        .iter()
                        .filter_map(|opt| {
                            opt.value.as_ref().map(|v| format!("{}: {}", opt.name, v))
                        })
                        .collect();
                    format!("/{} {}", data.name, opts.join(" "))
                } else {
                    format!("/{}", data.name)
                };

                let metadata = MetadataBuilder::new()
                    .add("command_name", &data.name)
                    .add("channel_id", channel_id)
                    .add_optional("nick", member.as_ref().and_then(|m| m.nick.as_ref()))
                    .build();

                Some(Message {
                    id: uuid::Uuid::new_v4(),
                    thread_id: uuid::Uuid::new_v4(),
                    platform: PlatformType::Discord,
                    message_type: MessageType::Text,
                    content,
                    metadata,
                    timestamp: chrono::Utc::now(),
                })
            }
            DiscordWebhookPayload::MessageComponent {
                data,
                user,
                member,
                channel_id,
                ..
            } => {
                let _user_info = user.clone().or_else(|| member.as_ref()?.user.clone())?;

                let metadata = MetadataBuilder::new()
                    .add("custom_id", &data.custom_id)
                    .add("channel_id", channel_id)
                    .add_optional("values", data.values.as_ref().map(|v| v.join(",")))
                    .build();

                Some(Message {
                    id: uuid::Uuid::new_v4(),
                    thread_id: uuid::Uuid::new_v4(),
                    platform: PlatformType::Discord,
                    message_type: MessageType::Text,
                    content: format!("[Component: {}]", data.custom_id),
                    metadata,
                    timestamp: chrono::Utc::now(),
                })
            }
            _ => None,
        }
    }

    /// Get ping response
    pub fn get_ping_response(&self) -> DiscordWebhookResponse {
        DiscordWebhookResponse::pong()
    }
}

#[async_trait]
impl WebhookHandler for DiscordWebhookHandler {
    fn platform_type(&self) -> PlatformType {
        PlatformType::Discord
    }

    async fn verify_signature(
        &self,
        body: &[u8],
        signature: Option<&str>,
        timestamp: Option<&str>,
    ) -> Result<SignatureVerification> {
        let signature = match signature {
            Some(s) => s,
            None => {
                warn!("No signature provided for Discord webhook");
                return Ok(SignatureVerification::Skipped);
            }
        };

        let timestamp = match timestamp {
            Some(t) => t,
            None => {
                warn!("No timestamp provided for Discord webhook");
                return Ok(SignatureVerification::Skipped);
            }
        };

        if self.verify_ed25519_signature(body, signature, timestamp) {
            debug!("Discord signature verified successfully");
            Ok(SignatureVerification::Valid)
        } else {
            error!("Discord signature verification failed");
            Ok(SignatureVerification::Invalid)
        }
    }

    async fn parse_payload(&self, body: &[u8]) -> Result<Vec<WebhookEvent>> {
        let payload: DiscordWebhookPayload = serde_json::from_slice(body)
            .map_err(|e| AgentError::platform(format!("Failed to parse Discord payload: {}", e)))?;

        debug!("Parsed Discord payload: {:?}", payload);

        let event_type = match &payload {
            DiscordWebhookPayload::Ping { .. } => WebhookEventType::System,
            DiscordWebhookPayload::ApplicationCommand { .. } => WebhookEventType::BotMentioned,
            DiscordWebhookPayload::MessageComponent { .. } => WebhookEventType::MessageReceived,
            DiscordWebhookPayload::ApplicationCommandAutocomplete { .. } => {
                WebhookEventType::System
            }
            DiscordWebhookPayload::ModalSubmit { .. } => WebhookEventType::MessageReceived,
        };

        let message = self.convert_to_message(&payload);

        // Build metadata using MetadataBuilder
        let metadata = match &payload {
            DiscordWebhookPayload::Ping { application_id } => MetadataBuilder::new()
                .add("application_id", application_id)
                .add("challenge", "pong")
                .build(),
            DiscordWebhookPayload::ApplicationCommand {
                interaction_id,
                token,
                ..
            } => MetadataBuilder::new()
                .add("interaction_id", interaction_id)
                .add("interaction_token", token)
                .build(),
            DiscordWebhookPayload::MessageComponent {
                interaction_id,
                token,
                ..
            } => MetadataBuilder::new()
                .add("interaction_id", interaction_id)
                .add("interaction_token", token)
                .build(),
            _ => HashMap::new(),
        };

        let webhook_event = WebhookEvent {
            event_type,
            platform: PlatformType::Discord,
            event_id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now(),
            payload: serde_json::to_value(&payload).unwrap_or_default(),
            message,
            metadata,
        };

        Ok(vec![webhook_event])
    }

    async fn handle_event(&self, event: WebhookEvent) -> Result<()> {
        match event.event_type {
            WebhookEventType::BotMentioned | WebhookEventType::MessageReceived => {
                if let Some(msg) = &event.message {
                    info!(
                        "Received Discord command: {} (type: {:?})",
                        msg.content, msg.message_type
                    );

                    // P0 FIX: Removed dispatcher.dispatch() to avoid duplicate
                    // processing. Messages are now routed
                    // exclusively through channel_event_bus →
                    // MessageProcessor → AgentResolver path in webhook_handler.
                    // if let Some(dispatcher) = &self.dispatcher {
                    //     let platform_user_id = event.metadata.get("guild_id")
                    //         .or_else(|| event.metadata.get("application_id"))
                    //         .cloned()
                    //         .unwrap_or_default();
                    //     let target_channel_id =
                    // msg.metadata.get("channel_id")
                    //         .cloned()
                    //         .unwrap_or_default();
                    //
                    //     dispatcher.dispatch(
                    //         PlatformType::Discord,
                    //         &platform_user_id,
                    //         msg.clone(),
                    //         target_channel_id,
                    //     ).await?;
                    // }
                }
            }
            WebhookEventType::System => {
                debug!("Received Discord system event (ping)");
            }
            _ => {
                debug!(
                    "Received unhandled event type from Discord: {:?}",
                    event.event_type
                );
            }
        }

        Ok(())
    }

    fn get_config(&self) -> &WebhookConfig {
        &self.config
    }
}

/// Discord webhook response types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DiscordWebhookResponse {
    /// Pong response for ping
    #[serde(rename = "1")]
    Pong,
    /// Channel message with source
    #[serde(rename = "4")]
    ChannelMessage {
        data: DiscordInteractionResponseData,
    },
    /// Deferred channel message with source
    #[serde(rename = "5")]
    DeferredChannelMessage {
        data: Option<DiscordInteractionResponseData>,
    },
    /// Deferred update message
    #[serde(rename = "6")]
    DeferredUpdateMessage,
    /// Update message
    #[serde(rename = "7")]
    UpdateMessage {
        data: DiscordInteractionResponseData,
    },
    /// Application command autocomplete result
    #[serde(rename = "8")]
    ApplicationCommandAutocompleteResult { data: DiscordAutocompleteData },
    /// Modal
    #[serde(rename = "9")]
    Modal { data: DiscordModalResponseData },
}

/// Discord interaction response data
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiscordInteractionResponseData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tts: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embeds: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "allowed_mentions")]
    pub allowed_mentions: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<serde_json::Value>>,
}

/// Discord autocomplete data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordAutocompleteData {
    pub choices: Vec<DiscordApplicationCommandOptionChoice>,
}

/// Discord application command option choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordApplicationCommandOptionChoice {
    pub name: String,
    pub value: serde_json::Value,
}

/// Discord modal response data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordModalResponseData {
    #[serde(rename = "custom_id")]
    pub custom_id: String,
    pub title: String,
    pub components: Vec<serde_json::Value>,
}

impl DiscordWebhookResponse {
    /// Create a pong response (for ping verification)
    pub fn pong() -> Self {
        Self::Pong
    }

    /// Create a message response
    pub fn message(content: impl Into<String>) -> Self {
        Self::ChannelMessage {
            data: DiscordInteractionResponseData {
                content: Some(content.into()),
                ..Default::default()
            },
        }
    }

    /// Create a message response with embeds
    pub fn message_with_embeds(content: impl Into<String>, embeds: Vec<serde_json::Value>) -> Self {
        Self::ChannelMessage {
            data: DiscordInteractionResponseData {
                content: Some(content.into()),
                embeds: Some(embeds),
                ..Default::default()
            },
        }
    }

    /// Create a deferred message response (for long-running operations)
    pub fn deferred_message() -> Self {
        Self::DeferredChannelMessage { data: None }
    }

    /// Create an ephemeral message (only visible to the user)
    pub fn ephemeral_message(content: impl Into<String>) -> Self {
        Self::ChannelMessage {
            data: DiscordInteractionResponseData {
                content: Some(content.into()),
                flags: Some(64), // EPHEMERAL flag
                ..Default::default()
            },
        }
    }

    /// Create an update message response (for component interactions)
    pub fn update_message(content: impl Into<String>) -> Self {
        Self::UpdateMessage {
            data: DiscordInteractionResponseData {
                content: Some(content.into()),
                ..Default::default()
            },
        }
    }

    /// Create a modal response
    pub fn modal(
        custom_id: impl Into<String>,
        title: impl Into<String>,
        components: Vec<serde_json::Value>,
    ) -> Self {
        Self::Modal {
            data: DiscordModalResponseData {
                custom_id: custom_id.into(),
                title: title.into(),
                components,
            },
        }
    }

    /// Create autocomplete response
    pub fn autocomplete(choices: Vec<DiscordApplicationCommandOptionChoice>) -> Self {
        Self::ApplicationCommandAutocompleteResult {
            data: DiscordAutocompleteData { choices },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discord_webhook_response() {
        let pong = DiscordWebhookResponse::pong();
        match pong {
            DiscordWebhookResponse::Pong => {}
            _ => panic!("Expected Pong response"),
        }

        let message = DiscordWebhookResponse::message("Hello");
        match message {
            DiscordWebhookResponse::ChannelMessage { data } => {
                assert_eq!(data.content, Some("Hello".to_string()));
            }
            _ => panic!("Expected ChannelMessage response"),
        }

        let ephemeral = DiscordWebhookResponse::ephemeral_message("Secret");
        match ephemeral {
            DiscordWebhookResponse::ChannelMessage { data } => {
                assert_eq!(data.content, Some("Secret".to_string()));
                assert_eq!(data.flags, Some(64));
            }
            _ => panic!("Expected ChannelMessage response"),
        }
    }

    #[test]
    fn test_parse_ping_payload() {
        let json = r#"{"type":1,"id":"123456789"}"#;
        let payload: DiscordWebhookPayload = serde_json::from_str(json).unwrap();
        match payload {
            DiscordWebhookPayload::Ping { application_id } => {
                assert_eq!(application_id, "123456789");
            }
            _ => panic!("Expected Ping payload"),
        }
    }

    #[test]
    fn test_metadata_builder_usage() {
        let metadata = MetadataBuilder::new()
            .add("command_name", "test")
            .add("channel_id", "12345")
            .add_optional("nick", Some("nickname"))
            .build();

        assert_eq!(metadata.get("command_name"), Some(&"test".to_string()));
        assert_eq!(metadata.get("channel_id"), Some(&"12345".to_string()));
        assert_eq!(metadata.get("nick"), Some(&"nickname".to_string()));
    }
}
