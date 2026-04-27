//! State Machine HTTP Handlers
//!
//! REST API handlers for agent state machine operations including:
//! - State queries
//! - State transitions
//! - Lifecycle management
//! - Statistics
//!
//! 🔒 P1 FIX: Enhanced state machine HTTP API.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use gateway::error::GatewayError;
use gateway::middleware::{require_any_role, AuthUser};
use serde::Deserialize;
use serde_json::json;

use crate::handlers::common::check_ownership;
use crate::state_machine::AgentLifecycleState;
use crate::AppState;

/// Get agent state
pub async fn get_agent_state(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    // Check ownership
    let agent = state
        .agent_service
        .get_agent(&id)
        .await?
        .ok_or_else(|| GatewayError::not_found("Agent", &id))?;

    check_ownership(&user, &agent)?;

    let state_info = if let Some(sm_service) = &state.state_machine_service {
        let current_state = sm_service.get_state(&id).await;
        let valid_transitions = sm_service.valid_transitions(&id).await;

        json!({
            "current_state": current_state.map(|s| s.to_string()),
            "valid_transitions": valid_transitions.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            "can_execute_tasks": current_state.map(|s| s.can_execute_tasks()).unwrap_or(false),
            "is_terminal": current_state.map(|s| s.is_terminal()).unwrap_or(true),
        })
    } else {
        // Fallback to basic state from database
        json!({
            "current_state": agent.status,
            "valid_transitions": [],
            "can_execute_tasks": agent.status == "running",
            "is_terminal": agent.status == "stopped",
        })
    };

    Ok(Json(state_info))
}

/// Get agent state context (detailed)
pub async fn get_agent_state_context(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    // Check ownership
    let agent = state
        .agent_service
        .get_agent(&id)
        .await?
        .ok_or_else(|| GatewayError::not_found("Agent", &id))?;

    check_ownership(&user, &agent)?;

    let context = if let Some(sm_service) = &state.state_machine_service {
        sm_service.get_context(&id).await.map(|ctx| {
            json!({
                "current_state": ctx.state().to_string(),
                "previous_state": ctx.previous_state.map(|s| s.to_string()),
                "state_duration_secs": ctx.current_state_duration().as_secs(),
                "total_transitions": ctx.transition_count,
                "error_count": ctx.error_count,
                "history": ctx.history().iter().map(|t| {
                    json!({
                        "to_state": t.to_state.to_string(),
                        "reason": t.reason,
                        "metadata": t.metadata,
                    })
                }).collect::<Vec<_>>(),
            })
        })
    } else {
        None
    };

    Ok(Json(json!({
        "agent_id": id,
        "context": context,
    })))
}

/// Transition agent to a specific state
pub async fn transition_state(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<TransitionStateRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    // Check ownership
    let agent = state
        .agent_service
        .get_agent(&id)
        .await?
        .ok_or_else(|| GatewayError::not_found("Agent", &id))?;

    check_ownership(&user, &agent)?;

    let target_state = parse_lifecycle_state(&req.target_state)
        .ok_or_else(|| GatewayError::bad_request("Invalid target state"))?;

    if let Some(sm_service) = &state.state_machine_service {
        sm_service
            .transition_to(&id, target_state, &req.reason)
            .await?;
    } else {
        return Err(GatewayError::service_unavailable(
            "StateMachine",
            "State machine service not available",
        ));
    }

    Ok(Json(json!({
        "agent_id": id,
        "new_state": target_state.to_string(),
        "message": "State transition completed",
    })))
}

/// Pause agent
pub async fn pause_agent(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let agent = state
        .agent_service
        .get_agent(&id)
        .await?
        .ok_or_else(|| GatewayError::not_found("Agent", &id))?;

    check_ownership(&user, &agent)?;

    if let Some(sm_service) = &state.state_machine_service {
        sm_service.pause_agent(&id).await?;
    }

    Ok(Json(json!({
        "agent_id": id,
        "status": "paused",
        "message": "Agent paused successfully",
    })))
}

/// Resume agent
pub async fn resume_agent(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let agent = state
        .agent_service
        .get_agent(&id)
        .await?
        .ok_or_else(|| GatewayError::not_found("Agent", &id))?;

    check_ownership(&user, &agent)?;

    if let Some(sm_service) = &state.state_machine_service {
        sm_service.resume_agent(&id).await?;
    }

    Ok(Json(json!({
        "agent_id": id,
        "status": "resumed",
        "message": "Agent resumed successfully",
    })))
}

/// Retry agent after error
pub async fn retry_agent(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let agent = state
        .agent_service
        .get_agent(&id)
        .await?
        .ok_or_else(|| GatewayError::not_found("Agent", &id))?;

    check_ownership(&user, &agent)?;

    if let Some(sm_service) = &state.state_machine_service {
        sm_service.retry_agent(&id).await?;
    }

    Ok(Json(json!({
        "agent_id": id,
        "status": "retrying",
        "message": "Agent retry initiated",
    })))
}

/// List valid transitions for an agent
pub async fn get_valid_transitions(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    // Check ownership
    let agent = state
        .agent_service
        .get_agent(&id)
        .await?
        .ok_or_else(|| GatewayError::not_found("Agent", &id))?;

    check_ownership(&user, &agent)?;

    let valid_transitions = if let Some(sm_service) = &state.state_machine_service {
        sm_service.valid_transitions(&id).await
    } else {
        Vec::new()
    };

    Ok(Json(json!({
        "agent_id": id,
        "current_state": if let Some(sm) = &state.state_machine_service {
            sm.get_state(&id).await.map(|s| s.to_string())
        } else {
            None
        },
        "valid_transitions": valid_transitions.iter().map(|s| {
            json!({
                "state": s.to_string(),
                "description": s.description(),
            })
        }).collect::<Vec<_>>(),
    })))
}

/// Get state machine statistics
pub async fn get_state_machine_stats(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let stats = if let Some(sm_service) = &state.state_machine_service {
        let stats = sm_service.get_statistics().await;
        json!({
            "total_agents": stats.total_agents,
            "state_distribution": stats.state_counts.iter().map(|(state, count)| {
                json!({
                    "state": state.to_string(),
                    "count": count,
                    "description": state.description(),
                })
            }).collect::<Vec<_>>(),
            "timed_out_agents": stats.timed_out_agents,
        })
    } else {
        json!({
            "message": "State machine service not available",
        })
    };

    Ok(Json(stats))
}

/// Get all possible states
pub async fn list_states() -> Json<serde_json::Value> {
    use AgentLifecycleState::*;

    let states = vec![
        Pending,
        Initializing,
        Idle,
        Working,
        Paused,
        ShuttingDown,
        Stopped,
        Error,
    ];

    Json(json!({
        "states": states.iter().map(|s| {
            json!({
                "name": s.to_string(),
                "description": s.description(),
                "is_terminal": s.is_terminal(),
                "can_execute_tasks": s.can_execute_tasks(),
                "valid_transitions": s.valid_transitions().iter().map(|t| t.to_string()).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
    }))
}

/// Check for timed out agents
pub async fn check_timeouts(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let timeouts = if let Some(sm_service) = &state.state_machine_service {
        sm_service.check_timeouts().await
    } else {
        Vec::new()
    };

    Ok(Json(json!({
        "timed_out_count": timeouts.len(),
        "agents": timeouts.iter().map(|(id, state, timeout)| {
            json!({
                "agent_id": id,
                "current_state": state.to_string(),
                "timeout_duration_secs": timeout.as_secs(),
            })
        }).collect::<Vec<_>>(),
    })))
}

// Request/Response types

#[derive(Debug, Deserialize)]
pub struct TransitionStateRequest {
    pub target_state: String,
    pub reason: String,
}

fn parse_lifecycle_state(s: &str) -> Option<AgentLifecycleState> {
    match s.to_lowercase().as_str() {
        "pending" => Some(AgentLifecycleState::Pending),
        "initializing" => Some(AgentLifecycleState::Initializing),
        "idle" => Some(AgentLifecycleState::Idle),
        "working" => Some(AgentLifecycleState::Working),
        "paused" => Some(AgentLifecycleState::Paused),
        "shutting_down" => Some(AgentLifecycleState::ShuttingDown),
        "stopped" => Some(AgentLifecycleState::Stopped),
        "error" => Some(AgentLifecycleState::Error),
        _ => None,
    }
}
