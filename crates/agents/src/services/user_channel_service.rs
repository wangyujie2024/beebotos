//! User channel lifecycle service.
//!
//! Orchestrates encrypted storage, webhook path generation, and runtime
//! instance creation for user channel bindings.

use std::collections::HashMap;
use std::sync::Arc;

use tracing::{info, warn};

use super::credential_crypto::ChannelConfigEncryptor;
use super::user_channel_store::UserChannelStore;
use crate::communication::channel_instance_manager::ChannelInstanceManager;
use crate::communication::user_channel::{
    ChannelBindingStatus, ChannelInstanceId, UserChannelBinding, UserChannelConfig,
};
use crate::communication::PlatformType;
use crate::error::{AgentError, Result};

/// Service for managing user channel lifecycle.
pub struct UserChannelService {
    store: Arc<dyn UserChannelStore>,
    instance_manager: Arc<ChannelInstanceManager>,
    encryptor: ChannelConfigEncryptor,
}

impl UserChannelService {
    pub fn new(
        store: Arc<dyn UserChannelStore>,
        instance_manager: Arc<ChannelInstanceManager>,
        encryptor: ChannelConfigEncryptor,
    ) -> Self {
        Self {
            store,
            instance_manager,
            encryptor,
        }
    }

    /// Create a new user channel binding and its runtime instance.
    ///
    /// The `config` is encrypted in-flight using the configured `encryptor`
    /// before being written to the database.
    pub async fn create_user_channel(
        &self,
        user_id: impl Into<String>,
        platform: PlatformType,
        instance_name: impl Into<String>,
        platform_user_id: Option<String>,
        config: &UserChannelConfig,
    ) -> Result<UserChannelBinding> {
        let user_id = user_id.into();
        let instance_name = instance_name.into();

        if !self.instance_manager.has_factory(platform).await {
            return Err(AgentError::configuration(format!(
                "No factory registered for platform {:?}",
                platform
            )));
        }

        let webhook_path = Some(format!(
            "/webhook/{}/inst-{}",
            platform,
            uuid::Uuid::new_v4()
        ));

        let binding = UserChannelBinding {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user_id.clone(),
            platform,
            instance_name: instance_name.clone(),
            platform_user_id: platform_user_id.clone(),
            status: ChannelBindingStatus::Active,
            webhook_path: webhook_path.clone(),
        };

        let config_encrypted = (self.encryptor)(config)?;
        self.store.create(&binding, &config_encrypted).await?;

        let instance_id = ChannelInstanceId::new(&user_id, platform, &instance_name);
        let config_json =
            serde_json::to_value(config).map_err(|e| AgentError::serialization(e.to_string()))?;

        if let Err(e) = self
            .instance_manager
            .create_instance(instance_id, binding.id.clone(), &config_json, None)
            .await
        {
            warn!(
                "Failed to create runtime instance for user channel {}: {}",
                binding.id, e
            );
            self.store
                .update_status(&binding.id, ChannelBindingStatus::Error)
                .await?;
            return Err(e);
        }

        info!("Created user channel {} for user {}", binding.id, user_id);
        Ok(binding)
    }

    /// Create a user channel binding record only, without spawning a runtime
    /// instance. P1 FIX: Used for back-filling bindings when auto-creating
    /// user_channel.
    pub async fn create_binding_only(&self, binding: &UserChannelBinding) -> Result<()> {
        let config = UserChannelConfig {
            platform: binding.platform,
            connection_mode: crate::communication::channel::ConnectionMode::Webhook,
            credentials: crate::communication::user_channel::PlatformCredentials::Generic {
                fields: HashMap::new(),
            },
        };
        let config_encrypted = (self.encryptor)(&config)?;
        self.store.create(binding, &config_encrypted).await
    }

    /// Delete a user channel binding and clean up its runtime instance.
    pub async fn delete_user_channel(&self, user_channel_id: &str) -> Result<()> {
        let binding = self.store.get(user_channel_id).await?.ok_or_else(|| {
            AgentError::not_found(format!("User channel not found: {}", user_channel_id))
        })?;

        let instance_id =
            ChannelInstanceId::new(&binding.user_id, binding.platform, &binding.instance_name);

        if let Err(e) = self.instance_manager.remove_instance(&instance_id).await {
            warn!(
                "Error removing runtime instance for user channel {}: {}",
                user_channel_id, e
            );
        }

        self.store.delete(user_channel_id).await?;
        info!("Deleted user channel {}", user_channel_id);
        Ok(())
    }

    /// Connect a specific user channel instance.
    pub async fn connect_user_channel(&self, user_channel_id: &str) -> Result<()> {
        let binding = self.store.get(user_channel_id).await?.ok_or_else(|| {
            AgentError::not_found(format!("User channel not found: {}", user_channel_id))
        })?;

        let instance_id =
            ChannelInstanceId::new(&binding.user_id, binding.platform, &binding.instance_name);

        self.instance_manager.connect_instance(&instance_id).await?;
        self.store
            .update_status(user_channel_id, ChannelBindingStatus::Active)
            .await?;
        Ok(())
    }

    /// Disconnect a specific user channel instance.
    pub async fn disconnect_user_channel(&self, user_channel_id: &str) -> Result<()> {
        let binding = self.store.get(user_channel_id).await?.ok_or_else(|| {
            AgentError::not_found(format!("User channel not found: {}", user_channel_id))
        })?;

        let instance_id =
            ChannelInstanceId::new(&binding.user_id, binding.platform, &binding.instance_name);

        self.instance_manager
            .disconnect_instance(&instance_id)
            .await?;
        self.store
            .update_status(user_channel_id, ChannelBindingStatus::Paused)
            .await?;
        Ok(())
    }

    /// Get a single user channel binding by ID.
    pub async fn get(&self, user_channel_id: &str) -> Result<Option<UserChannelBinding>> {
        self.store.get(user_channel_id).await
    }

    /// List all channels for a user.
    pub async fn list_by_user(&self, user_id: &str) -> Result<Vec<UserChannelBinding>> {
        self.store.list_by_user(user_id).await
    }

    /// Find a user channel by platform + platform_user_id.
    pub async fn find_by_platform_user(
        &self,
        platform: PlatformType,
        platform_user_id: &str,
    ) -> Result<Option<UserChannelBinding>> {
        self.store
            .find_by_platform_user(platform, platform_user_id)
            .await
    }
}
