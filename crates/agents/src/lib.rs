//! BeeBotOS Agent Runtime
//!
//! Layer 3: Autonomous agent runtime with OpenClaw features:
//! - Session isolation and management
//! - Non-blocking subagent spawning
//! - Multi-queue concurrency
//! - Heartbeat and cron scheduling
//! - A2A protocol support
//! - MCP integration
//! - 🔒 P0 FIX: Chain wallet integration for on-chain transactions
//! - 🔒 P0 FIX: Type-safe task processing with TaskType enum
//!
//! # 架构改进 V2
//!
//! ## Service Mesh 模式
//! 统一的服务注册发现中心，接入链上 DID Resolver：
//! ```ignore
//! use beebotos_agents::service_mesh::{AgentServiceMesh, ServiceMeshBuilder};
//! use beebotos_agents::service_mesh::registry::InMemoryServiceRegistry;
//! use beebotos_agents::did::DIDResolver;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let registry = std::sync::Arc::new(InMemoryServiceRegistry::new());
//! let did_resolver = std::sync::Arc::new(DIDResolver::new("http://localhost:8545"));
//! let mesh = ServiceMeshBuilder::new()
//!     .with_registry(registry)
//!     .with_did_resolver(did_resolver)
//!     .build()?;
//! # Ok(())
//! # }
//! ```
//!
//! ## 统一事件总线
//! 复用 `beebotos_core::event::EventBus` 实现跨模块事件通信：
//! ```ignore
//! use beebotos_agents::events::AgentEventBus;
//!
//! # tokio::runtime::Runtime::new().unwrap().block_on(async {
//! let event_bus = AgentEventBus::new();
//! let mut rx = event_bus.subscribe("handler").await;
//! # });
//! ```

pub mod a2a;
pub mod agent_impl;
pub mod channel_registry;
// Channel factories are now in communication::channel module
pub mod communication;
pub mod config_watcher;
pub mod context;

pub mod deduplicator;
pub mod device;
pub mod did;
pub mod error;
pub mod services;
// 🔧 FIX: Error integration with BeeBotOSError
pub mod error_integration;
// 🔧 FIX: Integration tests
#[cfg(test)]
pub mod tests_integration;
// 🔧 FIX: Performance optimizations
pub mod events;
pub mod health;
pub mod llm;
pub mod mcp;
pub mod media;
pub mod memory;
pub mod message_bus;
pub mod metrics;
pub mod models;
pub mod performance_optimizations;
// 🟢 P1 FIX: Planning module - structured agent planning capabilities
pub mod planning;
pub mod queue;
pub mod rate_limit;
pub mod runtime;
pub mod scheduling;
pub mod security;
pub mod session;
pub mod skills;
pub mod spawning;
pub mod tools;
pub mod task;
pub mod timeout;
pub mod types;

// 🟢 P1 FIX: Service Mesh 模式 - 统一服务注册发现
pub mod service_mesh;

// 🟡 P1 FIX: Wallet integration module
pub mod wallet;

// 🆕 OPTIMIZATION: Testing utilities for planning integration
pub mod testing;

// 🔒 P0 FIX: Unified state manager for agent lifecycle
pub mod state_manager;

// 🟡 P1 FIX: Kernel integration for sandboxed agent execution
pub mod kernel_integration;

// Re-export webhook types for Gateway integration
// Re-export core agent and task types
pub use agent_impl::Agent;
// 🆕 PLANNING FIX: Re-export TaskComplexity for planning integration
pub use agent_impl::TaskComplexity;
// 🔒 P0 FIX: Re-export beebotos_chain wallet types for agent use
pub use beebotos_chain::wallet::{AccountInfo, EncryptedMnemonic, HDWallet, Wallet as ChainWallet};
// 🟢 P1 FIX: Re-export unified event bus types
pub use beebotos_core::event::{Event, EventBus as CoreEventBus};
pub use channel_registry::{ChannelInfo, ChannelRegistry};
pub use communication::channel::{
    ChannelFactory, ChannelManager, ChannelManagerConfig, DingTalkChannelFactory,
    DiscordChannelFactory, LarkChannelFactory, PersonalWeChatFactory, SlackChannelFactory,
    TelegramChannelFactory, WebChatFactory,
};
pub use communication::webhook::*;
pub use communication::{
    AgentChannelBinding, AgentMessageDispatcher, ChannelBindingStatus, ChannelInstanceId,
    ChannelInstanceManager, ChannelInstanceRef, ChannelInstanceStatus, InboundMessageRouter,
    MemoryOfflineMessageStore, OfflineMessageStore, OutboundMessageRouter, ReplyRoute,
    RoutingDecision, RoutingRules, SqliteOfflineMessageStore, UserChannelBinding,
    UserChannelConfig, UserMessageContext,
};
pub use deduplicator::{MessageDeduplicator, MessageKey, MessageStatus};
// 🆕 DEVICE FIX: Re-export device module types
pub use device::{
    AndroidController, AndroidDevice, Device, DeviceAutomation, DeviceCapability, DeviceError,
    DeviceInfo, DeviceNode, DeviceStatus, ElementLocator, GestureAction, IosController, IosDevice,
    LocatorType, Point, ScreenBounds, Size, SwipeDirection,
};
pub use events::AgentEventBus;
// 🟢 P1 FIX: AgentLifecycleEvent is in events module, not message_bus
pub use events::AgentLifecycleEvent;
// 🟡 P1 FIX: Re-export kernel integration types
pub use kernel_integration::{
    AgentKernelTask, KernelAgentBuilder, KernelAgentConfig, KernelIntegrable, KernelTaskRequest,
};
pub use memory::{MemoryEntry, MemoryError, MemoryLimitsConfig};
// 🟢 P1 FIX: Message Bus integration
pub use message_bus::{init_message_bus, message_bus, AgentsMessageBus};
// Re-export unified ModelConfig from models module
pub use models::ModelConfig;
// 🟢 P1 FIX: Re-export planning module types
pub use planning::{
    // Core plan types
    Action,
    // Executor types
    ActionHandler,
    // Replanner types
    AdaptationResult,
    AdaptationStrategy,
    // Engine types
    ChainOfThoughtPlanner,
    // Decomposer types
    CompositeDecomposer,
    CompositeRePlanner,
    ConditionRePlanner,
    Decomposer,
    DecompositionContext,
    DecompositionStrategy,
    DefaultActionHandler,
    DomainDecomposer,
    ExecutionConfig,
    ExecutionContext,
    ExecutionEvent,
    ExecutionResult,
    ExecutionStrategy,
    FeedbackRePlanner,
    GoalBasedPlanner,
    HierarchicalDecomposer,
    HybridPlanner,
    ParallelDecomposer,
    ParallelExecutor,
    Plan,
    PlanContext,
    PlanExecutor,
    PlanId,
    PlanStatus,
    PlanStep,
    PlanStrategy,
    Planner,
    PlannerToolRegistry,
    PlanningConfig,
    PlanningEngine,
    PlanningError,
    PlanningResult,
    Priority,
    ReActPlanner,
    RePlanTrigger,
    RePlanner,
    ResourceRePlanner,
    SequentialExecutor,
    StepStatus,
    StepType,
    TaskDecomposer,
    ToolExecutor,
};
// 🟢 P1 FIX: Gateway AgentRuntime implementation
pub use runtime::agent_runtime_impl::GatewayAgentRuntime;
// 🟢 P1 FIX: Re-export runtime types for object pool and batch processing
pub use runtime::{
    AgentRuntime, AgentRuntimeBuilder, BatchExecutor, BatchResult, RuntimeConfig,
    RuntimeConfigBuilder, RuntimeMetrics, TaskExecutor,
};
pub use scheduling::{CronScheduler, HeartbeatScheduler};
use serde::{Deserialize, Serialize};
// 🟢 P1 FIX: Re-export Service Mesh types
pub use service_mesh::{
    AgentServiceMesh, LoadBalanceStrategy, ServiceMeshBuilder, ServiceMeshConfig, ServiceMeshStats,
};
pub use services::{
    plaintext_encryptor, AgentChannelBindingStore, AgentChannelService, ChannelConfigEncryptor,
    SqliteAgentChannelBindingStore, SqliteUserChannelStore, UserChannelService, UserChannelStore,
};
pub use session::{SessionContext, SessionKey, SessionType};
pub use spawning::{SpawnConfig, SpawnEngine, SpawnResult};
// 🔒 P0 FIX: Re-export state manager types
pub use state_manager::{
    AgentState, AgentStateManager, AgentStateRecord, AgentStats, StateChangeEvent,
    StateManagerHandle, StateTransition,
};
pub use task::{Artifact, Task, TaskResult, TaskType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub capabilities: Vec<String>,
    pub models: ModelConfig,
    pub memory: MemoryConfig,
    pub personality: PersonalityConfig,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Unnamed Agent".to_string(),
            description: "Default agent configuration".to_string(),
            version: "1.0.0".to_string(),
            capabilities: Vec::new(),
            models: ModelConfig::default(),
            memory: MemoryConfig::default(),
            personality: PersonalityConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub episodic_capacity: usize,
    pub semantic_capacity: usize,
    pub working_memory_size: usize,
    pub consolidation_interval_hours: u32,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            episodic_capacity: 1000,
            semantic_capacity: 500,
            working_memory_size: 10,
            consolidation_interval_hours: 24,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalityConfig {
    pub openness: f32,
    pub conscientiousness: f32,
    pub extraversion: f32,
    pub agreeableness: f32,
    pub neuroticism: f32,
    pub base_mood: String,
}

impl Default for PersonalityConfig {
    fn default() -> Self {
        Self {
            openness: 0.5,
            conscientiousness: 0.5,
            extraversion: 0.5,
            agreeableness: 0.5,
            neuroticism: 0.5,
            base_mood: "neutral".to_string(),
        }
    }
}

// Re-export AgentError from error module
pub use crate::error::AgentError;

impl From<mcp::MCPError> for AgentError {
    fn from(e: mcp::MCPError) -> Self {
        AgentError::MCPError(e.to_string())
    }
}

pub struct AgentBuilder {
    config: AgentConfig,
}

impl AgentBuilder {
    pub fn new(name: &str) -> Self {
        Self {
            config: AgentConfig {
                id: uuid::Uuid::new_v4().to_string(),
                name: name.to_string(),
                description: String::new(),
                version: "0.1.0".to_string(),
                capabilities: vec![],
                models: ModelConfig::default(),
                memory: MemoryConfig {
                    episodic_capacity: 1000,
                    semantic_capacity: 5000,
                    working_memory_size: 10,
                    consolidation_interval_hours: 24,
                },
                personality: PersonalityConfig {
                    openness: 0.5,
                    conscientiousness: 0.5,
                    extraversion: 0.5,
                    agreeableness: 0.5,
                    neuroticism: 0.5,
                    base_mood: "neutral".to_string(),
                },
            },
        }
    }

    pub fn description(mut self, desc: &str) -> Self {
        self.config.description = desc.to_string();
        self
    }

    pub fn with_capability(mut self, capability: &str) -> Self {
        self.config.capabilities.push(capability.to_string());
        self
    }

    pub fn with_model(mut self, provider: &str, model: &str) -> Self {
        self.config.models.provider = provider.to_string();
        self.config.models.model = model.to_string();
        self
    }

    pub fn build(self) -> Agent {
        Agent::new(self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_builder() {
        let agent = AgentBuilder::new("TestAgent")
            .description("A test agent")
            .with_capability("chat")
            .with_model("openai", "gpt-4")
            .build();

        assert_eq!(agent.get_config().name, "TestAgent");
        assert!(agent
            .get_config()
            .capabilities
            .contains(&"chat".to_string()));
    }
}
