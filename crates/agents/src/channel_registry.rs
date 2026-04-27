//! Channel Registry
//!
//! Backward-compatible wrapper around `ChannelInstanceManager`.
//! Maintains the legacy per-platform registry API while internally
//! delegating to the multi-instance aware `ChannelInstanceManager`.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{mpsc, RwLock};
use tracing::info;

use crate::communication::channel::{Channel, ChannelEvent, ChannelFactory};
use crate::communication::channel_instance_manager::ChannelInstanceManager;
use crate::communication::user_channel::ChannelInstanceId;
use crate::communication::PlatformType;
use crate::deduplicator::MessageDeduplicator;
use crate::error::{AgentError, Result};

/// Channel registration information
#[derive(Debug, Clone)]
pub struct ChannelInfo {
    /// Channel type name
    pub channel_type: String,
    /// Platform type
    pub platform: PlatformType,
    /// Whether the channel is enabled
    pub enabled: bool,
    /// Connection mode
    pub connection_mode: String,
    /// Whether the channel is currently connected
    pub is_connected: bool,
}

/// Backward-compatible identifier used for legacy single-instance APIs.
const LEGACY_USER_ID: &str = "__legacy__";
const LEGACY_INSTANCE_NAME: &str = "default";

/// Channel Registry
///
/// Manages channel factories and instances. Internally delegates to
/// `ChannelInstanceManager` for multi-instance support.
pub struct ChannelRegistry {
    /// Internal multi-instance manager
    instance_manager: Arc<ChannelInstanceManager>,
    /// Legacy mapping from channel_type -> instance_id
    legacy_map: RwLock<HashMap<String, ChannelInstanceId>>,
    /// Event bus sender
    #[allow(dead_code)]
    event_bus: mpsc::Sender<ChannelEvent>,
    /// Message deduplicator
    #[allow(dead_code)]
    deduplicator: Arc<MessageDeduplicator>,
}

impl ChannelRegistry {
    /// Create a new channel registry
    pub fn new(event_bus: mpsc::Sender<ChannelEvent>) -> Self {
        let instance_manager = Arc::new(ChannelInstanceManager::new(event_bus.clone()));
        Self {
            instance_manager,
            legacy_map: RwLock::new(HashMap::new()),
            event_bus,
            deduplicator: Arc::new(MessageDeduplicator::default()),
        }
    }

    /// Register a channel factory
    pub async fn register(&self, factory: Box<dyn ChannelFactory>) {
        self.instance_manager.register_factory(factory).await;
    }

    /// Create a channel from configuration (legacy API).
    pub async fn create_channel(
        &self,
        channel_type: &str,
        config: &Value,
    ) -> Result<Arc<RwLock<dyn Channel>>> {
        let platform = platform_from_channel_type(channel_type);
        let instance_id = ChannelInstanceId::new(LEGACY_USER_ID, platform, LEGACY_INSTANCE_NAME);
        let user_channel_id = format!("legacy-{}", channel_type);

        let channel = self
            .instance_manager
            .create_instance(
                instance_id.clone(),
                user_channel_id,
                config,
                Some(channel_type),
            )
            .await?;

        self.legacy_map
            .write()
            .await
            .insert(channel_type.to_string(), instance_id);

        info!("✅ Created channel: {}", channel_type);
        Ok(channel)
    }

    /// Get channel by type
    pub async fn get_channel(&self, channel_type: &str) -> Option<Arc<RwLock<dyn Channel>>> {
        let instance_id = self.legacy_map.read().await.get(channel_type)?.clone();
        self.instance_manager.get_instance(&instance_id).await
    }

    /// Get channel by platform type
    pub async fn get_channel_by_platform(
        &self,
        platform: PlatformType,
    ) -> Option<Arc<RwLock<dyn Channel>>> {
        let instance_id = ChannelInstanceId::new(LEGACY_USER_ID, platform, LEGACY_INSTANCE_NAME);
        self.instance_manager.get_instance(&instance_id).await
    }

    /// Get all registered channel information
    pub async fn list_channels(&self) -> Vec<ChannelInfo> {
        let legacy = self.legacy_map.read().await;
        let mut result = Vec::new();
        for (channel_type, instance_id) in legacy.iter() {
            let is_connected = self
                .instance_manager
                .get_status(instance_id)
                .await
                .map(|s| matches!(s, crate::communication::channel_instance_manager::ChannelInstanceStatus::Connected))
                .unwrap_or(false);

            result.push(ChannelInfo {
                channel_type: channel_type.clone(),
                platform: instance_id.platform,
                enabled: true,
                connection_mode: "websocket".to_string(),
                is_connected,
            });
        }
        result
    }

    /// Check if channel type is registered (factory exists)
    pub async fn is_registered(&self, channel_type: &str) -> bool {
        self.instance_manager
            .has_factory(platform_from_channel_type(channel_type))
            .await
    }

    /// Get the number of registered factories
    pub async fn factory_count(&self) -> usize {
        self.instance_manager.factory_count().await
    }

    /// Get the number of active channels
    pub async fn channel_count(&self) -> usize {
        self.legacy_map.read().await.len()
    }

    /// Remove a channel
    pub async fn remove_channel(&self, channel_type: &str) -> Result<()> {
        let instance_id = self
            .legacy_map
            .write()
            .await
            .remove(channel_type)
            .ok_or_else(|| AgentError::not_found(format!("Channel not found: {}", channel_type)))?;

        self.instance_manager.remove_instance(&instance_id).await?;
        info!("🗑️  Removed channel: {}", channel_type);
        Ok(())
    }

    /// Get channel by message ID prefix
    pub async fn get_channel_by_msg_id(&self, msg_id: &str) -> Option<Arc<RwLock<dyn Channel>>> {
        let prefixes: Vec<&str> = if msg_id.contains(':') {
            msg_id.split(':').next().into_iter().collect()
        } else if msg_id.contains('_') {
            msg_id.split('_').next().into_iter().collect()
        } else {
            vec![]
        };

        for prefix in prefixes {
            if let Some(channel) = self.get_channel(prefix).await {
                return Some(channel);
            }
        }
        None
    }

    /// Get the underlying `ChannelInstanceManager`.
    pub fn instance_manager(&self) -> Arc<ChannelInstanceManager> {
        self.instance_manager.clone()
    }
}

impl Default for ChannelRegistry {
    fn default() -> Self {
        let (tx, _rx) = mpsc::channel(1000);
        Self::new(tx)
    }
}

fn platform_from_channel_type(channel_type: &str) -> PlatformType {
    match channel_type.to_lowercase().as_str() {
        "slack" => PlatformType::Slack,
        "telegram" => PlatformType::Telegram,
        "discord" => PlatformType::Discord,
        "whatsapp" => PlatformType::WhatsApp,
        "signal" => PlatformType::Signal,
        "imessage" => PlatformType::IMessage,
        "wechat" => PlatformType::WeChat,
        "personal_wechat" => PlatformType::WeChat,
        "teams" => PlatformType::Teams,
        "twitter" => PlatformType::Twitter,
        "lark" => PlatformType::Lark,
        "dingtalk" => PlatformType::DingTalk,
        "matrix" => PlatformType::Matrix,
        "googlechat" => PlatformType::GoogleChat,
        "line" => PlatformType::Line,
        "qq" => PlatformType::QQ,
        "irc" => PlatformType::IRC,
        "webchat" => PlatformType::WebChat,
        _ => PlatformType::Custom,
    }
}

/// Channel registry builder
pub struct ChannelRegistryBuilder {
    factories: Vec<Box<dyn ChannelFactory>>,
    event_bus: Option<mpsc::Sender<ChannelEvent>>,
}

impl ChannelRegistryBuilder {
    pub fn new() -> Self {
        Self {
            factories: Vec::new(),
            event_bus: None,
        }
    }

    pub fn with_channel(mut self, factory: Box<dyn ChannelFactory>) -> Self {
        self.factories.push(factory);
        self
    }

    pub fn with_event_bus(mut self, event_bus: mpsc::Sender<ChannelEvent>) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    pub async fn build(self) -> ChannelRegistry {
        let event_bus = self.event_bus.unwrap_or_else(|| {
            let (tx, _rx) = mpsc::channel(1000);
            tx
        });

        let registry = ChannelRegistry::new(event_bus);

        for factory in self.factories {
            registry.register(factory).await;
        }

        registry
    }
}

impl Default for ChannelRegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}
