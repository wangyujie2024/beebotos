//! DingTalk Channel Implementation
//!
//! Unified Channel trait implementation for DingTalk (钉钉).
//! Supports WebSocket mode (default), Webhook mode, and Polling mode.

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
use uuid::Uuid;

use super::r#trait::{BaseChannelConfig, ConnectionMode, ContentType};
use super::{Channel, ChannelConfig, ChannelEvent, ChannelInfo, MemberInfo};
use crate::communication::{Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

/// DingTalk WebSocket URL for Stream API
const DINGTALK_WS_URL: &str = "wss://comet.dingtalk.com/comet_server";

/// DingTalk API base URL
const DINGTALK_API_BASE: &str = "https://oapi.dingtalk.com";

/// DingTalk Channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DingTalkChannelConfig {
    /// App key
    pub app_key: String,
    /// App secret
    pub app_secret: String,
    /// Custom WebSocket URL (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ws_url: Option<String>,
    /// Base channel configuration (flattened)
    #[serde(flatten)]
    pub base: BaseChannelConfig,
}

impl Default for DingTalkChannelConfig {
    fn default() -> Self {
        Self {
            app_key: String::new(),
            app_secret: String::new(),
            ws_url: None,
            base: BaseChannelConfig::default(),
        }
    }
}

impl ChannelConfig for DingTalkChannelConfig {
    fn from_env() -> Option<Self>
    where
        Self: Sized,
    {
        let app_key = std::env::var("DINGTALK_APP_KEY").ok()?;
        let app_secret = std::env::var("DINGTALK_APP_SECRET").ok()?;
        let ws_url = std::env::var("DINGTALK_WS_URL").ok();

        let base = BaseChannelConfig::from_env("DINGTALK")?;

        Some(Self {
            app_key,
            app_secret,
            ws_url,
            base,
        })
    }

    fn is_valid(&self) -> bool {
        !self.app_key.is_empty() && !self.app_secret.is_empty()
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

/// DingTalk WebSocket connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DingTalkWsState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

/// DingTalk API response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DingTalkTokenResponse {
    pub errcode: i32,
    pub errmsg: String,
    #[serde(rename = "access_token")]
    pub access_token: Option<String>,
    #[serde(rename = "expires_in")]
    pub expires_in: Option<i64>,
}

/// DingTalk message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DingTalkMessage {
    pub msgtype: String,
    pub text: Option<DingTalkTextContent>,
    pub markdown: Option<DingTalkMarkdownContent>,
}

/// DingTalk text content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DingTalkTextContent {
    pub content: String,
}

/// DingTalk markdown content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DingTalkMarkdownContent {
    pub title: String,
    pub text: String,
}

/// DingTalk send response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DingTalkSendResponse {
    pub errcode: i32,
    pub errmsg: String,
    #[serde(rename = "message_id")]
    pub message_id: Option<String>,
}

/// DingTalk WebSocket message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DingTalkWsMessage {
    #[serde(rename = "connect")]
    Connect { data: serde_json::Value },
    #[serde(rename = "event")]
    Event { data: serde_json::Value },
    #[serde(rename = "ping")]
    Ping,
    #[serde(rename = "pong")]
    Pong,
    #[serde(rename = "error")]
    Error { code: i32, message: String },
}

/// DingTalk Channel implementation
pub struct DingTalkChannel {
    config: DingTalkChannelConfig,
    http_client: reqwest::Client,
    access_token: Arc<RwLock<Option<String>>>,
    connected: Arc<RwLock<bool>>,
    ws_state: Arc<RwLock<DingTalkWsState>>,
    ws_sender: Arc<RwLock<Option<mpsc::UnboundedSender<WsMessage>>>>,
    listener_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    /// Event bus for emitting channel events (P1 FIX)
    event_bus: Arc<RwLock<Option<mpsc::Sender<ChannelEvent>>>>,
}

impl DingTalkChannel {
    /// Create a new DingTalk channel
    pub fn new(config: DingTalkChannelConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
            access_token: Arc::new(RwLock::new(None)),
            connected: Arc::new(RwLock::new(false)),
            ws_state: Arc::new(RwLock::new(DingTalkWsState::Disconnected)),
            ws_sender: Arc::new(RwLock::new(None)),
            listener_handle: Arc::new(RwLock::new(None)),
            event_bus: Arc::new(RwLock::new(None)),
        }
    }

    /// Create from environment variables
    pub fn from_env() -> Result<Self> {
        let config = DingTalkChannelConfig::from_env().ok_or_else(|| {
            AgentError::configuration("DINGTALK_APP_KEY or DINGTALK_APP_SECRET not set")
        })?;
        Ok(Self::new(config))
    }

    /// Get access token
    async fn get_access_token(&self) -> Result<String> {
        // Check if we have a cached token
        if let Some(token) = self.access_token.read().await.clone() {
            return Ok(token);
        }

        // Fetch new token
        let url = format!(
            "{}/gettoken?appkey={}&appsecret={}",
            DINGTALK_API_BASE, self.config.app_key, self.config.app_secret
        );

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to get access token: {}", e)))?;

        let token_response: DingTalkTokenResponse = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse token response: {}", e)))?;

        if token_response.errcode != 0 {
            return Err(AgentError::authentication(format!(
                "DingTalk auth failed: {}",
                token_response.errmsg
            ))
            .into());
        }

        let token = token_response
            .access_token
            .ok_or_else(|| AgentError::authentication("No access token in response"))?;

        *self.access_token.write().await = Some(token.clone());
        Ok(token)
    }

    /// Send message to user
    pub async fn send_to_user(&self, userid: &str, message: DingTalkMessage) -> Result<String> {
        let token = self.get_access_token().await?;
        let url = format!(
            "{}/topapi/message/corpconversation/asyncsend_v2",
            DINGTALK_API_BASE
        );

        let body = serde_json::json!({
            "access_token": token,
            "userid_list": userid,
            "msg": message,
        });

        let response = self
            .http_client
            .post(&url)
            .query(&[("access_token", &token)])
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to send message: {}", e)))?;

        let send_response: DingTalkSendResponse = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse send response: {}", e)))?;

        if send_response.errcode != 0 {
            return Err(AgentError::platform(format!(
                "Failed to send message: {}",
                send_response.errmsg
            ))
            .into());
        }

        Ok(send_response.message_id.unwrap_or_default())
    }

    /// Send text message to user
    pub async fn send_text_message(&self, userid: &str, content: &str) -> Result<String> {
        let message = DingTalkMessage {
            msgtype: "text".to_string(),
            text: Some(DingTalkTextContent {
                content: content.to_string(),
            }),
            markdown: None,
        };
        self.send_to_user(userid, message).await
    }

    /// Send markdown message to user
    pub async fn send_markdown_message(
        &self,
        userid: &str,
        title: &str,
        text: &str,
    ) -> Result<String> {
        let message = DingTalkMessage {
            msgtype: "markdown".to_string(),
            text: None,
            markdown: Some(DingTalkMarkdownContent {
                title: title.to_string(),
                text: text.to_string(),
            }),
        };
        self.send_to_user(userid, message).await
    }

    /// Connect via WebSocket
    async fn connect_websocket(&self) -> Result<()> {
        let token = self.get_access_token().await?;

        {
            let mut state = self.ws_state.write().await;
            *state = DingTalkWsState::Connecting;
        }

        let ws_url = self
            .config
            .ws_url
            .clone()
            .unwrap_or_else(|| DINGTALK_WS_URL.to_string());
        let ws_url = format!("{}?access_token={}", ws_url, token);

        let (ws_stream, _) = connect_async(&ws_url)
            .await
            .map_err(|e| AgentError::platform(format!("WebSocket connection failed: {}", e)))?;

        info!("DingTalk WebSocket connected");

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        let (tx, mut rx) = mpsc::unbounded_channel::<WsMessage>();
        *self.ws_sender.write().await = Some(tx);

        {
            let mut state = self.ws_state.write().await;
            *state = DingTalkWsState::Connected;
        }
        *self.connected.write().await = true;

        // Spawn WebSocket handler
        let ws_state = self.ws_state.clone();
        let auto_reconnect = self.config.base.auto_reconnect;
        let _max_reconnect_attempts = self.config.base.max_reconnect_attempts;
        let event_bus = self.event_bus.clone();

        tokio::spawn(async move {
            let mut heartbeat_interval = interval(Duration::from_secs(30));

            loop {
                tokio::select! {
                    Some(msg) = ws_receiver.next() => {
                        match msg {
                            Ok(WsMessage::Text(text)) => {
                                debug!("Received WebSocket message: {}", text);
                                if let Err(e) = Self::handle_websocket_message(&text, event_bus.clone()).await {
                                    warn!("Failed to handle WebSocket message: {}", e);
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
                        let ping = serde_json::json!({"type": "ping"});
                        if let Err(e) = ws_sender.send(WsMessage::Text(ping.to_string())).await {
                            error!("Failed to send heartbeat: {}", e);
                            break;
                        }
                    }
                }
            }

            {
                let mut state = ws_state.write().await;
                *state = DingTalkWsState::Disconnected;
            }

            if auto_reconnect {
                info!("Attempting to reconnect to DingTalk WebSocket...");
            }
        });

        Ok(())
    }

    /// P2 OPTIMIZE: Robustly extract text content from a DingTalk event
    /// payload. Handles multiple message types and nested structures.
    fn extract_event_content(data: &serde_json::Value) -> String {
        // Try direct content field first
        if let Some(s) = data.get("content").and_then(|v| v.as_str()) {
            if !s.is_empty() {
                return s.to_string();
            }
        }
        // Try text.msg_type content
        if let Some(text_obj) = data.get("text") {
            if let Some(s) = text_obj.get("content").and_then(|v| v.as_str()) {
                if !s.is_empty() {
                    return s.to_string();
                }
            }
        }
        // Try markdown content
        if let Some(md) = data.get("markdown") {
            if let Some(s) = md.get("text").and_then(|v| v.as_str()) {
                if !s.is_empty() {
                    return s.to_string();
                }
            }
        }
        // Try action card
        if let Some(card) = data.get("action_card") {
            if let Some(s) = card.get("markdown").and_then(|v| v.as_str()) {
                if !s.is_empty() {
                    return s.to_string();
                }
            }
        }
        // Try rich text (oa message)
        if let Some(body) = data.get("body") {
            if let Some(s) = body.get("content").and_then(|v| v.as_str()) {
                if !s.is_empty() {
                    return s.to_string();
                }
            }
        }
        // Try title as last resort
        if let Some(s) = data.get("title").and_then(|v| v.as_str()) {
            if !s.is_empty() {
                return s.to_string();
            }
        }
        String::new()
    }

    /// P2 OPTIMIZE: Extract a string field from a JSON object using a
    /// prioritized list of keys.
    fn extract_event_field(data: &serde_json::Value, keys: &[&str]) -> String {
        for key in keys {
            if let Some(s) = data.get(*key).and_then(|v| v.as_str()) {
                if !s.is_empty() {
                    return s.to_string();
                }
            }
        }
        String::new()
    }

    /// Handle WebSocket message
    async fn handle_websocket_message(
        text: &str,
        event_bus: Arc<RwLock<Option<mpsc::Sender<ChannelEvent>>>>,
    ) -> Result<()> {
        match serde_json::from_str::<DingTalkWsMessage>(text) {
            Ok(msg) => {
                match msg {
                    DingTalkWsMessage::Connect { data } => {
                        debug!("WebSocket connection established: {:?}", data);
                    }
                    DingTalkWsMessage::Event { data } => {
                        debug!("Received event: {:?}", data);
                        // P1/P2 FIX: Emit ChannelEvent through event_bus with robust field
                        // extraction
                        if let Some(bus) = event_bus.read().await.as_ref() {
                            // Robust content extraction: handles text, markdown, rich text, etc.
                            let content = Self::extract_event_content(&data);
                            // Robust sender ID extraction with multiple fallback keys
                            let sender_id = Self::extract_event_field(
                                &data,
                                &[
                                    "senderStaffId",
                                    "senderUserId",
                                    "senderStaffId",
                                    "staffId",
                                    "userId",
                                    "senderNick",
                                    "senderName",
                                ],
                            );
                            // Robust chat/channel ID extraction with multiple fallback keys
                            let chat_id = Self::extract_event_field(
                                &data,
                                &[
                                    "conversationId",
                                    "chatId",
                                    "openConversationId",
                                    "groupId",
                                    "chatbotCorpId",
                                    "corpId",
                                ],
                            );

                            if content.is_empty() {
                                debug!("DingTalk event has no extractable content, skipping");
                            } else {
                                let mut message =
                                    Message::new(Uuid::new_v4(), PlatformType::DingTalk, content);
                                message
                                    .metadata
                                    .insert("sender_id".to_string(), sender_id.clone());
                                message
                                    .metadata
                                    .insert("channel_id".to_string(), chat_id.clone());
                                // Preserve raw event type if available
                                if let Some(event_type) = data
                                    .get("msgtype")
                                    .or_else(|| data.get("msgType"))
                                    .and_then(|v| v.as_str())
                                {
                                    message
                                        .metadata
                                        .insert("msg_type".to_string(), event_type.to_string());
                                }
                                if let Err(e) = bus
                                    .send(ChannelEvent::MessageReceived {
                                        platform: PlatformType::DingTalk,
                                        channel_id: chat_id,
                                        message,
                                    })
                                    .await
                                {
                                    warn!("Failed to emit DingTalk ChannelEvent: {}", e);
                                } else {
                                    info!(
                                        "📨 Emitted DingTalk ChannelEvent from sender {}",
                                        sender_id
                                    );
                                }
                            }
                        }
                    }
                    DingTalkWsMessage::Ping => {
                        debug!("Received ping");
                    }
                    DingTalkWsMessage::Pong => {
                        debug!("Received pong");
                    }
                    DingTalkWsMessage::Error { code, message } => {
                        error!("DingTalk WebSocket error: {} - {}", code, message);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to parse WebSocket message: {}", e);
            }
        }
        Ok(())
    }

    /// Connect via Webhook
    async fn connect_webhook(&self) -> Result<()> {
        // Verify token
        self.get_access_token().await?;
        *self.connected.write().await = true;
        info!("DingTalk webhook mode activated");
        Ok(())
    }

    /// Connect via Polling
    async fn connect_polling(&self) -> Result<()> {
        // Verify token
        self.get_access_token().await?;
        *self.connected.write().await = true;
        info!("DingTalk polling mode activated");
        Ok(())
    }

    /// Run WebSocket listener
    async fn run_websocket_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        // P1 FIX: Store event_bus so connect_websocket can emit events
        *self.event_bus.write().await = Some(event_bus);
        // Keep this task alive so listener_handle is non-empty
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }

    /// Run webhook listener
    async fn run_webhook_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        info!(
            "DingTalk webhook listener started on port {}",
            self.config.base.webhook_port
        );
        // P1 FIX: Store event_bus
        *self.event_bus.write().await = Some(event_bus);
        // TODO: Implement HTTP server for webhook
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }

    /// Run polling listener
    async fn run_polling_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        info!("DingTalk polling listener started");
        // P1 FIX: Store event_bus
        *self.event_bus.write().await = Some(event_bus);
        // TODO: Implement polling for events
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }
}

#[async_trait]
impl Channel for DingTalkChannel {
    fn name(&self) -> &str {
        "dingtalk"
    }

    fn platform(&self) -> PlatformType {
        PlatformType::DingTalk
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
            ConnectionMode::Polling => self.connect_polling().await,
        }
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.stop_listener().await?;

        // Close WebSocket connection if active
        if let Some(sender) = self.ws_sender.write().await.take() {
            let _ = sender.send(WsMessage::Close(None));
        }

        {
            let mut state = self.ws_state.write().await;
            *state = DingTalkWsState::Disconnected;
        }

        *self.access_token.write().await = None;
        *self.connected.write().await = false;

        info!("Disconnected from DingTalk");
        Ok(())
    }

    async fn send(&self, channel_id: &str, message: &Message) -> Result<()> {
        // For DingTalk, channel_id is the user ID
        match message.message_type {
            MessageType::Text => {
                self.send_text_message(channel_id, &message.content).await?;
            }
            _ => {
                // Default to text for other types
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
            ConnectionMode::Polling => {
                let channel = self.clone();
                let handle = tokio::spawn(async move {
                    if let Err(e) = channel.run_polling_listener(event_bus).await {
                        error!("Polling listener error: {}", e);
                    }
                });
                *self.listener_handle.write().await = Some(handle);
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
        // DingTalk doesn't have a direct API to list all conversations
        Ok(vec![])
    }

    async fn list_members(&self, _channel_id: &str) -> Result<Vec<MemberInfo>> {
        // DingTalk has APIs for this but requires specific permissions
        Ok(vec![])
    }

    fn connection_mode(&self) -> ConnectionMode {
        self.config.base.connection_mode
    }
}

impl Clone for DingTalkChannel {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            http_client: self.http_client.clone(),
            access_token: self.access_token.clone(),
            connected: self.connected.clone(),
            ws_state: self.ws_state.clone(),
            ws_sender: Arc::new(RwLock::new(None)),
            listener_handle: Arc::new(RwLock::new(None)),
            event_bus: self.event_bus.clone(),
        }
    }
}

// ============================================================================
// DingTalk Channel Factory
// ============================================================================

use serde_json::Value;

use super::r#trait::ChannelFactory;

/// DingTalk channel factory
#[derive(Debug, Clone)]
pub struct DingTalkChannelFactory;

impl DingTalkChannelFactory {
    pub fn new() -> Self {
        Self
    }

    pub fn default_config() -> Value {
        json!({
            "app_key": "",
            "app_secret": "",
            "connection_mode": "websocket",
            "auto_reconnect": true,
            "max_reconnect_attempts": 10,
            "webhook_port": 8080,
        })
    }
}

#[async_trait]
impl ChannelFactory for DingTalkChannelFactory {
    fn name(&self) -> &str {
        "dingtalk"
    }

    fn platform_type(&self) -> super::PlatformType {
        super::PlatformType::DingTalk
    }

    async fn create(
        &self,
        config: &Value,
    ) -> crate::error::Result<Arc<RwLock<dyn super::Channel>>> {
        use crate::error::AgentError;

        let app_key = config
            .get("app_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AgentError::platform("DingTalk app_key is required"))?
            .to_string();

        let app_secret = config
            .get("app_secret")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AgentError::platform("DingTalk app_secret is required"))?
            .to_string();

        let channel = DingTalkChannel::new(DingTalkChannelConfig {
            app_key,
            app_secret,
            ws_url: None,
            base: super::r#trait::BaseChannelConfig {
                connection_mode: super::r#trait::ConnectionMode::WebSocket,
                auto_reconnect: true,
                max_reconnect_attempts: 10,
                webhook_url: None,
                webhook_port: config
                    .get("webhook_port")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(8080) as u16,
            },
        });

        Ok(Arc::new(RwLock::new(channel)))
    }

    fn validate_config(&self, config: &Value) -> bool {
        let has_key = config
            .get("app_key")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false);

        let has_secret = config
            .get("app_secret")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false);

        has_key && has_secret
    }

    fn default_config(&self) -> Value {
        Self::default_config()
    }
}
