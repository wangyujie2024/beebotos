//! Agent-Channel binding models and routing rules

use serde::{Deserialize, Serialize};

/// Binding between an Agent and a UserChannel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentChannelBinding {
    pub id: String,
    pub agent_id: String,
    pub user_channel_id: String,
    pub binding_name: Option<String>,
    pub is_default: bool,
    pub priority: i32,
    pub routing_rules: RoutingRules,
}

/// Rules used to decide whether an inbound message should be routed to an
/// agent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoutingRules {
    pub allowed_group_ids: Vec<String>,
    pub allowed_user_ids: Vec<String>,
    pub keyword_filters: Vec<String>,
    pub require_mention: bool,
}
