use async_trait::async_trait;

use crate::communication::user_channel::{ChannelBindingStatus, UserChannelBinding};
use crate::communication::PlatformType;
use crate::error::Result;

#[async_trait]
pub trait UserChannelStore: Send + Sync {
    async fn create(&self, binding: &UserChannelBinding, config_encrypted: &str) -> Result<()>;

    async fn get(&self, id: &str) -> Result<Option<UserChannelBinding>>;

    async fn find_by_platform_user(
        &self,
        platform: PlatformType,
        platform_user_id: &str,
    ) -> Result<Option<UserChannelBinding>>;

    async fn list_by_user(&self, user_id: &str) -> Result<Vec<UserChannelBinding>>;

    async fn update_status(&self, id: &str, status: ChannelBindingStatus) -> Result<()>;

    async fn delete(&self, id: &str) -> Result<()>;
}
