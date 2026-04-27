//! Agent Resolver - Maps channels/users to agents
//!
//! Provides resolution logic to determine which agent should handle
//! an incoming channel message.

use std::sync::Arc;

use tracing::{debug, error, info, warn};

use crate::error::GatewayError;

/// Resolves a channel message to an agent ID
pub struct AgentResolver {
    /// Optional default agent ID from configuration
    default_agent_id: Option<String>,
    /// State store for querying registered agents
    state_store: Arc<gateway::StateStore>,
    /// Agent runtime for creating fallback agents if needed
    agent_runtime: Arc<dyn gateway::AgentRuntime>,
    /// Channel-to-agent binding store (LEGACY — deprecated)
    ///
    /// P2 OPTIMIZE: Use `agent_channel_service` (new system) instead.
    /// This field is kept for backward compatibility during the migration
    /// period.
    channel_binding_store: Option<Arc<gateway::ChannelBindingStore>>,
    /// Agent channel service (new system with routing rules / default agent
    /// support)
    agent_channel_service: Option<Arc<beebotos_agents::services::AgentChannelService>>,
    /// User channel service (for auto-creating user_channel on agent
    /// auto-create)
    user_channel_service: Option<Arc<beebotos_agents::services::UserChannelService>>,
}

impl AgentResolver {
    /// Create a new agent resolver
    pub fn new(
        default_agent_id: Option<String>,
        state_store: Arc<gateway::StateStore>,
        agent_runtime: Arc<dyn gateway::AgentRuntime>,
    ) -> Self {
        Self {
            default_agent_id,
            state_store,
            agent_runtime,
            channel_binding_store: None,
            agent_channel_service: None,
            user_channel_service: None,
        }
    }

    /// Set the channel binding store (LEGACY — deprecated)
    ///
    /// P2 OPTIMIZE: Prefer `with_agent_channel_service()` for new code.
    pub fn with_channel_binding_store(mut self, store: Arc<gateway::ChannelBindingStore>) -> Self {
        self.channel_binding_store = Some(store);
        self
    }

    /// Set the agent channel service (new architecture)
    pub fn with_agent_channel_service(
        mut self,
        service: Arc<beebotos_agents::services::AgentChannelService>,
    ) -> Self {
        self.agent_channel_service = Some(service);
        self
    }

    /// Set the user channel service (for auto-creating user_channel)
    pub fn with_user_channel_service(
        mut self,
        service: Arc<beebotos_agents::services::UserChannelService>,
    ) -> Self {
        self.user_channel_service = Some(service);
        self
    }

    /// Resolve the target agent ID for a channel message
    ///
    /// Resolution order:
    /// 1. ChannelBindingStore binding for (platform, channel_id) — legacy
    ///    system
    /// 2. AgentChannelService default binding for (platform, channel_id) — new
    ///    system
    /// 3. Configured default_agent_id (if valid and running)
    /// 4. First available agent from StateStore
    /// 5. Auto-create a default agent if none found
    pub async fn resolve(
        &self,
        platform: beebotos_agents::communication::PlatformType,
        channel_id: &str,
        user_id: &str,
    ) -> Result<String, GatewayError> {
        let platform_str = platform.to_string();

        // 1. Try legacy channel-agent binding store (DEPRECATED)
        // P2 OPTIMIZE: This layer will be removed once all bindings are migrated
        // to the new system. Run `POST /api/v1/admin/migrate-bindings` to migrate.
        if let Some(ref binding_store) = self.channel_binding_store {
            if let Some(agent_id) = binding_store.resolve_agent(&platform_str, channel_id).await {
                warn!(
                    "Agent {} resolved via LEGACY ChannelBindingStore ({}:{}). Consider migrating \
                     to the new system.",
                    agent_id, platform_str, channel_id
                );
                match self.agent_runtime.status(&agent_id).await {
                    Ok(status) => {
                        if status.state != gateway::AgentState::Stopped
                            && status.state != gateway::AgentState::Error
                        {
                            info!(
                                "Resolved agent {} from ChannelBindingStore ({}:{})",
                                agent_id, platform_str, channel_id
                            );
                            return Ok(agent_id);
                        }
                        warn!(
                            "Bound agent {} for {}:{} is in state {:?}, skipping",
                            agent_id, platform_str, channel_id, status.state
                        );
                    }
                    Err(e) => {
                        warn!(
                            "Bound agent {} for {}:{} not found: {}",
                            agent_id, platform_str, channel_id, e
                        );
                    }
                }
            }
        }

        // 🟢 P1 FIX: Try new AgentChannelService system
        // P2 OPTIMIZE: Use channel_id as platform_channel_id to align with
        // bind_agent_channel semantics. The lookup key is the platform-level
        // channel identifier (chat_id, room_id, etc.), not the individual
        // sender/user ID.
        if let Some(ref agent_channel_service) = self.agent_channel_service {
            match agent_channel_service
                .find_default_agent_for_platform_channel(platform, channel_id)
                .await
            {
                Ok(Some(agent_id)) => match self.agent_runtime.status(&agent_id).await {
                    Ok(status) => {
                        if status.state != gateway::AgentState::Stopped
                            && status.state != gateway::AgentState::Error
                        {
                            info!(
                                "Resolved agent {} from AgentChannelService ({}:{})",
                                agent_id, platform_str, channel_id
                            );
                            return Ok(agent_id);
                        }
                        warn!(
                            "New-system bound agent {} for {}:{} is in state {:?}, skipping",
                            agent_id, platform_str, channel_id, status.state
                        );
                    }
                    Err(e) => {
                        warn!(
                            "New-system bound agent {} for {}:{} not found: {}",
                            agent_id, platform_str, channel_id, e
                        );
                    }
                },
                Ok(None) => {
                    debug!(
                        "No default agent binding found in AgentChannelService for {}:{}",
                        platform_str, channel_id
                    );
                }
                Err(e) => {
                    warn!(
                        "Failed to query AgentChannelService for {}:{}: {}",
                        platform_str, channel_id, e
                    );
                }
            }
        }

        // 2. Try configured default agent
        if let Some(ref agent_id) = self.default_agent_id {
            match self.agent_runtime.status(agent_id).await {
                Ok(status) => {
                    if status.state != gateway::AgentState::Stopped
                        && status.state != gateway::AgentState::Error
                    {
                        info!("Resolved agent {} from default_agent_id config", agent_id);
                        return Ok(agent_id.clone());
                    }
                    warn!(
                        "Configured default_agent_id {} is in state {:?}, skipping",
                        agent_id, status.state
                    );
                }
                Err(e) => {
                    warn!("Configured default_agent_id {} not found: {}", agent_id, e);
                }
            }
        }

        // 2. Query StateStore for the first available agent
        let query_result = self
            .state_store
            .query(gateway::StateQuery::ListAgents {
                filter: Some(gateway::AgentFilter {
                    state: None,
                    has_capability: None,
                    created_after: None,
                    created_before: None,
                }),
                limit: 100,
                offset: 0,
            })
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to list agents from state store: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            })?;

        if let gateway::QueryResult::AgentList { agents, .. } = query_result {
            for agent_info in agents {
                if agent_info.current_state != gateway::AgentState::Stopped
                    && agent_info.current_state != gateway::AgentState::Error
                {
                    info!(
                        "Resolved agent {} from StateStore (first available)",
                        agent_info.agent_id
                    );
                    return Ok(agent_info.agent_id);
                }
            }
        }

        // 3. Auto-create a default agent
        let agent_id = format!("auto-agent-{}-{}", platform_str, channel_id);

        // 🟢 P1 FIX: Check if agent already exists (e.g., recovered from persistent
        // state) before trying to spawn
        match self.agent_runtime.status(&agent_id).await {
            Ok(status) => {
                if status.state != gateway::AgentState::Stopped
                    && status.state != gateway::AgentState::Error
                {
                    info!(
                        "Auto-created agent {} already exists and is healthy, reusing",
                        agent_id
                    );
                    return Ok(agent_id);
                }
                warn!(
                    "Auto-created agent {} exists but is in state {:?}, respawning",
                    agent_id, status.state
                );
            }
            Err(_) => {
                // Agent doesn't exist, proceed to create
            }
        }

        let agent_name = format!("Auto Agent {} {}", platform_str, channel_id);
        let llm_config = gateway::LlmConfig {
            provider: "kimi".to_string(),
            model: "kimi-k2.5".to_string(),
            api_key: None,
            temperature: 0.7,
            max_tokens: 800,
        };
        let agent_config = gateway::AgentConfigBuilder::new(&agent_id, &agent_name)
            .description("Auto-created default agent for incoming messages")
            .with_llm(llm_config)
            .build();

        info!(
            "🆕 No available agent found, auto-creating default agent {}",
            agent_id
        );
        self.agent_runtime.spawn(agent_config).await.map_err(|e| {
            error!("❌ Failed to auto-create default agent {}: {}", agent_id, e);
            GatewayError::Internal {
                message: format!("Failed to auto-create default agent: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            }
        })?;

        // 🟢 P1 FIX: Skip LEGACY binding — new system now fully handles agent-channel
        // binding. LEGACY ChannelBindingStore binding is deprecated and removed
        // to prevent duplicate binding records and misleading migration
        // warnings.
        //
        // if let Some(ref binding_store) = self.channel_binding_store {
        //     if let Err(e) = binding_store.bind(&platform_str, channel_id,
        // &agent_id).await {         warn!("Failed to bind auto-created agent
        // ...");     }
        // }

        // P1 FIX: Auto-create user_channel and bind via new system
        if let (Some(ref user_ch_svc), Some(ref agent_ch_svc)) = (
            self.user_channel_service.as_ref(),
            self.agent_channel_service.as_ref(),
        ) {
            use beebotos_agents::communication::{
                ChannelBindingStatus, PlatformType, UserChannelBinding,
            };
            let platform_type = match platform_str.as_str() {
                "slack" => PlatformType::Slack,
                "telegram" => PlatformType::Telegram,
                "discord" => PlatformType::Discord,
                "whatsapp" => PlatformType::WhatsApp,
                "signal" => PlatformType::Signal,
                "imessage" => PlatformType::IMessage,
                "wechat" => PlatformType::WeChat,
                "teams" => PlatformType::Teams,
                "twitter" => PlatformType::Twitter,
                "lark" | "feishu" => PlatformType::Lark,
                "dingtalk" => PlatformType::DingTalk,
                "matrix" => PlatformType::Matrix,
                "googlechat" => PlatformType::GoogleChat,
                "line" => PlatformType::Line,
                "qq" => PlatformType::QQ,
                "irc" => PlatformType::IRC,
                "webchat" => PlatformType::WebChat,
                _ => PlatformType::Custom,
            };

            let uc_binding = UserChannelBinding {
                id: uuid::Uuid::new_v4().to_string(),
                user_id: user_id.to_string(),
                platform: platform_type,
                instance_name: format!("{}_auto", platform_str),
                platform_user_id: Some(channel_id.to_string()),
                status: ChannelBindingStatus::Active,
                webhook_path: None,
            };

            match user_ch_svc.create_binding_only(&uc_binding).await {
                Ok(()) => {
                    info!(
                        "Auto-created user_channel {} for auto-agent {} (platform_user_id: {})",
                        uc_binding.id, agent_id, channel_id
                    );
                    let routing_rules =
                        beebotos_agents::communication::agent_channel::RoutingRules::default();
                    if let Err(e) = agent_ch_svc
                        .bind_agent(&agent_id, &uc_binding.id, None, 0, routing_rules, true)
                        .await
                    {
                        warn!(
                            "Failed to bind auto-created agent {} to user_channel {}: {}",
                            agent_id, uc_binding.id, e
                        );
                    } else {
                        info!(
                            "Bound auto-created agent {} to user_channel {} (new system)",
                            agent_id, uc_binding.id
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to auto-create user_channel for auto-agent {}: {}",
                        agent_id, e
                    );
                }
            }
        }

        Ok(agent_id)
    }
}
