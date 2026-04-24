//! Multi-instance channel manager.
//!
//! Replaces the single-instance `ChannelRegistry` with per-user channel
//! instance tracking.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};
use tracing::{error, info, warn};

use crate::communication::channel::{Channel, ChannelEvent, ChannelFactory};
use crate::communication::user_channel::{ChannelInstanceId, ChannelInstanceRef};
use crate::communication::PlatformType;
use crate::deduplicator::MessageDeduplicator;
use crate::error::{AgentError, Result};

/// Runtime state of a single channel instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelInstanceStatus {
    Connected,
    Disconnected,
    Error,
}

struct ChannelInstance {
    id: ChannelInstanceId,
    channel: Arc<RwLock<dyn Channel>>,
    status: ChannelInstanceStatus,
    user_channel_id: String,
}

/// Manages channel factories and per-user channel instances.
pub struct ChannelInstanceManager {
    factories: RwLock<HashMap<String, Box<dyn ChannelFactory>>>,
    instances: Arc<RwLock<HashMap<ChannelInstanceId, ChannelInstance>>>,
    platform_index: Arc<RwLock<HashMap<(String, PlatformType), Vec<ChannelInstanceId>>>>,
    event_bus: mpsc::Sender<ChannelEvent>,
    #[allow(dead_code)]
    deduplicator: Arc<MessageDeduplicator>,
    auto_reconnect: bool,
}

impl ChannelInstanceManager {
    pub fn new(event_bus: mpsc::Sender<ChannelEvent>) -> Self {
        Self {
            factories: RwLock::new(HashMap::new()),
            instances: Arc::new(RwLock::new(HashMap::new())),
            platform_index: Arc::new(RwLock::new(HashMap::new())),
            event_bus,
            deduplicator: Arc::new(MessageDeduplicator::default()),
            auto_reconnect: true,
        }
    }

    /// Enable or disable automatic reconnection of error instances.
    pub fn set_auto_reconnect(&mut self, enabled: bool) {
        self.auto_reconnect = enabled;
    }

    /// Register a global channel factory.
    pub async fn register_factory(&self, factory: Box<dyn ChannelFactory>) {
        let name = factory.name().to_string();
        info!("📦 Registering channel factory: {}", name);
        self.factories.write().await.insert(name, factory);
    }

    /// Create a new channel instance for a specific user binding.
    ///
    /// If an instance with the same `ChannelInstanceId` already exists,
    /// it will be removed and replaced.
    pub async fn create_instance(
        &self,
        id: ChannelInstanceId,
        user_channel_id: String,
        config: &serde_json::Value,
        factory_name: Option<&str>,
    ) -> Result<Arc<RwLock<dyn Channel>>> {
        // Clean up any existing instance with the same ID to prevent resource leaks.
        if self.get_instance(&id).await.is_some() {
            warn!(
                "Channel instance {:?} already exists, removing old instance first",
                id
            );
            self.remove_instance(&id).await?;
        }

        let platform_str = format!("{}", id.platform);

        let factories = self.factories.read().await;
        // Try explicit factory name first (e.g. "personal_wechat" vs "wechat"),
        // then fall back to platform string.
        let factory = factory_name
            .and_then(|name| factories.get(name))
            .or_else(|| factories.get(&platform_str))
            .ok_or_else(|| {
                AgentError::configuration(format!(
                    "Unknown channel type / platform: {} (hint: {:?})",
                    platform_str, factory_name
                ))
            })?;

        let channel_arc = factory.create(config).await?;
        drop(factories);

        let instance = ChannelInstance {
            id: id.clone(),
            channel: channel_arc.clone(),
            status: ChannelInstanceStatus::Disconnected,
            user_channel_id,
        };

        let mut instances = self.instances.write().await;
        instances.insert(id.clone(), instance);
        drop(instances);

        let mut index = self.platform_index.write().await;
        index
            .entry((id.user_id.clone(), id.platform))
            .or_default()
            .push(id.clone());

        info!("✅ Created channel instance: {:?}", id);
        Ok(channel_arc)
    }

    /// Get a channel instance by its ID.
    pub async fn get_instance(&self, id: &ChannelInstanceId) -> Option<Arc<RwLock<dyn Channel>>> {
        let instances = self.instances.read().await;
        instances.get(id).map(|i| i.channel.clone())
    }

    /// Get instances by user + platform.
    pub async fn get_instances_by_user_platform(
        &self,
        user_id: &str,
        platform: PlatformType,
    ) -> Vec<ChannelInstanceRef> {
        let index = self.platform_index.read().await;
        let ids = index.get(&(user_id.to_string(), platform));
        let instances = self.instances.read().await;
        ids.map(|vec| {
            vec.iter()
                .filter_map(|id| instances.get(id))
                .map(|i| ChannelInstanceRef {
                    id: i.id.clone(),
                    user_channel_id: i.user_channel_id.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
    }

    /// Get all instances for a user.
    pub async fn get_instances_by_user(&self, user_id: &str) -> Vec<ChannelInstanceRef> {
        let instances = self.instances.read().await;
        instances
            .values()
            .filter(|i| i.id.user_id == user_id)
            .map(|i| ChannelInstanceRef {
                id: i.id.clone(),
                user_channel_id: i.user_channel_id.clone(),
            })
            .collect()
    }

    /// Get all instances for a platform.
    pub async fn get_instances_by_platform(
        &self,
        platform: PlatformType,
    ) -> Vec<ChannelInstanceRef> {
        let instances = self.instances.read().await;
        instances
            .values()
            .filter(|i| i.id.platform == platform)
            .map(|i| ChannelInstanceRef {
                id: i.id.clone(),
                user_channel_id: i.user_channel_id.clone(),
            })
            .collect()
    }

    /// Remove an instance and clean up resources.
    pub async fn remove_instance(&self, id: &ChannelInstanceId) -> Result<()> {
        let mut instances = self.instances.write().await;
        if let Some(instance) = instances.remove(id) {
            if let Err(e) = instance.channel.read().await.stop_listener().await {
                warn!("Error stopping listener for {:?}: {}", id, e);
            }
            drop(instances);

            let mut index = self.platform_index.write().await;
            if let Some(vec) = index.get_mut(&(id.user_id.clone(), id.platform)) {
                vec.retain(|x| x != id);
                if vec.is_empty() {
                    index.remove(&(id.user_id.clone(), id.platform));
                }
            }

            info!("🗑️ Removed channel instance: {:?}", id);
        }
        Ok(())
    }

    /// Update instance status.
    pub async fn set_status(&self, id: &ChannelInstanceId, status: ChannelInstanceStatus) {
        let mut instances = self.instances.write().await;
        if let Some(inst) = instances.get_mut(id) {
            inst.status = status;
        }
    }

    /// Get instance status.
    pub async fn get_status(&self, id: &ChannelInstanceId) -> Option<ChannelInstanceStatus> {
        let instances = self.instances.read().await;
        instances.get(id).map(|i| i.status)
    }

    /// Connect and optionally start listener for an instance.
    pub async fn connect_instance(&self, id: &ChannelInstanceId) -> Result<()> {
        let channel = self
            .get_instance(id)
            .await
            .ok_or_else(|| AgentError::configuration(format!("Instance not found: {:?}", id)))?;

        {
            let mut ch = channel.write().await;
            ch.connect().await?;
        }

        self.set_status(id, ChannelInstanceStatus::Connected).await;

        let event_tx = self.event_bus.clone();
        let id_clone = id.clone();
        let channel_clone = channel.clone();
        let instances = Arc::clone(&self.instances);

        tokio::spawn(async move {
            if let Err(e) = channel_clone.read().await.start_listener(event_tx).await {
                error!("Instance {:?} listener error: {}", id_clone, e);
                let mut inst = instances.write().await;
                if let Some(i) = inst.get_mut(&id_clone) {
                    i.status = ChannelInstanceStatus::Error;
                }
            }
        });

        info!("🔌 Connected channel instance: {:?}", id);
        Ok(())
    }

    /// Reconnect all instances that are currently in `Error` status.
    pub async fn reconnect_all_error_instances(&self) -> Vec<(ChannelInstanceId, Result<()>)> {
        let error_ids: Vec<ChannelInstanceId> = {
            let instances = self.instances.read().await;
            instances
                .values()
                .filter(|i| i.status == ChannelInstanceStatus::Error)
                .map(|i| i.id.clone())
                .collect()
        };

        let mut results = Vec::new();
        for id in error_ids {
            let result = self.connect_instance(&id).await;
            results.push((id, result));
        }
        results
    }

    /// Start a background health-reconnection task.
    ///
    /// Every `interval` it scans for instances in `Error` state and tries to
    /// reconnect them (only if `auto_reconnect` is enabled).
    pub fn start_health_monitor(self: &Arc<Self>, interval: std::time::Duration) {
        let manager = Arc::clone(self);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                if manager.auto_reconnect {
                    let results = manager.reconnect_all_error_instances().await;
                    for (id, result) in results {
                        match result {
                            Ok(()) => info!("Auto-reconnected channel instance {:?}", id),
                            Err(e) => warn!("Auto-reconnect failed for {:?}: {}", id, e),
                        }
                    }
                }
            }
        });
    }

    /// Disconnect an instance.
    pub async fn disconnect_instance(&self, id: &ChannelInstanceId) -> Result<()> {
        let channel = self
            .get_instance(id)
            .await
            .ok_or_else(|| AgentError::configuration(format!("Instance not found: {:?}", id)))?;

        {
            let mut ch = channel.write().await;
            ch.disconnect().await?;
            ch.stop_listener().await.ok();
        }

        self.set_status(id, ChannelInstanceStatus::Disconnected)
            .await;
        info!("🔌 Disconnected channel instance: {:?}", id);
        Ok(())
    }

    /// Send a message through a specific instance.
    pub async fn send_message(
        &self,
        id: &ChannelInstanceId,
        target_channel_id: &str,
        message: &crate::communication::Message,
    ) -> Result<()> {
        let channel = self
            .get_instance(id)
            .await
            .ok_or_else(|| AgentError::configuration(format!("Instance not found: {:?}", id)))?;

        let result = channel.read().await.send(target_channel_id, message).await;
        result
    }

    /// Returns true if the given platform has a registered factory.
    pub async fn has_factory(&self, platform: PlatformType) -> bool {
        let factories = self.factories.read().await;
        factories.contains_key(&format!("{}", platform))
    }

    /// Returns the number of registered factories.
    pub async fn factory_count(&self) -> usize {
        self.factories.read().await.len()
    }

    /// Returns the number of active instances.
    pub async fn instance_count(&self) -> usize {
        self.instances.read().await.len()
    }
}

impl Default for ChannelInstanceManager {
    fn default() -> Self {
        let (tx, _rx) = mpsc::channel(1000);
        Self::new(tx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::communication::PlatformType;

    #[tokio::test]
    async fn test_create_and_get_instance() {
        let (tx, _rx) = mpsc::channel(100);
        let manager = ChannelInstanceManager::new(tx);

        let id = ChannelInstanceId::new("user-1", PlatformType::Lark, "default");
        let config = serde_json::json!({"app_id": "test", "app_secret": "test"});

        // Without a factory, creation should fail
        let result = manager
            .create_instance(id.clone(), "uc-1".to_string(), &config, None)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_instance_lookup_by_user() {
        let manager = ChannelInstanceManager::default();
        let instances = manager.get_instances_by_user("user-1").await;
        assert!(instances.is_empty());
    }
}
