//! Discord Channel Implementation
//!
//! Unified Channel trait implementation for Discord.
//! Supports WebSocket Gateway mode (default) and Webhook mode.

use std::sync::Arc;

use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tracing::{debug, error, info, warn};

use super::r#trait::{BaseChannelConfig, ConnectionMode, ContentType};
use super::{Channel, ChannelConfig, ChannelEvent, ChannelInfo, MemberInfo};
use crate::communication::{Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

/// Discord API base URL
const DISCORD_API_BASE: &str = "https://discord.com/api/v10";

/// Discord Gateway URL
const DISCORD_GATEWAY_URL: &str = "wss://gateway.discord.gg/?v=10&encoding=json";

/// Discord Channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordChannelConfig {
    /// Bot token
    pub bot_token: String,
    /// Application ID (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<String>,
    /// Gateway intents (default: GUILDS + GUILD_MESSAGES + DIRECT_MESSAGES +
    /// MESSAGE_CONTENT)
    #[serde(default = "default_intents")]
    pub intents: u64,
    /// Base channel configuration
    #[serde(flatten)]
    pub base: BaseChannelConfig,
}

fn default_intents() -> u64 {
    // GUILDS | GUILD_MESSAGES | DIRECT_MESSAGES | MESSAGE_CONTENT
    1 << 0 | 1 << 9 | 1 << 12 | 1 << 15
}

impl Default for DiscordChannelConfig {
    fn default() -> Self {
        Self {
            bot_token: String::new(),
            application_id: None,
            intents: default_intents(),
            base: BaseChannelConfig::default(),
        }
    }
}

impl ChannelConfig for DiscordChannelConfig {
    fn from_env() -> Option<Self>
    where
        Self: Sized,
    {
        let bot_token = std::env::var("DISCORD_BOT_TOKEN").ok()?;
        let application_id = std::env::var("DISCORD_APPLICATION_ID").ok();

        let intents = std::env::var("DISCORD_INTENTS")
            .map(|v| v.parse().unwrap_or_else(|_| default_intents()))
            .unwrap_or_else(|_| default_intents());

        let base = BaseChannelConfig::from_env("DISCORD").unwrap_or_default();

        Some(Self {
            bot_token,
            application_id,
            intents,
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

/// Discord Gateway opcode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum GatewayOpcode {
    Dispatch = 0,
    Heartbeat = 1,
    Identify = 2,
    PresenceUpdate = 3,
    VoiceStateUpdate = 4,
    Resume = 6,
    Reconnect = 7,
    InvalidSession = 9,
    Hello = 10,
    HeartbeatAck = 11,
}

/// Discord Gateway payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayPayload {
    pub op: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub d: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub t: Option<String>,
}

/// Discord identify payload
#[derive(Debug, Clone, Serialize)]
pub struct IdentifyPayload {
    pub token: String,
    pub properties: IdentifyProperties,
    pub intents: u64,
}

/// Discord identify properties
#[derive(Debug, Clone, Serialize)]
pub struct IdentifyProperties {
    pub os: String,
    pub browser: String,
    pub device: String,
}

/// Discord resume payload
#[derive(Debug, Clone, Serialize)]
pub struct ResumePayload {
    pub token: String,
    #[serde(rename = "session_id")]
    pub session_id: String,
    pub seq: u64,
}

/// Discord hello payload
#[derive(Debug, Clone, Deserialize)]
pub struct HelloPayload {
    #[serde(rename = "heartbeat_interval")]
    pub heartbeat_interval: u64,
}

/// Discord user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordUser {
    pub id: String,
    pub username: String,
    #[serde(rename = "global_name")]
    pub global_name: Option<String>,
    #[serde(rename = "bot")]
    pub is_bot: Option<bool>,
}

/// Discord message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordMessage {
    pub id: String,
    #[serde(rename = "channel_id")]
    pub channel_id: String,
    pub author: DiscordUser,
    pub content: String,
    pub timestamp: String,
    #[serde(rename = "guild_id")]
    pub guild_id: Option<String>,
}

/// Discord Channel implementation
pub struct DiscordChannel {
    config: DiscordChannelConfig,
    http_client: reqwest::Client,
    connected: Arc<RwLock<bool>>,
    ws_stream: Arc<RwLock<Option<WebSocketStream<MaybeTlsStream<TcpStream>>>>>,
    session_id: Arc<RwLock<Option<String>>>,
    last_seq: Arc<RwLock<Option<u64>>>,
    heartbeat_interval: Arc<RwLock<Option<Duration>>>,
    listener_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl DiscordChannel {
    /// Create a new Discord channel
    pub fn new(config: DiscordChannelConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
            connected: Arc::new(RwLock::new(false)),
            ws_stream: Arc::new(RwLock::new(None)),
            session_id: Arc::new(RwLock::new(None)),
            last_seq: Arc::new(RwLock::new(None)),
            heartbeat_interval: Arc::new(RwLock::new(None)),
            listener_handle: Arc::new(RwLock::new(None)),
        }
    }

    /// Create from environment variables
    pub fn from_env() -> Result<Self> {
        let config = DiscordChannelConfig::from_env()
            .ok_or_else(|| AgentError::configuration("DISCORD_BOT_TOKEN not set"))?;
        Ok(Self::new(config))
    }

    /// Get API URL
    fn get_api_url(&self, endpoint: &str) -> String {
        format!("{}/{}", DISCORD_API_BASE, endpoint)
    }

    /// Send identify payload
    async fn identify(&self) -> Result<()> {
        let identify = IdentifyPayload {
            token: self.config.bot_token.clone(),
            properties: IdentifyProperties {
                os: "linux".to_string(),
                browser: "beebotos".to_string(),
                device: "beebotos".to_string(),
            },
            intents: self.config.intents,
        };

        let payload = GatewayPayload {
            op: GatewayOpcode::Identify as u8,
            d: Some(serde_json::to_value(identify).unwrap()),
            s: None,
            t: None,
        };

        self.send_payload(&payload).await?;
        info!("Sent Identify payload");
        Ok(())
    }

    /// Send resume payload
    #[allow(dead_code)]
    async fn resume(&self) -> Result<()> {
        let session_id = self.session_id.read().await.clone();
        let last_seq = *self.last_seq.read().await;

        if let (Some(session_id), Some(seq)) = (session_id, last_seq) {
            let resume = ResumePayload {
                token: self.config.bot_token.clone(),
                session_id,
                seq,
            };

            let payload = GatewayPayload {
                op: GatewayOpcode::Resume as u8,
                d: Some(serde_json::to_value(resume).unwrap()),
                s: None,
                t: None,
            };

            self.send_payload(&payload).await?;
            info!("Sent Resume payload");
            Ok(())
        } else {
            Err(AgentError::platform("Cannot resume without session_id and seq").into())
        }
    }

    /// Send payload to Gateway
    async fn send_payload(&self, payload: &GatewayPayload) -> Result<()> {
        if let Some(ws) = self.ws_stream.write().await.as_mut() {
            let json = serde_json::to_string(payload)
                .map_err(|e| AgentError::platform(format!("Failed to serialize payload: {}", e)))?;

            ws.send(WsMessage::Text(json))
                .await
                .map_err(|e| AgentError::platform(format!("Failed to send payload: {}", e)))?;

            Ok(())
        } else {
            Err(AgentError::platform("WebSocket not connected").into())
        }
    }

    /// Receive message from Gateway
    async fn receive_message(&self) -> Option<WsMessage> {
        if let Some(ws) = self.ws_stream.write().await.as_mut() {
            ws.next().await.and_then(|r| r.ok())
        } else {
            None
        }
    }

    /// Handle Gateway event
    async fn handle_gateway_event(
        &self,
        payload: GatewayPayload,
        event_bus: &mpsc::Sender<ChannelEvent>,
    ) -> Result<()> {
        match payload.op {
            0 => {
                // Dispatch
                if let Some(seq) = payload.s {
                    *self.last_seq.write().await = Some(seq);
                }

                if let Some(event_type) = payload.t {
                    match event_type.as_str() {
                        "READY" => {
                            if let Some(data) = payload.d {
                                if let Ok(ready) = serde_json::from_value::<serde_json::Value>(data)
                                {
                                    if let Some(session_id) =
                                        ready.get("session_id").and_then(|s| s.as_str())
                                    {
                                        *self.session_id.write().await =
                                            Some(session_id.to_string());
                                        info!("Discord Gateway Ready, session_id: {}", session_id);
                                    }
                                }
                            }
                        }
                        "MESSAGE_CREATE" => {
                            if let Some(data) = payload.d {
                                if let Ok(message) = serde_json::from_value::<DiscordMessage>(data)
                                {
                                    // Convert to internal Message
                                    if let Some(internal_msg) = self.convert_message(&message) {
                                        let event = ChannelEvent::MessageReceived {
                                            platform: PlatformType::Discord,
                                            channel_id: message.channel_id.clone(),
                                            message: internal_msg,
                                        };
                                        let _ = event_bus.send(event).await;
                                    }
                                }
                            }
                        }
                        _ => {
                            debug!("Received event: {}", event_type);
                        }
                    }
                }
            }
            7 => {
                // Reconnect
                warn!("Received reconnect request");
            }
            9 => {
                // Invalid session
                warn!("Invalid session");
                *self.session_id.write().await = None;
                *self.last_seq.write().await = None;
                tokio::time::sleep(Duration::from_secs(5)).await;
                self.identify().await?;
            }
            10 => {
                // Hello
                if let Some(data) = payload.d {
                    if let Ok(hello) = serde_json::from_value::<HelloPayload>(data) {
                        let interval = Duration::from_millis(hello.heartbeat_interval);
                        *self.heartbeat_interval.write().await = Some(interval);
                        info!("Received Hello, heartbeat interval: {:?}", interval);
                    }
                }
            }
            11 => {
                // Heartbeat ACK
                debug!("Received heartbeat ACK");
            }
            _ => {
                debug!("Unknown opcode: {}", payload.op);
            }
        }

        Ok(())
    }

    /// Convert Discord message to internal Message
    fn convert_message(&self, discord_msg: &DiscordMessage) -> Option<Message> {
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("message_id".to_string(), discord_msg.id.clone());
        metadata.insert("channel_id".to_string(), discord_msg.channel_id.clone());
        metadata.insert("author_id".to_string(), discord_msg.author.id.clone());
        metadata.insert(
            "author_name".to_string(),
            discord_msg.author.username.clone(),
        );
        metadata.insert(
            "is_bot".to_string(),
            discord_msg.author.is_bot.unwrap_or(false).to_string(),
        );

        if let Some(guild_id) = &discord_msg.guild_id {
            metadata.insert("guild_id".to_string(), guild_id.clone());
        }

        let timestamp = chrono::DateTime::parse_from_rfc3339(&discord_msg.timestamp)
            .ok()?
            .with_timezone(&chrono::Utc);

        Some(Message {
            id: uuid::Uuid::new_v4(),
            thread_id: uuid::Uuid::new_v4(),
            platform: PlatformType::Discord,
            message_type: MessageType::Text,
            content: discord_msg.content.clone(),
            metadata,
            timestamp,
        })
    }

    /// Connect via WebSocket Gateway
    async fn connect_websocket(&self) -> Result<()> {
        info!("Connecting to Discord Gateway...");

        let (ws_stream, _) = connect_async(DISCORD_GATEWAY_URL)
            .await
            .map_err(|e| AgentError::platform(format!("Failed to connect to Gateway: {}", e)))?;

        *self.ws_stream.write().await = Some(ws_stream);
        info!("WebSocket connection established");

        // Wait for Hello
        if let Some(msg) = self.receive_message().await {
            if let Ok(text) = msg.to_text() {
                let payload: GatewayPayload = serde_json::from_str(text)
                    .map_err(|e| AgentError::platform(format!("Failed to parse Hello: {}", e)))?;

                if payload.op == GatewayOpcode::Hello as u8 {
                    if let Some(data) = payload.d {
                        let hello: HelloPayload = serde_json::from_value(data).map_err(|e| {
                            AgentError::platform(format!("Failed to parse Hello data: {}", e))
                        })?;
                        *self.heartbeat_interval.write().await =
                            Some(Duration::from_millis(hello.heartbeat_interval));
                    }
                }
            }
        }

        // Send Identify
        self.identify().await?;

        *self.connected.write().await = true;
        info!("Discord Gateway connected successfully");

        Ok(())
    }

    /// Connect via Webhook
    async fn connect_webhook(&self) -> Result<()> {
        // Verify token by calling getMe equivalent
        let url = self.get_api_url("users/@me");

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bot {}", self.config.bot_token))
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to verify token: {}", e)))?;

        if !response.status().is_success() {
            return Err(AgentError::authentication("Invalid bot token").into());
        }

        *self.connected.write().await = true;
        info!("Discord webhook mode activated");
        Ok(())
    }

    /// Run WebSocket listener
    async fn run_websocket_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        let heartbeat_interval = self.heartbeat_interval.read().await.clone();

        if let Some(interval_duration) = heartbeat_interval {
            let mut heartbeat_timer = interval(interval_duration);

            loop {
                tokio::select! {
                    _ = heartbeat_timer.tick() => {
                        let last_seq = *self.last_seq.read().await;
                        let payload = GatewayPayload {
                            op: GatewayOpcode::Heartbeat as u8,
                            d: last_seq.map(|s| serde_json::json!(s)),
                            s: None,
                            t: None,
                        };

                        if let Err(e) = self.send_payload(&payload).await {
                            error!("Failed to send heartbeat: {}", e);
                            break;
                        }
                        debug!("Sent heartbeat");
                    }
                    Some(msg) = self.receive_message() => {
                        if let Ok(text) = msg.to_text() {
                            if let Ok(payload) = serde_json::from_str::<GatewayPayload>(text) {
                                if let Err(e) = self.handle_gateway_event(payload, &event_bus).await {
                                    error!("Failed to handle gateway event: {}", e);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Run webhook listener
    async fn run_webhook_listener(&self, _event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        info!(
            "Discord webhook listener started on port {}",
            self.config.base.webhook_port
        );
        // TODO: Implement HTTP server for webhook
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }

    /// Send text message to channel
    pub async fn send_text_message(&self, channel_id: &str, content: &str) -> Result<String> {
        let url = self.get_api_url("channels");
        let url = format!("{}/{}/messages", url, channel_id);

        let body = serde_json::json!({
            "content": content,
        });

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bot {}", self.config.bot_token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to send message: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(
                AgentError::platform(format!("Discord API error: {} - {}", status, body)).into(),
            );
        }

        let message: DiscordMessage = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        Ok(message.id)
    }
}

#[async_trait]
impl Channel for DiscordChannel {
    fn name(&self) -> &str {
        "discord"
    }

    fn platform(&self) -> PlatformType {
        PlatformType::Discord
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
            ConnectionMode::WebSocket => self.connect_websocket().await,
            ConnectionMode::Webhook => self.connect_webhook().await,
            _ => Err(AgentError::platform(
                "Discord does not support Polling mode",
            )),
        }
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.stop_listener().await?;

        if let Some(mut ws) = self.ws_stream.write().await.take() {
            let _ = ws.close(None).await;
        }

        *self.connected.write().await = false;
        info!("Disconnected from Discord");
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
                    if let Err(e) = channel.run_websocket_listener(event_bus).await {
                        error!("WebSocket listener error: {}", e);
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
                    "Discord does not support Polling mode",
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
            ContentType::Sticker,
            ContentType::Rich,
            ContentType::Reaction,
        ]
    }

    async fn list_channels(&self) -> Result<Vec<ChannelInfo>> {
        // Discord has APIs for this but requires guild context
        Ok(vec![])
    }

    async fn list_members(&self, _channel_id: &str) -> Result<Vec<MemberInfo>> {
        // Discord has APIs for this
        Ok(vec![])
    }

    fn connection_mode(&self) -> ConnectionMode {
        self.config.base.connection_mode
    }
}

impl Clone for DiscordChannel {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            http_client: self.http_client.clone(),
            connected: self.connected.clone(),
            ws_stream: Arc::new(RwLock::new(None)),
            session_id: self.session_id.clone(),
            last_seq: self.last_seq.clone(),
            heartbeat_interval: self.heartbeat_interval.clone(),
            listener_handle: Arc::new(RwLock::new(None)),
        }
    }
}

// ============================================================================
// Discord Channel Factory
// ============================================================================

use serde_json::Value;

use super::r#trait::ChannelFactory;

/// Discord channel factory
#[derive(Debug, Clone)]
pub struct DiscordChannelFactory;

impl DiscordChannelFactory {
    pub fn new() -> Self {
        Self
    }

    pub fn default_config() -> Value {
        json!({
            "bot_token": "",
            "connection_mode": "websocket",
            "auto_reconnect": true,
            "max_reconnect_attempts": 10,
            "intents": 1 << 0 | 1 << 9 | 1 << 12 | 1 << 15,
            "webhook_port": 8080,
        })
    }
}

#[async_trait]
impl ChannelFactory for DiscordChannelFactory {
    fn name(&self) -> &str {
        "discord"
    }

    fn platform_type(&self) -> super::PlatformType {
        super::PlatformType::Discord
    }

    async fn create(
        &self,
        config: &Value,
    ) -> crate::error::Result<Arc<RwLock<dyn super::Channel>>> {
        use crate::error::AgentError;

        let bot_token = config
            .get("bot_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AgentError::platform("Discord bot_token is required"))?
            .to_string();

        let channel = DiscordChannel::new(DiscordChannelConfig {
            bot_token,
            application_id: config
                .get("application_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            intents: config
                .get("intents")
                .and_then(|v| v.as_u64())
                .unwrap_or(1 << 0 | 1 << 9 | 1 << 12 | 1 << 15),
            base: BaseChannelConfig::default(),
        });

        Ok(Arc::new(RwLock::new(channel)))
    }

    fn validate_config(&self, config: &Value) -> bool {
        config
            .get("bot_token")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    }

    fn default_config(&self) -> Value {
        Self::default_config()
    }
}
