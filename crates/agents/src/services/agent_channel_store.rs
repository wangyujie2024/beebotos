use async_trait::async_trait;

use crate::communication::agent_channel::AgentChannelBinding;
use crate::communication::PlatformType;
use crate::error::Result;

#[async_trait]
pub trait AgentChannelBindingStore: Send + Sync {
    async fn bind(&self, binding: &AgentChannelBinding) -> Result<()>;

    async fn unbind(&self, agent_id: &str, user_channel_id: &str) -> Result<()>;

    async fn list_by_agent(&self, agent_id: &str) -> Result<Vec<AgentChannelBinding>>;

    async fn list_by_user_channel(&self, user_channel_id: &str)
        -> Result<Vec<AgentChannelBinding>>;

    async fn set_default(&self, user_channel_id: &str, agent_id: &str) -> Result<()>;

    /// Find the default agent bound to a user channel identified by platform +
    /// platform_channel_id. Returns the agent_id of the default binding, if
    /// any.
    ///
    /// P2 OPTIMIZE: Renamed from `find_default_agent_by_platform_user` to
    /// clarify that the lookup key is the platform-level channel identifier
    /// (e.g. chat_id, room_id), not the individual sender/user ID.
    async fn find_default_agent_by_platform_channel(
        &self,
        platform: PlatformType,
        platform_channel_id: &str,
    ) -> Result<Option<String>>;
}
