//! Slack Channel Implementation
//!
//! Unified Channel trait implementation for Slack.
//! Supports Socket Mode (WebSocket) as default, Events API (Webhook), and
//! Polling.

use std::sync::Arc;

use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, error, info, warn};

use super::r#trait::{BaseChannelConfig, ConnectionMode, ContentType};
use super::{Channel, ChannelConfig, ChannelEvent, ChannelInfo, MemberInfo};
use crate::communication::{Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

/// Slack API base URL
const SLACK_API_BASE: &str = "https://slack.com/api";

/// Slack Channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackChannelConfig {
    /// Bot token (xoxb-...)
    pub bot_token: String,
    /// App token for Socket Mode (xapp-...)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_token: Option<String>,
    /// Signing secret for webhook verification
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signing_secret: Option<String>,
    /// Base channel configuration
    #[serde(flatten)]
    pub base: BaseChannelConfig,
}

impl Default for SlackChannelConfig {
    fn default() -> Self {
        Self {
            bot_token: String::new(),
            app_token: None,
            signing_secret: None,
            base: BaseChannelConfig::default(),
        }
    }
}

impl ChannelConfig for SlackChannelConfig {
    fn from_env() -> Option<Self>
    where
        Self: Sized,
    {
        let bot_token = std::env::var("SLACK_BOT_TOKEN").ok()?;
        let app_token = std::env::var("SLACK_APP_TOKEN").ok();
        let signing_secret = std::env::var("SLACK_SIGNING_SECRET").ok();

        let base = BaseChannelConfig::from_env("SLACK").unwrap_or_default();

        Some(Self {
            bot_token,
            app_token,
            signing_secret,
            base,
        })
    }

    fn is_valid(&self) -> bool {
        !self.bot_token.is_empty()
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

/// Slack API response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackApiResponse<T> {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(flatten)]
    pub data: T,
}

/// Slack user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackUser {
    pub id: String,
    pub name: String,
    #[serde(rename = "real_name")]
    pub real_name: Option<String>,
}

/// Slack message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    pub user: Option<String>,
    pub text: Option<String>,
    pub ts: String,
    pub channel: Option<String>,
    #[serde(rename = "bot_id")]
    pub bot_id: Option<String>,
}

/// Slack Socket Mode message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SlackSocketMessage {
    #[serde(rename = "hello")]
    Hello { envelope_id: String },
    #[serde(rename = "events_api")]
    EventsApi {
        envelope_id: String,
        payload: serde_json::Value,
    },
    #[serde(rename = "interactive")]
    Interactive {
        envelope_id: String,
        payload: serde_json::Value,
    },
    #[serde(rename = "disconnect")]
    Disconnect { reason: Option<String> },
    #[serde(rename = "error")]
    Error { error: SlackSocketError },
}

/// Slack Socket Mode error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackSocketError {
    pub msg: String,
    pub code: i32,
}

/// Slack Channel implementation
pub struct SlackChannel {
    config: SlackChannelConfig,
    http_client: reqwest::Client,
    connected: Arc<RwLock<bool>>,
    ws_sender: Arc<RwLock<Option<mpsc::UnboundedSender<WsMessage>>>>,
    listener_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    bot_info: Arc<RwLock<Option<SlackUser>>>,
    /// Event bus for emitting channel events
    event_bus: Arc<RwLock<Option<mpsc::Sender<ChannelEvent>>>>,
}

impl SlackChannel {
    /// Create a new Slack channel
    pub fn new(config: SlackChannelConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
            connected: Arc::new(RwLock::new(false)),
            ws_sender: Arc::new(RwLock::new(None)),
            listener_handle: Arc::new(RwLock::new(None)),
            bot_info: Arc::new(RwLock::new(None)),
            event_bus: Arc::new(RwLock::new(None)),
        }
    }

    /// Create from environment variables
    pub fn from_env() -> Result<Self> {
        let config = SlackChannelConfig::from_env()
            .ok_or_else(|| AgentError::configuration("SLACK_BOT_TOKEN not set"))?;
        Ok(Self::new(config))
    }

    /// Get API URL
    fn get_api_url(&self, method: &str) -> String {
        format!("{}/{}", SLACK_API_BASE, method)
    }

    /// Get bot info
    async fn get_bot_info(&self) -> Result<SlackUser> {
        let url = self.get_api_url("auth.test");

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.config.bot_token))
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to get bot info: {}", e)))?;

        let api_response: SlackApiResponse<serde_json::Value> = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        if !api_response.ok {
            return Err(
                AgentError::platform(format!("Slack API error: {:?}", api_response.error)).into(),
            );
        }

        let user_id = api_response
            .data
            .get("user_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AgentError::platform("No user_id in response"))?;

        let user = api_response
            .data
            .get("user")
            .and_then(|v| v.as_str())
            .unwrap_or(user_id);

        Ok(SlackUser {
            id: user_id.to_string(),
            name: user.to_string(),
            real_name: None,
        })
    }

    /// Open Socket Mode connection
    async fn open_socket_mode(&self) -> Result<String> {
        let app_token =
            self.config.app_token.as_ref().ok_or_else(|| {
                AgentError::configuration("SLACK_APP_TOKEN not set for Socket Mode")
            })?;

        let url = self.get_api_url("apps.connections.open");

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", app_token))
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to open Socket Mode: {}", e)))?;

        let api_response: SlackApiResponse<serde_json::Value> = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        if !api_response.ok {
            return Err(
                AgentError::platform(format!("Slack API error: {:?}", api_response.error)).into(),
            );
        }

        let ws_url = api_response
            .data
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AgentError::platform("No WebSocket URL in response"))?;

        Ok(ws_url.to_string())
    }

    /// Send text message to channel
    pub async fn send_text_message(&self, channel_id: &str, text: &str) -> Result<String> {
        let url = self.get_api_url("chat.postMessage");

        let body = serde_json::json!({
            "channel": channel_id,
            "text": text,
        });

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.bot_token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to send message: {}", e)))?;

        let api_response: SlackApiResponse<SlackMessage> = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        if !api_response.ok {
            return Err(
                AgentError::platform(format!("Slack API error: {:?}", api_response.error)).into(),
            );
        }

        Ok(api_response.data.ts)
    }

    /// Connect via Socket Mode (WebSocket)
    async fn connect_socket_mode(&self) -> Result<()> {
        let ws_url = self.open_socket_mode().await?;

        let (ws_stream, _) = connect_async(&ws_url)
            .await
            .map_err(|e| AgentError::platform(format!("WebSocket connection failed: {}", e)))?;

        info!("Slack Socket Mode WebSocket connected");

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        let (tx, mut rx) = mpsc::unbounded_channel::<WsMessage>();
        *self.ws_sender.write().await = Some(tx);

        *self.connected.write().await = true;

        // Verify bot token
        let bot_info = self.get_bot_info().await?;
        info!(
            "Connected to Slack as @{} (ID: {})",
            bot_info.name, bot_info.id
        );
        *self.bot_info.write().await = Some(bot_info);

        // Spawn WebSocket handler
        let auto_reconnect = self.config.base.auto_reconnect;
        let _max_reconnect_attempts = self.config.base.max_reconnect_attempts;
        let channel = self.clone();

        tokio::spawn(async move {
            let mut heartbeat_interval = interval(Duration::from_secs(30));

            loop {
                tokio::select! {
                    Some(msg) = ws_receiver.next() => {
                        match msg {
                            Ok(WsMessage::Text(text)) => {
                                debug!("Received Socket Mode message: {}", text);
                                if let Err(e) = channel.handle_socket_message(&text).await {
                                    warn!("Failed to handle Socket Mode message: {}", e);
                                }
                            }
                            Ok(WsMessage::Binary(data)) => {
                                debug!("Received binary message: {} bytes", data.len());
                            }
                            Ok(WsMessage::Ping(data)) => {
                                if let Err(e) = ws_sender.send(WsMessage::Pong(data)).await {
                                    error!("Failed to send pong: {}", e);
                                    break;
                                }
                            }
                            Ok(WsMessage::Close(_)) => {
                                info!("WebSocket closed by server");
                                break;
                            }
                            Err(e) => {
                                error!("WebSocket error: {}", e);
                                break;
                            }
                            _ => {}
                        }
                    }
                    Some(msg) = rx.recv() => {
                        if let Err(e) = ws_sender.send(msg).await {
                            error!("Failed to send WebSocket message: {}", e);
                            break;
                        }
                    }
                    _ = heartbeat_interval.tick() => {
                        // Slack Socket Mode doesn't require client-side heartbeat
                    }
                }
            }

            if auto_reconnect {
                info!("Attempting to reconnect to Slack Socket Mode...");
            }
        });

        Ok(())
    }

    /// Handle Socket Mode message
    async fn handle_socket_message(&self, text: &str) -> Result<()> {
        match serde_json::from_str::<SlackSocketMessage>(text) {
            Ok(msg) => {
                match msg {
                    SlackSocketMessage::Hello { envelope_id } => {
                        debug!("Socket Mode hello received: {}", envelope_id);
                    }
                    SlackSocketMessage::EventsApi {
                        envelope_id,
                        payload,
                    } => {
                        debug!("Received events API payload: {}", envelope_id);
                        // 🟢 P1 FIX: Process message events and emit ChannelEvent
                        if let Err(e) = self.process_events_api_payload(&payload).await {
                            warn!("Failed to process Events API payload: {}", e);
                        }
                    }
                    SlackSocketMessage::Disconnect { reason } => {
                        warn!("Socket Mode disconnect: {:?}", reason);
                    }
                    SlackSocketMessage::Error { error } => {
                        error!("Socket Mode error: {} (code: {})", error.msg, error.code);
                    }
                    _ => {
                        debug!("Received Socket Mode message: {:?}", msg);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to parse Socket Mode message: {}", e);
            }
        }
        Ok(())
    }

    /// Process an Events API payload and emit ChannelEvent for message events.
    async fn process_events_api_payload(&self, payload: &serde_json::Value) -> Result<()> {
        let event = payload.get("event");
        let event_type = event.and_then(|e| e.get("type")).and_then(|v| v.as_str());

        if event_type != Some("message") {
            return Ok(());
        }

        // Ignore bot's own messages
        if let Some(bot_id) = event.and_then(|e| e.get("bot_id")).and_then(|v| v.as_str()) {
            if let Some(ref bot_info) = *self.bot_info.read().await {
                if bot_id == bot_info.id {
                    return Ok(());
                }
            }
        }

        let user = event
            .and_then(|e| e.get("user"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let text = event
            .and_then(|e| e.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let channel = event
            .and_then(|e| e.get("channel"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let ts = event
            .and_then(|e| e.get("ts"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if user.is_empty() || text.is_empty() {
            return Ok(());
        }

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("user_id".to_string(), user.clone());
        metadata.insert("channel".to_string(), channel.clone());
        metadata.insert("message_ts".to_string(), ts.clone());
        metadata.insert("sender_id".to_string(), user.clone());
        metadata.insert("channel_id".to_string(), channel.clone());

        let message = Message {
            id: uuid::Uuid::new_v4(),
            thread_id: uuid::Uuid::new_v4(),
            platform: PlatformType::Slack,
            message_type: MessageType::Text,
            content: text,
            metadata,
            timestamp: chrono::Utc::now(),
        };

        let event = ChannelEvent::MessageReceived {
            platform: PlatformType::Slack,
            channel_id: user.clone(),
            message,
        };

        if let Some(ref tx) = *self.event_bus.read().await {
            if let Err(e) = tx.send(event).await {
                warn!("Failed to send ChannelEvent to event bus: {}", e);
            } else {
                info!(
                    "📨 Emitted ChannelEvent::MessageReceived for Slack user {}",
                    user
                );
            }
        } else {
            warn!("Event bus not set, cannot emit ChannelEvent");
        }

        Ok(())
    }

    /// Connect via Webhook (Events API)
    async fn connect_webhook(&self) -> Result<()> {
        // Verify bot token
        let bot_info = self.get_bot_info().await?;
        info!("Connected to Slack via Webhook as @{}", bot_info.name);
        *self.bot_info.write().await = Some(bot_info);
        *self.connected.write().await = true;
        Ok(())
    }

    /// Connect via Polling (RTM API)
    async fn connect_polling(&self) -> Result<()> {
        // RTM API is deprecated, use Socket Mode instead
        warn!("Slack RTM API is deprecated, using Socket Mode instead");
        self.connect_socket_mode().await
    }

    /// Run Socket Mode listener
    async fn run_socket_mode_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        // Store event bus so the WebSocket handler can emit events
        *self.event_bus.write().await = Some(event_bus);
        // The actual WebSocket read loop is spawned inside connect_socket_mode.
        // This task just keeps alive so the listener_handle is non-empty.
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }

    /// Run webhook listener
    async fn run_webhook_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        info!(
            "Slack webhook listener started on port {}",
            self.config.base.webhook_port
        );
        *self.event_bus.write().await = Some(event_bus);
        // TODO: Implement HTTP server for webhook
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }

    /// Convert Slack message to internal Message
    #[allow(dead_code)]
    fn convert_message(&self, slack_msg: &SlackMessage) -> Option<Message> {
        let content = slack_msg.text.clone().unwrap_or_default();

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("message_ts".to_string(), slack_msg.ts.clone());
        metadata.insert(
            "channel".to_string(),
            slack_msg.channel.clone().unwrap_or_default(),
        );

        if let Some(user) = &slack_msg.user {
            metadata.insert("user_id".to_string(), user.clone());
        }

        if let Some(bot_id) = &slack_msg.bot_id {
            metadata.insert("bot_id".to_string(), bot_id.clone());
        }

        let timestamp = chrono::Utc::now();

        Some(Message {
            id: uuid::Uuid::new_v4(),
            thread_id: uuid::Uuid::new_v4(),
            platform: PlatformType::Slack,
            message_type: MessageType::Text,
            content,
            metadata,
            timestamp,
        })
    }
}

#[async_trait]
impl Channel for SlackChannel {
    fn name(&self) -> &str {
        "slack"
    }

    fn platform(&self) -> PlatformType {
        PlatformType::Slack
    }

    fn is_connected(&self) -> bool {
        if let Ok(connected) = self.connected.try_read() {
            *connected
        } else {
            false
        }
    }

    async fn connect(&mut self) -> Result<()> {
        match self.config.base.connection_mode {
            ConnectionMode::WebSocket => self.connect_socket_mode().await,
            ConnectionMode::Webhook => self.connect_webhook().await,
            ConnectionMode::Polling => self.connect_polling().await,
        }
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.stop_listener().await?;

        if let Some(sender) = self.ws_sender.write().await.take() {
            let _ = sender.send(WsMessage::Close(None));
        }

        *self.connected.write().await = false;
        info!("Disconnected from Slack");
        Ok(())
    }

    async fn send(&self, channel_id: &str, message: &Message) -> Result<()> {
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
            ConnectionMode::WebSocket => {
                let channel = self.clone();
                let handle = tokio::spawn(async move {
                    if let Err(e) = channel.run_socket_mode_listener(event_bus).await {
                        error!("Socket Mode listener error: {}", e);
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
                    "Slack does not support Polling mode (use Socket Mode or Webhook)",
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
            ContentType::Rich,
            ContentType::Card,
        ]
    }

    async fn list_channels(&self) -> Result<Vec<ChannelInfo>> {
        // Slack has conversations.list API
        Ok(vec![])
    }

    async fn list_members(&self, _channel_id: &str) -> Result<Vec<MemberInfo>> {
        // Slack has conversations.members API
        Ok(vec![])
    }

    fn connection_mode(&self) -> ConnectionMode {
        self.config.base.connection_mode
    }
}

impl Clone for SlackChannel {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            http_client: self.http_client.clone(),
            connected: self.connected.clone(),
            ws_sender: Arc::new(RwLock::new(None)),
            listener_handle: Arc::new(RwLock::new(None)),
            bot_info: self.bot_info.clone(),
            event_bus: self.event_bus.clone(),
        }
    }
}

// ============================================================================
// Slack Channel Factory
// ============================================================================

use serde_json::Value;

use super::r#trait::ChannelFactory;

/// Slack channel factory
#[derive(Debug, Clone)]
pub struct SlackChannelFactory;

impl SlackChannelFactory {
    pub fn new() -> Self {
        Self
    }

    pub fn default_config() -> Value {
        json!({
            "bot_token": "xoxb-YOUR-TOKEN",
            "connection_mode": "websocket",
            "auto_reconnect": true,
            "max_reconnect_attempts": 10,
            "webhook_port": 8080,
            "webhook_url": null,
        })
    }
}

#[async_trait]
impl ChannelFactory for SlackChannelFactory {
    fn name(&self) -> &str {
        "slack"
    }

    fn platform_type(&self) -> super::PlatformType {
        super::PlatformType::Slack
    }

    async fn create(
        &self,
        config: &Value,
    ) -> crate::error::Result<Arc<RwLock<dyn super::Channel>>> {
        use crate::error::AgentError;

        let bot_token = config
            .get("bot_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AgentError::platform("Slack bot_token is required"))?
            .to_string();

        let channel = SlackChannel::new(SlackChannelConfig {
            bot_token,
            app_token: config
                .get("app_token")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            signing_secret: config
                .get("signing_secret")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            base: BaseChannelConfig::default(),
        });

        Ok(Arc::new(RwLock::new(channel)))
    }

    fn validate_config(&self, config: &Value) -> bool {
        config
            .get("bot_token")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty() && s.starts_with("xoxb-"))
            .unwrap_or(false)
    }

    fn default_config(&self) -> Value {
        Self::default_config()
    }
}
