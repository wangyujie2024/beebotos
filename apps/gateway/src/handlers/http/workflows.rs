//! Workflow HTTP Handlers
//!
//! REST API for declarative workflow management.
//! Provides CRUD for WorkflowDefinition and manual execution triggers.

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

use crate::error::GatewayError;
use crate::services::agent_runtime_manager::GatewayLLMInterface;
use crate::AppState;

/// Create workflow request (YAML or JSON)
#[derive(Debug, Deserialize)]
pub struct CreateWorkflowRequest {
    /// YAML content of the workflow definition
    pub yaml: String,
    /// Optional explicit ID override
    pub id: Option<String>,
}

/// Update workflow request
#[derive(Debug, Deserialize)]
pub struct UpdateWorkflowRequest {
    /// New YAML content
    pub yaml: String,
}

/// Install workflow from local file path request
#[derive(Debug, Deserialize)]
pub struct InstallWorkflowRequest {
    /// Absolute or relative path to the workflow YAML/JSON file
    pub source_path: String,
}

/// Install workflow response
#[derive(Debug, Serialize)]
pub struct InstallWorkflowResponse {
    pub success: bool,
    pub id: String,
    pub name: String,
    pub message: String,
    pub installed_path: String,
}

/// Workflow source response (raw YAML/JSON content)
#[derive(Debug, Serialize)]
pub struct WorkflowSourceResponse {
    pub yaml: String,
}

/// Workflow response
#[derive(Debug, Serialize)]
pub struct WorkflowResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: Option<String>,
    pub tags: Vec<String>,
    pub steps_count: usize,
    pub triggers: Vec<WorkflowTriggerResponse>,
    pub steps: Vec<WorkflowStepResponse>,
}

/// Workflow trigger summary for API response
#[derive(Debug, Serialize)]
pub struct WorkflowTriggerResponse {
    pub trigger_type: String,
    pub detail: String,
}

/// Workflow step summary for DAG visualization
#[derive(Debug, Serialize)]
pub struct WorkflowStepResponse {
    pub id: String,
    pub name: String,
    pub skill: String,
    pub depends_on: Option<Vec<String>>,
    pub condition: Option<String>,
}

/// Workflow execution request
#[derive(Debug, Deserialize, Default)]
pub struct ExecuteWorkflowRequest {
    /// Optional trigger context (passed to workflow instance)
    #[serde(default)]
    pub context: serde_json::Value,
    /// Optional target agent ID to run the workflow
    pub agent_id: Option<String>,
}

/// Workflow execution response
#[derive(Debug, Serialize)]
pub struct WorkflowExecutionResponse {
    pub instance_id: String,
    pub workflow_id: String,
    pub status: String,
    pub message: String,
}

/// Workflow instance status response
#[derive(Debug, Serialize)]
pub struct WorkflowInstanceResponse {
    pub instance_id: String,
    pub workflow_id: String,
    pub status: String,
    pub completion_pct: f32,
    pub duration_secs: u64,
    pub step_states: Vec<StepStateResponse>,
    pub error_log: Vec<WorkflowErrorResponse>,
}

/// List workflow instances query parameters
#[derive(Debug, Deserialize)]
pub struct ListInstancesQuery {
    pub workflow_id: Option<String>,
    pub status: Option<String>,
    pub limit: Option<usize>,
}

/// Summary response for listing instances
#[derive(Debug, Serialize)]
pub struct WorkflowInstanceSummary {
    pub instance_id: String,
    pub workflow_id: String,
    pub workflow_name: String,
    pub status: String,
    pub completion_pct: f32,
    pub duration_secs: u64,
    pub started_at: String,
    pub step_count: usize,
}

/// Step state in API response
#[derive(Debug, Serialize)]
pub struct StepStateResponse {
    pub step_id: String,
    pub status: String,
    pub duration_secs: u64,
    pub error: Option<String>,
}

/// Workflow error in API response
#[derive(Debug, Serialize)]
pub struct WorkflowErrorResponse {
    pub step_id: Option<String>,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Database persistence helpers
// ---------------------------------------------------------------------------

/// Save or update a workflow instance to the database
pub async fn save_workflow_instance(
    db: &sqlx::SqlitePool,
    instance: &beebotos_agents::workflow::WorkflowInstance,
) -> Result<(), sqlx::Error> {
    let step_states = serde_json::to_string(&instance.step_states).unwrap_or_default();
    let error_log = serde_json::to_string(&instance.error_log).unwrap_or_default();
    let trigger_context = serde_json::to_string(&instance.trigger_context).unwrap_or_default();
    let completed_at = instance.completed_at.map(|dt| dt.to_rfc3339());

    sqlx::query(
        r#"
        INSERT INTO workflow_instances (id, workflow_id, status, trigger_context, step_states, error_log, started_at, completed_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now'))
        ON CONFLICT(id) DO UPDATE SET
            status = excluded.status,
            trigger_context = excluded.trigger_context,
            step_states = excluded.step_states,
            error_log = excluded.error_log,
            completed_at = excluded.completed_at,
            updated_at = datetime('now')
        "#
    )
    .bind(&instance.id)
    .bind(&instance.workflow_id)
    .bind(instance.status.to_string())
    .bind(&trigger_context)
    .bind(&step_states)
    .bind(&error_log)
    .bind(instance.started_at.to_rfc3339())
    .bind(&completed_at)
    .execute(db)
    .await?;

    Ok(())
}

/// Load workflow instances from database
pub async fn load_workflow_instances(
    db: &sqlx::SqlitePool,
) -> Result<Vec<beebotos_agents::workflow::WorkflowInstance>, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct InstanceRow {
        id: String,
        workflow_id: String,
        status: String,
        trigger_context: String,
        step_states: String,
        error_log: String,
        started_at: String,
        completed_at: Option<String>,
    }

    let rows: Vec<InstanceRow> = sqlx::query_as(
        "SELECT id, workflow_id, status, trigger_context, step_states, error_log, started_at, completed_at FROM workflow_instances ORDER BY started_at DESC LIMIT 1000"
    )
    .fetch_all(db)
    .await?;

    let mut instances = Vec::new();
    for row in rows {
        let status = match row.status.as_str() {
            "completed" => beebotos_agents::workflow::WorkflowStatus::Completed,
            "failed" => beebotos_agents::workflow::WorkflowStatus::Failed,
            "cancelled" => beebotos_agents::workflow::WorkflowStatus::Cancelled,
            "running" => beebotos_agents::workflow::WorkflowStatus::Running,
            _ => beebotos_agents::workflow::WorkflowStatus::Pending,
        };

        let step_states: std::collections::HashMap<String, beebotos_agents::workflow::StepState> =
            serde_json::from_str(&row.step_states).unwrap_or_default();
        let error_log: Vec<beebotos_agents::workflow::WorkflowError> =
            serde_json::from_str(&row.error_log).unwrap_or_default();
        let trigger_context: serde_json::Value =
            serde_json::from_str(&row.trigger_context).unwrap_or_default();
        let started_at = chrono::DateTime::parse_from_rfc3339(&row.started_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now());
        let completed_at = row.completed_at.and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&chrono::Utc)));

        instances.push(beebotos_agents::workflow::WorkflowInstance {
            id: row.id,
            workflow_id: row.workflow_id,
            status,
            step_states,
            trigger_context,
            started_at,
            completed_at,
            error_log,
        });
    }

    Ok(instances)
}

/// Delete a workflow instance from the database
pub async fn delete_workflow_instance_db(
    db: &sqlx::SqlitePool,
    instance_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM workflow_instances WHERE id = ?1")
        .bind(instance_id)
        .execute(db)
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Cron scheduling helpers
// ---------------------------------------------------------------------------

/// Dynamically add cron jobs for a workflow to the running JobScheduler.
/// Returns the list of UUIDs for the created jobs.
pub async fn add_cron_jobs_for_workflow(
    state: &Arc<AppState>,
    workflow_id: &str,
    definition: &beebotos_agents::workflow::WorkflowDefinition,
) -> Vec<uuid::Uuid> {
    let mut uuids = Vec::new();
    let Some(ref scheduler) = state.workflow_cron_scheduler else {
        return uuids;
    };

    for trigger in &definition.triggers {
        if let beebotos_agents::workflow::TriggerType::Cron { schedule, timezone } = &trigger.trigger_type {
            let state_clone = state.clone();
            let wf_id = workflow_id.to_string();
            let sched_str = schedule.clone();
            let tz_str = timezone.clone().unwrap_or_else(|| "UTC".to_string());
            let fired_at = chrono::Utc::now().to_rfc3339();
            // Clone for logging after the closure moves the originals
            let log_sched = sched_str.clone();
            let log_tz = tz_str.clone();

            let job = tokio_cron_scheduler::Job::new_async(sched_str.clone(), move |_uuid, _l| {
                let state = state_clone.clone();
                let wf_id = wf_id.clone();
                let sched = sched_str.clone();
                let tz = tz_str.clone();
                let fired_at = fired_at.clone();
                Box::pin(async move {
                    let trigger_context = serde_json::json!({
                        "trigger_type": "cron",
                        "schedule": sched,
                        "timezone": tz,
                        "fired_at": fired_at
                    });
                    match execute_workflow_internal(&state, &wf_id, trigger_context).await {
                        Ok(instance) => {
                            info!("✅ Cron workflow {} completed with status: {}", wf_id, instance.status);
                        }
                        Err(e) => {
                            warn!("❌ Cron workflow {} failed: {}", wf_id, e);
                        }
                    }
                })
            });

            match job {
                Ok(j) => {
                    let job_uuid = j.guid();
                    if let Err(e) = scheduler.add(j).await {
                        warn!("Failed to add cron job for workflow {}: {}", workflow_id, e);
                    } else {
                        info!("⏰ Registered cron job for workflow {}: {} ({})", workflow_id, log_sched, log_tz);
                        uuids.push(job_uuid);
                    }
                }
                Err(e) => {
                    warn!("Invalid cron schedule for workflow {}: {}", workflow_id, e);
                }
            }
        }
    }

    if !uuids.is_empty() {
        let mut map = state.workflow_cron_job_uuids.write().await;
        map.insert(workflow_id.to_string(), uuids.clone());
    }

    uuids
}

/// Remove all cron jobs for a workflow from the JobScheduler.
pub async fn remove_cron_jobs_for_workflow(
    state: &Arc<AppState>,
    workflow_id: &str,
) {
    let uuids: Vec<uuid::Uuid> = {
        let mut map = state.workflow_cron_job_uuids.write().await;
        map.remove(workflow_id).unwrap_or_default()
    };

    if uuids.is_empty() {
        return;
    }

    if let Some(ref scheduler) = state.workflow_cron_scheduler {
        for job_uuid in &uuids {
            if let Err(e) = scheduler.remove(job_uuid).await {
                warn!("Failed to remove cron job {} for workflow {}: {}", job_uuid, workflow_id, e);
            } else {
                info!("⏰ Removed cron job {} for workflow {}", job_uuid, workflow_id);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// CRUD handlers
// ---------------------------------------------------------------------------

/// Create or register a workflow from YAML
pub async fn create_workflow(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateWorkflowRequest>,
) -> Result<Json<WorkflowResponse>, GatewayError> {
    info!("Creating workflow from YAML");

    let mut def: beebotos_agents::workflow::WorkflowDefinition =
        serde_yaml::from_str(&req.yaml).map_err(|e| GatewayError::bad_request(format!("Invalid YAML: {}", e)))?;

    if let Some(id_override) = req.id {
        def.id = id_override;
    }

    if def.id.is_empty() {
        return Err(GatewayError::bad_request("Workflow ID is required"));
    }

    {
        let registry = state.workflow_registry()?;
        let mut reg = registry.write().await;
        reg.register(def.clone());
    }

    // Register triggers in the trigger engine
    if let Some(ref trigger_engine) = state.workflow_trigger_engine {
        let mut engine = trigger_engine.write().await;
        engine.register(&def);
        info!("Registered {} triggers for workflow {}", def.triggers.len(), def.id);
    }

    // 🟢 CRON FIX: Dynamically register cron jobs for runtime-created workflows
    let cron_uuids = add_cron_jobs_for_workflow(&state, &def.id, &def).await;
    if !cron_uuids.is_empty() {
        info!("Dynamically registered {} cron job(s) for workflow {}", cron_uuids.len(), def.id);
    }

    // Persist to disk
    let workflow_dir = std::path::PathBuf::from("data/workflows");
    if let Err(e) = tokio::fs::create_dir_all(&workflow_dir).await {
        warn!("Failed to create workflow directory: {}", e);
    }
    let path = workflow_dir.join(format!("{}.yaml", def.id));
    if let Err(e) = tokio::fs::write(&path, &req.yaml).await {
        warn!("Failed to persist workflow {}: {}", def.id, e);
    }

    info!("Workflow created: {} ({})", def.name, def.id);
    Ok(Json(to_workflow_response(&def)))
}

/// List all registered workflows
pub async fn list_workflows(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<WorkflowResponse>>, GatewayError> {
    let registry = state.workflow_registry()?;
    let reg = registry.read().await;
    let workflows: Vec<WorkflowResponse> = reg.list_all().into_iter().map(to_workflow_response).collect();

    Ok(Json(workflows))
}

/// Get a single workflow by ID
pub async fn get_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowResponse>, GatewayError> {
    let registry = state.workflow_registry()?;
    let reg = registry.read().await;
    let def = reg
        .get(&id)
        .ok_or_else(|| GatewayError::not_found("Workflow", &id))?;

    Ok(Json(to_workflow_response(def)))
}

/// Get raw YAML/JSON source of a workflow
pub async fn get_workflow_source(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowSourceResponse>, GatewayError> {
    // Try root data/workflows/ first
    let root_yaml = std::path::PathBuf::from("data/workflows").join(format!("{}.yaml", &id));
    let root_yml = std::path::PathBuf::from("data/workflows").join(format!("{}.yml", &id));
    let root_json = std::path::PathBuf::from("data/workflows").join(format!("{}.json", &id));

    let paths = vec![root_yaml, root_yml, root_json];
    for path in &paths {
        if path.exists() {
            let content = tokio::fs::read_to_string(path).await
                .map_err(|e| GatewayError::Internal {
                    message: format!("Failed to read workflow source: {}", e),
                    correlation_id: uuid::Uuid::new_v4().to_string(),
                })?;
            return Ok(Json(WorkflowSourceResponse { yaml: content }));
        }
    }

    // Try data/workflows/local/
    let local_dir = std::path::PathBuf::from("data/workflows/local");
    if local_dir.exists() {
        let mut entries = tokio::fs::read_dir(&local_dir).await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to read local directory: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            })?;
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if let Some(stem) = path.file_stem() {
                if stem.to_string_lossy() == id {
                    let content = tokio::fs::read_to_string(&path).await
                        .map_err(|e| GatewayError::Internal {
                            message: format!("Failed to read workflow source: {}", e),
                            correlation_id: uuid::Uuid::new_v4().to_string(),
                        })?;
                    return Ok(Json(WorkflowSourceResponse { yaml: content }));
                }
            }
        }
    }

    Err(GatewayError::not_found("Workflow source", &id))
}

/// Update an existing workflow (replace YAML definition)
pub async fn update_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateWorkflowRequest>,
) -> Result<Json<WorkflowResponse>, GatewayError> {
    info!("Updating workflow: {}", id);

    let mut def: beebotos_agents::workflow::WorkflowDefinition =
        serde_yaml::from_str(&req.yaml).map_err(|e| GatewayError::bad_request(format!("Invalid YAML: {}", e)))?;

    def.id = id.clone();

    // Remove old cron jobs before updating
    remove_cron_jobs_for_workflow(&state, &id).await;

    // Update registry
    {
        let registry = state.workflow_registry()?;
        let mut reg = registry.write().await;
        reg.register(def.clone());
    }

    // Re-register triggers
    if let Some(ref trigger_engine) = state.workflow_trigger_engine {
        let mut engine = trigger_engine.write().await;
        engine.unregister(&id);
        engine.register(&def);
        info!("Re-registered {} triggers for workflow {}", def.triggers.len(), id);
    }

    // Re-register cron jobs for the updated definition
    let cron_uuids = add_cron_jobs_for_workflow(&state, &id, &def).await;
    if !cron_uuids.is_empty() {
        info!("Dynamically registered {} cron job(s) for updated workflow {}", cron_uuids.len(), id);
    }

    // Persist to disk (overwrite)
    let workflow_dir = std::path::PathBuf::from("data/workflows");
    if let Err(e) = tokio::fs::create_dir_all(&workflow_dir).await {
        warn!("Failed to create workflow directory: {}", e);
    }
    let path = workflow_dir.join(format!("{}.yaml", id));
    if let Err(e) = tokio::fs::write(&path, &req.yaml).await {
        warn!("Failed to persist updated workflow {}: {}", id, e);
    }

    info!("Workflow updated: {} ({})", def.name, id);
    Ok(Json(to_workflow_response(&def)))
}

/// Delete a workflow
pub async fn delete_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    {
        let registry = state.workflow_registry()?;
        let mut reg = registry.write().await;
        reg.remove(&id);
    }

    // 🟢 CRON FIX: Remove cron jobs before unregistering triggers
    remove_cron_jobs_for_workflow(&state, &id).await;

    // Unregister triggers from the trigger engine
    if let Some(ref trigger_engine) = state.workflow_trigger_engine {
        let mut engine = trigger_engine.write().await;
        engine.unregister(&id);
        info!("Unregistered triggers for workflow {}", id);
    }

    // Remove from disk
    let path = std::path::PathBuf::from("data/workflows").join(format!("{}.yaml", &id));
    if path.exists() {
        let _ = tokio::fs::remove_file(&path).await;
    }

    info!("Workflow deleted: {}", id);
    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Workflow {} deleted", id)
    })))
}

/// Install a workflow from a local file path
pub async fn install_workflow(
    State(state): State<Arc<AppState>>,
    Json(req): Json<InstallWorkflowRequest>,
) -> Result<Json<InstallWorkflowResponse>, GatewayError> {
    info!("Installing workflow from: {}", req.source_path);

    let source_path = std::path::PathBuf::from(&req.source_path);
    if !source_path.exists() {
        return Err(GatewayError::bad_request(format!("Source file not found: {}", req.source_path)));
    }

    // Read source file content
    let content = tokio::fs::read_to_string(&source_path).await
        .map_err(|e| GatewayError::bad_request(format!("Failed to read source file: {}", e)))?;

    // Parse workflow definition
    let ext = source_path.extension().and_then(|e| e.to_str()).unwrap_or("yaml");
    let mut def: beebotos_agents::workflow::WorkflowDefinition = if ext == "json" {
        serde_json::from_str(&content).map_err(|e| GatewayError::bad_request(format!("Invalid JSON: {}", e)))?
    } else {
        serde_yaml::from_str(&content).map_err(|e| GatewayError::bad_request(format!("Invalid YAML: {}", e)))?
    };

    // OpenClaw compatibility: auto-populate id/name
    if def.id.is_empty() {
        def.id = def.name.clone();
    }
    if def.name.is_empty() {
        def.name = def.id.clone();
    }
    if def.id.is_empty() {
        def.id = source_path.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
    }
    if def.name.is_empty() {
        def.name = def.id.clone();
    }

    // Ensure local directory exists
    let local_dir = std::path::PathBuf::from("data/workflows/local");
    if let Err(e) = tokio::fs::create_dir_all(&local_dir).await {
        warn!("Failed to create local workflow directory: {}", e);
    }

    // Copy file to data/workflows/local/
    let filename = format!("{}.{}", def.id, ext);
    let installed_path = local_dir.join(&filename);
    if let Err(e) = tokio::fs::write(&installed_path, &content).await {
        return Err(GatewayError::Internal {
            message: format!("Failed to write workflow to local directory: {}", e),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        });
    }

    // Register in workflow registry
    {
        let registry = state.workflow_registry()?;
        let mut reg = registry.write().await;
        reg.register(def.clone());
    }

    // Register triggers in the trigger engine
    if let Some(ref trigger_engine) = state.workflow_trigger_engine {
        let mut engine = trigger_engine.write().await;
        engine.register(&def);
        info!("Registered {} triggers for workflow {}", def.triggers.len(), def.id);
    }

    // Register cron jobs
    let cron_uuids = add_cron_jobs_for_workflow(&state, &def.id, &def).await;
    if !cron_uuids.is_empty() {
        info!("Dynamically registered {} cron job(s) for installed workflow {}", cron_uuids.len(), def.id);
    }

    info!("Workflow installed: {} ({}) -> {:?}", def.name, def.id, installed_path);
    Ok(Json(InstallWorkflowResponse {
        success: true,
        id: def.id.clone(),
        name: def.name.clone(),
        message: format!("Workflow '{}' installed successfully", def.name),
        installed_path: installed_path.to_string_lossy().to_string(),
    }))
}

/// Uninstall a workflow (remove from registry and delete from local/ root dirs)
pub async fn uninstall_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    info!("Uninstalling workflow: {}", id);

    // Remove from registry
    {
        let registry = state.workflow_registry()?;
        let mut reg = registry.write().await;
        reg.remove(&id);
    }

    // Remove cron jobs
    remove_cron_jobs_for_workflow(&state, &id).await;

    // Unregister triggers
    if let Some(ref trigger_engine) = state.workflow_trigger_engine {
        let mut engine = trigger_engine.write().await;
        engine.unregister(&id);
        info!("Unregistered triggers for workflow {}", id);
    }

    // Try delete from root data/workflows/
    let root_path = std::path::PathBuf::from("data/workflows").join(format!("{}.yaml", &id));
    if root_path.exists() {
        let _ = tokio::fs::remove_file(&root_path).await;
    }
    let root_path_json = std::path::PathBuf::from("data/workflows").join(format!("{}.json", &id));
    if root_path_json.exists() {
        let _ = tokio::fs::remove_file(&root_path_json).await;
    }

    // Try delete from data/workflows/local/
    let local_dir = std::path::PathBuf::from("data/workflows/local");
    if local_dir.exists() {
        let mut entries = tokio::fs::read_dir(&local_dir).await
            .map_err(|e| GatewayError::Internal {
                message: format!("Failed to read local directory: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            })?;
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if let Some(stem) = path.file_stem() {
                if stem.to_string_lossy() == id {
                    let _ = tokio::fs::remove_file(&path).await;
                }
            }
        }
    }

    info!("Workflow uninstalled: {}", id);
    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Workflow {} uninstalled", id)
    })))
}

// ---------------------------------------------------------------------------
// Step progress reporter that persists instance state after each step
// ---------------------------------------------------------------------------

pub struct DbProgressReporter {
    db: sqlx::SqlitePool,
}

impl DbProgressReporter {
    pub fn new(db: sqlx::SqlitePool) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl beebotos_agents::workflow::StepProgressReporter for DbProgressReporter {
    async fn on_step_complete(&self, instance: &beebotos_agents::workflow::WorkflowInstance) {
        if let Err(e) = save_workflow_instance(&self.db, instance).await {
            warn!("Failed to persist workflow instance progress {}: {}", instance.id, e);
        }
    }
}

// ---------------------------------------------------------------------------
// Execution helpers (shared by HTTP handler and background tasks)
// ---------------------------------------------------------------------------

/// Internal workflow execution logic
pub async fn execute_workflow_internal(
    state: &Arc<AppState>,
    workflow_id: &str,
    trigger_context: serde_json::Value,
) -> Result<beebotos_agents::workflow::WorkflowInstance, GatewayError> {
    let registry = state.workflow_registry()?;
    let def = {
        let reg = registry.read().await;
        reg.get(workflow_id)
            .cloned()
            .ok_or_else(|| GatewayError::not_found("Workflow", workflow_id))?
    };

    // Build a temporary agent for workflow step execution
    let skill_registry = state.skill_registry()?.clone();

    let llm_interface: Arc<dyn beebotos_agents::communication::LLMCallInterface> =
        Arc::new(GatewayLLMInterface::new(state.llm_service.clone()));

    let agent = beebotos_agents::AgentBuilder::new("workflow-runner")
        .description("Temporary agent for workflow execution")
        .build()
        .with_skill_registry(skill_registry)
        .with_llm_interface(llm_interface);

    // Setup cancellation signal
    let cancel_signal = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let instance_id_pre = uuid::Uuid::new_v4().to_string();
    {
        let mut signals = state.workflow_cancel_signals.write().await;
        signals.insert(instance_id_pre.clone(), cancel_signal.clone());
    }

    // Execute workflow with DB progress reporting and cancellation support
    let engine = beebotos_agents::workflow::WorkflowEngine::new();
    let reporter = DbProgressReporter::new(state.db.clone());
    let instance = engine.execute_with_cancel(&def, &agent, trigger_context, Some(&reporter), Some(cancel_signal.clone())).await
        .map_err(|e| GatewayError::Internal {
            message: format!("Workflow execution failed: {}", e),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

    // Clean up cancellation signal
    {
        let mut signals = state.workflow_cancel_signals.write().await;
        signals.remove(&instance.id);
    }

    let instance_id = instance.id.clone();
    let status = instance.status.to_string();

    // Store the instance in memory
    if let Some(instances) = state.workflow_instances.as_ref() {
        let mut inst_map = instances.write().await;
        inst_map.insert(instance_id.clone(), instance.clone());
    }

    // Persist to database
    if let Err(e) = save_workflow_instance(&state.db, &instance).await {
        warn!("Failed to persist workflow instance {}: {}", instance_id, e);
    }

    info!(
        "Workflow '{}' executed: instance={} status={}",
        workflow_id, instance_id, status
    );

    Ok(instance)
}

// ---------------------------------------------------------------------------
// Execution handlers
// ---------------------------------------------------------------------------

/// Execute a workflow manually
pub async fn execute_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<ExecuteWorkflowRequest>,
) -> Result<Json<WorkflowExecutionResponse>, GatewayError> {
    info!("Executing workflow: {} with context: {:?}", id, req.context);

    let instance = execute_workflow_internal(&state, &id, req.context).await?;
    let instance_id = instance.id.clone();
    let status = instance.status.to_string();

    let message = if status == "completed" {
        "Workflow executed successfully".to_string()
    } else if status == "failed" {
        "Workflow execution failed".to_string()
    } else {
        format!("Workflow execution finished with status: {}", status)
    };

    Ok(Json(WorkflowExecutionResponse {
        instance_id,
        workflow_id: id,
        status,
        message,
    }))
}

/// Webhook trigger handler for workflow execution
pub async fn workflow_webhook_trigger(
    State(state): State<Arc<AppState>>,
    axum::extract::OriginalUri(uri): axum::extract::OriginalUri,
    headers: axum::http::HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<WorkflowExecutionResponse>, GatewayError> {
    let path = uri.path();
    info!("Workflow webhook triggered: {}", path);

    let trigger_engine = state.workflow_trigger_engine()?;

    let trigger_match = {
        let engine = trigger_engine.read().await;
        engine.match_webhook(path, "POST")
    };

    let trigger_match = trigger_match
        .ok_or_else(|| GatewayError::not_found("Webhook trigger", path))?;

    // 🟢 AUTH FIX: Validate bearer token if webhook requires auth
    if let Some(expected_token) = &trigger_match.auth {
        let auth_header = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok());
        let provided_token = auth_header.and_then(|h| {
            h.strip_prefix("Bearer ").or_else(|| h.strip_prefix("bearer "))
        });
        match provided_token {
            Some(token) if token == expected_token => {
                // Auth valid, proceed
            }
            _ => {
                warn!("Webhook auth failed for path {}: expected Bearer token", path);
                return Err(GatewayError::unauthorized("Invalid or missing webhook token"));
            }
        }
    }

    let mut context = trigger_match.trigger_context;
    // Merge webhook payload into trigger context
    if let serde_json::Value::Object(ref mut map) = context {
        map.insert("payload".to_string(), payload);
    }

    let instance = execute_workflow_internal(&state, &trigger_match.workflow_id, context).await?;
    let instance_id = instance.id.clone();
    let status = instance.status.to_string();

    Ok(Json(WorkflowExecutionResponse {
        instance_id,
        workflow_id: trigger_match.workflow_id,
        status,
        message: "Workflow triggered via webhook".to_string(),
    }))
}

/// Get workflow instance status
pub async fn get_workflow_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowInstanceResponse>, GatewayError> {
    let instances = state.workflow_instances()?;
    let inst_map = instances.read().await;
    let instance = inst_map
        .get(&id)
        .ok_or_else(|| GatewayError::not_found("WorkflowInstance", &id))?;

    let step_states: Vec<StepStateResponse> = instance
        .step_states
        .values()
        .map(|s| StepStateResponse {
            step_id: s.step_id.clone(),
            status: s.status.to_string(),
            duration_secs: s.duration_secs(),
            error: s.error.clone(),
        })
        .collect();

    let error_log: Vec<WorkflowErrorResponse> = instance
        .error_log
        .iter()
        .map(|e| WorkflowErrorResponse {
            step_id: e.step_id.clone(),
            message: e.message.clone(),
        })
        .collect();

    Ok(Json(WorkflowInstanceResponse {
        instance_id: instance.id.clone(),
        workflow_id: instance.workflow_id.clone(),
        status: instance.status.to_string(),
        completion_pct: instance.completion_pct(),
        duration_secs: instance.duration_secs(),
        step_states,
        error_log,
    }))
}

/// List workflow instances
pub async fn list_workflow_instances(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListInstancesQuery>,
) -> Result<Json<Vec<WorkflowInstanceSummary>>, GatewayError> {
    let instances = state.workflow_instances()?;
    let registry = state.workflow_registry()?;

    let inst_map = instances.read().await;
    let reg = registry.read().await;

    let mut results: Vec<WorkflowInstanceSummary> = inst_map
        .values()
        .filter(|inst| {
            if let Some(ref wf_id) = query.workflow_id {
                if inst.workflow_id != *wf_id {
                    return false;
                }
            }
            if let Some(ref status) = query.status {
                if inst.status.to_string() != *status {
                    return false;
                }
            }
            true
        })
        .map(|inst| {
            let workflow_name = reg
                .get(&inst.workflow_id)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            WorkflowInstanceSummary {
                instance_id: inst.id.clone(),
                workflow_id: inst.workflow_id.clone(),
                workflow_name,
                status: inst.status.to_string(),
                completion_pct: inst.completion_pct(),
                duration_secs: inst.duration_secs(),
                started_at: inst.started_at.to_rfc3339(),
                step_count: inst.step_states.len(),
            }
        })
        .collect();

    // Sort by started_at descending
    results.sort_by(|a, b| b.started_at.cmp(&a.started_at));

    if let Some(limit) = query.limit {
        results.truncate(limit);
    }

    Ok(Json(results))
}

/// Get a single workflow instance by ID
pub async fn get_workflow_instance(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<WorkflowInstanceResponse>, GatewayError> {
    let instances = state.workflow_instances()?;
    let inst_map = instances.read().await;
    let instance = inst_map
        .get(&id)
        .cloned()
        .ok_or_else(|| GatewayError::not_found("WorkflowInstance", &id))?;

    let step_states: Vec<StepStateResponse> = instance
        .step_states
        .values()
        .map(|s| StepStateResponse {
            step_id: s.step_id.clone(),
            status: s.status.to_string(),
            duration_secs: s.duration_secs(),
            error: s.error.clone(),
        })
        .collect();

    let error_log: Vec<WorkflowErrorResponse> = instance
        .error_log
        .iter()
        .map(|e| WorkflowErrorResponse {
            step_id: e.step_id.clone(),
            message: e.message.clone(),
        })
        .collect();

    Ok(Json(WorkflowInstanceResponse {
        instance_id: instance.id.clone(),
        workflow_id: instance.workflow_id.clone(),
        status: instance.status.to_string(),
        completion_pct: instance.completion_pct(),
        duration_secs: instance.duration_secs(),
        step_states,
        error_log,
    }))
}

/// Cancel a running workflow instance
pub async fn cancel_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    // Signal cancellation to the running workflow engine
    {
        let signals = state.workflow_cancel_signals.read().await;
        if let Some(signal) = signals.get(&id) {
            signal.store(true, std::sync::atomic::Ordering::Relaxed);
            info!("Cancellation signal sent to workflow instance {}", id);
        }
    }

    let instances = state.workflow_instances()?;
    let mut inst_map = instances.write().await;
    let instance = inst_map
        .get_mut(&id)
        .ok_or_else(|| GatewayError::not_found("WorkflowInstance", &id))?;

    if instance.status.is_terminal() {
        return Ok(Json(serde_json::json!({
            "success": false,
            "message": format!("Workflow instance {} is already in terminal state: {}", id, instance.status)
        })));
    }

    instance.mark_cancelled();
    let instance_clone = instance.clone();
    drop(inst_map);

    // Persist cancellation to database
    if let Err(e) = save_workflow_instance(&state.db, &instance_clone).await {
        warn!("Failed to persist cancelled workflow instance {}: {}", id, e);
    }

    // Clean up cancellation signal
    {
        let mut signals = state.workflow_cancel_signals.write().await;
        signals.remove(&id);
    }

    info!("Workflow instance cancelled: {}", id);
    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Workflow instance {} marked as cancelled", id)
    })))
}

/// Delete a workflow instance
pub async fn delete_workflow_instance(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    // Remove from memory
    if let Some(instances) = state.workflow_instances.as_ref() {
        let mut inst_map = instances.write().await;
        inst_map.remove(&id);
    }

    // Remove from database
    if let Err(e) = delete_workflow_instance_db(&state.db, &id).await {
        warn!("Failed to delete workflow instance {} from DB: {}", id, e);
    }

    info!("Workflow instance deleted: {}", id);
    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Workflow instance {} deleted", id)
    })))
}

/// Dashboard: global workflow statistics
pub async fn dashboard_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let registry = state.workflow_registry()?;
    let instances = state.workflow_instances()?;

    let reg = registry.read().await;
    let inst_map = instances.read().await;

    let total_workflows = reg.list_all().len();
    let total_instances = inst_map.len();

    let (completed, failed, running, pending) = inst_map.values().fold(
        (0usize, 0usize, 0usize, 0usize),
        |(c, f, r, p), inst| match inst.status {
            beebotos_agents::workflow::WorkflowStatus::Completed => (c + 1, f, r, p),
            beebotos_agents::workflow::WorkflowStatus::Failed => (c, f + 1, r, p),
            beebotos_agents::workflow::WorkflowStatus::Running => (c, f, r + 1, p),
            _ => (c, f, r, p + 1),
        },
    );

    Ok(Json(serde_json::json!({
        "total_workflows": total_workflows,
        "total_instances": total_instances,
        "completed": completed,
        "failed": failed,
        "running": running,
        "pending": pending,
    })))
}

/// Dashboard: statistics for a single workflow
pub async fn workflow_stats(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let registry = state.workflow_registry()?;
    let instances = state.workflow_instances()?;

    let reg = registry.read().await;
    let def = reg.get(&id).cloned();
    drop(reg);

    let inst_map = instances.read().await;
    let workflow_instances: Vec<_> = inst_map.values()
        .filter(|i| i.workflow_id == id)
        .collect();

    let total = workflow_instances.len();
    let completed = workflow_instances.iter()
        .filter(|i| i.status == beebotos_agents::workflow::WorkflowStatus::Completed)
        .count();
    let failed = workflow_instances.iter()
        .filter(|i| i.status == beebotos_agents::workflow::WorkflowStatus::Failed)
        .count();
    let avg_duration = if total > 0 {
        let sum: u64 = workflow_instances.iter().map(|i| i.duration_secs()).sum();
        sum / total as u64
    } else {
        0
    };

    Ok(Json(serde_json::json!({
        "workflow_id": id,
        "workflow_name": def.as_ref().map(|d| d.name.clone()),
        "total_instances": total,
        "completed": completed,
        "failed": failed,
        "avg_duration_secs": avg_duration,
        "success_rate": if total > 0 { (completed as f64 / total as f64 * 100.0).round() } else { 0.0 },
    })))
}

/// Dashboard: recent workflow instances (last N)
pub async fn recent_instances(
    State(state): State<Arc<AppState>>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<WorkflowInstanceSummary>>, GatewayError> {
    let limit = query.get("limit")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(20);

    // 🟢 P2 FIX: Use StateStore CQRS for workflow queries when available
    let store_result = state.state_store.query(
        gateway::state_store::StateQuery::ListWorkflowInstances {
            status: None,
            limit,
        }
    ).await;

    if let Ok(gateway::state_store::QueryResult::WorkflowInstanceList { instances, .. }) = store_result {
        let registry = state.workflow_registry()?;
        let reg = registry.read().await;

        let summaries: Vec<WorkflowInstanceSummary> = instances.into_iter().filter_map(|v| {
            let instance_id = v.get("instance_id")?.as_str()?.to_string();
            let workflow_id = v.get("workflow_id")?.as_str()?.to_string();
            let status = v.get("status")?.as_str()?.to_string();
            let def = reg.get(&workflow_id);
            Some(WorkflowInstanceSummary {
                instance_id,
                workflow_id: workflow_id.clone(),
                workflow_name: def.map(|d| d.name.clone()).unwrap_or_default(),
                status,
                completion_pct: 0.0, // Not available from StateStore query yet
                duration_secs: 0,
                started_at: v.get("started_at").and_then(|s| s.as_str()).unwrap_or("").to_string(),
                step_count: 0,
            })
        }).collect();

        return Ok(Json(summaries));
    }

    // Fallback to in-memory map
    let instances = state.workflow_instances()?;
    let registry = state.workflow_registry()?;

    let inst_map = instances.read().await;
    let reg = registry.read().await;

    let mut recent: Vec<_> = inst_map.values().cloned().collect();
    recent.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    recent.truncate(limit);

    let summaries: Vec<WorkflowInstanceSummary> = recent.into_iter().map(|inst| {
        let def = reg.get(&inst.workflow_id);
        WorkflowInstanceSummary {
            instance_id: inst.id.clone(),
            workflow_id: inst.workflow_id.clone(),
            workflow_name: def.map(|d| d.name.clone()).unwrap_or_default(),
            status: inst.status.to_string(),
            completion_pct: inst.completion_pct(),
            duration_secs: inst.duration_secs(),
            started_at: inst.started_at.to_rfc3339(),
            step_count: inst.step_states.len(),
        }
    }).collect();

    Ok(Json(summaries))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn to_workflow_response(def: &beebotos_agents::workflow::WorkflowDefinition) -> WorkflowResponse {
    let triggers: Vec<WorkflowTriggerResponse> = def
        .triggers
        .iter()
        .map(|t| {
            let (trigger_type, detail) = match &t.trigger_type {
                beebotos_agents::workflow::TriggerType::Cron { schedule, timezone } => {
                    (
                        "cron".to_string(),
                        format!("{} (tz: {})", schedule, timezone.as_deref().unwrap_or("UTC")),
                    )
                }
                beebotos_agents::workflow::TriggerType::Event { source, filter } => {
                    (
                        "event".to_string(),
                        format!(
                            "source: {}{}",
                            source,
                            filter.as_ref().map(|f| format!(", filter: {}", f)).unwrap_or_default()
                        ),
                    )
                }
                beebotos_agents::workflow::TriggerType::Webhook { path, method, auth } => {
                    (
                        "webhook".to_string(),
                        format!(
                            "{} {}{}",
                            method,
                            path,
                            auth.as_ref().map(|a| format!(", auth: {}", a)).unwrap_or_default()
                        ),
                    )
                }
                beebotos_agents::workflow::TriggerType::Manual { .. } => {
                    ("manual".to_string(), "triggered via API".to_string())
                }
            };
            WorkflowTriggerResponse {
                trigger_type,
                detail,
            }
        })
        .collect();

    let steps: Vec<WorkflowStepResponse> = def
        .steps
        .iter()
        .map(|s| WorkflowStepResponse {
            id: s.id.clone(),
            name: s.name.clone(),
            skill: s.skill.clone(),
            depends_on: s.depends_on.clone(),
            condition: s.condition.clone(),
        })
        .collect();

    WorkflowResponse {
        id: def.id.clone(),
        name: def.name.clone(),
        description: def.description.clone(),
        version: def.version.clone(),
        author: def.author.clone(),
        tags: def.tags.clone(),
        steps_count: def.steps.len(),
        triggers,
        steps,
    }
}
