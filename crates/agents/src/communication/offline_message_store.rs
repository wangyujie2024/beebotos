//! Persistent offline message store for agents.
//!
//! When an agent is offline, inbound messages are buffered here and
//! flushed once the agent reconnects.

use async_trait::async_trait;

use super::message_router_v2::UserMessageContext;
use crate::error::Result;

#[async_trait]
pub trait OfflineMessageStore: Send + Sync {
    /// Store a message for later delivery.
    async fn enqueue(&self, agent_id: &str, ctx: &UserMessageContext) -> Result<()>;

    /// Retrieve and remove all buffered messages for an agent.
    async fn dequeue_all(&self, agent_id: &str) -> Result<Vec<UserMessageContext>>;

    /// Delete all buffered messages for an agent.
    async fn clear(&self, agent_id: &str) -> Result<()>;
}
