//! BeeBotOS Kernel
//!
//! Core kernel module providing:
//! - Preemptive task scheduling with work-stealing
//! - Capability-based security with 11 levels
//! - 29 system calls for agent management
//! - WASM runtime with WASI support
//! - Persistent storage backends
//! - Inter-process communication
//! - Resource management and limits
//! - Audit logging with tamper detection
//!
//! # Architecture
//!
//! The kernel is organized into modules:
//! - `scheduler`: Task scheduling and execution
//! - `security`: Access control and audit logging
//! - `capabilities`: Capability-based permissions
//! - `syscalls`: System call handlers
//! - `storage`: Persistent key-value storage
//! - `wasm`: WebAssembly runtime
//! - `ipc`: Inter-process communication
//! - `memory`: Memory management and allocation
//! - `network`: P2P networking
//! - `resource`: Resource limits and monitoring
//!
//! # Example
//!
//! ```
//! use beebotos_kernel::{KernelBuilder, KernelConfig};
//!
//! let kernel = KernelBuilder::new()
//!     .with_max_agents(1000)
//!     .build()
//!     .expect("Failed to build kernel");
//! ```

#![warn(missing_docs)]
#![warn(clippy::missing_docs_in_private_items)]
#![warn(clippy::unwrap_used)]
#![warn(clippy::expect_used)]

/// Architecture-specific implementations
pub mod arch;
/// Kernel boot sequence and initialization
pub mod boot;
/// Capability-based security system
pub mod capabilities;
/// Hardware device drivers and management
pub mod device;
/// Error types and result definitions
pub mod error;
/// Event types for SystemEventBus
pub mod events;
/// Inter-process communication mechanisms
pub mod ipc;
/// Memory management including allocation, paging, and isolation
pub mod memory;
/// Message Bus integration
pub mod message_bus;
/// P2P networking stack
pub mod network;
/// Resource limits and monitoring
pub mod resource;
/// Task scheduler with work-stealing and priority-based scheduling
pub mod scheduler;
/// Security modules including ACL, audit, and sandbox
pub mod security;
/// Persistent storage backends
pub mod storage;
/// System call handlers and dispatcher
pub mod syscalls;
/// Task and process management
pub mod task;
/// Task monitoring and state change notifications
pub mod task_monitor;
/// WebAssembly runtime integration
pub mod wasm;

/// Initialize kernel memory safety tracking (call during boot)
pub fn init_memory_safety(paranoid_mode: bool) {
    memory::safety::init_global_tracker(paranoid_mode);
}

/// Print memory leak report
pub fn print_memory_leak_report() {
    if let Some(tracker) = memory::safety::global_tracker() {
        tracker.print_leak_report();
    }
}

use beebotos_core::types::AgentId;
pub use error::{KernelError, Result, SecurityError};
// 🟢 P1 FIX: Message Bus integration
pub use message_bus::{
    init_message_bus, message_bus, KernelCapabilityEvent, KernelMessageBus, KernelTaskEvent,
};

/// Kernel version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Kernel builder
pub struct KernelBuilder {
    config: KernelConfig,
}

impl KernelBuilder {
    /// Create new kernel builder
    pub fn new() -> Self {
        Self {
            config: KernelConfig::default(),
        }
    }

    /// Set scheduler config
    pub fn with_scheduler(mut self, config: scheduler::SchedulerConfig) -> Self {
        self.config.scheduler = config;
        self
    }

    /// Set security policy
    pub fn with_security_policy<P: security::SecurityPolicy + 'static>(
        mut self,
        policy: P,
    ) -> Self {
        self.config.security_policy = Box::new(policy);
        self
    }

    /// Set memory configuration
    pub fn with_memory_config(mut self, config: memory::MemoryConfig) -> Self {
        self.config.memory_config = config;
        self
    }

    /// Enable/disable WASM runtime
    pub fn with_wasm(mut self, enabled: bool) -> Self {
        self.config.wasm_enabled = enabled;
        self
    }

    /// Enable TEE with specific provider
    pub fn with_tee(mut self, provider: security::tee::TeeProviderType) -> Self {
        self.config.tee_provider = Some(provider);
        self
    }

    /// Enable TEE with auto-detection
    pub fn with_tee_auto(mut self) -> Self {
        if let Some(provider) = security::tee::TeeProviderFactory::detect_best_available() {
            self.config.tee_provider = Some(provider);
            tracing::info!("Auto-detected TEE: {:?}", provider);
        } else {
            tracing::warn!("No TEE available, using simulation");
            self.config.tee_provider = Some(security::tee::TeeProviderType::Simulation);
        }
        self
    }

    /// Set max agents
    ///
    /// # Panics
    /// Panics if max is 0
    pub fn with_max_agents(mut self, max: usize) -> Self {
        assert!(max > 0, "max_agents must be greater than 0");
        self.config.max_agents = max;
        self
    }

    /// Validate configuration
    fn validate(&self) -> Result<()> {
        // Validate max_agents
        if self.config.max_agents == 0 {
            return Err(KernelError::invalid_argument(
                "max_agents must be greater than 0",
            ));
        }
        if self.config.max_agents > 100_000 {
            return Err(KernelError::invalid_argument(
                "max_agents cannot exceed 100,000",
            ));
        }

        // Validate scheduler config
        if self.config.scheduler.max_concurrent == 0 {
            return Err(KernelError::invalid_argument(
                "max_concurrent must be greater than 0",
            ));
        }

        Ok(())
    }

    /// Build kernel
    pub fn build(self) -> Result<Kernel> {
        // Validate configuration before building
        self.validate()?;

        let scheduler_config = self.config.scheduler.clone();
        let wasm_engine = if self.config.wasm_enabled {
            Some(wasm::WasmEngine::new(wasm::EngineConfig::default())?)
        } else {
            None
        };

        Ok(Kernel {
            scheduler: scheduler::Scheduler::new(scheduler_config),
            security: security::SecurityManager::new(),
            syscall_dispatcher: syscalls::SyscallDispatcher::new(),
            wasm_engine,
            config: self.config,
            running: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Boot and build kernel
    pub fn boot_and_build(self, boot_info: &boot::BootInfo) -> Result<Kernel> {
        // Perform boot sequence
        boot::boot(boot_info).map_err(|e| KernelError::internal(format!("Boot failed: {}", e)))?;

        self.build()
    }
}

impl Default for KernelBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Kernel configuration
pub struct KernelConfig {
    /// Scheduler configuration
    pub scheduler: scheduler::SchedulerConfig,
    /// Security policy
    pub security_policy: Box<dyn security::SecurityPolicy>,
    /// Memory configuration
    pub memory_config: memory::MemoryConfig,
    /// TEE provider type
    pub tee_provider: Option<security::tee::TeeProviderType>,
    /// Max agents
    pub max_agents: usize,
    /// Audit enabled
    pub audit_enabled: bool,
    /// WASM runtime enabled
    pub wasm_enabled: bool,
}

impl std::fmt::Debug for KernelConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KernelConfig")
            .field("scheduler", &self.scheduler)
            .field("security_policy", &"<dyn SecurityPolicy>")
            .field("memory_config", &self.memory_config)
            .field("tee_provider", &self.tee_provider)
            .field("max_agents", &self.max_agents)
            .field("audit_enabled", &self.audit_enabled)
            .field("wasm_enabled", &self.wasm_enabled)
            .finish()
    }
}

impl Default for KernelConfig {
    fn default() -> Self {
        Self {
            scheduler: scheduler::SchedulerConfig::default(),
            security_policy: Box::new(security::DiscretionaryAccessControl::new()),
            memory_config: memory::MemoryConfig::default(),
            tee_provider: None,
            max_agents: 1000,
            audit_enabled: true,
            wasm_enabled: true,
        }
    }
}

/// BeeBotOS Kernel
pub struct Kernel {
    scheduler: scheduler::Scheduler,
    #[allow(dead_code)]
    security: security::SecurityManager,
    syscall_dispatcher: syscalls::SyscallDispatcher,
    #[allow(dead_code)]
    wasm_engine: Option<wasm::WasmEngine>,
    #[allow(dead_code)]
    config: KernelConfig,
    running: std::sync::atomic::AtomicBool,
}

impl Kernel {
    /// Start kernel
    pub async fn start(&self) -> Result<()> {
        self.running
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.scheduler.start().await?;
        Ok(())
    }

    /// Stop kernel gracefully
    ///
    /// Waits for running tasks to complete up to a timeout,
    /// then cancels remaining tasks.
    pub async fn stop(&self) {
        self.running
            .store(false, std::sync::atomic::Ordering::SeqCst);

        // Stop accepting new tasks
        tracing::info!("Stopping scheduler, waiting for tasks to complete...");

        // Wait for tasks with timeout
        let timeout = std::time::Duration::from_secs(30);
        let start = std::time::Instant::now();

        loop {
            let running = self.scheduler.running_count().await;
            if running == 0 {
                tracing::info!("All tasks completed");
                break;
            }

            if start.elapsed() > timeout {
                tracing::warn!("Timeout waiting for {} tasks, forcing cancel", running);
                break;
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        // Stop scheduler
        self.scheduler.stop().await;

        // Flush audit logs
        if let Err(e) = self.security.flush_audit_log() {
            tracing::warn!("Failed to flush audit log: {}", e);
        }
    }

    /// Force immediate shutdown without waiting
    pub async fn force_stop(&self) {
        self.running
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.scheduler.stop().await;
    }

    /// Check if running
    pub fn is_running(&self) -> bool {
        self.running.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Spawn agent task
    pub async fn spawn_task<F>(
        &self,
        name: impl Into<String>,
        priority: scheduler::Priority,
        capabilities: capabilities::CapabilitySet,
        f: F,
    ) -> Result<scheduler::TaskId>
    where
        F: std::future::Future<Output = Result<()>> + Send + 'static,
    {
        self.scheduler
            .spawn(name, priority, capabilities, f)
            .await
            .map_err(std::convert::Into::into)
    }

    /// Cancel a running task by its task ID
    pub async fn cancel_task(&self, task_id: scheduler::TaskId) -> bool {
        self.scheduler.cancel(task_id).await
    }

    /// Register security context
    pub fn register_agent(&mut self, context: security::SecurityContext) -> Result<()> {
        self.security.register(context)
    }

    /// Dispatch syscall
    pub async fn syscall(
        &self,
        number: u64,
        args: syscalls::SyscallArgs,
        caller: AgentId,
    ) -> syscalls::SyscallResult {
        self.syscall_dispatcher.dispatch(number, args, caller).await
    }

    /// Get scheduler stats
    pub async fn scheduler_stats(&self) -> scheduler::SchedulerStats {
        self.scheduler.stats().await
    }

    /// Get task status
    pub async fn get_task_status(&self, task_id: TaskId) -> Option<scheduler::TaskStatus> {
        // Get from scheduler's task info
        self.scheduler.get_task_info(task_id).await.map(|info| {
            use scheduler::TaskState;
            match info.state {
                TaskState::Ready => scheduler::TaskStatus::Pending,
                TaskState::Running => scheduler::TaskStatus::Running,
                TaskState::Blocked(_) => scheduler::TaskStatus::Running,
                TaskState::Zombie => scheduler::TaskStatus::Completed,
            }
        })
    }

    /// Get task info
    pub async fn get_task_info(&self, task_id: TaskId) -> Option<scheduler::TaskInfo> {
        self.scheduler.get_task_info(task_id).await
    }

    /// List active tasks
    pub async fn list_active_tasks(&self) -> Vec<scheduler::TaskInfo> {
        use scheduler::TaskState;
        self.scheduler.list_tasks_by_state(TaskState::Running).await
    }

    /// Wait for task completion with timeout
    pub async fn wait_for_task(
        &self,
        task_id: TaskId,
        timeout: std::time::Duration,
    ) -> std::result::Result<TaskWaitResult, TaskWaitError> {
        let start = std::time::Instant::now();

        loop {
            match self.get_task_status(task_id).await {
                Some(scheduler::TaskStatus::Completed) => {
                    return Ok(TaskWaitResult::Success);
                }
                Some(scheduler::TaskStatus::Failed) => {
                    return Ok(TaskWaitResult::Failure("Task failed".to_string()));
                }
                Some(scheduler::TaskStatus::Cancelled) => {
                    return Ok(TaskWaitResult::Cancelled);
                }
                Some(_) => {
                    if start.elapsed() > timeout {
                        return Err(TaskWaitError::Timeout);
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
                None => {
                    return Err(TaskWaitError::TaskNotFound);
                }
            }
        }
    }

    /// Get WASM engine (if enabled)
    pub fn wasm_engine(&self) -> Option<&wasm::WasmEngine> {
        self.wasm_engine.as_ref()
    }

    /// Compile WASM module
    pub fn compile_wasm(&self, wasm_bytes: &[u8]) -> Result<wasmtime::Module> {
        match &self.wasm_engine {
            Some(engine) => engine.compile(wasm_bytes),
            None => Err(KernelError::not_implemented("WASM runtime not enabled")),
        }
    }

    /// Instantiate WASM module
    pub fn instantiate_wasm(&self, module: &wasmtime::Module) -> Result<wasm::WasmInstance> {
        match &self.wasm_engine {
            Some(engine) => engine.instantiate(module),
            None => Err(KernelError::not_implemented("WASM runtime not enabled")),
        }
    }

    /// Get memory statistics
    pub fn memory_stats(&self) -> memory::MemorySnapshot {
        memory::MemorySnapshot::capture()
    }

    /// Get kernel statistics
    pub async fn stats(&self) -> KernelStats {
        KernelStats {
            scheduler: self.scheduler.stats().await,
            memory: self.memory_stats(),
            running: self.is_running(),
        }
    }
}

/// Task wait result
#[derive(Debug, Clone)]
pub enum TaskWaitResult {
    /// Task completed successfully
    Success,
    /// Task failed with error message
    Failure(String),
    /// Task was cancelled
    Cancelled,
    /// Task timed out
    TimedOut,
}

/// Task wait error
#[derive(Debug, thiserror::Error)]
pub enum TaskWaitError {
    /// Task not found
    #[error("Task not found")]
    TaskNotFound,
    /// Wait timeout
    #[error("Wait timeout")]
    Timeout,
}

/// Kernel statistics
#[derive(Debug, Clone)]
pub struct KernelStats {
    /// Scheduler statistics
    pub scheduler: scheduler::SchedulerStats,
    /// Memory usage statistics
    pub memory: memory::MemorySnapshot,
    /// Whether the kernel is currently running
    pub running: bool,
}

impl KernelStats {
    /// Format as human-readable string
    pub fn format(&self) -> String {
        format!(
            "Kernel Status: {} | {} | {}",
            if self.running { "running" } else { "stopped" },
            self.memory.format(),
            format!(
                "Tasks: {} submitted, {} completed",
                self.scheduler.tasks_submitted, self.scheduler.tasks_completed
            )
        )
    }
}

// Convenience re-exports
pub use capabilities::{CapabilityLevel, CapabilitySet};
pub use memory::{MemoryPressure, MemorySnapshot};
pub use scheduler::{Priority, TaskId};

/// Architecture information and detection
pub mod arch_info {
    //! Architecture-specific information and utilities

    /// Current target architecture
    pub const CURRENT_ARCH: &str = std::env::consts::ARCH;

    /// Target family (unix, windows, wasm)
    pub const TARGET_FAMILY: &str = std::env::consts::FAMILY;

    /// Target OS
    pub const TARGET_OS: &str = std::env::consts::OS;

    /// Check if running on x86_64
    pub fn is_x86_64() -> bool {
        CURRENT_ARCH == "x86_64"
    }

    /// Check if running on aarch64 (ARM64)
    pub fn is_aarch64() -> bool {
        CURRENT_ARCH == "aarch64"
    }

    /// Check if running on riscv64
    pub fn is_riscv64() -> bool {
        CURRENT_ARCH == "riscv64"
    }

    /// Get architecture name for display
    pub fn arch_display_name() -> &'static str {
        match CURRENT_ARCH {
            "x86_64" => "x86-64 (AMD64)",
            "aarch64" => "AArch64 (ARM64)",
            "riscv64" => "RISC-V 64",
            _ => CURRENT_ARCH,
        }
    }

    /// Architecture features for the current target
    #[derive(Debug, Clone)]
    pub struct ArchFeatures {
        /// Architecture name
        pub name: String,
        /// CPU features detected
        pub features: Vec<String>,
        /// Page size in bytes
        pub page_size: usize,
        /// Pointer width in bits
        pub pointer_width: usize,
        /// Whether the architecture supports atomic operations
        pub has_atomics: bool,
    }

    impl ArchFeatures {
        /// Detect features for the current architecture
        pub fn detect() -> Self {
            Self {
                name: arch_display_name().to_string(),
                features: detect_features(),
                page_size: detect_page_size(),
                pointer_width: std::mem::size_of::<usize>() * 8,
                has_atomics: cfg!(target_has_atomic = "ptr"),
            }
        }
    }

    /// Detect CPU features for current architecture
    fn detect_features() -> Vec<String> {
        #[cfg(target_arch = "x86_64")]
        {
            crate::arch::x86_64::cpu_features()
        }

        #[cfg(target_arch = "aarch64")]
        {
            crate::arch::aarch64::cpu_features()
        }

        #[cfg(target_arch = "riscv64")]
        {
            crate::arch::riscv64::cpu_features()
        }

        #[cfg(not(any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "riscv64"
        )))]
        {
            vec![CURRENT_ARCH.to_string()]
        }
    }

    /// Detect page size for current architecture
    fn detect_page_size() -> usize {
        #[cfg(target_arch = "x86_64")]
        {
            crate::arch::x86_64::memory::page_size()
        }

        #[cfg(target_arch = "aarch64")]
        {
            crate::arch::aarch64::memory::page_size()
        }

        #[cfg(target_arch = "riscv64")]
        {
            crate::arch::riscv64::memory::page_size()
        }

        #[cfg(not(any(
            target_arch = "x86_64",
            target_arch = "aarch64",
            target_arch = "riscv64"
        )))]
        {
            4096 // Default page size
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kernel_builder() {
        let kernel = KernelBuilder::new().with_max_agents(100).build().unwrap();

        assert!(!kernel.is_running());
    }

    #[test]
    fn test_kernel_config_default() {
        let config = KernelConfig::default();
        assert_eq!(config.max_agents, 1000);
        assert!(config.audit_enabled);
        assert!(config.wasm_enabled);
    }
}
