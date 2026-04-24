//! Agent Runtime Interface
//!
//! Defines the abstract interface between Gateway and Agent runtime.
//! This module decouples the gateway application from the concrete
//! beebotos_agents implementation, enabling:
//! - Easier testing (mock implementations)
//! - Runtime swapping (different agent backends)
//! - Clear contract definition
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐
//! │     Gateway     │
//! └────────┬────────┘
//!          │ Uses AgentRuntime trait
//!          ▼
//! ┌─────────────────┐
//! │  gateway-lib    │◄──── Trait definition (this module)
//! └────────┬────────┘
//!          │ Implemented by
//!          ▼
//! ┌─────────────────┐
//! │     agents      │◄──── Concrete implementation
//! └─────────────────┘
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Unique identifier for an agent
pub type AgentId = String;

/// Unique identifier for a task
pub type TaskId = String;

/// Agent states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentState {
    /// Agent is registered but not started
    Registered,
    /// Agent is initializing
    Initializing,
    /// Agent is idle and ready
    Idle,
    /// Agent is actively working on a task
    Working,
    /// Agent is paused
    Paused,
    /// Agent is shutting down
    ShuttingDown,
    /// Agent has stopped
    Stopped,
    /// Agent encountered an error
    Error,
}

impl std::fmt::Display for AgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentState::Registered => write!(f, "registered"),
            AgentState::Initializing => write!(f, "initializing"),
            AgentState::Idle => write!(f, "idle"),
            AgentState::Working => write!(f, "working"),
            AgentState::Paused => write!(f, "paused"),
            AgentState::ShuttingDown => write!(f, "shutting_down"),
            AgentState::Stopped => write!(f, "stopped"),
            AgentState::Error => write!(f, "error"),
        }
    }
}

/// Agent capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCapability {
    /// Capability name
    pub name: String,
    /// Capability version
    pub version: String,
    /// Capability parameters
    pub params: HashMap<String, String>,
}

/// Agent configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Agent unique ID
    pub id: AgentId,
    /// Agent name
    pub name: String,
    /// Agent description
    pub description: String,
    /// Agent version
    pub version: String,
    /// Agent capabilities
    pub capabilities: Vec<AgentCapability>,
    /// LLM provider configuration
    pub llm_config: LlmConfig,
    /// Memory configuration
    pub memory_config: MemoryConfig,
    /// Additional configuration
    pub extra: HashMap<String, serde_json::Value>,
}

/// LLM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Default provider
    pub provider: String,
    /// Default model
    pub model: String,
    /// API key (encrypted)
    pub api_key: Option<String>,
    /// Temperature
    pub temperature: f32,
    /// Max tokens
    pub max_tokens: u32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            api_key: None,
            temperature: 0.7,
            max_tokens: 2000,
        }
    }
}

/// Memory configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Memory type
    pub memory_type: String,
    /// Storage path
    pub storage_path: String,
    /// Max entries
    pub max_entries: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            memory_type: "local".to_string(),
            storage_path: ".beebotos/memory".to_string(),
            max_entries: 10000,
        }
    }
}

/// Task configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConfig {
    /// Task type
    pub task_type: String,
    /// Task input
    pub input: serde_json::Value,
    /// Timeout in seconds
    pub timeout_secs: u64,
    /// Priority (1-10)
    pub priority: u8,
}

/// Task result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    /// Success flag
    pub success: bool,
    /// Result output
    pub output: serde_json::Value,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Error message (if failed)
    pub error: Option<String>,
}

/// Agent status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    /// Agent ID
    pub agent_id: AgentId,
    /// Current state
    pub state: AgentState,
    /// Current task (if Working)
    pub current_task: Option<TaskId>,
    /// Last activity timestamp
    pub last_activity: DateTime<Utc>,
    /// Total tasks executed
    pub total_tasks: u64,
    /// Failed tasks
    pub failed_tasks: u64,
    /// Kernel task ID (if running in kernel)
    pub kernel_task_id: Option<u64>,
}

/// Agent handle for interacting with a running agent
#[derive(Debug, Clone)]
pub struct AgentHandle {
    /// Agent ID
    pub agent_id: AgentId,
    /// Kernel task ID
    pub kernel_task_id: Option<u64>,
}

/// State transition command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateCommand {
    /// Start the agent
    Start,
    /// Pause the agent
    Pause,
    /// Resume the agent
    Resume,
    /// Stop the agent
    Stop,
    /// Restart the agent
    Restart,
}

/// Agent runtime trait - Abstract interface for agent lifecycle management
///
/// This trait defines the contract between Gateway and Agent runtime.
/// Implementations can be:
/// - `KernelAgentRuntime`: Runs agents in kernel sandbox
/// - `LocalAgentRuntime`: Runs agents locally (for testing)
/// - `MockAgentRuntime`: Mock implementation for tests
#[async_trait]
pub trait AgentRuntime: Send + Sync + 'static {
    /// Spawn a new agent with the given configuration
    ///
    /// # Arguments
    /// * `config` - Agent configuration
    ///
    /// # Returns
    /// * `AgentHandle` - Handle to interact with the spawned agent
    async fn spawn(&self, config: AgentConfig) -> Result<AgentHandle>;

    /// Stop a running agent
    ///
    /// # Arguments
    /// * `agent_id` - Agent ID to stop
    async fn stop(&self, agent_id: &AgentId) -> Result<()>;

    /// Get agent current status
    ///
    /// # Arguments
    /// * `agent_id` - Agent ID
    async fn status(&self, agent_id: &AgentId) -> Result<AgentStatus>;

    /// Execute a task on an agent
    ///
    /// # Arguments
    /// * `agent_id` - Target agent
    /// * `task` - Task configuration
    ///
    /// # Returns
    /// * `TaskResult` - Task execution result
    async fn execute_task(&self, agent_id: &AgentId, task: TaskConfig) -> Result<TaskResult>;

    /// Send a state command to an agent
    ///
    /// # Arguments
    /// * `agent_id` - Target agent
    /// * `command` - State command
    async fn send_command(&self, agent_id: &AgentId, command: StateCommand) -> Result<()>;

    /// List all running agents
    async fn list_agents(&self) -> Result<Vec<AgentStatus>>;

    /// Get agent configuration
    ///
    /// # Arguments
    /// * `agent_id` - Agent ID
    async fn get_config(&self, agent_id: &AgentId) -> Result<AgentConfig>;

    /// Update agent configuration
    ///
    /// # Arguments
    /// * `agent_id` - Agent ID
    /// * `config` - New configuration
    async fn update_config(&self, agent_id: &AgentId, config: AgentConfig) -> Result<()>;

    /// Subscribe to agent events
    ///
    /// # Returns
    /// * Event receiver channel
    async fn subscribe_events(&self) -> Result<tokio::sync::broadcast::Receiver<AgentEvent>>;
}

/// Agent lifecycle events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentEvent {
    /// Agent created
    Created {
        agent_id: AgentId,
        config: AgentConfig,
        timestamp: DateTime<Utc>,
    },
    /// Agent started
    Started {
        agent_id: AgentId,
        timestamp: DateTime<Utc>,
    },
    /// Agent state changed
    StateChanged {
        agent_id: AgentId,
        from: AgentState,
        to: AgentState,
        timestamp: DateTime<Utc>,
    },
    /// Task assigned
    TaskAssigned {
        agent_id: AgentId,
        task_id: TaskId,
        timestamp: DateTime<Utc>,
    },
    /// Task completed
    TaskCompleted {
        agent_id: AgentId,
        task_id: TaskId,
        result: TaskResult,
        timestamp: DateTime<Utc>,
    },
    /// Agent stopped
    Stopped {
        agent_id: AgentId,
        timestamp: DateTime<Utc>,
    },
    /// Agent error
    Error {
        agent_id: AgentId,
        error: String,
        timestamp: DateTime<Utc>,
    },
}

/// Agent runtime factory
pub trait AgentRuntimeFactory: Send + Sync {
    /// Create a new agent runtime instance
    fn create(&self, config: RuntimeConfig) -> Result<Arc<dyn AgentRuntime>>;
}

/// Runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Max concurrent agents
    pub max_agents: usize,
    /// Kernel integration enabled
    pub kernel_enabled: bool,
    /// Sandbox level
    pub sandbox_level: SandboxLevel,
    /// Database connection string
    pub database_url: String,
}

/// Sandbox security level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxLevel {
    /// No sandbox (testing only)
    None,
    /// WASM sandbox
    Wasm,
    /// Kernel sandbox with capabilities
    Kernel,
    /// TEE (Trusted Execution Environment)
    Tee,
}

/// Builder for creating agent configurations
#[derive(Debug, Default)]
pub struct AgentConfigBuilder {
    config: AgentConfig,
}

impl AgentConfigBuilder {
    /// Create new builder
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            config: AgentConfig {
                id: id.into(),
                name: name.into(),
                description: String::new(),
                version: "1.0.0".to_string(),
                capabilities: Vec::new(),
                llm_config: LlmConfig::default(),
                memory_config: MemoryConfig::default(),
                extra: HashMap::new(),
            },
        }
    }

    /// Set description
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.config.description = desc.into();
        self
    }

    /// Add capability
    pub fn with_capability(mut self, capability: AgentCapability) -> Self {
        self.config.capabilities.push(capability);
        self
    }

    /// Set LLM configuration
    pub fn with_llm(mut self, config: LlmConfig) -> Self {
        self.config.llm_config = config;
        self
    }

    /// Set memory configuration
    pub fn with_memory(mut self, config: MemoryConfig) -> Self {
        self.config.memory_config = config;
        self
    }

    /// Add extra configuration
    pub fn with_extra(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.config.extra.insert(key.into(), value);
        self
    }

    /// Build configuration
    pub fn build(self) -> AgentConfig {
        self.config
    }
}

/// Mock implementation for testing
#[cfg(test)]
pub mod mock {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use tokio::sync::broadcast;

    use super::*;
    use crate::GatewayError;

    /// Mock agent runtime for testing
    pub struct MockAgentRuntime {
        agents: Mutex<HashMap<AgentId, (AgentConfig, AgentState)>>,
        event_tx: broadcast::Sender<AgentEvent>,
    }

    impl MockAgentRuntime {
        /// Create new mock runtime
        pub fn new() -> Self {
            let (event_tx, _) = broadcast::channel(100);
            Self {
                agents: Mutex::new(HashMap::new()),
                event_tx,
            }
        }
    }

    #[async_trait]
    impl AgentRuntime for MockAgentRuntime {
        async fn spawn(&self, config: AgentConfig) -> Result<AgentHandle> {
            let mut agents = self.agents.lock().unwrap();
            if agents.contains_key(&config.id) {
                return Err(GatewayError::Agent {
                    message: format!("Agent {} already exists", config.id),
                });
            }

            let agent_id = config.id.clone();
            agents.insert(agent_id.clone(), (config.clone(), AgentState::Registered));

            let _ = self.event_tx.send(AgentEvent::Created {
                agent_id: agent_id.clone(),
                config,
                timestamp: Utc::now(),
            });

            Ok(AgentHandle {
                agent_id,
                kernel_task_id: None,
            })
        }

        async fn stop(&self, agent_id: &AgentId) -> Result<()> {
            let mut agents = self.agents.lock().unwrap();
            agents
                .get_mut(agent_id)
                .map(|(_, state)| *state = AgentState::Stopped);

            let _ = self.event_tx.send(AgentEvent::Stopped {
                agent_id: agent_id.clone(),
                timestamp: Utc::now(),
            });

            Ok(())
        }

        async fn status(&self, agent_id: &AgentId) -> Result<AgentStatus> {
            let agents = self.agents.lock().unwrap();
            let (config, state) = agents.get(agent_id).ok_or_else(|| GatewayError::Agent {
                message: format!("Agent {} not found", agent_id),
            })?;

            Ok(AgentStatus {
                agent_id: agent_id.clone(),
                state: *state,
                current_task: None,
                last_activity: Utc::now(),
                total_tasks: 0,
                failed_tasks: 0,
                kernel_task_id: None,
            })
        }

        async fn execute_task(&self, agent_id: &AgentId, task: TaskConfig) -> Result<TaskResult> {
            Ok(TaskResult {
                success: true,
                output: serde_json::json!({"mock": true}),
                execution_time_ms: 100,
                error: None,
            })
        }

        async fn send_command(&self, agent_id: &AgentId, command: StateCommand) -> Result<()> {
            Ok(())
        }

        async fn list_agents(&self) -> Result<Vec<AgentStatus>> {
            let agents = self.agents.lock().unwrap();
            Ok(agents
                .iter()
                .map(|(id, (config, state))| AgentStatus {
                    agent_id: id.clone(),
                    state: *state,
                    current_task: None,
                    last_activity: Utc::now(),
                    total_tasks: 0,
                    failed_tasks: 0,
                    kernel_task_id: None,
                })
                .collect())
        }

        async fn get_config(&self, agent_id: &AgentId) -> Result<AgentConfig> {
            let agents = self.agents.lock().unwrap();
            let (config, _) = agents.get(agent_id).ok_or_else(|| GatewayError::Agent {
                message: format!("Agent {} not found", agent_id),
            })?;
            Ok(config.clone())
        }

        async fn update_config(&self, agent_id: &AgentId, config: AgentConfig) -> Result<()> {
            let mut agents = self.agents.lock().unwrap();
            if let Some((existing, _)) = agents.get_mut(agent_id) {
                *existing = config;
            }
            Ok(())
        }

        async fn subscribe_events(&self) -> Result<broadcast::Receiver<AgentEvent>> {
            Ok(self.event_tx.subscribe())
        }
    }

    impl Default for MockAgentRuntime {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mock::MockAgentRuntime;
    use super::*;

    #[tokio::test]
    async fn test_mock_runtime() {
        let runtime = MockAgentRuntime::new();

        // Create config
        let config = AgentConfigBuilder::new("test-1", "Test Agent")
            .description("A test agent")
            .build();

        // Spawn agent
        let handle = runtime.spawn(config.clone()).await.unwrap();
        assert_eq!(handle.agent_id, "test-1");

        // Check status
        let status = runtime.status(&handle.agent_id).await.unwrap();
        assert_eq!(status.agent_id, "test-1");

        // List agents
        let agents = runtime.list_agents().await.unwrap();
        assert_eq!(agents.len(), 1);

        // Stop agent
        runtime.stop(&handle.agent_id).await.unwrap();

        // Check status after stop
        let status = runtime.status(&handle.agent_id).await.unwrap();
        assert_eq!(status.state, AgentState::Stopped);
    }

    #[test]
    fn test_agent_state_display() {
        assert_eq!(AgentState::Idle.to_string(), "idle");
        assert_eq!(AgentState::Working.to_string(), "working");
        assert_eq!(AgentState::Error.to_string(), "error");
    }

    #[test]
    fn test_config_builder() {
        let config = AgentConfigBuilder::new("agent-1", "My Agent")
            .description("Test description")
            .with_capability(AgentCapability {
                name: "chat".to_string(),
                version: "1.0".to_string(),
                params: HashMap::new(),
            })
            .with_extra("custom_key", serde_json::json!("custom_value"))
            .build();

        assert_eq!(config.id, "agent-1");
        assert_eq!(config.name, "My Agent");
        assert_eq!(config.description, "Test description");
        assert_eq!(config.capabilities.len(), 1);
        assert!(config.extra.contains_key("custom_key"));
    }
}
