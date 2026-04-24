//! WeChat Work Channel Implementation
//!
//! Unified Channel trait implementation for WeChat Work (企业微信).
//! Uses HTTP API for sending messages and webhook for receiving.
//!
//! 🔧 P0 FIX: Added LinkHandler and CommandHandler for feature parity with
//! OpenClaw

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json;
use tokio::sync::{mpsc, RwLock};
use tokio::time::Duration;
use tracing::{debug, error, info, warn};

use super::r#trait::{BaseChannelConfig, ConnectionMode, ContentType};
use super::{Channel, ChannelConfig, ChannelEvent, ChannelInfo, MemberInfo};
use crate::communication::{Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};
use crate::skills::command_handler::{CommandContext, CommandHandler, CommandResult};
use crate::skills::link_handler::{format_summary_for_display, LinkHandler};

/// WeChat Work API base URL
const WECHAT_API_BASE: &str = "https://qyapi.weixin.qq.com/cgi-bin";

/// Deserialize string or integer as string
fn deserialize_agent_id<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(s) => Ok(s),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        _ => Err(D::Error::custom("expected string or number for agent_id")),
    }
}

/// WeChat Work Channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeChatChannelConfig {
    /// Corp ID (企业ID)
    pub corp_id: String,
    /// Corp Secret (应用凭证密钥)
    #[serde(alias = "secret")]
    pub corp_secret: String,
    /// Agent ID (应用ID)
    #[serde(deserialize_with = "deserialize_agent_id")]
    pub agent_id: String,
    /// Base channel configuration
    #[serde(flatten)]
    pub base: BaseChannelConfig,
}

impl Default for WeChatChannelConfig {
    fn default() -> Self {
        Self {
            corp_id: String::new(),
            corp_secret: String::new(),
            agent_id: String::new(),
            base: BaseChannelConfig {
                connection_mode: ConnectionMode::Webhook,
                ..Default::default()
            },
        }
    }
}

impl ChannelConfig for WeChatChannelConfig {
    fn from_env() -> Option<Self>
    where
        Self: Sized,
    {
        let corp_id = std::env::var("WECHAT_CORP_ID").ok()?;
        let corp_secret = std::env::var("WECHAT_CORP_SECRET").ok()?;
        let agent_id = std::env::var("WECHAT_AGENT_ID").ok()?;

        let mut base = BaseChannelConfig::from_env("WECHAT").unwrap_or_default();
        // WeChat default is Webhook
        if std::env::var("WECHAT_CONNECTION_MODE").is_err() {
            base.connection_mode = ConnectionMode::Webhook;
        }

        Some(Self {
            corp_id,
            corp_secret,
            agent_id,
            base,
        })
    }

    fn is_valid(&self) -> bool {
        !self.corp_id.is_empty() && !self.corp_secret.is_empty() && !self.agent_id.is_empty()
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

/// WeChat token response
#[derive(Debug, Clone, Deserialize)]
pub struct WeChatTokenResponse {
    pub errcode: i32,
    pub errmsg: String,
    #[serde(rename = "access_token")]
    pub access_token: Option<String>,
    #[serde(rename = "expires_in")]
    pub expires_in: Option<i64>,
}

/// WeChat send response
#[derive(Debug, Clone, Deserialize)]
pub struct WeChatSendResponse {
    pub errcode: i32,
    pub errmsg: String,
    #[serde(rename = "invaliduser")]
    pub invalid_user: Option<String>,
}

/// WeChat message recipient
#[derive(Debug, Clone, Serialize)]
pub struct WeChatRecipient {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub touser: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toparty: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub totag: Option<String>,
    #[serde(rename = "msgtype")]
    pub msg_type: String,
    #[serde(rename = "agentid")]
    pub agent_id: String,
    #[serde(flatten)]
    pub content: serde_json::Value,
}

/// Token cache entry
#[derive(Debug, Clone)]
struct TokenCache {
    token: String,
    expires_at: chrono::DateTime<chrono::Utc>,
}

/// WeChat Channel implementation
///
/// 🔧 P0 FIX: Integrated LinkHandler and CommandHandler for enhanced
/// functionality
pub struct WeChatChannel {
    config: WeChatChannelConfig,
    http_client: reqwest::Client,
    access_token: Arc<RwLock<Option<TokenCache>>>,
    connected: Arc<RwLock<bool>>,
    listener_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    /// Link handler for URL summarization
    link_handler: Option<Arc<LinkHandler>>,
    /// Command handler for bot commands
    command_handler: Option<Arc<CommandHandler>>,
}

impl WeChatChannel {
    /// Create a new WeChat channel
    pub fn new(config: WeChatChannelConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
            access_token: Arc::new(RwLock::new(None)),
            connected: Arc::new(RwLock::new(false)),
            listener_handle: Arc::new(RwLock::new(None)),
            link_handler: None,
            command_handler: None,
        }
    }

    /// Create from environment variables
    pub fn from_env() -> Result<Self> {
        let config = WeChatChannelConfig::from_env().ok_or_else(|| {
            AgentError::configuration(
                "WECHAT_CORP_ID, WECHAT_CORP_SECRET, or WECHAT_AGENT_ID not set",
            )
        })?;
        Ok(Self::new(config))
    }

    /// 🔧 P0 FIX: Set link handler for URL summarization
    pub fn with_link_handler(mut self, handler: Arc<LinkHandler>) -> Self {
        self.link_handler = Some(handler);
        self
    }

    /// 🔧 P0 FIX: Set command handler for bot commands
    pub fn with_command_handler(mut self, handler: Arc<CommandHandler>) -> Self {
        self.command_handler = Some(handler);
        self
    }

    /// 🔧 P0 FIX: Handle incoming message with link detection and command
    /// processing
    pub async fn handle_message(&self, message: &Message) -> Result<Option<String>> {
        let content = &message.content;
        let sender = message
            .metadata
            .get("from_user")
            .map(|s| s.as_str())
            .unwrap_or("unknown");

        // Check if it's a command
        if let Some(ref cmd_handler) = self.command_handler {
            if content.starts_with('/') {
                let ctx = CommandContext {
                    sender_id: sender.to_string(),
                    channel: "wechat".to_string(),
                    metadata: message.metadata.clone(),
                };

                match cmd_handler.execute(content, ctx).await {
                    CommandResult::Success(response) => return Ok(Some(response)),
                    CommandResult::NotFound => {} // Continue to other handlers
                    CommandResult::Error(e) => {
                        return Ok(Some(format!("❌ 命令执行失败: {}", e)));
                    }
                }
            }
        }

        // Check if content contains a URL
        if let Some(ref link_handler) = self.link_handler {
            if let Some(url) = self.extract_url(content) {
                info!("Detected URL in message: {}", url);

                // Send "processing" message
                let _ = self
                    .send_text_message(sender, "⏳ 正在处理链接,请稍候...")
                    .await;

                match link_handler.process(&url).await {
                    Ok(summary) => {
                        let formatted = format_summary_for_display(&summary);
                        return Ok(Some(formatted));
                    }
                    Err(e) => {
                        warn!("Failed to process link {}: {}", url, e);
                        return Ok(Some(format!("❌ 链接处理失败: {}", e)));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Extract URL from text content
    fn extract_url(&self, text: &str) -> Option<String> {
        // Simple URL regex pattern
        let url_regex = regex::Regex::new(
            r"https?://[a-zA-Z0-9][-a-zA-Z0-9]*[\.a-zA-Z0-9]*[a-zA-Z0-9][a-zA-Z0-9._/-]*",
        )
        .ok()?;

        url_regex.find(text).map(|m| m.as_str().to_string())
    }

    /// Get access token (with auto-refresh)
    async fn get_access_token(&self) -> Result<String> {
        // Check if we have a valid cached token
        {
            let cache = self.access_token.read().await;
            if let Some(token_cache) = cache.as_ref() {
                if token_cache.expires_at > chrono::Utc::now() + chrono::Duration::minutes(5) {
                    debug!("Using cached WeChat access token");
                    return Ok(token_cache.token.clone());
                }
            }
        }

        // Need to fetch new token
        self.refresh_access_token().await
    }

    /// Refresh access token
    async fn refresh_access_token(&self) -> Result<String> {
        info!("Refreshing WeChat access token");

        let url = format!(
            "{}/gettoken?corpid={}&corpsecret={}",
            WECHAT_API_BASE, self.config.corp_id, self.config.corp_secret
        );

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to get access token: {}", e)))?;

        let token_response: WeChatTokenResponse = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse token response: {}", e)))?;

        if token_response.errcode != 0 {
            return Err(AgentError::authentication(format!(
                "WeChat auth failed: {} (code: {})",
                token_response.errmsg, token_response.errcode
            ))
            .into());
        }

        let token = token_response
            .access_token
            .ok_or_else(|| AgentError::authentication("No access token in response"))?;

        let expires_in = token_response.expires_in.unwrap_or(7200);
        let expires_at = chrono::Utc::now() + chrono::Duration::seconds(expires_in);

        // Cache the token
        let mut cache = self.access_token.write().await;
        *cache = Some(TokenCache {
            token: token.clone(),
            expires_at,
        });

        info!("WeChat access token refreshed successfully");
        Ok(token)
    }

    /// Send message
    async fn send_message(&self, recipient: WeChatRecipient) -> Result<()> {
        info!("Sending WeChat message, getting access token...");
        let token = self.get_access_token().await?;
        info!("Got access token, sending HTTP request...");
        let url = format!("{}/message/send?access_token={}", WECHAT_API_BASE, token);

        let response = self
            .http_client
            .post(&url)
            .json(&recipient)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to send message: {}", e)))?;

        let send_response: WeChatSendResponse = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse send response: {}", e)))?;

        info!(
            "WeChat send response: errcode={}, errmsg={}",
            send_response.errcode, send_response.errmsg
        );

        if send_response.errcode != 0 {
            return Err(AgentError::platform(format!(
                "Failed to send message: {} (code: {})",
                send_response.errmsg, send_response.errcode
            ))
            .into());
        }

        if let Some(invalid) = send_response.invalid_user {
            warn!("Invalid users: {}", invalid);
        }

        info!("WeChat message sent successfully");
        Ok(())
    }

    /// Send text message to user
    pub async fn send_text_message(&self, to_user: &str, content: &str) -> Result<()> {
        let recipient = WeChatRecipient {
            touser: Some(to_user.to_string()),
            toparty: None,
            totag: None,
            msg_type: "text".to_string(),
            agent_id: self.config.agent_id.clone(),
            content: serde_json::json!({
                "text": {
                    "content": content
                }
            }),
        };

        self.send_message(recipient).await
    }

    /// 🔧 P0 FIX: Send image message to user
    ///
    /// # Arguments
    /// * `to_user` - Target user ID
    /// * `media_id` - Media ID obtained from upload_temp_media or
    ///   upload_permanent_media
    pub async fn send_image(&self, to_user: &str, media_id: &str) -> Result<()> {
        let recipient = WeChatRecipient {
            touser: Some(to_user.to_string()),
            toparty: None,
            totag: None,
            msg_type: "image".to_string(),
            agent_id: self.config.agent_id.clone(),
            content: serde_json::json!({
                "image": {
                    "media_id": media_id
                }
            }),
        };

        self.send_message(recipient).await
    }

    /// 🔧 P0 FIX: Send file message to user
    ///
    /// # Arguments
    /// * `to_user` - Target user ID
    /// * `media_id` - Media ID of the file
    pub async fn send_file(&self, to_user: &str, media_id: &str) -> Result<()> {
        let recipient = WeChatRecipient {
            touser: Some(to_user.to_string()),
            toparty: None,
            totag: None,
            msg_type: "file".to_string(),
            agent_id: self.config.agent_id.clone(),
            content: serde_json::json!({
                "file": {
                    "media_id": media_id
                }
            }),
        };

        self.send_message(recipient).await
    }

    /// 🔧 P0 FIX: Send video message to user
    ///
    /// # Arguments
    /// * `to_user` - Target user ID
    /// * `media_id` - Media ID of the video
    /// * `title` - Video title (optional)
    /// * `description` - Video description (optional)
    pub async fn send_video(
        &self,
        to_user: &str,
        media_id: &str,
        title: Option<&str>,
        description: Option<&str>,
    ) -> Result<()> {
        let mut video_content = serde_json::json!({
            "media_id": media_id
        });

        if let Some(t) = title {
            video_content["title"] = serde_json::json!(t);
        }
        if let Some(d) = description {
            video_content["description"] = serde_json::json!(d);
        }

        let recipient = WeChatRecipient {
            touser: Some(to_user.to_string()),
            toparty: None,
            totag: None,
            msg_type: "video".to_string(),
            agent_id: self.config.agent_id.clone(),
            content: serde_json::json!({ "video": video_content }),
        };

        self.send_message(recipient).await
    }

    /// 🔧 P0 FIX: Send voice message to user
    ///
    /// # Arguments
    /// * `to_user` - Target user ID
    /// * `media_id` - Media ID of the voice file
    pub async fn send_voice(&self, to_user: &str, media_id: &str) -> Result<()> {
        let recipient = WeChatRecipient {
            touser: Some(to_user.to_string()),
            toparty: None,
            totag: None,
            msg_type: "voice".to_string(),
            agent_id: self.config.agent_id.clone(),
            content: serde_json::json!({
                "voice": {
                    "media_id": media_id
                }
            }),
        };

        self.send_message(recipient).await
    }

    /// 🔧 P0 FIX: Send news/article message to user
    ///
    /// # Arguments
    /// * `to_user` - Target user ID
    /// * `articles` - List of articles (max 8 for WeChat Work)
    pub async fn send_news(
        &self,
        to_user: &str,
        articles: Vec<super::wechat_content::WeChatArticle>,
    ) -> Result<()> {
        #[allow(unused_imports)]
        use super::wechat_content::WeChatArticle;

        if articles.is_empty() || articles.len() > 8 {
            return Err(AgentError::invalid_input(
                "Articles count must be between 1 and 8",
            ));
        }

        let recipient = WeChatRecipient {
            touser: Some(to_user.to_string()),
            toparty: None,
            totag: None,
            msg_type: "news".to_string(),
            agent_id: self.config.agent_id.clone(),
            content: serde_json::json!({
                "news": {
                    "articles": articles.iter().map(|a| {
                        serde_json::json!({
                            "title": a.title,
                            "description": a.description,
                            "url": a.url,
                            "picurl": a.pic_url
                        })
                    }).collect::<Vec<_>>()
                }
            }),
        };

        self.send_message(recipient).await
    }

    /// 🔧 P0 FIX: Send markdown message to user (WeChat Work only)
    ///
    /// # Arguments
    /// * `to_user` - Target user ID
    /// * `content` - Markdown content
    pub async fn send_markdown(&self, to_user: &str, content: &str) -> Result<()> {
        let recipient = WeChatRecipient {
            touser: Some(to_user.to_string()),
            toparty: None,
            totag: None,
            msg_type: "markdown".to_string(),
            agent_id: self.config.agent_id.clone(),
            content: serde_json::json!({
                "markdown": {
                    "content": content
                }
            }),
        };

        self.send_message(recipient).await
    }

    /// 🔧 P0 FIX: Send message to multiple users (broadcast)
    ///
    /// # Arguments
    /// * `to_users` - List of target user IDs (max 1000)
    /// * `message_fn` - Function that creates the message recipient
    pub async fn send_to_multiple<F>(&self, to_users: Vec<String>, message_fn: F) -> Result<()>
    where
        F: Fn(&str) -> WeChatRecipient,
    {
        if to_users.is_empty() {
            return Ok(());
        }

        // WeChat allows max 1000 users per request
        for chunk in to_users.chunks(1000) {
            let user_list = chunk.join("|");
            let recipient = message_fn(&user_list);
            self.send_message(recipient).await?;
        }

        Ok(())
    }

    /// Run webhook listener
    async fn run_webhook_listener(&self, _event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        info!(
            "WeChat webhook listener started on port {}",
            self.config.base.webhook_port
        );
        // TODO: Implement HTTP server for webhook
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }

    /// Run polling listener (WeChat doesn't support polling, use webhook)
    #[allow(dead_code)]
    async fn run_polling_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        warn!("WeChat does not support polling mode, using webhook instead");
        self.run_webhook_listener(event_bus).await
    }
}

#[async_trait]
impl Channel for WeChatChannel {
    fn name(&self) -> &str {
        "wechat"
    }

    fn platform(&self) -> PlatformType {
        PlatformType::WeChat
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn is_connected(&self) -> bool {
        if let Ok(connected) = self.connected.try_read() {
            *connected
        } else {
            false
        }
    }

    async fn connect(&mut self) -> Result<()> {
        // Verify credentials by getting access token
        self.get_access_token().await?;
        *self.connected.write().await = true;
        info!("WeChat Work channel connected successfully");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.stop_listener().await?;
        *self.access_token.write().await = None;
        *self.connected.write().await = false;
        info!("Disconnected from WeChat Work");
        Ok(())
    }

    async fn send(&self, channel_id: &str, message: &Message) -> Result<()> {
        // For WeChat, channel_id is the user ID
        match message.message_type {
            MessageType::Text => {
                self.send_text_message(channel_id, &message.content).await?;
            }
            _ => {
                self.send_text_message(channel_id, &message.content).await?;
            }
        }
        Ok(())
    }

    async fn start_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        self.stop_listener().await?;

        match self.config.base.connection_mode {
            ConnectionMode::Webhook | ConnectionMode::Polling => {
                let channel = self.clone();
                let handle = tokio::spawn(async move {
                    if let Err(e) = channel.run_webhook_listener(event_bus).await {
                        error!("Webhook listener error: {}", e);
                    }
                });
                *self.listener_handle.write().await = Some(handle);
            }
            _ => {
                return Err(AgentError::platform(
                    "WeChat does not support WebSocket mode",
                ));
            }
        }

        Ok(())
    }

    async fn stop_listener(&self) -> Result<()> {
        if let Some(handle) = self.listener_handle.write().await.take() {
            handle.abort();
        }
        Ok(())
    }

    fn supported_content_types(&self) -> Vec<ContentType> {
        vec![
            ContentType::Text,
            ContentType::Image,
            ContentType::File,
            ContentType::Audio,
            ContentType::Video,
            ContentType::Card,
        ]
    }

    async fn list_channels(&self) -> Result<Vec<ChannelInfo>> {
        // WeChat doesn't have an API to list all conversations
        Ok(vec![])
    }

    async fn list_members(&self, _channel_id: &str) -> Result<Vec<MemberInfo>> {
        // WeChat has department APIs
        Ok(vec![])
    }

    fn connection_mode(&self) -> ConnectionMode {
        self.config.base.connection_mode
    }
}

impl Clone for WeChatChannel {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            http_client: self.http_client.clone(),
            access_token: self.access_token.clone(),
            connected: self.connected.clone(),
            listener_handle: Arc::new(RwLock::new(None)),
            link_handler: self.link_handler.clone(),
            command_handler: self.command_handler.clone(),
        }
    }
}
