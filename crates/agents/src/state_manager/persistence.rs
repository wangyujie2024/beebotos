//! State persistence for AgentStateManager
//!
//! Provides database persistence for agent state records,
//! enabling recovery after restarts and audit trails.

use std::collections::HashMap;

use sqlx::{Row, SqlitePool};
use tracing::{error, info, warn};

use crate::error::AgentError;
use crate::state_manager::{AgentState, AgentStateRecord, AgentStats};

/// State persistence manager
pub struct StatePersistence {
    db: Option<SqlitePool>,
    enable_auto_persist: bool,
    persist_interval_secs: u64,
}

/// 🔧 FIX: Full agent configuration for persistence
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PersistedAgentConfig {
    /// Agent unique identifier
    pub agent_id: String,
    /// Agent name
    pub name: String,
    /// Agent description
    pub description: String,
    /// Agent version
    pub version: String,
    /// Capabilities
    pub capabilities: Vec<String>,
    /// Model configuration
    pub model_config: crate::ModelConfig,
    /// Memory configuration
    pub memory_config: crate::MemoryConfig,
    /// Personality configuration
    pub personality_config: crate::PersonalityConfig,
    /// When the config was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the config was last updated
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl StatePersistence {
    /// Create new persistence manager
    pub fn new(db: Option<SqlitePool>) -> Self {
        Self {
            db,
            enable_auto_persist: true,
            persist_interval_secs: 60,
        }
    }

    /// Disable auto persistence
    pub fn without_auto_persist(mut self) -> Self {
        self.enable_auto_persist = false;
        self
    }

    /// Set persist interval
    pub fn with_interval(mut self, secs: u64) -> Self {
        self.persist_interval_secs = secs;
        self
    }

    /// Check if persistence is available
    pub fn is_available(&self) -> bool {
        self.db.is_some()
    }

    /// Save agent state record to database
    pub async fn save_record(&self, record: &AgentStateRecord) -> Result<(), AgentError> {
        let db = match &self.db {
            Some(db) => db,
            None => return Ok(()),
        };

        let state_str = format!("{:?}", record.state);
        let prev_state_str = record.previous_state.as_ref().map(|s| format!("{:?}", s));
        let metadata_json = serde_json::to_string(&record.metadata)
            .map_err(|e| AgentError::Serialization(e.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO agent_states (
                agent_id, state, previous_state, registered_at, state_changed_at,
                current_task_id, kernel_task_id, last_error, total_tasks,
                successful_tasks, failed_tasks, total_execution_time_ms, metadata
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT (agent_id) DO UPDATE SET
                state = excluded.state,
                previous_state = excluded.previous_state,
                state_changed_at = excluded.state_changed_at,
                current_task_id = excluded.current_task_id,
                kernel_task_id = excluded.kernel_task_id,
                last_error = excluded.last_error,
                total_tasks = excluded.total_tasks,
                successful_tasks = excluded.successful_tasks,
                failed_tasks = excluded.failed_tasks,
                total_execution_time_ms = excluded.total_execution_time_ms,
                metadata = excluded.metadata,
                updated_at = datetime('now')
            "#,
        )
        .bind(&record.agent_id)
        .bind(&state_str)
        .bind(prev_state_str)
        .bind(record.registered_at)
        .bind(record.state_changed_at)
        .bind(&record.current_task_id)
        .bind(record.kernel_task_id.map(|id| id as i64))
        .bind(&record.last_error)
        .bind(record.stats.total_tasks as i64)
        .bind(record.stats.successful_tasks as i64)
        .bind(record.stats.failed_tasks as i64)
        .bind(record.stats.total_execution_time_ms as i64)
        .bind(metadata_json)
        .execute(db)
        .await
        .map_err(|e| {
            error!("Failed to save agent state: {}", e);
            AgentError::Database(e.to_string())
        })?;

        Ok(())
    }

    /// Load agent state record from database
    pub async fn load_record(
        &self,
        agent_id: &str,
    ) -> Result<Option<AgentStateRecord>, AgentError> {
        let db = match &self.db {
            Some(db) => db,
            None => return Ok(None),
        };

        let row = sqlx::query(
            r#"
            SELECT
                agent_id, state, previous_state, registered_at, state_changed_at,
                current_task_id, kernel_task_id, last_error, total_tasks,
                successful_tasks, failed_tasks, total_execution_time_ms, metadata
            FROM agent_states
            WHERE agent_id = ?1
            "#,
        )
        .bind(agent_id)
        .fetch_optional(db)
        .await
        .map_err(|e| {
            error!("Failed to load agent state: {}", e);
            AgentError::Database(e.to_string())
        })?;

        match row {
            Some(row) => {
                let record = Self::row_to_record(row)?;
                Ok(Some(record))
            }
            None => Ok(None),
        }
    }

    /// Load all agent state records
    pub async fn load_all_records(&self) -> Result<Vec<AgentStateRecord>, AgentError> {
        let db = match &self.db {
            Some(db) => db,
            None => return Ok(Vec::new()),
        };

        let rows = sqlx::query(
            r#"
            SELECT
                agent_id, state, previous_state, registered_at, state_changed_at,
                current_task_id, kernel_task_id, last_error, total_tasks,
                successful_tasks, failed_tasks, total_execution_time_ms, metadata
            FROM agent_states
            WHERE state NOT IN ('Stopped', 'Error')
            ORDER BY registered_at DESC
            "#,
        )
        .fetch_all(db)
        .await
        .map_err(|e| {
            error!("Failed to load agent states: {}", e);
            AgentError::Database(e.to_string())
        })?;

        let mut records = Vec::new();
        for row in rows {
            match Self::row_to_record(row) {
                Ok(record) => records.push(record),
                Err(e) => warn!("Failed to parse record: {}", e),
            }
        }

        Ok(records)
    }

    /// Delete agent state record
    pub async fn delete_record(&self, agent_id: &str) -> Result<(), AgentError> {
        let db = match &self.db {
            Some(db) => db,
            None => return Ok(()),
        };

        sqlx::query("DELETE FROM agent_states WHERE agent_id = ?1")
            .bind(agent_id)
            .execute(db)
            .await
            .map_err(|e| {
                error!("Failed to delete agent state: {}", e);
                AgentError::Database(e.to_string())
            })?;

        Ok(())
    }

    /// Convert database row to AgentStateRecord
    fn row_to_record(row: sqlx::sqlite::SqliteRow) -> Result<AgentStateRecord, AgentError> {
        let state_str: String = row
            .try_get("state")
            .map_err(|e| AgentError::Database(e.to_string()))?;
        let state = parse_agent_state(&state_str)?;

        let prev_state_str: Option<String> = row
            .try_get("previous_state")
            .map_err(|e| AgentError::Database(e.to_string()))?;
        let previous_state = prev_state_str
            .map(|s: String| parse_agent_state(s.as_str()))
            .transpose()?;

        let metadata_json: String = row
            .try_get("metadata")
            .map_err(|e| AgentError::Database(e.to_string()))?;
        let metadata: HashMap<String, String> = serde_json::from_str(&metadata_json)
            .map_err(|e| AgentError::Serialization(e.to_string()))?;

        let record = AgentStateRecord {
            agent_id: row
                .try_get("agent_id")
                .map_err(|e| AgentError::Database(e.to_string()))?,
            state,
            previous_state,
            registered_at: row
                .try_get("registered_at")
                .map_err(|e| AgentError::Database(e.to_string()))?,
            state_changed_at: row
                .try_get("state_changed_at")
                .map_err(|e| AgentError::Database(e.to_string()))?,
            current_task_id: row
                .try_get("current_task_id")
                .map_err(|e| AgentError::Database(e.to_string()))?,
            kernel_task_id: row
                .try_get::<Option<i64>, _>("kernel_task_id")
                .map_err(|e| AgentError::Database(e.to_string()))?
                .map(|id| id as u64),
            last_error: row
                .try_get("last_error")
                .map_err(|e| AgentError::Database(e.to_string()))?,
            stats: AgentStats {
                total_tasks: row
                    .try_get::<i64, _>("total_tasks")
                    .map_err(|e| AgentError::Database(e.to_string()))?
                    as u64,
                successful_tasks: row
                    .try_get::<i64, _>("successful_tasks")
                    .map_err(|e| AgentError::Database(e.to_string()))?
                    as u64,
                failed_tasks: row
                    .try_get::<i64, _>("failed_tasks")
                    .map_err(|e| AgentError::Database(e.to_string()))?
                    as u64,
                total_execution_time_ms: row
                    .try_get::<i64, _>("total_execution_time_ms")
                    .map_err(|e| AgentError::Database(e.to_string()))?
                    as u64,
                last_task_at: None,
            },
            metadata,
        };

        Ok(record)
    }

    /// Run database migrations
    pub async fn migrate(&self) -> Result<(), AgentError> {
        let db = match &self.db {
            Some(db) => db,
            None => return Ok(()),
        };

        info!("Running state persistence migrations...");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS agent_states (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                agent_id TEXT UNIQUE NOT NULL,
                state TEXT NOT NULL,
                previous_state TEXT,
                registered_at DATETIME NOT NULL,
                state_changed_at DATETIME NOT NULL,
                current_task_id TEXT,
                kernel_task_id INTEGER,
                last_error TEXT,
                total_tasks INTEGER DEFAULT 0,
                successful_tasks INTEGER DEFAULT 0,
                failed_tasks INTEGER DEFAULT 0,
                total_execution_time_ms INTEGER DEFAULT 0,
                metadata TEXT DEFAULT '{}',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE INDEX IF NOT EXISTS idx_agent_states_state ON agent_states(state);
            CREATE INDEX IF NOT EXISTS idx_agent_states_updated ON agent_states(updated_at);
            "#,
        )
        .execute(db)
        .await
        .map_err(|e| AgentError::Database(e.to_string()))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS agent_state_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                agent_id TEXT NOT NULL,
                from_state TEXT NOT NULL,
                to_state TEXT NOT NULL,
                reason TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE INDEX IF NOT EXISTS idx_state_history_agent ON agent_state_history(agent_id);
            CREATE INDEX IF NOT EXISTS idx_state_history_created ON agent_state_history(created_at);
            "#,
        )
        .execute(db)
        .await
        .map_err(|e| AgentError::Database(e.to_string()))?;

        // 🔧 FIX: Create agent_configs table for full configuration persistence
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS agent_configs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                agent_id TEXT UNIQUE NOT NULL,
                name TEXT NOT NULL,
                description TEXT,
                version TEXT NOT NULL,
                capabilities TEXT DEFAULT '[]',
                model_config TEXT NOT NULL,
                memory_config TEXT NOT NULL,
                personality_config TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE INDEX IF NOT EXISTS idx_agent_configs_agent_id ON agent_configs(agent_id);
            "#,
        )
        .execute(db)
        .await
        .map_err(|e| AgentError::Database(e.to_string()))?;

        info!("State persistence migrations complete");
        Ok(())
    }

    /// 🔧 FIX: Save full agent configuration
    pub async fn save_config(&self, config: &PersistedAgentConfig) -> Result<(), AgentError> {
        let db = match &self.db {
            Some(db) => db,
            None => return Ok(()),
        };

        let capabilities_json = serde_json::to_string(&config.capabilities)
            .map_err(|e| AgentError::Serialization(e.to_string()))?;
        let model_config_json = serde_json::to_string(&config.model_config)
            .map_err(|e| AgentError::Serialization(e.to_string()))?;
        let memory_config_json = serde_json::to_string(&config.memory_config)
            .map_err(|e| AgentError::Serialization(e.to_string()))?;
        let personality_config_json = serde_json::to_string(&config.personality_config)
            .map_err(|e| AgentError::Serialization(e.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO agent_configs (
                agent_id, name, description, version, capabilities,
                model_config, memory_config, personality_config, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT (agent_id) DO UPDATE SET
                name = excluded.name,
                description = excluded.description,
                version = excluded.version,
                capabilities = excluded.capabilities,
                model_config = excluded.model_config,
                memory_config = excluded.memory_config,
                personality_config = excluded.personality_config,
                updated_at = datetime('now')
            "#,
        )
        .bind(&config.agent_id)
        .bind(&config.name)
        .bind(&config.description)
        .bind(&config.version)
        .bind(&capabilities_json)
        .bind(&model_config_json)
        .bind(&memory_config_json)
        .bind(&personality_config_json)
        .bind(config.created_at)
        .bind(config.updated_at)
        .execute(db)
        .await
        .map_err(|e| {
            error!("Failed to save agent config: {}", e);
            AgentError::Database(e.to_string())
        })?;

        // 🔧 FIX: Also ensure the agent exists in the `agents` table to satisfy FK
        // constraints (e.g. agent_channel_bindings.agent_id REFERENCES
        // agents(id))
        let model_provider = config.model_config.provider.clone();
        let model_name = config.model_config.model.clone();
        sqlx::query(
            r#"
            INSERT INTO agents (
                id, name, description, status, capabilities,
                model_provider, model_name, created_at, updated_at
            ) VALUES (?1, ?2, ?3, 'active', ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT (id) DO UPDATE SET
                name = excluded.name,
                description = excluded.description,
                status = excluded.status,
                capabilities = excluded.capabilities,
                model_provider = excluded.model_provider,
                model_name = excluded.model_name,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&config.agent_id)
        .bind(&config.name)
        .bind(&config.description)
        .bind(&capabilities_json)
        .bind(&model_provider)
        .bind(&model_name)
        .bind(config.created_at)
        .bind(config.updated_at)
        .execute(db)
        .await
        .map_err(|e| {
            error!("Failed to save agent to agents table: {}", e);
            AgentError::Database(e.to_string())
        })?;

        info!(
            "Agent config and agents table record saved for {}",
            config.agent_id
        );
        Ok(())
    }

    /// 🔧 FIX: Fast-sync only the agents table (used during recovery to satisfy
    /// FK constraints without the overhead of full save_config which
    /// updates agent_configs too).
    pub async fn sync_agents_table(&self, config: &PersistedAgentConfig) -> Result<(), AgentError> {
        let db = match &self.db {
            Some(db) => db,
            None => return Ok(()),
        };

        let capabilities_json = serde_json::to_string(&config.capabilities)
            .map_err(|e| AgentError::Serialization(e.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO agents (id, name, description, status, capabilities, model_provider, model_name, created_at, updated_at)
            VALUES (?1, ?2, ?3, 'active', ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                description = excluded.description,
                status = excluded.status,
                capabilities = excluded.capabilities,
                model_provider = excluded.model_provider,
                model_name = excluded.model_name,
                updated_at = excluded.updated_at
            "#
        )
        .bind(&config.agent_id)
        .bind(&config.name)
        .bind(&config.description)
        .bind(&capabilities_json)
        .bind(&config.model_config.provider)
        .bind(&config.model_config.model)
        .bind(config.created_at)
        .bind(config.updated_at)
        .execute(db)
        .await
        .map_err(|e| AgentError::Database(e.to_string()))?;

        Ok(())
    }

    /// 🔧 FIX: Load full agent configuration
    pub async fn load_config(
        &self,
        agent_id: &str,
    ) -> Result<Option<PersistedAgentConfig>, AgentError> {
        let db = match &self.db {
            Some(db) => db,
            None => return Ok(None),
        };

        let row = sqlx::query(
            r#"
            SELECT
                agent_id, name, description, version, capabilities,
                model_config, memory_config, personality_config, created_at, updated_at
            FROM agent_configs
            WHERE agent_id = ?1
            "#,
        )
        .bind(agent_id)
        .fetch_optional(db)
        .await
        .map_err(|e| {
            error!("Failed to load agent config: {}", e);
            AgentError::Database(e.to_string())
        })?;

        match row {
            Some(row) => {
                let config = Self::row_to_config(row)?;
                Ok(Some(config))
            }
            None => Ok(None),
        }
    }

    /// 🔧 FIX: Convert database row to PersistedAgentConfig
    fn row_to_config(row: sqlx::sqlite::SqliteRow) -> Result<PersistedAgentConfig, AgentError> {
        let capabilities_json: String = row
            .try_get("capabilities")
            .map_err(|e| AgentError::Database(e.to_string()))?;
        let capabilities: Vec<String> = serde_json::from_str(&capabilities_json)
            .map_err(|e| AgentError::Serialization(e.to_string()))?;

        let model_config_json: String = row
            .try_get("model_config")
            .map_err(|e| AgentError::Database(e.to_string()))?;
        let model_config: crate::ModelConfig = serde_json::from_str(&model_config_json)
            .map_err(|e| AgentError::Serialization(e.to_string()))?;

        let memory_config_json: String = row
            .try_get("memory_config")
            .map_err(|e| AgentError::Database(e.to_string()))?;
        let memory_config: crate::MemoryConfig = serde_json::from_str(&memory_config_json)
            .map_err(|e| AgentError::Serialization(e.to_string()))?;

        let personality_config_json: String = row
            .try_get("personality_config")
            .map_err(|e| AgentError::Database(e.to_string()))?;
        let personality_config: crate::PersonalityConfig =
            serde_json::from_str(&personality_config_json)
                .map_err(|e| AgentError::Serialization(e.to_string()))?;

        Ok(PersistedAgentConfig {
            agent_id: row
                .try_get("agent_id")
                .map_err(|e| AgentError::Database(e.to_string()))?,
            name: row
                .try_get("name")
                .map_err(|e| AgentError::Database(e.to_string()))?,
            description: row
                .try_get("description")
                .map_err(|e| AgentError::Database(e.to_string()))?,
            version: row
                .try_get("version")
                .map_err(|e| AgentError::Database(e.to_string()))?,
            capabilities,
            model_config,
            memory_config,
            personality_config,
            created_at: row
                .try_get("created_at")
                .map_err(|e| AgentError::Database(e.to_string()))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| AgentError::Database(e.to_string()))?,
        })
    }
}

impl Default for StatePersistence {
    fn default() -> Self {
        Self::new(None)
    }
}

/// Parse agent state from string
fn parse_agent_state(s: &str) -> Result<AgentState, AgentError> {
    match s {
        "Registered" => Ok(AgentState::Registered),
        "Initializing" => Ok(AgentState::Initializing),
        "Idle" => Ok(AgentState::Idle),
        s if s.starts_with("Working") => Ok(AgentState::Working {
            task_id: String::new(),
        }),
        "Paused" => Ok(AgentState::Paused),
        "ShuttingDown" => Ok(AgentState::ShuttingDown),
        "Stopped" => Ok(AgentState::Stopped),
        s if s.starts_with("Error") => Ok(AgentState::Error {
            message: s.to_string(),
        }),
        _ => Err(AgentError::InvalidConfig(format!("Unknown state: {}", s))),
    }
}
