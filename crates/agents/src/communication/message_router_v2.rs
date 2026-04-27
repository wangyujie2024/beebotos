//! Unified inbound and outbound message router for multi-user multi-agent
//! channels.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};

use crate::communication::agent_channel::{AgentChannelBinding, RoutingRules};
use crate::communication::offline_message_store::OfflineMessageStore;
use crate::communication::user_channel::{ChannelInstanceId, UserChannelBinding};
use crate::communication::{Message, PlatformType};
use crate::error::{AgentError, Result};
use crate::services::agent_channel_store::AgentChannelBindingStore;
use crate::services::user_channel_store::UserChannelStore;

/// Context passed to an Agent together with an inbound message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessageContext {
    pub message: Message,
    pub reply_route: ReplyRoute,
    pub user_channel_binding_id: String,
    pub agent_channel_binding_id: String,
    pub user_id: String,
    pub platform: PlatformType,
}

/// Route used by an Agent when it wants to reply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyRoute {
    pub channel_instance_id: ChannelInstanceId,
    pub target_channel_id: String,
}

/// Decision made by the inbound router.
#[derive(Debug, Clone)]
pub enum RoutingDecision {
    SingleAgent(String),
    Broadcast(Vec<String>),
    Drop,
}

/// Inbound router: maps platform messages to the right agent(s).
pub struct InboundMessageRouter {
    user_channel_store: Arc<dyn UserChannelStore>,
    agent_binding_store: Arc<dyn AgentChannelBindingStore>,
}

impl InboundMessageRouter {
    pub fn new(
        user_channel_store: Arc<dyn UserChannelStore>,
        agent_binding_store: Arc<dyn AgentChannelBindingStore>,
    ) -> Self {
        Self {
            user_channel_store,
            agent_binding_store,
        }
    }

    pub async fn route_inbound(
        &self,
        platform: PlatformType,
        platform_user_id: &str,
        message: &Message,
    ) -> Result<(UserChannelBinding, RoutingDecision)> {
        let user_channel = self
            .user_channel_store
            .find_by_platform_user(platform, platform_user_id)
            .await?
            .ok_or_else(|| {
                AgentError::not_found(format!(
                    "user channel not found for platform {:?} user {}",
                    platform, platform_user_id
                ))
            })?;

        let bindings = self
            .agent_binding_store
            .list_by_user_channel(&user_channel.id)
            .await?;

        if bindings.is_empty() {
            return Ok((user_channel, RoutingDecision::Drop));
        }

        let mut targets: Vec<AgentChannelBinding> = bindings
            .clone()
            .into_iter()
            .filter(|b| matches_rules(&b.routing_rules, message))
            .collect();

        targets.sort_by(|a, b| b.priority.cmp(&a.priority));

        if targets.is_empty() {
            // fallback to default agent from the already fetched bindings
            let default = bindings.into_iter().find(|b| b.is_default);

            if let Some(d) = default {
                return Ok((user_channel, RoutingDecision::SingleAgent(d.agent_id)));
            }
            return Ok((user_channel, RoutingDecision::Drop));
        }

        let agent_ids: Vec<String> = targets.into_iter().map(|b| b.agent_id).collect();

        if agent_ids.len() == 1 {
            Ok((
                user_channel,
                RoutingDecision::SingleAgent(agent_ids.into_iter().next().unwrap()),
            ))
        } else {
            Ok((user_channel, RoutingDecision::Broadcast(agent_ids)))
        }
    }
}

fn matches_rules(rules: &RoutingRules, message: &Message) -> bool {
    let metadata = &message.metadata;

    if rules.require_mention {
        let mentioned = metadata
            .get("mentioned")
            .map(|v| v == "true")
            .unwrap_or(false);
        if !mentioned {
            return false;
        }
    }

    if !rules.allowed_group_ids.is_empty() {
        let group_id = metadata.get("group_id").map(|s| s.as_str()).unwrap_or("");
        if !rules.allowed_group_ids.iter().any(|g| g == group_id) {
            return false;
        }
    }

    if !rules.allowed_user_ids.is_empty() {
        let sender_id = metadata.get("sender_id").map(|s| s.as_str()).unwrap_or("");
        if !rules.allowed_user_ids.iter().any(|u| u == sender_id) {
            return false;
        }
    }

    if !rules.keyword_filters.is_empty() {
        let content_lower = message.content.to_lowercase();
        if !rules
            .keyword_filters
            .iter()
            .any(|kw| content_lower.contains(&kw.to_lowercase()))
        {
            return false;
        }
    }

    true
}

/// Outbound router: delivers agent replies back to the correct channel
/// instance.
pub struct OutboundMessageRouter {
    instance_manager: Arc<crate::communication::channel_instance_manager::ChannelInstanceManager>,
}

impl OutboundMessageRouter {
    pub fn new(
        instance_manager: Arc<
            crate::communication::channel_instance_manager::ChannelInstanceManager,
        >,
    ) -> Self {
        Self { instance_manager }
    }

    pub async fn send_reply(&self, reply_route: &ReplyRoute, message: &Message) -> Result<()> {
        self.instance_manager
            .send_message(
                &reply_route.channel_instance_id,
                &reply_route.target_channel_id,
                message,
            )
            .await
    }
}

/// Central dispatcher used by the Gateway layer.
pub struct AgentMessageDispatcher {
    inbound_router: Arc<InboundMessageRouter>,
    agent_queues: Arc<RwLock<HashMap<String, mpsc::Sender<UserMessageContext>>>>,
    /// Persistent offline message store.
    offline_store: Arc<dyn OfflineMessageStore>,
}

impl AgentMessageDispatcher {
    pub fn new(
        inbound_router: Arc<InboundMessageRouter>,
        offline_store: Arc<dyn OfflineMessageStore>,
    ) -> Self {
        Self {
            inbound_router,
            agent_queues: Arc::new(RwLock::new(HashMap::new())),
            offline_store,
        }
    }

    /// Register an agent message queue.
    pub async fn register_agent(&self, agent_id: &str, tx: mpsc::Sender<UserMessageContext>) {
        self.agent_queues
            .write()
            .await
            .insert(agent_id.to_string(), tx.clone());
        self.flush_pending(agent_id, &tx).await;
    }

    /// Unregister an agent queue.
    pub async fn unregister_agent(&self, agent_id: &str) {
        self.agent_queues.write().await.remove(agent_id);
        let _ = self.offline_store.clear(agent_id).await;
    }

    /// Main entry point for webhooks / websocket events.
    pub async fn dispatch(
        &self,
        platform: PlatformType,
        platform_user_id: &str,
        message: Message,
        target_channel_id: String,
    ) -> Result<()> {
        let (user_channel, decision) = match self
            .inbound_router
            .route_inbound(platform, platform_user_id, &message)
            .await
        {
            Ok(result) => result,
            Err(AgentError::AgentNotFound(_)) => {
                // P0 FIX: No user channel found — don't fail, let other paths
                // (e.g. channel_event_bus → AgentResolver) handle the message.
                tracing::warn!(
                    "No user channel found for {}:{}, skipping dispatcher path",
                    platform,
                    platform_user_id
                );
                return Ok(());
            }
            Err(e) => return Err(e),
        };

        let reply_route = ReplyRoute {
            channel_instance_id: crate::communication::user_channel::ChannelInstanceId::new(
                &user_channel.user_id,
                platform,
                &user_channel.instance_name,
            ),
            target_channel_id,
        };

        match decision {
            RoutingDecision::SingleAgent(agent_id) => {
                self.send_to_agent(
                    &agent_id,
                    UserMessageContext {
                        message,
                        reply_route,
                        user_channel_binding_id: user_channel.id.clone(),
                        // TODO: propagate actual binding_id from route_inbound when rules match
                        agent_channel_binding_id: String::new(),
                        user_id: user_channel.user_id.clone(),
                        platform,
                    },
                )
                .await;
            }
            RoutingDecision::Broadcast(agent_ids) => {
                for agent_id in agent_ids {
                    self.send_to_agent(
                        &agent_id,
                        UserMessageContext {
                            message: message.clone(),
                            reply_route: reply_route.clone(),
                            user_channel_binding_id: user_channel.id.clone(),
                            agent_channel_binding_id: String::new(),
                            user_id: user_channel.user_id.clone(),
                            platform,
                        },
                    )
                    .await;
                }
            }
            RoutingDecision::Drop => {}
        }

        Ok(())
    }

    async fn send_to_agent(&self, agent_id: &str, ctx: UserMessageContext) {
        let queues = self.agent_queues.read().await;
        if let Some(tx) = queues.get(agent_id) {
            if let Err(e) = tx.send(ctx).await {
                tracing::warn!("Agent {} queue error: {}", agent_id, e);
            }
            return;
        }
        drop(queues);

        // Agent offline: persist message for later delivery.
        tracing::info!("Agent {} is offline, persisting message", agent_id);
        if let Err(e) = self.offline_store.enqueue(agent_id, &ctx).await {
            tracing::warn!(
                "Failed to persist offline message for agent {}: {}",
                agent_id,
                e
            );
        }
    }

    /// Flush any buffered messages for the given agent.
    async fn flush_pending(&self, agent_id: &str, tx: &mpsc::Sender<UserMessageContext>) {
        match self.offline_store.dequeue_all(agent_id).await {
            Ok(msgs) if !msgs.is_empty() => {
                tracing::info!(
                    "Flushing {} pending messages to agent {}",
                    msgs.len(),
                    agent_id
                );
                for ctx in msgs {
                    if let Err(e) = tx.send(ctx).await {
                        tracing::warn!("Agent {} queue error while flushing: {}", agent_id, e);
                        break;
                    }
                }
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(
                "Failed to flush pending messages for agent {}: {}",
                agent_id,
                e
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::communication::{Message, PlatformType};

    #[test]
    fn test_routing_rules_match_text() {
        let rules = RoutingRules {
            keyword_filters: vec!["hello".to_string()],
            ..Default::default()
        };
        let msg = Message::new(uuid::Uuid::new_v4(), PlatformType::Lark, "hello world");
        assert!(super::matches_rules(&rules, &msg));
    }

    #[test]
    fn test_routing_rules_no_match() {
        let rules = RoutingRules {
            keyword_filters: vec!["goodbye".to_string()],
            ..Default::default()
        };
        let msg = Message::new(uuid::Uuid::new_v4(), PlatformType::Lark, "hello world");
        assert!(!super::matches_rules(&rules, &msg));
    }

    #[test]
    fn test_routing_rules_require_mention() {
        let rules = RoutingRules {
            require_mention: true,
            ..Default::default()
        };
        let mut msg = Message::new(uuid::Uuid::new_v4(), PlatformType::Lark, "hello");
        assert!(!super::matches_rules(&rules, &msg));

        msg.metadata
            .insert("mentioned".to_string(), "true".to_string());
        assert!(super::matches_rules(&rules, &msg));
    }
}
