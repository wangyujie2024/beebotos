//! Task Monitor HTTP Handlers
//!
//! REST API handlers for monitoring kernel task execution and fault detection.
//!
//! 🔒 P0 FIX: Kernel task fault awareness HTTP API.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use gateway::error::GatewayError;
use gateway::middleware::{require_any_role, AuthUser};
use serde_json::json;

use crate::handlers::common::check_ownership;
use crate::AppState;

/// Get task monitor statistics
pub async fn get_task_monitor_stats(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let stats = if let Some(task_monitor) = &state.task_monitor {
        let stats = task_monitor.get_statistics().await;
        json!({
            "active_monitors": stats.active_monitors,
            "monitored_agents": stats.monitored_agents,
        })
    } else {
        json!({
            "message": "Task monitor not available",
            "active_monitors": 0,
            "monitored_agents": [],
        })
    };

    Ok(Json(stats))
}

/// Get agent task status
pub async fn get_agent_task_status(
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

    let task_info = if let Some(task_monitor) = &state.task_monitor {
        let has_active = task_monitor.has_active_task(&id).await;
        let info = task_monitor.get_task_info(&id).await;

        json!({
            "has_active_task": has_active,
            "task_id": info.map(|h| h.task_id.to_string()),
        })
    } else {
        json!({
            "has_active_task": false,
            "message": "Task monitor not available",
        })
    };

    Ok(Json(json!({
        "agent_id": id,
        "task_info": task_info,
    })))
}

/// List all monitored agents
pub async fn list_monitored_agents(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let agents = if let Some(task_monitor) = &state.task_monitor {
        task_monitor.list_monitored_agents().await
    } else {
        Vec::new()
    };

    Ok(Json(json!({
        "agents": agents,
        "count": agents.len(),
    })))
}

/// Cancel task monitoring for an agent
pub async fn cancel_task_monitoring(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    if let Some(task_monitor) = &state.task_monitor {
        task_monitor.cancel_monitoring(&id).await?;
    }

    Ok(Json(json!({
        "agent_id": id,
        "message": "Task monitoring cancelled",
    })))
}

/// Get fault detection status
pub async fn get_fault_detection_status(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let status = if let Some(task_monitor) = &state.task_monitor {
        let stats = task_monitor.get_statistics().await;

        json!({
            "enabled": true,
            "active_monitors": stats.active_monitors,
            "monitored_agents": stats.monitored_agents.len(),
            "fault_detection": {
                "task_completion_tracking": true,
                "task_failure_detection": true,
                "timeout_detection": true,
                "auto_state_update": true,
            },
        })
    } else {
        json!({
            "enabled": false,
            "message": "Task monitor not initialized",
        })
    };

    Ok(Json(status))
}
