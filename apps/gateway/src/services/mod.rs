//! Business Logic Services
//!
//! This module contains business logic orchestration between
//! HTTP handlers and external systems (database, kernel, blockchain).

pub mod agent_resolver;
pub mod agent_runtime_manager;
pub mod agent_service;
pub mod cache_warmer;
pub mod chain_event_parser;
pub mod chain_events;
pub mod chain_service;
pub mod chain_signer;
pub mod chain_transaction;
pub mod dao_service;
pub mod encryption_service;
pub mod identity_cache;
pub mod identity_service;
pub mod llm_provider_db;
pub mod llm_service;
pub use llm_service::{LlmMetrics, MetricsSummary};
pub mod auth_service;
pub mod message_processor;
pub mod multichain_config;
pub mod state_machine_service;
pub mod task_monitor;
pub mod wallet_service;
pub mod webchat_service;
// Re-export commonly used services
#[allow(unused_imports)]
pub use agent_runtime_manager::AgentRuntimeManager;
#[allow(unused_imports)]
pub use agent_service::AgentService;
pub use auth_service::AuthService;
// Re-export chain event types
#[allow(unused_imports)]
pub use chain_event_parser::{ChainEventParser, ParsedEvent};
#[allow(unused_imports)]
pub use chain_events::ChainEventManager;
pub use chain_service::{ChainService, ChainServiceConfig};
pub use dao_service::{DaoService, DaoServiceConfig};
#[allow(unused_imports)]
pub use identity_cache::IdentityCache;
pub use identity_service::{IdentityService, IdentityServiceConfig};
#[allow(unused_imports)]
pub use state_machine_service::{StateMachineService, StateMachineStatistics};
pub use task_monitor::TaskMonitorService;
#[allow(unused_imports)]
pub use task_monitor::{TaskEvent, TaskMonitorHandle};
pub use wallet_service::{WalletService, WalletServiceConfig};
