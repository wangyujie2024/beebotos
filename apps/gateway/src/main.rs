//! BeeBotOS Gateway Application
//!
//! Production-ready API Gateway using beebotos-gateway-lib for infrastructure.
//! This module focuses on business logic:
//! - HTTP handlers for Agent management
//! - Database persistence
//! - Integration with kernel for Agent execution

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Extension, Query, State};
use axum::middleware::from_fn_with_state;
use axum::routing::{delete, get, post, put};
use axum::Router;
use axum_server::tls_rustls::RustlsConfig;
// Use gateway-lib for infrastructure
use gateway::{
    error::GatewayError,
    middleware::{auth_middleware, cors_layer, rate_limit_middleware, trace_layer, GatewayState},
    rate_limit::RateLimitManager,
    websocket::WebSocketManager,
    Gateway,
};
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use tokio::signal;
use tokio::sync::{mpsc, RwLock};
use tower_http::compression::CompressionLayer;
use tower_http::timeout::TimeoutLayer;
use uuid;

// Business logic modules
mod auth;
mod capability;
pub mod clients;
mod color_theme;
mod config;
mod config_center_integration;
mod config_wizard;
mod error;
mod grpc;
pub mod handlers;
mod health;
mod message_bus;
mod middleware;
mod models;
mod services;
mod state_machine;
mod telemetry;

use beebotos_agents::{
    ChannelRegistry, DingTalkChannelFactory, DiscordChannelFactory, GatewayAgentRuntime,
    LarkChannelFactory, PersonalWeChatFactory, SlackChannelFactory, TelegramChannelFactory,
    WebChatFactory,
};
use beebotos_agents::communication::channel::WeChatFactory;
use beebotos_agents::communication::channel_instance_manager::ChannelInstanceManager;
use beebotos_agents::communication::message_router_v2::{AgentMessageDispatcher, InboundMessageRouter};
use beebotos_agents::communication::offline_message_store_sqlite::SqliteOfflineMessageStore;
use beebotos_agents::services::{
    plaintext_encryptor, AgentChannelService, SqliteAgentChannelBindingStore,
    SqliteUserChannelStore, UserChannelService,
};
use tracing::{error, info, warn};

// 🟢 P1 FIX: Import gateway-lib traits and types
use gateway::{
    RuntimeConfig as AgentRuntimeConfig, SandboxLevel,
    StateStore, StateStoreConfig,
};

use crate::config::{AppConfig, BeeBotOSConfig};
use crate::handlers::http::agents;
use crate::services::agent_runtime_manager::AgentRuntimeManager;
use crate::services::message_processor::MessageProcessor;
use crate::services::agent_resolver::AgentResolver;
// Channel Manager integration
use crate::services::agent_service::AgentService;

/// Agent runtime info managed by this gateway
///
/// Tracks agent state in memory. Kernel integration is handled by AgentService.
#[derive(Debug, Clone)]
pub struct AgentRuntimeInfo {
    pub id: String,
    pub name: String,
    pub status: String,
    pub created_at: u64,
    pub capabilities: Vec<String>,
    /// Kernel information from AgentService
    pub kernel_info: Option<crate::services::agent_service::AgentKernelInfo>,
}

/// Application state combining gateway-lib infrastructure with business state
///
/// Agent lifecycle is managed by AgentService which internally uses
/// beebotos-kernel.
///
/// 🔒 P0 FIX: Unified state management - using StateStore (CQRS) as single source
/// of truth, removed duplicate in-memory HashMap.
///
/// 🟢 P1 FIX: Using AgentRuntime trait for decoupled agent management.
pub struct AppState {
    /// Business configuration (DB, etc.)
    pub config: BeeBotOSConfig,
    /// Database connection pool
    pub db: SqlitePool,
    /// Unified state store (CQRS pattern)
    pub state_store: Arc<gateway::StateStore>,
    /// Agent runtime (trait-based, decoupled from concrete implementation)
    pub agent_runtime: Arc<dyn gateway::AgentRuntime>,
    /// Agent service for business logic (includes kernel integration)
    /// DEPRECATED: Migrate to agent_runtime
    pub agent_service: AgentService,
    /// Agent runtime manager bridging gateway with beebotos_agents
    /// DEPRECATED: Migrate to agent_runtime
    pub agent_runtime_manager: Arc<AgentRuntimeManager>,
    /// Unified state manager handle
    /// DEPRECATED: Migrate to state_store
    pub state_manager: beebotos_agents::StateManagerHandle,
    /// Enhanced state machine service
    pub state_machine_service: Option<Arc<crate::services::StateMachineService>>,
    /// Task monitor service for kernel fault awareness
    pub task_monitor: Option<Arc<crate::services::TaskMonitorService>>,
    /// Chain service for blockchain interactions
    pub chain_service: Option<Arc<crate::services::ChainService>>,
    /// Wallet service for blockchain transactions
    pub wallet_service: Option<Arc<crate::services::WalletService>>,
    /// DAO service for governance operations
    pub dao_service: Option<Arc<crate::services::DaoService>>,
    /// Identity service for on-chain identity management
    pub identity_service: Option<Arc<crate::services::IdentityService>>,
    /// Rate limiter from gateway-lib
    pub rate_limiter: Arc<RateLimitManager>,
    /// Metrics
    pub metrics: telemetry::Metrics,
    /// WebSocket manager from gateway-lib
    pub ws_manager: Option<Arc<WebSocketManager>>,
    /// Webhook handler state
    pub webhook_state: Arc<RwLock<handlers::http::webhooks::WebhookHandlerState>>,
    /// Channel registry for messaging platforms
    pub channel_registry: Option<Arc<ChannelRegistry>>,
    /// Channel event bus sender for starting listeners outside initialization
    pub channel_event_bus: Option<mpsc::Sender<beebotos_agents::communication::channel::ChannelEvent>>,
    /// New multi-instance channel manager
    pub channel_instance_manager: Option<Arc<ChannelInstanceManager>>,
    /// Message dispatcher for inbound webhook events
    pub agent_message_dispatcher: Option<Arc<AgentMessageDispatcher>>,
    /// User channel service (lifecycle + storage)
    pub user_channel_service: Option<Arc<UserChannelService>>,
    /// Agent channel service (bindings)
    pub agent_channel_service: Option<Arc<AgentChannelService>>,
    /// Kernel reference for health checks and direct access
    pub kernel: Arc<beebotos_kernel::Kernel>,
    /// LLM service for processing messages
    pub llm_service: Arc<crate::services::llm_service::LlmService>,
    /// Skill registry for skill management
    pub skill_registry: Option<Arc<beebotos_agents::skills::SkillRegistry>>,
    /// Skill executor for WASM skill execution (cached to avoid recreating WasmEngine)
    pub skill_executor: Option<Arc<beebotos_agents::skills::SkillExecutor>>,
    /// Skill instance manager for instance-based execution model
    pub skill_instance_manager: Option<Arc<beebotos_agents::skills::InstanceManager>>,
    /// Composition registry for skill composition management
    pub composition_registry: Option<Arc<tokio::sync::RwLock<beebotos_agents::skills::composition::CompositionRegistry>>>,
    /// Workflow registry for declarative workflow management
    pub workflow_registry: Option<Arc<tokio::sync::RwLock<beebotos_agents::workflow::WorkflowRegistry>>>,
    /// Active workflow instances (runtime state)
    pub workflow_instances: Option<Arc<tokio::sync::RwLock<std::collections::HashMap<String, beebotos_agents::workflow::WorkflowInstance>>>>,
    /// Workflow trigger engine for cron/event/webhook triggers
    pub workflow_trigger_engine: Option<Arc<tokio::sync::RwLock<beebotos_agents::workflow::TriggerEngine>>>,
    /// Cron job scheduler for workflow triggers (tokio-cron-scheduler)
    pub workflow_cron_scheduler: Option<Arc<tokio_cron_scheduler::JobScheduler>>,
    /// Cron job UUIDs tracked per workflow for dynamic removal
    pub workflow_cron_job_uuids: Arc<tokio::sync::RwLock<std::collections::HashMap<String, Vec<uuid::Uuid>>>>,
    /// Cancellation signals for actively-running workflow instances
    pub workflow_cancel_signals: Arc<tokio::sync::RwLock<std::collections::HashMap<String, std::sync::Arc<std::sync::atomic::AtomicBool>>>>,
    /// Message processor for channel events
    pub message_processor: Option<Arc<MessageProcessor>>,
    /// Agent resolver for mapping channels/users to agents
    pub agent_resolver: Option<Arc<AgentResolver>>,
    /// Channel-to-agent binding store
    pub channel_binding_store: Option<Arc<gateway::ChannelBindingStore>>,
    /// Webchat service for chat persistence
    pub webchat_service: Option<Arc<crate::services::webchat_service::WebchatService>>,
    /// Memory system for agent memory coordination
    pub memory_system: Option<Arc<beebotos_agents::memory::UnifiedMemorySystem>>,
    /// Authentication service
    pub auth_service: Option<Arc<crate::services::AuthService>>,
    /// Config manager for hot-reload
    pub config_manager: Option<Arc<crate::config_center_integration::GatewayConfigManager>>,
    /// Agent event bus for system-wide pub/sub (used by TriggerEngine event listener)
    pub agent_event_bus: Option<beebotos_agents::events::AgentEventBus>,
}

impl AppState {
    // ── Convenience accessors for optional services (reduces handler boilerplate) ──

    pub fn wallet(&self) -> Result<&Arc<crate::services::WalletService>, gateway::error::GatewayError> {
        self.wallet_service.as_ref()
            .ok_or_else(|| gateway::error::GatewayError::service_unavailable("wallet", "Wallet service not initialized"))
    }

    pub fn identity(&self) -> Result<&Arc<crate::services::IdentityService>, gateway::error::GatewayError> {
        self.identity_service.as_ref()
            .ok_or_else(|| gateway::error::GatewayError::service_unavailable("identity", "Identity service not initialized"))
    }

    pub fn dao(&self) -> Result<&Arc<crate::services::DaoService>, gateway::error::GatewayError> {
        self.dao_service.as_ref()
            .ok_or_else(|| gateway::error::GatewayError::service_unavailable("dao", "DAO service not initialized"))
    }

    pub fn chain(&self) -> Result<&Arc<crate::services::ChainService>, gateway::error::GatewayError> {
        self.chain_service.as_ref()
            .ok_or_else(|| gateway::error::GatewayError::service_unavailable("chain", "Chain service not available"))
    }

    pub fn composition_registry(&self) -> Result<&Arc<tokio::sync::RwLock<beebotos_agents::skills::composition::CompositionRegistry>>, gateway::error::GatewayError> {
        self.composition_registry.as_ref()
            .ok_or_else(|| gateway::error::GatewayError::service_unavailable("CompositionRegistry", "Not initialized"))
    }

    pub fn workflow_registry(&self) -> Result<&Arc<tokio::sync::RwLock<beebotos_agents::workflow::WorkflowRegistry>>, gateway::error::GatewayError> {
        self.workflow_registry.as_ref()
            .ok_or_else(|| gateway::error::GatewayError::service_unavailable("WorkflowRegistry", "Not initialized"))
    }

    pub fn workflow_instances(&self) -> Result<&Arc<tokio::sync::RwLock<std::collections::HashMap<String, beebotos_agents::workflow::WorkflowInstance>>>, gateway::error::GatewayError> {
        self.workflow_instances.as_ref()
            .ok_or_else(|| gateway::error::GatewayError::service_unavailable("WorkflowInstances", "Not initialized"))
    }

    pub fn workflow_trigger_engine(&self) -> Result<&Arc<tokio::sync::RwLock<beebotos_agents::workflow::TriggerEngine>>, gateway::error::GatewayError> {
        self.workflow_trigger_engine.as_ref()
            .ok_or_else(|| gateway::error::GatewayError::service_unavailable("TriggerEngine", "Not initialized"))
    }

    pub fn skill_registry(&self) -> Result<&Arc<beebotos_agents::skills::SkillRegistry>, gateway::error::GatewayError> {
        self.skill_registry.as_ref()
            .ok_or_else(|| gateway::error::GatewayError::service_unavailable("SkillRegistry", "Not initialized"))
    }

    /// Create new application state
    ///
    /// 🟢 P1 FIX: Now initializes StateStore (CQRS) and AgentRuntime trait.
    pub async fn new(
        config: BeeBotOSConfig,
        db: SqlitePool,
        ws_manager: Option<Arc<WebSocketManager>>,
        rate_limiter: Arc<RateLimitManager>,
        kernel: Arc<beebotos_kernel::Kernel>,
    ) -> anyhow::Result<Self> {
        // 🟢 P1 FIX: Initialize StateStore (CQRS pattern)
        let state_store_config = StateStoreConfig {
            event_sourcing: true,
            cache_ttl_secs: 300,
            max_cache_entries: 10000,
            event_retention_days: 90,
            audit_logging: true,
        };
        let state_store = Arc::new(
            StateStore::new(db.clone(), state_store_config)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to initialize StateStore: {}", e))?
        );
        info!("✅ StateStore (CQRS) initialized");

        // Initialize Memory System
        let memory_system = {
            use std::path::PathBuf;
            use beebotos_agents::memory::{UnifiedMemorySystem, UnifiedMemoryConfig, markdown_storage::MarkdownStorageConfig};
            let memory_config = UnifiedMemoryConfig {
                storage_config: MarkdownStorageConfig {
                    workspace_dir: PathBuf::from("data/workspace"),
                    ..Default::default()
                },
                search_db_path: PathBuf::from("data/memory_search.db"),
                embedding_config: beebotos_agents::memory::embedding::EmbeddingConfig::mock(1536),
                auto_index: true,
                index_batch_size: 10,
            };
            match UnifiedMemorySystem::new(memory_config).await {
                Ok(sys) => {
                    info!("✅ UnifiedMemorySystem initialized");
                    Some(Arc::new(sys))
                }
                Err(e) => {
                    warn!("❌ Failed to initialize UnifiedMemorySystem: {}", e);
                    None
                }
            }
        };

        // Initialize WebchatService
        let webchat_service = {
            let svc = crate::services::webchat_service::WebchatService::new(db.clone());
            info!("✅ WebchatService initialized");
            Some(Arc::new(svc))
        };

        // Initialize AuthService
        let auth_service = {
            let svc = crate::services::AuthService::new(db.clone());
            info!("✅ AuthService initialized");
            Some(Arc::new(svc))
        };

        // Initialize ConfigManager for hot-reload
        let config_manager = {
            let mgr = crate::config_center_integration::GatewayConfigManager::new(config.clone());
            info!("✅ ConfigManager initialized");
            Some(Arc::new(mgr))
        };

        // Initialize LLM service first (needed by AgentRuntime)
        let llm_service = match crate::services::llm_service::LlmService::new(config.clone()).await {
            Ok(service) => {
                info!("✅ LLM Service initialized with beebotos_agents::llm");
                Arc::new(service)
            }
            Err(e) => {
                error!("❌ Failed to initialize LLM Service: {}", e);
                return Err(anyhow::anyhow!("LLM Service initialization failed: {}", e));
            }
        };

        // 🟢 P1 FIX: Initialize AgentRuntime trait implementation
        let agent_runtime_config = AgentRuntimeConfig {
            max_agents: 1000,
            kernel_enabled: true,
            sandbox_level: SandboxLevel::Kernel,
            database_url: config.database.url.clone(),
        };
        let llm_interface: Arc<dyn beebotos_agents::communication::LLMCallInterface> =
            Arc::new(crate::services::agent_runtime_manager::GatewayLLMInterface::new(llm_service.clone()));

        // 🟢 P0 FIX: Initialize SkillRegistry **before** AgentRuntime so it can be injected
        let skill_registry = Arc::new(beebotos_agents::skills::SkillRegistry::new());
        info!("✅ SkillRegistry initialized");
        restore_skills_from_disk(&skill_registry).await;
        register_builtin_skills(&skill_registry).await;

        let agent_runtime: Arc<dyn gateway::AgentRuntime> = Arc::new(
            GatewayAgentRuntime::new(Some(kernel.clone()), Some(llm_interface), agent_runtime_config, Some(db.clone()))
                .await
                .map_err(|e| anyhow::anyhow!("Failed to initialize AgentRuntime: {}", e))?
                .with_skill_registry(skill_registry.clone())
        );
        info!("✅ AgentRuntime (trait-based) initialized with SkillRegistry");

        // Legacy: Agent runtime manager bridges gateway with beebotos_agents
        let agent_runtime_manager = Arc::new(
            AgentRuntimeManager::new_with_default_state_manager(
                Some(kernel.clone()),
                config.clone(),
                llm_service.clone(),
                memory_system.clone(),
                Some(skill_registry.clone()),
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to initialize AgentRuntimeManager: {}", e))?
        );

        // Legacy: AgentService now owns the kernel integration
        let agent_service = AgentService::new(
            db.clone(),
            kernel.clone(),
            agent_runtime_manager.clone(),
            config.clone(),
        );

        // Initialize webhook state (handlers registered later with dispatcher)
        let webhook_state = Arc::new(RwLock::new(
            handlers::http::webhooks::WebhookHandlerState::new(),
        ));

        // Legacy: Get state manager handle from runtime manager
        let state_manager = agent_runtime_manager.state_manager();

        // Initialize enhanced state machine service
        let state_machine_service = {
            let service = Arc::new(crate::services::StateMachineService::new(
                state_manager.clone(),
            ));
            info!("✅ StateMachineService initialized");
            Some(service)
        };

        // Initialize task monitor service for kernel fault awareness
        let task_monitor = {
            let monitor = Arc::new(crate::services::TaskMonitorService::new(
                kernel.clone(),
                state_machine_service.clone(),
            ));
            info!("✅ TaskMonitorService initialized");
            Some(monitor)
        };

        // 🟢 P1 FIX: Initialize Wallet service
        let wallet_service = if config.blockchain.enabled {
            let wallet_config = crate::services::WalletServiceConfig::from(&config.blockchain);
            match crate::services::WalletService::new(wallet_config).await {
                Ok(service) => {
                    info!("✅ WalletService initialized");
                    Some(Arc::new(service))
                }
                Err(e) => {
                    warn!("❌ Failed to initialize WalletService: {}", e);
                    None
                }
            }
        } else {
            info!("ℹ️ Blockchain disabled, WalletService not initialized");
            None
        };

        // 🟢 P1 FIX: Initialize DAO service
        let dao_service = if config.blockchain.enabled {
            if let Some(ref wallet) = wallet_service {
                let dao_config = crate::services::DaoServiceConfig::from(&config.blockchain);
                match crate::services::DaoService::new(dao_config, wallet.clone()).await {
                    Ok(service) => {
                        info!("✅ DaoService initialized");
                        Some(Arc::new(service))
                    }
                    Err(e) => {
                        warn!("❌ Failed to initialize DaoService: {}", e);
                        None
                    }
                }
            } else {
                warn!("❌ DaoService not initialized: WalletService required");
                None
            }
        } else {
            info!("ℹ️ Blockchain disabled, DaoService not initialized");
            None
        };

        // 🟢 P1 FIX: Initialize Identity service
        let identity_service = if config.blockchain.enabled {
            if let Some(ref wallet) = wallet_service {
                let identity_config = crate::services::IdentityServiceConfig::from(&config.blockchain);
                match crate::services::IdentityService::new(identity_config, wallet.clone()).await {
                    Ok(service) => {
                        info!("✅ IdentityService initialized");
                        Some(Arc::new(service))
                    }
                    Err(e) => {
                        warn!("❌ Failed to initialize IdentityService: {}", e);
                        None
                    }
                }
            } else {
                warn!("❌ IdentityService not initialized: WalletService required");
                None
            }
        } else {
            info!("ℹ️ Blockchain disabled, IdentityService not initialized");
            None
        };

        // Legacy: Initialize Chain service (will be deprecated)
        let chain_service = if config.blockchain.enabled {
            let chain_config = crate::services::ChainServiceConfig::from(&config.blockchain);
            match crate::services::ChainService::new(chain_config).await {
                Ok(service) => {
                    info!("✅ ChainService initialized (legacy)");
                    Some(Arc::new(service))
                }
                Err(e) => {
                    warn!("❌ Failed to initialize ChainService: {}", e);
                    None
                }
            }
        } else {
            info!("ℹ️ Blockchain disabled, ChainService not initialized");
            None
        };

        // Initialize SkillExecutor
        let skill_executor = match beebotos_agents::skills::SkillExecutor::new() {
            Ok(executor) => {
                info!("✅ SkillExecutor initialized");
                Some(Arc::new(executor))
            }
            Err(e) => {
                warn!("⚠️ Failed to initialize SkillExecutor: {}", e);
                None
            }
        };

        // Initialize SkillInstanceManager
        let skill_instance_manager = Arc::new(beebotos_agents::skills::InstanceManager::new());
        info!("✅ SkillInstanceManager initialized");

        // Initialize CompositionRegistry
        let composition_registry = Arc::new(tokio::sync::RwLock::new(
            beebotos_agents::skills::composition::CompositionRegistry::with_dir("data/compositions")
        ));
        {
            let mut registry = composition_registry.write().await;
            let composition_dir = std::path::Path::new("data/compositions");
            if let Err(e) = registry.load_from_dir(composition_dir).await {
                warn!("⚠️ Failed to load compositions from disk: {}", e);
            } else {
                let count = registry.list_all().len();
                info!("✅ CompositionRegistry initialized with {} compositions", count);
            }
        }

        // Initialize WorkflowRegistry
        let workflow_registry = Arc::new(tokio::sync::RwLock::new(
            beebotos_agents::workflow::WorkflowRegistry::new()
        ));
        {
            let mut registry = workflow_registry.write().await;
            let workflow_dir = std::path::Path::new("data/workflows");
            if let Err(e) = registry.load_from_dir(workflow_dir).await {
                warn!("⚠️ Failed to load workflows from disk: {}", e);
            } else {
                let count = registry.list_all().len();
                info!("✅ WorkflowRegistry initialized with {} workflows", count);
            }
        }

        // Initialize workflow instances tracking
        let workflow_instances: Arc<tokio::sync::RwLock<std::collections::HashMap<String, beebotos_agents::workflow::WorkflowInstance>>> =
            Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()));

        // Load persisted workflow instances from database
        {
            use handlers::http::workflows::load_workflow_instances;
            match load_workflow_instances(&db).await {
                Ok(instances) => {
                    let mut inst_map = workflow_instances.write().await;
                    for instance in instances {
                        inst_map.insert(instance.id.clone(), instance);
                    }
                    info!("✅ Loaded {} workflow instances from database", inst_map.len());
                }
                Err(e) => {
                    warn!("⚠️ Failed to load workflow instances from database: {}", e);
                }
            }
        }

        // Initialize workflow trigger engine from loaded workflows
        let mut trigger_engine = beebotos_agents::workflow::TriggerEngine::new();
        {
            let registry = workflow_registry.read().await;
            let mut cron_count = 0;
            let mut webhook_count = 0;
            for def in registry.list_all() {
                for trigger in &def.triggers {
                    match &trigger.trigger_type {
                        beebotos_agents::workflow::TriggerType::Cron { .. } => cron_count += 1,
                        beebotos_agents::workflow::TriggerType::Webhook { .. } => webhook_count += 1,
                        _ => {}
                    }
                }
                trigger_engine.register(def);
            }
            info!("✅ TriggerEngine initialized with {} cron, {} webhook triggers", cron_count, webhook_count);
        }

        // Initialize channel binding store (LEGACY — deprecated)
        // P2 OPTIMIZE: This is the old single-binding system. New code should use
        // UserChannelService + AgentChannelService. Run migrate-bindings API to migrate.
        let channel_binding_store = Arc::new(
            gateway::ChannelBindingStore::new(db.clone())
                .await
                .map_err(|e| anyhow::anyhow!("Failed to initialize ChannelBindingStore: {}", e))?
        );
        info!("✅ ChannelBindingStore initialized (LEGACY — migrate to new system when ready)");

        let agent_resolver = Arc::new(
            AgentResolver::new(
                config.channels.default_agent_id.clone(),
                state_store.clone(),
                agent_runtime.clone(),
                config.clone(),
            )
            .with_channel_binding_store(channel_binding_store.clone()),
        );

        Ok(Self {
            config,
            db,
            state_store,
            agent_runtime,
            agent_service,
            agent_runtime_manager,
            state_manager,
            state_machine_service,
            task_monitor,
            chain_service,
            wallet_service,
            dao_service,
            identity_service,
            rate_limiter,
            metrics: telemetry::Metrics::new(),
            ws_manager,
            webhook_state,
            channel_registry: None,
            channel_event_bus: None,
            channel_instance_manager: None,
            agent_message_dispatcher: None,
            user_channel_service: None,
            agent_channel_service: None,
            kernel,
            llm_service,
            skill_registry: Some(skill_registry),
            skill_executor,
            skill_instance_manager: Some(skill_instance_manager),
            composition_registry: Some(composition_registry),
            workflow_registry: Some(workflow_registry),
            workflow_instances: Some(workflow_instances),
            workflow_trigger_engine: Some(Arc::new(tokio::sync::RwLock::new(trigger_engine))),
            workflow_cron_scheduler: None,
            workflow_cron_job_uuids: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            workflow_cancel_signals: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            message_processor: None,
            agent_resolver: Some(agent_resolver),
            channel_binding_store: Some(channel_binding_store),
            webchat_service,
            memory_system,
            auth_service,
            config_manager,
            agent_event_bus: Some(beebotos_agents::events::AgentEventBus::new()),
        })
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment variables from .env file if present
    dotenvy::dotenv().ok();

    // Detect and apply color theme from command line arguments
    // This must happen before any colored output
    let args: Vec<String> = std::env::args().collect();
    
    // Check for --theme or --no-color arguments
    if let Some(theme) = color_theme::ColorTheme::from_args(&args) {
        theme.apply();
        if theme.colors_enabled() {
            eprintln!("[Config] Using color theme: {}", theme.display_name());
        }
    } else {
        // Auto-detect from environment
        let theme = color_theme::ColorTheme::detect_from_env();
        theme.apply();
    }

    // Load configuration directly (no interactive wizard; all config managed via web admin)
    let app_config = BeeBotOSConfig::load()
        .map_err(|e| anyhow::anyhow!("Failed to load configuration: {}", e))?;
    app_config.validate()?;

    // Set WeChat environment variables from config for webhook handlers
    if let Some(wechat_config) = &app_config.channels.wechat {
        if wechat_config.enabled {
            if let Some(corp_id) = wechat_config.settings.get("corp_id").and_then(|v| v.as_str()) {
                std::env::set_var("WECHAT_CORP_ID", corp_id);
            }
            if let Some(token) = wechat_config.settings.get("token").and_then(|v| v.as_str()) {
                std::env::set_var("WECHAT_TOKEN", token);
            }
            if let Some(aes_key) = wechat_config.settings.get("encoding_aes_key").and_then(|v| v.as_str()) {
                std::env::set_var("WECHAT_ENCODING_AES_KEY", aes_key);
            }
            info!("✅ WeChat environment variables set from config");
        }
    }

    // Initialize telemetry
    telemetry::init_telemetry(&app_config.logging, &app_config.tracing);
    info!("Starting BeeBotOS Gateway v{}", env!("CARGO_PKG_VERSION"));

    // Debug: Print loaded LLM configuration
    info!(
        "📋 Loaded LLM config: default_provider={}, fallback_chain={:?}",
        app_config.models.default_provider, app_config.models.fallback_chain
    );

    // Initialize database
    let db = init_database(&app_config).await?;
    info!("Database connection pool initialized");

    // Set personal WeChat session file path to centralized data directory
    {
        let session_file = std::path::PathBuf::from("data/personal_wechat_session.json");
        std::env::set_var("PERSONAL_WECHAT_SESSION_FILE", session_file.as_os_str());
        info!("个人微信 session 持久化路径: {:?}", session_file);
    }

    // Initialize BeeBotOS Kernel for sandboxed agent execution
    info!("Initializing BeeBotOS Kernel...");
    let kernel = Arc::new(
        beebotos_kernel::KernelBuilder::new()
            .with_max_agents(1000)
            .with_wasm(true)
            .with_tee_auto()
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build kernel: {}", e))?,
    );

    // Start the kernel
    kernel
        .start()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to start kernel: {}", e))?;
    info!("✅ BeeBotOS Kernel initialized and started");

    // 🟢 P1 FIX: Initialize Message Bus
    info!("Initializing Message Bus...");
    let message_bus = message_bus::GatewayMessageBus::new();
    message_bus::init_global_message_bus(message_bus);
    info!("✅ Message Bus initialized");

    // Initialize gateway-lib infrastructure
    let gateway_config = app_config.to_gateway_config()?;
    let gateway = Gateway::new(gateway_config.clone()).await?;

    // Get rate limiter from gateway-lib
    let rate_limiter = gateway.rate_limiter.clone();

    // Get WebSocket manager if enabled
    let ws_manager = gateway.websocket.clone();

    // Start gateway infrastructure
    gateway.start().await?;

    // Initialize Channel Registry
    let (event_tx, mut event_rx) =
        mpsc::channel::<beebotos_agents::communication::channel::ChannelEvent>(1000);
    let channel_registry = init_channel_registry(&app_config, event_tx.clone()).await?;

    // Attach WebSocket manager to WebChat channel if it exists
    if let (Some(ref registry), Some(ref ws)) = (&channel_registry, &ws_manager) {
        if let Some(webchat_channel) = registry.get_channel("webchat").await {
            let guard = webchat_channel.read().await;
            if let Some(wc) = guard.as_any().downcast_ref::<
                beebotos_agents::communication::channel::WebChatChannel
            >() {
                wc.set_ws_manager(ws.clone()).await;
                info!("✅ WebSocket manager attached to WebChat channel");
            }
        }
    }

    // Create application state with kernel
    let mut app_state = AppState::new(
        app_config.clone(),
        db,
        ws_manager,
        rate_limiter.clone(),
        kernel.clone(),
    )
    .await?;
    app_state.channel_registry = channel_registry.clone();
    app_state.channel_event_bus = Some(event_tx.clone());

    // Wire new multi-instance channel infrastructure
    if let Some(ref registry) = channel_registry {
        let instance_manager = registry.instance_manager();

        // Start health monitor for auto-reconnect
        instance_manager.start_health_monitor(Duration::from_secs(30));

        // Initialize SQLite-backed stores
        let db = app_state.db.clone();
        let user_channel_store = Arc::new(SqliteUserChannelStore::new(db.clone()));
        let agent_channel_store = Arc::new(SqliteAgentChannelBindingStore::new(db.clone()));
        let offline_store = Arc::new(SqliteOfflineMessageStore::new(db.clone()));

        // Build routers and dispatcher
        let inbound_router = Arc::new(InboundMessageRouter::new(
            user_channel_store.clone(),
            agent_channel_store.clone(),
        ));
        let dispatcher = Arc::new(AgentMessageDispatcher::new(inbound_router, offline_store));

        // Build services
        let encryptor = plaintext_encryptor();
        let user_channel_service = Arc::new(UserChannelService::new(
            user_channel_store,
            instance_manager.clone(),
            encryptor,
        ));
        let agent_channel_service =
            Arc::new(AgentChannelService::new(agent_channel_store));

        app_state.channel_instance_manager = Some(instance_manager);
        app_state.agent_message_dispatcher = Some(dispatcher.clone());
        app_state.user_channel_service = Some(user_channel_service.clone());
        app_state.agent_channel_service = Some(agent_channel_service.clone());

        // 🟢 P1 FIX: Rebuild AgentResolver with both legacy and new binding systems
        if app_state.agent_resolver.is_some() {
            let new_resolver = AgentResolver::new(
                app_state.config.channels.default_agent_id.clone(),
                app_state.state_store.clone(),
                app_state.agent_runtime.clone(),
                app_state.config.clone(),
            )
            .with_channel_binding_store(
                app_state.channel_binding_store.as_ref().unwrap().clone(),
            )
            .with_agent_channel_service(agent_channel_service)
            .with_user_channel_service(user_channel_service.clone());
            app_state.agent_resolver = Some(Arc::new(new_resolver));
        }

        // Register webhook handlers with dispatcher
        {
            let state = app_state.webhook_state.write().await;
            if let Err(e) = state.register_handlers(Some(dispatcher)).await {
                warn!("Failed to register some webhook handlers: {}", e);
            }
        }
    }

    // Initialize MessageProcessor now that channel_registry is available
    if let Some(ref registry) = channel_registry {
        let clawhub_client = crate::clients::ClawHubClient::new().ok();
        if clawhub_client.is_some() {
            info!("🔍 ClawHub client initialized for skill marketplace integration");
        }
        app_state.message_processor = Some(Arc::new(MessageProcessor::new(
            app_state.llm_service.clone(),
            registry.clone(),
            app_state.memory_system.clone(),
            app_state.webchat_service.clone(),
            app_state.skill_registry.clone(),
            app_state.workflow_registry.clone(),
            clawhub_client,
        )));
    }
    let mut app_state = Arc::new(app_state);

    // 🟢 P1 FIX: Initialize tokio-cron-scheduler for workflow cron triggers
    let cron_scheduler = tokio_cron_scheduler::JobScheduler::new()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create cron scheduler: {}", e))?;
    let mut boot_cron_uuids: std::collections::HashMap<String, Vec<uuid::Uuid>> = std::collections::HashMap::new();
    if let Some(ref registry) = app_state.workflow_registry {
        let reg = registry.read().await;
        for def in reg.list_all() {
            for trigger in &def.triggers {
                if let beebotos_agents::workflow::TriggerType::Cron { schedule, timezone } = &trigger.trigger_type {
                    let state_clone = app_state.clone();
                    let workflow_id = def.id.clone();
                    let sched_str = schedule.clone();
                    let tz_str = timezone.clone().unwrap_or_else(|| "UTC".to_string());
                    let sched_for_job = sched_str.clone();
                    let tz_for_job = tz_str.clone();
                    let wf_id_for_job = workflow_id.clone();
                    let job = tokio_cron_scheduler::Job::new_async(&sched_str, move |_uuid, _l| {
                        let state = state_clone.clone();
                        let wf_id = wf_id_for_job.clone();
                        let sched = sched_for_job.clone();
                        let tz = tz_for_job.clone();
                        let fired_at = chrono::Utc::now().to_rfc3339();
                        Box::pin(async move {
                            let trigger_context = serde_json::json!({
                                "trigger_type": "cron",
                                "schedule": sched,
                                "timezone": tz,
                                "fired_at": fired_at
                            });
                            match handlers::http::workflows::execute_workflow_internal(&state, &wf_id, trigger_context).await {
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
                            if let Err(e) = cron_scheduler.add(j).await {
                                warn!("Failed to add cron job for workflow {}: {}", workflow_id, e);
                            } else {
                                info!("⏰ Registered cron job for workflow {}: {} ({})", workflow_id, sched_str, tz_str);
                                boot_cron_uuids.entry(workflow_id.clone()).or_default().push(job_uuid);
                            }
                        }
                        Err(e) => {
                            warn!("Invalid cron schedule for workflow {}: {}", workflow_id, e);
                        }
                    }
                }
            }
        }
    }
    cron_scheduler.start()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to start cron scheduler: {}", e))?;
    info!("✅ Cron scheduler started with tokio-cron-scheduler");

    // Store scheduler and UUID mappings in AppState for runtime introspection and lifecycle management
    if let Some(state_mut) = Arc::get_mut(&mut app_state) {
        state_mut.workflow_cron_scheduler = Some(Arc::new(cron_scheduler));
        state_mut.workflow_cron_job_uuids = Arc::new(tokio::sync::RwLock::new(boot_cron_uuids));
    }

    // 🟢 P2 FIX: Start TriggerEngine event listener for event-based workflow triggers
    if let (Some(ref trigger_engine), Some(ref event_bus)) = (
        app_state.workflow_trigger_engine.clone(),
        app_state.agent_event_bus.clone()
    ) {
        let te = trigger_engine.clone();
        let event_bus_clone = event_bus.clone();
        let state_clone = app_state.clone();
        tokio::spawn(async move {
            let engine = te.read().await;
            let mut rx = engine.listen_events(event_bus_clone).await;
            drop(engine); // release read lock
            info!("🎧 TriggerEngine event listener started");
            while let Some(matches) = rx.recv().await {
                for m in matches {
                    info!("⚡ Event trigger matched workflow: {}", m.workflow_id);
                    let ctx = m.trigger_context.clone();
                    match handlers::http::workflows::execute_workflow_internal(&state_clone, &m.workflow_id, ctx).await {
                        Ok(instance) => {
                            info!("✅ Event-triggered workflow {} completed: {}", m.workflow_id, instance.status);
                        }
                        Err(e) => {
                            warn!("❌ Event-triggered workflow {} failed: {}", m.workflow_id, e);
                        }
                    }
                }
            }
        });
    }

    // Start event processing loop using app_state
    if app_state.channel_registry.is_some() {
        let app_state_clone = app_state.clone();
        tokio::spawn(async move {
            info!("🎧 Starting Channel event processing loop...");

            loop {
                match event_rx.recv().await {
                    Some(event) => {
                        info!("📨 Received channel event: {:?}", event);

                        if let Some(ref reg) = app_state_clone.channel_registry {
                            if let beebotos_agents::communication::channel::ChannelEvent::MessageReceived {
                                platform,
                                channel_id,
                                message
                            } = &event {
                                // Try Agent-aware processing first
                                if let (Some(processor), Some(resolver)) = (
                                    app_state_clone.message_processor.as_ref(),
                                    app_state_clone.agent_resolver.as_ref()
                                ) {
                                    if let Err(e) = processor.handle_message_via_agent(
                                        *platform,
                                        channel_id,
                                        message.clone(),
                                        resolver.clone(),
                                        app_state_clone.agent_runtime.clone(),
                                    ).await {
                                        error!("❌ Agent message processing error: {}", e);
                                    }
                                } else {
                                    // Fallback to direct LLM processing
                                    warn!("⚠️  MessageProcessor or AgentResolver not available, falling back to direct LLM");
                                    let llm_svc = &app_state_clone.llm_service;
                                    let llm_response = if let Some(channel) = reg.get_channel_by_platform(*platform).await {
                                        let channel_clone = channel.clone();
                                        let download_fn = move |key: &str, msg_id: Option<&str>| {
                                            let chan = channel_clone.clone();
                                            let key = key.to_string();
                                            let msg_id = msg_id.map(|s| s.to_string());
                                            async move {
                                                chan.read().await.download_image(&key, msg_id.as_deref()).await
                                            }
                                        };
                                        llm_svc.process_message_with_images(message, Some(download_fn)).await
                                    } else {
                                        llm_svc.process_message(message).await
                                    };

                                    match llm_response {
                                        Ok(response) => {
                                            info!("🤖 LLM response: {}", response);

                                            let reply_message = beebotos_agents::communication::Message {
                                                id: uuid::Uuid::new_v4(),
                                                thread_id: message.thread_id,
                                                platform: *platform,
                                                message_type: beebotos_agents::communication::MessageType::Text,
                                                content: response,
                                                metadata: std::collections::HashMap::new(),
                                                timestamp: chrono::Utc::now(),
                                            };

                                            if let Some(channel) = reg.get_channel_by_platform(*platform).await {
                                                if let Err(e) = channel.read().await.send(channel_id, &reply_message).await {
                                                    error!("❌ Failed to send reply: {}", e);
                                                } else {
                                                    info!("✅ Reply sent to {:?} channel {}", platform, channel_id);
                                                }
                                            } else {
                                                error!("❌ Channel for platform {:?} not found", platform);
                                            }
                                        }
                                        Err(e) => {
                                            error!("❌ LLM processing error: {}", e);
                                        }
                                    }
                                }
                            }
                        } else {
                            warn!(
                                "⚠️  Channel registry not available, skipping message processing"
                            );
                        }
                    }
                    None => {
                        warn!("Channel event receiver closed");
                        break;
                    }
                }
            }
        });
    }

    // Ensure default agent exists if configured
    if let Some(ref default_agent_id) = app_config.channels.default_agent_id {
        ensure_default_agent(
            app_state.agent_runtime.clone(),
            default_agent_id,
            &app_config,
        ).await;
    }

    // Create gateway state for middleware
    let gateway_state = Arc::new(GatewayState::new(gateway_config, rate_limiter));

    // Create router
    let app = create_router(app_state.clone(), gateway_state);

    // Start server
    let addr = app_config
        .server_addr()
        .map_err(|e| anyhow::anyhow!("Invalid server address: {}", e))?;
    info!("Server configured to listen on {}", addr);

    // Choose between HTTP and HTTPS
    if app_config.tls.as_ref().map(|t| t.enabled).unwrap_or(false) {
        start_https_server(app, addr, &app_config).await?;
    } else {
        start_http_server(app, addr).await?;
    }

    // Start gRPC server for skill registry and instance management
    let grpc_addr = std::net::SocketAddr::from((
        std::net::IpAddr::from_str(&app_config.server.host)
            .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0))),
        app_config.server.grpc_port,
    ));
    let grpc_service = grpc::skills::SkillsGrpcService::new(
        app_state.skill_registry.clone(),
        app_state.skill_instance_manager.clone(),
        app_state.skill_executor.clone(),
        handlers::http::skills::get_skills_base_dir(),
    )
    .with_rating_store(app_state.db.clone());
    tokio::spawn(async move {
        info!("🚀 Starting gRPC SkillRegistry server on {}", grpc_addr);
        if let Err(e) = tonic::transport::Server::builder()
            .add_service(grpc_service.into_server())
            .serve(grpc_addr)
            .await
        {
            error!("❌ gRPC server error: {}", e);
        }
    });

    // 🟢 P1 FIX: Start workflow instance TTL cleanup background loop
    let cleanup_app_state = app_state.clone();
    tokio::spawn(async move {
        info!("🧹 Starting workflow instance TTL cleanup loop");
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            cleanup_workflow_instances(&cleanup_app_state).await;
        }
    });

    // Graceful shutdown
    shutdown_signal().await;
    info!("Shutting down gracefully...");

    // Cleanup
    info!("Shutting down services...");
    gateway.shutdown().await;
    telemetry::shutdown_telemetry();
    info!("Shutdown complete");

    Ok(())
}

/// Channel initialization configuration
const CHANNELS: &[(&str, &str)] = &[
    ("webchat", "webchat_default"),
    ("lark", "lark_main"),
    ("dingtalk", "dingtalk_main"),
    ("telegram", "telegram_main"),
    ("discord", "discord_main"),
    ("slack", "slack_main"),
    ("wechat", "wechat_main"),
    ("personal_wechat", "personal_wechat_main"),
];

/// Try to initialize a single channel if enabled
async fn try_init_channel(
    registry: &ChannelRegistry,
    config: &BeeBotOSConfig,
    platform: &str,
    channel_id: &str,
    event_bus: Option<mpsc::Sender<beebotos_agents::communication::channel::ChannelEvent>>,
) -> anyhow::Result<bool> {
    tracing::info!("🔍 Checking platform: {}", platform);
    let channel_config: Option<crate::config::ChannelConfig> = match platform {
        "lark" => config.channels.lark.clone(),
        "dingtalk" => config.channels.dingtalk.clone(),
        "telegram" => config.channels.telegram.clone(),
        "discord" => config.channels.discord.clone(),
        "slack" => config.channels.slack.clone(),
        "wechat" => {
            tracing::info!("📋 wechat config: {:?}", config.channels.wechat.is_some());
            config.channels.wechat.clone()
        },
        "personal_wechat" => {
            tracing::info!("📋 personal_wechat config: {:?}", config.channels.personal_wechat.is_some());
            config.channels.personal_wechat.clone()
        },
        "webchat" => {
            tracing::info!("📋 webchat config: {:?}", config.channels.webchat.is_some());
            config.channels.webchat.clone().or_else(|| Some(crate::config::ChannelConfig {
                enabled: true,
                settings: std::collections::HashMap::new(),
            }))
        },
        _ => None,
    };

    let Some(cfg) = channel_config else {
        tracing::warn!("⚠️ No config found for platform: {}", platform);
        return Ok(false);
    };

    if !cfg.enabled {
        return Ok(false);
    }

    info!("📱 Creating {} channel...", platform);
    let settings = serde_json::to_value(&cfg.settings)?;
    tracing::info!("🔧 {} settings: {:?}", platform, settings);

    match registry
        .create_channel(platform, &settings)
        .await
    {
        Ok(channel) => {
            info!("✅ {} channel '{}' created successfully", platform, channel_id);

            // Connect and start listener
            {
                let mut guard = channel.write().await;
                if let Err(e) = guard.connect().await {
                    warn!("❌ Failed to connect {} channel: {}", platform, e);
                } else {
                    info!("✅ {} channel '{}' connected", platform, channel_id);
                    if let Some(event_bus) = event_bus {
                        let channel_clone = channel.clone();
                        let platform_name = platform.to_string();
                        let channel_id_name = channel_id.to_string();
                        tokio::spawn(async move {
                            let guard = channel_clone.write().await;
                            if let Err(e) = guard.start_listener(event_bus).await {
                                warn!("❌ Failed to start {} listener: {}", platform_name, e);
                            } else {
                                info!("✅ {} channel '{}' listener started", platform_name, channel_id_name);
                            }
                        });
                    }
                }
            }

            Ok(true)
        }
        Err(e) => {
            warn!("❌ Failed to create {} channel: {}", platform, e);
            Ok(false)
        }
    }
}

/// Initialize Channel Registry
async fn init_channel_registry(
    config: &BeeBotOSConfig,
    event_tx: mpsc::Sender<beebotos_agents::communication::channel::ChannelEvent>,
) -> anyhow::Result<Option<Arc<ChannelRegistry>>> {
    let registry = ChannelRegistry::new(event_tx.clone());

    // Register channel factories
    info!("📦 Registering channel factories...");
    registry.register(Box::new(LarkChannelFactory::new())).await;
    registry
        .register(Box::new(DingTalkChannelFactory::new()))
        .await;
    registry
        .register(Box::new(TelegramChannelFactory::new()))
        .await;
    registry
        .register(Box::new(DiscordChannelFactory::new()))
        .await;
    registry
        .register(Box::new(SlackChannelFactory::new()))
        .await;
    registry
        .register(Box::new(WeChatFactory::new()))
        .await;
    registry
        .register(Box::new(PersonalWeChatFactory::new()))
        .await;
    registry
        .register(Box::new(WebChatFactory::new()))
        .await;
    info!(
        "✅ Registered {} channel factories",
        registry.factory_count().await
    );

    // Create and start channels from configuration
    let mut has_channels = false;

    for (platform, channel_id) in CHANNELS {
        match try_init_channel(&registry, config, platform, channel_id, Some(event_tx.clone())).await {
            Ok(true) => has_channels = true,
            Ok(false) => {}
            Err(e) => warn!("❌ Channel initialization error: {}", e),
        }
    }

    if has_channels {
        info!(
            "✅ Channel Registry initialized with {} active channels",
            registry.channel_count().await
        );
        Ok(Some(Arc::new(registry)))
    } else {
        info!("ℹ️ No channels configured, skipping Channel Registry");
        Ok(None)
    }
}

/// Scan skills directory and load installed skills into registry
async fn restore_skills_from_disk(registry: &Arc<beebotos_agents::skills::SkillRegistry>) {
    let base_dir = handlers::http::skills::get_skills_base_dir();
    if !base_dir.exists() {
        return;
    }

    let mut loader = beebotos_agents::skills::SkillLoader::new();
    loader.add_path(&base_dir);

    let mut restored = 0;
    if let Ok(mut entries) = tokio::fs::read_dir(&base_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let skill_id = match path.file_name().and_then(|n| n.to_str()) {
                Some(id) => id.to_string(),
                None => continue,
            };

            match loader.load_skill(&skill_id).await {
                Ok(skill) => {
                    let category = "general".to_string();
                    registry.register(skill, category, vec![]).await;
                    restored += 1;
                }
                Err(e) => {
                    warn!("Failed to restore skill {}: {}", skill_id, e);
                }
            }
        }
    }

    if restored > 0 {
        info!("✅ Restored {} skills from disk", restored);
    }
}

/// Scan project skills/ directory and register markdown-defined skills as lightweight builtins.
/// Delegates to the shared loader in beebotos-agents to keep behaviour in sync.
async fn register_builtin_skills(registry: &Arc<beebotos_agents::skills::SkillRegistry>) {
    beebotos_agents::skills::builtin_loader::load_builtin_skills(registry).await;
}

/// Initialize database connection pool with retry logic
/// 
/// REL-002: Implements connection retry with exponential backoff
async fn init_database(config: &AppConfig) -> anyhow::Result<SqlitePool> {
    const MAX_RETRIES: u32 = 5;
    const INITIAL_RETRY_DELAY_MS: u64 = 1000;
    const MAX_RETRY_DELAY_MS: u64 = 30000;
    
    let mut last_error = None;
    let mut retry_delay = INITIAL_RETRY_DELAY_MS;
    
    for attempt in 0..MAX_RETRIES {
        match try_connect_database(config).await {
            Ok(pool) => {
                if attempt > 0 {
                    info!("✅ Database connection established after {} attempt(s)", attempt + 1);
                }
                return Ok(pool);
            }
            Err(e) => {
                last_error = Some(e);
                if attempt < MAX_RETRIES - 1 {
                    warn!(
                        "⚠️ Database connection attempt {}/{} failed, retrying in {}ms...",
                        attempt + 1,
                        MAX_RETRIES,
                        retry_delay
                    );
                    tokio::time::sleep(Duration::from_millis(retry_delay)).await;
                    // Exponential backoff with jitter prevention
                    retry_delay = std::cmp::min(retry_delay * 2, MAX_RETRY_DELAY_MS);
                }
            }
        }
    }
    
    Err(anyhow::anyhow!(
        "Failed to connect to database after {} attempts: {:?}",
        MAX_RETRIES,
        last_error
    ))
}

/// Try to connect to database once
async fn try_connect_database(config: &AppConfig) -> anyhow::Result<SqlitePool> {
    // Ensure database directory exists for file-based SQLite
    let db_url = &config.database.url;
    if db_url.starts_with("sqlite:") && !db_url.contains(":memory:") {
        // Handle both "sqlite://path" and "sqlite:path" formats
        let path = if let Some(p) = db_url.strip_prefix("sqlite://") {
            // On Windows, absolute paths may have a leading slash before the drive letter
            // e.g., sqlite:///C:/path -> /C:/path. Strip it so Path can parse correctly.
            if cfg!(target_os = "windows") && p.len() > 2 && p.starts_with('/') && p.as_bytes()[2] == b':' {
                &p[1..]
            } else {
                p
            }
        } else {
            db_url.strip_prefix("sqlite:").unwrap_or(db_url)
        };
        
        if let Some(dir) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(dir)?;
        }
    }

    // SQLx 默认 create_if_missing=false，对于文件型 SQLite 需要显式启用创建
    let connect_options = sqlx::sqlite::SqliteConnectOptions::from_str(db_url)?
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(config.database.max_connections)
        .min_connections(config.database.min_connections)
        .acquire_timeout(Duration::from_secs(config.database.connect_timeout_seconds))
        .idle_timeout(Duration::from_secs(config.database.idle_timeout_seconds))
        .connect_with(connect_options)
        .await?;

    // Test the connection
    sqlx::query("SELECT 1").fetch_one(&pool).await?;

    // Run migrations if enabled
    if config.database.run_migrations {
        info!("Running database migrations...");
        sqlx::migrate!("../../migrations_sqlite").run(&pool).await?;
        info!("Database migrations complete");
    }

    Ok(pool)
}

/// Health check handler
async fn health_handler() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "healthy",
        "version": env!("CARGO_PKG_VERSION"),
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}

/// Readiness check handler
async fn readiness_handler() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "ready",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}

/// Liveness check handler
async fn liveness_handler() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "alive",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}

/// Create API router combining gateway-lib middleware with business handlers
pub fn create_router(app_state: Arc<AppState>, gateway_state: Arc<GatewayState>) -> Router {
    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/health", get(health_handler))
        .route("/ready", get(readiness_handler))
        .route("/live", get(liveness_handler))
        .route("/status", get(system_status_handler))
        .route("/metrics", get(telemetry::metrics_handler))
        // Auth routes (public)
        .route("/api/v1/auth/login", post(handlers::http::auth::login))
        .route("/api/v1/auth/register", post(handlers::http::auth::register))
        .route("/api/v1/auth/refresh", post(handlers::http::auth::refresh_token))
        // WebSocket status endpoint (public for health checks)
        .route("/ws/status", get(handlers::websocket::ws_status_handler))
        // Webhook routes
        .route(
            "/webhook/lark",
            post(handlers::http::webhooks::lark_webhook_handler),
        )
        // WeChat webhook - needs special handling for URL verification (GET with echostr)
        .route(
            "/webhook/wechat",
            get(handlers::http::webhooks::wechat_get_handler)
                .post({
                    let state = app_state.clone();
                    move |Query(params): Query<std::collections::HashMap<String, String>>, body: String| async move {
                        let msg_sig = params.get("msg_signature").map(|s| s.as_str()).unwrap_or("");
                        let ts = params.get("timestamp").map(|s| s.as_str()).unwrap_or("");
                        let nonce = params.get("nonce").map(|s| s.as_str()).unwrap_or("");
                        handlers::http::webhooks::wechat_post_handler_impl(
                            state, msg_sig, ts, nonce, &body
                        ).await
                    }
                }),
        )
        .route(
            "/webhook/:platform",
            post(handlers::http::webhooks::webhook_handler),
        );

    // Protected API routes
    let api_routes = Router::new()
        // Agent API (V1 - Legacy)
        .route("/api/v1/agents", get(agents::list_agents))
        .route("/api/v1/agents", post(agents::create_agent))
        .route("/api/v1/agents/:id", get(agents::get_agent))
        .route("/api/v1/agents/:id", put(agents::update_agent))
        .route("/api/v1/agents/:id", delete(agents::delete_agent))
        .route("/api/v1/agents/:id/logs", get(handlers::http::agent_logs::get_agent_logs))
        // Browser Automation API
        .route("/api/v1/browser/status", get(handlers::http::browser::get_status))
        .route("/api/v1/browser/profiles", get(handlers::http::browser::list_profiles))
        .route("/api/v1/browser/profiles", post(handlers::http::browser::create_profile))
        .route("/api/v1/browser/profiles/:id", delete(handlers::http::browser::delete_profile))
        .route("/api/v1/browser/connect", post(handlers::http::browser::connect))
        .route("/api/v1/browser/disconnect", post(handlers::http::browser::disconnect))
        .route("/api/v1/browser/navigate", post(handlers::http::browser::navigate))
        .route("/api/v1/browser/evaluate", post(handlers::http::browser::evaluate))
        .route("/api/v1/browser/screenshot", post(handlers::http::browser::screenshot))
        .route("/api/v1/browser/batch", post(handlers::http::browser::execute_batch))
        .route("/api/v1/browser/sandboxes", get(handlers::http::browser::list_sandboxes))
        .route("/api/v1/browser/sandboxes", post(handlers::http::browser::create_sandbox))
        .route("/api/v1/browser/sandboxes/:id", delete(handlers::http::browser::delete_sandbox))
        .route("/api/v1/browser/sandboxes/:id/stats", get(handlers::http::browser::get_sandbox_stats))
        // Admin Config API
        .route("/api/v1/admin/config", get(handlers::http::admin_config::get_config))
        .route("/api/v1/admin/config/reload", post(handlers::http::admin_config::reload_config))
        .route("/api/v1/agents/:id/start", post(agents::start_agent))
        .route("/api/v1/agents/:id/stop", post(agents::stop_agent))
        .route("/api/v1/agents/:id/tasks", post(agents::execute_agent_task))
        // Agent API (V2 - AgentRuntime trait + StateStore CQRS)
        .route("/api/v2/agents", get(handlers::http::agents_v2::list_agents_v2))
        .route("/api/v2/agents", post(handlers::http::agents_v2::create_agent_v2))
        .route("/api/v2/agents/:id", get(handlers::http::agents_v2::get_agent_v2))
        .route("/api/v2/agents/:id", delete(handlers::http::agents_v2::delete_agent_v2))
        .route("/api/v2/agents/:id/start", post(handlers::http::agents_v2::start_agent_v2))
        .route("/api/v2/agents/:id/stop", post(handlers::http::agents_v2::stop_agent_v2))
        .route("/api/v2/agents/:id/status", get(handlers::http::agents_v2::get_agent_status_v2))
        .route("/api/v2/agents/:id/tasks", post(handlers::http::agents_v2::execute_task_v2))
        .route("/api/v2/agents/:id/channels", post(handlers::http::agents_v2::bind_agent_channel))
        .route("/api/v2/agents/:id/channels", get(handlers::http::agents_v2::list_agent_channels))
        .route("/api/v2/agents/:id/channels/:channel_id", delete(handlers::http::agents_v2::unbind_agent_channel))
        // P2 FIX: Pure new-system Agent-Channel binding APIs
        .route("/api/v2/agents/:id/agent-channel-bindings", post(handlers::http::agents_v2::bind_agent_channel_v2))
        .route("/api/v2/agents/:id/agent-channel-bindings", get(handlers::http::agents_v2::list_agent_channel_bindings_v2))
        .route("/api/v2/agents/:id/agent-channel-bindings/unbind", post(handlers::http::agents_v2::unbind_agent_channel_v2))
        // Capability API
        .route("/api/v1/capabilities", get(agents::list_capability_types))
        .route("/api/v1/capabilities/validate", post(agents::validate_capabilities))
        // Chain API (V1 - Legacy ChainService)
        .route("/api/v1/chain/status", get(handlers::http::chain::get_chain_status))
        .route("/api/v1/chain/agents/:id/identity", post(handlers::http::chain::register_agent_identity))
        .route("/api/v1/chain/agents/:id/identity", get(handlers::http::chain::get_agent_identity))
        .route("/api/v1/chain/agents/:id/has-identity", get(handlers::http::chain::has_agent_identity))
        .route("/api/v1/chain/dao/summary", get(handlers::http::chain::get_dao_summary))
        .route("/api/v1/chain/dao/proposals", get(handlers::http::chain::list_proposals))
        .route("/api/v1/chain/dao/proposals", post(handlers::http::chain::create_dao_proposal))
        .route("/api/v1/chain/dao/proposals/:id", get(handlers::http::chain::get_proposal))
        .route("/api/v1/chain/dao/proposals/:id/vote", post(handlers::http::chain::cast_vote))
        // Chain API (V2 - Split Services: WalletService, DaoService, IdentityService)
        // Wallet endpoints
        .route("/api/v2/chain/wallet", get(handlers::http::chain_v2::get_wallet_info))
        .route("/api/v2/chain/wallet/transfer", post(handlers::http::chain_v2::transfer))
        // Treasury API
        .route("/api/v1/treasury", get(handlers::http::treasury::get_treasury))
        .route("/api/v1/treasury/transfer", post(handlers::http::treasury::transfer))
        // Identity endpoints
        .route("/api/v2/chain/agents/:id/identity", post(handlers::http::chain_v2::register_agent_identity))
        .route("/api/v2/chain/agents/:id/identity", get(handlers::http::chain_v2::get_agent_identity))
        .route("/api/v2/chain/agents/:id/has-identity", get(handlers::http::chain_v2::has_agent_identity))
        // DAO endpoints
        .route("/api/v2/chain/dao/proposals", get(handlers::http::chain_v2::list_proposals))
        .route("/api/v2/chain/dao/proposals", post(handlers::http::chain_v2::create_dao_proposal))
        .route("/api/v2/chain/dao/proposals/:id", get(handlers::http::chain_v2::get_proposal))
        .route("/api/v2/chain/dao/proposals/:id/vote", post(handlers::http::chain_v2::cast_vote))
        // State Machine API
        .route("/api/v1/states", get(handlers::http::state_machine::list_states))
        .route("/api/v1/states/stats", get(handlers::http::state_machine::get_state_machine_stats))
        .route("/api/v1/states/timeouts", get(handlers::http::state_machine::check_timeouts))
        .route("/api/v1/agents/:id/state", get(handlers::http::state_machine::get_agent_state))
        .route("/api/v1/agents/:id/state/context", get(handlers::http::state_machine::get_agent_state_context))
        .route("/api/v1/agents/:id/state/transitions", get(handlers::http::state_machine::get_valid_transitions))
        .route("/api/v1/agents/:id/state/transition", post(handlers::http::state_machine::transition_state))
        .route("/api/v1/agents/:id/pause", post(handlers::http::state_machine::pause_agent))
        .route("/api/v1/agents/:id/resume", post(handlers::http::state_machine::resume_agent))
        .route("/api/v1/agents/:id/retry", post(handlers::http::state_machine::retry_agent))
        // Task Monitor API
        .route("/api/v1/tasks/stats", get(handlers::http::task_monitor::get_task_monitor_stats))
        .route("/api/v1/tasks/monitored", get(handlers::http::task_monitor::list_monitored_agents))
        .route("/api/v1/tasks/agents/:id", get(handlers::http::task_monitor::get_agent_task_status))
        .route("/api/v1/tasks/agents/:id/cancel", post(handlers::http::task_monitor::cancel_task_monitoring))
        .route("/api/v1/tasks/fault-detection", get(handlers::http::task_monitor::get_fault_detection_status))
        // LLM Metrics API
        .route("/api/v1/llm/metrics", get(handlers::http::llm_metrics::get_llm_metrics))
        .route("/api/v1/llm/config", get(handlers::http::llm_config::get_llm_global_config))
        .route("/api/v1/llm/config", put(handlers::http::llm_config::update_llm_global_config))
        .route("/api/v1/llm/health", get(handlers::http::llm_metrics::get_llm_health))
        // Skills API
        .route("/api/v1/skills", get(handlers::http::skills::list_skills))
        .route("/api/v1/skills/install", post(handlers::http::skills::install_skill))
        .route("/api/v1/skills/:id", get(handlers::http::skills::get_skill))
        .route("/api/v1/skills/:id/uninstall", delete(handlers::http::skills::uninstall_skill))
        .route("/api/v1/skills/:id/execute", post(handlers::http::skills::execute_skill))
        .route("/api/v1/skills/hub/health", get(handlers::http::skills::hub_health))
        // Workflow orchestration API
        .route("/api/v1/workflows", get(handlers::http::workflows::list_workflows))
        .route("/api/v1/workflows", post(handlers::http::workflows::create_workflow))
        .route("/api/v1/workflows/install", post(handlers::http::workflows::install_workflow))
        .route("/api/v1/workflows/:id", get(handlers::http::workflows::get_workflow))
        .route("/api/v1/workflows/:id/source", get(handlers::http::workflows::get_workflow_source))
        .route("/api/v1/workflows/:id", put(handlers::http::workflows::update_workflow))
        .route("/api/v1/workflows/:id", delete(handlers::http::workflows::delete_workflow))
        .route("/api/v1/workflows/:id/uninstall", post(handlers::http::workflows::uninstall_workflow))
        .route("/api/v1/workflows/:id/execute", post(handlers::http::workflows::execute_workflow))
        .route("/api/v1/workflows/:id/status", get(handlers::http::workflows::get_workflow_status))
        // Skill composition API
        .route("/api/v1/compositions", get(handlers::http::compositions::list_compositions))
        .route("/api/v1/compositions", post(handlers::http::compositions::create_composition))
        .route("/api/v1/compositions/:id", get(handlers::http::compositions::get_composition))
        .route("/api/v1/compositions/:id", delete(handlers::http::compositions::delete_composition))
        .route("/api/v1/compositions/:id/execute", post(handlers::http::compositions::execute_composition))
        // Workflow instance APIs
        .route("/api/v1/workflow-instances", get(handlers::http::workflows::list_workflow_instances))
        .route("/api/v1/workflow-instances/:id", get(handlers::http::workflows::get_workflow_instance))
        .route("/api/v1/workflow-instances/:id/cancel", post(handlers::http::workflows::cancel_workflow))
        .route("/api/v1/workflow-instances/:id", delete(handlers::http::workflows::delete_workflow_instance))
        // Workflow webhook triggers (catch-all for registered webhook paths)
        .route("/api/v1/workflows/webhook/*path", post(handlers::http::workflows::workflow_webhook_trigger))
        // Workflow dashboard APIs
        .route("/api/v1/workflows/dashboard/stats", get(handlers::http::workflows::dashboard_stats))
        .route("/api/v1/workflows/dashboard/recent-instances", get(handlers::http::workflows::recent_instances))
        .route("/api/v1/workflows/:id/stats", get(handlers::http::workflows::workflow_stats))
        // Instance-based skill execution
        .route("/api/v1/instances", post(handlers::http::skills::create_instance))
        .route("/api/v1/instances", get(handlers::http::skills::list_instances))
        .route("/api/v1/instances/:id", get(handlers::http::skills::get_instance))
        .route("/api/v1/instances/:id", put(handlers::http::skills::update_instance))
        .route("/api/v1/instances/:id", delete(handlers::http::skills::delete_instance))
        .route("/api/v1/instances/:id/execute", post(handlers::http::skills::execute_instance))
        // Auth routes (protected)
        .route("/api/v1/auth/logout", post(handlers::http::auth::logout))
        .route("/api/v1/auth/me", get(handlers::http::auth::me))
        // User Settings
        .route("/api/v1/user/settings", get(handlers::http::user_settings::get_user_settings))
        .route("/api/v1/user/settings", put(handlers::http::user_settings::update_user_settings))
        // Webchat routes
        .route("/api/v1/webchat/sessions", get(handlers::http::webchat::list_sessions))
        .route("/api/v1/webchat/sessions", post(handlers::http::webchat::create_session))
        .route("/api/v1/webchat/sessions/:id", delete(handlers::http::webchat::delete_session))
        .route("/api/v1/webchat/sessions/:id/messages", get(handlers::http::webchat::get_messages))
        .route("/api/v1/webchat/sessions/:id/title", put(handlers::http::webchat::update_title))
        .route("/api/v1/webchat/sessions/:id/pin", post(handlers::http::webchat::toggle_pin))
        .route("/api/v1/webchat/sessions/:id/archive", post(handlers::http::webchat::archive_session))
        .route("/api/v1/webchat/sessions/:id/clear", post(handlers::http::webchat::clear_messages))
        .route("/api/v1/webchat/sessions/:id/export", get(handlers::http::webchat::export_session))
        .route("/api/v1/webchat/sessions/import", post(handlers::http::webchat::import_session))
        .route("/api/v1/webchat/sessions/:id/messages/stream", post(handlers::http::webchat::send_message_streaming))
        .route("/api/v1/webchat/usage", get(handlers::http::webchat::get_usage))
        .route("/api/v1/webchat/side-questions", post(handlers::http::webchat::create_side_question))
        // Channel routes
        .route("/api/v1/channels", get(handlers::http::channels::list_channels))
        .route("/api/v1/channels/:id", get(handlers::http::channels::get_channel))
        .route("/api/v1/channels/:id", put(handlers::http::channels::update_channel))
        .route("/api/v1/channels/:id/enable", post(handlers::http::channels::set_channel_enabled))
        .route("/api/v1/channels/:id/test", post(handlers::http::channels::test_channel_connection))
        .route("/api/v1/channels/wechat/qr", post(handlers::http::channels::get_wechat_qr))
        .route("/api/v1/channels/wechat/qr/check", post(handlers::http::channels::check_wechat_qr))
        .route("/api/v1/channels/webchat/messages", post(handlers::http::channels::send_webchat_message))
        // P2 FIX: User Channel management APIs
        .route("/api/v2/user-channels", post(handlers::http::user_channels::create_user_channel))
        .route("/api/v2/user-channels", get(handlers::http::user_channels::list_user_channels))
        .route("/api/v2/user-channels/:id", get(handlers::http::user_channels::get_user_channel))
        .route("/api/v2/user-channels/:id", delete(handlers::http::user_channels::delete_user_channel))
        .route("/api/v2/user-channels/:id/connect", post(handlers::http::user_channels::connect_user_channel))
        .route("/api/v2/user-channels/:id/disconnect", post(handlers::http::user_channels::disconnect_user_channel))
        // P2 FIX: Admin migration API
        .route("/api/v2/admin/migrate-bindings", post(handlers::http::agents_v2::migrate_legacy_bindings))
        // WebSocket broadcast (admin only)
        .route(
            "/api/v1/ws/broadcast",
            post(handlers::websocket::ws_broadcast_handler),
        )
        // WebSocket upgrade endpoint (auth required)
        .route("/ws", get(handlers::websocket::ws_handler))
        // Layer: Authentication from gateway-lib
        .layer(from_fn_with_state(gateway_state.clone(), auth_middleware));

    // Combine routes and apply global middleware from gateway-lib
    let app = Router::new().merge(public_routes).merge(api_routes);

    // Apply layers one by one
    let app = app
        .layer(trace_layer())
        .layer(cors_layer(&gateway_state.config.cors))
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::new(Duration::from_secs(
            app_state.config.server.timeout_seconds,
        )))
        .layer(axum::extract::DefaultBodyLimit::max(
            app_state.config.server.max_body_size_mb * 1024 * 1024,
        ))
        .layer(from_fn_with_state(
            gateway_state.clone(),
            rate_limit_middleware,
        ))
        .layer(Extension(app_state.clone()));

    app.with_state(app_state)
}

/// System status handler
/// 
/// OBS-003: Enhanced health check with all components
async fn system_status_handler(
    State(state): State<Arc<AppState>>,
) -> Result<axum::Json<serde_json::Value>, GatewayError> {
    // Use full health check for detailed status
    let health = health::check_system_full(&state).await;
    
    // Get agent count from unified state manager (single source of truth)
    let agent_count = state.state_manager.list_agents().await.len();

    Ok(axum::Json(serde_json::json!({
        "service": "beebotos-gateway",
        "version": env!("CARGO_PKG_VERSION"),
        "status": health.overall,
        "components": {
            "database": health.database,
            "kernel": health.kernel,
            "chain": health.chain,
            "llm_service": health.llm_service,
            "webhook_handler": health.webhook_handler
        },
        "agents": {
            "active": agent_count
        },
        "websocket": state.ws_manager.is_some(),
        "timestamp": health.timestamp
    })))
}

/// Cleanup old workflow instances from memory to prevent unbounded growth
async fn cleanup_workflow_instances(state: &Arc<AppState>) {
    let instances = match state.workflow_instances.as_ref() {
        Some(i) => i,
        None => return,
    };

    let max_age_hours = 24;
    let now = chrono::Utc::now();
    let mut removed = 0;

    {
        let mut inst_map = instances.write().await;
        let to_remove: Vec<String> = inst_map
            .iter()
            .filter(|(_, inst)| {
                inst.status.is_terminal()
                    && inst.completed_at.map(|t| (now - t).num_hours() >= max_age_hours).unwrap_or(false)
            })
            .map(|(id, _)| id.clone())
            .collect();

        for id in to_remove {
            inst_map.remove(&id);
            removed += 1;
        }
    }

    if removed > 0 {
        info!("🧹 Cleaned up {} old workflow instances (older than {}h)", removed, max_age_hours);
    }
}

/// Start HTTP server
async fn start_http_server(app: Router, addr: SocketAddr) -> anyhow::Result<()> {
    warn!("Starting HTTP server (TLS is disabled - not recommended for production)");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("HTTP server listening on {}", listener.local_addr()?);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    Ok(())
}

/// Start HTTPS server
async fn start_https_server(
    app: Router,
    addr: SocketAddr,
    config: &AppConfig,
) -> anyhow::Result<()> {
    info!("Starting HTTPS server");

    let tls = config
        .tls
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("TLS configuration not found"))?;

    let cert_path = tls
        .cert_path
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("TLS cert path must be set when TLS is enabled"))?;
    let key_path = tls
        .key_path
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("TLS key path must be set when TLS is enabled"))?;

    let tls_config = RustlsConfig::from_pem_file(cert_path, key_path).await?;

    info!("HTTPS server listening on {}", addr);

    axum_server::bind_rustls(addr, tls_config)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await?;

    Ok(())
}

/// Ensure default agent exists on startup
///
/// If `channels.default_agent_id` is configured but the agent does not exist,
/// automatically create a generic default agent.
async fn ensure_default_agent(
    agent_runtime: Arc<dyn gateway::AgentRuntime>,
    agent_id: &str,
    config: &BeeBotOSConfig,
) {
    let agent_id_string = agent_id.to_string();
    match agent_runtime.status(&agent_id_string).await {
        Ok(_) => {
            info!("Default agent {} already exists", agent_id);
            return;
        }
        Err(_) => {
            info!("Default agent {} not found, creating...", agent_id);
        }
    }

    let default_provider = config.models.default_provider.clone();
    let model_config = config.models.providers.get(&default_provider).cloned();

    let llm_config = gateway::agent_runtime::LlmConfig {
        provider: default_provider.clone(),
        model: model_config.as_ref().and_then(|m| m.model.clone()).unwrap_or_else(|| "gpt-4".to_string()),
        api_key: model_config.as_ref().and_then(|m| m.api_key.clone()),
        temperature: model_config.as_ref().map(|m| m.temperature).unwrap_or(0.7),
        max_tokens: model_config.as_ref().and_then(|m| m.context_window).unwrap_or(4096) as u32,
    };

    let agent_config = gateway::agent_runtime::AgentConfigBuilder::new(agent_id, "Default Agent")
        .description("Auto-created default agent for channel messages")
        .with_llm(llm_config)
        .with_memory(gateway::agent_runtime::MemoryConfig::default())
        .build();

    match agent_runtime.spawn(agent_config).await {
        Ok(handle) => {
            info!("✅ Default agent {} created successfully", handle.agent_id);
        }
        Err(e) => {
            warn!("❌ Failed to create default agent {}: {}", agent_id, e);
        }
    }
}

/// Graceful shutdown signal handler
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = signal::ctrl_c().await {
            tracing::error!("Failed to install Ctrl+C handler: {}", e);
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(e) => {
                tracing::error!("Failed to install SIGTERM handler: {}", e);
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C signal");
        }
        _ = terminate => {
            info!("Received SIGTERM signal");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Create a test configuration for unit tests
    fn create_test_config() -> BeeBotOSConfig {
        BeeBotOSConfig {
            system_name: "BeeBotOS".to_string(),
            version: "2.0.0".to_string(),
            server: config::ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 8080,
                grpc_port: 50051,
                timeout_seconds: 30,
                max_body_size_mb: 10,
                cors: config::CorsConfig {
                    allowed_origins: vec!["http://localhost:8000".to_string()],
                    allowed_methods: vec!["GET".to_string(), "POST".to_string()],
                    allowed_headers: vec!["Content-Type".to_string()],
                    allow_credentials: true,
                },
            },
            database: config::DatabaseConfig {
                url: "sqlite://./data/beebotos.db".to_string(),
                max_connections: 20,
                min_connections: 5,
                connect_timeout_seconds: 10,
                idle_timeout_seconds: 600,
                run_migrations: true,
            },
            jwt: config::JwtConfig {
                secret: secrecy::SecretString::new("a-very-long-secret-key-at-least-32-chars".to_string()),
                expiry_hours: 24,
                refresh_expiry_hours: 168,
                issuer: "beebotos".to_string(),
                audience: "api".to_string(),
            },
            models: config::ModelsConfig {
                default_provider: "kimi".to_string(),
                fallback_chain: vec!["openai".to_string()],
                request_timeout: 60,
                cost_optimization: false,
                max_tokens: 4096,
                system_prompt: "You are a helpful assistant.".to_string(),
                providers: {
                    let mut map = HashMap::new();
                    map.insert("kimi".to_string(), config::ModelProviderConfig {
                        api_key: Some("test-key".to_string()),
                        base_url: Some("https://api.moonshot.cn".to_string()),
                        model: Some("moonshot-v1-8k".to_string()),
                        temperature: 0.7,
                        deployment: None,
                        context_window: Some(8192),
                    });
                    map
                },
            },
            channels: config::ChannelsConfig {
                auto_download_media: true,
                media_storage_path: "./data/media".to_string(),
                max_file_size_mb: 50,
                context_window_size: 20,
                auto_reply: true,
                enable_typing_indicator: true,
                enabled_platforms: vec!["lark".to_string()],
                default_agent_id: None,
                lark: None,
                dingtalk: None,
                telegram: None,
                discord: None,
                slack: None,
                wechat: None,
                personal_wechat: None,
                webchat: None,
                teams: None,
                twitter: None,
                whatsapp: None,
                signal: None,
                matrix: None,
                imessage: None,
            },
            logging: config::LoggingConfig {
                level: "info".to_string(),
                format: "json".to_string(),
                file: "./data/logs/beebotos.log".to_string(),
                rotation: config::LogRotationConfig {
                    enabled: true,
                    max_size_mb: 100,
                    max_files: 10,
                },
            },
            metrics: config::MetricsConfig {
                enabled: true,
                endpoint: "0.0.0.0:9090".to_string(),
                interval_seconds: 60,
            },
            tracing: config::TracingConfig {
                enabled: false,
                otel_endpoint: None,
                sample_rate: 0.1,
            },
            rate_limit: config::RateLimitConfig {
                enabled: true,
                requests_per_second: 10,
                burst_size: 50,
                cooldown_seconds: 60,
            },
            security: config::SecurityConfig {
                allowed_webhook_ips: vec!["0.0.0.0/0".to_string()],
                verify_webhook_signatures: true,
                encryption_enabled: true,
            },
            tls: None,
            services: None,
            blockchain: config::BlockchainConfig {
                enabled: false,
                chain_id: 10143,
                rpc_url: None,
                agent_wallet_mnemonic: None,
                identity_contract_address: None,
                registry_contract_address: None,
                dao_contract_address: None,
                skill_nft_contract_address: None,
            },
            wizard: color_theme::WizardConfig::default(),
        }
    }

    #[tokio::test]
    async fn test_router_creation() {
        let config = create_test_config();
        let gateway_config = config.to_gateway_config().unwrap();

        let db = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();

        let rate_limiter = Arc::new(RateLimitManager::new(Arc::new(
            gateway::rate_limit::token_bucket::TokenBucketRateLimiter::new(100.0, 200),
        )));

        let kernel = Arc::new(
            beebotos_kernel::KernelBuilder::new()
                .with_max_agents(100)
                .build()
                .unwrap(),
        );

        let app_state = Arc::new(
            AppState::new(config, db, None, rate_limiter.clone(), kernel)
                .await
                .unwrap(),
        );

        let gateway_state = Arc::new(GatewayState::new(gateway_config, rate_limiter));

        let _router = create_router(app_state, gateway_state);
    }

    // ------------------------------------------------------------------
    // ClawHub 客户端测试
    // ------------------------------------------------------------------
    #[tokio::test]
    async fn test_clawhub_client_creation_with_config() {
        let client = clients::ClawHubClient::with_config(
            "https://custom.hub.dev/v1".to_string(),
            Some("test-api-key".to_string()),
        );
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_clawhub_client_creation_without_api_key() {
        let client = clients::ClawHubClient::with_config(
            "https://open.hub.dev".to_string(),
            None,
        );
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_clawhub_get_skill_network_error() {
        let client = clients::ClawHubClient::with_config(
            "http://127.0.0.1:59999/v1".to_string(),
            None,
        )
        .expect("创建客户端失败");

        let result = client.get_skill("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_clawhub_download_skill_network_error() {
        let client = clients::ClawHubClient::with_config(
            "http://127.0.0.1:59999/v1".to_string(),
            None,
        )
        .expect("创建客户端失败");

        let result = client.download_skill("test-skill", None).await;
        assert!(result.is_err());
    }

    // ------------------------------------------------------------------
    // Skill 目录工具函数测试
    // ------------------------------------------------------------------
    #[test]
    fn test_skills_base_dir_default() {
        let base = handlers::http::skills::get_skills_base_dir();
        assert_eq!(base, std::path::PathBuf::from("data/skills"));
    }

    #[test]
    fn test_skill_install_path_construction() {
        let path = handlers::http::skills::get_skill_install_path("my-awesome-skill");
        assert!(path.to_string_lossy().contains("my-awesome-skill"));
        assert!(path.to_string_lossy().contains("data/skills"));
    }

    #[tokio::test]
    async fn test_extract_and_load_skill() {
        use std::io::Write;
        let skill_id = "e2e-test-skill";

        // 构造 ZIP
        let mut zip_buf = Vec::new();
        {
            let mut zip = zip::write::ZipWriter::new(std::io::Cursor::new(&mut zip_buf));
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);

            let manifest = format!(
                r#"id: {id}
name: {id}
version: 1.0.0
description: E2E test skill
author: e2e-test
license: MIT
capabilities: []
permissions: []
entry_point: handle
"#,
                id = skill_id
            );
            zip.start_file("skill.yaml", options).unwrap();
            zip.write_all(manifest.as_bytes()).unwrap();

            zip.start_file("skill.wasm", options).unwrap();
            zip.write_all(b"\0asm\x01\0\0\0").unwrap();

            zip.finish().unwrap();
        }

        // 解压到临时目录
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join(skill_id);
        std::fs::create_dir_all(&skill_dir).unwrap();
        let cursor = std::io::Cursor::new(&zip_buf);
        let mut archive = zip::ZipArchive::new(cursor).expect("读取 ZIP 失败");
        archive.extract(&skill_dir).expect("解压 ZIP 失败");

        assert!(skill_dir.join("skill.yaml").exists());
        assert!(skill_dir.join("skill.wasm").exists());

        // SkillLoader 加载
        let mut loader = beebotos_agents::skills::SkillLoader::new();
        loader.add_path(tmp.path());
        let loaded = loader.load_skill(skill_id).await.expect("加载 skill 失败");

        assert_eq!(loaded.id, skill_id);
        assert_eq!(loaded.manifest.version.to_string(), "1.0.0");
    }

    // ------------------------------------------------------------------
    // Workflow HTTP endpoint tests
    // ------------------------------------------------------------------

    /// Helper to create a test router for workflow HTTP tests
    async fn setup_workflow_test_router() -> Router {
        let config = create_test_config();
        let gateway_config = config.to_gateway_config().unwrap();

        let db = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();

        // Run migrations so workflow tables exist
        sqlx::migrate!("../../migrations_sqlite")
            .run(&db)
            .await
            .ok();

        let rate_limiter = Arc::new(RateLimitManager::new(Arc::new(
            gateway::rate_limit::token_bucket::TokenBucketRateLimiter::new(100.0, 200),
        )));

        let kernel = Arc::new(
            beebotos_kernel::KernelBuilder::new()
                .with_max_agents(100)
                .build()
                .unwrap(),
        );

        let app_state = Arc::new(
            AppState::new(config, db, None, rate_limiter.clone(), kernel)
                .await
                .unwrap(),
        );

        let gateway_state = Arc::new(GatewayState::new(gateway_config, rate_limiter));
        create_router(app_state, gateway_state)
    }

    /// Build an Authorization header with the demo token
    fn demo_auth_header() -> (String, String) {
        ("Authorization".to_string(), "Bearer demo-token".to_string())
    }

    #[tokio::test]
    async fn test_workflow_create_and_list() {
        use tower::ServiceExt;

        let app = setup_workflow_test_router().await;
        let (auth_key, auth_val) = demo_auth_header();

        // 1. Create a workflow
        let workflow_yaml = r#"
id: test_create_workflow
name: "Test Create"
description: "A workflow for testing creation"
version: "1.0.0"
triggers:
  - type: manual
config:
  timeout_sec: 60
  continue_on_failure: false
steps:
  - id: step1
    name: "Step One"
    skill: echo
    params:
      input: "hello"
"#;

        let create_response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/workflows")
                    .header("Content-Type", "application/yaml")
                    .header(&auth_key, &auth_val)
                    .body(workflow_yaml.to_string())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(create_response.status(), 200);
        let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["id"], "test_create_workflow");
        assert_eq!(json["name"], "Test Create");

        // 2. List workflows
        let list_response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/api/v1/workflows")
                    .header(&auth_key, &auth_val)
                    .body(String::new())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(list_response.status(), 200);
        let list_body = axum::body::to_bytes(list_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let list_json: Vec<serde_json::Value> = serde_json::from_slice(&list_body).unwrap();
        assert!(!list_json.is_empty());
        assert!(list_json.iter().any(|w| w["id"] == "test_create_workflow"));
    }

    #[tokio::test]
    async fn test_workflow_get_and_delete() {
        use tower::ServiceExt;

        let app = setup_workflow_test_router().await;
        let (auth_key, auth_val) = demo_auth_header();

        // Create a workflow first
        let workflow_yaml = r#"
id: test_get_delete
name: "Test Get Delete"
description: "For get/delete testing"
version: "1.0.0"
triggers:
  - type: manual
steps:
  - id: s1
    name: "S1"
    skill: echo
    params:
      input: "x"
"#;

        let _ = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/workflows")
                    .header("Content-Type", "application/yaml")
                    .header(&auth_key, &auth_val)
                    .body(workflow_yaml.to_string())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Get the workflow
        let get_response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/api/v1/workflows/test_get_delete")
                    .header(&auth_key, &auth_val)
                    .body(String::new())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(get_response.status(), 200);
        let body = axum::body::to_bytes(get_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["id"], "test_get_delete");

        // Delete the workflow
        let delete_response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("DELETE")
                    .uri("/api/v1/workflows/test_get_delete")
                    .header(&auth_key, &auth_val)
                    .body(String::new())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(delete_response.status(), 200);

        // Verify it's gone
        let get_after = app
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/api/v1/workflows/test_get_delete")
                    .header(&auth_key, &auth_val)
                    .body(String::new())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(get_after.status(), 404);
    }

    #[tokio::test]
    async fn test_workflow_webhook_trigger() {
        use tower::ServiceExt;

        let app = setup_workflow_test_router().await;
        let (auth_key, auth_val) = demo_auth_header();

        // Create a workflow with a webhook trigger
        let workflow_yaml = r#"
id: test_webhook_wf
name: "Test Webhook Workflow"
description: "Triggered by webhook"
version: "1.0.0"
triggers:
  - type: webhook
    path: "/test-webhook"
    method: "POST"
config:
  timeout_sec: 30
  continue_on_failure: false
steps:
  - id: step1
    name: "Process"
    skill: echo
    params:
      input: "webhook received"
"#;

        let create_resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/workflows")
                    .header("Content-Type", "application/yaml")
                    .header(&auth_key, &auth_val)
                    .body(workflow_yaml.to_string())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(create_resp.status(), 200);

        // Trigger via webhook endpoint
        let webhook_payload = serde_json::json!({"event": "test", "data": 42});
        let trigger_resp = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/workflows/webhook/test-webhook")
                    .header("Content-Type", "application/json")
                    .header(&auth_key, &auth_val)
                    .body(webhook_payload.to_string())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(trigger_resp.status(), 200);
        let body = axum::body::to_bytes(trigger_resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["workflow_id"], "test_webhook_wf");
        assert!(json["instance_id"].as_str().is_some());
        assert!(json["status"].as_str().is_some());
    }

    #[tokio::test]
    async fn test_workflow_execute_manual() {
        use tower::ServiceExt;

        let app = setup_workflow_test_router().await;
        let (auth_key, auth_val) = demo_auth_header();

        // Create a simple workflow
        let workflow_yaml = r#"
id: test_execute_manual
name: "Test Execute"
description: "For manual execution testing"
version: "1.0.0"
triggers:
  - type: manual
config:
  timeout_sec: 30
  continue_on_failure: false
steps:
  - id: step1
    name: "Echo"
    skill: echo
    params:
      input: "hello"
"#;

        let _ = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/workflows")
                    .header("Content-Type", "application/yaml")
                    .header(&auth_key, &auth_val)
                    .body(workflow_yaml.to_string())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Execute it
        let exec_resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/v1/workflows/test_execute_manual/execute")
                    .header("Content-Type", "application/json")
                    .header(&auth_key, &auth_val)
                    .body(r#"{"trigger_context": {"source": "test"}}"#.to_string())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(exec_resp.status(), 200);
        let body = axum::body::to_bytes(exec_resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["workflow_id"], "test_execute_manual");
        assert!(json["instance_id"].as_str().is_some());
    }
}
