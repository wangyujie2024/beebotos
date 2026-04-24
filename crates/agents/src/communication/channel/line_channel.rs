//! LINE Messaging API Channel Implementation
//!
//! Provides integration with LINE through the LINE Messaging API.
//! Supports both push messages and webhook receiving.
//!
//! # Features
//! - One-on-one chat
//! - Group and room support
//! - Rich messages (buttons, carousels)
//! - Line Beacon integration
//!
//! # API Reference
//! - <https://developers.line.biz/en/reference/messaging-api/>

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, info};

use super::r#trait::{
    BaseChannelConfig, Channel, ChannelConfig, ChannelEvent, ChannelInfo, ConnectionMode,
    ContentType, MemberInfo,
};
use crate::communication::{Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

/// LINE Messaging API base URL
const LINE_API_BASE: &str = "https://api.line.me/v2";
#[allow(dead_code)]
const LINE_DATA_API_BASE: &str = "https://api-data.line.me/v2";

/// LINE channel implementation
pub struct LineChannel {
    config: LineConfig,
    client: Client,
    connected: bool,
}

/// LINE configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineConfig {
    /// Channel access token
    pub channel_access_token: String,
    /// Channel secret (for webhook verification)
    pub channel_secret: Option<String>,
    #[serde(flatten)]
    pub base: BaseChannelConfig,
}

impl LineConfig {
    /// Create configuration from environment variables
    pub fn from_env() -> Option<Self> {
        let channel_access_token = std::env::var("LINE_CHANNEL_ACCESS_TOKEN").ok()?;
        let channel_secret = std::env::var("LINE_CHANNEL_SECRET").ok();
        let mut base = BaseChannelConfig::from_env("LINE")?;
        // LINE defaults to Webhook mode
        base.connection_mode = ConnectionMode::Webhook;

        Some(Self {
            channel_access_token,
            channel_secret,
            base,
        })
    }

    /// Validate configuration
    pub fn is_valid(&self) -> bool {
        !self.channel_access_token.is_empty()
    }
}

impl Default for LineConfig {
    fn default() -> Self {
        let mut base = BaseChannelConfig::default();
        // LINE defaults to Webhook mode
        base.connection_mode = ConnectionMode::Webhook;

        Self {
            channel_access_token: String::new(),
            channel_secret: None,
            base,
        }
    }
}

impl ChannelConfig for LineConfig {
    fn from_env() -> Option<Self>
    where
        Self: Sized,
    {
        Self::from_env()
    }

    fn is_valid(&self) -> bool {
        self.is_valid()
    }

    fn allowlist(&self) -> Vec<String> {
        vec![]
    }

    fn connection_mode(&self) -> ConnectionMode {
        self.base.connection_mode
    }

    fn auto_reconnect(&self) -> bool {
        self.base.auto_reconnect
    }

    fn max_reconnect_attempts(&self) -> u32 {
        self.base.max_reconnect_attempts
    }
}

/// LINE message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
enum LineMessageType {
    Text {
        text: String,
    },
    Image {
        original_content_url: String,
        preview_image_url: String,
    },
    Video {
        original_content_url: String,
        preview_image_url: String,
    },
    Audio {
        original_content_url: String,
        duration: i32,
    },
    File {
        original_content_url: String,
        file_name: String,
    },
    Location {
        title: String,
        address: String,
        latitude: f64,
        longitude: f64,
    },
    Sticker {
        package_id: String,
        sticker_id: String,
    },
}

/// LINE message request
#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
struct LineMessageRequest {
    to: String,
    messages: Vec<serde_json::Value>,
}

/// LINE broadcast request
#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
struct LineBroadcastRequest {
    messages: Vec<serde_json::Value>,
}

impl LineChannel {
    /// Create a new LINE channel
    pub fn new(config: LineConfig) -> Result<Self> {
        if !config.is_valid() {
            return Err(AgentError::configuration(
                "LINE channel access token is required",
            ));
        }

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| {
                AgentError::configuration(format!("Failed to create HTTP client: {}", e))
            })?;

        Ok(Self {
            config,
            client,
            connected: false,
        })
    }

    /// Build LINE message from Message
    fn build_line_message(message: &Message) -> serde_json::Value {
        match message.message_type {
            MessageType::Image => {
                // For images, we'd need to upload first
                serde_json::json!({
                    "type": "text",
                    "text": format!("[Image] {}", message.content)
                })
            }
            MessageType::Voice | MessageType::Video => {
                serde_json::json!({
                    "type": "text",
                    "text": format!("[Media] {}", message.content)
                })
            }
            MessageType::File => {
                serde_json::json!({
                    "type": "text",
                    "text": format!("[File] {}", message.content)
                })
            }
            _ => {
                serde_json::json!({
                    "type": "text",
                    "text": message.content
                })
            }
        }
    }

    /// Send push message to user
    async fn send_push_message(&self, to: &str, message: &Message) -> Result<()> {
        let url = format!("{}/bot/message/push", LINE_API_BASE);

        let line_message = Self::build_line_message(message);

        let request = serde_json::json!({
            "to": to,
            "messages": [line_message]
        });

        let response = self
            .client
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.config.channel_access_token),
            )
            .json(&request)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to send message: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!(
                "LINE API error: {}",
                error_text
            )));
        }

        debug!("Message sent to LINE user: {}", to);
        Ok(())
    }

    /// Send reply message
    #[allow(dead_code)]
    async fn send_reply_message(&self, reply_token: &str, message: &Message) -> Result<()> {
        let url = format!("{}/bot/message/reply", LINE_API_BASE);

        let line_message = Self::build_line_message(message);

        let request = serde_json::json!({
            "replyToken": reply_token,
            "messages": [line_message]
        });

        let response = self
            .client
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.config.channel_access_token),
            )
            .json(&request)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to send reply: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!(
                "LINE API error: {}",
                error_text
            )));
        }

        debug!("Reply sent to LINE");
        Ok(())
    }

    /// Broadcast message to all friends
    async fn broadcast_message(&self, message: &Message) -> Result<()> {
        let url = format!("{}/bot/message/broadcast", LINE_API_BASE);

        let line_message = Self::build_line_message(message);

        let request = serde_json::json!({
            "messages": [line_message]
        });

        let response = self
            .client
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.config.channel_access_token),
            )
            .json(&request)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to broadcast: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!(
                "LINE API error: {}",
                error_text
            )));
        }

        debug!("Broadcast sent to LINE");
        Ok(())
    }

    /// Get user profile
    #[allow(dead_code)]
    async fn get_user_profile(&self, user_id: &str) -> Result<LineUserProfile> {
        let url = format!("{}/bot/profile/{}", LINE_API_BASE, user_id);

        let response = self
            .client
            .get(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.config.channel_access_token),
            )
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to get profile: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!(
                "LINE API error: {}",
                error_text
            )));
        }

        let profile: LineUserProfile = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse profile: {}", e)))?;

        Ok(profile)
    }

    /// Send rich menu (interactive buttons)
    pub async fn send_rich_menu(
        &self,
        to: &str,
        alt_text: &str,
        actions: Vec<LineAction>,
    ) -> Result<()> {
        let url = format!("{}/bot/message/push", LINE_API_BASE);

        // Build template message
        let template = serde_json::json!({
            "type": "template",
            "altText": alt_text,
            "template": {
                "type": "buttons",
                "text": alt_text,
                "actions": actions.iter().map(|a| a.to_json()).collect::<Vec<_>>()
            }
        });

        let request = serde_json::json!({
            "to": to,
            "messages": [template]
        });

        let response = self
            .client
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.config.channel_access_token),
            )
            .json(&request)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to send rich menu: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!(
                "LINE API error: {}",
                error_text
            )));
        }

        Ok(())
    }

    /// Verify webhook signature
    pub fn verify_webhook_signature(&self, body: &[u8], signature: &str) -> Result<bool> {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let secret = self
            .config
            .channel_secret
            .as_ref()
            .ok_or_else(|| AgentError::configuration("Channel secret not configured"))?;

        type HmacSha256 = Hmac<Sha256>;

        let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
            .map_err(|e| AgentError::configuration(format!("Invalid secret: {}", e)))?;

        mac.update(body);

        let result = mac.finalize();
        let expected_signature = hex::encode(result.into_bytes());

        Ok(expected_signature == signature)
    }
}

#[async_trait]
impl Channel for LineChannel {
    fn name(&self) -> &str {
        "line"
    }

    fn platform(&self) -> PlatformType {
        PlatformType::Line
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    async fn connect(&mut self) -> Result<()> {
        info!("Connecting to LINE Messaging API...");

        // Validate token by making a test API call
        let url = format!("{}/bot/info", LINE_API_BASE);
        let response = self
            .client
            .get(&url)
            .header(
                "Authorization",
                format!("Bearer {}", self.config.channel_access_token),
            )
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to connect: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AgentError::authentication(format!(
                "LINE authentication failed: {}",
                error_text
            )));
        }

        self.connected = true;
        info!("Connected to LINE Messaging API");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from LINE...");
        self.connected = false;
        info!("Disconnected from LINE");
        Ok(())
    }

    async fn send(&self, channel_id: &str, message: &Message) -> Result<()> {
        if !self.connected {
            return Err(AgentError::platform("Not connected to LINE"));
        }

        if channel_id.is_empty() {
            // Broadcast if no specific channel
            self.broadcast_message(message).await
        } else {
            self.send_push_message(channel_id, message).await
        }
    }

    async fn start_listener(&self, _event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        info!("LINE uses webhook push notifications");
        info!("Please configure webhook endpoint at: https://your-domain/line/webhook");
        Ok(())
    }

    async fn stop_listener(&self) -> Result<()> {
        Ok(())
    }

    fn supported_content_types(&self) -> Vec<ContentType> {
        vec![
            ContentType::Text,
            ContentType::Image,
            ContentType::Video,
            ContentType::Audio,
            ContentType::File,
            ContentType::Location,
            ContentType::Sticker,
        ]
    }

    async fn list_channels(&self) -> Result<Vec<ChannelInfo>> {
        // LINE doesn't have a traditional channel concept like Slack
        // Users and groups are the channels
        // Would need to get from user database or cache
        Ok(vec![])
    }

    async fn list_members(&self, _channel_id: &str) -> Result<Vec<MemberInfo>> {
        // LINE group/room member list requires separate API call
        Ok(vec![])
    }

    fn connection_mode(&self) -> ConnectionMode {
        self.config.connection_mode()
    }
}

/// LINE user profile
#[derive(Debug, Clone, Deserialize)]
pub struct LineUserProfile {
    pub user_id: String,
    pub display_name: String,
    pub picture_url: Option<String>,
    pub status_message: Option<String>,
    pub language: Option<String>,
}

/// LINE action for rich menu
#[derive(Debug, Clone)]
pub enum LineAction {
    Postback { label: String, data: String },
    Message { label: String, text: String },
    URI { label: String, uri: String },
}

impl LineAction {
    fn to_json(&self) -> serde_json::Value {
        match self {
            LineAction::Postback { label, data } => serde_json::json!({
                "type": "postback",
                "label": label,
                "data": data
            }),
            LineAction::Message { label, text } => serde_json::json!({
                "type": "message",
                "label": label,
                "text": text
            }),
            LineAction::URI { label, uri } => serde_json::json!({
                "type": "uri",
                "label": label,
                "uri": uri
            }),
        }
    }
}

/// LINE webhook handler
pub struct LineWebhookHandler {
    event_sender: mpsc::Sender<ChannelEvent>,
    channel: LineChannel,
}

impl LineWebhookHandler {
    pub fn new(event_sender: mpsc::Sender<ChannelEvent>, channel: LineChannel) -> Self {
        Self {
            event_sender,
            channel,
        }
    }

    /// Handle incoming webhook request
    pub async fn handle_request(&self, body: &[u8], signature: &str) -> Result<()> {
        // Verify signature
        if !self.channel.verify_webhook_signature(body, signature)? {
            return Err(AgentError::authentication("Invalid webhook signature"));
        }

        let events: LineWebhookBody = serde_json::from_slice(body)
            .map_err(|e| AgentError::platform(format!("Invalid webhook payload: {}", e)))?;

        for event in events.events {
            match event.type_.as_str() {
                "message" => {
                    if let Some(message) = event.message {
                        let content = match message.type_.as_str() {
                            "text" => message.text.unwrap_or_default(),
                            "image" => "[Image message]".to_string(),
                            "video" => "[Video message]".to_string(),
                            "audio" => "[Audio message]".to_string(),
                            "file" => "[File message]".to_string(),
                            "location" => "[Location message]".to_string(),
                            "sticker" => "[Sticker message]".to_string(),
                            _ => "[Unknown message type]".to_string(),
                        };

                        let channel_event = ChannelEvent::MessageReceived {
                            platform: PlatformType::Line,
                            channel_id: event.source.user_id.unwrap_or_default(),
                            message: Message::new(
                                uuid::Uuid::new_v4(),
                                PlatformType::Line,
                                content,
                            ),
                        };

                        self.event_sender.send(channel_event).await.map_err(|e| {
                            AgentError::platform(format!("Failed to send event: {}", e))
                        })?;
                    }
                }
                "follow" => {
                    debug!("New follower");
                }
                "unfollow" => {
                    debug!("User unfollowed");
                }
                _ => {
                    debug!("Unhandled LINE event type: {}", event.type_);
                }
            }
        }

        Ok(())
    }
}

/// LINE webhook body
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct LineWebhookBody {
    events: Vec<LineEvent>,
}

/// LINE event
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct LineEvent {
    #[serde(rename = "type")]
    type_: String,
    source: LineSource,
    message: Option<LineMessageEvent>,
    #[serde(rename = "replyToken")]
    reply_token: Option<String>,
    timestamp: i64,
}

/// LINE source
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct LineSource {
    #[serde(rename = "type")]
    type_: String,
    #[serde(rename = "userId")]
    user_id: Option<String>,
    #[serde(rename = "groupId")]
    group_id: Option<String>,
    #[serde(rename = "roomId")]
    room_id: Option<String>,
}

/// LINE message event
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct LineMessageEvent {
    #[serde(rename = "type")]
    type_: String,
    id: String,
    text: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_config_validation() {
        let invalid_config = LineConfig::default();
        assert!(!invalid_config.is_valid());

        let valid_config = LineConfig {
            channel_access_token: "test_token".to_string(),
            ..Default::default()
        };
        assert!(valid_config.is_valid());
    }

    #[test]
    fn test_line_action_json() {
        let action = LineAction::Message {
            label: "Test".to_string(),
            text: "Hello".to_string(),
        };
        let json = action.to_json();
        assert_eq!(json["type"], "message");
        assert_eq!(json["label"], "Test");
    }
}
