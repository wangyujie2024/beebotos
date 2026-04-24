//! Telegram Channel Implementation
//!
//! Unified Channel trait implementation for Telegram Bot API.
//! Supports Polling mode (default) and Webhook mode.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};

use super::r#trait::{BaseChannelConfig, ConnectionMode, ContentType};
use super::telegram_content::{
    TelegramAudioFile, TelegramContactContent, TelegramDocumentFile, TelegramLocationContent,
    TelegramStickerFile, TelegramVideoFile, TelegramVoiceFile,
};
use super::{Channel, ChannelConfig, ChannelEvent, ChannelInfo, MemberInfo};
use crate::communication::{Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

/// Telegram API base URL
const TELEGRAM_API_BASE: &str = "https://api.telegram.org/bot";

/// Telegram Channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    /// Bot token from @BotFather
    pub bot_token: String,
    /// Polling interval in seconds (default: 1)
    #[serde(default = "default_polling_interval")]
    pub polling_interval_secs: u64,
    /// Allowed updates types (default: all)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_updates: Option<Vec<String>>,
    /// Base channel configuration
    #[serde(flatten)]
    pub base: BaseChannelConfig,
}

fn default_polling_interval() -> u64 {
    1
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            bot_token: String::new(),
            polling_interval_secs: 1,
            allowed_updates: None,
            base: BaseChannelConfig {
                connection_mode: ConnectionMode::Polling,
                ..Default::default()
            },
        }
    }
}

impl ChannelConfig for TelegramConfig {
    fn from_env() -> Option<Self>
    where
        Self: Sized,
    {
        let bot_token = std::env::var("TELEGRAM_BOT_TOKEN").ok()?;

        let polling_interval_secs = std::env::var("TELEGRAM_POLLING_INTERVAL")
            .map(|v| v.parse().unwrap_or(1))
            .unwrap_or(1);

        let mut base = BaseChannelConfig::from_env("TELEGRAM").unwrap_or_default();
        // Telegram default is Polling, not WebSocket
        if std::env::var("TELEGRAM_CONNECTION_MODE").is_err() {
            base.connection_mode = ConnectionMode::Polling;
        }

        Some(Self {
            bot_token,
            polling_interval_secs,
            allowed_updates: None,
            base,
        })
    }

    fn is_valid(&self) -> bool {
        !self.bot_token.is_empty()
    }

    fn allowlist(&self) -> Vec<String> {
        // Telegram doesn't have a native allowlist concept
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

/// Telegram API response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramApiResponse<T> {
    pub ok: bool,
    pub result: Option<T>,
    #[serde(rename = "description")]
    pub error_description: Option<String>,
    pub error_code: Option<i32>,
}

/// Telegram Update (incoming message/event)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramUpdate {
    #[serde(rename = "update_id")]
    pub update_id: i64,
    pub message: Option<TelegramMessage>,
    #[serde(rename = "edited_message")]
    pub edited_message: Option<TelegramMessage>,
    #[serde(rename = "channel_post")]
    pub channel_post: Option<TelegramMessage>,
    #[serde(rename = "callback_query")]
    pub callback_query: Option<serde_json::Value>,
}

/// Telegram Message
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelegramMessage {
    #[serde(rename = "message_id")]
    pub message_id: i64,
    pub from: Option<TelegramUser>,
    pub date: i64,
    pub chat: TelegramChat,
    pub text: Option<String>,
    pub photo: Option<Vec<TelegramPhotoSize>>,
    pub video: Option<TelegramVideoFile>,
    pub audio: Option<TelegramAudioFile>,
    pub voice: Option<TelegramVoiceFile>,
    pub video_note: Option<serde_json::Value>,
    pub document: Option<TelegramDocumentFile>,
    pub sticker: Option<TelegramStickerFile>,
    pub location: Option<TelegramLocationContent>,
    pub contact: Option<TelegramContactContent>,
    pub caption: Option<String>,
    #[serde(rename = "reply_to_message")]
    pub reply_to_message: Option<Box<TelegramMessage>>,
}

/// Telegram User
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelegramUser {
    pub id: i64,
    #[serde(rename = "is_bot")]
    pub is_bot: bool,
    #[serde(rename = "first_name")]
    pub first_name: String,
    #[serde(rename = "last_name")]
    pub last_name: Option<String>,
    pub username: Option<String>,
}

/// Telegram Chat
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelegramChat {
    pub id: i64,
    #[serde(rename = "type")]
    pub chat_type: String,
    pub title: Option<String>,
    pub username: Option<String>,
    #[serde(rename = "first_name")]
    pub first_name: Option<String>,
    #[serde(rename = "last_name")]
    pub last_name: Option<String>,
}

/// Telegram Photo Size
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelegramPhotoSize {
    #[serde(rename = "file_id")]
    pub file_id: String,
    #[serde(rename = "file_unique_id")]
    pub file_unique_id: String,
    pub width: i32,
    pub height: i32,
    #[serde(rename = "file_size")]
    pub file_size: Option<i32>,
}

/// Telegram Callback Query
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelegramCallbackQuery {
    pub id: String,
    pub from: TelegramUser,
    pub data: Option<String>,
    pub chat_instance: String,
}

/// Telegram Inline Query
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelegramInlineQuery {
    pub id: String,
    pub from: TelegramUser,
    pub query: String,
    pub offset: String,
}

/// Telegram Channel implementation
pub struct TelegramChannel {
    config: TelegramConfig,
    http_client: reqwest::Client,
    connected: Arc<RwLock<bool>>,
    last_update_id: Arc<RwLock<Option<i64>>>,
    listener_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl TelegramChannel {
    /// Create a new Telegram channel
    pub fn new(config: TelegramConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
            connected: Arc::new(RwLock::new(false)),
            last_update_id: Arc::new(RwLock::new(None)),
            listener_handle: Arc::new(RwLock::new(None)),
        }
    }

    /// Create from environment variables
    pub fn from_env() -> Result<Self> {
        let config = TelegramConfig::from_env()
            .ok_or_else(|| AgentError::configuration("TELEGRAM_BOT_TOKEN not set"))?;
        Ok(Self::new(config))
    }

    fn get_api_url(&self, method: &str) -> String {
        format!("{}{}/{}", TELEGRAM_API_BASE, self.config.bot_token, method)
    }

    /// Send text message
    pub async fn send_text_message(&self, chat_id: i64, text: &str) -> Result<i64> {
        let url = self.get_api_url("sendMessage");

        let body = serde_json::json!({
            "chat_id": chat_id,
            "text": text,
        });

        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to send message: {}", e)))?;

        let api_response: TelegramApiResponse<TelegramMessage> = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        if !api_response.ok {
            return Err(AgentError::platform(format!(
                "Telegram API error: {:?}",
                api_response.error_description
            ))
            .into());
        }

        api_response
            .result
            .map(|m| m.message_id)
            .ok_or_else(|| AgentError::platform("No message ID in response").into())
    }

    /// Send photo
    pub async fn send_photo(
        &self,
        chat_id: i64,
        photo: &str,
        caption: Option<&str>,
    ) -> Result<i64> {
        let url = self.get_api_url("sendPhoto");

        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "photo": photo,
        });

        if let Some(cap) = caption {
            body["caption"] = cap.into();
        }

        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to send photo: {}", e)))?;

        let api_response: TelegramApiResponse<TelegramMessage> = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        if !api_response.ok {
            return Err(AgentError::platform(format!(
                "Telegram API error: {:?}",
                api_response.error_description
            ))
            .into());
        }

        api_response
            .result
            .map(|m| m.message_id)
            .ok_or_else(|| AgentError::platform("No message ID in response").into())
    }

    /// Get updates (for polling mode)
    async fn get_updates(&self, limit: i32, timeout: i32) -> Result<Vec<TelegramUpdate>> {
        let url = self.get_api_url("getUpdates");

        let mut params = std::collections::HashMap::new();
        params.insert("limit", limit.to_string());
        params.insert("timeout", timeout.to_string());

        if let Some(offset) = *self.last_update_id.read().await {
            params.insert("offset", (offset + 1).to_string());
        }

        let response = self
            .http_client
            .post(&url)
            .form(&params)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to get updates: {}", e)))?;

        let api_response: TelegramApiResponse<Vec<TelegramUpdate>> = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        if !api_response.ok {
            return Err(AgentError::platform(format!(
                "Telegram API error: {:?}",
                api_response.error_description
            ))
            .into());
        }

        Ok(api_response.result.unwrap_or_default())
    }

    /// Set webhook
    async fn set_webhook(&self, webhook_url: &str) -> Result<()> {
        let url = self.get_api_url("setWebhook");

        let body = serde_json::json!({
            "url": webhook_url,
        });

        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to set webhook: {}", e)))?;

        let api_response: TelegramApiResponse<bool> = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        if !api_response.ok {
            return Err(AgentError::platform(format!(
                "Telegram API error: {:?}",
                api_response.error_description
            ))
            .into());
        }

        info!("Telegram webhook set successfully");
        Ok(())
    }

    /// Delete webhook
    async fn delete_webhook(&self) -> Result<()> {
        let url = self.get_api_url("deleteWebhook");

        let response = self
            .http_client
            .post(&url)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to delete webhook: {}", e)))?;

        let api_response: TelegramApiResponse<bool> = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        if !api_response.ok {
            return Err(AgentError::platform(format!(
                "Telegram API error: {:?}",
                api_response.error_description
            ))
            .into());
        }

        info!("Telegram webhook deleted successfully");
        Ok(())
    }

    /// Get bot info
    async fn get_me(&self) -> Result<TelegramUser> {
        let url = self.get_api_url("getMe");

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to get bot info: {}", e)))?;

        let api_response: TelegramApiResponse<TelegramUser> = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        if !api_response.ok {
            return Err(AgentError::platform(format!(
                "Telegram API error: {:?}",
                api_response.error_description
            ))
            .into());
        }

        api_response
            .result
            .ok_or_else(|| AgentError::platform("No result in response").into())
    }

    /// Convert Telegram message to internal Message
    fn convert_message(&self, tg_msg: &TelegramMessage) -> Option<Message> {
        let message_type = if tg_msg.text.is_some() {
            MessageType::Text
        } else if tg_msg.photo.is_some() {
            MessageType::Image
        } else if tg_msg.video.is_some() {
            MessageType::Video
        } else if tg_msg.audio.is_some() || tg_msg.voice.is_some() {
            MessageType::Voice
        } else if tg_msg.document.is_some() {
            MessageType::File
        } else if tg_msg.sticker.is_some() {
            MessageType::Sticker
        } else {
            MessageType::System
        };

        let content = tg_msg
            .text
            .clone()
            .or_else(|| tg_msg.caption.clone())
            .unwrap_or_default();

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("chat_id".to_string(), tg_msg.chat.id.to_string());
        metadata.insert("message_id".to_string(), tg_msg.message_id.to_string());
        metadata.insert("chat_type".to_string(), tg_msg.chat.chat_type.clone());

        if let Some(user) = &tg_msg.from {
            metadata.insert("user_id".to_string(), user.id.to_string());
            metadata.insert("user_name".to_string(), user.first_name.clone());
            if let Some(username) = &user.username {
                metadata.insert("username".to_string(), username.clone());
            }
        }

        if let Some(photos) = &tg_msg.photo {
            if let Some(photo) = photos.last() {
                metadata.insert("file_id".to_string(), photo.file_id.clone());
            }
        }

        let timestamp = chrono::DateTime::from_timestamp(tg_msg.date, 0)?;

        Some(Message {
            id: uuid::Uuid::new_v4(),
            thread_id: uuid::Uuid::new_v4(),
            platform: PlatformType::Telegram,
            message_type,
            content,
            metadata,
            timestamp,
        })
    }

    /// Run polling listener
    async fn run_polling_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        let mut interval = interval(Duration::from_secs(self.config.polling_interval_secs));
        let max_reconnect_attempts = self.config.base.max_reconnect_attempts;
        let auto_reconnect = self.config.base.auto_reconnect;
        let mut reconnect_attempts = 0;

        loop {
            interval.tick().await;

            match self.get_updates(100, 30).await {
                Ok(updates) => {
                    reconnect_attempts = 0;

                    for update in updates {
                        // Update last update ID
                        if update.update_id > 0 {
                            *self.last_update_id.write().await = Some(update.update_id);
                        }

                        // Process message
                        if let Some(tg_msg) = update.message.as_ref() {
                            if let Some(message) = self.convert_message(tg_msg) {
                                let event = ChannelEvent::MessageReceived {
                                    platform: PlatformType::Telegram,
                                    channel_id: tg_msg.chat.id.to_string(),
                                    message,
                                };

                                if let Err(e) = event_bus.send(event).await {
                                    error!("Failed to send event to event bus: {}", e);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Polling error: {}", e);

                    if auto_reconnect {
                        reconnect_attempts += 1;
                        if reconnect_attempts > max_reconnect_attempts {
                            error!("Max reconnect attempts reached");
                            break;
                        }
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    } else {
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Run webhook listener
    async fn run_webhook_listener(&self, _event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        // For webhook mode, we need to start an HTTP server
        // This is a simplified version - in production you'd use axum or similar
        info!(
            "Webhook listener started on port {}",
            self.config.base.webhook_port
        );

        // TODO: Implement HTTP server for webhook
        // For now, just keep the task alive
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }
}

#[async_trait]
impl Channel for TelegramChannel {
    fn name(&self) -> &str {
        "telegram"
    }

    fn platform(&self) -> PlatformType {
        PlatformType::Telegram
    }

    fn is_connected(&self) -> bool {
        // Try to read the connected state
        if let Ok(connected) = self.connected.try_read() {
            *connected
        } else {
            false
        }
    }

    async fn connect(&mut self) -> Result<()> {
        // Verify token by calling getMe
        let bot_info = self.get_me().await?;
        info!(
            "Connected to Telegram as @{} (ID: {})",
            bot_info.username.as_ref().unwrap_or(&bot_info.first_name),
            bot_info.id
        );

        *self.connected.write().await = true;

        // Set up webhook if in webhook mode
        if self.config.base.connection_mode == ConnectionMode::Webhook {
            if let Some(webhook_url) = &self.config.base.webhook_url {
                self.set_webhook(webhook_url).await?;
            } else {
                warn!("Webhook mode selected but no webhook URL provided");
            }
        }

        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        // Stop listener if running
        self.stop_listener().await?;

        // Delete webhook if in webhook mode
        if self.config.base.connection_mode == ConnectionMode::Webhook {
            self.delete_webhook().await.ok();
        }

        *self.connected.write().await = false;
        info!("Disconnected from Telegram");

        Ok(())
    }

    async fn send(&self, channel_id: &str, message: &Message) -> Result<()> {
        let chat_id: i64 = channel_id
            .parse()
            .map_err(|_| AgentError::platform(format!("Invalid chat ID: {}", channel_id)))?;

        match message.message_type {
            MessageType::Text => {
                self.send_text_message(chat_id, &message.content).await?;
            }
            MessageType::Image => {
                // For images, we'd need to handle file uploads
                // For now, just send the content as text
                self.send_text_message(chat_id, &message.content).await?;
            }
            _ => {
                // Default to text for other types
                self.send_text_message(chat_id, &message.content).await?;
            }
        }

        Ok(())
    }

    async fn start_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        // Stop existing listener if any
        self.stop_listener().await?;

        match self.config.base.connection_mode {
            ConnectionMode::Polling => {
                let channel = self.clone();
                let handle = tokio::spawn(async move {
                    if let Err(e) = channel.run_polling_listener(event_bus).await {
                        error!("Polling listener error: {}", e);
                    }
                });
                *self.listener_handle.write().await = Some(handle);
            }
            ConnectionMode::Webhook => {
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
                    "Telegram does not support WebSocket mode",
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
            ContentType::Location,
            ContentType::Sticker,
        ]
    }

    async fn list_channels(&self) -> Result<Vec<ChannelInfo>> {
        // Telegram doesn't have an API to list all chats
        // We'd need to track them from incoming messages
        Ok(vec![])
    }

    async fn list_members(&self, _channel_id: &str) -> Result<Vec<MemberInfo>> {
        // Telegram has getChatMember API but requires admin permissions
        Ok(vec![])
    }

    fn connection_mode(&self) -> ConnectionMode {
        self.config.base.connection_mode
    }
}

impl Clone for TelegramChannel {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            http_client: self.http_client.clone(),
            connected: self.connected.clone(),
            last_update_id: self.last_update_id.clone(),
            listener_handle: Arc::new(RwLock::new(None)),
        }
    }
}

// ============================================================================
// Telegram Channel Factory
// ============================================================================

use serde_json::Value;

use super::r#trait::ChannelFactory;

/// Telegram channel factory
#[derive(Debug, Clone)]
pub struct TelegramChannelFactory;

impl TelegramChannelFactory {
    pub fn new() -> Self {
        Self
    }

    pub fn default_config() -> Value {
        json!({
            "bot_token": "",
            "connection_mode": "polling",
            "auto_reconnect": true,
            "max_reconnect_attempts": 10,
            "polling_interval_secs": 1,
            "webhook_port": 8080,
        })
    }
}

#[async_trait]
impl ChannelFactory for TelegramChannelFactory {
    fn name(&self) -> &str {
        "telegram"
    }

    fn platform_type(&self) -> super::PlatformType {
        super::PlatformType::Telegram
    }

    async fn create(
        &self,
        config: &Value,
    ) -> crate::error::Result<Arc<RwLock<dyn super::Channel>>> {
        use crate::error::AgentError;

        let bot_token = config
            .get("bot_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AgentError::platform("Telegram bot_token is required"))?
            .to_string();

        let mut base = BaseChannelConfig::default();
        base.connection_mode = ConnectionMode::Polling;
        let channel = TelegramChannel::new(TelegramConfig {
            bot_token,
            polling_interval_secs: config
                .get("polling_interval_secs")
                .and_then(|v| v.as_u64())
                .unwrap_or(1),
            allowed_updates: config.get("allowed_updates").and_then(|v| {
                v.as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
            }),
            base,
        });

        Ok(Arc::new(RwLock::new(channel)))
    }

    fn validate_config(&self, config: &Value) -> bool {
        config
            .get("bot_token")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty() && s.contains(':'))
            .unwrap_or(false)
    }

    fn default_config(&self) -> Value {
        Self::default_config()
    }
}
