//! Agent-Channel binding service.
//!
//! Manages the N-to-1 relationship between user channels and agents,
//! including default agent semantics and routing rule validation.

use std::sync::Arc;

use tracing::info;

use super::agent_channel_store::AgentChannelBindingStore;
use crate::communication::agent_channel::{AgentChannelBinding, RoutingRules};
use crate::error::Result;

/// Service for managing agent-channel bindings.
pub struct AgentChannelService {
    store: Arc<dyn AgentChannelBindingStore>,
}

impl AgentChannelService {
    pub fn new(store: Arc<dyn AgentChannelBindingStore>) -> Self {
        Self { store }
    }

    /// Bind an agent to a user channel.
    pub async fn bind_agent(
        &self,
        agent_id: impl Into<String>,
        user_channel_id: impl Into<String>,
        binding_name: Option<String>,
        priority: i32,
        routing_rules: RoutingRules,
        set_as_default: bool,
    ) -> Result<AgentChannelBinding> {
        let agent_id = agent_id.into();
        let user_channel_id = user_channel_id.into();

        let binding = AgentChannelBinding {
            id: uuid::Uuid::new_v4().to_string(),
            agent_id: agent_id.clone(),
            user_channel_id: user_channel_id.clone(),
            binding_name: binding_name.clone(),
            is_default: false,
            priority,
            routing_rules,
        };

        self.store.bind(&binding).await?;

        if set_as_default {
            self.store.set_default(&user_channel_id, &agent_id).await?;
        }

        info!(
            "Bound agent {} to user channel {} (default={})",
            agent_id, user_channel_id, set_as_default
        );
        Ok(binding)
    }

    /// Unbind an agent from a user channel.
    pub async fn unbind_agent(&self, agent_id: &str, user_channel_id: &str) -> Result<()> {
        self.store.unbind(agent_id, user_channel_id).await?;
        info!(
            "Unbound agent {} from user channel {}",
            agent_id, user_channel_id
        );
        Ok(())
    }

    /// Set an agent as the default for a user channel.
    ///
    /// Enforces the invariant that at most one agent is default per user
    /// channel.
    pub async fn set_default_agent(&self, user_channel_id: &str, agent_id: &str) -> Result<()> {
        self.store.set_default(user_channel_id, agent_id).await?;
        info!(
            "Set agent {} as default for user channel {}",
            agent_id, user_channel_id
        );
        Ok(())
    }

    /// List all agents bound to a user channel.
    pub async fn list_agents_for_channel(
        &self,
        user_channel_id: &str,
    ) -> Result<Vec<AgentChannelBinding>> {
        self.store.list_by_user_channel(user_channel_id).await
    }

    /// List all channels bound to an agent.
    pub async fn list_channels_for_agent(
        &self,
        agent_id: &str,
    ) -> Result<Vec<AgentChannelBinding>> {
        self.store.list_by_agent(agent_id).await
    }

    /// Find the default agent for a user channel identified by platform +
    /// platform_channel_id.
    ///
    /// P2 OPTIMIZE: Renamed from `find_default_agent_for_platform_user` to
    /// clarify that the lookup key is the platform-level channel identifier
    /// (e.g. chat_id, room_id), not the individual sender/user ID.
    pub async fn find_default_agent_for_platform_channel(
        &self,
        platform: crate::communication::PlatformType,
        platform_channel_id: &str,
    ) -> Result<Option<String>> {
        self.store
            .find_default_agent_by_platform_channel(platform, platform_channel_id)
            .await
    }
}
