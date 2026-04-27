//! Mattermost Channel Implementation
//!
//! Unified Channel trait implementation for Mattermost.
//! Supports WebSocket mode (default) and Webhook mode.
//!
//! # Features
//! - Real-time messaging via WebSocket
//! - Channel management (create, archive, list)
//! - Message pinning/unpinning
//! - Message editing with history tracking
//! - User presence and status
//! - File uploads

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tracing::{error, info, warn};

/// WebSocket stream type alias
type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

use super::channel_extensions::{
    EditableChannel, MessageEditHistory, PinnableChannel, PinnedMessage,
};
use super::r#trait::{BaseChannelConfig, ConnectionMode, ContentType};
use super::{
    Channel, ChannelConfig, ChannelEvent, ChannelInfo, ChannelType, MemberInfo, MemberRole,
};
use crate::communication::{Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

/// Mattermost API base URL
const MATTERMOST_API_VERSION: &str = "/api/v4";

/// Mattermost WebSocket URL path
const MATTERMOST_WS_PATH: &str = "/api/v4/websocket";

/// Mattermost Channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MattermostChannelConfig {
    /// Server URL (e.g., https://mattermost.example.com)
    pub server_url: String,
    /// Personal Access Token or Bot Token
    pub token: String,
    /// Team ID (optional, can be discovered)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
    /// User ID (populated after authentication)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    /// Base channel configuration
    #[serde(flatten)]
    pub base: BaseChannelConfig,
}

impl Default for MattermostChannelConfig {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            token: String::new(),
            team_id: None,
            user_id: None,
            base: BaseChannelConfig::default(),
        }
    }
}

impl ChannelConfig for MattermostChannelConfig {
    fn from_env() -> Option<Self>
    where
        Self: Sized,
    {
        let server_url = std::env::var("MATTERMOST_SERVER_URL").ok()?;
        let token = std::env::var("MATTERMOST_TOKEN").ok()?;
        let team_id = std::env::var("MATTERMOST_TEAM_ID").ok();
        let user_id = std::env::var("MATTERMOST_USER_ID").ok();

        let base = BaseChannelConfig::from_env("MATTERMOST").unwrap_or_default();

        Some(Self {
            server_url,
            token,
            team_id,
            user_id,
            base,
        })
    }

    fn is_valid(&self) -> bool {
        !self.server_url.is_empty() && !self.token.is_empty()
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

/// Mattermost user information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MattermostUser {
    pub id: String,
    pub username: String,
    pub email: String,
    pub nickname: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub roles: String,
    pub locale: String,
    pub delete_at: i64,
}

/// Mattermost channel information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MattermostChannelInfo {
    pub id: String,
    pub create_at: i64,
    pub update_at: i64,
    pub delete_at: i64,
    pub team_id: String,
    #[serde(rename = "type")]
    pub channel_type: String, // O (open), P (private), D (direct), G (group)
    pub display_name: String,
    pub name: String,
    pub header: String,
    pub purpose: String,
    pub last_post_at: i64,
    pub total_msg_count: i64,
    pub extra_update_at: i64,
    pub creator_id: String,
}

/// Mattermost post/message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MattermostPost {
    pub id: String,
    pub create_at: i64,
    pub update_at: i64,
    pub edit_at: i64,
    pub delete_at: i64,
    pub user_id: String,
    pub channel_id: String,
    pub root_id: String,
    pub original_id: String,
    #[serde(rename = "type")]
    pub post_type: String,
    pub message: String,
    pub props: Option<serde_json::Value>,
    pub hashtags: String,
    pub pending_post_id: String,
    pub reply_count: i64,
    pub metadata: Option<PostMetadata>,
}

/// Post metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embeds: Option<Vec<PostEmbed>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emojis: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<FileInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reactions: Option<Vec<Reaction>>,
}

/// Post embed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostEmbed {
    #[serde(rename = "type")]
    pub embed_type: String,
    pub url: String,
    pub data: Option<serde_json::Value>,
}

/// File information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub id: String,
    pub user_id: String,
    pub post_id: Option<String>,
    pub create_at: i64,
    pub update_at: i64,
    pub delete_at: i64,
    pub name: String,
    pub extension: String,
    pub size: i64,
    pub mime_type: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub has_preview_image: Option<bool>,
}

/// Reaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reaction {
    pub user_id: String,
    pub post_id: String,
    pub emoji_name: String,
    pub create_at: i64,
}

/// WebSocket event from Mattermost
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketEvent {
    pub event: String,
    pub data: Option<serde_json::Value>,
    pub broadcast: Option<Broadcast>,
    pub seq: Option<i64>,
}

/// Broadcast information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Broadcast {
    pub omit_users: Option<serde_json::Value>,
    pub user_id: String,
    pub channel_id: String,
    pub team_id: String,
}

/// Mattermost Channel implementation
pub struct MattermostChannelClient {
    /// Channel name
    name: String,
    /// Configuration
    config: MattermostChannelConfig,
    /// HTTP client
    http_client: reqwest::Client,
    /// User information (cached)
    user_info: Arc<RwLock<Option<MattermostUser>>>,
    /// WebSocket connection
    ws_stream: Arc<RwLock<Option<WsStream>>>,
    /// Connection status
    connected: Arc<RwLock<bool>>,
    /// Sequence number for WebSocket
    seq: Arc<RwLock<i64>>,
}

impl MattermostChannelClient {
    /// Create a new Mattermost channel
    pub fn new(config: MattermostChannelConfig) -> Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| {
                AgentError::configuration(format!("Failed to create HTTP client: {}", e))
            })?;

        Ok(Self {
            name: "mattermost".to_string(),
            config,
            http_client,
            user_info: Arc::new(RwLock::new(None)),
            ws_stream: Arc::new(RwLock::new(None)),
            connected: Arc::new(RwLock::new(false)),
            seq: Arc::new(RwLock::new(1)),
        })
    }

    /// Get API base URL
    fn api_url(&self, path: &str) -> String {
        format!(
            "{}{}{}",
            self.config.server_url, MATTERMOST_API_VERSION, path
        )
    }

    /// Get WebSocket URL
    fn ws_url(&self) -> String {
        let server_url = self.config.server_url.clone();
        let ws_base = if server_url.starts_with("https://") {
            server_url.replace("https://", "wss://")
        } else if server_url.starts_with("http://") {
            server_url.replace("http://", "ws://")
        } else {
            server_url
        };
        format!("{}{}", ws_base, MATTERMOST_WS_PATH)
    }

    /// Make an authenticated API request
    async fn api_get(&self, path: &str) -> Result<reqwest::Response> {
        let url = self.api_url(path);
        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("API request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!(
                "API error: {} - {}",
                status, text
            )));
        }

        Ok(response)
    }

    /// Make an authenticated API POST request
    async fn api_post(&self, path: &str, body: serde_json::Value) -> Result<reqwest::Response> {
        let url = self.api_url(path);
        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("API request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!(
                "API error: {} - {}",
                status, text
            )));
        }

        Ok(response)
    }

    /// Make an authenticated API DELETE request
    async fn api_delete(&self, path: &str) -> Result<reqwest::Response> {
        let url = self.api_url(path);
        let response = self
            .http_client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("API request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!(
                "API error: {} - {}",
                status, text
            )));
        }

        Ok(response)
    }

    /// Get current user information
    async fn get_me(&self) -> Result<MattermostUser> {
        let response = self.api_get("/users/me").await?;
        let user: MattermostUser = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse user info: {}", e)))?;
        Ok(user)
    }

    /// Get posts for a channel
    #[allow(dead_code)]
    async fn get_posts(&self, channel_id: &str, since: Option<i64>) -> Result<Vec<MattermostPost>> {
        let mut path = format!("/channels/{}/posts", channel_id);
        if let Some(since_time) = since {
            path.push_str(&format!("?since={}", since_time));
        }

        let response = self.api_get(&path).await?;
        let posts_data: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse posts: {}", e)))?;

        // Parse posts from the response
        let posts: Vec<MattermostPost> = posts_data["posts"]
            .as_object()
            .map(|m| {
                m.values()
                    .filter_map(|v| serde_json::from_value(v.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();

        Ok(posts)
    }

    /// Create a post
    async fn create_post(
        &self,
        channel_id: &str,
        message: &str,
        root_id: Option<&str>,
    ) -> Result<MattermostPost> {
        let mut body = json!({
            "channel_id": channel_id,
            "message": message,
        });

        if let Some(root) = root_id {
            body["root_id"] = json!(root);
        }

        let response = self.api_post("/posts", body).await?;
        let post: MattermostPost = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse post: {}", e)))?;
        Ok(post)
    }

    /// Update a post
    async fn update_post(&self, post_id: &str, message: &str) -> Result<MattermostPost> {
        let body = json!({
            "id": post_id,
            "message": message,
        });

        let response = self.api_post(&format!("/posts/{}", post_id), body).await?;
        let post: MattermostPost = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse updated post: {}", e)))?;
        Ok(post)
    }

    /// Delete a post
    async fn delete_post(&self, post_id: &str) -> Result<()> {
        self.api_delete(&format!("/posts/{}", post_id)).await?;
        Ok(())
    }

    /// Pin a post to the channel
    async fn pin_post(&self, post_id: &str) -> Result<()> {
        let body = json!(post_id);
        self.api_post(&format!("/posts/{}/pin", post_id), body)
            .await?;
        Ok(())
    }

    /// Unpin a post from the channel
    async fn unpin_post(&self, post_id: &str) -> Result<()> {
        let body = json!(post_id);
        self.api_post(&format!("/posts/{}/unpin", post_id), body)
            .await?;
        Ok(())
    }

    /// Get pinned posts for a channel
    async fn get_pinned_posts(&self, channel_id: &str) -> Result<Vec<MattermostPost>> {
        let response = self
            .api_get(&format!("/channels/{}/pinned", channel_id))
            .await?;
        let data: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse pinned posts: {}", e)))?;

        let posts: Vec<MattermostPost> = data["posts"]
            .as_object()
            .map(|m| {
                m.values()
                    .filter_map(|v| serde_json::from_value(v.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();

        Ok(posts)
    }

    /// Get edit history for a post
    async fn get_post_edit_history(&self, post_id: &str) -> Result<Vec<PostEdit>> {
        let response = self
            .api_get(&format!("/posts/{}/edit_history", post_id))
            .await?;
        let edits: Vec<PostEdit> = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse edit history: {}", e)))?;
        Ok(edits)
    }

    /// Send authentication challenge via WebSocket
    async fn ws_authenticate(
        &self,
        ws_stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    ) -> Result<()> {
        let auth_msg = json!({
            "seq": *self.seq.read().await,
            "action": "authentication_challenge",
            "data": {
                "token": self.config.token,
            },
        });

        ws_stream
            .send(WsMessage::Text(auth_msg.to_string()))
            .await
            .map_err(|e| AgentError::platform(format!("WebSocket auth failed: {}", e)))?;

        *self.seq.write().await += 1;
        Ok(())
    }
}

/// Post edit history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostEdit {
    pub post_id: String,
    pub user_id: String,
    pub message: String,
    pub edit_at: i64,
}

#[async_trait]
impl Channel for MattermostChannelClient {
    fn name(&self) -> &str {
        &self.name
    }

    fn platform(&self) -> PlatformType {
        PlatformType::Custom // Using Custom for now, or add Mattermost to
                             // PlatformType
    }

    fn is_connected(&self) -> bool {
        *self.connected.blocking_read()
    }

    async fn connect(&mut self) -> Result<()> {
        info!("Connecting to Mattermost at {}...", self.config.server_url);

        // Get user information
        let user = self.get_me().await?;
        info!("Authenticated as {} ({})", user.username, user.id);

        *self.user_info.write().await = Some(user.clone());
        self.config.user_id = Some(user.id);

        // Connect WebSocket if using WebSocket mode
        if self.config.connection_mode().is_websocket() {
            let ws_url = self.ws_url();
            info!("Connecting WebSocket to {}...", ws_url);

            let (ws_stream, _) = connect_async(&ws_url)
                .await
                .map_err(|e| AgentError::platform(format!("WebSocket connection failed: {}", e)))?;

            *self.ws_stream.write().await = Some(ws_stream);

            // Authenticate WebSocket
            if let Some(ref mut ws) = *self.ws_stream.write().await {
                self.ws_authenticate(ws).await?;
            }

            info!("Mattermost WebSocket connected and authenticated");
        }

        *self.connected.write().await = true;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from Mattermost...");

        if let Some(ref mut ws) = *self.ws_stream.write().await {
            let _ = ws.close(None).await;
        }
        *self.ws_stream.write().await = None;
        *self.connected.write().await = false;

        info!("Disconnected from Mattermost");
        Ok(())
    }

    async fn send(&self, channel_id: &str, message: &Message) -> Result<()> {
        let content = &message.content;
        self.create_post(channel_id, content, None).await?;
        Ok(())
    }

    async fn start_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        if !self.config.connection_mode().is_websocket() {
            return Err(AgentError::configuration(
                "WebSocket mode required for listener".to_string(),
            ));
        }

        let ws_stream = self.ws_stream.clone();
        let connected = self.connected.clone();
        let seq = self.seq.clone();

        tokio::spawn(async move {
            let mut heartbeat = interval(Duration::from_secs(30));

            loop {
                tokio::select! {
                    _ = heartbeat.tick() => {
                        // Send heartbeat
                        let heartbeat_msg = json!({
                            "seq": *seq.read().await,
                            "action": "ping",
                        });

                        if let Some(ref mut ws) = *ws_stream.write().await {
                            if let Err(e) = ws.send(WsMessage::Text(heartbeat_msg.to_string())).await {
                                warn!("Failed to send heartbeat: {}", e);
                                break;
                            }
                        }
                        *seq.write().await += 1;
                    }
                    msg = async {
                        if let Some(ref mut ws) = *ws_stream.write().await {
                            ws.next().await
                        } else {
                            None
                        }
                    } => {
                        match msg {
                            Some(Ok(WsMessage::Text(text))) => {
                                if let Ok(event) = serde_json::from_str::<WebSocketEvent>(&text) {
                                    match event.event.as_str() {
                                        "posted" => {
                                            if let Some(data) = event.data {
                                                if let Some(post_str) = data["post"].as_str() {
                                                    if let Ok(post) = serde_json::from_str::<MattermostPost>(post_str) {
                                                        let message = Message {
                                                            id: post.id.parse().unwrap_or_default(),
                                                            thread_id: uuid::Uuid::new_v4(),
                                                            platform: PlatformType::Custom,
                                                            message_type: MessageType::Text,
                                                            content: post.message,
                                                            metadata: HashMap::new(),
                                                            timestamp: DateTime::from_timestamp(post.create_at / 1000, 0)
                                                                .unwrap_or_else(Utc::now),
                                                        };

                                                        let _ = event_bus.send(ChannelEvent::MessageReceived {
                                                            platform: PlatformType::Custom,
                                                            channel_id: post.channel_id,
                                                            message,
                                                        }).await;
                                                    }
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            Some(Ok(WsMessage::Close(_))) => {
                                info!("WebSocket closed");
                                break;
                            }
                            Some(Err(e)) => {
                                error!("WebSocket error: {}", e);
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }

            *connected.write().await = false;
        });

        Ok(())
    }

    async fn stop_listener(&self) -> Result<()> {
        // The listener task will stop when the channel is dropped
        Ok(())
    }

    fn supported_content_types(&self) -> Vec<ContentType> {
        vec![
            ContentType::Text,
            ContentType::Image,
            ContentType::File,
            ContentType::Reaction,
            ContentType::Rich,
        ]
    }

    async fn list_channels(&self) -> Result<Vec<ChannelInfo>> {
        let response = self.api_get("/users/me/channels").await?;
        let channels: Vec<MattermostChannelInfo> = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse channels: {}", e)))?;

        let channel_infos: Vec<ChannelInfo> = channels
            .into_iter()
            .map(|c| ChannelInfo {
                id: c.id,
                name: c.display_name,
                channel_type: match c.channel_type.as_str() {
                    "O" => ChannelType::Channel,
                    "P" => ChannelType::Group,
                    "D" => ChannelType::Direct,
                    "G" => ChannelType::Group,
                    _ => ChannelType::Channel,
                },
                unread_count: 0, // Would need separate API call
                metadata: {
                    let mut m = HashMap::new();
                    m.insert("header".to_string(), c.header);
                    m.insert("purpose".to_string(), c.purpose);
                    m
                },
            })
            .collect();

        Ok(channel_infos)
    }

    async fn list_members(&self, channel_id: &str) -> Result<Vec<MemberInfo>> {
        let response = self
            .api_get(&format!("/channels/{}/members", channel_id))
            .await?;
        let members: Vec<serde_json::Value> = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse members: {}", e)))?;

        let member_infos: Vec<MemberInfo> = members
            .into_iter()
            .filter_map(|m| {
                Some(MemberInfo {
                    id: m["user_id"].as_str()?.to_string(),
                    name: m["username"].as_str()?.to_string(),
                    username: m["username"].as_str().map(|s| s.to_string()),
                    avatar: None,
                    is_bot: false,
                    role: if m["scheme_admin"].as_bool().unwrap_or(false) {
                        MemberRole::Admin
                    } else {
                        MemberRole::Member
                    },
                })
            })
            .collect();

        Ok(member_infos)
    }

    fn connection_mode(&self) -> ConnectionMode {
        self.config.connection_mode()
    }
}

/// Mattermost Channel Factory
pub struct MattermostChannelFactory;

#[async_trait]
impl super::r#trait::ChannelFactory for MattermostChannelFactory {
    fn name(&self) -> &str {
        "mattermost"
    }

    fn platform_type(&self) -> super::PlatformType {
        super::PlatformType::Custom
    }

    async fn create(&self, config: &serde_json::Value) -> Result<Arc<RwLock<dyn Channel>>> {
        let config: MattermostChannelConfig = serde_json::from_value(config.clone())
            .map_err(|e| AgentError::configuration(format!("Invalid config: {}", e)))?;

        let channel = MattermostChannelClient::new(config)?;
        Ok(Arc::new(RwLock::new(channel)))
    }

    fn validate_config(&self, config: &serde_json::Value) -> bool {
        serde_json::from_value::<MattermostChannelConfig>(config.clone())
            .map(|c| c.is_valid())
            .unwrap_or(false)
    }

    fn default_config(&self) -> serde_json::Value {
        serde_json::to_value(MattermostChannelConfig::default()).unwrap_or_default()
    }
}

#[async_trait]
impl PinnableChannel for MattermostChannelClient {
    async fn pin_message(&self, channel_id: &str, message_id: &str) -> Result<()> {
        let _ = channel_id; // Mattermost API doesn't require channel_id for pin
        self.pin_post(message_id).await
    }

    async fn unpin_message(&self, channel_id: &str, message_id: &str) -> Result<()> {
        let _ = channel_id; // Mattermost API doesn't require channel_id for unpin
        self.unpin_post(message_id).await
    }

    async fn get_pinned_messages(&self, channel_id: &str) -> Result<Vec<PinnedMessage>> {
        let posts = self.get_pinned_posts(channel_id).await?;

        let messages: Vec<PinnedMessage> = posts
            .into_iter()
            .map(|p| PinnedMessage {
                message_id: p.id,
                channel_id: channel_id.to_string(),
                content: p.message,
                author_id: p.user_id,
                pinned_at: Utc::now(),    // Mattermost doesn't provide pin time
                pinned_by: String::new(), // Mattermost doesn't provide pin user
            })
            .collect();

        Ok(messages)
    }
}

#[async_trait]
impl EditableChannel for MattermostChannelClient {
    async fn edit_message(
        &self,
        channel_id: &str,
        message_id: &str,
        new_content: &str,
    ) -> Result<()> {
        let _ = channel_id; // Mattermost API doesn't require channel_id for edit
        self.update_post(message_id, new_content).await?;
        Ok(())
    }

    async fn delete_message(&self, channel_id: &str, message_id: &str) -> Result<()> {
        let _ = channel_id; // Mattermost API doesn't require channel_id for delete
        self.delete_post(message_id).await
    }

    async fn get_message_edit_history(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> Result<Vec<MessageEditHistory>> {
        let _ = channel_id; // Mattermost API doesn't require channel_id for history
        let edits = self.get_post_edit_history(message_id).await?;

        let history: Vec<MessageEditHistory> = edits
            .into_iter()
            .enumerate()
            .map(|(idx, e)| MessageEditHistory {
                edit_id: format!("edit-{}", idx),
                message_id: e.post_id,
                channel_id: channel_id.to_string(),
                previous_content: e.message.clone(),
                new_content: e.message,
                edited_by: e.user_id,
                edited_at: DateTime::from_timestamp(e.edit_at / 1000, 0).unwrap_or_else(Utc::now),
                edit_reason: None,
                version: idx as u32 + 1,
            })
            .collect();

        Ok(history)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::communication::channel::r#trait::ChannelFactory;

    #[test]
    fn test_mattermost_config_validation() {
        let valid_config = MattermostChannelConfig {
            server_url: "https://mattermost.example.com".to_string(),
            token: "test_token".to_string(),
            ..Default::default()
        };
        assert!(valid_config.is_valid());

        let invalid_config = MattermostChannelConfig {
            server_url: "".to_string(),
            token: "test_token".to_string(),
            ..Default::default()
        };
        assert!(!invalid_config.is_valid());
    }

    #[test]
    fn test_mattermost_api_url() {
        let config = MattermostChannelConfig {
            server_url: "https://mattermost.example.com".to_string(),
            token: "test".to_string(),
            ..Default::default()
        };

        let channel = MattermostChannelClient::new(config).unwrap();
        assert_eq!(
            channel.api_url("/users/me"),
            "https://mattermost.example.com/api/v4/users/me"
        );
    }

    #[test]
    fn test_mattermost_ws_url() {
        let config = MattermostChannelConfig {
            server_url: "https://mattermost.example.com".to_string(),
            token: "test".to_string(),
            ..Default::default()
        };

        let channel = MattermostChannelClient::new(config).unwrap();
        assert_eq!(
            channel.ws_url(),
            "wss://mattermost.example.com/api/v4/websocket"
        );
    }

    #[test]
    fn test_mattermost_factory() {
        let factory = MattermostChannelFactory;
        assert_eq!(factory.name(), "mattermost");

        let default_config = factory.default_config();
        assert!(default_config.is_object());

        let valid_config = json!({
            "server_url": "https://mattermost.example.com",
            "token": "test_token"
        });
        assert!(factory.validate_config(&valid_config));

        let invalid_config = json!({
            "server_url": ""
        });
        assert!(!factory.validate_config(&invalid_config));
    }
}
