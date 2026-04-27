//! AgentRuntime Trait Implementation
//!
//! Implements the `AgentRuntime` trait from `beebotos_gateway_lib`,
//! providing the concrete integration between Gateway and Agent runtime.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use beebotos_gateway_lib::agent_runtime::{
    AgentCapability, AgentConfig as GatewayAgentConfig, AgentEvent as GatewayAgentEvent,
    AgentHandle, AgentId, AgentRuntime, AgentState as GatewayAgentState,
    AgentStatus as GatewayAgentStatus, LlmConfig, MemoryConfig, RuntimeConfig, StateCommand,
    TaskConfig, TaskResult,
};
use beebotos_gateway_lib::error::GatewayError;
use beebotos_gateway_lib::Result;
use chrono::Utc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{error, info, trace, warn};

use crate::kernel_integration::KernelAgentBuilder;
use crate::state_manager::{AgentState, AgentStateManager, AgentStateRecord, StateTransition};
use crate::task::TaskType;
use crate::{Agent, AgentConfig};

/// Agent runtime implementation for Gateway integration
pub struct GatewayAgentRuntime {
    /// State manager for agent lifecycle
    state_manager: Arc<AgentStateManager>,
    /// Kernel reference
    kernel: Option<Arc<beebotos_kernel::Kernel>>,
    /// Active agent tasks
    agent_tasks: RwLock<HashMap<AgentId, AgentTaskHandle>>,
    /// Event broadcaster
    event_tx: broadcast::Sender<GatewayAgentEvent>,
    /// Configuration
    #[allow(dead_code)]
    config: RuntimeConfig,
    /// 🟢 P2 FIX: Metrics collector for observability
    metrics: Arc<crate::metrics::MetricsCollector>,
    /// 🟢 P2 FIX: Memory search system for agent memory retrieval
    memory_system: Option<Arc<dyn crate::memory::MemorySearch>>,
    /// 🆕 PLANNING FIX: Planning engine for agent task planning
    planning_engine: Option<Arc<crate::planning::PlanningEngine>>,
    /// 🆕 PLANNING FIX: Plan executor for plan execution
    plan_executor: Option<Arc<crate::planning::PlanExecutor>>,
    /// 🆕 PLANNING FIX: RePlanner for dynamic replanning
    replanner: Option<Arc<dyn crate::planning::RePlanner>>,
    /// LLM interface for agent execution
    llm_interface: Option<Arc<dyn crate::communication::LLMCallInterface>>,
    /// 🆕 TOOL-CALLING FIX: LLM provider for building LLMClient with tool support
    llm_provider: Option<Arc<dyn crate::llm::LLMProvider>>,
    /// 🟢 P2 FIX: Skill registry for WASM skill execution
    skill_registry: Option<Arc<crate::skills::SkillRegistry>>,
}

/// Handle to an agent's task
#[derive(Clone)]
struct AgentTaskHandle {
    /// Task sender channel
    task_sender: mpsc::UnboundedSender<crate::kernel_integration::KernelTaskRequest>,
    /// Kernel task ID
    kernel_task_id: Option<u64>,
    /// Agent configuration
    config: AgentConfig,
}

impl GatewayAgentRuntime {
    /// Create new gateway agent runtime
    ///
    /// 🔒 P0 FIX: Automatically recovers agents from persistent state on
    /// startup. This ensures that agents survive gateway restarts.
    ///
    /// 🟢 P2 FIX: Initializes metrics collector for observability.
    pub async fn new(
        kernel: Option<Arc<beebotos_kernel::Kernel>>,
        llm_interface: Option<Arc<dyn crate::communication::LLMCallInterface>>,
        config: RuntimeConfig,
        db_pool: Option<sqlx::SqlitePool>,
    ) -> Result<Self> {
        let state_manager = match db_pool {
            Some(pool) => {
                let sm = AgentStateManager::with_persistence(pool, None);
                if let Err(e) = sm.init_persistence().await {
                    warn!("Failed to initialize agent state persistence: {}", e);
                }
                Arc::new(sm)
            }
            None => Arc::new(AgentStateManager::new(None)),
        };

        let (event_tx, _) = broadcast::channel(1000);

        // 🟢 P2 FIX: Initialize metrics collector
        let metrics = Arc::new(crate::metrics::MetricsCollector::new());
        info!("✅ Metrics collector initialized");

        // 🟢 P2 FIX: Initialize memory search system
        let memory_db_path = std::path::PathBuf::from("data/memory_search.db");

        let memory_system =
            match crate::memory::HybridSearchSqlite::default_with_path(&memory_db_path) {
                Ok(engine) => {
                    info!(
                        "✅ Memory search system initialized at {:?}",
                        memory_db_path
                    );
                    Some(Arc::new(engine) as Arc<dyn crate::memory::MemorySearch>)
                }
                Err(e) => {
                    warn!("❌ Failed to initialize memory search system: {}", e);
                    None
                }
            };

        // 🆕 PLANNING FIX: Initialize planning components
        let planning_engine = Arc::new(crate::planning::PlanningEngine::new());
        let plan_executor = Arc::new(crate::planning::PlanExecutor::new());
        let replanner = Arc::new(crate::planning::ConditionRePlanner::new())
            as Arc<dyn crate::planning::RePlanner>;
        info!("✅ Planning components initialized");

        // 🟢 P2 FIX: Initialize skill registry (skills can be registered at runtime via
        // API)
        let skill_registry = Arc::new(crate::skills::SkillRegistry::new());
        // 🆕 FIX: Auto-load markdown skills from skills/ directory so registry is never
        // empty.
        crate::skills::builtin_loader::load_builtin_skills(&skill_registry).await;
        info!("✅ Skill registry initialized");

        let runtime = Self {
            state_manager: state_manager.clone(),
            kernel: kernel.clone(),
            agent_tasks: RwLock::new(HashMap::new()),
            event_tx,
            config: config.clone(),
            metrics: metrics.clone(),
            memory_system: memory_system.clone(),
            planning_engine: Some(planning_engine.clone()),
            plan_executor: Some(plan_executor.clone()),
            replanner: Some(replanner.clone()),
            llm_interface: llm_interface.clone(),
            // 🆕 TOOL-CALLING FIX: LLM provider initialized as None, set via with_llm_provider
            llm_provider: None,
            skill_registry: Some(skill_registry.clone()),
        };

        // 🆕 FIX: Agent recovery moved to after with_llm_provider is set,
        // so recovered agents get llm_client and tools properly configured.

        Ok(runtime)
    }

    /// 🟢 P2 FIX: Get metrics collector reference
    pub fn metrics(&self) -> &Arc<crate::metrics::MetricsCollector> {
        &self.metrics
    }

    /// 🟢 P2 FIX: Export metrics in Prometheus format
    pub fn export_metrics(&self) -> String {
        self.metrics.export_prometheus()
    }

    /// 🟢 P2 FIX: Inject an externally prepared skill registry (e.g. one that
    /// has already had built-in skills registered).
    pub fn with_skill_registry(mut self, registry: Arc<crate::skills::SkillRegistry>) -> Self {
        self.skill_registry = Some(registry);
        self
    }

    /// 🆕 TOOL-CALLING FIX: Set LLM provider for building LLMClient with tool support
    pub fn with_llm_provider(mut self, provider: Arc<dyn crate::llm::LLMProvider>) -> Self {
        self.llm_provider = Some(provider);
        self
    }

    /// 🔒 P0 FIX: Recover agents from persistent state
    ///
    /// On gateway restart, this method restores all agents that were in a
    /// non-terminal state (not Stopped or Error) and respawns them in the
    /// kernel.
    pub async fn recover_agents(&self) -> Result<()> {
        info!("Starting agent recovery from persistent state...");

        // Load state from database if persistence is enabled
        if let Err(e) = self.state_manager.load_from_db().await {
            warn!("Could not load state from database: {}", e);
            // Continue with in-memory state
        }

        // Get all agents that need recovery
        let agents_to_recover = self.state_manager.list_agents().await;

        if agents_to_recover.is_empty() {
            info!("No agents to recover");
            return Ok(());
        }

        info!(
            "Found {} agents in state manager, checking for recovery...",
            agents_to_recover.len()
        );

        let mut recovered_count = 0;
        let mut failed_count = 0;

        for agent_id in &agents_to_recover {
            match self.recover_agent(&agent_id).await {
                Ok(true) => {
                    recovered_count += 1;
                    info!("Successfully recovered agent: {}", agent_id);
                }
                Ok(false) => {
                    // Agent in terminal state, no recovery needed
                    trace!("Agent {} in terminal state, skipping recovery", agent_id);
                }
                Err(e) => {
                    failed_count += 1;
                    warn!("Failed to recover agent {}: {}", agent_id, e);
                    // Mark agent as in error state
                    let _ = self
                        .state_manager
                        .transition(
                            &agent_id,
                            StateTransition::Error {
                                message: format!("Recovery failed: {}", e),
                            },
                        )
                        .await;
                }
            }
        }

        info!(
            "Agent recovery complete: {} recovered, {} failed, {} skipped",
            recovered_count,
            failed_count,
            agents_to_recover.len() - recovered_count - failed_count
        );

        Ok(())
    }

    /// 🔒 P0 FIX: Recover a single agent
    ///
    /// Returns:
    /// - Ok(true) if agent was recovered
    /// - Ok(false) if agent is in terminal state and doesn't need recovery
    /// - Err if recovery failed
    async fn recover_agent(&self, agent_id: &str) -> Result<bool> {
        let state = self
            .state_manager
            .get_state(agent_id)
            .await
            .map_err(|e| GatewayError::state(format!("Failed to get state: {}", e)))?;
        let record = self
            .state_manager
            .get_record(agent_id)
            .await
            .map_err(|e| GatewayError::state(format!("Failed to get record: {}", e)))?;

        // Don't recover agents in terminal states
        match state {
            AgentState::Stopped | AgentState::Error { .. } => {
                return Ok(false);
            }
            _ => {}
        }

        // Check if we have kernel to spawn the agent
        let kernel = match &self.kernel {
            Some(k) => k,
            None => {
                warn!("Cannot recover agent {}: no kernel available", agent_id);
                return Err(GatewayError::agent("Kernel not available for recovery"));
            }
        };

        // Reconstruct agent configuration from database or metadata
        let agent_config = self.reconstruct_config_from_record(&record).await?;

        // Only transition to Initializing if agent is in Registered state.
        // If already Idle/Working/Paused, we can respawn directly without
        // re-initializing.
        match state {
            AgentState::Registered => {
                self.state_manager
                    .transition(agent_id, StateTransition::Start)
                    .await
                    .map_err(|e| {
                        GatewayError::agent(format!("Failed to transition agent state: {}", e))
                    })?;
            }
            AgentState::Idle | AgentState::Working { .. } | AgentState::Paused => {
                info!(
                    "Agent {} is in {:?} state, skipping Start transition during recovery",
                    agent_id, state
                );
            }
            _ => {}
        }

        // Spawn agent in kernel
        // 🆕 FIX: 添加 skill/planning 所需权限，避免 capability denied
        let capabilities = beebotos_kernel::capabilities::CapabilitySet::standard()
            .with_permission("llm:chat")
            .with_permission("network:outbound")
            .with_permission("mcp:call")
            .with_permission("wasm:execute")
            .with_permission("file:read")
            .with_permission("planning:execute")
            .with_permission("skill:call");

        let mut builder = KernelAgentBuilder::new()
            .with_config(agent_config.clone())
            .with_kernel(kernel.clone())
            .with_state_manager(self.state_manager.clone())
            .with_capabilities(capabilities);

        if let Some(ref memory) = self.memory_system {
            builder = builder.with_memory_system(memory.clone());
        }

        // 🆕 PLANNING FIX: Inject planning components into recovered agent
        if let Some(ref engine) = self.planning_engine {
            builder = builder.with_planning_engine(engine.clone());
        }
        if let Some(ref executor) = self.plan_executor {
            builder = builder.with_plan_executor(executor.clone());
        }
        if let Some(ref replanner) = self.replanner {
            builder = builder.with_replanner(replanner.clone());
        }
        if let Some(ref llm) = self.llm_interface {
            builder = builder.with_llm_interface(llm.clone());
        }

        // 🆕 TOOL-CALLING FIX: Pass LLM provider for tool-calling support
        if let Some(ref provider) = self.llm_provider {
            builder = builder.with_llm_provider(provider.clone());
        }

        let (task_id, task_sender) = builder
            .spawn()
            .await
            .map_err(|e| GatewayError::agent(format!("Failed to respawn agent: {}", e)))?;

        // Store handle
        let handle = AgentTaskHandle {
            task_sender,
            kernel_task_id: Some(task_id.0),
            config: agent_config,
        };

        self.agent_tasks
            .write()
            .await
            .insert(agent_id.to_string(), handle);

        // Update kernel task ID in state
        let _ = self
            .state_manager
            .set_kernel_task_id(agent_id, task_id.0)
            .await;

        // Broadcast recovery event
        self.broadcast_event(GatewayAgentEvent::Started {
            agent_id: agent_id.to_string(),
            timestamp: Utc::now(),
        });

        Ok(true)
    }

    /// 🔒 P0 FIX: Reconstruct agent configuration from state record
    ///
    /// 🔧 FIX: Now uses persisted configuration from database instead of
    /// defaults
    async fn reconstruct_config_from_record(
        &self,
        record: &AgentStateRecord,
    ) -> Result<AgentConfig> {
        // First try to load full config from database
        if let Some(persistence) = self.state_manager.persistence() {
            match persistence.load_config(&record.agent_id).await {
                Ok(Some(persisted_config)) => {
                    info!(
                        "Loaded full config from database for agent {}",
                        record.agent_id
                    );
                    // 🔧 FIX: Fast-sync agents table record only (avoid slow save_config during
                    // recovery)
                    let _ = persistence.sync_agents_table(&persisted_config).await;
                    return Ok(AgentConfig {
                        id: persisted_config.agent_id,
                        name: persisted_config.name,
                        description: persisted_config.description,
                        version: persisted_config.version,
                        capabilities: persisted_config.capabilities,
                        models: persisted_config.model_config,
                        memory: persisted_config.memory_config,
                        personality: persisted_config.personality_config,
                    });
                }
                Ok(None) => {
                    warn!(
                        "No persisted config found for agent {}, using metadata fallback",
                        record.agent_id
                    );
                }
                Err(e) => {
                    warn!(
                        "Failed to load config for agent {}: {}, using metadata fallback",
                        record.agent_id, e
                    );
                }
            }
        }

        // Fallback: Extract configuration from metadata (legacy mode)
        let name = record
            .metadata
            .get("name")
            .cloned()
            .unwrap_or_else(|| format!("recovered-agent-{}", record.agent_id));

        let version = record
            .metadata
            .get("version")
            .cloned()
            .unwrap_or_else(|| "1.0.0".to_string());

        // Parse capabilities from metadata if available
        let capabilities: Vec<String> = record
            .metadata
            .get("capabilities")
            .map(|c| c.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default();

        // Try to parse model config from metadata
        let models = record
            .metadata
            .get("model_config")
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_else(|| crate::ModelConfig {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
                temperature: 0.7,
                max_tokens: 2000,
                top_p: 1.0,
            });

        // Try to parse memory config from metadata
        let memory = record
            .metadata
            .get("memory_config")
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_else(|| crate::MemoryConfig {
                episodic_capacity: 1000,
                semantic_capacity: 5000,
                working_memory_size: 10,
                consolidation_interval_hours: 24,
            });

        // Try to parse personality config from metadata
        let personality = record
            .metadata
            .get("personality_config")
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_else(|| crate::PersonalityConfig {
                openness: 0.5,
                conscientiousness: 0.5,
                extraversion: 0.5,
                agreeableness: 0.5,
                neuroticism: 0.5,
                base_mood: "neutral".to_string(),
            });

        let config = AgentConfig {
            id: record.agent_id.clone(),
            name: name.clone(),
            description: format!("Recovered agent (previous state: {:?})", record.state),
            version: version.clone(),
            capabilities: capabilities.clone(),
            models: models.clone(),
            memory: memory.clone(),
            personality: personality.clone(),
        };

        // 🔧 FIX: Fast-sync agents table for fallback config too
        if let Some(persistence) = self.state_manager.persistence() {
            let fallback = crate::state_manager::PersistedAgentConfig {
                agent_id: record.agent_id.clone(),
                name: name.clone(),
                description: config.description.clone(),
                version: version.clone(),
                capabilities: capabilities.clone(),
                model_config: models.clone(),
                memory_config: memory.clone(),
                personality_config: personality.clone(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };
            let _ = persistence.sync_agents_table(&fallback).await;
        }

        Ok(config)
    }

    /// Create with explicit state manager
    ///
    /// 🟢 P2 FIX: Also initializes metrics collector.
    pub fn with_state_manager(
        state_manager: Arc<AgentStateManager>,
        kernel: Option<Arc<beebotos_kernel::Kernel>>,
        llm_interface: Option<Arc<dyn crate::communication::LLMCallInterface>>,
        config: RuntimeConfig,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(1000);
        let metrics = Arc::new(crate::metrics::MetricsCollector::new());

        // 🟢 P2 FIX: Initialize memory search system
        let memory_db_path = std::path::PathBuf::from("data/memory_search.db");

        let memory_system =
            match crate::memory::HybridSearchSqlite::default_with_path(&memory_db_path) {
                Ok(engine) => {
                    info!(
                        "✅ Memory search system initialized at {:?}",
                        memory_db_path
                    );
                    Some(Arc::new(engine) as Arc<dyn crate::memory::MemorySearch>)
                }
                Err(e) => {
                    warn!("❌ Failed to initialize memory search system: {}", e);
                    None
                }
            };

        // 🆕 PLANNING FIX: Initialize planning components
        let planning_engine = Arc::new(crate::planning::PlanningEngine::new());
        let plan_executor = Arc::new(crate::planning::PlanExecutor::new());
        let replanner = Arc::new(crate::planning::ConditionRePlanner::new())
            as Arc<dyn crate::planning::RePlanner>;
        info!("✅ Planning components initialized");

        // 🟢 P2 FIX: Initialize skill registry
        let skill_registry = Arc::new(crate::skills::SkillRegistry::new());
        info!("✅ Skill registry initialized");

        Self {
            state_manager,
            kernel,
            agent_tasks: RwLock::new(HashMap::new()),
            event_tx,
            config,
            metrics,
            memory_system,
            planning_engine: Some(planning_engine),
            plan_executor: Some(plan_executor),
            replanner: Some(replanner),
            llm_interface,
            // 🆕 TOOL-CALLING FIX: LLM provider initialized as None
            llm_provider: None,
            skill_registry: Some(skill_registry),
        }
    }

    /// Convert gateway config to agent config
    fn convert_config(&self, gateway_config: &GatewayAgentConfig) -> AgentConfig {
        AgentConfig {
            id: gateway_config.id.clone(),
            name: gateway_config.name.clone(),
            description: gateway_config.description.clone(),
            version: gateway_config.version.clone(),
            capabilities: gateway_config
                .capabilities
                .iter()
                .map(|c| c.name.clone())
                .collect(),
            models: crate::ModelConfig {
                provider: gateway_config.llm_config.provider.clone(),
                model: gateway_config.llm_config.model.clone(),
                temperature: gateway_config.llm_config.temperature,
                max_tokens: gateway_config.llm_config.max_tokens,
                top_p: 1.0,
            },
            memory: crate::MemoryConfig {
                episodic_capacity: gateway_config.memory_config.max_entries,
                semantic_capacity: 5000,
                working_memory_size: 10,
                consolidation_interval_hours: 24,
            },
            personality: crate::PersonalityConfig {
                openness: 0.5,
                conscientiousness: 0.5,
                extraversion: 0.5,
                agreeableness: 0.5,
                neuroticism: 0.5,
                base_mood: "neutral".to_string(),
            },
        }
    }

    /// Convert agent state to gateway state
    fn convert_state(&self, state: &AgentState) -> GatewayAgentState {
        match state {
            AgentState::Registered => GatewayAgentState::Registered,
            AgentState::Initializing => GatewayAgentState::Initializing,
            AgentState::Idle => GatewayAgentState::Idle,
            AgentState::Working { .. } => GatewayAgentState::Working,
            AgentState::Paused => GatewayAgentState::Paused,
            AgentState::ShuttingDown => GatewayAgentState::ShuttingDown,
            AgentState::Stopped => GatewayAgentState::Stopped,
            AgentState::Error { .. } => GatewayAgentState::Error,
        }
    }

    /// Convert gateway state command to state transition
    fn convert_command(&self, command: StateCommand) -> Option<StateTransition> {
        match command {
            StateCommand::Start => Some(StateTransition::Start),
            StateCommand::Pause => Some(StateTransition::Pause),
            StateCommand::Resume => Some(StateTransition::Resume),
            StateCommand::Stop => Some(StateTransition::Shutdown),
            StateCommand::Restart => Some(StateTransition::Shutdown),
        }
    }

    /// Broadcast event
    fn broadcast_event(&self, event: GatewayAgentEvent) {
        let _ = self.event_tx.send(event);
    }

    /// Convert task type
    fn convert_task_type(&self, task_type: &str) -> TaskType {
        match task_type {
            "llm_chat" => TaskType::LlmChat,
            "skill_execution" => TaskType::SkillExecution,
            "mcp_tool" => TaskType::McpTool,
            "file_processing" => TaskType::FileProcessing,
            "a2a_send" => TaskType::A2aSend,
            "chain_transaction" => TaskType::ChainTransaction,
            _ => TaskType::Custom(task_type.to_string()),
        }
    }
}

#[async_trait]
impl AgentRuntime for GatewayAgentRuntime {
    async fn spawn(&self, gateway_config: GatewayAgentConfig) -> Result<AgentHandle> {
        let agent_id = gateway_config.id.clone();

        info!(agent_id = %agent_id, "Spawning agent via GatewayAgentRuntime");

        // 🟢 P2 FIX: Record metric
        self.metrics
            .record_session_started(&agent_id, "agent_spawn");

        // Check if agent already exists
        if self.agent_tasks.read().await.contains_key(&agent_id) {
            return Err(GatewayError::agent(format!(
                "Agent {} already exists",
                agent_id
            )));
        }

        // Convert config
        let agent_config = self.convert_config(&gateway_config);

        // 🔧 FIX: If agent exists in state_manager but not in active tasks (e.g. stale
        // DB record in Error state that was skipped during recovery),
        // unregister it so we can respawn.
        match self.state_manager.get_state(&agent_id).await {
            Ok(_) if !self.agent_tasks.read().await.contains_key(&agent_id) => {
                warn!(
                    "Agent {} exists in state_manager but not in active tasks, unregistering \
                     stale record",
                    agent_id
                );
                let _ = self.state_manager.unregister_agent(&agent_id).await;
                if let Some(persistence) = self.state_manager.persistence() {
                    let _ = persistence.delete_record(&agent_id).await;
                }
            }
            _ => {}
        }

        // Register in state manager
        let mut metadata = HashMap::new();
        metadata.insert("name".to_string(), gateway_config.name.clone());
        metadata.insert("version".to_string(), gateway_config.version.clone());

        // 🔧 FIX: Store full config in metadata for recovery
        metadata.insert(
            "model_config".to_string(),
            serde_json::to_string(&agent_config.models).unwrap_or_default(),
        );
        metadata.insert(
            "memory_config".to_string(),
            serde_json::to_string(&agent_config.memory).unwrap_or_default(),
        );
        metadata.insert(
            "personality_config".to_string(),
            serde_json::to_string(&agent_config.personality).unwrap_or_default(),
        );

        self.state_manager
            .register_agent(&agent_id, metadata)
            .await
            .map_err(|e| GatewayError::agent(format!("Failed to register agent: {}", e)))?;

        // 🔧 FIX: Save full configuration to database
        if let Some(persistence) = self.state_manager.persistence() {
            let persisted_config = crate::state_manager::PersistedAgentConfig {
                agent_id: agent_id.clone(),
                name: agent_config.name.clone(),
                description: agent_config.description.clone(),
                version: agent_config.version.clone(),
                capabilities: agent_config.capabilities.clone(),
                model_config: agent_config.models.clone(),
                memory_config: agent_config.memory.clone(),
                personality_config: agent_config.personality.clone(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            };

            if let Err(e) = persistence.save_config(&persisted_config).await {
                warn!("Failed to save agent config to database: {}", e);
                // Continue even if config save fails
            } else {
                info!("Agent config saved to database for {}", agent_id);
            }
        }

        // Create agent handle
        let handle = if let Some(ref kernel) = self.kernel {
            // Spawn in kernel sandbox
            // 🆕 FIX: 添加 skill/planning 所需权限，避免 capability denied
            let capabilities = beebotos_kernel::capabilities::CapabilitySet::standard()
                .with_permission("llm:chat")
                .with_permission("network:outbound")
                .with_permission("mcp:call")
                .with_permission("wasm:execute")
                .with_permission("file:read")
                .with_permission("planning:execute")
                .with_permission("skill:call");

            let mut builder = KernelAgentBuilder::new()
                .with_config(agent_config.clone())
                .with_kernel(kernel.clone())
                .with_state_manager(self.state_manager.clone())
                .with_capabilities(capabilities);

            if let Some(ref memory) = self.memory_system {
                builder = builder.with_memory_system(memory.clone());
            }

            // 🆕 PLANNING FIX: Inject planning components into agent
            if let Some(ref engine) = self.planning_engine {
                builder = builder.with_planning_engine(engine.clone());
            }
            if let Some(ref executor) = self.plan_executor {
                builder = builder.with_plan_executor(executor.clone());
            }
            if let Some(ref replanner) = self.replanner {
                builder = builder.with_replanner(replanner.clone());
            }
            if let Some(ref llm) = self.llm_interface {
                builder = builder.with_llm_interface(llm.clone());
            }

            // 🆕 TOOL-CALLING FIX: Pass LLM provider for tool-calling support
            if let Some(ref provider) = self.llm_provider {
                builder = builder.with_llm_provider(provider.clone());
            }

            // 🟢 P2 FIX: Attach skill registry to agent
            if let Some(ref registry) = self.skill_registry {
                builder = builder.with_skill_registry(registry.clone());
            }

            let (task_id, task_sender) = builder.spawn().await.map_err(|e| {
                error!("❌ builder.spawn() failed for agent {}: {}", agent_id, e);
                GatewayError::agent(format!("Failed to spawn agent: {}", e))
            })?;

            AgentTaskHandle {
                task_sender,
                kernel_task_id: Some(task_id.0),
                config: agent_config,
            }
        } else {
            // Local execution (for testing)
            let _agent = Agent::new(agent_config.clone());
            let (tx, _rx) = mpsc::unbounded_channel();

            AgentTaskHandle {
                task_sender: tx,
                kernel_task_id: None,
                config: agent_config,
            }
        };

        // Store handle
        self.agent_tasks
            .write()
            .await
            .insert(agent_id.clone(), handle);

        // Broadcast event
        self.broadcast_event(GatewayAgentEvent::Created {
            agent_id: agent_id.clone(),
            config: gateway_config,
            timestamp: Utc::now(),
        });

        info!(agent_id = %agent_id, "Agent spawned successfully");

        Ok(AgentHandle {
            agent_id,
            kernel_task_id: None,
        })
    }

    async fn stop(&self, agent_id: &AgentId) -> Result<()> {
        info!(agent_id = %agent_id, "Stopping agent");

        // Transition state
        self.state_manager
            .transition(agent_id, StateTransition::Shutdown)
            .await
            .map_err(|e| GatewayError::agent(format!("Failed to stop agent: {}", e)))?;

        // Remove from active tasks
        self.agent_tasks.write().await.remove(agent_id);

        // Broadcast event
        self.broadcast_event(GatewayAgentEvent::Stopped {
            agent_id: agent_id.clone(),
            timestamp: Utc::now(),
        });

        info!(agent_id = %agent_id, "Agent stopped");
        Ok(())
    }

    async fn status(&self, agent_id: &AgentId) -> Result<GatewayAgentStatus> {
        // Get state from state manager
        let state = self
            .state_manager
            .get_state(agent_id)
            .await
            .map_err(|e| GatewayError::agent(format!("Failed to get state: {}", e)))?;

        // Get task handle for additional info
        let task_handle = self.agent_tasks.read().await.get(agent_id).cloned();

        Ok(GatewayAgentStatus {
            agent_id: agent_id.clone(),
            state: self.convert_state(&state),
            current_task: None, // TODO: Track current task
            last_activity: Utc::now(),
            total_tasks: 0, // TODO: Track task counts
            failed_tasks: 0,
            kernel_task_id: task_handle.and_then(|h| h.kernel_task_id),
        })
    }

    async fn execute_task(&self, agent_id: &AgentId, task: TaskConfig) -> Result<TaskResult> {
        let task_handle = self
            .agent_tasks
            .read()
            .await
            .get(agent_id)
            .cloned()
            .ok_or_else(|| GatewayError::not_found("agent", agent_id))?;

        // 🟢 P2 FIX: Record task started metric
        self.metrics.record_task_started(agent_id, &task.task_type);
        let start_time = std::time::Instant::now();

        // Create task
        let mut parameters = HashMap::new();

        // 🟢 P1 FIX: Extract session metadata from task input and inject into
        // parameters
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&task.input.to_string()) {
            if let Some(session_id) = json.get("session_id").and_then(|v| v.as_str()) {
                parameters.insert("session_id".to_string(), session_id.to_string());
            }
            if let Some(platform) = json.get("platform").and_then(|v| v.as_str()) {
                parameters.insert("platform".to_string(), platform.to_string());
            }
            if let Some(channel_id) = json.get("channel_id").and_then(|v| v.as_str()) {
                parameters.insert("channel_id".to_string(), channel_id.to_string());
            }
            if let Some(user_id) = json.get("user_id").and_then(|v| v.as_str()) {
                parameters.insert("user_id".to_string(), user_id.to_string());
            }
        }

        let agent_task = crate::Task {
            id: uuid::Uuid::new_v4().to_string(),
            task_type: self.convert_task_type(&task.task_type),
            input: task.input.to_string(),
            parameters,
        };

        // Create oneshot channel for result
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();

        // Send task to agent
        let kernel_request = crate::kernel_integration::KernelTaskRequest {
            task: agent_task,
            result_tx,
        };

        task_handle
            .task_sender
            .send(kernel_request)
            .map_err(|_| GatewayError::agent("Agent task channel closed".to_string()))?;

        // Wait for result with timeout
        let timeout = tokio::time::Duration::from_secs(task.timeout_secs);
        let result = tokio::time::timeout(timeout, result_rx)
            .await
            .map_err(|_| GatewayError::agent("Task execution timeout".to_string()))?
            .map_err(|_| GatewayError::agent("Task result channel closed".to_string()))?;

        // 🟢 P2 FIX: Calculate duration and record completion metric
        let duration_ms = start_time.elapsed().as_millis() as u64;

        match result {
            Ok(task_result) => {
                // Record success metric
                self.metrics
                    .record_task_completed(agent_id, &task.task_type, duration_ms);

                Ok(TaskResult {
                    success: true,
                    output: serde_json::to_value(&task_result.output)
                        .unwrap_or(serde_json::Value::Null),
                    execution_time_ms: duration_ms,
                    error: None,
                })
            }
            Err(e) => {
                // Record failure metric
                self.metrics
                    .record_task_failed(agent_id, &task.task_type, "execution_error");

                Ok(TaskResult {
                    success: false,
                    output: serde_json::Value::Null,
                    execution_time_ms: duration_ms,
                    error: Some(e.to_string()),
                })
            }
        }
    }

    async fn send_command(&self, agent_id: &AgentId, command: StateCommand) -> Result<()> {
        let transition = self
            .convert_command(command)
            .ok_or_else(|| GatewayError::bad_request("Invalid state command"))?;

        self.state_manager
            .transition(agent_id, transition)
            .await
            .map_err(|e| GatewayError::agent(format!("Failed to send command: {}", e)))?;

        Ok(())
    }

    async fn list_agents(&self) -> Result<Vec<GatewayAgentStatus>> {
        let agents = self.state_manager.list_agents().await;
        let tasks = self.agent_tasks.read().await;

        let mut statuses = Vec::new();
        for agent_id in agents {
            if let Ok(state) = self.state_manager.get_state(&agent_id).await {
                let task_handle = tasks.get(&agent_id).cloned();

                statuses.push(GatewayAgentStatus {
                    agent_id: agent_id.clone(),
                    state: self.convert_state(&state),
                    current_task: None,
                    last_activity: Utc::now(),
                    total_tasks: 0,
                    failed_tasks: 0,
                    kernel_task_id: task_handle.and_then(|h| h.kernel_task_id),
                });
            }
        }

        Ok(statuses)
    }

    async fn get_config(&self, agent_id: &AgentId) -> Result<GatewayAgentConfig> {
        let task_handle = self
            .agent_tasks
            .read()
            .await
            .get(agent_id)
            .cloned()
            .ok_or_else(|| GatewayError::not_found("agent", agent_id))?;

        let config = &task_handle.config;

        Ok(GatewayAgentConfig {
            id: config.id.clone(),
            name: config.name.clone(),
            description: config.description.clone(),
            version: config.version.clone(),
            capabilities: config
                .capabilities
                .iter()
                .map(|c| AgentCapability {
                    name: c.clone(),
                    version: "1.0".to_string(),
                    params: HashMap::new(),
                })
                .collect(),
            llm_config: LlmConfig {
                provider: config.models.provider.clone(),
                model: config.models.model.clone(),
                api_key: Some(String::new()), // API key not stored in ModelConfig
                temperature: config.models.temperature,
                max_tokens: config.models.max_tokens,
            },
            memory_config: MemoryConfig {
                memory_type: "local".to_string(),
                storage_path: "data/memory".to_string(),
                max_entries: config.memory.episodic_capacity,
            },
            extra: HashMap::new(),
        })
    }

    async fn update_config(&self, agent_id: &AgentId, _config: GatewayAgentConfig) -> Result<()> {
        // TODO: Implement config update
        warn!(agent_id = %agent_id, "Config update not yet implemented");
        Ok(())
    }

    async fn subscribe_events(&self) -> Result<broadcast::Receiver<GatewayAgentEvent>> {
        Ok(self.event_tx.subscribe())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_gateway_agent_runtime_creation() {
        let config = RuntimeConfig {
            max_agents: 10,
            kernel_enabled: false,
            sandbox_level: beebotos_gateway_lib::agent_runtime::SandboxLevel::None,
            database_url: "sqlite://./data/beebotos.db".to_string(),
        };

        let runtime = GatewayAgentRuntime::new(None, None, config, None).await;
        assert!(runtime.is_ok());
    }
}
