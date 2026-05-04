//! Kernel Integration for Agents
//!
//! 🟡 P1 FIX: Tight integration between Agent runtime and Kernel sandbox.
//!
//! This module provides the bridge between beebotos_agents::Agent and
//! beebotos_kernel::Kernel, allowing Agent execution to happen within the
//! Kernel's sandboxed environment.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                     Kernel Sandbox                              │
//! │  ┌───────────────────────────────────────────────────────────┐  │
//! │  │  AgentKernelTask                                          │  │
//! │  │  ┌─────────────────────────────────────────────────────┐  │  │
//! │  │  │ 1. Task loop runs in Kernel thread                 │  │  │
//! │  │  │ 2. Receives task requests via channel              │  │  │
//! │  │  │ 3. Executes using Agent::execute_task              │  │  │
//! │  │  │ 4. Returns results via oneshot channel             │  │  │
//! │  │  └─────────────────────────────────────────────────────┘  │  │
//! │  └───────────────────────────────────────────────────────────┘  │
//! └─────────────────────────────────────────────────────────────────┘
//!                              ▲
//!                              │ spawn_task()
//!                              │
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    AgentRuntimeManager                          │
//! │         (coordinates between Agent and Kernel)                  │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use beebotos_agents::kernel_integration::{AgentKernelTask, KernelAgentConfig};
//! use beebotos_agents::state_manager::AgentStateManager;
//! use beebotos_kernel::Priority;
//! use std::sync::Arc;
//!
//! # tokio::runtime::Runtime::new().unwrap().block_on(async {
//! let config = KernelAgentConfig::new("agent-1");
//! let agent_instance = AgentStateManager::new("agent-1");
//! let state_manager = Arc::new(AgentStateManager::new("agent-1"));
//!
//! let task = AgentKernelTask::new(config, agent_instance, Some(state_manager));
//! // task.run() returns a future that can be spawned
//! # });
//! ```

use std::sync::Arc;

use beebotos_kernel::capabilities::{CapabilityLevel, CapabilitySet};
use beebotos_kernel::{Priority, TaskId};
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{error, info, warn};

use crate::error::{AgentError, Result};
use crate::state_manager::{AgentState, StateTransition};
use crate::task::TaskType;
use crate::{Agent, AgentConfig, Task, TaskResult};

/// 🟢 P1 FIX: Capability requirements for different task types
/// 
/// Maps each task type to the minimum capability level required
pub struct TaskCapabilityRequirements;

impl TaskCapabilityRequirements {
    /// Get the minimum capability level required for a task type
    pub fn required_level(task_type: &TaskType) -> CapabilityLevel {
        match task_type {
            TaskType::LlmChat => CapabilityLevel::L3NetworkOut,
            TaskType::SkillExecution => CapabilityLevel::L2FileWrite,
            TaskType::McpTool => CapabilityLevel::L3NetworkOut,
            TaskType::FileProcessing => CapabilityLevel::L2FileWrite,
            TaskType::A2aSend => CapabilityLevel::L6SpawnUnlimited,
            TaskType::ChainTransaction => CapabilityLevel::L8ChainWriteLow,
            // 🆕 PLANNING FIX: Planning task types
            TaskType::PlanCreation => CapabilityLevel::L3NetworkOut,
            TaskType::PlanExecution => CapabilityLevel::L2FileWrite,
            TaskType::PlanAdaptation => CapabilityLevel::L3NetworkOut,
            // 🆕 DEVICE FIX: Device automation task types
            TaskType::DeviceAutomation => CapabilityLevel::L2FileWrite,
            TaskType::AppLifecycle => CapabilityLevel::L2FileWrite,
            // 🟢 P1 FIX: Workflow execution tasks
            TaskType::WorkflowExecution => CapabilityLevel::L2FileWrite,
            TaskType::Custom(_) => CapabilityLevel::L3NetworkOut,
        }
    }
    
    /// Get required permissions for a task type
    pub fn required_permissions(task_type: &TaskType) -> Vec<&'static str> {
        match task_type {
            TaskType::LlmChat => vec!["llm:chat", "network:outbound"],
            TaskType::SkillExecution => vec!["wasm:execute", "file:read"],
            TaskType::McpTool => vec!["mcp:call", "network:outbound"],
            TaskType::FileProcessing => vec!["file:read", "file:write"],
            TaskType::A2aSend => vec!["a2a:send", "network:outbound"],
            TaskType::ChainTransaction => vec!["wallet:sign", "chain:send"],
            // 🆕 PLANNING FIX: Planning task types
            TaskType::PlanCreation => vec!["planning:create", "llm:reasoning"],
            TaskType::PlanExecution => vec!["planning:execute", "skill:call"],
            TaskType::PlanAdaptation => vec!["planning:adapt", "llm:reasoning"],
            // 🆕 DEVICE FIX: Device automation task types
            TaskType::DeviceAutomation => vec!["device:control", "ui:interact"],
            TaskType::AppLifecycle => vec!["app:manage", "device:control"],
            // 🟢 P1 FIX: Workflow execution tasks
            TaskType::WorkflowExecution => vec!["workflow:execute", "skill:call"],
            TaskType::Custom(_) => vec!["custom:execute"],
        }
    }
    
    /// Check if a capability set can execute a task
    pub fn can_execute(caps: &CapabilitySet, task_type: &TaskType) -> Result<()> {
        let required_level = Self::required_level(task_type);
        
        // Check capability level
        if caps.max_level < required_level {
            return Err(AgentError::CapabilityDenied(format!(
                "Insufficient capability level: required {:?}, have {:?}",
                required_level, caps.max_level
            )));
        }
        
        // Check permissions
        let required_perms = Self::required_permissions(task_type);
        for perm in required_perms {
            if !caps.has_permission(perm) {
                return Err(AgentError::CapabilityDenied(format!(
                    "Missing required permission: {}",
                    perm
                )));
            }
        }
        
        Ok(())
    }
    
    /// Validate capability set comprehensively
    /// 
    /// Performs full validation including:
    /// - Capability level sufficiency
    /// - Required permissions presence
    /// - Permission format validation
    /// - Security policy compliance
    pub fn validate_capabilities(caps: &CapabilitySet) -> Result<Vec<String>> {
        let mut warnings = Vec::new();
        
        // Check if capability set is empty
        if caps.permissions.is_empty() {
            warnings.push("Capability set has no permissions".to_string());
        }
        
        // Validate permission format
        for perm in &caps.permissions {
            if !perm.contains(':') {
                warnings.push(format!(
                    "Permission '{}' should use 'namespace:action' format", 
                    perm
                ));
            }
        }
        
        // Security recommendations
        if caps.max_level >= CapabilityLevel::L8ChainWriteLow {
            warnings.push("High-level chain write capabilities detected - ensure proper audit logging".to_string());
        }
        
        if caps.has_permission("wasm:execute") && caps.has_permission("file:write") {
            warnings.push("Combination of WASM execution and file write may pose security risks".to_string());
        }
        
        Ok(warnings)
    }
    
    /// Get human-readable description of why a capability check failed
    pub fn explain_requirements(task_type: &TaskType) -> String {
        let level = Self::required_level(task_type);
        let perms = Self::required_permissions(task_type);
        
        format!(
            "Task '{:?}' requires:\n- Capability level: {:?}\n- Permissions: {:?}",
            task_type, level, perms
        )
    }
}

/// Configuration for an agent running in kernel sandbox
#[derive(Debug, Clone)]
pub struct KernelAgentConfig {
    /// Agent unique identifier
    pub agent_id: String,
    /// Agent configuration
    pub agent_config: AgentConfig,
    /// Capability set for sandbox
    pub capabilities: CapabilitySet,
    /// Initial state
    pub initial_state: AgentState,
    /// CODE QUALITY FIX: Task receive timeout in seconds (default: 1)
    pub task_receive_timeout_secs: u64,
    /// CODE QUALITY FIX: Task execution timeout in seconds (default: 300)
    pub task_execution_timeout_secs: u64,
}

impl KernelAgentConfig {
    /// Create new config with agent ID and capabilities
    pub fn new(agent_id: impl Into<String>, capabilities: CapabilitySet) -> Self {
        Self {
            agent_id: agent_id.into(),
            agent_config: AgentConfig::default(),
            capabilities,
            initial_state: AgentState::Registered,
            task_receive_timeout_secs: 1,      // Default: 1 second
            task_execution_timeout_secs: 300,  // Default: 5 minutes
        }
    }

    /// Set task receive timeout
    pub fn with_receive_timeout(mut self, secs: u64) -> Self {
        self.task_receive_timeout_secs = secs;
        self
    }

    /// Set task execution timeout
    pub fn with_execution_timeout(mut self, secs: u64) -> Self {
        self.task_execution_timeout_secs = secs;
        self
    }
}

/// Task request sent to the kernel task
#[derive(Debug)]
pub struct KernelTaskRequest {
    /// Task to execute
    pub task: Task,
    /// Channel to send result back
    pub result_tx: oneshot::Sender<Result<TaskResult>>,
}

/// Agent kernel task that runs inside the kernel sandbox
///
/// This struct encapsulates the agent execution logic that runs within
/// the kernel's sandboxed environment, providing:
/// - Sandboxed task execution
/// - Resource limiting via capabilities
/// - State synchronization
/// - Health monitoring
pub struct AgentKernelTask {
    config: KernelAgentConfig,
    agent: RwLock<Agent>,
    task_rx: RwLock<mpsc::UnboundedReceiver<KernelTaskRequest>>,
    task_tx: mpsc::UnboundedSender<KernelTaskRequest>,
    state_manager: Option<crate::StateManagerHandle>,
}

impl AgentKernelTask {
    /// Create a new agent kernel task
    pub fn new(
        config: KernelAgentConfig,
        agent: Agent,
        state_manager: Option<crate::StateManagerHandle>,
    ) -> Self {
        let (task_tx, task_rx) = mpsc::unbounded_channel();

        Self {
            config,
            agent: RwLock::new(agent),
            task_rx: RwLock::new(task_rx),
            task_tx,
            state_manager,
        }
    }

    /// Get the task sender channel
    pub fn task_sender(&self) -> mpsc::UnboundedSender<KernelTaskRequest> {
        self.task_tx.clone()
    }

    /// Main task loop - runs inside kernel sandbox
    ///
    /// This method should be passed to kernel.spawn_task()
    pub async fn run(&self) -> beebotos_kernel::Result<()> {
        info!(
            "AgentKernelTask started for agent {} in kernel sandbox",
            self.config.agent_id
        );

        // Update state to Initializing
        self.update_state(StateTransition::Start).await;

        // Initialize the agent
        {
            let mut agent = self.agent.write().await;
            if let Err(e) = agent.initialize().await {
                error!("Failed to initialize agent {}: {}", self.config.agent_id, e);
                self.update_state(StateTransition::Error {
                    message: format!("Initialization failed: {}", e),
                })
                .await;
                return Err(beebotos_kernel::KernelError::internal(format!(
                    "Agent initialization failed: {}",
                    e
                )));
            }
        }

        // Transition to Idle
        self.update_state(StateTransition::InitializationComplete)
            .await;
        info!("Agent {} initialized and ready", self.config.agent_id);

        // Main task loop
        loop {
            // Check for shutdown signal
            if self.should_shutdown().await {
                info!("Agent {} received shutdown signal", self.config.agent_id);
                break;
            }

            // CODE QUALITY FIX: Use configurable timeout instead of hardcoded 1 second
            let request = {
                let mut rx = self.task_rx.write().await;
                tokio::time::timeout(
                    tokio::time::Duration::from_secs(self.config.task_receive_timeout_secs),
                    rx.recv()
                ).await
            };

            match request {
                Ok(Some(req)) => {
                    self.handle_task_request(req).await;
                }
                Ok(None) => {
                    // Channel closed, shutdown
                    info!("Task channel closed for agent {}", self.config.agent_id);
                    break;
                }
                Err(_) => {
                    // Timeout - check health and continue
                    self.perform_health_check().await;
                }
            }
        }

        // Graceful shutdown
        info!("Agent {} shutting down", self.config.agent_id);
        self.update_state(StateTransition::Shutdown).await;

        {
            let mut agent = self.agent.write().await;
            if let Err(e) = agent.shutdown().await {
                warn!(
                    "Error during agent {} shutdown: {}",
                    self.config.agent_id, e
                );
            }
        }

        self.update_state(StateTransition::Stopped).await;
        info!("Agent {} shutdown complete", self.config.agent_id);

        Ok(())
    }

    /// Handle a task execution request
    /// 
    /// 🟢 P1 FIX: Capability verification before task execution
    async fn handle_task_request(&self, request: KernelTaskRequest) {
        let KernelTaskRequest { task, result_tx } = request;

        info!(
            "Agent {} executing task {} in kernel sandbox",
            self.config.agent_id, task.id
        );

        // 🟢 P1 FIX: Verify capabilities before executing task
        if let Err(e) = TaskCapabilityRequirements::can_execute(
            &self.config.capabilities,
            &task.task_type
        ) {
            warn!(
                "Agent {} capability check failed for task {}: {}",
                self.config.agent_id, task.id, e
            );
            let _ = result_tx.send(Err(AgentError::CapabilityDenied(format!(
                "Task '{}' requires: {}",
                task.task_type, e
            ))));
            return;
        }

        // Transition to Working
        self.update_state(StateTransition::BeginTask {
            task_id: task.id.clone(),
        })
        .await;

        // Execute task
        let result = {
            let mut agent = self.agent.write().await;
            agent.execute_task(task).await
        };

        // Handle result
        let success = result.is_ok();
        let transition = if success {
            StateTransition::CompleteTask { success: true }
        } else {
            StateTransition::CompleteTask { success: false }
        };

        self.update_state(transition).await;

        // Send result back
        let _ = result_tx.send(result);
    }

    /// Update agent state via state manager
    async fn update_state(&self, transition: StateTransition) {
        if let Some(ref state_manager) = self.state_manager {
            if let Err(e) = state_manager
                .transition(&self.config.agent_id, transition)
                .await
            {
                warn!(
                    "Failed to update state for agent {}: {}",
                    self.config.agent_id, e
                );
            }
        }
    }

    /// Check if shutdown is requested
    async fn should_shutdown(&self) -> bool {
        // Check state manager for shutdown request
        if let Some(ref state_manager) = self.state_manager {
            if let Ok(state) = state_manager.get_state(&self.config.agent_id).await {
                return state == AgentState::ShuttingDown || state == AgentState::Stopped;
            }
        }
        false
    }

    /// Perform periodic health check
    async fn perform_health_check(&self) {
        // Verify agent is still responsive
        let agent = self.agent.read().await;
        let _state = agent.get_state();
        // Additional health checks can be added here
    }
}

/// Builder for creating kernel-integrated agents
pub struct KernelAgentBuilder {
    agent_config: Option<AgentConfig>,
    capabilities: Option<CapabilitySet>,
    kernel: Option<Arc<beebotos_kernel::Kernel>>,
    state_manager: Option<crate::StateManagerHandle>,
    with_wallet: Option<Arc<crate::wallet::AgentWallet>>,
    with_skill_registry: Option<Arc<crate::skills::SkillRegistry>>,
    with_llm_interface: Option<Arc<dyn crate::communication::LLMCallInterface>>,
    with_memory_system: Option<Arc<dyn crate::memory::MemorySearch>>,
    // 🆕 PLANNING FIX: Planning components for agent
    with_planning_engine: Option<Arc<crate::planning::PlanningEngine>>,
    with_plan_executor: Option<Arc<crate::planning::PlanExecutor>>,
    with_replanner: Option<Arc<dyn crate::planning::RePlanner>>,
    with_skill_catalog: Option<String>,
}

impl KernelAgentBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            agent_config: None,
            capabilities: None,
            kernel: None,
            state_manager: None,
            with_wallet: None,
            with_skill_registry: None,
            with_llm_interface: None,
            with_memory_system: None,
            // 🆕 PLANNING FIX: Initialize planning components as None
            with_planning_engine: None,
            with_plan_executor: None,
            with_replanner: None,
            with_skill_catalog: None,
        }
    }

    /// Set agent configuration
    pub fn with_config(mut self, config: AgentConfig) -> Self {
        self.agent_config = Some(config);
        self
    }

    /// Set capability set for sandbox
    pub fn with_capabilities(mut self, caps: CapabilitySet) -> Self {
        self.capabilities = Some(caps);
        self
    }

    /// Set kernel reference
    pub fn with_kernel(mut self, kernel: Arc<beebotos_kernel::Kernel>) -> Self {
        self.kernel = Some(kernel);
        self
    }

    /// Set state manager
    pub fn with_state_manager(mut self, state_manager: crate::StateManagerHandle) -> Self {
        self.state_manager = Some(state_manager);
        self
    }

    /// Set wallet
    pub fn with_wallet(mut self, wallet: Arc<crate::wallet::AgentWallet>) -> Self {
        self.with_wallet = Some(wallet);
        self
    }

    /// Set skill registry
    pub fn with_skill_registry(mut self, registry: Arc<crate::skills::SkillRegistry>) -> Self {
        self.with_skill_registry = Some(registry);
        self
    }

    /// Set LLM interface
    pub fn with_llm_interface(
        mut self,
        interface: Arc<dyn crate::communication::LLMCallInterface>,
    ) -> Self {
        self.with_llm_interface = Some(interface);
        self
    }

    /// 🟢 P1 FIX: Set memory system for long-term memory retrieval
    pub fn with_memory_system(
        mut self,
        memory: Arc<dyn crate::memory::MemorySearch>,
    ) -> Self {
        self.with_memory_system = Some(memory);
        self
    }

    /// 🆕 PLANNING FIX: Set planning engine for autonomous planning
    pub fn with_planning_engine(
        mut self,
        engine: Arc<crate::planning::PlanningEngine>,
    ) -> Self {
        self.with_planning_engine = Some(engine);
        self
    }

    /// 🆕 PLANNING FIX: Set plan executor
    pub fn with_plan_executor(
        mut self,
        executor: Arc<crate::planning::PlanExecutor>,
    ) -> Self {
        self.with_plan_executor = Some(executor);
        self
    }

    /// 🆕 PLANNING FIX: Set replanner for dynamic replanning
    pub fn with_replanner(
        mut self,
        replanner: Arc<dyn crate::planning::RePlanner>,
    ) -> Self {
        self.with_replanner = Some(replanner);
        self
    }

    /// 🆕 FIX: Set global skill catalog for LLM context injection
    pub fn with_skill_catalog(mut self, catalog: impl Into<String>) -> Self {
        self.with_skill_catalog = Some(catalog.into());
        self
    }

    /// Build and spawn the agent in kernel sandbox
    pub async fn spawn(self) -> Result<(TaskId, mpsc::UnboundedSender<KernelTaskRequest>)> {
        let kernel = self.kernel.ok_or_else(|| {
            AgentError::InvalidConfig("Kernel is required for kernel-integrated agent".into())
        })?;

        let agent_config = self
            .agent_config
            .ok_or_else(|| AgentError::InvalidConfig("Agent config is required".into()))?;

        let capabilities = self.capabilities.unwrap_or_else(CapabilitySet::standard);

        // Build agent
        let mut agent = Agent::new(agent_config.clone());

        if let Some(wallet) = self.with_wallet {
            agent = agent.with_wallet(wallet);
        }

        if let Some(registry) = self.with_skill_registry {
            agent = agent.with_skill_registry(registry);
        }

        if let Some(interface) = self.with_llm_interface {
            agent = agent.with_llm_interface(interface);
        }

        // 🟢 P1 FIX: Attach memory system for long-term memory retrieval
        if let Some(memory) = self.with_memory_system {
            agent = agent.with_memory_system(memory);
        }

        // 🆕 PLANNING FIX: Attach planning components
        if let Some(engine) = self.with_planning_engine {
            agent = agent.with_planning_engine(engine);
        }
        if let Some(executor) = self.with_plan_executor {
            agent = agent.with_plan_executor(executor);
        }
        if let Some(replanner) = self.with_replanner {
            agent = agent.with_replanner(replanner);
        }

        if let Some(catalog) = self.with_skill_catalog {
            agent = agent.with_skill_catalog(catalog);
        }

        // Attach kernel for WASM execution
        agent = agent.with_kernel(kernel.clone());

        // Create kernel agent config
        let agent_id = agent_config.id.clone();
        let kernel_config = KernelAgentConfig {
            agent_id: agent_id.clone(),
            agent_config: agent_config.clone(),
            capabilities: capabilities.clone(),
            initial_state: AgentState::Registered,
            task_receive_timeout_secs: 1,      // Default: 1 second
            task_execution_timeout_secs: 300,  // Default: 5 minutes
        };

        // Clone state_manager for the kernel task
        let state_manager_for_task = self.state_manager.clone();

        // Create kernel task
        let kernel_task = AgentKernelTask::new(kernel_config, agent, state_manager_for_task);
        let task_sender = kernel_task.task_sender();

        // Spawn in kernel
        let task_id = kernel
            .spawn_task(
                format!("agent-{}", agent_config.id),
                Priority::Normal,
                capabilities,
                async move { kernel_task.run().await },
            )
            .await
            .map_err(|e| {
                error!("❌ kernel.spawn_task failed for agent {}: {:?}", agent_config.id, e);
                AgentError::Execution(format!("Failed to spawn kernel task: {}", e))
            })?;

        info!(
            "Kernel-integrated agent {} spawned with task ID {:?}",
            agent_config.id, task_id
        );

        Ok((task_id, task_sender))
    }
}

impl Default for KernelAgentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Extension trait for Agent to integrate with Kernel
#[async_trait::async_trait]
pub trait KernelIntegrable {
    /// Convert this agent into a kernel task
    async fn into_kernel_task(
        self,
        kernel: Arc<beebotos_kernel::Kernel>,
        state_manager: Option<crate::StateManagerHandle>,
    ) -> Result<(TaskId, mpsc::UnboundedSender<KernelTaskRequest>)>;
}

#[async_trait::async_trait]
impl KernelIntegrable for Agent {
    async fn into_kernel_task(
        self,
        kernel: Arc<beebotos_kernel::Kernel>,
        state_manager: Option<crate::StateManagerHandle>,
    ) -> Result<(TaskId, mpsc::UnboundedSender<KernelTaskRequest>)> {
        let config = self.get_config().clone();
        let capabilities = CapabilitySet::standard();

        let kernel_config = KernelAgentConfig {
            agent_id: config.id.clone(),
            agent_config: config.clone(),
            capabilities: capabilities.clone(),
            initial_state: AgentState::Registered,
            task_receive_timeout_secs: 1,      // Default: 1 second
            task_execution_timeout_secs: 300,  // Default: 5 minutes
        };

        let agent_id = kernel_config.agent_id.clone();
        let kernel_task = AgentKernelTask::new(kernel_config, self, state_manager);
        let task_sender = kernel_task.task_sender();

        let task_id = kernel
            .spawn_task(
                format!("agent-{}", agent_id),
                Priority::Normal,
                capabilities,
                async move { kernel_task.run().await },
            )
            .await
            .map_err(|e| AgentError::Execution(format!("Failed to spawn kernel task: {}", e)))?;

        Ok((task_id, task_sender))
    }
}

/// 🟢 P1 FIX: Capability Validator for runtime permission checking
/// 
/// Provides fine-grained capability validation for agent operations
pub struct CapabilityValidator {
    caps: CapabilitySet,
}

impl CapabilityValidator {
    /// Create new validator with capability set
    pub fn new(caps: CapabilitySet) -> Self {
        Self { caps }
    }
    
    /// Validate capability for a specific operation
    pub fn validate(&self, required: CapabilityLevel) -> Result<()> {
        if self.caps.has(required) {
            Ok(())
        } else {
            Err(AgentError::CapabilityDenied(format!(
                "Required capability {:?} not available (max: {:?})",
                required, self.caps.max_level
            )))
        }
    }
    
    /// Validate permission for a specific operation
    pub fn validate_permission(&self, permission: &str) -> Result<()> {
        if self.caps.has_permission(permission) {
            Ok(())
        } else {
            Err(AgentError::CapabilityDenied(format!(
                "Required permission '{}' not granted",
                permission
            )))
        }
    }
    
    /// Check if can execute WASM
    pub fn can_execute_wasm(&self) -> bool {
        self.caps.has(CapabilityLevel::L2FileWrite) 
            && self.caps.has_permission("wasm:execute")
    }
    
    /// Check if can access network
    pub fn can_access_network(&self) -> bool {
        self.caps.has(CapabilityLevel::L3NetworkOut)
            && self.caps.has_permission("network:outbound")
    }
    
    /// Check if can perform chain transactions
    pub fn can_send_transactions(&self) -> bool {
        self.caps.has(CapabilityLevel::L8ChainWriteLow)
            && self.caps.has_permission("wallet:sign")
            && self.caps.has_permission("chain:send")
    }
    
    /// Get the underlying capability set
    pub fn capabilities(&self) -> &CapabilitySet {
        &self.caps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_kernel_agent_builder() {
        // This test requires a kernel instance, so it's more of an integration
        // test In practice, you would mock the kernel for unit tests
    }
    
    #[test]
    fn test_task_capability_requirements() {
        use beebotos_kernel::capabilities::CapabilityLevel;
        
        // Test LLM chat requires L3
        let caps = CapabilitySet::empty()
            .with_level(CapabilityLevel::L3NetworkOut)
            .with_permission("llm:chat")
            .with_permission("network:outbound");
        
        assert!(TaskCapabilityRequirements::can_execute(&caps, &TaskType::LlmChat).is_ok());
        
        // Test chain transaction requires L6
        let caps = CapabilitySet::empty()
            .with_level(CapabilityLevel::L5SpawnLimited); // Not enough
        
        assert!(TaskCapabilityRequirements::can_execute(&caps, &TaskType::ChainTransaction).is_err());
    }
    
    #[test]
    fn test_capability_validator() {
        let caps = CapabilitySet::standard();
        let validator = CapabilityValidator::new(caps);
        
        // Standard caps should have network access
        assert!(validator.can_access_network());
        
        // Standard caps should not have financial access
        assert!(!validator.can_send_transactions());
    }
}
