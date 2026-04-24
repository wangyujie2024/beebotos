//! Agent Business Service
//!
//! Orchestrates Agent lifecycle operations between:
//! - Database persistence
//! - Kernel execution (sandboxed agent runtime)
//! - WebSocket notifications
//!
//! This service is the single entry point for all agent lifecycle operations,
//! properly layered between HTTP handlers (gateway) and kernel
//! (infrastructure).

use std::sync::Arc;

use sqlx::SqlitePool;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::config::BeeBotOSConfig;
use crate::error::AppError;
use crate::models::{AgentRecord, CreateAgentRequest};
use crate::services::agent_runtime_manager::AgentRuntimeManager;
use crate::services::task_monitor::TaskMonitorService;

/// Kernel task information for an agent
#[derive(Debug, Clone)]
pub struct AgentKernelInfo {
    /// Kernel task ID
    pub task_id: beebotos_kernel::TaskId,
    /// Capability set assigned to the agent
    pub capability_set: beebotos_kernel::capabilities::CapabilitySet,
}

/// Agent service for business logic
///
/// This service manages the complete agent lifecycle:
/// 1. Database persistence (SQLite)
/// 2. Kernel sandbox execution (beebotos-kernel)
/// 3. Status tracking and heartbeat
pub struct AgentService {
    db: SqlitePool,
    kernel: Arc<beebotos_kernel::Kernel>,
    runtime_manager: Arc<AgentRuntimeManager>,
    #[allow(dead_code)]
    config: BeeBotOSConfig,
}

impl std::fmt::Debug for AgentService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentService")
            .field("db", &"<SqlitePool>")
            .field("kernel", &"<Kernel>")
            .field("runtime_manager", &"<AgentRuntimeManager>")
            .finish()
    }
}

impl AgentService {
    /// Create new agent service with kernel and runtime manager integration
    pub fn new(
        db: SqlitePool,
        kernel: Arc<beebotos_kernel::Kernel>,
        runtime_manager: Arc<AgentRuntimeManager>,
        config: BeeBotOSConfig,
    ) -> Self {
        Self {
            db,
            kernel,
            runtime_manager,
            config,
        }
    }

    /// Create agent and spawn in kernel sandbox
    ///
    /// This is the main entry point for agent creation. It:
    /// 1. Creates the database record
    /// 2. Creates capability set based on agent configuration
    /// 3. Spawns the agent task in kernel sandbox (with monitoring)
    /// 4. Registers the agent runtime with `beebotos_agents`
    ///
    /// 🔒 P0 FIX: Uses TaskMonitorService for kernel fault awareness
    pub async fn create_and_spawn(
        &self,
        request: CreateAgentRequest,
        owner_id: &str,
        task_monitor: Option<&TaskMonitorService>,
    ) -> Result<(AgentRecord, AgentKernelInfo), AppError> {
        info!(
            "Creating and spawning agent '{}' for owner {}",
            request.name, owner_id
        );

        // Step 1: Create database record
        let agent_record = self.create_agent_in_db(&request, owner_id).await?;
        let agent_id = agent_record.id;

        // Step 2: Create capability set based on agent capabilities
        let capability_set = self.create_capability_set(&agent_record)?;

        // Step 3: Spawn agent task in kernel sandbox (with monitoring)
        let task_id = self
            .spawn_agent_task(agent_id, capability_set.clone(), task_monitor)
            .await?;

        // Step 4: Register agent runtime with beebotos_agents
        let agent_id_str = agent_id.to_string();
        if let Err(e) = self
            .runtime_manager
            .register_agent(&agent_id_str, &agent_record)
            .await
        {
            error!(
                "Failed to register agent runtime for {}: {}",
                agent_id_str, e
            );
            // We don't fail the whole operation; the DB/kernel parts succeeded.
        }

        // Step 5: Update status to initializing
        self.update_status(&agent_id.to_string(), "initializing")
            .await?;

        // Step 6: Record status change
        self.record_status_change(agent_id, "initializing", "Kernel sandbox created")
            .await?;

        // Step 7: Log creation
        self.add_log(
            &agent_id.to_string(),
            "info",
            "Agent created and spawned in kernel sandbox",
            Some("lifecycle"),
        )
        .await
        .ok();

        info!(
            "Agent {} created and spawned in kernel sandbox (task_id: {})",
            agent_id, task_id
        );

        let kernel_info = AgentKernelInfo {
            task_id,
            capability_set,
        };

        Ok((agent_record, kernel_info))
    }

    /// Start (or restart) an agent by respawning its kernel task
    ///
    /// This is used when a previously stopped agent is started again.
    ///
    /// 🔒 P0 FIX: Uses TaskMonitorService for kernel fault awareness
    pub async fn start_agent(
        &self,
        agent_id: &str,
        task_monitor: Option<&TaskMonitorService>,
    ) -> Result<AgentKernelInfo, AppError> {
        info!("Starting agent {}", agent_id);

        // Step 1: Fetch agent record from database
        let agent_record = self
            .get_agent(agent_id)
            .await?
            .ok_or_else(|| AppError::not_found("Agent", agent_id))?;

        // Step 2: Create capability set
        let capability_set = self.create_capability_set(&agent_record)?;

        // Step 3: Spawn agent task in kernel sandbox (with monitoring)
        let agent_uuid = Uuid::parse_str(agent_id)
            .map_err(|_| AppError::Internal("Invalid agent ID".to_string()))?;
        let task_id = self
            .spawn_agent_task(agent_uuid, capability_set.clone(), task_monitor)
            .await?;

        // Step 4: Register agent runtime with beebotos_agents
        if let Err(e) = self
            .runtime_manager
            .register_agent(agent_id, &agent_record)
            .await
        {
            error!("Failed to register agent runtime for {}: {}", agent_id, e);
        }

        // Step 5: Update status to running
        self.update_status(agent_id, "running").await?;

        // Step 6: Record status change
        self.record_status_change(agent_uuid, "running", "User initiated start")
            .await?;

        // Step 7: Log start
        self.add_log(agent_id, "info", "Agent started", Some("lifecycle"))
            .await
            .ok();

        info!(
            "Agent {} started successfully (task_id: {})",
            agent_id, task_id
        );

        Ok(AgentKernelInfo {
            task_id,
            capability_set,
        })
    }

    /// Stop agent via kernel
    ///
    /// Hard-cancels the kernel task and updates database status
    pub async fn stop_agent(
        &self,
        agent_id: &str,
        kernel_info: Option<&AgentKernelInfo>,
    ) -> Result<(), AppError> {
        info!("Stopping agent {}", agent_id);

        // Step 1: Hard-cancel kernel task if task_id is known
        if let Some(info) = kernel_info {
            let cancelled = self.kernel.cancel_task(info.task_id).await;
            if cancelled {
                info!(
                    "Hard-cancelled kernel task {} for agent {}",
                    info.task_id, agent_id
                );
            } else {
                warn!(
                    "Kernel task {} for agent {} not found for cancellation",
                    info.task_id, agent_id
                );
            }
        }

        // Step 2: Unregister agent runtime
        self.runtime_manager.unregister_agent(agent_id).await;

        // Step 3: Update database status
        self.update_status(agent_id, "stopped").await?;

        // Step 4: Record status change
        let uuid = Uuid::parse_str(agent_id)
            .map_err(|_| AppError::Internal("Invalid agent ID".to_string()))?;
        self.record_status_change(uuid, "stopped", "User initiated stop")
            .await?;

        // Step 5: Log stop
        self.add_log(agent_id, "info", "Agent stopped", Some("lifecycle"))
            .await
            .ok();

        info!("Agent {} stopped successfully", agent_id);
        Ok(())
    }

    /// Delete agent with cleanup
    ///
    /// Hard-cancels the kernel task, unregisters runtime, and deletes from DB
    pub async fn delete_agent(
        &self,
        agent_id: &str,
        kernel_info: Option<&AgentKernelInfo>,
    ) -> Result<(), AppError> {
        info!("Deleting agent {}", agent_id);

        // Step 1: Hard-cancel kernel task if task_id is known
        if let Some(info) = kernel_info {
            let cancelled = self.kernel.cancel_task(info.task_id).await;
            if cancelled {
                info!(
                    "Hard-cancelled kernel task {} for agent {}",
                    info.task_id, agent_id
                );
            } else {
                warn!(
                    "Kernel task {} for agent {} not found for cancellation",
                    info.task_id, agent_id
                );
            }
        }

        // Step 2: Unregister agent runtime
        self.runtime_manager.unregister_agent(agent_id).await;

        // Step 3: Delete from database
        let result = sqlx::query("DELETE FROM agents WHERE id = ?1")
            .bind(agent_id.to_string())
            .execute(&self.db)
            .await
            .map_err(|e| {
                error!("Failed to delete agent: {}", e);
                AppError::database(e)
            })?;

        if result.rows_affected() == 0 {
            return Err(AppError::not_found("Agent", agent_id));
        }

        // Step 4: Log deletion
        self.add_log(agent_id, "info", "Agent deleted", Some("lifecycle"))
            .await
            .ok();

        info!("Agent {} deleted", agent_id);
        Ok(())
    }

    /// Create agent in database (internal)
    async fn create_agent_in_db(
        &self,
        request: &CreateAgentRequest,
        owner_id: &str,
    ) -> Result<AgentRecord, AppError> {
        let id = Uuid::new_v4();
        let capabilities_json = serde_json::to_string(&request.capabilities)
            .map_err(|e| AppError::Internal(format!("Failed to serialize capabilities: {}", e)))?;
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO agents (
                id, name, description, status, capabilities,
                model_provider, model_name, owner_id, metadata,
                created_at, updated_at
            ) VALUES (
                ?1, ?2, ?3, 'pending', ?4,
                ?5, ?6, ?7, '{}',
                ?8, ?8
            )
            "#,
        )
        .bind(id.to_string())
        .bind(&request.name)
        .bind(&request.description)
        .bind(capabilities_json)
        .bind(&request.model_provider)
        .bind(&request.model_name)
        .bind(owner_id)
        .bind(&now)
        .execute(&self.db)
        .await
        .map_err(|e| {
            error!("Failed to create agent: {}", e);
            AppError::database(e)
        })?;

        // Fetch the created record
        let row: crate::models::AgentRecordRow =
            sqlx::query_as("SELECT * FROM agents WHERE id = ?1")
                .bind(id.to_string())
                .fetch_one(&self.db)
                .await
                .map_err(|e| AppError::database(e))?;

        let agent_record = row.try_into().map_err(|e: String| {
            AppError::Internal(format!("Failed to parse agent record: {}", e))
        })?;

        Ok(agent_record)
    }

    /// Create capability set for agent
    ///
    /// 🔒 P1 FIX: Type-safe capability mapping using AgentCapabilitySet
    fn create_capability_set(
        &self,
        agent: &AgentRecord,
    ) -> Result<beebotos_kernel::capabilities::CapabilitySet, AppError> {
        use crate::capability::AgentCapabilitySet;

        // Parse capabilities from database strings
        let capability_set: AgentCapabilitySet = agent.capabilities.clone().into();

        // Convert to kernel CapabilitySet with type-safe mapping
        let kernel_caps = capability_set.to_kernel_capability_set();

        info!(
            "Created capability set for agent {} with {} capabilities",
            agent.id,
            capability_set.capabilities.len()
        );

        Ok(kernel_caps)
    }

    /// Spawn agent task in kernel sandbox with monitoring
    ///
    /// 🔒 P0 FIX: Integrated with TaskMonitorService for fault awareness
    async fn spawn_agent_task(
        &self,
        agent_id: Uuid,
        capability_set: beebotos_kernel::capabilities::CapabilitySet,
        task_monitor: Option<&TaskMonitorService>,
    ) -> Result<beebotos_kernel::TaskId, AppError> {
        let db = self.db.clone();
        let agent_id_clone = agent_id.to_string();

        // Create completion and failure callbacks
        let on_complete = Box::new(move || {
            info!("Agent {} task completed callback", agent_id_clone);
        });

        let agent_id_for_failure = agent_id.to_string();
        let on_failure = Box::new(move |error: String| {
            error!("Agent {} task failed: {}", agent_id_for_failure, error);
        });

        if let Some(monitor) = task_monitor {
            // Use monitored spawn
            let handle = monitor
                .spawn_and_monitor(
                    agent_id.to_string(),
                    format!("agent-{}", agent_id),
                    beebotos_kernel::Priority::Normal,
                    capability_set,
                    move || async move { Self::run_agent_task(agent_id, db).await },
                    Some(on_complete),
                    Some(on_failure),
                )
                .await?;

            Ok(handle.task_id)
        } else {
            // Fallback to direct spawn
            let task_id = self
                .kernel
                .spawn_task(
                    format!("agent-{}", agent_id),
                    beebotos_kernel::Priority::Normal,
                    capability_set,
                    async move { Self::run_agent_task(agent_id, db).await },
                )
                .await
                .map_err(|e| {
                    error!("Failed to spawn agent task in kernel: {}", e);
                    AppError::kernel(format!("Kernel spawn failed: {}", e))
                })?;

            Ok(task_id)
        }
    }

    /// Agent task main loop (runs in kernel sandbox)
    async fn run_agent_task(agent_id: Uuid, db: SqlitePool) -> beebotos_kernel::Result<()> {
        use tracing::{error, info, warn};

        info!("Agent {} sandboxed task started", agent_id);

        // Simulate initialization
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Update status to running
        let _ = sqlx::query(
            "UPDATE agents SET status = 'running', updated_at = datetime('now') WHERE id = ?1",
        )
        .bind(agent_id.to_string())
        .execute(&db)
        .await;

        // Record status change
        let _ = sqlx::query(
            "INSERT INTO agent_status_history (agent_id, status, reason) VALUES (?1, ?2, ?3)",
        )
        .bind(agent_id.to_string())
        .bind("running")
        .bind("Kernel sandbox initialization complete")
        .execute(&db)
        .await;

        // Main heartbeat loop
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        let mut consecutive_errors = 0;

        loop {
            interval.tick().await;

            // Check if agent has been stopped
            let status_result: Result<Option<String>, sqlx::Error> =
                sqlx::query_scalar("SELECT status FROM agents WHERE id = ?1")
                    .bind(agent_id.to_string())
                    .fetch_optional(&db)
                    .await;

            match status_result {
                Ok(Some(status)) if status == "stopped" => {
                    info!("Agent {} stopped, exiting kernel task", agent_id);
                    break;
                }
                Ok(None) => {
                    info!("Agent {} deleted, exiting kernel task", agent_id);
                    break;
                }
                Err(e) => {
                    consecutive_errors += 1;
                    warn!("Agent {} heartbeat query failed: {}", agent_id, e);
                    if consecutive_errors >= 3 {
                        error!("Agent {} too many errors, exiting", agent_id);
                        break;
                    }
                }
                _ => {
                    consecutive_errors = 0;
                }
            }

            // Update heartbeat
            let result = sqlx::query(
                "UPDATE agents SET last_heartbeat = datetime('now') WHERE id = ?1 AND status = \
                 'running'",
            )
            .bind(agent_id.to_string())
            .execute(&db)
            .await;

            if let Err(e) = result {
                consecutive_errors += 1;
                warn!("Agent {} heartbeat update failed: {}", agent_id, e);
            } else {
                consecutive_errors = 0;
            }
        }

        info!("Agent {} kernel sandbox task stopped", agent_id);
        Ok(())
    }

    /// Record status change in history table
    async fn record_status_change(
        &self,
        agent_id: Uuid,
        status: &str,
        reason: &str,
    ) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO agent_status_history (agent_id, status, reason) VALUES (?1, ?2, ?3)",
        )
        .bind(agent_id.to_string())
        .bind(status)
        .bind(reason)
        .execute(&self.db)
        .await
        .map_err(|e| {
            error!("Failed to record status change: {}", e);
            AppError::database(e)
        })?;

        Ok(())
    }

    /// Create agent in database (public for backward compatibility)
    /// Create agent record in database (public for backward compatibility)
    pub async fn create_agent(
        &self,
        request: CreateAgentRequest,
        owner_id: &str,
    ) -> Result<AgentRecord, AppError> {
        self.create_agent_in_db(&request, owner_id).await
    }

    /// List agents for user (legacy method, use list_agents_with_count for
    /// better performance)
    pub async fn list_agents(
        &self,
        owner_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<AgentRecord>, AppError> {
        let rows: Vec<crate::models::AgentRecordRow> = if let Some(owner) = owner_id {
            sqlx::query_as::<_, crate::models::AgentRecordRow>(
                "SELECT * FROM agents WHERE owner_id = ?1 ORDER BY created_at DESC LIMIT ?2 \
                 OFFSET ?3",
            )
            .bind(owner)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.db)
            .await
        } else {
            sqlx::query_as::<_, crate::models::AgentRecordRow>(
                "SELECT * FROM agents ORDER BY created_at DESC LIMIT ?1 OFFSET ?2",
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.db)
            .await
        }
        .map_err(|e| {
            error!("Failed to list agents: {}", e);
            AppError::database(e)
        })?;

        let agents: Result<Vec<AgentRecord>, AppError> = rows
            .into_iter()
            .map(|row| {
                row.try_into().map_err(|e: String| {
                    AppError::Internal(format!("Failed to parse agent: {}", e))
                })
            })
            .collect();

        agents
    }

    /// List agents with total count (optimized to use single query with window
    /// function)
    pub async fn list_agents_with_count(
        &self,
        owner_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<AgentRecord>, i64), AppError> {
        // First, get total count in a separate query (still better than N+1)
        let total: i64 = if let Some(owner) = owner_id {
            sqlx::query_scalar("SELECT COUNT(*) FROM agents WHERE owner_id = ?1")
                .bind(owner)
                .fetch_one(&self.db)
                .await
        } else {
            sqlx::query_scalar("SELECT COUNT(*) FROM agents")
                .fetch_one(&self.db)
                .await
        }
        .map_err(|e| {
            error!("Failed to count agents: {}", e);
            AppError::database(e)
        })?;

        // Then get the actual agents
        let agents = self.list_agents(owner_id, limit, offset).await?;

        Ok((agents, total))
    }

    /// Get agent by ID
    pub async fn get_agent(&self, agent_id: &str) -> Result<Option<AgentRecord>, AppError> {
        let row: Option<crate::models::AgentRecordRow> =
            sqlx::query_as::<_, crate::models::AgentRecordRow>(
                "SELECT * FROM agents WHERE id = ?1",
            )
            .bind(agent_id.to_string())
            .fetch_optional(&self.db)
            .await
            .map_err(|e| {
                error!("Failed to get agent: {}", e);
                AppError::database(e)
            })?;

        match row {
            Some(row) => {
                let agent: AgentRecord = row.try_into().map_err(|e: String| {
                    AppError::Internal(format!("Failed to parse agent: {}", e))
                })?;
                Ok(Some(agent))
            }
            None => Ok(None),
        }
    }

    /// Update agent status
    pub async fn update_status(&self, agent_id: &str, status: &str) -> Result<(), AppError> {
        sqlx::query("UPDATE agents SET status = ?1, updated_at = datetime('now') WHERE id = ?2")
            .bind(status)
            .bind(agent_id.to_string())
            .execute(&self.db)
            .await
            .map_err(|e| {
                error!("Failed to update agent status: {}", e);
                AppError::database(e)
            })?;

        debug!("Agent {} status updated to {}", agent_id, status);
        Ok(())
    }

    /// Update agent fields (name, description, capabilities, model_provider,
    /// model_name)
    pub async fn update_agent(
        &self,
        agent_id: &str,
        request: &crate::models::UpdateAgentRequest,
    ) -> Result<AgentRecord, AppError> {
        info!("Updating agent {}", agent_id);

        // Build dynamic query parts
        let mut set_parts = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        if let Some(name) = &request.name {
            set_parts.push("name = ?".to_string());
            binds.push(name.clone());
        }
        if let Some(description) = &request.description {
            set_parts.push("description = ?".to_string());
            binds.push(description.clone());
        }
        if let Some(status) = &request.status {
            set_parts.push("status = ?".to_string());
            binds.push(status.clone());
        }
        if let Some(caps) = &request.capabilities {
            let caps_json = serde_json::to_string(caps).map_err(|e| {
                AppError::Internal(format!("Failed to serialize capabilities: {}", e))
            })?;
            set_parts.push("capabilities = ?".to_string());
            binds.push(caps_json);
        }
        if let Some(provider) = &request.model_provider {
            set_parts.push("model_provider = ?".to_string());
            binds.push(provider.clone());
        }
        if let Some(model) = &request.model_name {
            set_parts.push("model_name = ?".to_string());
            binds.push(model.clone());
        }

        if set_parts.is_empty() {
            return self
                .get_agent(agent_id)
                .await?
                .ok_or_else(|| AppError::not_found("Agent", agent_id));
        }

        set_parts.push("updated_at = datetime('now')".to_string());

        // Build the full SQL. SQLite doesn't support bind positions in column names,
        // but all our binds are VALUES so positional ? works fine.
        let sql = format!(
            "UPDATE agents SET {} WHERE id = ?{}",
            set_parts.join(", "),
            binds.len() + 1
        );

        let mut query = sqlx::query(&sql);
        for val in &binds {
            query = query.bind(val);
        }
        query = query.bind(agent_id.to_string());

        query.execute(&self.db).await.map_err(|e| {
            error!("Failed to update agent: {}", e);
            AppError::database(e)
        })?;

        info!("Agent {} updated", agent_id);

        // Log update
        self.add_log(
            agent_id,
            "info",
            "Agent configuration updated",
            Some("lifecycle"),
        )
        .await
        .ok();

        // Return updated record
        self.get_agent(agent_id)
            .await?
            .ok_or_else(|| AppError::not_found("Agent", agent_id))
    }

    /// Add a log entry for an agent
    pub async fn add_log(
        &self,
        agent_id: &str,
        level: &str,
        message: &str,
        source: Option<&str>,
    ) -> Result<(), AppError> {
        sqlx::query(
            "INSERT INTO agent_logs (agent_id, level, message, source, timestamp)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))",
        )
        .bind(agent_id)
        .bind(level)
        .bind(message)
        .bind(source)
        .execute(&self.db)
        .await
        .map_err(|e| {
            error!("Failed to add agent log: {}", e);
            AppError::database(e)
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // Tests would require a test database
    // #[tokio::test]
    // async fn test_create_agent() {
    //     let service = AgentService::new(/* test pool */);
    //     // ...
    // }
}
