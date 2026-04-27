//! BeeBotOS Gateway Library
//!
//! Production-ready API Gateway library with:
//! - Multiple rate limiting algorithms (token bucket, sliding window, fixed
//!   window)
//! - JWT authentication and authorization
//! - WebSocket connection management
//! - Service discovery and load balancing
//! - Circuit breaker pattern
//! - Health checks and observability
//! - Comprehensive error handling
//!
//! # Example
//!
//! ```rust
//! use std::sync::Arc;
//!
//! use beebotos_gateway_lib::config::GatewayConfig;
//! use beebotos_gateway_lib::rate_limit::token_bucket::TokenBucketRateLimiter;
//! use beebotos_gateway_lib::rate_limit::RateLimitManager;
//!
//! async fn setup() {
//!     let config = GatewayConfig::from_env().unwrap();
//!     let rate_limiter = Arc::new(RateLimitManager::new(Arc::new(
//!         TokenBucketRateLimiter::new(100.0, 200),
//!     )));
//! }
//! ```

#![allow(missing_docs)]
#![warn(clippy::all)]

pub mod agent_runtime;
pub mod config;
pub mod discovery;
pub mod error;
// 🟢 P1 FIX: Error adapter for unified error types
pub mod error_adapter;
pub mod health;
pub mod middleware;
pub mod rate_limit;
// 🟢 P1 FIX: Repository pattern for data access
pub mod channel_binding_store;
pub mod repository;
pub mod state_store;
pub mod websocket;

// 🟢 P1 FIX: Re-export unified error types from core
/// Re-export commonly used types
pub use agent_runtime::{
    AgentCapability, AgentConfig, AgentConfigBuilder, AgentEvent, AgentHandle, AgentId,
    AgentRuntime, AgentRuntimeFactory, AgentState, AgentStatus, LlmConfig, MemoryConfig,
    RuntimeConfig, SandboxLevel, StateCommand as AgentStateCommand, TaskConfig, TaskId, TaskResult,
};
pub use beebotos_core::{
    bail as core_bail, err as core_err, BeeBotOSError, ErrorBuilder, ErrorCode, ErrorContext,
    Result as CoreResult, Severity,
};
// 🟢 P1 FIX: Re-export unified configuration management
pub use beebotos_core::{ConfigCenter, Environment};
/// Re-export channel binding store types
pub use channel_binding_store::{ChannelBinding, ChannelBindingStore};
pub use config::{app_config, GatewayConfig, IntoGatewayConfig};
/// Re-export discovery types
pub use discovery::{
    CircuitBreaker, CircuitState, LoadBalancer, RandomBalancer, RoundRobinBalancer,
    ServiceDiscovery, ServiceInstance, ServiceRouter, StaticDiscovery, WeightedRoundRobinBalancer,
};
pub use error::{GatewayError, Result};
/// Re-export error adapter traits
pub use error_adapter::BeeBotOSErrorExt;
/// Re-export health types
pub use health::{ComponentHealth, HealthRegistry, HealthResponse, HealthStatus};
/// Re-export middleware types
pub use middleware::{
    auth_middleware, cors_layer, generate_access_token, generate_refresh_token, logging_middleware,
    rate_limit_middleware, request_id_middleware, trace_layer, AuthUser, Claims, GatewayState,
    RequestContext, TokenType,
};
/// Re-export rate limiting types
pub use rate_limit::{
    ClientTier, FixedWindowRateLimiter, NoopRateLimiter, RateLimitConfig, RateLimitManager,
    RateLimitResult, RateLimiter,
};
// MockRepository is only available in test configuration
#[cfg(test)]
pub use repository::MockRepository;
pub use repository::{
    Entity, FilterCondition, FilterOperator, FilterValue, Pagination, PgRepository, QueryFilter,
    Repository, SortOrder,
};
pub use state_store::{
    AgentFilter, AgentInfo, QueryResult, StateCommand, StateEvent, StateEventType, StateQuery,
    StateStore, StateStoreConfig, StateStoreStats,
};
/// Re-export WebSocket types
pub use websocket::{ConnectionState, WebSocketManager, WsMessage};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Library name
pub const NAME: &str = env!("CARGO_PKG_NAME");

use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::info;

/// Gateway application state
#[derive(Debug)]
pub struct Gateway {
    /// Configuration
    pub config: config::GatewayConfig,
    /// Rate limit manager
    pub rate_limiter: Arc<rate_limit::RateLimitManager>,
    /// Service router
    pub router: Arc<discovery::ServiceRouter>,
    /// Health registry
    pub health: Arc<health::HealthRegistry>,
    /// WebSocket manager
    pub websocket: Option<Arc<websocket::WebSocketManager>>,
    /// Running state
    running: RwLock<bool>,
}

impl Gateway {
    /// Create new Gateway instance
    pub async fn new(config: config::GatewayConfig) -> crate::Result<Self> {
        info!("Initializing BeeBotOS Gateway v{}", VERSION);

        // Initialize rate limiter
        let rate_limiter = Arc::new(rate_limit::RateLimitManager::new(Arc::new(
            rate_limit::token_bucket::TokenBucketRateLimiter::from_config(&config.rate_limit),
        )));

        // Initialize service discovery
        let discovery: Arc<dyn discovery::ServiceDiscovery> =
            match config.discovery.provider.as_str() {
                "static" => Arc::new(discovery::StaticDiscovery::new(&config.discovery)),
                _ => Arc::new(discovery::StaticDiscovery::new(&config.discovery)),
            };

        // Initialize load balancer
        let balancer: Arc<dyn discovery::LoadBalancer> =
            Arc::new(discovery::RoundRobinBalancer::new());

        // Initialize router
        let router = Arc::new(discovery::ServiceRouter::new(discovery, balancer));

        // Initialize health registry
        let health = Arc::new(health::HealthRegistry::new(config.health.clone()));

        // Initialize WebSocket manager if enabled
        let websocket = if config.websocket.max_connections > 0 {
            Some(Arc::new(websocket::WebSocketManager::new(
                config.websocket.clone(),
            )))
        } else {
            None
        };

        info!("Gateway initialization complete");

        Ok(Self {
            config,
            rate_limiter,
            router,
            health,
            websocket,
            running: RwLock::new(false),
        })
    }

    /// Start the gateway
    pub async fn start(&self) -> crate::Result<()> {
        info!("Starting Gateway...");

        // Start health checks
        self.health.start();

        // Mark as running
        *self.running.write().await = true;

        info!("Gateway started successfully");
        Ok(())
    }

    /// Stop the gateway gracefully
    pub async fn shutdown(&self) {
        info!("Shutting down Gateway...");

        // Mark as not running
        *self.running.write().await = false;

        // Shutdown WebSocket connections
        if let Some(ref ws) = self.websocket {
            ws.shutdown().await;
        }

        info!("Gateway shutdown complete");
    }

    /// Check if gateway is running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }
}

/// Builder pattern for Gateway configuration
#[derive(Debug, Default)]
pub struct GatewayBuilder {
    config: Option<config::GatewayConfig>,
}

impl GatewayBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set configuration
    pub fn with_config(mut self, config: config::GatewayConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Load configuration from file
    pub fn with_config_file(mut self, path: impl AsRef<std::path::Path>) -> crate::Result<Self> {
        self.config = Some(config::GatewayConfig::from_file(path)?);
        Ok(self)
    }

    /// Build Gateway
    pub async fn build(self) -> crate::Result<Gateway> {
        let config = self.config.unwrap_or_default();
        Gateway::new(config).await
    }
}

/// Utility functions
pub mod utils {
    use std::net::SocketAddr;

    use super::*;

    /// Parse socket address from string
    pub fn parse_addr(addr: impl AsRef<str>) -> crate::Result<SocketAddr> {
        addr.as_ref().parse().map_err(|e| GatewayError::Config {
            message: format!("Invalid address: {}", e),
        })
    }

    /// Generate request ID
    pub fn generate_request_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    /// Get current timestamp in RFC3339 format
    pub fn now_rfc3339() -> String {
        chrono::Utc::now().to_rfc3339()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_gateway_creation() {
        let config = GatewayConfig::default();
        let gateway = Gateway::new(config).await;
        assert!(gateway.is_ok());
    }

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
        assert!(!NAME.is_empty());
    }

    #[test]
    fn test_generate_request_id() {
        let id1 = utils::generate_request_id();
        let id2 = utils::generate_request_id();

        assert_ne!(id1, id2);
        assert!(!id1.is_empty());
    }

    #[tokio::test]
    async fn test_gateway_lifecycle() {
        let config = GatewayConfig::default();
        let gateway = Gateway::new(config).await.unwrap();

        assert!(!gateway.is_running().await);

        gateway.start().await.unwrap();
        assert!(gateway.is_running().await);

        gateway.shutdown().await;
        assert!(!gateway.is_running().await);
    }
}
