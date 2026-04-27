//! WebChat Channel Factory
//!
//! Factory for creating WebChatChannel instances.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::RwLock;
use tracing::info;

use super::webchat_channel::{WebChatChannel, WebChatConfig};
use super::{BaseChannelConfig, Channel, ChannelFactory, ConnectionMode};
use crate::communication::PlatformType;
use crate::error::Result;

/// Factory for creating WebChat channels
pub struct WebChatFactory;

impl WebChatFactory {
    /// Create a new factory instance
    pub fn new() -> Self {
        Self
    }

    /// Create default config as JSON Value
    pub fn default_config_json() -> Value {
        serde_json::json!({
            "connection_mode": "webhook",
            "auto_reconnect": false,
            "max_reconnect_attempts": 0,
            "webhook_port": 8000,
        })
    }
}

impl Default for WebChatFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ChannelFactory for WebChatFactory {
    fn name(&self) -> &str {
        "webchat"
    }

    fn platform_type(&self) -> PlatformType {
        PlatformType::WebChat
    }

    async fn create(&self, config: &Value) -> Result<Arc<RwLock<dyn Channel>>> {
        info!("Creating WebChat channel from config");

        let base_config = BaseChannelConfig {
            connection_mode: config
                .get("connection_mode")
                .and_then(|v| v.as_str())
                .and_then(|s| match s {
                    "webhook" => Some(ConnectionMode::Webhook),
                    "websocket" => Some(ConnectionMode::WebSocket),
                    "polling" => Some(ConnectionMode::Polling),
                    _ => None,
                })
                .unwrap_or(ConnectionMode::Webhook),
            auto_reconnect: config
                .get("auto_reconnect")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            max_reconnect_attempts: config
                .get("max_reconnect_attempts")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32)
                .unwrap_or(0),
            webhook_port: config
                .get("webhook_port")
                .and_then(|v| v.as_u64())
                .map(|v| v as u16)
                .unwrap_or(8000),
            webhook_url: config
                .get("webhook_url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        };

        let channel_config = WebChatConfig { base: base_config };
        let channel = WebChatChannel::new(channel_config);

        Ok(Arc::new(RwLock::new(channel)))
    }

    fn validate_config(&self, _config: &Value) -> bool {
        // WebChat has minimal configuration requirements
        true
    }

    fn default_config(&self) -> Value {
        Self::default_config_json()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factory_name() {
        let factory = WebChatFactory::new();
        assert_eq!(factory.name(), "webchat");
        assert_eq!(factory.platform_type(), PlatformType::WebChat);
    }

    #[test]
    fn test_validate_config() {
        let factory = WebChatFactory::new();
        assert!(factory.validate_config(&serde_json::json!({})));
    }
}
