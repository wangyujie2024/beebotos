//! WebChat Channel Implementation
//!
//! First-class channel for the web admin "Chat" page.
//! Messages are injected via HTTP handler and replies are pushed back
//! through the WebSocket manager.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::{
    BaseChannelConfig, Channel, ChannelConfig, ChannelEvent, ChannelInfo, ConnectionMode,
    MemberInfo,
};
use crate::communication::channel::r#trait::ContentType;
use crate::communication::{Message, PlatformType};
use crate::error::{AgentError, Result};

/// WebChat channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebChatConfig {
    #[serde(flatten)]
    pub base: BaseChannelConfig,
}

impl Default for WebChatConfig {
    fn default() -> Self {
        Self {
            base: BaseChannelConfig {
                connection_mode: ConnectionMode::Webhook,
                auto_reconnect: false,
                max_reconnect_attempts: 0,
                webhook_port: 8000,
                webhook_url: None,
            },
        }
    }
}

impl ChannelConfig for WebChatConfig {
    fn from_env() -> Option<Self>
    where
        Self: Sized,
    {
        Some(Self::default())
    }

    fn is_valid(&self) -> bool {
        true
    }

    fn allowlist(&self) -> Vec<String> {
        vec![]
    }

    fn connection_mode(&self) -> ConnectionMode {
        ConnectionMode::Webhook
    }

    fn auto_reconnect(&self) -> bool {
        false
    }

    fn max_reconnect_attempts(&self) -> u32 {
        0
    }
}

/// WebChat channel
pub struct WebChatChannel {
    ws_manager: RwLock<Option<Arc<beebotos_gateway_lib::websocket::WebSocketManager>>>,
    connected: AtomicBool,
    config: WebChatConfig,
}

impl WebChatChannel {
    /// Create a new WebChat channel
    pub fn new(config: WebChatConfig) -> Self {
        Self {
            ws_manager: RwLock::new(None),
            connected: AtomicBool::new(true),
            config,
        }
    }

    /// Set the WebSocket manager used to push replies to the browser
    pub async fn set_ws_manager(
        &self,
        ws_manager: Arc<beebotos_gateway_lib::websocket::WebSocketManager>,
    ) {
        *self.ws_manager.write().await = Some(ws_manager);
        info!("WebChatChannel: WebSocket manager attached");
    }

    /// Build the payload sent over WebSocket
    fn build_payload(&self, channel_id: &str, message: &Message) -> serde_json::Value {
        let role = match message.message_type {
            crate::communication::MessageType::Image => "assistant",
            _ => "assistant",
        };

        serde_json::json!({
            "type": "chat_message",
            "session_id": channel_id,
            "message": {
                "id": message.id.to_string(),
                "role": role,
                "content": message.content,
                "timestamp": message.timestamp.to_rfc3339(),
                "attachments": [],
                "metadata": {},
                "token_usage": null
            }
        })
    }
}

#[async_trait]
impl Channel for WebChatChannel {
    fn name(&self) -> &str {
        "webchat"
    }

    fn platform(&self) -> PlatformType {
        PlatformType::WebChat
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    async fn connect(&mut self) -> Result<()> {
        info!("WebChatChannel: connect (no-op, uses HTTP/WebSocket)");
        self.connected.store(true, Ordering::Relaxed);
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        info!("WebChatChannel: disconnect");
        self.connected.store(false, Ordering::Relaxed);
        Ok(())
    }

    async fn send(&self, channel_id: &str, message: &Message) -> Result<()> {
        let ws = self.ws_manager.read().await;
        if let Some(ref manager) = *ws {
            let payload = self.build_payload(channel_id, message);
            manager
                .broadcast_to_channel("webchat", payload)
                .await
                .map_err(|e| AgentError::platform(format!("WebSocket broadcast failed: {}", e)))?;
            debug!(
                "WebChatChannel: reply broadcasted to channel {}",
                channel_id
            );
            Ok(())
        } else {
            warn!("WebChatChannel: WebSocket manager not set, cannot send reply");
            Err(AgentError::platform(
                "WebSocket manager not attached to WebChatChannel",
            ))
        }
    }

    async fn start_listener(
        &self,
        _event_bus: tokio::sync::mpsc::Sender<ChannelEvent>,
    ) -> Result<()> {
        // WebChat receives messages via HTTP handler, no persistent listener needed
        info!("WebChatChannel: start_listener (no-op, messages injected via HTTP)");
        Ok(())
    }

    async fn stop_listener(&self) -> Result<()> {
        info!("WebChatChannel: stop_listener (no-op)");
        Ok(())
    }

    fn supported_content_types(&self) -> Vec<ContentType> {
        vec![ContentType::Text, ContentType::Image]
    }

    async fn list_channels(&self) -> Result<Vec<ChannelInfo>> {
        Ok(vec![ChannelInfo {
            id: "webchat_default".to_string(),
            name: "WebChat".to_string(),
            channel_type: super::ChannelType::Direct,
            unread_count: 0,
            metadata: std::collections::HashMap::new(),
        }])
    }

    async fn list_members(&self, _channel_id: &str) -> Result<Vec<MemberInfo>> {
        Ok(vec![])
    }

    fn connection_mode(&self) -> ConnectionMode {
        self.config.base.connection_mode
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webchat_channel_platform() {
        let channel = WebChatChannel::new(WebChatConfig::default());
        assert_eq!(channel.platform(), PlatformType::WebChat);
        assert_eq!(channel.name(), "webchat");
    }
}
