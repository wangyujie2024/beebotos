//! Personal WeChat Channel Factory (iLink Protocol)
//!
//! Factory implementation for creating PersonalWeChatChannel instances.
//! Uses Tencent's official iLink Bot API for direct WeChat personal account
//! integration.
//!
//! Features:
//! - QR code login with 24h session
//! - Auto-reconnection before session expiration
//! - Long-polling message reception

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::personal_wechat_channel::{PersonalWeChatChannel, PersonalWeChatConfig};
use super::{Channel, ChannelFactory};

/// Factory for creating Personal WeChat channels via iLink protocol
///
/// # Example
/// ```
/// use beebotos_agents::communication::channel::{ChannelFactory, PersonalWeChatFactory};
///
/// let factory = PersonalWeChatFactory::new();
/// assert_eq!(factory.name(), "personal_wechat");
/// ```
pub struct PersonalWeChatFactory;

impl PersonalWeChatFactory {
    /// Create a new factory instance
    pub fn new() -> Self {
        Self
    }

    /// Create default config as JSON Value
    pub fn default_config_json() -> Value {
        serde_json::json!({
            "base_url": "https://ilinkai.weixin.qq.com",
            "bot_token": null,
            "bot_base_url": null,
            "connection_mode": "polling",
            "auto_reconnect": true,
            "reconnect_interval_secs": 300,
            "warning_before_secs": 7200,
            "force_before_secs": 1800,
            "max_reconnect_attempts": 10,
        })
    }
}

impl Default for PersonalWeChatFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ChannelFactory for PersonalWeChatFactory {
    fn name(&self) -> &str {
        "personal_wechat"
    }

    fn platform_type(&self) -> super::PlatformType {
        super::PlatformType::WeChat
    }

    async fn create(&self, config: &Value) -> crate::error::Result<Arc<RwLock<dyn Channel>>> {
        debug!("Creating Personal WeChat channel from config (iLink protocol)");

        // Extract base URL for iLink API
        let base_url = config
            .get("base_url")
            .and_then(|v| v.as_str())
            .unwrap_or("https://ilinkai.weixin.qq.com")
            .to_string();

        // Extract optional bot_token (if already logged in)
        // Also support "api_key" for backward compatibility with beebotos.toml
        let bot_token = config
            .get("bot_token")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                config
                    .get("api_key")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            });

        // Extract optional bot_base_url (may differ from base_url after login)
        let bot_base_url = config
            .get("bot_base_url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Extract auto-reconnect with default
        let auto_reconnect = config
            .get("auto_reconnect")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // Extract reconnect interval with default
        let reconnect_interval_secs = config
            .get("reconnect_interval_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(300);

        // Extract warning threshold with default (2 hours)
        let warning_before_secs = config
            .get("warning_before_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(7200);

        // Extract force reconnect threshold with default (30 minutes)
        let force_before_secs = config
            .get("force_before_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(1800);

        // Parse base config
        let base_config = super::BaseChannelConfig {
            connection_mode: super::ConnectionMode::Polling, // iLink always uses polling
            auto_reconnect: config
                .get("auto_reconnect")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            max_reconnect_attempts: config
                .get("max_reconnect_attempts")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32)
                .unwrap_or(10),
            webhook_port: 0, // Not used for iLink
            webhook_url: None,
        };

        // Build config
        let channel_config = PersonalWeChatConfig {
            base_url,
            bot_token,
            bot_base_url,
            auto_reconnect,
            reconnect_interval_secs,
            warning_before_secs,
            force_before_secs,
            base: base_config,
        };

        info!(
            "Creating PersonalWeChatChannel with base_url: {}, auto_reconnect: {}",
            channel_config.base_url, channel_config.auto_reconnect
        );

        let channel = PersonalWeChatChannel::new(channel_config);
        Ok(Arc::new(RwLock::new(channel)))
    }

    fn validate_config(&self, config: &Value) -> bool {
        // Validate base_url if provided
        if let Some(base_url) = config.get("base_url").and_then(|v| v.as_str()) {
            if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
                warn!("Personal WeChat config validation failed: invalid base_url format");
                return false;
            }
        }

        // Validate bot_token format if provided (should be alphanumeric)
        if let Some(bot_token) = config.get("bot_token").and_then(|v| v.as_str()) {
            if bot_token.is_empty() {
                warn!("Personal WeChat config validation failed: empty bot_token");
                return false;
            }
        }

        // Validate threshold values make sense
        if let Some(warning_secs) = config.get("warning_before_secs").and_then(|v| v.as_u64()) {
            if let Some(force_secs) = config.get("force_before_secs").and_then(|v| v.as_u64()) {
                if warning_secs <= force_secs {
                    warn!(
                        "Personal WeChat config validation failed: warning_before_secs must be \
                         greater than force_before_secs"
                    );
                    return false;
                }
            }
        }

        debug!("Personal WeChat config validation passed");
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
        let factory = PersonalWeChatFactory::new();
        assert_eq!(factory.name(), "personal_wechat");
    }

    #[test]
    fn test_validate_config() {
        let factory = PersonalWeChatFactory::new();

        // Valid config with base_url
        let valid = serde_json::json!({
            "base_url": "https://ilinkai.weixin.qq.com",
            "auto_reconnect": true
        });
        assert!(factory.validate_config(&valid));

        // Valid config with bot_token
        let with_token = serde_json::json!({
            "base_url": "https://ilinkai.weixin.qq.com",
            "bot_token": "test_token_abc123"
        });
        assert!(factory.validate_config(&with_token));

        // Invalid base_url format
        let bad_url = serde_json::json!({
            "base_url": "not_a_url"
        });
        assert!(!factory.validate_config(&bad_url));

        // Empty bot_token
        let empty_token = serde_json::json!({
            "base_url": "https://ilinkai.weixin.qq.com",
            "bot_token": ""
        });
        assert!(!factory.validate_config(&empty_token));

        // Invalid thresholds (warning <= force)
        let bad_thresholds = serde_json::json!({
            "base_url": "https://ilinkai.weixin.qq.com",
            "warning_before_secs": 1800,
            "force_before_secs": 7200
        });
        assert!(!factory.validate_config(&bad_thresholds));
    }

    #[test]
    fn test_default_config() {
        let factory = PersonalWeChatFactory::new();
        let default = factory.default_config();

        assert_eq!(
            default.get("base_url").unwrap().as_str().unwrap(),
            "https://ilinkai.weixin.qq.com"
        );
        assert_eq!(
            default
                .get("reconnect_interval_secs")
                .unwrap()
                .as_u64()
                .unwrap(),
            300
        );
        assert!(default.get("auto_reconnect").unwrap().as_bool().unwrap());
        assert_eq!(
            default
                .get("warning_before_secs")
                .unwrap()
                .as_u64()
                .unwrap(),
            7200
        );
        assert_eq!(
            default.get("force_before_secs").unwrap().as_u64().unwrap(),
            1800
        );
    }
}
