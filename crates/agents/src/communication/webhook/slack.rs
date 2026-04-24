//! Slack Webhook Handler
//!
//! Handles incoming webhooks from Slack Events API and interactive components.
//! Supports signature verification (HMAC-SHA256) for request validation.
//!
//! Refactored to use common webhook utilities from the common module.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

// Import common webhook utilities
use super::common::{compute_hmac_sha256, MetadataBuilder, TokenVerifier};
use crate::communication::webhook::{
    SignatureVerification, WebhookConfig, WebhookEvent, WebhookEventType, WebhookHandler,
};
use crate::communication::{AgentMessageDispatcher, Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

/// Slack webhook payload types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SlackWebhookPayload {
    /// URL verification challenge
    UrlVerification {
        #[serde(rename = "type")]
        payload_type: String,
        token: String,
        challenge: String,
    },
    /// Events API event
    Event {
        token: String,
        #[serde(rename = "team_id")]
        team_id: String,
        #[serde(rename = "api_app_id")]
        api_app_id: String,
        event: Option<serde_json::Value>,
        #[serde(rename = "type")]
        payload_type: String,
        #[serde(rename = "event_id")]
        event_id: String,
        #[serde(rename = "event_time")]
        event_time: i64,
        #[serde(rename = "authed_users")]
        authed_users: Option<Vec<String>>,
        #[serde(rename = "authed_teams")]
        authed_teams: Option<Vec<String>>,
    },
    /// Interactive component payload
    Interactive {
        #[serde(rename = "type")]
        payload_type: String,
        #[serde(rename = "team")]
        team: Option<SlackTeamInfo>,
        #[serde(rename = "user")]
        user: Option<SlackUserInfo>,
        #[serde(rename = "channel")]
        channel: Option<SlackChannelInfo>,
        #[serde(rename = "message")]
        message: Option<SlackMessageInfo>,
        #[serde(rename = "actions")]
        actions: Option<Vec<SlackActionInfo>>,
        #[serde(rename = "view")]
        view: Option<SlackViewInfo>,
        #[serde(rename = "response_url")]
        response_url: Option<String>,
        #[serde(rename = "trigger_id")]
        trigger_id: Option<String>,
        #[serde(rename = "callback_id")]
        callback_id: Option<String>,
        #[serde(flatten)]
        extra: HashMap<String, serde_json::Value>,
    },
    /// Slash command payload
    SlashCommand {
        token: String,
        #[serde(rename = "team_id")]
        team_id: String,
        #[serde(rename = "team_domain")]
        team_domain: String,
        #[serde(rename = "channel_id")]
        channel_id: String,
        #[serde(rename = "channel_name")]
        channel_name: String,
        #[serde(rename = "user_id")]
        user_id: String,
        #[serde(rename = "user_name")]
        user_name: String,
        command: String,
        text: String,
        #[serde(rename = "response_url")]
        response_url: String,
        #[serde(rename = "trigger_id")]
        trigger_id: String,
    },
}

/// Slack team info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackTeamInfo {
    pub id: String,
    pub domain: Option<String>,
}

/// Slack user info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackUserInfo {
    pub id: String,
    pub username: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "team_id")]
    pub team_id: Option<String>,
}

/// Slack channel info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackChannelInfo {
    pub id: String,
    pub name: Option<String>,
}

/// Slack message info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackMessageInfo {
    #[serde(rename = "type")]
    pub message_type: Option<String>,
    pub user: Option<String>,
    pub ts: Option<String>,
    pub text: Option<String>,
    pub bot_id: Option<String>,
    pub blocks: Option<Vec<serde_json::Value>>,
}

/// Slack action info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackActionInfo {
    #[serde(rename = "action_id")]
    pub action_id: String,
    #[serde(rename = "block_id")]
    pub block_id: String,
    #[serde(rename = "type")]
    pub action_type: String,
    #[serde(rename = "value")]
    pub value: Option<String>,
    #[serde(rename = "selected_option")]
    pub selected_option: Option<serde_json::Value>,
    #[serde(rename = "selected_options")]
    pub selected_options: Option<Vec<serde_json::Value>>,
}

/// Slack view info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackViewInfo {
    pub id: String,
    #[serde(rename = "team_id")]
    pub team_id: String,
    #[serde(rename = "type")]
    pub view_type: String,
    #[serde(rename = "callback_id")]
    pub callback_id: String,
    pub state: Option<serde_json::Value>,
    pub hash: Option<String>,
}

/// Slack event message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackEventMessage {
    #[serde(rename = "type")]
    pub event_type: String,
    pub user: String,
    pub text: String,
    pub ts: String,
    pub channel: String,
    #[serde(rename = "event_ts")]
    pub event_ts: String,
    #[serde(rename = "channel_type")]
    pub channel_type: Option<String>,
    #[serde(rename = "thread_ts")]
    pub thread_ts: Option<String>,
    pub files: Option<Vec<SlackFileInfo>>,
    pub blocks: Option<Vec<serde_json::Value>>,
    pub attachments: Option<Vec<serde_json::Value>>,
    pub edited: Option<SlackEditedInfo>,
    pub bot_id: Option<String>,
    pub subtype: Option<String>,
}

/// Slack file info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackFileInfo {
    pub id: String,
    pub timestamp: i64,
    pub name: String,
    pub title: String,
    #[serde(rename = "mimetype")]
    pub mime_type: String,
    #[serde(rename = "filetype")]
    pub file_type: String,
    pub size: i64,
    #[serde(rename = "url_private")]
    pub url_private: String,
    #[serde(rename = "url_private_download")]
    pub url_private_download: Option<String>,
}

/// Slack edited info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackEditedInfo {
    pub user: String,
    pub ts: String,
}

/// Slack webhook handler
pub struct SlackWebhookHandler {
    config: WebhookConfig,
    signing_secret: String,
    // Use common verifier for standard cases
    #[allow(dead_code)]
    token_verifier: TokenVerifier,
    dispatcher: Option<Arc<AgentMessageDispatcher>>,
}

impl SlackWebhookHandler {
    /// Create a new Slack webhook handler
    pub fn new(signing_secret: String) -> Self {
        let mut config = WebhookConfig::default();
        config.platform = PlatformType::Slack;
        config.endpoint_path = "/webhook/slack".to_string();
        config.verify_signatures = true;

        let verifier = TokenVerifier::new(&signing_secret);

        Self {
            config,
            signing_secret: signing_secret.clone(),
            token_verifier: verifier,
            dispatcher: None,
        }
    }

    /// Attach an agent message dispatcher.
    pub fn with_dispatcher(mut self, dispatcher: Arc<AgentMessageDispatcher>) -> Self {
        self.dispatcher = Some(dispatcher);
        self
    }

    /// Create handler from environment variables
    pub fn from_env() -> Result<Self> {
        let signing_secret = std::env::var("SLACK_SIGNING_SECRET")
            .map_err(|_| AgentError::configuration("SLACK_SIGNING_SECRET not set"))?;

        Ok(Self::new(signing_secret))
    }

    /// Compute Slack HMAC-SHA256 signature (Slack-specific format)
    fn compute_slack_signature(&self, timestamp: &str, body: &[u8]) -> String {
        let base_string = format!("v0:{}:", timestamp);
        format!(
            "v0={}",
            compute_hmac_sha256(&self.signing_secret, body, &base_string)
        )
    }

    /// Verify Slack signature
    fn verify_slack_signature(&self, body: &[u8], signature: &str, timestamp: &str) -> bool {
        let computed = self.compute_slack_signature(timestamp, body);
        computed == signature
    }

    /// Parse event type using common pattern
    fn parse_event_type(&self, event_type: &str) -> WebhookEventType {
        match event_type {
            "message" => WebhookEventType::MessageReceived,
            "app_mention" => WebhookEventType::BotMentioned,
            "reaction_added" | "reaction_removed" => WebhookEventType::MessageReceived,
            "file_shared" | "file_created" => WebhookEventType::FileShared,
            "member_joined_channel" => WebhookEventType::UserJoined,
            "member_left_channel" => WebhookEventType::UserLeft,
            "channel_created" | "channel_deleted" | "app_home_opened" | "app_uninstalled" => {
                WebhookEventType::System
            }
            _ => WebhookEventType::Unknown,
        }
    }

    /// Convert Slack event to internal message using common patterns
    fn convert_event_to_message(&self, event: &SlackEventMessage) -> Option<Message> {
        // Skip bot messages
        if event.bot_id.is_some() {
            return None;
        }

        // Skip message subtypes
        if event.subtype.is_some() {
            return None;
        }

        // Use MetadataBuilder for cleaner metadata construction
        let metadata = MetadataBuilder::new()
            .add("user", &event.user)
            .add("channel", &event.channel)
            .add("ts", &event.ts)
            .add_optional("channel_type", event.channel_type.as_ref())
            .add_optional("thread_ts", event.thread_ts.as_ref())
            .build();

        Some(Message {
            id: uuid::Uuid::new_v4(),
            thread_id: uuid::Uuid::new_v4(),
            platform: PlatformType::Slack,
            message_type: MessageType::Text,
            content: event.text.clone(),
            metadata,
            timestamp: chrono::Utc::now(),
        })
    }

    /// Convert interactive payload to message
    fn convert_interactive_to_message(&self, payload: &SlackWebhookPayload) -> Option<Message> {
        match payload {
            SlackWebhookPayload::Interactive {
                user,
                channel,
                actions,
                callback_id,
                ..
            } => {
                let user_id = user.as_ref()?.id.clone();
                let channel_id = channel.as_ref()?.id.clone();

                let content = if let Some(actions) = actions {
                    actions
                        .iter()
                        .map(|a| {
                            format!(
                                "[Action: {} - {}]",
                                a.action_id,
                                a.value.as_deref().unwrap_or("")
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                } else {
                    "[Interactive]".to_string()
                };

                let metadata = MetadataBuilder::new()
                    .add("user_id", &user_id)
                    .add("channel_id", &channel_id)
                    .add_optional("callback_id", callback_id.as_ref())
                    .build();

                Some(Message {
                    id: uuid::Uuid::new_v4(),
                    thread_id: uuid::Uuid::new_v4(),
                    platform: PlatformType::Slack,
                    message_type: MessageType::Text,
                    content,
                    metadata,
                    timestamp: chrono::Utc::now(),
                })
            }
            _ => None,
        }
    }

    /// Convert slash command to message
    fn convert_slash_command_to_message(&self, payload: &SlackWebhookPayload) -> Option<Message> {
        match payload {
            SlackWebhookPayload::SlashCommand {
                user_id,
                user_name,
                channel_id,
                command,
                text,
                ..
            } => {
                let content = format!("{} {}", command, text);

                let metadata = MetadataBuilder::new()
                    .add("user_id", user_id)
                    .add("user_name", user_name)
                    .add("channel_id", channel_id)
                    .add("command", command)
                    .build();

                Some(Message {
                    id: uuid::Uuid::new_v4(),
                    thread_id: uuid::Uuid::new_v4(),
                    platform: PlatformType::Slack,
                    message_type: MessageType::Text,
                    content,
                    metadata,
                    timestamp: chrono::Utc::now(),
                })
            }
            _ => None,
        }
    }

    /// Get ping response
    pub fn get_ping_response(&self) -> SlackWebhookResponse {
        SlackWebhookResponse::pong()
    }
}

#[async_trait]
impl WebhookHandler for SlackWebhookHandler {
    fn platform_type(&self) -> PlatformType {
        PlatformType::Slack
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
                warn!("No signature provided for Slack webhook");
                return Ok(SignatureVerification::Skipped);
            }
        };

        let timestamp = match timestamp {
            Some(t) => t,
            None => {
                warn!("No timestamp provided for Slack webhook");
                return Ok(SignatureVerification::Skipped);
            }
        };

        if self.verify_slack_signature(body, signature, timestamp) {
            debug!("Slack signature verified successfully");
            Ok(SignatureVerification::Valid)
        } else {
            error!("Slack signature verification failed");
            Ok(SignatureVerification::Invalid)
        }
    }

    async fn parse_payload(&self, body: &[u8]) -> Result<Vec<WebhookEvent>> {
        let payload: SlackWebhookPayload = serde_json::from_slice(body)
            .map_err(|e| AgentError::platform(format!("Failed to parse Slack payload: {}", e)))?;

        debug!("Parsed Slack payload");

        match payload {
            SlackWebhookPayload::UrlVerification { challenge, .. } => Ok(vec![WebhookEvent {
                event_type: WebhookEventType::System,
                platform: PlatformType::Slack,
                event_id: "challenge".to_string(),
                timestamp: chrono::Utc::now(),
                payload: serde_json::json!({ "challenge": challenge }),
                message: None,
                metadata: MetadataBuilder::new().add("challenge", challenge).build(),
            }]),
            SlackWebhookPayload::Event {
                event,
                event_id,
                event_time,
                ..
            } => {
                if let Some(event_data) = event {
                    let event_type_str = event_data
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");

                    let event_type = self.parse_event_type(event_type_str);
                    let message = if event_type == WebhookEventType::MessageReceived
                        || event_type == WebhookEventType::BotMentioned
                    {
                        serde_json::from_value::<SlackEventMessage>(event_data.clone())
                            .ok()
                            .and_then(|e| self.convert_event_to_message(&e))
                    } else {
                        None
                    };

                    let webhook_event = WebhookEvent {
                        event_type,
                        platform: PlatformType::Slack,
                        event_id,
                        timestamp: chrono::DateTime::from_timestamp(event_time, 0)
                            .unwrap_or_else(chrono::Utc::now),
                        payload: event_data,
                        message,
                        metadata: HashMap::new(),
                    };

                    Ok(vec![webhook_event])
                } else {
                    Ok(vec![])
                }
            }
            SlackWebhookPayload::Interactive { .. } => {
                let message = self.convert_interactive_to_message(&payload);

                let webhook_event = WebhookEvent {
                    event_type: WebhookEventType::MessageReceived,
                    platform: PlatformType::Slack,
                    event_id: uuid::Uuid::new_v4().to_string(),
                    timestamp: chrono::Utc::now(),
                    payload: serde_json::to_value(&payload).unwrap_or_default(),
                    message,
                    metadata: HashMap::new(),
                };

                Ok(vec![webhook_event])
            }
            SlackWebhookPayload::SlashCommand { .. } => {
                let message = self.convert_slash_command_to_message(&payload);

                let webhook_event = WebhookEvent {
                    event_type: WebhookEventType::MessageReceived,
                    platform: PlatformType::Slack,
                    event_id: uuid::Uuid::new_v4().to_string(),
                    timestamp: chrono::Utc::now(),
                    payload: serde_json::to_value(&payload).unwrap_or_default(),
                    message,
                    metadata: HashMap::new(),
                };

                Ok(vec![webhook_event])
            }
        }
    }

    async fn handle_event(&self, event: WebhookEvent) -> Result<()> {
        match event.event_type {
            WebhookEventType::MessageReceived | WebhookEventType::BotMentioned => {
                if let Some(msg) = &event.message {
                    info!(
                        "Received message from Slack: {} (type: {:?})",
                        msg.content, msg.message_type
                    );

                    // P0 FIX: Removed dispatcher.dispatch() to avoid duplicate
                    // processing. Messages are now routed
                    // exclusively through channel_event_bus →
                    // MessageProcessor → AgentResolver path in webhook_handler.
                    // if let Some(dispatcher) = &self.dispatcher {
                    //     let platform_user_id = event.metadata.get("team_id")
                    //         .cloned()
                    //         .unwrap_or_default();
                    //     let target_channel_id =
                    // msg.metadata.get("channel_id")
                    //         .cloned()
                    //         .unwrap_or_default();
                    //
                    //     dispatcher.dispatch(
                    //         PlatformType::Slack,
                    //         &platform_user_id,
                    //         msg.clone(),
                    //         target_channel_id,
                    //     ).await?;
                    // }
                }
            }
            WebhookEventType::UserJoined => info!("User joined Slack channel"),
            WebhookEventType::UserLeft => info!("User left Slack channel"),
            WebhookEventType::System => debug!("Received Slack system event"),
            _ => debug!(
                "Received unhandled event type from Slack: {:?}",
                event.event_type
            ),
        }

        Ok(())
    }

    fn get_config(&self) -> &WebhookConfig {
        &self.config
    }
}

/// Slack webhook response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackWebhookResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocks: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "response_type")]
    pub response_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "replace_original")]
    pub replace_original: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "delete_original")]
    pub delete_original: Option<bool>,
}

impl SlackWebhookResponse {
    /// Create a text response
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            text: Some(content.into()),
            blocks: None,
            attachments: None,
            response_type: None,
            replace_original: None,
            delete_original: None,
        }
    }

    /// Create an ephemeral response
    pub fn ephemeral(content: impl Into<String>) -> Self {
        Self {
            text: Some(content.into()),
            blocks: None,
            attachments: None,
            response_type: Some("ephemeral".to_string()),
            replace_original: None,
            delete_original: None,
        }
    }

    /// Create an in-channel response
    pub fn in_channel(content: impl Into<String>) -> Self {
        Self {
            text: Some(content.into()),
            blocks: None,
            attachments: None,
            response_type: Some("in_channel".to_string()),
            replace_original: None,
            delete_original: None,
        }
    }

    /// Create a blocks response
    pub fn blocks(blocks: Vec<serde_json::Value>) -> Self {
        Self {
            text: None,
            blocks: Some(blocks),
            attachments: None,
            response_type: None,
            replace_original: None,
            delete_original: None,
        }
    }

    /// Create an empty success response
    pub fn success() -> Self {
        Self {
            text: None,
            blocks: None,
            attachments: None,
            response_type: None,
            replace_original: None,
            delete_original: None,
        }
    }

    /// Create pong response for URL verification
    pub fn pong() -> Self {
        Self::success()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slack_webhook_response() {
        let text_resp = SlackWebhookResponse::text("Hello");
        assert_eq!(text_resp.text, Some("Hello".to_string()));

        let ephemeral_resp = SlackWebhookResponse::ephemeral("Secret");
        assert_eq!(ephemeral_resp.response_type, Some("ephemeral".to_string()));

        let in_channel_resp = SlackWebhookResponse::in_channel("Public");
        assert_eq!(
            in_channel_resp.response_type,
            Some("in_channel".to_string())
        );
    }

    #[test]
    fn test_parse_url_verification() {
        let json =
            r#"{"type":"url_verification","token":"test_token","challenge":"test_challenge"}"#;
        let payload: SlackWebhookPayload = serde_json::from_str(json).unwrap();
        match payload {
            SlackWebhookPayload::UrlVerification { challenge, .. } => {
                assert_eq!(challenge, "test_challenge");
            }
            _ => panic!("Expected UrlVerification payload"),
        }
    }

    #[test]
    fn test_parse_slash_command() {
        let json = r#"{
            "token":"test_token",
            "team_id":"T123",
            "team_domain":"test",
            "channel_id":"C123",
            "channel_name":"general",
            "user_id":"U123",
            "user_name":"testuser",
            "command":"/hello",
            "text":"world",
            "response_url":"https://hooks.slack.com/...",
            "trigger_id":"123.456"
        }"#;
        let payload: SlackWebhookPayload = serde_json::from_str(json).unwrap();
        match payload {
            SlackWebhookPayload::SlashCommand { command, text, .. } => {
                assert_eq!(command, "/hello");
                assert_eq!(text, "world");
            }
            _ => panic!("Expected SlashCommand payload"),
        }
    }

    #[test]
    fn test_metadata_builder() {
        let metadata = MetadataBuilder::new()
            .add("key1", "value1")
            .add_optional("key2", Some("value2"))
            .add_optional("key3", None::<&str>)
            .build();

        assert_eq!(metadata.get("key1"), Some(&"value1".to_string()));
        assert_eq!(metadata.get("key2"), Some(&"value2".to_string()));
        assert!(!metadata.contains_key("key3"));
    }
}
