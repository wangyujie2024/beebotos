//! Skill Composition HTTP Handlers
//!
//! REST API for declarative skill composition management.

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

use crate::error::GatewayError;
use crate::services::agent_runtime_manager::GatewayLLMInterface;
use crate::AppState;

/// Create composition request (YAML or JSON)
#[derive(Debug, Deserialize)]
pub struct CreateCompositionRequest {
    /// YAML or JSON content of the composition definition
    pub content: String,
    /// Optional explicit ID override
    pub id: Option<String>,
}

/// Execute composition request
#[derive(Debug, Deserialize, Default)]
pub struct ExecuteCompositionRequest {
    /// Input text to feed into the composition
    #[serde(default = "default_input")]
    pub input: String,
    /// Optional target agent ID
    pub agent_id: Option<String>,
}

fn default_input() -> String {
    String::new()
}

/// Composition response
#[derive(Debug, Serialize)]
pub struct CompositionResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub composition_type: String,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Composition execution response
#[derive(Debug, Serialize)]
pub struct CompositionExecutionResponse {
    pub composition_id: String,
    pub status: String,
    pub output: String,
    pub message: String,
}

// ---------------------------------------------------------------------------
// CRUD handlers
// ---------------------------------------------------------------------------

/// Create or register a composition from YAML/JSON
pub async fn create_composition(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateCompositionRequest>,
) -> Result<Json<CompositionResponse>, GatewayError> {
    info!("Creating composition");

    let mut def: beebotos_agents::skills::composition::CompositionDefinition =
        serde_yaml::from_str(&req.content)
            .or_else(|_| serde_json::from_str(&req.content))
            .map_err(|e| GatewayError::bad_request(format!("Invalid YAML/JSON: {}", e)))?;

    if let Some(id_override) = req.id {
        def.id = id_override;
    }

    if def.id.is_empty() {
        return Err(GatewayError::bad_request("Composition ID is required"));
    }

    let registry = state.composition_registry()?;
    let mut reg = registry.write().await;

    reg.create(def.clone())
        .await
        .map_err(|e| GatewayError::bad_request(format!("Failed to create composition: {}", e)))?;

    info!("Composition created: {} ({})", def.name, def.id);
    Ok(Json(to_composition_response(&def)))
}

/// List all registered compositions
pub async fn list_compositions(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<CompositionResponse>>, GatewayError> {
    let registry = state.composition_registry()?;
    let reg = registry.read().await;
    let compositions: Vec<CompositionResponse> =
        reg.list_all().into_iter().map(to_composition_response).collect();

    Ok(Json(compositions))
}

/// Get a single composition by ID
pub async fn get_composition(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<CompositionResponse>, GatewayError> {
    let registry = state.composition_registry()?;
    let reg = registry.read().await;
    let def = reg
        .get(&id)
        .ok_or_else(|| GatewayError::not_found("Composition", &id))?;

    Ok(Json(to_composition_response(def)))
}

/// Delete a composition
pub async fn delete_composition(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let registry = state.composition_registry()?;
    let mut reg = registry.write().await;

    reg.delete(&id)
        .await
        .map_err(|e| GatewayError::bad_request(format!("Failed to delete composition: {}", e)))?;

    info!("Composition deleted: {}", id);
    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Composition {} deleted", id)
    })))
}

// ---------------------------------------------------------------------------
// Execution handler
// ---------------------------------------------------------------------------

/// Execute a composition manually
pub async fn execute_composition(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<ExecuteCompositionRequest>,
) -> Result<Json<CompositionExecutionResponse>, GatewayError> {
    info!("Executing composition: {} with input: {:?}", id, req.input);

    let registry = state.composition_registry()?;
    let def = {
        let reg = registry.read().await;
        reg.get(&id)
            .cloned()
            .ok_or_else(|| GatewayError::not_found("Composition", &id))?
    };

    // Build a temporary agent for composition execution
    let skill_registry = state.skill_registry()?.clone();

    let llm_interface: Arc<dyn beebotos_agents::communication::LLMCallInterface> =
        Arc::new(GatewayLLMInterface::new(state.llm_service.clone()));

    let agent = beebotos_agents::AgentBuilder::new("composition-runner")
        .description("Temporary agent for composition execution")
        .build()
        .with_skill_registry(skill_registry)
        .with_llm_interface(llm_interface);

    let runtime_node = def
        .to_runtime(&agent)
        .map_err(|e| GatewayError::Internal {
            message: format!("Failed to build composition runtime: {}", e),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

    match runtime_node.execute(&req.input, &agent).await {
        Ok(output) => {
            info!(
                "Composition '{}' executed successfully",
                id
            );
            Ok(Json(CompositionExecutionResponse {
                composition_id: id,
                status: "completed".to_string(),
                output,
                message: "Composition executed successfully".to_string(),
            }))
        }
        Err(e) => {
            warn!("Composition '{}' execution failed: {}", id, e);
            return Err(GatewayError::Internal {
                message: format!("Execution failed: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn to_composition_response(
    def: &beebotos_agents::skills::composition::CompositionDefinition,
) -> CompositionResponse {
    let composition_type = match &def.config {
        beebotos_agents::skills::composition::CompositionConfig::Pipeline { .. } => "pipeline",
        beebotos_agents::skills::composition::CompositionConfig::Parallel { .. } => "parallel",
        beebotos_agents::skills::composition::CompositionConfig::Conditional { .. } => "conditional",
        beebotos_agents::skills::composition::CompositionConfig::Loop { .. } => "loop",
    }
    .to_string();

    CompositionResponse {
        id: def.id.clone(),
        name: def.name.clone(),
        description: def.description.clone(),
        composition_type,
        tags: def.tags.clone(),
        created_at: def.created_at.clone(),
        updated_at: def.updated_at.clone(),
    }
}
