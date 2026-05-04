//! Agent HTTP Handlers
//!
//! Production-ready agent management handlers.
//!
//! **Architecture Note:** These handlers delegate all business logic to
//! `AgentService`, maintaining proper layering:
//!
//! ```
//! handlers (HTTP layer) → AgentService (business layer) → kernel (infrastructure)
//! ```
//!
//! The handlers are responsible for:
//! - HTTP request/response handling
//! - Input validation
//! - Authentication/authorization checks
//! - Response formatting
//!
//! 🔒 P0 FIX: Unified state management - all agent state is now managed by
//! `AgentStateManager` (via `state_manager` handle), removed duplicate
//! in-memory HashMap in AppState.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
// Use gateway-lib for infrastructure
use gateway::{
    error::GatewayError,
    middleware::{require_any_role, AuthUser},
};
use serde::Deserialize;
use serde_json::json;

// Business logic imports
use crate::handlers::common::check_ownership;
use crate::models::{AgentResponse, CreateAgentRequest, PaginatedResponse, PaginationParams};
use crate::AppState;

/// List all agents with pagination
pub async fn list_agents(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    axum::extract::Query(params): axum::extract::Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<AgentResponse>>, GatewayError> {
    // Check permission
    require_any_role(&user, &["user", "admin"])?;

    // Use service layer
    let (agents, total) = if user.is_admin() {
        state
            .agent_service
            .list_agents_with_count(None, params.per_page, params.offset())
            .await?
    } else {
        state
            .agent_service
            .list_agents_with_count(Some(&user.user_id), params.per_page, params.offset())
            .await?
    };

    // Convert to response models
    let responses: Vec<AgentResponse> = agents.into_iter().map(AgentResponse::from).collect();

    Ok(Json(PaginatedResponse::new(
        responses,
        total,
        params.page,
        params.per_page,
    )))
}

/// Create new agent
///
/// Delegates to AgentService for complete lifecycle management:
/// - Database persistence
/// - Capability set creation
/// - Kernel sandbox spawning
/// - State manager registration (unified state)
pub async fn create_agent(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<CreateAgentRequest>,
) -> Result<impl IntoResponse, GatewayError> {
    // Check permission
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

    if req.name.len() > 255 {
        return Err(GatewayError::validation(vec![
            gateway::error::ValidationError {
                field: "name".to_string(),
                message: "Name must be less than 255 characters".to_string(),
                code: "max_length".to_string(),
            },
        ]));
    }

    // Delegate to AgentService for complete lifecycle management
    // This internally registers with state_manager via AgentRuntimeManager
    // 🔒 P0 FIX: Pass task_monitor for kernel fault awareness
    let (agent, _kernel_info) = state
        .agent_service
        .create_and_spawn(req, &user.user_id, state.task_monitor.as_ref().map(|v| &**v))
        .await?;

    tracing::info!(
        "Agent {} created by user {} (kernel task: {})",
        agent.id,
        user.user_id,
        agent.id
    );

    let response = AgentResponse::from(agent);

    Ok((
        StatusCode::CREATED,
        Json(response),
    ))
}

/// Get agent by ID
pub async fn get_agent(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<AgentResponse>, GatewayError> {
    let agent_id = uuid::Uuid::parse_str(&id)
        .map_err(|_| GatewayError::bad_request("Invalid agent ID format"))?;

    // Fetch from database using service
    let agent = state
        .agent_service
        .get_agent(&id)
        .await?
        .ok_or_else(|| GatewayError::not_found("Agent", &id))?;

    // Check ownership
    check_ownership(&user, &agent)?;

    // Get status history
    let _history: Vec<crate::models::AgentStatusHistory> = sqlx::query_as(
        "SELECT * FROM agent_status_history WHERE agent_id = $1 ORDER BY created_at DESC LIMIT 10",
    )
    .bind(agent_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error fetching agent status history: {}", e);
        GatewayError::internal("Failed to retrieve agent history".to_string())
    })?;

    // 🔒 P0 FIX: Get runtime state from unified state manager instead of local
    // cache
    let _runtime_state = state
        .state_manager
        .get_record(&id)
        .await
        .ok()
        .map(|record| {
            json!({
                "state": format!("{:?}", record.state),
                "registered_at": record.registered_at,
                "state_changed_at": record.state_changed_at,
                "metadata": record.metadata,
            })
        });

    // Get kernel task ID if available
    let _kernel_task_id = state
        .agent_runtime_manager
        .get_kernel_task_id(&id)
        .await
        .ok()
        .flatten();

    let response = AgentResponse::from(agent);

    Ok(Json(response))
}

/// Update agent
///
/// Delegates to AgentService for database update.
pub async fn update_agent(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<crate::models::UpdateAgentRequest>,
) -> Result<Json<AgentResponse>, GatewayError> {
    let agent = state
        .agent_service
        .get_agent(&id)
        .await?
        .ok_or_else(|| GatewayError::not_found("Agent", &id))?;

    check_ownership(&user, &agent)?;

    let updated = state.agent_service.update_agent(&id, &req).await?;
    let response = AgentResponse::from(updated);

    Ok(Json(response))
}

/// Delete agent
///
/// Delegates to AgentService for proper cleanup:
/// - Kernel task cancellation
/// - Runtime unregistration (state_manager)
/// - Database deletion
pub async fn delete_agent(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, GatewayError> {
    // Check ownership before deleting
    let agent = state
        .agent_service
        .get_agent(&id)
        .await?
        .ok_or_else(|| GatewayError::not_found("Agent", &id))?;

    check_ownership(&user, &agent)?;

    // 🔒 P0 FIX: Get kernel task ID from runtime manager (unified state)
    let kernel_task_id = state
        .agent_runtime_manager
        .get_kernel_task_id(&id)
        .await
        .ok()
        .flatten();

    // Convert to kernel info if available
    let kernel_info = kernel_task_id.map(|task_id| {
        use beebotos_kernel::capabilities::CapabilitySet;
        use beebotos_kernel::TaskId;

        use crate::services::agent_service::AgentKernelInfo;
        AgentKernelInfo {
            task_id: TaskId::new(task_id),
            capability_set: CapabilitySet::standard(),
        }
    });

    // Delegate to AgentService for cleanup
    state
        .agent_service
        .delete_agent(&id, kernel_info.as_ref())
        .await?;

    tracing::info!("Agent {} deleted by user {}", id, user.user_id);

    Ok(StatusCode::NO_CONTENT)
}

/// Start agent
///
/// Re-spawns the kernel task and registers the runtime.
/// State updates are managed by AgentService and unified state manager.
pub async fn start_agent(
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

    // Re-spawn kernel task and register runtime
    // This internally updates the state_manager via AgentRuntimeManager
    // 🔒 P0 FIX: Pass task_monitor for kernel fault awareness
    let _kernel_info = state
        .agent_service
        .start_agent(&id, state.task_monitor.as_ref().map(|v| &**v))
        .await?;

    tracing::info!("Agent {} started by user {}", id, user.user_id);

    Ok(Json(json!({
        "id": id,
        "status": "running",
        "message": "Agent started successfully",
    })))
}

/// Stop agent
///
/// Delegates to AgentService for proper kernel cleanup:
/// - Kernel task cancellation
/// - Database status update
/// - State manager state transition
pub async fn stop_agent(
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

    // 🔒 P0 FIX: Get kernel task ID from runtime manager (unified state)
    let kernel_task_id = state
        .agent_runtime_manager
        .get_kernel_task_id(&id)
        .await
        .ok()
        .flatten();

    // Convert to kernel info if available
    let kernel_info = kernel_task_id.map(|task_id| {
        use beebotos_kernel::capabilities::CapabilitySet;
        use beebotos_kernel::TaskId;

        use crate::services::agent_service::AgentKernelInfo;
        AgentKernelInfo {
            task_id: TaskId::new(task_id),
            capability_set: CapabilitySet::standard(),
        }
    });

    // Delegate to AgentService for proper cleanup
    state
        .agent_service
        .stop_agent(&id, kernel_info.as_ref())
        .await?;

    tracing::info!("Agent {} stopped by user {}", id, user.user_id);

    Ok(Json(json!({
        "id": id,
        "status": "stopped",
        "message": "Agent stopped successfully via kernel",
    })))
}

/// Request body for executing a task on an agent
#[derive(Debug, Deserialize)]
pub struct ExecuteTaskRequest {
    pub task_type: String,
    pub input: String,
    #[serde(default)]
    pub parameters: std::collections::HashMap<String, String>,
}

/// Execute a task on an agent via the `beebotos_agents` runtime.
///
/// This endpoint bridges the Gateway HTTP API with the actual Agent runtime,
/// enabling LLM chat, skill execution, MCP tool calls, and chain transactions.
pub async fn execute_agent_task(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<ExecuteTaskRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    // Check ownership
    let agent = state
        .agent_service
        .get_agent(&id)
        .await?
        .ok_or_else(|| GatewayError::not_found("Agent", &id))?;

    check_ownership(&user, &agent)?;

    // Ensure agent runtime is registered before executing
    if !state.agent_runtime_manager.is_registered(&id).await {
        state
            .agent_runtime_manager
            .register_agent(&id, &agent)
            .await
            .map_err(|e| {
                GatewayError::internal(format!("Failed to register agent runtime: {}", e))
            })?;
    }

    let task = beebotos_agents::Task {
        id: uuid::Uuid::new_v4().to_string(),
        task_type: beebotos_agents::TaskType::parse(&req.task_type),
        input: req.input,
        parameters: req.parameters,
    };

    let result = state
        .agent_runtime_manager
        .execute_task(&id, task)
        .await
        .map_err(|e| GatewayError::internal(format!("Task execution failed: {}", e)))?;

    Ok(Json(json!({
        "task_id": result.task_id,
        "success": result.success,
        "output": result.output,
        "artifacts": result.artifacts,
        "execution_time_ms": result.execution_time_ms,
    })))
}

/// Get available capability types
///
/// Returns the list of supported capability types and their schemas
pub async fn list_capability_types() -> Json<serde_json::Value> {
    Json(json!({
        "capabilities": [
            {
                "type": "file_read",
                "description": "Read access to specific file system paths",
                "config_schema": {
                    "paths": ["string"]
                },
                "example": {
                    "type": "file_read",
                    "config": {
                        "paths": ["/tmp", "/data"]
                    }
                }
            },
            {
                "type": "file_write",
                "description": "Write access to specific file system paths",
                "config_schema": {
                    "paths": ["string"]
                },
                "example": {
                    "type": "file_write",
                    "config": {
                        "paths": ["/output"]
                    }
                }
            },
            {
                "type": "network_http",
                "description": "HTTP network access to specific hosts",
                "config_schema": {
                    "hosts": ["string"],
                    "methods": ["GET", "POST", "PUT", "DELETE", "PATCH"]
                },
                "example": {
                    "type": "network_http",
                    "config": {
                        "hosts": ["api.example.com"],
                        "methods": ["GET", "POST"]
                    }
                }
            },
            {
                "type": "network_tcp",
                "description": "TCP network access to specific ports",
                "config_schema": {
                    "ports": ["number"],
                    "hosts": ["string"]
                },
                "example": {
                    "type": "network_tcp",
                    "config": {
                        "ports": [5432, 6379],
                        "hosts": ["localhost"]
                    }
                }
            },
            {
                "type": "database",
                "description": "Database table access",
                "config_schema": {
                    "tables": ["string"],
                    "operations": ["select", "insert", "update", "delete"]
                },
                "example": {
                    "type": "database",
                    "config": {
                        "tables": ["users", "orders"],
                        "operations": ["select", "insert"]
                    }
                }
            },
            {
                "type": "llm",
                "description": "LLM/AI model access",
                "config_schema": {
                    "providers": ["string"],
                    "max_tokens_per_request": "number"
                },
                "example": {
                    "type": "llm",
                    "config": {
                        "providers": ["openai", "anthropic"],
                        "max_tokens_per_request": 4000
                    }
                }
            },
            {
                "type": "wallet",
                "description": "Blockchain wallet access",
                "config_schema": {
                    "chain_ids": ["number"],
                    "max_transaction_value": "string"
                },
                "example": {
                    "type": "wallet",
                    "config": {
                        "chain_ids": [1, 137],
                        "max_transaction_value": "1.0"
                    }
                }
            }
        ]
    }))
}

/// Validate capabilities
///
/// Validates a list of capabilities and returns normalized versions
pub async fn validate_capabilities(
    Json(capabilities): Json<Vec<crate::capability::AgentCapability>>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    use crate::capability::AgentCapability;

    let mut validated = Vec::new();
    let mut errors = Vec::new();

    for (idx, cap) in capabilities.iter().enumerate() {
        // Validate the capability
        let validation = match cap {
            AgentCapability::FileRead { paths } if paths.is_empty() => {
                Err("FileRead paths cannot be empty".to_string())
            }
            AgentCapability::FileWrite { paths } if paths.is_empty() => {
                Err("FileWrite paths cannot be empty".to_string())
            }
            AgentCapability::NetworkHttp { hosts, .. } if hosts.is_empty() => {
                Err("NetworkHttp hosts cannot be empty".to_string())
            }
            AgentCapability::NetworkTcp { ports, .. } if ports.is_empty() => {
                Err("NetworkTcp ports cannot be empty".to_string())
            }
            AgentCapability::Database { tables, .. } if tables.is_empty() => {
                Err("Database tables cannot be empty".to_string())
            }
            _ => Ok(()),
        };

        match validation {
            Ok(()) => {
                validated.push(json!({
                    "index": idx,
                    "capability": cap,
                    "description": cap.description(),
                    "compact_string": cap.to_compact_string(),
                    "valid": true,
                }));
            }
            Err(e) => {
                errors.push(json!({
                    "index": idx,
                    "capability": cap,
                    "error": e,
                }));
            }
        }
    }

    let is_valid = errors.is_empty();

    Ok(Json(json!({
        "valid": is_valid,
        "validated": validated,
        "errors": errors,
        "count": validated.len(),
    })))
}
