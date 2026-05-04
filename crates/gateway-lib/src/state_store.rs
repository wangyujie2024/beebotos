//! Unified State Store
//!
//! Implements CQRS (Command Query Responsibility Segregation) pattern for
//! agent state management. Provides:
//! - Single source of truth for agent states
//! - Event sourcing for audit trail
//! - Read/write separation for performance
//! - Cache consistency guarantees
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                        StateStore                               │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
//! │  │   Command   │  │    Event    │  │         Query           │  │
//! │  │   Handler   │──►│   Store     │◄─│        Handler          │  │
//! │  └─────────────┘  └──────┬──────┘  └─────────────────────────┘  │
//! │                          │                                      │
//! │                          ▼                                      │
//! │                   ┌─────────────┐                               │
//! │                   │  SQLite     │                               │
//! │                   │  (Event Log)│                               │
//! │                   └─────────────┘                               │
//! │                          │                                      │
//! │                          ▼                                      │
//! │                   ┌─────────────┐                               │
//! │                   │    Cache    │                               │
//! │                   │  (Current)  │                               │
//! │                   └─────────────┘                               │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use beebotos_gateway_lib::state_store::{StateStore, StateCommand, StateQuery, AgentState};
//!
//! async fn example(db_pool: sqlx::SqlitePool, config: StateStoreConfig) -> Result<(), Box<dyn std::error::Error>> {
//!     let store = StateStore::new(db_pool, config).await?;
//!
//!     // Command: Change state
//!     store.execute(StateCommand::Transition {
//!         agent_id: "agent-1".to_string(),
//!         from: AgentState::Idle,
//!         to: AgentState::Working,
//!         reason: Some("Starting task".to_string()),
//!     }).await?;
//!
//!     // Query: Get current state
//!     let state = store.query(StateQuery::GetState {
//!         agent_id: "agent-1".to_string(),
//!     }).await?;
//!     Ok(())
//! }
//! ```

use std::collections::HashMap;


use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info};
use uuid::Uuid;

use crate::agent_runtime::{AgentConfig, AgentId, AgentState, TaskId};
// 🟢 P1 FIX: Import unified error types alongside GatewayError
use crate::error::{GatewayError, Result};


/// State event - immutable record of state changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateEvent {
    /// Event ID
    pub event_id: String,
    /// Agent ID
    pub agent_id: AgentId,
    /// Event type
    pub event_type: StateEventType,
    /// Event payload
    pub payload: serde_json::Value,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Correlation ID for tracing
    pub correlation_id: String,
    /// Sequence number (for ordering)
    pub sequence: u64,
}

/// State event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateEventType {
    /// Agent registered
    AgentRegistered,
    /// State transitioned
    StateTransitioned,
    /// Task assigned
    TaskAssigned,
    /// Task completed
    TaskCompleted,
    /// Agent configuration updated
    ConfigUpdated,
    /// Agent metadata updated
    MetadataUpdated,
    /// Agent archived
    AgentArchived,
}

/// State command - write operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateCommand {
    /// Register a new agent
    RegisterAgent {
        agent_id: AgentId,
        config: AgentConfig,
        metadata: HashMap<String, String>,
    },
    /// Transition agent state
    Transition {
        agent_id: AgentId,
        from: AgentState,
        to: AgentState,
        reason: Option<String>,
    },
    /// Assign task to agent
    AssignTask {
        agent_id: AgentId,
        task_id: TaskId,
    },
    /// Complete task
    CompleteTask {
        agent_id: AgentId,
        task_id: TaskId,
        success: bool,
        result: Option<serde_json::Value>,
    },
    /// Update configuration
    UpdateConfig {
        agent_id: AgentId,
        config: AgentConfig,
    },
    /// Update metadata
    UpdateMetadata {
        agent_id: AgentId,
        metadata: HashMap<String, String>,
    },
    /// Archive agent
    ArchiveAgent {
        agent_id: AgentId,
    },
}

/// State query - read operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateQuery {
    /// Get current state
    GetState { agent_id: AgentId },
    /// Get agent info
    GetAgentInfo { agent_id: AgentId },
    /// List all agents
    ListAgents {
        filter: Option<AgentFilter>,
        limit: usize,
        offset: usize,
    },
    /// Get event history
    GetEventHistory {
        agent_id: AgentId,
        from_sequence: Option<u64>,
        limit: usize,
    },
    /// Get state at specific time
    GetStateAt {
        agent_id: AgentId,
        timestamp: DateTime<Utc>,
    },
    /// List workflow instances
    ListWorkflowInstances {
        status: Option<String>,
        limit: usize,
    },
    /// Get a single workflow instance
    GetWorkflowInstance {
        instance_id: String,
    },
}

/// Filter for listing agents
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentFilter {
    /// Filter by state
    pub state: Option<AgentState>,
    /// Filter by capability
    pub has_capability: Option<String>,
    /// Filter by creation time (after)
    pub created_after: Option<DateTime<Utc>>,
    /// Filter by creation time (before)
    pub created_before: Option<DateTime<Utc>>,
}

/// Query result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueryResult {
    /// Current state
    State {
        agent_id: AgentId,
        state: AgentState,
        since: DateTime<Utc>,
        metadata: HashMap<String, String>,
    },
    /// Full agent info
    AgentInfo {
        agent_id: AgentId,
        config: AgentConfig,
        current_state: AgentState,
        metadata: HashMap<String, String>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
        task_count: u64,
        success_count: u64,
        failure_count: u64,
    },
    /// List of agents
    AgentList {
        agents: Vec<AgentInfo>,
        total: usize,
        limit: usize,
        offset: usize,
    },
    /// Event history
    EventHistory {
        agent_id: AgentId,
        events: Vec<StateEvent>,
        next_sequence: Option<u64>,
    },
    /// State at specific time
    StateSnapshot {
        agent_id: AgentId,
        state: AgentState,
        timestamp: DateTime<Utc>,
    },
    /// Workflow instance list
    WorkflowInstanceList {
        instances: Vec<serde_json::Value>,
        total: usize,
    },
    /// Single workflow instance
    WorkflowInstance {
        instance: Option<serde_json::Value>,
    },
}

/// Agent info struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    /// Agent ID
    pub agent_id: AgentId,
    /// Agent configuration
    pub config: AgentConfig,
    /// Current state
    pub current_state: AgentState,
    /// Metadata
    pub metadata: HashMap<String, String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
    /// Total tasks executed
    pub task_count: u64,
    /// Successful tasks
    pub success_count: u64,
    /// Failed tasks
    pub failure_count: u64,
}

/// State store configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateStoreConfig {
    /// Enable event sourcing
    pub event_sourcing: bool,
    /// Cache TTL in seconds
    pub cache_ttl_secs: u64,
    /// Max cache entries
    pub max_cache_entries: usize,
    /// Event retention days
    pub event_retention_days: u32,
    /// Enable audit logging
    pub audit_logging: bool,
}

impl Default for StateStoreConfig {
    fn default() -> Self {
        Self {
            event_sourcing: true,
            cache_ttl_secs: 300, // 5 minutes
            max_cache_entries: 10000,
            event_retention_days: 90,
            audit_logging: true,
        }
    }
}

/// State store statistics
#[derive(Debug, Clone, Default)]
pub struct StateStoreStats {
    /// Total commands processed
    pub commands_processed: u64,
    /// Total queries processed
    pub queries_processed: u64,
    /// Cache hit rate
    pub cache_hits: u64,
    /// Cache miss rate
    pub cache_misses: u64,
    /// Total events stored
    pub total_events: u64,
}

/// Unified state store
pub struct StateStore {
    /// Database connection pool
    db: SqlitePool,
    /// In-memory cache (DashMap for concurrent access)
    cache: DashMap<AgentId, CachedAgentState>,
    /// Event bus for publishing state changes
    event_tx: broadcast::Sender<StateEvent>,
    /// Configuration
    config: StateStoreConfig,
    /// Statistics
    stats: RwLock<StateStoreStats>,
    /// Sequence counter for events
    sequence_counter: RwLock<u64>,
}

/// Cached agent state
#[derive(Debug, Clone)]
struct CachedAgentState {
    info: AgentInfo,
    #[allow(dead_code)]
    cached_at: DateTime<Utc>,
    sequence: u64,
}

impl StateStore {
    /// Create new state store
    pub async fn new(db: SqlitePool, config: StateStoreConfig) -> Result<Self> {
        // Initialize database schema
        Self::init_schema(&db).await?;

        let (event_tx, _) = broadcast::channel(1000);

        let store = Self {
            db,
            cache: DashMap::with_capacity(config.max_cache_entries),
            event_tx,
            config,
            stats: RwLock::new(StateStoreStats::default()),
            sequence_counter: RwLock::new(0),
        };

        // Load initial state from database
        store.load_initial_state().await?;

        info!("StateStore initialized with CQRS pattern");
        Ok(store)
    }

    /// Initialize database schema
    async fn init_schema(db: &SqlitePool) -> Result<()> {
        // Create events table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS agent_state_events (
                event_id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                payload TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                correlation_id TEXT NOT NULL,
                sequence INTEGER NOT NULL,
                created_at TEXT DEFAULT (datetime('now'))
            )
            "#,
        )
        .execute(db)
        .await
        .map_err(|e| GatewayError::state(format!("Failed to create events table: {}", e)))?;

        // Create indexes
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_state_events_agent_id 
            ON agent_state_events(agent_id, sequence DESC)
            "#,
        )
        .execute(db)
        .await
        .map_err(|e| GatewayError::state(format!("Failed to create index: {}", e)))?;

        Ok(())
    }

    /// Load initial state from database
    async fn load_initial_state(&self) -> Result<()> {
        // Load latest state for all agents (SQLite compatible - uses GROUP BY instead of DISTINCT ON)
        let rows = sqlx::query_as::<_, (String, String, serde_json::Value, i64)>(
            r#"
            SELECT
                agent_id, event_type, payload, MAX(sequence) as sequence
            FROM agent_state_events
            WHERE event_type != 'agent_archived'
            GROUP BY agent_id
            "#,
        )
        .fetch_all(&self.db)
        .await
        .map_err(|e| GatewayError::state(format!("Failed to load initial state: {}", e)))?;

        for (agent_id, _event_type, payload, sequence) in rows {
            if let Ok(info) = serde_json::from_value::<AgentInfo>(payload) {
                self.cache.insert(
                    agent_id,
                    CachedAgentState {
                        info,
                        cached_at: Utc::now(),
                        sequence: sequence as u64,
                    },
                );
            }
        }

        info!("Loaded {} agents into cache", self.cache.len());
        Ok(())
    }

    /// Execute state command (write operation)
    pub async fn execute(&self, command: StateCommand) -> Result<StateEvent> {
        let correlation_id = Uuid::new_v4().to_string();
        let timestamp = Utc::now();

        debug!(?command, "Executing state command");

        // Validate command
        self.validate_command(&command).await?;

        // Generate event
        let event = self.command_to_event(&command, &correlation_id, timestamp).await?;

        // Persist event
        if self.config.event_sourcing {
            self.persist_event(&event).await?;
        }

        // Update cache
        self.apply_event_to_cache(&event).await;

        // Publish event
        let _ = self.event_tx.send(event.clone());

        // Update stats
        self.stats.write().await.commands_processed += 1;

        info!(
            agent_id = %event.agent_id,
            event_type = ?event.event_type,
            "State command executed successfully"
        );

        Ok(event)
    }

    /// Execute query (read operation)
    pub async fn query(&self, query: StateQuery) -> Result<QueryResult> {
        debug!(?query, "Executing state query");

        let result = match &query {
            StateQuery::GetState { agent_id } => self.query_state(agent_id).await?,
            StateQuery::GetAgentInfo { agent_id } => self.query_agent_info(agent_id).await?,
            StateQuery::ListAgents { filter, limit, offset } => {
                self.query_list_agents(filter.as_ref(), *limit, *offset).await?
            }
            StateQuery::GetEventHistory { agent_id, from_sequence, limit } => {
                self.query_event_history(agent_id, *from_sequence, *limit).await?
            }
            StateQuery::GetStateAt { agent_id, timestamp } => {
                self.query_state_at(agent_id, *timestamp).await?
            }
            StateQuery::ListWorkflowInstances { status, limit } => {
                self.query_list_workflow_instances(status.as_deref(), *limit).await?
            }
            StateQuery::GetWorkflowInstance { instance_id } => {
                self.query_workflow_instance(instance_id).await?
            }
        };

        // Update stats
        self.stats.write().await.queries_processed += 1;

        Ok(result)
    }

    /// Subscribe to state events
    pub fn subscribe(&self) -> broadcast::Receiver<StateEvent> {
        self.event_tx.subscribe()
    }

    /// Get store statistics
    pub async fn stats(&self) -> StateStoreStats {
        self.stats.read().await.clone()
    }

    /// Validate command
    async fn validate_command(&self, command: &StateCommand) -> Result<()> {
        match command {
            StateCommand::Transition { agent_id, from, .. } => {
                // Check if agent exists
                if let Some(cached) = self.cache.get(agent_id) {
                    if &cached.info.current_state != from {
                        return Err(GatewayError::state(format!(
                            "Invalid transition: agent {} is in state {:?}, expected {:?}",
                            agent_id, cached.info.current_state, from
                        )));
                    }
                } else {
                    return Err(GatewayError::state(format!(
                        "Agent {} not found",
                        agent_id
                    )));
                }
            }
            StateCommand::AssignTask { agent_id, .. } => {
                if let Some(cached) = self.cache.get(agent_id) {
                    if cached.info.current_state != AgentState::Idle {
                        return Err(GatewayError::state(format!(
                            "Cannot assign task: agent {} is not idle (state: {:?})",
                            agent_id, cached.info.current_state
                        )));
                    }
                } else {
                    return Err(GatewayError::state(format!(
                        "Agent {} not found",
                        agent_id
                    )));
                }
            }
            _ => {} // Other commands don't need special validation
        }

        Ok(())
    }

    /// Convert command to event
    async fn command_to_event(
        &self,
        command: &StateCommand,
        correlation_id: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<StateEvent> {
        let (agent_id, event_type, payload) = match command {
            StateCommand::RegisterAgent {
                agent_id,
                config,
                metadata,
            } => {
                let info = AgentInfo {
                    agent_id: agent_id.clone(),
                    config: config.clone(),
                    current_state: AgentState::Registered,
                    metadata: metadata.clone(),
                    created_at: timestamp,
                    updated_at: timestamp,
                    task_count: 0,
                    success_count: 0,
                    failure_count: 0,
                };
                (
                    agent_id.clone(),
                    StateEventType::AgentRegistered,
                    serde_json::to_value(info).map_err(|e| GatewayError::state(e.to_string()))?,
                )
            }
            StateCommand::Transition {
                agent_id,
                from,
                to,
                reason,
            } => (
                agent_id.clone(),
                StateEventType::StateTransitioned,
                serde_json::json!({
                    "from": from,
                    "to": to,
                    "reason": reason,
                }),
            ),
            StateCommand::AssignTask { agent_id, task_id } => (
                agent_id.clone(),
                StateEventType::TaskAssigned,
                serde_json::json!({"task_id": task_id}),
            ),
            StateCommand::CompleteTask {
                agent_id,
                task_id,
                success,
                result,
            } => (
                agent_id.clone(),
                StateEventType::TaskCompleted,
                serde_json::json!({
                    "task_id": task_id,
                    "success": success,
                    "result": result,
                }),
            ),
            StateCommand::UpdateConfig { agent_id, config } => (
                agent_id.clone(),
                StateEventType::ConfigUpdated,
                serde_json::to_value(config).map_err(|e| GatewayError::state(e.to_string()))?,
            ),
            StateCommand::UpdateMetadata { agent_id, metadata } => (
                agent_id.clone(),
                StateEventType::MetadataUpdated,
                serde_json::to_value(metadata).map_err(|e| GatewayError::state(e.to_string()))?,
            ),
            StateCommand::ArchiveAgent { agent_id } => (
                agent_id.clone(),
                StateEventType::AgentArchived,
                serde_json::json!({}),
            ),
        };

        let sequence = {
            let mut counter = self.sequence_counter.write().await;
            *counter += 1;
            *counter
        };

        Ok(StateEvent {
            event_id: Uuid::new_v4().to_string(),
            agent_id,
            event_type,
            payload,
            timestamp,
            correlation_id: correlation_id.to_string(),
            sequence,
        })
    }

    /// Persist event to database
    async fn persist_event(&self, event: &StateEvent) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO agent_state_events
                (event_id, agent_id, event_type, payload, timestamp, correlation_id, sequence)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
        )
        .bind(&event.event_id)
        .bind(&event.agent_id)
        .bind(format!("{:?}", event.event_type))
        .bind(&event.payload)
        .bind(event.timestamp.to_rfc3339())
        .bind(&event.correlation_id)
        .bind(event.sequence as i64)
        .execute(&self.db)
        .await
        .map_err(|e| GatewayError::state(format!("Failed to persist event: {}", e)))?;

        Ok(())
    }

    /// Apply event to cache
    async fn apply_event_to_cache(&self, event: &StateEvent) {
        use StateEventType::*;

        match event.event_type {
            AgentRegistered => {
                if let Ok(info) = serde_json::from_value::<AgentInfo>(event.payload.clone()) {
                    self.cache.insert(
                        event.agent_id.clone(),
                        CachedAgentState {
                            info,
                            cached_at: Utc::now(),
                            sequence: event.sequence,
                        },
                    );
                }
            }
            StateTransitioned => {
                if let Some(mut cached) = self.cache.get_mut(&event.agent_id) {
                    if let Some(to) = event.payload.get("to").and_then(|v| {
                        serde_json::from_value::<AgentState>(v.clone()).ok()
                    }) {
                        cached.info.current_state = to;
                        cached.info.updated_at = event.timestamp;
                        cached.sequence = event.sequence;
                    }
                }
            }
            TaskAssigned => {
                if let Some(mut cached) = self.cache.get_mut(&event.agent_id) {
                    cached.info.current_state = AgentState::Working;
                    cached.info.task_count += 1;
                    cached.info.updated_at = event.timestamp;
                    cached.sequence = event.sequence;
                }
            }
            TaskCompleted => {
                if let Some(mut cached) = self.cache.get_mut(&event.agent_id) {
                    cached.info.current_state = AgentState::Idle;
                    cached.info.updated_at = event.timestamp;
                    cached.sequence = event.sequence;

                    if let Some(success) = event.payload.get("success").and_then(|v| v.as_bool()) {
                        if success {
                            cached.info.success_count += 1;
                        } else {
                            cached.info.failure_count += 1;
                        }
                    }
                }
            }
            ConfigUpdated => {
                if let Some(mut cached) = self.cache.get_mut(&event.agent_id) {
                    if let Ok(config) = serde_json::from_value::<AgentConfig>(event.payload.clone())
                    {
                        cached.info.config = config;
                        cached.info.updated_at = event.timestamp;
                        cached.sequence = event.sequence;
                    }
                }
            }
            MetadataUpdated => {
                if let Some(mut cached) = self.cache.get_mut(&event.agent_id) {
                    if let Ok(metadata) =
                        serde_json::from_value::<HashMap<String, String>>(event.payload.clone())
                    {
                        cached.info.metadata = metadata;
                        cached.info.updated_at = event.timestamp;
                        cached.sequence = event.sequence;
                    }
                }
            }
            AgentArchived => {
                self.cache.remove(&event.agent_id);
            }
        }
    }

    /// Query: Get current state
    async fn query_state(&self, agent_id: &AgentId) -> Result<QueryResult> {
        if let Some(cached) = self.cache.get(agent_id) {
            self.stats.write().await.cache_hits += 1;
            return Ok(QueryResult::State {
                agent_id: agent_id.clone(),
                state: cached.info.current_state,
                since: cached.info.updated_at,
                metadata: cached.info.metadata.clone(),
            });
        }

        self.stats.write().await.cache_misses += 1;

        // Fallback to database
        let row: Option<(String, String, String)> = sqlx::query_as(
            r#"
            SELECT agent_id, payload, timestamp
            FROM agent_state_events
            WHERE agent_id = ?1
            ORDER BY sequence DESC
            LIMIT 1
            "#,
        )
        .bind(agent_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| GatewayError::state(format!("Database error: {}", e)))?;

        let row = row.and_then(|(id, payload, ts)| {
            let payload: serde_json::Value = serde_json::from_str(&payload).ok()?;
            let ts = ts.parse::<DateTime<Utc>>().ok()?;
            Some((id, payload, ts))
        });

        if let Some((_, payload, since)) = row {
            if let Ok(info) = serde_json::from_value::<AgentInfo>(payload) {
                return Ok(QueryResult::State {
                    agent_id: agent_id.clone(),
                    state: info.current_state,
                    since,
                    metadata: info.metadata,
                });
            }
        }

        Err(GatewayError::not_found("agent", agent_id))
    }

    /// Query: Get agent info
    async fn query_agent_info(&self, agent_id: &AgentId) -> Result<QueryResult> {
        if let Some(cached) = self.cache.get(agent_id) {
            self.stats.write().await.cache_hits += 1;
            return Ok(QueryResult::AgentInfo {
                agent_id: cached.info.agent_id.clone(),
                config: cached.info.config.clone(),
                current_state: cached.info.current_state,
                metadata: cached.info.metadata.clone(),
                created_at: cached.info.created_at,
                updated_at: cached.info.updated_at,
                task_count: cached.info.task_count,
                success_count: cached.info.success_count,
                failure_count: cached.info.failure_count,
            });
        }

        self.stats.write().await.cache_misses += 1;
        Err(GatewayError::not_found("agent", agent_id))
    }

    /// Query: List agents
    async fn query_list_agents(
        &self,
        filter: Option<&AgentFilter>,
        limit: usize,
        offset: usize,
    ) -> Result<QueryResult> {
        let mut agents: Vec<AgentInfo> = self
            .cache
            .iter()
            .map(|entry| entry.info.clone())
            .filter(|info| {
                if let Some(f) = filter {
                    // Apply filters
                    if let Some(state) = &f.state {
                        if &info.current_state != state {
                            return false;
                        }
                    }
                    if let Some(cap) = &f.has_capability {
                        if !info.config.capabilities.iter().any(|c| &c.name == cap) {
                            return false;
                        }
                    }
                    if let Some(after) = f.created_after {
                        if info.created_at < after {
                            return false;
                        }
                    }
                    if let Some(before) = f.created_before {
                        if info.created_at > before {
                            return false;
                        }
                    }
                }
                true
            })
            .collect();

        let total = agents.len();

        // Sort by creation time (newest first)
        agents.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        // Apply pagination
        let agents: Vec<AgentInfo> = agents.into_iter().skip(offset).take(limit).collect();

        Ok(QueryResult::AgentList {
            agents,
            total,
            limit,
            offset,
        })
    }

    /// Query: Get event history
    async fn query_event_history(
        &self,
        agent_id: &AgentId,
        from_sequence: Option<u64>,
        limit: usize,
    ) -> Result<QueryResult> {
        let rows = if let Some(seq) = from_sequence {
            sqlx::query_as::<_, StateEventRow>(
                r#"
                SELECT event_id, agent_id, event_type, payload, timestamp, correlation_id, sequence
                FROM agent_state_events
                WHERE agent_id = ?1 AND sequence > ?2
                ORDER BY sequence ASC LIMIT ?3
                "#
            )
                .bind(agent_id)
                .bind(seq as i64)
                .bind(limit as i64)
                .fetch_all(&self.db)
                .await
        } else {
            sqlx::query_as::<_, StateEventRow>(
                r#"
                SELECT event_id, agent_id, event_type, payload, timestamp, correlation_id, sequence
                FROM agent_state_events
                WHERE agent_id = ?1
                ORDER BY sequence ASC LIMIT ?2
                "#
            )
                .bind(agent_id)
                .bind(limit as i64)
                .fetch_all(&self.db)
                .await
        }
        .map_err(|e| GatewayError::state(format!("Database error: {}", e)))?;

        let events: Vec<StateEvent> = rows
            .into_iter()
            .filter_map(|row| row.try_into().ok())
            .collect();

        let next_sequence = events.last().map(|e| e.sequence + 1);

        Ok(QueryResult::EventHistory {
            agent_id: agent_id.clone(),
            events,
            next_sequence,
        })
    }

    /// Query: Get state at specific time
    async fn query_state_at(
        &self,
        agent_id: &AgentId,
        timestamp: DateTime<Utc>,
    ) -> Result<QueryResult> {
        let row: Option<(String, String)> = sqlx::query_as(
            r#"
            SELECT agent_id, payload
            FROM agent_state_events
            WHERE agent_id = ?1 AND datetime(timestamp) <= datetime(?2)
            ORDER BY sequence DESC
            LIMIT 1
            "#,
        )
        .bind(agent_id)
        .bind(timestamp.to_rfc3339())
        .fetch_optional(&self.db)
        .await
        .map_err(|e| GatewayError::state(format!("Database error: {}", e)))?;

        if let Some((_, payload)) = row {
            if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&payload) {
                if let Ok(info) = serde_json::from_value::<AgentInfo>(payload) {
                    return Ok(QueryResult::StateSnapshot {
                        agent_id: agent_id.clone(),
                        state: info.current_state,
                        timestamp,
                    });
                }
            }
        }

        Err(GatewayError::not_found("agent state at time", format!("{}@{}", agent_id, timestamp)))
    }

    /// Query workflow instances
    async fn query_list_workflow_instances(
        &self,
        status: Option<&str>,
        limit: usize,
    ) -> Result<QueryResult> {
        let rows: Vec<(String, String, String, String)> = if let Some(s) = status {
            sqlx::query_as(
                r#"
                SELECT id, workflow_id, status, trigger_context
                FROM workflow_instances
                WHERE status = ?1
                ORDER BY started_at DESC
                LIMIT ?2
                "#
            )
            .bind(s)
            .bind(limit as i64)
            .fetch_all(&self.db)
            .await
            .map_err(|e| GatewayError::state(format!("Database error: {}", e)))?
        } else {
            sqlx::query_as(
                r#"
                SELECT id, workflow_id, status, trigger_context
                FROM workflow_instances
                ORDER BY started_at DESC
                LIMIT ?1
                "#
            )
            .bind(limit as i64)
            .fetch_all(&self.db)
            .await
            .map_err(|e| GatewayError::state(format!("Database error: {}", e)))?
        };

        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workflow_instances")
            .fetch_one(&self.db)
            .await
            .map_err(|e| GatewayError::state(format!("Database error: {}", e)))?;

        let instances: Vec<serde_json::Value> = rows.into_iter().map(|(id, workflow_id, status, trigger_context)| {
            serde_json::json!({
                "instance_id": id,
                "workflow_id": workflow_id,
                "status": status,
                "trigger_context": serde_json::from_str::<serde_json::Value>(&trigger_context).unwrap_or(serde_json::Value::Null),
            })
        }).collect();

        Ok(QueryResult::WorkflowInstanceList { instances, total: total as usize })
    }

    /// Query a single workflow instance
    async fn query_workflow_instance(&self, instance_id: &str) -> Result<QueryResult> {
        let row: Option<(String, String, String, String, String, String, String, Option<String>)> = sqlx::query_as(
            r#"
            SELECT id, workflow_id, status, trigger_context, step_states, error_log, started_at, completed_at
            FROM workflow_instances
            WHERE id = ?1
            "#
        )
        .bind(instance_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| GatewayError::state(format!("Database error: {}", e)))?;

        let instance = row.map(|(id, workflow_id, status, trigger_context, step_states, error_log, started_at, completed_at)| {
            serde_json::json!({
                "instance_id": id,
                "workflow_id": workflow_id,
                "status": status,
                "trigger_context": serde_json::from_str::<serde_json::Value>(&trigger_context).unwrap_or(serde_json::Value::Null),
                "step_states": serde_json::from_str::<serde_json::Value>(&step_states).unwrap_or(serde_json::Value::Null),
                "error_log": serde_json::from_str::<serde_json::Value>(&error_log).unwrap_or(serde_json::Value::Null),
                "started_at": started_at,
                "completed_at": completed_at,
            })
        });

        Ok(QueryResult::WorkflowInstance { instance })
    }

    /// Upsert a workflow instance directly (bypasses event sourcing)
    pub async fn upsert_workflow_instance(
        &self,
        instance_id: &str,
        workflow_id: &str,
        status: &str,
        trigger_context: &serde_json::Value,
        step_states: &serde_json::Value,
        error_log: &serde_json::Value,
        started_at: DateTime<Utc>,
        completed_at: Option<DateTime<Utc>>,
    ) -> Result<()> {
        let trigger_ctx_str = serde_json::to_string(trigger_context).unwrap_or_default();
        let step_states_str = serde_json::to_string(step_states).unwrap_or_default();
        let error_log_str = serde_json::to_string(error_log).unwrap_or_default();
        let completed_at_str = completed_at.map(|dt| dt.to_rfc3339());

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
        .bind(instance_id)
        .bind(workflow_id)
        .bind(status)
        .bind(&trigger_ctx_str)
        .bind(&step_states_str)
        .bind(&error_log_str)
        .bind(started_at.to_rfc3339())
        .bind(&completed_at_str)
        .execute(&self.db)
        .await
        .map_err(|e| GatewayError::state(format!("Failed to upsert workflow instance: {}", e)))?;

        Ok(())
    }
}

/// Helper struct for SQL queries
#[derive(sqlx::FromRow)]
struct StateEventRow {
    event_id: String,
    agent_id: String,
    event_type: String,
    payload: String,
    timestamp: String,
    correlation_id: String,
    sequence: i64,
}

impl TryFrom<StateEventRow> for StateEvent {
    type Error = String;

    fn try_from(row: StateEventRow) -> std::result::Result<Self, Self::Error> {
        let event_type = match row.event_type.as_str() {
            "AgentRegistered" => StateEventType::AgentRegistered,
            "StateTransitioned" => StateEventType::StateTransitioned,
            "TaskAssigned" => StateEventType::TaskAssigned,
            "TaskCompleted" => StateEventType::TaskCompleted,
            "ConfigUpdated" => StateEventType::ConfigUpdated,
            "MetadataUpdated" => StateEventType::MetadataUpdated,
            "AgentArchived" => StateEventType::AgentArchived,
            _ => return Err(format!("Unknown event type: {}", row.event_type)),
        };

        let payload: serde_json::Value = serde_json::from_str(&row.payload)
            .map_err(|e| format!("Failed to parse payload: {}", e))?;
        let timestamp: DateTime<Utc> = row.timestamp.parse()
            .map_err(|e: chrono::ParseError| format!("Failed to parse timestamp: {}", e))?;

        Ok(StateEvent {
            event_id: row.event_id,
            agent_id: row.agent_id,
            event_type,
            payload,
            timestamp,
            correlation_id: row.correlation_id,
            sequence: row.sequence as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_state_store_operations() {
        // This would require a test database
        // Skipping for now as it requires infrastructure
    }

    #[test]
    fn test_agent_filter() {
        let filter = AgentFilter {
            state: Some(AgentState::Idle),
            has_capability: Some("chat".to_string()),
            ..Default::default()
        };

        assert_eq!(filter.state, Some(AgentState::Idle));
        assert_eq!(filter.has_capability, Some("chat".to_string()));
    }

    #[test]
    fn test_state_command_serialization() {
        let cmd = StateCommand::Transition {
            agent_id: "agent-1".to_string(),
            from: AgentState::Idle,
            to: AgentState::Working,
            reason: Some("Task assigned".to_string()),
        };

        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("Transition"));
        assert!(json.contains("agent-1"));
    }
}
