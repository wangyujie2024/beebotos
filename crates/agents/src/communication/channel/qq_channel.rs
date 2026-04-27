//! QQ Channel Implementation
//!
//! Provides integration with QQ messaging platform through
//! go-cqhttp or OneBot protocol.
//!
//! # Features
//! - Private messaging
//! - Group chat support
//! - Friend management
//! - File and image sharing
//! - Group member management
//!
//! # API Reference
//! - OneBot Protocol: <https://github.com/howmanybots/onebot>
//! - go-cqhttp: <https://docs.go-cqhttp.org/>

use std::collections::HashMap;

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, info};

use super::r#trait::{
    BaseChannelConfig, Channel, ChannelConfig, ChannelEvent, ChannelInfo, ChannelType,
    ConnectionMode, ContentType, MemberInfo, MemberRole,
};
use crate::communication::{Message, PlatformType};
use crate::error::{AgentError, Result};

/// OneBot API base URL
const ONEBOT_API_BASE: &str = "http://localhost:5700";

/// QQ channel implementation
pub struct QQChannel {
    config: QQConfig,
    client: Client,
    connected: bool,
    #[allow(dead_code)]
    event_sender: Option<mpsc::Sender<ChannelEvent>>,
}

/// QQ configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QQConfig {
    /// OneBot API base URL (default: http://localhost:5700)
    pub api_base: String,
    /// Webhook secret for verification
    pub webhook_secret: Option<String>,
    /// Bot QQ ID
    pub bot_qq_id: Option<i64>,
    #[serde(flatten)]
    pub base: BaseChannelConfig,
}

impl QQConfig {
    /// Create configuration from environment variables
    pub fn from_env() -> Option<Self> {
        let api_base = std::env::var("QQ_API_BASE").ok()?;
        let webhook_secret = std::env::var("QQ_WEBHOOK_SECRET").ok();
        let bot_qq_id = std::env::var("QQ_BOT_ID").ok()?.parse().ok();
        let base = BaseChannelConfig::from_env("QQ")?;

        Some(Self {
            api_base,
            webhook_secret,
            bot_qq_id,
            base,
        })
    }

    /// Validate configuration
    pub fn is_valid(&self) -> bool {
        !self.api_base.is_empty()
    }
}

impl Default for QQConfig {
    fn default() -> Self {
        let mut base = BaseChannelConfig::default();
        // QQ uses Webhook mode for receiving messages
        base.connection_mode = ConnectionMode::Webhook;

        Self {
            api_base: ONEBOT_API_BASE.to_string(),
            webhook_secret: None,
            bot_qq_id: None,
            base,
        }
    }
}

impl ChannelConfig for QQConfig {
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

/// OneBot API response
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OneBotResponse {
    status: String,
    retcode: i32,
    data: Option<serde_json::Value>,
    echo: Option<String>,
}

/// QQ message request
#[derive(Debug, Serialize)]
struct QQMessageRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    user_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    group_id: Option<i64>,
    message: Vec<QQMessageSegment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auto_escape: Option<bool>,
}

/// QQ message segment
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum QQMessageSegment {
    Text { data: TextData },
    Image { data: ImageData },
    At { data: AtData },
    Face { data: FaceData },
    Reply { data: ReplyData },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TextData {
    text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ImageData {
    file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AtData {
    qq: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FaceData {
    id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReplyData {
    id: String,
}

/// OneBot webhook event
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OneBotEvent {
    #[serde(rename = "post_type")]
    post_type: String,
    #[serde(rename = "message_type")]
    message_type: Option<String>,
    #[serde(rename = "user_id")]
    user_id: Option<i64>,
    #[serde(rename = "group_id")]
    group_id: Option<i64>,
    message: Option<serde_json::Value>,
    raw_message: Option<String>,
    sender: Option<OneBotSender>,
    #[serde(rename = "message_id")]
    message_id: Option<i32>,
}

/// OneBot sender info
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OneBotSender {
    #[serde(rename = "user_id")]
    user_id: i64,
    nickname: String,
    #[serde(rename = "card")]
    card: Option<String>,
    role: Option<String>,
}

impl QQChannel {
    /// Create a new QQ channel
    pub fn new(config: QQConfig) -> Result<Self> {
        if !config.is_valid() {
            return Err(AgentError::configuration("Invalid QQ configuration"));
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
            event_sender: None,
        })
    }

    /// Build QQ message segments from content
    fn build_message_segments(content: &str) -> Vec<QQMessageSegment> {
        vec![QQMessageSegment::Text {
            data: TextData {
                text: content.to_string(),
            },
        }]
    }

    /// Build at segment
    #[allow(dead_code)]
    fn build_at_segment(qq_id: i64) -> QQMessageSegment {
        QQMessageSegment::At {
            data: AtData {
                qq: qq_id.to_string(),
            },
        }
    }

    /// Send private message
    async fn send_private_message(&self, user_id: i64, message: &Message) -> Result<()> {
        let url = format!("{}/send_private_msg", self.config.api_base);

        let segments = Self::build_message_segments(&message.content);
        let request = QQMessageRequest {
            user_id: Some(user_id),
            group_id: None,
            message: segments,
            auto_escape: Some(false),
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to send message: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!(
                "OneBot API error: {}",
                error_text
            )));
        }

        debug!("Private message sent to QQ user: {}", user_id);
        Ok(())
    }

    /// Send group message
    async fn send_group_message(&self, group_id: i64, message: &Message) -> Result<()> {
        let url = format!("{}/send_group_msg", self.config.api_base);

        let segments = Self::build_message_segments(&message.content);
        let request = QQMessageRequest {
            user_id: None,
            group_id: Some(group_id),
            message: segments,
            auto_escape: Some(false),
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to send message: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!(
                "OneBot API error: {}",
                error_text
            )));
        }

        debug!("Group message sent to QQ group: {}", group_id);
        Ok(())
    }

    /// Get group member list
    async fn get_group_member_list(&self, group_id: i64) -> Result<Vec<MemberInfo>> {
        let url = format!("{}/get_group_member_list", self.config.api_base);

        let response = self
            .client
            .get(&url)
            .query(&[("group_id", group_id.to_string())])
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to get members: {}", e)))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let data: OneBotResponse = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        let members: Vec<MemberInfo> = data
            .data
            .and_then(|d| d.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|m| {
                let user_id = m.get("user_id")?.as_i64()?;
                let nickname = m.get("nickname")?.as_str()?.to_string();
                let role = m.get("role")?.as_str().unwrap_or("member");

                let member_role = match role {
                    "owner" => MemberRole::Owner,
                    "admin" => MemberRole::Admin,
                    _ => MemberRole::Member,
                };

                Some(MemberInfo {
                    id: user_id.to_string(),
                    name: nickname.clone(),
                    username: Some(nickname),
                    avatar: None,
                    is_bot: false,
                    role: member_role,
                })
            })
            .collect();

        Ok(members)
    }

    /// Get friend list
    async fn get_friend_list(&self) -> Result<Vec<ChannelInfo>> {
        let url = format!("{}/get_friend_list", self.config.api_base);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to get friends: {}", e)))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let data: OneBotResponse = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        let friends: Vec<ChannelInfo> = data
            .data
            .and_then(|d| d.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|f| {
                let user_id = f.get("user_id")?.as_i64()?;
                let nickname = f.get("nickname")?.as_str()?.to_string();

                Some(ChannelInfo {
                    id: user_id.to_string(),
                    name: nickname,
                    channel_type: ChannelType::Direct,
                    unread_count: 0,
                    metadata: HashMap::new(),
                })
            })
            .collect();

        Ok(friends)
    }

    /// Get group list
    async fn get_group_list(&self) -> Result<Vec<ChannelInfo>> {
        let url = format!("{}/get_group_list", self.config.api_base);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to get groups: {}", e)))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let data: OneBotResponse = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        let groups: Vec<ChannelInfo> = data
            .data
            .and_then(|d| d.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|g| {
                let group_id = g.get("group_id")?.as_i64()?;
                let group_name = g.get("group_name")?.as_str()?.to_string();

                Some(ChannelInfo {
                    id: group_id.to_string(),
                    name: group_name,
                    channel_type: ChannelType::Group,
                    unread_count: 0,
                    metadata: HashMap::new(),
                })
            })
            .collect();

        Ok(groups)
    }

    /// Handle OneBot webhook event
    pub async fn handle_webhook(&self, payload: &[u8]) -> Result<Option<Message>> {
        let event: OneBotEvent = serde_json::from_slice(payload)
            .map_err(|e| AgentError::platform(format!("Invalid webhook payload: {}", e)))?;

        if event.post_type != "message" {
            return Ok(None);
        }

        let message_type = event.message_type.as_deref().unwrap_or("");
        let content = event.raw_message.clone().unwrap_or_default();

        let (channel_id, sender_id) = match message_type {
            "private" => {
                let user_id = event.user_id.unwrap_or(0);
                (user_id.to_string(), user_id.to_string())
            }
            "group" => {
                let group_id = event.group_id.unwrap_or(0);
                let user_id = event.user_id.unwrap_or(0);
                (group_id.to_string(), user_id.to_string())
            }
            _ => return Ok(None),
        };

        // Skip messages from self
        if let Some(bot_id) = self.config.bot_qq_id {
            if sender_id == bot_id.to_string() {
                return Ok(None);
            }
        }

        let message = Message::new(uuid::Uuid::new_v4(), PlatformType::QQ, content);

        info!("Received QQ message from {} in {}", sender_id, channel_id);

        Ok(Some(message))
    }

    /// Verify webhook signature
    pub fn verify_signature(&self, _payload: &[u8], _signature: &str) -> bool {
        // OneBot doesn't use signature verification by default
        // Can be implemented with HMAC if needed
        true
    }
}

#[async_trait]
impl Channel for QQChannel {
    fn name(&self) -> &str {
        "qq"
    }

    fn platform(&self) -> PlatformType {
        PlatformType::QQ
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    async fn connect(&mut self) -> Result<()> {
        info!("Connecting to QQ (OneBot)...");

        // Test connection by getting version
        let url = format!("{}/get_version_info", self.config.api_base);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to connect: {}", e)))?;

        if !response.status().is_success() {
            return Err(AgentError::platform("OneBot API not available"));
        }

        self.connected = true;
        info!("Connected to QQ via OneBot");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from QQ...");
        self.connected = false;
        Ok(())
    }

    async fn send(&self, channel_id: &str, message: &Message) -> Result<()> {
        let id = channel_id
            .parse::<i64>()
            .map_err(|_| AgentError::platform("Invalid QQ ID"))?;

        // Try group first, then private
        if let Err(_) = self.send_group_message(id, message).await {
            self.send_private_message(id, message).await?;
        }

        Ok(())
    }

    async fn start_listener(&self, _event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        // Listener is via webhook
        Ok(())
    }

    async fn stop_listener(&self) -> Result<()> {
        Ok(())
    }

    fn supported_content_types(&self) -> Vec<ContentType> {
        vec![ContentType::Text, ContentType::Image]
    }

    fn connection_mode(&self) -> ConnectionMode {
        self.config.base.connection_mode
    }

    async fn download_image(
        &self,
        _file_key: &str,
        _message_id: Option<&str>,
    ) -> crate::error::Result<Vec<u8>> {
        Err(crate::error::AgentError::platform(
            "Image download not implemented for QQ",
        ))
    }

    async fn list_channels(&self) -> Result<Vec<ChannelInfo>> {
        let mut channels = Vec::new();
        channels.extend(self.get_friend_list().await?);
        channels.extend(self.get_group_list().await?);
        Ok(channels)
    }

    async fn list_members(&self, channel_id: &str) -> Result<Vec<MemberInfo>> {
        let group_id = channel_id
            .parse::<i64>()
            .map_err(|_| AgentError::platform("Invalid group ID"))?;

        self.get_group_member_list(group_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qq_config_default() {
        let config = QQConfig::default();
        assert_eq!(config.api_base, ONEBOT_API_BASE);
        assert!(config.base.auto_reconnect);
    }

    #[test]
    fn test_build_message_segments() {
        let segments = QQChannel::build_message_segments("Hello QQ");
        assert_eq!(segments.len(), 1);
    }

    #[test]
    fn test_build_at_segment() {
        let segment = QQChannel::build_at_segment(123456);
        match segment {
            QQMessageSegment::At { data } => assert_eq!(data.qq, "123456"),
            _ => panic!("Expected At segment"),
        }
    }
}
