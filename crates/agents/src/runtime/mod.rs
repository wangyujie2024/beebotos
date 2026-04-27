//! Agent Runtime Module
//!
//! 高性能 Agent 运行时，提供：
//! - 对象池模式：共享 MediaDownloader 和 HTTP Client
//! - 批量处理：高效处理多个任务
//! - 资源管理：统一的资源生命周期管理
//!
//! # 架构设计
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                      AgentRuntime                           │
//! ├─────────────────────────────────────────────────────────────┤
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
//! │  │    Media     │  │     HTTP     │  │    Task      │      │
//! │  │  Downloader  │  │    Client    │  │   Executor   │      │
//! │  │  (Shared)    │  │  (Shared)    │  │  (Parallel)  │      │
//! │  └──────────────┘  └──────────────┘  └──────────────┘      │
//! ├─────────────────────────────────────────────────────────────┤
//! │              Resource Pool & Connection Pool                │
//! └─────────────────────────────────────────────────────────────┘
//! ```

pub mod agent;
pub mod agent_runtime_impl;
pub mod context;
pub mod executor;
pub mod lifecycle;
pub mod react_framework;
pub mod scheduler;
pub mod session_pool;
pub mod signals;
pub mod state_machine;

// 🟢 P1 FIX: Re-export new types for object pool and batch processing
use std::sync::Arc;

pub use agent::{AgentRuntime, AgentRuntimeBuilder};
pub use executor::{BatchExecutor, BatchResult, TaskExecutor};
pub use react_framework::tools::{CalculatorTool, SearchTool};
pub use react_framework::{
    Action, LLMInterface, ReActAgent, ReActConfig, ReActResult, ReActStep, Tool, ToolResult,
};
use serde::{Deserialize, Serialize};
pub use session_pool::{
    PooledSession, PooledSessionState, SessionCapabilities, SessionMetrics, SessionPool,
    SessionPoolConfig, SessionPoolStats, SessionRequirements, TaskAssignment,
    DEFAULT_IDLE_TIMEOUT_SECS, DEFAULT_MAX_POOL_SIZE, DEFAULT_MIN_POOL_SIZE,
};
use tokio::sync::Semaphore;

/// Runtime configuration for performance tuning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Maximum concurrent tasks (default: 100)
    pub max_concurrent_tasks: usize,
    /// Task queue capacity (default: 1000)
    pub task_queue_capacity: usize,
    /// Batch size for processing (default: 10)
    pub batch_size: usize,
    /// Enable connection pooling (default: true)
    pub enable_connection_pool: bool,
    /// HTTP connection pool max idle per host (default: 10)
    pub http_pool_max_idle: usize,
    /// HTTP connection pool idle timeout in seconds (default: 90)
    pub http_pool_idle_timeout_secs: u64,
    /// HTTP timeout in seconds (default: 30)
    pub http_timeout_secs: u64,
    /// Enable media downloader singleton (default: true)
    pub enable_shared_media_downloader: bool,
    /// Max concurrent downloads (default: 10)
    pub max_concurrent_downloads: usize,
    /// Enable task batching (default: true)
    pub enable_batch_processing: bool,
    /// Batch timeout in milliseconds (default: 100)
    pub batch_timeout_ms: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_concurrent_tasks: 100,
            task_queue_capacity: 1000,
            batch_size: 10,
            enable_connection_pool: true,
            http_pool_max_idle: 10,
            http_pool_idle_timeout_secs: 90,
            http_timeout_secs: 30,
            enable_shared_media_downloader: true,
            max_concurrent_downloads: 10,
            enable_batch_processing: true,
            batch_timeout_ms: 100,
        }
    }
}

impl RuntimeConfig {
    /// Create builder for fluent API
    pub fn builder() -> RuntimeConfigBuilder {
        RuntimeConfigBuilder::default()
    }
}

/// Runtime configuration builder
#[derive(Debug, Default)]
pub struct RuntimeConfigBuilder {
    config: RuntimeConfig,
}

impl RuntimeConfigBuilder {
    pub fn max_concurrent_tasks(mut self, value: usize) -> Self {
        self.config.max_concurrent_tasks = value;
        self
    }

    pub fn task_queue_capacity(mut self, value: usize) -> Self {
        self.config.task_queue_capacity = value;
        self
    }

    pub fn batch_size(mut self, value: usize) -> Self {
        self.config.batch_size = value;
        self
    }

    pub fn enable_connection_pool(mut self, value: bool) -> Self {
        self.config.enable_connection_pool = value;
        self
    }

    pub fn http_pool_max_idle(mut self, value: usize) -> Self {
        self.config.http_pool_max_idle = value;
        self
    }

    pub fn http_pool_idle_timeout_secs(mut self, value: u64) -> Self {
        self.config.http_pool_idle_timeout_secs = value;
        self
    }

    pub fn http_timeout_secs(mut self, value: u64) -> Self {
        self.config.http_timeout_secs = value;
        self
    }

    pub fn enable_shared_media_downloader(mut self, value: bool) -> Self {
        self.config.enable_shared_media_downloader = value;
        self
    }

    pub fn max_concurrent_downloads(mut self, value: usize) -> Self {
        self.config.max_concurrent_downloads = value;
        self
    }

    pub fn enable_batch_processing(mut self, value: bool) -> Self {
        self.config.enable_batch_processing = value;
        self
    }

    pub fn batch_timeout_ms(mut self, value: u64) -> Self {
        self.config.batch_timeout_ms = value;
        self
    }

    pub fn build(self) -> RuntimeConfig {
        self.config
    }
}

/// Resource pool metrics
#[derive(Debug, Clone, Default)]
pub struct RuntimeMetrics {
    /// Total tasks processed
    pub tasks_processed: u64,
    /// Total batches processed
    pub batches_processed: u64,
    /// Average batch size
    pub avg_batch_size: f64,
    /// HTTP connection pool size
    pub http_pool_size: usize,
    /// Active downloads
    pub active_downloads: usize,
    /// Memory usage estimate in MB
    pub memory_usage_mb: usize,
}

/// Shared resource pool for efficient resource management
pub struct SharedResourcePool {
    /// HTTP client with connection pooling
    http_client: reqwest::Client,
    /// Semaphore for limiting concurrent operations
    task_semaphore: Arc<Semaphore>,
    /// Download semaphore
    download_semaphore: Arc<Semaphore>,
}

impl SharedResourcePool {
    /// Create new resource pool with configuration
    pub fn new(config: &RuntimeConfig) -> crate::error::Result<Self> {
        let http_client = if config.enable_connection_pool {
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(config.http_timeout_secs))
                .pool_max_idle_per_host(config.http_pool_max_idle)
                .pool_idle_timeout(std::time::Duration::from_secs(
                    config.http_pool_idle_timeout_secs,
                ))
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()
                .map_err(|e| {
                    crate::error::AgentError::configuration(format!(
                        "Failed to build HTTP client: {}",
                        e
                    ))
                })?
        } else {
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(config.http_timeout_secs))
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()
                .map_err(|e| {
                    crate::error::AgentError::configuration(format!(
                        "Failed to build HTTP client: {}",
                        e
                    ))
                })?
        };

        let task_semaphore = Arc::new(Semaphore::new(config.max_concurrent_tasks));
        let download_semaphore = Arc::new(Semaphore::new(config.max_concurrent_downloads));

        Ok(Self {
            http_client,
            task_semaphore,
            download_semaphore,
        })
    }

    /// Get HTTP client reference
    pub fn http_client(&self) -> &reqwest::Client {
        &self.http_client
    }

    /// Acquire task permit
    pub async fn acquire_task_permit(
        &self,
    ) -> crate::error::Result<tokio::sync::SemaphorePermit<'_>> {
        self.task_semaphore
            .acquire()
            .await
            .map_err(|_| crate::error::AgentError::platform("Failed to acquire task permit"))
    }

    /// Acquire download permit
    pub async fn acquire_download_permit(
        &self,
    ) -> crate::error::Result<tokio::sync::SemaphorePermit<'_>> {
        self.download_semaphore
            .acquire()
            .await
            .map_err(|_| crate::error::AgentError::platform("Failed to acquire download permit"))
    }

    /// Get available task permits
    pub fn available_task_permits(&self) -> usize {
        self.task_semaphore.available_permits()
    }

    /// Get available download permits
    pub fn available_download_permits(&self) -> usize {
        self.download_semaphore.available_permits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_config_default() {
        let config = RuntimeConfig::default();
        assert_eq!(config.max_concurrent_tasks, 100);
        assert_eq!(config.task_queue_capacity, 1000);
        assert_eq!(config.batch_size, 10);
        assert!(config.enable_connection_pool);
        assert!(config.enable_shared_media_downloader);
        assert!(config.enable_batch_processing);
    }

    #[test]
    fn test_runtime_config_builder() {
        let config = RuntimeConfig::builder()
            .max_concurrent_tasks(50)
            .batch_size(20)
            .http_timeout_secs(60)
            .build();

        assert_eq!(config.max_concurrent_tasks, 50);
        assert_eq!(config.batch_size, 20);
        assert_eq!(config.http_timeout_secs, 60);
        // Default values
        assert_eq!(config.task_queue_capacity, 1000);
        assert!(config.enable_connection_pool);
    }

    #[test]
    fn test_runtime_metrics_default() {
        let metrics = RuntimeMetrics::default();
        assert_eq!(metrics.tasks_processed, 0);
        assert_eq!(metrics.batches_processed, 0);
        assert_eq!(metrics.avg_batch_size, 0.0);
    }
}
