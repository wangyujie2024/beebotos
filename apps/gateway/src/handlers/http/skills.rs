//! Skills HTTP Handlers
//!
//! Handles skill installation, management, and execution through Gateway.
//! Acts as a proxy to ClawHub/BeeHub for skill downloads.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::clients::{BeeHubClient, ClawHubClient, HubType, SkillMetadata};
use crate::error::GatewayError;
use crate::AppState;

/// Install skill request
#[derive(Debug, Deserialize)]
pub struct InstallSkillRequest {
    /// Skill source (ID or name)
    pub source: String,
    /// Target agent ID (optional)
    pub agent_id: Option<String>,
    /// Version constraint (optional)
    pub version: Option<String>,
    /// Hub to use (default: clawhub)
    pub hub: Option<String>,
}

/// Install skill response
#[derive(Debug, Serialize)]
pub struct InstallSkillResponse {
    pub success: bool,
    pub skill_id: String,
    pub name: String,
    pub version: String,
    pub message: String,
    pub installed_path: String,
}

/// List skills query parameters
#[derive(Debug, Deserialize)]
pub struct ListSkillsQuery {
    /// Filter by category
    pub category: Option<String>,
    /// Search query
    pub search: Option<String>,
    /// Hub to query (default: local)
    pub hub: Option<String>,
}

/// Skill info response
#[derive(Debug, Serialize)]
pub struct SkillInfoResponse {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub license: String,
    pub installed: bool,
    pub capabilities: Vec<String>,
    pub tags: Vec<String>,
    pub downloads: u64,
    pub rating: f32,
}

/// Execute skill request
#[derive(Debug, Deserialize)]
pub struct ExecuteSkillRequest {
    /// Input parameters
    pub input: serde_json::Value,
    /// Target agent ID
    pub agent_id: Option<String>,
}

/// Execute skill response
#[derive(Debug, Serialize)]
pub struct ExecuteSkillResponse {
    pub success: bool,
    pub output: String,
    pub execution_time_ms: u64,
}

// ---------------------------------------------------------------------------
// Instance-based execution model
// ---------------------------------------------------------------------------

/// Create instance request
#[derive(Debug, Deserialize)]
pub struct CreateInstanceRequest {
    pub skill_id: String,
    pub agent_id: String,
    #[serde(default)]
    pub config: std::collections::HashMap<String, String>,
}

/// Instance response
#[derive(Debug, Serialize)]
pub struct InstanceResponse {
    pub instance_id: String,
    pub skill_id: String,
    pub agent_id: String,
    pub status: String,
    pub config: std::collections::HashMap<String, String>,
    pub started_at: i64,
    pub last_active: i64,
    pub usage: UsageStatsResponse,
}

/// Usage stats response
#[derive(Debug, Serialize)]
pub struct UsageStatsResponse {
    pub total_calls: u64,
    pub successful_calls: u64,
    pub failed_calls: u64,
    pub avg_latency_ms: f64,
}

/// Update instance request
#[derive(Debug, Deserialize)]
pub struct UpdateInstanceRequest {
    #[serde(default)]
    pub config_updates: std::collections::HashMap<String, String>,
    pub status: Option<String>,
}

/// Execute instance request
#[derive(Debug, Deserialize)]
pub struct ExecuteInstanceRequest {
    pub function_name: Option<String>,
    #[serde(default)]
    pub parameters: std::collections::HashMap<String, serde_json::Value>,
    pub timeout_ms: Option<u32>,
    pub input: Option<serde_json::Value>,
}

/// Install a skill from ClawHub or BeeHub
pub async fn install_skill(
    State(state): State<Arc<AppState>>,
    Json(req): Json<InstallSkillRequest>,
) -> Result<Json<InstallSkillResponse>, GatewayError> {
    info!("Installing skill: {} from hub: {:?}", req.source, req.hub);

    // Determine which hub to use
    let hub_type = req
        .hub
        .as_deref()
        .and_then(|h| h.parse::<HubType>().ok())
        .unwrap_or_default();

    // Fetch skill metadata from hub
    let metadata = match hub_type {
        HubType::ClawHub => {
            let client = ClawHubClient::new().map_err(|e| GatewayError::Internal {
                message: format!("Failed to create ClawHub client: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            })?;

            client
                .get_skill(&req.source)
                .await
                .map_err(|e| GatewayError::Internal {
                    message: format!("Failed to get skill from ClawHub: {}", e),
                    correlation_id: uuid::Uuid::new_v4().to_string(),
                })?
        }
        HubType::BeeHub => {
            let client = BeeHubClient::new().map_err(|e| GatewayError::Internal {
                message: format!("Failed to create BeeHub client: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            })?;

            client
                .get_skill(&req.source)
                .await
                .map_err(|e| GatewayError::Internal {
                    message: format!("Failed to get skill from BeeHub: {}", e),
                    correlation_id: uuid::Uuid::new_v4().to_string(),
                })?
        }
    };

    info!(
        "Found skill: {} v{} from {}",
        metadata.name, metadata.version, metadata.author
    );

    // Check if already installed
    let skill_dir = get_skill_install_path(&metadata.id);
    if skill_dir.exists() {
        warn!(
            "Skill {} is already installed at {:?}",
            metadata.id, skill_dir
        );
        return Ok(Json(InstallSkillResponse {
            success: true,
            skill_id: metadata.id,
            name: metadata.name,
            version: metadata.version,
            message: "Skill is already installed".to_string(),
            installed_path: skill_dir.to_string_lossy().to_string(),
        }));
    }

    // Download skill package
    info!("Downloading skill package for {}", metadata.id);
    let package_bytes = match hub_type {
        HubType::ClawHub => {
            let client = ClawHubClient::new().map_err(|e| GatewayError::Internal {
                message: format!("Failed to create ClawHub client: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            })?;

            client
                .download_skill(&req.source, req.version.as_deref())
                .await
                .map_err(|e| GatewayError::Internal {
                    message: format!("Failed to download skill: {}", e),
                    correlation_id: uuid::Uuid::new_v4().to_string(),
                })?
        }
        HubType::BeeHub => {
            let client = BeeHubClient::new().map_err(|e| GatewayError::Internal {
                message: format!("Failed to create BeeHub client: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            })?;

            client
                .download_skill(&req.source, req.version.as_deref())
                .await
                .map_err(|e| GatewayError::Internal {
                    message: format!("Failed to download skill: {}", e),
                    correlation_id: uuid::Uuid::new_v4().to_string(),
                })?
        }
    };

    // Extract and install skill
    install_skill_package(&metadata, &package_bytes)
        .await
        .map_err(|e| GatewayError::Internal {
            message: format!("Failed to install skill package: {}", e),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

    // Load and register to SkillRegistry if available
    if let Some(ref registry) = state.skill_registry {
        let mut loader = beebotos_agents::skills::SkillLoader::new();
        loader.add_path(get_skills_base_dir());
        match loader.load_skill(&metadata.id).await {
            Ok(skill) => {
                registry
                    .register(
                        skill,
                        metadata
                            .tags
                            .first()
                            .map(|s| s.as_str())
                            .unwrap_or("general"),
                        metadata.tags.clone(),
                    )
                    .await;
                info!("Registered skill {} to registry", metadata.id);
            }
            Err(e) => {
                warn!("Failed to load skill into registry: {}", e);
            }
        }
    }

    info!(
        "Successfully installed skill {} to {:?}",
        metadata.id, skill_dir
    );

    Ok(Json(InstallSkillResponse {
        success: true,
        skill_id: metadata.id.clone(),
        name: metadata.name,
        version: metadata.version,
        message: "Skill installed successfully".to_string(),
        installed_path: skill_dir.to_string_lossy().to_string(),
    }))
}

/// List installed skills or search from hub
pub async fn list_skills(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListSkillsQuery>,
) -> Result<Json<Vec<SkillInfoResponse>>, GatewayError> {
    let hub_type = query.hub.as_deref().and_then(|h| h.parse::<HubType>().ok());

    // If hub is specified, search from remote hub
    if let Some(hub) = hub_type {
        let skills =
            match hub {
                HubType::ClawHub => {
                    let client = ClawHubClient::new().map_err(|e| GatewayError::Internal {
                        message: format!("Failed to create ClawHub client: {}", e),
                        correlation_id: uuid::Uuid::new_v4().to_string(),
                    })?;

                    let search_query = query.search.as_deref().unwrap_or("");
                    client.search_skills(search_query).await.map_err(|e| {
                        GatewayError::Internal {
                            message: format!("Failed to search skills: {}", e),
                            correlation_id: uuid::Uuid::new_v4().to_string(),
                        }
                    })?
                }
                HubType::BeeHub => {
                    let client = BeeHubClient::new().map_err(|e| GatewayError::Internal {
                        message: format!("Failed to create BeeHub client: {}", e),
                        correlation_id: uuid::Uuid::new_v4().to_string(),
                    })?;

                    let search_query = query.search.as_deref().unwrap_or("");
                    client.search_skills(search_query).await.map_err(|e| {
                        GatewayError::Internal {
                            message: format!("Failed to list skills: {}", e),
                            correlation_id: uuid::Uuid::new_v4().to_string(),
                        }
                    })?
                }
            };

        let responses: Vec<SkillInfoResponse> = skills
            .into_iter()
            .map(|s| SkillInfoResponse {
                id: s.id.clone(),
                name: s.name,
                version: s.version,
                description: s.description,
                author: s.author,
                license: s.license,
                installed: is_skill_installed(&s.id),
                capabilities: s.capabilities,
                tags: s.tags,
                downloads: s.downloads,
                rating: s.rating,
            })
            .collect();

        return Ok(Json(responses));
    }

    // Otherwise, list locally installed skills
    let skills = list_installed_skills()
        .await
        .map_err(|e| GatewayError::Internal {
            message: format!("Failed to list installed skills: {}", e),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

    Ok(Json(skills))
}

/// Get skill details
pub async fn get_skill(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<SkillInfoResponse>, GatewayError> {
    let skill = get_skill_info(&id)
        .await
        .map_err(|e| GatewayError::NotFound {
            resource: format!("Skill: {}", id),
            id: id.clone(),
        })?;

    Ok(Json(skill))
}

/// Uninstall a skill
pub async fn uninstall_skill(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    info!("Uninstalling skill: {}", id);

    let skill_dir = get_skill_install_path(&id);
    if !skill_dir.exists() {
        return Err(GatewayError::NotFound {
            resource: "Skill".to_string(),
            id: id.clone(),
        });
    }

    tokio::fs::remove_dir_all(&skill_dir)
        .await
        .map_err(|e| GatewayError::Internal {
            message: format!("Failed to uninstall skill: {}", e),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

    info!("Successfully uninstalled skill: {}", id);

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Skill {} uninstalled", id),
    })))
}

/// Execute a skill
pub async fn execute_skill(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<ExecuteSkillRequest>,
) -> Result<Json<ExecuteSkillResponse>, GatewayError> {
    info!("Executing skill: {} with input: {:?}", id, req.input);

    // Check if skill is installed
    let skill_dir = get_skill_install_path(&id);
    if !skill_dir.exists() {
        return Err(GatewayError::NotFound {
            resource: "Skill".to_string(),
            id: id.clone(),
        });
    }

    // Load skill
    let mut loader = beebotos_agents::skills::SkillLoader::new();
    loader.add_path(get_skills_base_dir());
    let skill = loader
        .load_skill(&id)
        .await
        .map_err(|e| GatewayError::Internal {
            message: format!("Failed to load skill: {}", e),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

    // Get cached executor or create fallback
    let executor = match state.skill_executor.as_ref() {
        Some(exec) => exec.clone(),
        None => Arc::new(beebotos_agents::skills::SkillExecutor::new().map_err(|e| {
            GatewayError::Internal {
                message: format!("Failed to create skill executor: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            }
        })?),
    };

    // Build context
    let input = match &req.input {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let context = beebotos_agents::skills::SkillContext {
        input,
        parameters: std::collections::HashMap::new(),
    };

    // Execute
    let result = executor
        .execute(&skill, context)
        .await
        .map_err(|e| GatewayError::Internal {
            message: format!("Skill execution failed: {}", e),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

    // Record usage if registry is available
    if let Some(ref registry) = state.skill_registry {
        registry.record_usage(&id).await;
    }

    Ok(Json(ExecuteSkillResponse {
        success: result.success,
        output: result.output,
        execution_time_ms: result.execution_time_ms,
    }))
}

/// Create a skill instance
pub async fn create_instance(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateInstanceRequest>,
) -> Result<Json<InstanceResponse>, GatewayError> {
    let manager = state
        .skill_instance_manager
        .as_ref()
        .ok_or_else(|| GatewayError::Internal {
            message: "Instance manager not available".to_string(),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

    let registry = state
        .skill_registry
        .as_ref()
        .ok_or_else(|| GatewayError::Internal {
            message: "Skill registry not available".to_string(),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

    // Verify skill exists
    if registry.get(&req.skill_id).await.is_none() {
        return Err(GatewayError::NotFound {
            resource: "Skill".to_string(),
            id: req.skill_id.clone(),
        });
    }

    let instance_id = manager
        .create(req.skill_id, req.agent_id, req.config)
        .await
        .map_err(|e| GatewayError::Internal {
            message: format!("Instance limit reached: {}", e),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

    // Auto-transition to Running
    manager
        .update_status(
            &instance_id,
            beebotos_agents::skills::InstanceStatus::Running,
        )
        .await
        .map_err(|e| GatewayError::Internal {
            message: format!("Failed to start instance: {}", e),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

    let instance = manager
        .get(&instance_id)
        .await
        .ok_or_else(|| GatewayError::Internal {
            message: "Instance disappeared after creation".to_string(),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

    Ok(Json(map_instance_to_response(&instance)))
}

/// List instances
pub async fn list_instances(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListInstancesQuery>,
) -> Result<Json<Vec<InstanceResponse>>, GatewayError> {
    let manager = state
        .skill_instance_manager
        .as_ref()
        .ok_or_else(|| GatewayError::Internal {
            message: "Instance manager not available".to_string(),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

    let status_filter = query
        .status
        .as_deref()
        .and_then(|s| match s.to_lowercase().as_str() {
            "pending" => Some(beebotos_agents::skills::InstanceStatus::Pending),
            "running" => Some(beebotos_agents::skills::InstanceStatus::Running),
            "paused" => Some(beebotos_agents::skills::InstanceStatus::Paused),
            "stopped" => Some(beebotos_agents::skills::InstanceStatus::Stopped),
            "error" => Some(beebotos_agents::skills::InstanceStatus::Error),
            _ => None,
        });

    let filter = beebotos_agents::skills::InstanceFilter {
        agent_id: query.agent_id,
        skill_id: query.skill_id,
        status: status_filter,
        page: 0,
        page_size: 0,
    };

    let instances = manager.list(&filter).await;
    let responses = instances.iter().map(map_instance_to_response).collect();
    Ok(Json(responses))
}

/// List instances query parameters
#[derive(Debug, Deserialize)]
pub struct ListInstancesQuery {
    pub agent_id: Option<String>,
    pub skill_id: Option<String>,
    pub status: Option<String>,
}

/// Get instance details
pub async fn get_instance(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<InstanceResponse>, GatewayError> {
    let manager = state
        .skill_instance_manager
        .as_ref()
        .ok_or_else(|| GatewayError::Internal {
            message: "Instance manager not available".to_string(),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

    let instance = manager
        .get(&id)
        .await
        .ok_or_else(|| GatewayError::NotFound {
            resource: "Instance".to_string(),
            id: id.clone(),
        })?;

    Ok(Json(map_instance_to_response(&instance)))
}

/// Update instance
pub async fn update_instance(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateInstanceRequest>,
) -> Result<Json<InstanceResponse>, GatewayError> {
    let manager = state
        .skill_instance_manager
        .as_ref()
        .ok_or_else(|| GatewayError::Internal {
            message: "Instance manager not available".to_string(),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

    // Update config
    if !req.config_updates.is_empty() {
        manager
            .update_config(&id, req.config_updates)
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to update config: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            })?;
    }

    // Update status
    if let Some(status_str) = req.status {
        let new_status = match status_str.to_lowercase().as_str() {
            "pending" => beebotos_agents::skills::InstanceStatus::Pending,
            "running" => beebotos_agents::skills::InstanceStatus::Running,
            "paused" => beebotos_agents::skills::InstanceStatus::Paused,
            "stopped" => beebotos_agents::skills::InstanceStatus::Stopped,
            "error" => beebotos_agents::skills::InstanceStatus::Error,
            _ => {
                return Err(GatewayError::Internal {
                    message: format!("Invalid status: {}", status_str),
                    correlation_id: uuid::Uuid::new_v4().to_string(),
                })
            }
        };
        manager
            .update_status(&id, new_status)
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to update status: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            })?;
    }

    let instance = manager
        .get(&id)
        .await
        .ok_or_else(|| GatewayError::NotFound {
            resource: "Instance".to_string(),
            id: id.clone(),
        })?;

    Ok(Json(map_instance_to_response(&instance)))
}

/// Delete instance
pub async fn delete_instance(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let manager = state
        .skill_instance_manager
        .as_ref()
        .ok_or_else(|| GatewayError::Internal {
            message: "Instance manager not available".to_string(),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

    manager
        .delete(&id)
        .await
        .map_err(|e| GatewayError::Internal {
            message: format!("Failed to delete instance: {}", e),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Instance {} deleted", id),
    })))
}

/// Execute a skill through an instance
pub async fn execute_instance(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<ExecuteInstanceRequest>,
) -> Result<Json<ExecuteSkillResponse>, GatewayError> {
    info!(
        "Executing instance: {} function: {:?}",
        id, req.function_name
    );

    let manager = state
        .skill_instance_manager
        .as_ref()
        .ok_or_else(|| GatewayError::Internal {
            message: "Instance manager not available".to_string(),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

    let instance = manager
        .get(&id)
        .await
        .ok_or_else(|| GatewayError::NotFound {
            resource: "Instance".to_string(),
            id: id.clone(),
        })?;

    if instance.status != beebotos_agents::skills::InstanceStatus::Running {
        return Err(GatewayError::Internal {
            message: format!("Instance {} is not running", id),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        });
    }

    // Load skill
    let mut loader = beebotos_agents::skills::SkillLoader::new();
    loader.add_path(get_skills_base_dir());
    let skill =
        loader
            .load_skill(&instance.skill_id)
            .await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to load skill: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            })?;

    // Get executor
    let executor = match state.skill_executor.as_ref() {
        Some(exec) => exec.clone(),
        None => Arc::new(beebotos_agents::skills::SkillExecutor::new().map_err(|e| {
            GatewayError::Internal {
                message: format!("Failed to create skill executor: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            }
        })?),
    };

    // Convert parameters: map<string, json_value> -> map<string, bytes>
    let parameters: std::collections::HashMap<String, Vec<u8>> = req
        .parameters
        .into_iter()
        .map(|(k, v)| (k, v.to_string().into_bytes()))
        .collect();

    let input = req
        .input
        .map(|v| match v {
            serde_json::Value::String(s) => s,
            other => other.to_string(),
        })
        .unwrap_or_default();

    let context = beebotos_agents::skills::SkillContext {
        input,
        parameters: instance.config.clone(),
    };

    let result = executor
        .execute_function(
            &skill,
            req.function_name.as_deref(),
            parameters,
            req.timeout_ms,
            context,
        )
        .await
        .map_err(|e| GatewayError::Internal {
            message: format!("Execution failed: {}", e),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

    // Record usage
    let _ = manager
        .record_execution(&id, result.success, result.execution_time_ms as f64)
        .await;

    Ok(Json(ExecuteSkillResponse {
        success: result.success,
        output: result.output,
        execution_time_ms: result.execution_time_ms,
    }))
}

fn map_instance_to_response(instance: &beebotos_agents::skills::SkillInstance) -> InstanceResponse {
    InstanceResponse {
        instance_id: instance.instance_id.clone(),
        skill_id: instance.skill_id.clone(),
        agent_id: instance.agent_id.clone(),
        status: format!("{:?}", instance.status).to_lowercase(),
        config: instance.config.clone(),
        started_at: instance.started_at as i64,
        last_active: instance.last_active as i64,
        usage: UsageStatsResponse {
            total_calls: instance.usage.total_calls,
            successful_calls: instance.usage.successful_calls,
            failed_calls: instance.usage.failed_calls,
            avg_latency_ms: instance.usage.avg_latency_ms,
        },
    }
}

/// Check hub health
pub async fn hub_health() -> Result<Json<serde_json::Value>, GatewayError> {
    let clawhub_client = ClawHubClient::new();
    let beehub_client = BeeHubClient::new();

    let clawhub_healthy = if let Ok(client) = clawhub_client {
        client.health_check().await.unwrap_or(false)
    } else {
        false
    };

    let beehub_healthy = if let Ok(client) = beehub_client {
        client.health_check().await.unwrap_or(false)
    } else {
        false
    };

    Ok(Json(serde_json::json!({
        "clawhub": {
            "status": if clawhub_healthy { "healthy" } else { "unhealthy" },
            "url": std::env::var("CLAWHUB_URL").unwrap_or_else(|_| "https://hub.claw.dev/v1".to_string()),
        },
        "beehub": {
            "status": if beehub_healthy { "healthy" } else { "unhealthy" },
            "url": std::env::var("BEEHUB_URL").unwrap_or_else(|_| "http://localhost:3001".to_string()),
        },
    })))
}

// Helper functions

/// Get skill installation directory
pub fn get_skill_install_path(skill_id: &str) -> std::path::PathBuf {
    get_skills_base_dir().join(skill_id)
}

/// Get base skills directory
pub fn get_skills_base_dir() -> std::path::PathBuf {
    std::env::var("BEEBOTOS_SKILLS_DIR")
        .map(|d| std::path::PathBuf::from(d))
        .unwrap_or_else(|_| std::path::PathBuf::from("data/skills"))
}

/// Check if skill is installed
fn is_skill_installed(skill_id: &str) -> bool {
    get_skill_install_path(skill_id).exists()
}

/// Install skill package to disk
async fn install_skill_package(
    metadata: &SkillMetadata,
    package_bytes: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let skill_dir = get_skill_install_path(&metadata.id);

    // Create directory
    tokio::fs::create_dir_all(&skill_dir).await?;

    // Write package
    let package_path = skill_dir.join("package.zip");
    tokio::fs::write(&package_path, package_bytes).await?;

    // Extract archive in blocking task
    let skill_dir_clone = skill_dir.clone();
    let package_path_clone = package_path.clone();
    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&package_path_clone)
            .map_err(|e| format!("Failed to open package: {}", e))?;
        let mut archive =
            zip::ZipArchive::new(file).map_err(|e| format!("Failed to read zip archive: {}", e))?;
        archive
            .extract(&skill_dir_clone)
            .map_err(|e| format!("Failed to extract archive: {}", e))?;
        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Blocking task failed: {}", e))?
    .map_err(|e| format!("Extraction failed: {}", e))?;

    // Create skill.yaml manifest if not present in archive
    let manifest_path = skill_dir.join("skill.yaml");
    if !manifest_path.exists() {
        let manifest = serde_yaml::to_string(&serde_json::json!({
            "id": metadata.id,
            "name": metadata.name,
            "version": metadata.version,
            "description": metadata.description,
            "author": metadata.author,
            "license": metadata.license,
            "capabilities": metadata.capabilities,
            "entry_point": "skill.wasm",
        }))?;

        tokio::fs::write(&manifest_path, manifest).await?;
    }

    // Validate WASM if present
    let wasm_path = skill_dir.join("skill.wasm");
    if wasm_path.exists() {
        let wasm_bytes = tokio::fs::read(&wasm_path).await?;
        let validator = beebotos_agents::skills::SkillSecurityValidator::new(
            beebotos_agents::skills::SkillSecurityPolicy::default(),
        );
        validator
            .validate(&wasm_bytes)
            .map_err(|e| format!("WASM security validation failed: {}", e))?;
    }

    info!("Installed skill package to {:?}", skill_dir);
    Ok(())
}

/// List installed skills
/// Extract string array from yaml value
fn yaml_string_array(value: &serde_yaml::Value) -> Vec<String> {
    value
        .as_sequence()
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

async fn list_installed_skills() -> Result<Vec<SkillInfoResponse>, Box<dyn std::error::Error>> {
    let base_dir = get_skills_base_dir();
    let mut skills = Vec::new();

    if let Ok(mut entries) = tokio::fs::read_dir(&base_dir).await {
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                let skill_id = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                // Try to read manifest
                let manifest_path = path.join("skill.yaml");
                if let Ok(content) = tokio::fs::read_to_string(&manifest_path).await {
                    if let Ok(manifest) = serde_yaml::from_str::<serde_yaml::Value>(&content) {
                        skills.push(SkillInfoResponse {
                            id: skill_id.clone(),
                            name: manifest["name"].as_str().unwrap_or(&skill_id).to_string(),
                            version: manifest["version"].as_str().unwrap_or("1.0.0").to_string(),
                            description: manifest["description"].as_str().unwrap_or("").to_string(),
                            author: manifest["author"].as_str().unwrap_or("Unknown").to_string(),
                            license: manifest["license"].as_str().unwrap_or("MIT").to_string(),
                            installed: true,
                            capabilities: yaml_string_array(&manifest["capabilities"]),
                            tags: yaml_string_array(&manifest["tags"]),
                            downloads: 0,
                            rating: 0.0,
                        });
                    }
                }
            }
        }
    }

    Ok(skills)
}

/// Get skill info from local storage
async fn get_skill_info(skill_id: &str) -> Result<SkillInfoResponse, Box<dyn std::error::Error>> {
    let skill_dir = get_skill_install_path(skill_id);
    let manifest_path = skill_dir.join("skill.yaml");

    let content = tokio::fs::read_to_string(&manifest_path).await?;
    let manifest: serde_yaml::Value = serde_yaml::from_str(&content)?;

    Ok(SkillInfoResponse {
        id: skill_id.to_string(),
        name: manifest["name"].as_str().unwrap_or(skill_id).to_string(),
        version: manifest["version"].as_str().unwrap_or("1.0.0").to_string(),
        description: manifest["description"].as_str().unwrap_or("").to_string(),
        author: manifest["author"].as_str().unwrap_or("Unknown").to_string(),
        license: manifest["license"].as_str().unwrap_or("MIT").to_string(),
        installed: true,
        capabilities: yaml_string_array(&manifest["capabilities"]),
        tags: yaml_string_array(&manifest["tags"]),
        downloads: 0,
        rating: 0.0,
    })
}
