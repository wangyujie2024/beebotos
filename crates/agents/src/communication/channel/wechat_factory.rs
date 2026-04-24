//! WeChat Work (企业微信) Channel Factory

use async_trait::async_trait;
use serde_json::Value;

use super::r#trait::{Channel, ChannelConfig, ChannelFactory};
use super::wechat_channel::{WeChatChannel, WeChatChannelConfig};
use crate::error::Result;

/// Factory for creating WeChat Work channels
pub struct WeChatFactory;

impl WeChatFactory {
    /// Create a new factory instance
    pub fn new() -> Self {
        Self
    }
}

impl Default for WeChatFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ChannelFactory for WeChatFactory {
    fn name(&self) -> &str {
        "wechat"
    }

    fn platform_type(&self) -> super::PlatformType {
        super::PlatformType::WeChat
    }

    async fn create(
        &self,
        config: &Value,
    ) -> Result<std::sync::Arc<tokio::sync::RwLock<dyn Channel>>> {
        // Transform config: rename 'secret' to 'corp_secret' if needed
        let mut config = config.clone();
        if let Some(obj) = config.as_object_mut() {
            if let Some(secret) = obj.remove("secret") {
                obj.insert("corp_secret".to_string(), secret);
            }
        }

        let config: WeChatChannelConfig = serde_json::from_value(config).map_err(|e| {
            crate::error::AgentError::configuration(format!("Invalid WeChat config: {}", e))
        })?;

        let channel = WeChatChannel::new(config);
        Ok(std::sync::Arc::new(tokio::sync::RwLock::new(channel)))
    }

    fn validate_config(&self, config: &Value) -> bool {
        // Transform config: rename 'secret' to 'corp_secret' if needed
        let mut config = config.clone();
        if let Some(obj) = config.as_object_mut() {
            if let Some(secret) = obj.remove("secret") {
                obj.insert("corp_secret".to_string(), secret);
            }
        }

        match serde_json::from_value::<WeChatChannelConfig>(config) {
            Ok(config) => {
                let valid = config.is_valid();
                tracing::info!(
                    "WeChat config parsed: corp_id={}, agent_id={}, corp_secret_len={}, \
                     is_valid={}",
                    config.corp_id,
                    config.agent_id,
                    config.corp_secret.len(),
                    valid
                );
                valid
            }
            Err(e) => {
                tracing::warn!("WeChat config parse error: {}", e);
                false
            }
        }
    }

    fn default_config(&self) -> Value {
        serde_json::json!({
            "corp_id": "",
            "corp_secret": "",
            "agent_id": "",
            "connection_mode": "poll"
        })
    }
}
