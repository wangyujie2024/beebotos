//! Agent HTTP Handlers (V2 - Using new AgentRuntime trait)
//!
//! 🟢 P1 FIX: Migrated to use AgentRuntime trait and StateStore (CQRS).
//! This version is decoupled from the concrete beebotos_agents implementation.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use gateway::error::GatewayError;
use gateway::middleware::{require_any_role, AuthUser};
use gateway::{
    AgentConfigBuilder, AgentState, AgentStateCommand, LlmConfig, MemoryConfig, QueryResult,
    StateCommand, StateQuery, TaskConfig,
};
use serde::Deserialize;
use serde_json::json;
use tracing::{debug, info, warn};

use crate::models::{
    AgentResponse, CreateAgentRequest, ModelInfo, PaginatedResponse, PaginationParams,
};
use crate::AppState;

/// Parse a platform string into PlatformType.
fn parse_platform(platform: &str) -> Option<beebotos_agents::communication::PlatformType> {
    use beebotos_agents::communication::PlatformType;
    match platform.to_lowercase().as_str() {
        "slack" => Some(PlatformType::Slack),
        "telegram" => Some(PlatformType::Telegram),
        "discord" => Some(PlatformType::Discord),
        "whatsapp" => Some(PlatformType::WhatsApp),
        "signal" => Some(PlatformType::Signal),
        "imessage" => Some(PlatformType::IMessage),
        "wechat" => Some(PlatformType::WeChat),
        "teams" => Some(PlatformType::Teams),
        "twitter" => Some(PlatformType::Twitter),
        "lark" | "feishu" => Some(PlatformType::Lark),
        "dingtalk" => Some(PlatformType::DingTalk),
        "matrix" => Some(PlatformType::Matrix),
        "googlechat" => Some(PlatformType::GoogleChat),
        "line" => Some(PlatformType::Line),
        "qq" => Some(PlatformType::QQ),
        "irc" => Some(PlatformType::IRC),
        "webchat" => Some(PlatformType::WebChat),
        _ => Some(PlatformType::Custom),
    }
}

/// Check if user owns the agent from StateStore metadata.
fn check_v2_ownership(
    user: &AuthUser,
    metadata: &HashMap<String, String>,
) -> Result<(), GatewayError> {
    if user.is_admin() || metadata.get("owner_id").map(|s| s.as_str()) == Some(&user.user_id) {
        Ok(())
    } else {
        Err(GatewayError::forbidden(
            "You don't have permission to access this agent",
        ))
    }
}

/// List all agents with pagination (V2)
///
/// Uses StateStore (CQRS) for efficient querying.
pub async fn list_agents_v2(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    axum::extract::Query(params): axum::extract::Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<AgentResponse>>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    // 🟢 P1 FIX: Use StateStore (CQRS) for querying
    let query_result = state
        .state_store
        .query(StateQuery::ListAgents {
            filter: None,
            limit: params.per_page as usize,
            offset: params.offset() as usize,
        })
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to list agents: {}", e)))?;

    let agents = match query_result {
        QueryResult::AgentList { agents, .. } => agents,
        _ => return Err(GatewayError::internal("Unexpected query result")),
    };

    // 🟢 P0 FIX: Filter agents by ownership unless user is admin
    let agents: Vec<gateway::AgentInfo> = if user.is_admin() {
        agents
    } else {
        agents
            .into_iter()
            .filter(|info| info.metadata.get("owner_id").map(|s| s.as_str()) == Some(&user.user_id))
            .collect()
    };
    let total = agents.len();

    // Convert to response models
    let responses: Vec<AgentResponse> = agents
        .into_iter()
        .map(|info| AgentResponse {
            id: info.agent_id,
            name: info.config.name,
            description: Some(info.config.description),
            status: info.current_state.to_string(),
            capabilities: info
                .config
                .capabilities
                .into_iter()
                .map(|c| c.name)
                .collect(),
            model: ModelInfo {
                provider: info.config.llm_config.provider,
                name: info.config.llm_config.model,
            },
            created_at: info.created_at,
            updated_at: info.updated_at,
            last_heartbeat: None,
        })
        .collect();

    Ok(Json(PaginatedResponse::new(
        responses,
        total as i64,
        params.page,
        params.per_page,
    )))
}

/// Create new agent (V2)
///
/// Uses AgentRuntime trait for decoupled agent lifecycle management.
pub async fn create_agent_v2(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<CreateAgentRequest>,
) -> Result<impl IntoResponse, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    // Validate input
    if req.name.is_empty() {
        return Err(GatewayError::validation(vec![
            gateway::error::ValidationError {
                field: "name".to_string(),
                message: "Name is required".to_string(),
                code: "required".to_string(),
            },
        ]));
    }

    // 🟢 P1 FIX: Build GatewayAgentConfig using builder pattern
    let agent_config = AgentConfigBuilder::new(uuid::Uuid::new_v4().to_string(), &req.name)
        .description(&req.description.unwrap_or_default())
        .with_llm(LlmConfig {
            provider: req.model_provider.unwrap_or_else(|| "openai".to_string()),
            model: req.model_name.unwrap_or_else(|| "gpt-4".to_string()),
            api_key: None,
            temperature: 0.7,
            max_tokens: 2000,
        })
        .with_memory(MemoryConfig {
            memory_type: "local".to_string(),
            storage_path: "data/memory".to_string(),
            max_entries: 10000,
        })
        .build();

    // 🟢 P1 FIX: Use AgentRuntime trait to spawn agent
    let handle = state
        .agent_runtime
        .spawn(agent_config.clone())
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to spawn agent: {}", e)))?;

    // 🟢 P1 FIX: Register in StateStore for persistence
    state
        .state_store
        .execute(StateCommand::RegisterAgent {
            agent_id: handle.agent_id.clone(),
            config: agent_config.clone(),
            metadata: {
                let mut meta = std::collections::HashMap::new();
                meta.insert("owner_id".to_string(), user.user_id.clone());
                meta.insert("created_by".to_string(), user.user_id.clone());
                meta
            },
        })
        .await
        .map_err(|e| GatewayError::state(format!("Failed to register agent: {}", e)))?;

    tracing::info!(
        agent_id = %handle.agent_id,
        user_id = %user.user_id,
        "Agent created via AgentRuntime trait"
    );

    let response = AgentResponse {
        id: handle.agent_id,
        name: agent_config.name,
        description: Some(agent_config.description),
        status: "registered".to_string(),
        capabilities: agent_config
            .capabilities
            .into_iter()
            .map(|c| c.name)
            .collect(),
        model: ModelInfo {
            provider: agent_config.llm_config.provider,
            name: agent_config.llm_config.model,
        },
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        last_heartbeat: None,
    };

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "agent": response,
            "message": "Agent created successfully via AgentRuntime",
        })),
    ))
}

/// Get agent by ID (V2)
pub async fn get_agent_v2(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    // 🟢 P1 FIX: Use StateStore to query agent info
    let query_result = state
        .state_store
        .query(StateQuery::GetAgentInfo {
            agent_id: id.clone(),
        })
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to get agent: {}", e)))?;

    let info = match &query_result {
        QueryResult::AgentInfo { metadata, .. } => {
            // 🟢 P0 FIX: Check ownership from metadata
            check_v2_ownership(&user, metadata)?;
            query_result
        }
        _ => return Err(GatewayError::not_found("Agent", &id)),
    };

    Ok(Json(json!({
        "agent": info,
        "version": "v2 (AgentRuntime)",
    })))
}

/// Delete agent (V2)
pub async fn delete_agent_v2(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    // 🟢 P0 FIX: Verify ownership before deletion
    let query_result = state
        .state_store
        .query(StateQuery::GetAgentInfo {
            agent_id: id.clone(),
        })
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to get agent: {}", e)))?;
    if let QueryResult::AgentInfo { metadata, .. } = &query_result {
        check_v2_ownership(&user, metadata)?;
    } else {
        return Err(GatewayError::not_found("Agent", &id));
    }

    // 🟢 P1 FIX: Use AgentRuntime trait to stop agent
    state
        .agent_runtime
        .stop(&id)
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to stop agent: {}", e)))?;

    // 🟢 P1 FIX: Archive in StateStore
    state
        .state_store
        .execute(StateCommand::ArchiveAgent {
            agent_id: id.clone(),
        })
        .await
        .map_err(|e| GatewayError::state(format!("Failed to archive agent: {}", e)))?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "message": "Agent deleted successfully",
            "agent_id": id,
        })),
    ))
}

/// Start agent (V2)
pub async fn start_agent_v2(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    // 🟢 P0 FIX: Verify ownership before starting
    let query_result = state
        .state_store
        .query(StateQuery::GetAgentInfo {
            agent_id: id.clone(),
        })
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to get agent: {}", e)))?;
    if let QueryResult::AgentInfo { metadata, .. } = &query_result {
        check_v2_ownership(&user, metadata)?;
    } else {
        return Err(GatewayError::not_found("Agent", &id));
    }

    // 🟢 P1 FIX: Use AgentRuntime trait
    state
        .agent_runtime
        .send_command(&id, AgentStateCommand::Start)
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to start agent: {}", e)))?;

    // 🟢 P1 FIX: Record state transition in StateStore
    state
        .state_store
        .execute(StateCommand::Transition {
            agent_id: id.clone(),
            from: AgentState::Registered,
            to: AgentState::Working,
            reason: Some("User requested start".to_string()),
        })
        .await
        .map_err(|e| GatewayError::state(format!("Failed to record transition: {}", e)))?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "message": "Agent start command sent",
            "agent_id": id,
        })),
    ))
}

/// Stop agent (V2)
pub async fn stop_agent_v2(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    // 🟢 P0 FIX: Verify ownership before stopping
    let query_result = state
        .state_store
        .query(StateQuery::GetAgentInfo {
            agent_id: id.clone(),
        })
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to get agent: {}", e)))?;
    if let QueryResult::AgentInfo { metadata, .. } = &query_result {
        check_v2_ownership(&user, metadata)?;
    } else {
        return Err(GatewayError::not_found("Agent", &id));
    }

    // 🟢 P1 FIX: Use AgentRuntime trait
    state
        .agent_runtime
        .send_command(&id, AgentStateCommand::Stop)
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to stop agent: {}", e)))?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "message": "Agent stop command sent",
            "agent_id": id,
        })),
    ))
}

/// Get agent status (V2)
pub async fn get_agent_status_v2(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    // 🟢 P0 FIX: Verify ownership before querying status
    let query_result = state
        .state_store
        .query(StateQuery::GetAgentInfo {
            agent_id: id.clone(),
        })
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to get agent: {}", e)))?;
    if let QueryResult::AgentInfo { metadata, .. } = &query_result {
        check_v2_ownership(&user, metadata)?;
    } else {
        return Err(GatewayError::not_found("Agent", &id));
    }

    // 🟢 P1 FIX: Use AgentRuntime trait
    let status = state
        .agent_runtime
        .status(&id)
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to get agent status: {}", e)))?;

    Ok(Json(json!({
        "agent_id": status.agent_id,
        "state": status.state.to_string(),
        "current_task": status.current_task,
        "last_activity": status.last_activity,
        "total_tasks": status.total_tasks,
        "failed_tasks": status.failed_tasks,
        "kernel_task_id": status.kernel_task_id,
    })))
}

/// Execute task on agent (V2)
#[derive(Debug, Deserialize)]
pub struct ExecuteTaskRequest {
    pub task_type: String,
    pub input: serde_json::Value,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_timeout() -> u64 {
    60
}

pub async fn execute_task_v2(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<ExecuteTaskRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    // 🟢 P0 FIX: Verify ownership before executing task
    let query_result = state
        .state_store
        .query(StateQuery::GetAgentInfo {
            agent_id: id.clone(),
        })
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to get agent: {}", e)))?;
    if let QueryResult::AgentInfo { metadata, .. } = &query_result {
        check_v2_ownership(&user, metadata)?;
    } else {
        return Err(GatewayError::not_found("Agent", &id));
    }

    let task_config = TaskConfig {
        task_type: req.task_type,
        input: req.input,
        timeout_secs: req.timeout_secs,
        priority: 5,
    };

    // 🟢 P1 FIX: Use AgentRuntime trait to execute task
    let result = state
        .agent_runtime
        .execute_task(&id, task_config)
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to execute task: {}", e)))?;

    Ok(Json(json!({
        "success": result.success,
        "output": result.output,
        "execution_time_ms": result.execution_time_ms,
        "error": result.error,
    })))
}

/// Bind an agent to a channel
#[derive(Debug, Deserialize)]
pub struct BindChannelRequest {
    pub platform: String,
    pub channel_id: String,
    /// P1 FIX: Optional explicit platform_user_id. If not provided, uses
    /// channel_id. This should match the sender ID used by the platform's
    /// webhook handler (e.g., Telegram chat_id, Lark open_id, Slack
    /// user_id).
    pub platform_user_id: Option<String>,
}

pub async fn bind_agent_channel(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<BindChannelRequest>,
) -> Result<impl IntoResponse, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    // 🟢 P0 FIX: Verify ownership before binding
    let query_result = state
        .state_store
        .query(StateQuery::GetAgentInfo {
            agent_id: id.clone(),
        })
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to get agent: {}", e)))?;
    if let QueryResult::AgentInfo { metadata, .. } = &query_result {
        check_v2_ownership(&user, metadata)?;
    } else {
        return Err(GatewayError::not_found("Agent", &id));
    }

    let store = state
        .channel_binding_store
        .as_ref()
        .ok_or_else(|| GatewayError::internal("Channel binding store not initialized"))?;

    store
        .bind(&req.platform, &req.channel_id, &id)
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to bind channel: {}", e)))?;

    // 🟢 P1 FIX: Also bind via new AgentChannelService (auto-create user_channel if
    // missing)
    if let (Some(ref agent_ch_svc), Some(ref user_ch_svc)) = (
        state.agent_channel_service.as_ref(),
        state.user_channel_service.as_ref(),
    ) {
        if let Some(platform) = parse_platform(&req.platform) {
            // Use explicit platform_user_id if provided, otherwise fall back to channel_id
            let platform_user_id = req
                .platform_user_id
                .as_ref()
                .unwrap_or(&req.channel_id)
                .clone();

            let user_channel = match user_ch_svc
                .find_by_platform_user(platform, &platform_user_id)
                .await
            {
                Ok(Some(uc)) => uc,
                Ok(None) => {
                    // Auto-create user_channel if not exists
                    let binding = beebotos_agents::communication::UserChannelBinding {
                        id: uuid::Uuid::new_v4().to_string(),
                        user_id: user.user_id.clone(),
                        platform,
                        instance_name: format!("{}_default", req.platform),
                        platform_user_id: Some(platform_user_id.clone()),
                        status: beebotos_agents::communication::ChannelBindingStatus::Active,
                        webhook_path: None,
                    };
                    if let Err(e) = user_ch_svc.create_binding_only(&binding).await {
                        warn!(
                            "Failed to auto-create user_channel for {}:{}: {}",
                            platform, platform_user_id, e
                        );
                        return Ok((
                            StatusCode::OK,
                            Json(json!({
                                "message": "Channel bound successfully (legacy only — user_channel auto-create failed)",
                                "agent_id": id,
                                "platform": req.platform,
                                "channel_id": req.channel_id,
                            })),
                        ));
                    }
                    info!(
                        "Auto-created user_channel {} for user {} on {:?} (platform_user_id: {})",
                        binding.id, user.user_id, platform, platform_user_id
                    );
                    binding
                }
                Err(e) => {
                    warn!("Failed to lookup user channel for new-system bind: {}", e);
                    return Ok((
                        StatusCode::OK,
                        Json(json!({
                            "message": "Channel bound successfully (legacy only)",
                            "agent_id": id,
                            "platform": req.platform,
                            "channel_id": req.channel_id,
                        })),
                    ));
                }
            };

            let routing_rules =
                beebotos_agents::communication::agent_channel::RoutingRules::default();
            if let Err(e) = agent_ch_svc
                .bind_agent(&id, &user_channel.id, None, 0, routing_rules, true)
                .await
            {
                warn!(
                    "Failed to bind agent {} to user channel {} via new system: {}",
                    id, user_channel.id, e
                );
            } else {
                info!(
                    "Also bound agent {} to user channel {} via AgentChannelService",
                    id, user_channel.id
                );
            }
        }
    }

    Ok((
        StatusCode::OK,
        Json(json!({
            "message": "Channel bound successfully",
            "agent_id": id,
            "platform": req.platform,
            "channel_id": req.channel_id,
        })),
    ))
}

/// Unbind an agent from a channel
pub async fn unbind_agent_channel(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path((id, channel_id)): Path<(String, String)>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    // 🟢 P0 FIX: Verify ownership before unbinding
    let query_result = state
        .state_store
        .query(StateQuery::GetAgentInfo {
            agent_id: id.clone(),
        })
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to get agent: {}", e)))?;
    if let QueryResult::AgentInfo { metadata, .. } = &query_result {
        check_v2_ownership(&user, metadata)?;
    } else {
        return Err(GatewayError::not_found("Agent", &id));
    }

    let store = state
        .channel_binding_store
        .as_ref()
        .ok_or_else(|| GatewayError::internal("Channel binding store not initialized"))?;

    let platform = params
        .get("platform")
        .cloned()
        .unwrap_or_else(|| "webchat".to_string());

    store
        .unbind(&platform, &channel_id)
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to unbind channel: {}", e)))?;

    // 🟢 P1 FIX: Also unbind via new AgentChannelService if user channel exists
    if let (Some(ref agent_ch_svc), Some(ref user_ch_svc)) = (
        state.agent_channel_service.as_ref(),
        state.user_channel_service.as_ref(),
    ) {
        if let Some(platform_type) = parse_platform(&platform) {
            match user_ch_svc
                .find_by_platform_user(platform_type, &channel_id)
                .await
            {
                Ok(Some(user_channel)) => {
                    if let Err(e) = agent_ch_svc.unbind_agent(&id, &user_channel.id).await {
                        warn!(
                            "Failed to unbind agent {} from user channel {} via new system: {}",
                            id, user_channel.id, e
                        );
                    } else {
                        info!(
                            "Also unbound agent {} from user channel {} via AgentChannelService",
                            id, user_channel.id
                        );
                    }
                }
                Ok(None) => {
                    debug!(
                        "No user channel found for platform {:?} user {}, skipping new-system \
                         unbind",
                        platform_type, channel_id
                    );
                }
                Err(e) => {
                    warn!("Failed to lookup user channel for new-system unbind: {}", e);
                }
            }
        }
    }

    Ok((
        StatusCode::OK,
        Json(json!({
            "message": "Channel unbound successfully",
            "agent_id": id,
            "platform": platform,
            "channel_id": channel_id,
        })),
    ))
}

/// List channels bound to an agent
pub async fn list_agent_channels(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    // 🟢 P0 FIX: Verify ownership before listing channels
    let query_result = state
        .state_store
        .query(StateQuery::GetAgentInfo {
            agent_id: id.clone(),
        })
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to get agent: {}", e)))?;
    if let QueryResult::AgentInfo { metadata, .. } = &query_result {
        check_v2_ownership(&user, metadata)?;
    } else {
        return Err(GatewayError::not_found("Agent", &id));
    }

    let store = state
        .channel_binding_store
        .as_ref()
        .ok_or_else(|| GatewayError::internal("Channel binding store not initialized"))?;

    let legacy_bindings = store
        .list_bindings_for_agent(&id)
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to list bindings: {}", e)))?;

    // 🟢 P1 FIX: Also query new AgentChannelService bindings
    let mut merged_bindings: Vec<serde_json::Value> = legacy_bindings
        .into_iter()
        .map(|b| {
            json!({
                "platform": b.platform,
                "channel_id": b.channel_id,
                "agent_id": b.agent_id,
                "created_at": b.created_at,
                "source": "legacy",
            })
        })
        .collect();

    if let Some(ref agent_ch_svc) = state.agent_channel_service {
        match agent_ch_svc.list_channels_for_agent(&id).await {
            Ok(new_bindings) => {
                for b in new_bindings {
                    merged_bindings.push(json!({
                        "user_channel_id": b.user_channel_id,
                        "agent_id": b.agent_id,
                        "binding_name": b.binding_name,
                        "is_default": b.is_default,
                        "priority": b.priority,
                        "routing_rules": b.routing_rules,
                        "source": "agent_channel_service",
                    }));
                }
            }
            Err(e) => {
                warn!("Failed to list bindings from AgentChannelService: {}", e);
            }
        }
    }

    Ok(Json(json!({
        "agent_id": id,
        "bindings": merged_bindings,
        "total": merged_bindings.len(),
    })))
}

// ============================================================================
// P2 FIX: Pure new-system Agent-Channel binding APIs
// ============================================================================

/// Bind an agent to a user channel via the NEW system only (no legacy write)
#[derive(Debug, Deserialize)]
pub struct BindAgentChannelRequest {
    pub user_channel_id: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub routing_rules: beebotos_agents::communication::agent_channel::RoutingRules,
}

pub async fn bind_agent_channel_v2(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<BindAgentChannelRequest>,
) -> Result<impl IntoResponse, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    // Verify ownership
    let query_result = state
        .state_store
        .query(StateQuery::GetAgentInfo {
            agent_id: id.clone(),
        })
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to get agent: {}", e)))?;
    if let QueryResult::AgentInfo { metadata, .. } = &query_result {
        check_v2_ownership(&user, metadata)?;
    } else {
        return Err(GatewayError::not_found("Agent", &id));
    }

    let agent_ch_svc = state
        .agent_channel_service
        .as_ref()
        .ok_or_else(|| GatewayError::internal("Agent channel service not initialized"))?;

    let binding = agent_ch_svc
        .bind_agent(
            &id,
            &req.user_channel_id,
            None,
            req.priority,
            req.routing_rules,
            req.is_default,
        )
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to bind agent channel: {}", e)))?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "message": "Agent bound to user channel successfully (new system only)",
            "binding_id": binding.id,
            "agent_id": id,
            "user_channel_id": req.user_channel_id,
            "is_default": req.is_default,
        })),
    ))
}

/// List agent-channel bindings via the NEW system only
pub async fn list_agent_channel_bindings_v2(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let query_result = state
        .state_store
        .query(StateQuery::GetAgentInfo {
            agent_id: id.clone(),
        })
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to get agent: {}", e)))?;
    if let QueryResult::AgentInfo { metadata, .. } = &query_result {
        check_v2_ownership(&user, metadata)?;
    } else {
        return Err(GatewayError::not_found("Agent", &id));
    }

    let agent_ch_svc = state
        .agent_channel_service
        .as_ref()
        .ok_or_else(|| GatewayError::internal("Agent channel service not initialized"))?;

    let bindings = agent_ch_svc
        .list_channels_for_agent(&id)
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to list bindings: {}", e)))?;

    let bindings_json: Vec<serde_json::Value> = bindings
        .into_iter()
        .map(|b| {
            json!({
                "id": b.id,
                "agent_id": b.agent_id,
                "user_channel_id": b.user_channel_id,
                "binding_name": b.binding_name,
                "is_default": b.is_default,
                "priority": b.priority,
                "routing_rules": b.routing_rules,
            })
        })
        .collect();

    Ok(Json(json!({
        "agent_id": id,
        "bindings": bindings_json,
        "total": bindings_json.len(),
    })))
}

/// Unbind an agent from a user channel via the NEW system only
#[derive(Debug, Deserialize)]
pub struct UnbindAgentChannelRequest {
    pub user_channel_id: String,
}

pub async fn unbind_agent_channel_v2(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UnbindAgentChannelRequest>,
) -> Result<impl IntoResponse, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let query_result = state
        .state_store
        .query(StateQuery::GetAgentInfo {
            agent_id: id.clone(),
        })
        .await
        .map_err(|e| GatewayError::agent(format!("Failed to get agent: {}", e)))?;
    if let QueryResult::AgentInfo { metadata, .. } = &query_result {
        check_v2_ownership(&user, metadata)?;
    } else {
        return Err(GatewayError::not_found("Agent", &id));
    }

    let agent_ch_svc = state
        .agent_channel_service
        .as_ref()
        .ok_or_else(|| GatewayError::internal("Agent channel service not initialized"))?;

    agent_ch_svc
        .unbind_agent(&id, &req.user_channel_id)
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to unbind agent channel: {}", e)))?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "message": "Agent unbound from user channel successfully (new system only)",
            "agent_id": id,
            "user_channel_id": req.user_channel_id,
        })),
    ))
}

// ============================================================================
// P2 FIX: Migrate legacy bindings to new system
// ============================================================================

pub async fn migrate_legacy_bindings(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Result<impl IntoResponse, GatewayError> {
    if !user.is_admin() {
        return Err(GatewayError::forbidden("Admin access required"));
    }

    let legacy_store = state
        .channel_binding_store
        .as_ref()
        .ok_or_else(|| GatewayError::internal("Legacy channel binding store not initialized"))?;

    let user_ch_svc = state
        .user_channel_service
        .as_ref()
        .ok_or_else(|| GatewayError::internal("User channel service not initialized"))?;

    let agent_ch_svc = state
        .agent_channel_service
        .as_ref()
        .ok_or_else(|| GatewayError::internal("Agent channel service not initialized"))?;

    let legacy_bindings = legacy_store
        .list_all_bindings()
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to list legacy bindings: {}", e)))?;

    let mut migrated = 0;
    let mut skipped = 0;
    let mut errors = 0;

    for binding in legacy_bindings {
        let platform = match parse_platform(&binding.platform) {
            Some(p) => p,
            None => {
                warn!(
                    "Skipping migration for unknown platform: {}",
                    binding.platform
                );
                skipped += 1;
                continue;
            }
        };

        // Find or create user_channel
        let user_channel = match user_ch_svc
            .find_by_platform_user(platform, &binding.channel_id)
            .await
        {
            Ok(Some(uc)) => uc,
            Ok(None) => {
                let uc_binding = beebotos_agents::communication::UserChannelBinding {
                    id: uuid::Uuid::new_v4().to_string(),
                    user_id: "migrated".to_string(),
                    platform,
                    instance_name: format!("{}_migrated", binding.platform),
                    platform_user_id: Some(binding.channel_id.clone()),
                    status: beebotos_agents::communication::ChannelBindingStatus::Active,
                    webhook_path: None,
                };
                if let Err(e) = user_ch_svc.create_binding_only(&uc_binding).await {
                    warn!(
                        "Failed to create user_channel for migration {}:{}: {}",
                        binding.platform, binding.channel_id, e
                    );
                    errors += 1;
                    continue;
                }
                uc_binding
            }
            Err(e) => {
                warn!("Failed to lookup user_channel for migration: {}", e);
                errors += 1;
                continue;
            }
        };

        // Create agent_channel_binding
        let routing_rules = beebotos_agents::communication::agent_channel::RoutingRules::default();
        if let Err(e) = agent_ch_svc
            .bind_agent(
                &binding.agent_id,
                &user_channel.id,
                None,
                0,
                routing_rules,
                true,
            )
            .await
        {
            warn!(
                "Failed to bind agent {} to user_channel {} during migration: {}",
                binding.agent_id, user_channel.id, e
            );
            errors += 1;
            continue;
        }

        info!(
            "Migrated legacy binding: {}:{} -> agent {} (user_channel: {})",
            binding.platform, binding.channel_id, binding.agent_id, user_channel.id
        );
        migrated += 1;
    }

    Ok((
        StatusCode::OK,
        Json(json!({
            "message": "Migration completed",
            "migrated": migrated,
            "skipped": skipped,
            "errors": errors,
        })),
    ))
}

// Migration Guide Comments:
//
// To migrate existing handlers:
//
// 1. Replace `state.agent_service.xxx()` with `state.agent_runtime.xxx()` for
//    agent lifecycle
// 2. Replace database queries with `state.state_store.query()` for reads
// 3. Replace state changes with `state.state_store.execute()` for writes
// 4. Use `GatewayAgentConfig` instead of internal `AgentConfig`
// 5. Use `AgentState` from gateway-lib instead of internal state
//
// Benefits:
// - Decoupled from concrete beebotos_agents implementation
// - Type-safe trait-based interface
// - CQRS pattern for better performance
// - Event sourcing for audit trail
