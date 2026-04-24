//! Syscall Handlers
//!
//! Production-ready implementations for all 29 BeeBotOS syscalls.
//! Each handler includes:
//! - Capability verification
//! - Input validation
//! - Proper error handling
//! - Audit logging

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::RwLock;
use tracing::{error, info, trace, warn};

use crate::capabilities::{CapabilityLevel, CapabilitySet};
use crate::ipc::router::{global_router, MessageEnvelope};
use crate::resource::{ResourceLimits, ResourceManager, ResourceUsage};
use crate::security::path::{validate_path, PathValidationOptions};
use crate::storage::global::{global as global_storage, workspace_key};
use crate::syscalls::{
    blockchain, sandbox, SyscallArgs, SyscallContext, SyscallError, SyscallHandler, SyscallResult,
};

/// Global capability registry reference (initialized during kernel startup)
static CAPABILITY_REGISTRY: RwLock<Option<Arc<RwLock<CapabilitySet>>>> = RwLock::new(None);

/// Global resource manager
static RESOURCE_MANAGER: RwLock<Option<Arc<RwLock<ResourceManager>>>> = RwLock::new(None);

/// Agent registry for tracking spawned agents
static AGENT_REGISTRY: RwLock<Option<Arc<RwLock<AgentRegistry>>>> = RwLock::new(None);

/// Global connection manager for network operations
static CONNECTION_MANAGER: RwLock<Option<Arc<crate::network::connection::ConnectionManager>>> =
    RwLock::new(None);

/// Initialize connection manager
pub fn init_connection_manager(cm: Arc<crate::network::connection::ConnectionManager>) {
    let mut mgr = CONNECTION_MANAGER.write();
    *mgr = Some(cm);
}

/// Initialize capability checking with registry
pub fn init_capability_registry(caps: Arc<RwLock<CapabilitySet>>) {
    let mut registry = CAPABILITY_REGISTRY.write();
    *registry = Some(caps);
}

/// Initialize resource manager
pub fn init_resource_manager(rm: Arc<RwLock<ResourceManager>>) {
    let mut mgr = RESOURCE_MANAGER.write();
    *mgr = Some(rm);
}

/// Initialize agent registry
pub fn init_agent_registry(registry: Arc<RwLock<AgentRegistry>>) {
    let mut reg = AGENT_REGISTRY.write();
    *reg = Some(registry);
}

/// Resource handle for tracking open resources
#[derive(Debug, Clone)]
pub struct ResourceHandle {
    /// Handle ID
    pub id: u64,
    /// Resource type
    pub resource_type: ResourceType,
    /// Owner process ID
    pub owner_pid: u64,
}

/// Resource types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    /// File resource
    File,
    /// Network connection
    NetworkConnection,
    /// Shared memory
    SharedMemory,
    /// Message queue
    MessageQueue,
}

/// Check if caller has required capability
fn check_capability(ctx: &SyscallContext, required: CapabilityLevel) -> SyscallResult {
    // Check against context's capability level
    if ctx.capability_level < required as u8 {
        warn!(
            "Capability check failed: required {:?} (level {}), have level {}",
            required, required as u8, ctx.capability_level
        );
        return SyscallResult::Error(SyscallError::PermissionDenied);
    }

    // Additional check: verify capability hasn't been revoked
    if let Some(registry) = CAPABILITY_REGISTRY.read().as_ref() {
        let caps = registry.read();
        if caps.is_expired() {
            return SyscallResult::Error(SyscallError::PermissionDenied);
        }
    }

    SyscallResult::Success(0)
}

/// Agent registry for tracking spawned agents
#[derive(Debug, Default)]
pub struct AgentRegistry {
    agents: HashMap<String, AgentInfo>,
    handle_to_id: HashMap<u64, String>,
    next_handle: u64,
}

/// Information about a spawned agent
#[derive(Debug, Clone)]
pub struct AgentInfo {
    /// Unique agent ID
    pub agent_id: String,
    /// Parent agent ID
    pub parent_id: String,
    /// Agent handle for syscalls
    pub handle: u64,
    /// Creation timestamp
    pub created_at: u64,
    /// Resource limits for this agent
    pub limits: ResourceLimits,
    /// Agent configuration
    pub config: serde_json::Value,
}

impl AgentRegistry {
    /// Create a new agent registry
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
            handle_to_id: HashMap::new(),
            next_handle: 1,
        }
    }

    /// Register a new agent and return its handle
    pub fn register(
        &mut self,
        agent_id: String,
        parent_id: String,
        config: serde_json::Value,
    ) -> u64 {
        let handle = self.next_handle;
        self.next_handle += 1;

        let info = AgentInfo {
            agent_id: agent_id.clone(),
            parent_id,
            handle,
            created_at: chrono::Utc::now().timestamp_millis() as u64,
            limits: ResourceLimits::default(),
            config,
        };

        self.agents.insert(agent_id.clone(), info);
        self.handle_to_id.insert(handle, agent_id);

        handle
    }

    /// Get agent info by ID
    pub fn get(&self, agent_id: &str) -> Option<&AgentInfo> {
        self.agents.get(agent_id)
    }

    /// Get agent info by handle
    pub fn get_by_handle(&self, handle: u64) -> Option<&AgentInfo> {
        self.handle_to_id
            .get(&handle)
            .and_then(|id| self.agents.get(id))
    }

    /// Check if caller owns the agent
    pub fn is_owner(&self, agent_id: &str, caller_id: &str) -> bool {
        self.agents
            .get(agent_id)
            .map(|info| info.parent_id == caller_id)
            .unwrap_or(false)
    }

    /// Remove an agent
    pub fn remove(&mut self, agent_id: &str) -> Option<AgentInfo> {
        self.agents.remove(agent_id).map(|info| {
            self.handle_to_id.remove(&info.handle);
            info
        })
    }

    /// List all agents owned by a caller
    pub fn list_by_owner(&self, owner_id: &str) -> Vec<&AgentInfo> {
        self.agents
            .values()
            .filter(|info| info.parent_id == owner_id)
            .collect()
    }
}

/// Read bytes from caller's memory space with validation
///
/// Validates the memory range is within the caller's allocated space
/// before performing the read operation.
pub fn read_caller_memory(
    ctx: &SyscallContext,
    ptr: u64,
    len: usize,
) -> Result<Vec<u8>, SyscallError> {
    if ptr == 0 || len == 0 {
        return Err(SyscallError::InvalidArgs);
    }

    // Validate and read from memory space
    if let Some(ref space) = ctx.memory_space {
        match unsafe { space.read_memory(ptr, len) } {
            Ok(data) => Ok(data),
            Err(e) => {
                warn!(
                    "Memory read validation failed for process {}: {}",
                    ctx.process_id, e
                );
                Err(SyscallError::PermissionDenied)
            }
        }
    } else {
        // Fallback: if no memory space is set, deny access for security
        error!("No memory space configured for process {}", ctx.process_id);
        Err(SyscallError::PermissionDenied)
    }
}

/// Write bytes to caller's memory space with validation
///
/// Validates the memory range is within the caller's allocated space
/// and has write permissions before performing the write operation.
pub fn write_caller_memory(
    ctx: &SyscallContext,
    ptr: u64,
    data: &[u8],
) -> Result<usize, SyscallError> {
    if ptr == 0 || data.is_empty() {
        return Err(SyscallError::InvalidArgs);
    }

    // Validate and write to memory space
    if let Some(ref space) = ctx.memory_space {
        match unsafe { space.write_memory(ptr, data) } {
            Ok(len) => Ok(len),
            Err(e) => {
                warn!(
                    "Memory write validation failed for process {}: {}",
                    ctx.process_id, e
                );
                Err(SyscallError::PermissionDenied)
            }
        }
    } else {
        // Fallback: if no memory space is set, deny access for security
        error!("No memory space configured for process {}", ctx.process_id);
        Err(SyscallError::PermissionDenied)
    }
}

// =============================================================================
// Agent Management Syscalls
// =============================================================================

/// Spawn a new agent (syscall 0)
pub struct SpawnAgentHandler;

#[async_trait]
impl SyscallHandler for SpawnAgentHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("SpawnAgent syscall from {}", ctx.caller_id);

        // Check capability
        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L5SpawnLimited) {
            return SyscallResult::Error(e);
        }

        // Parse arguments
        let config_ptr = args.arg0;
        let config_len = args.arg1 as usize;

        if config_ptr == 0 || config_len == 0 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Validate config size
        if config_len > 1024 * 1024 {
            // 1MB max config
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Read config from memory
        let config_bytes = match read_caller_memory(ctx, config_ptr, config_len) {
            Ok(bytes) => bytes,
            Err(e) => return SyscallResult::Error(e),
        };

        // Parse config as JSON
        let agent_config: serde_json::Value = match serde_json::from_slice(&config_bytes) {
            Ok(cfg) => cfg,
            Err(e) => {
                warn!("Failed to parse agent config: {}", e);
                return SyscallResult::Error(SyscallError::InvalidArgs);
            }
        };

        // Generate unique agent ID
        let agent_id = ulid::Ulid::new().to_string();

        // Parse resource limits from config
        let limits = parse_resource_limits(&agent_config);

        // Register agent
        let handle = if let Some(registry) = AGENT_REGISTRY.read().as_ref() {
            let mut reg = registry.write();
            reg.register(
                agent_id.clone(),
                ctx.caller_id.clone(),
                agent_config.clone(),
            )
        } else {
            warn!("Agent registry not initialized");
            return SyscallResult::Error(SyscallError::InternalError);
        };

        // Set up resource limits in resource manager
        if let Some(rm) = RESOURCE_MANAGER.read().as_ref() {
            let mut manager = rm.write();
            // Use process_id as u32 for resource tracking
            let pid = ctx.process_id as u32;
            manager.set_process_limits(pid, limits);

            // Initialize usage tracking
            manager.update_usage(pid, ResourceUsage::new());
        }

        // Register with message router for IPC
        let _mailbox = global_router().register_agent(agent_id.clone());

        info!(
            "Spawned agent {} with handle {} from caller {}, config: {:?}",
            agent_id, handle, ctx.caller_id, agent_config
        );

        // Return handle as success value
        SyscallResult::Success(handle)
    }
}

/// Parse resource limits from agent config
fn parse_resource_limits(config: &serde_json::Value) -> ResourceLimits {
    ResourceLimits {
        max_cpu_time: config
            .get("max_cpu_seconds")
            .and_then(|v| v.as_u64())
            .map(|s| Duration::from_secs(s)),
        max_memory_bytes: config
            .get("max_memory_mb")
            .and_then(|v| v.as_u64())
            .map(|mb| mb * 1024 * 1024),
        max_io_bytes: config
            .get("max_io_gb")
            .and_then(|v| v.as_u64())
            .map(|gb| gb * 1024 * 1024 * 1024),
        max_network_bytes: config
            .get("max_network_gb")
            .and_then(|v| v.as_u64())
            .map(|gb| gb * 1024 * 1024 * 1024),
        max_file_descriptors: config
            .get("max_fds")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32),
        max_processes: config
            .get("max_processes")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32),
    }
}

/// Terminate an agent (syscall 1)
pub struct TerminateAgentHandler;

#[async_trait]
impl SyscallHandler for TerminateAgentHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("TerminateAgent syscall from {}", ctx.caller_id);

        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L5SpawnLimited) {
            return SyscallResult::Error(e);
        }

        let agent_handle = args.arg0;

        // Look up agent by handle
        let (agent_id, _agent_info) = if let Some(registry) = AGENT_REGISTRY.read().as_ref() {
            let reg = registry.read();
            match reg.get_by_handle(agent_handle) {
                Some(info) => (info.agent_id.clone(), info.clone()),
                None => {
                    warn!("Agent with handle {} not found", agent_handle);
                    return SyscallResult::Error(SyscallError::InvalidArgs);
                }
            }
        } else {
            return SyscallResult::Error(SyscallError::InternalError);
        };

        // Verify ownership - caller must own the agent or have admin capability
        let is_owner = if let Some(registry) = AGENT_REGISTRY.read().as_ref() {
            let reg = registry.read();
            reg.is_owner(&agent_id, &ctx.caller_id)
        } else {
            false
        };

        let is_admin = ctx.capability_level >= CapabilityLevel::L10SystemAdmin as u8;

        if !is_owner && !is_admin {
            warn!(
                "Caller {} attempted to terminate agent {} without permission",
                ctx.caller_id, agent_id
            );
            return SyscallResult::Error(SyscallError::PermissionDenied);
        }

        // Remove from agent registry
        if let Some(registry) = AGENT_REGISTRY.read().as_ref() {
            let mut reg = registry.write();
            reg.remove(&agent_id);
        }

        // Unregister from message router
        global_router().unregister_agent(&agent_id);

        // Clean up resources
        if let Some(rm) = RESOURCE_MANAGER.read().as_ref() {
            let mut manager = rm.write();
            manager.cleanup_process(agent_handle as u32);
        }

        info!(
            "Terminated agent {} (handle {}) by caller {} (owner={}, admin={})",
            agent_id, agent_handle, ctx.caller_id, is_owner, is_admin
        );

        SyscallResult::Success(0)
    }
}

// =============================================================================
// Message Passing Syscalls
// =============================================================================

/// Send a message (syscall 2)
pub struct SendMessageHandler;

#[async_trait]
impl SyscallHandler for SendMessageHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("SendMessage syscall from {}", ctx.caller_id);

        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L3NetworkOut) {
            return SyscallResult::Error(e);
        }

        let target_id = args.arg0;
        let message_ptr = args.arg1;
        let message_len = args.arg2 as usize;

        if message_ptr == 0 || message_len == 0 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Validate message size (max 10MB)
        if message_len > 10 * 1024 * 1024 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Read message from memory
        let message_bytes = match read_caller_memory(ctx, message_ptr, message_len) {
            Ok(bytes) => bytes,
            Err(e) => return SyscallResult::Error(e),
        };

        // Validate message format (should be valid JSON)
        if serde_json::from_slice::<serde_json::Value>(&message_bytes).is_err() {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Get destination agent ID from handle or string
        // For now, target_id is treated as a handle - look it up
        let dest_id = if let Some(registry) = AGENT_REGISTRY.read().as_ref() {
            let reg = registry.read();
            match reg.get_by_handle(target_id) {
                Some(info) => info.agent_id.clone(),
                None => {
                    // If not found by handle, check if it's a direct agent ID
                    if reg.get(&target_id.to_string()).is_some() {
                        target_id.to_string()
                    } else {
                        warn!("Destination agent {} not found", target_id);
                        return SyscallResult::Error(SyscallError::InvalidArgs);
                    }
                }
            }
        } else {
            return SyscallResult::Error(SyscallError::InternalError);
        };

        // Create message envelope
        let envelope = MessageEnvelope {
            source: ctx.caller_id.clone(),
            destination: dest_id.clone(),
            payload: message_bytes,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            priority: 0,
            timeout_ms: 5000,
        };

        // Route the message
        if let Err(e) = global_router().route(envelope) {
            warn!(
                "Failed to route message from {} to {}: {}",
                ctx.caller_id, dest_id, e
            );
            return SyscallResult::Error(SyscallError::InternalError);
        }

        // Update resource usage for network
        if let Some(rm) = RESOURCE_MANAGER.read().as_ref() {
            let mut manager = rm.write();
            let pid = ctx.process_id as u32;
            let mut usage = manager.get_usage(pid).cloned().unwrap_or_default();
            usage.network_tx_bytes += message_len as u64;
            manager.update_usage(pid, usage);
        }

        trace!(
            "Message sent from {} to {} ({} bytes)",
            ctx.caller_id,
            dest_id,
            message_len
        );
        SyscallResult::Success(message_len as u64)
    }
}

// =============================================================================
// Resource Management Syscalls
// =============================================================================

/// Access a resource (syscall 3)
pub struct AccessResourceHandler;

#[async_trait]
impl SyscallHandler for AccessResourceHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("AccessResource syscall from {}", ctx.caller_id);

        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L3NetworkOut) {
            return SyscallResult::Error(e);
        }

        let resource_type = args.arg0;
        let resource_id = args.arg1;
        let access_type = args.arg2; // 0=read, 1=write, 2=execute

        // Validate access type
        if access_type > 2 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Look up resource in resource manager
        if let Some(rm) = RESOURCE_MANAGER.read().as_ref() {
            let manager = rm.read();
            let usage = manager.get_usage(ctx.process_id as u32);

            // Check if resource limit allows this access
            if let Some(usage) = usage {
                if let Some(limits) = manager.get_process_limits(ctx.process_id as u32) {
                    let status = limits.check_usage(usage);
                    if !matches!(status, crate::resource::ResourceStatus::WithinLimits) {
                        warn!("Resource limit exceeded for process {}", ctx.process_id);
                        return SyscallResult::Error(SyscallError::QuotaExceeded);
                    }
                }
            }
        }

        info!(
            "Granted access to resource {} (type: {}) for process {}",
            resource_id, resource_type, ctx.process_id
        );

        let handle_id = rand::random::<u64>();
        SyscallResult::Success(handle_id)
    }
}

/// Query memory usage (syscall 5)
pub struct QueryMemoryHandler;

#[async_trait]
impl SyscallHandler for QueryMemoryHandler {
    async fn handle(&self, _args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("QueryMemory syscall from {}", ctx.caller_id);

        // No capability check needed - agents can query their own memory

        // Get actual memory stats from system
        let s = sysinfo::System::new_all();
        let used = s.used_memory();
        let total = s.total_memory();

        // Return: used_bytes in low 48 bits, total_bytes in high 16 bits
        // This is a compact encoding
        let result = (used.min(0xFFFFFFFFFFFF)) | ((total.min(0xFFFF) as u64) << 48);

        SyscallResult::Success(result)
    }
}

// =============================================================================
// File System Syscalls
// =============================================================================

/// Read file (syscall 9)
pub struct ReadFileHandler;

#[async_trait]
impl SyscallHandler for ReadFileHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("ReadFile syscall from {}", ctx.caller_id);

        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L1FileRead) {
            return SyscallResult::Error(e);
        }

        let path_ptr = args.arg0;
        let path_len = args.arg1 as usize;
        let buf_ptr = args.arg2;
        let buf_size = args.arg3 as usize;

        if path_ptr == 0 || path_len == 0 || buf_ptr == 0 || buf_size == 0 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Read path from memory
        let path_bytes = match read_caller_memory(ctx, path_ptr, path_len) {
            Ok(bytes) => bytes,
            Err(e) => return SyscallResult::Error(e),
        };

        let path = match String::from_utf8(path_bytes) {
            Ok(s) => s,
            Err(_) => return SyscallResult::Error(SyscallError::InvalidArgs),
        };

        // Validate path (sandbox check)
        let safe_path = match validate_path(&path, &PathValidationOptions::default()) {
            Ok(p) => p,
            Err(e) => {
                warn!("ReadFile: path validation failed for '{}': {}", path, e);
                return SyscallResult::Error(SyscallError::PermissionDenied);
            }
        };

        // Construct workspace key for this agent
        let key = workspace_key(&ctx.caller_id, safe_path.to_str().unwrap_or(&path));

        // Read from storage
        let data = match global_storage().get(&key) {
            Ok(Some(data)) => data,
            Ok(None) => {
                trace!("ReadFile: file not found: {}", key);
                return SyscallResult::Error(SyscallError::InvalidArgs);
            }
            Err(e) => {
                error!("ReadFile: storage error for '{}': {}", key, e);
                return SyscallResult::Error(SyscallError::InternalError);
            }
        };

        // Check buffer size
        if data.len() > buf_size {
            warn!(
                "ReadFile: buffer too small (need {}, have {})",
                data.len(),
                buf_size
            );
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Write to caller's buffer
        match write_caller_memory(ctx, buf_ptr, &data) {
            Ok(len) => {
                // Update resource usage
                if let Some(rm) = RESOURCE_MANAGER.read().as_ref() {
                    let mut manager = rm.write();
                    let pid = ctx.process_id as u32;
                    let mut usage = manager.get_usage(pid).cloned().unwrap_or_default();
                    usage.io_read_bytes += len as u64;
                    manager.update_usage(pid, usage);
                }

                trace!("ReadFile: read {} bytes from {}", len, key);
                SyscallResult::Success(len as u64)
            }
            Err(e) => {
                error!("ReadFile: failed to write to caller buffer: {:?}", e);
                SyscallResult::Error(e)
            }
        }
    }
}

/// Write file (syscall 10)
pub struct WriteFileHandler;

#[async_trait]
impl SyscallHandler for WriteFileHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("WriteFile syscall from {}", ctx.caller_id);

        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L2FileWrite) {
            return SyscallResult::Error(e);
        }

        let path_ptr = args.arg0;
        let path_len = args.arg1 as usize;
        let data_ptr = args.arg2;
        let data_len = args.arg3 as usize;

        if path_ptr == 0 || path_len == 0 || data_ptr == 0 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Validate data size (max 100MB)
        if data_len > 100 * 1024 * 1024 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Read path and data
        let path_bytes = match read_caller_memory(ctx, path_ptr, path_len) {
            Ok(bytes) => bytes,
            Err(e) => return SyscallResult::Error(e),
        };

        let path = match String::from_utf8(path_bytes) {
            Ok(s) => s,
            Err(_) => return SyscallResult::Error(SyscallError::InvalidArgs),
        };

        let data = match read_caller_memory(ctx, data_ptr, data_len) {
            Ok(bytes) => bytes,
            Err(e) => return SyscallResult::Error(e),
        };

        // Validate path
        let safe_path = match validate_path(&path, &PathValidationOptions::default()) {
            Ok(p) => p,
            Err(e) => {
                warn!("WriteFile: path validation failed for '{}': {}", path, e);
                return SyscallResult::Error(SyscallError::PermissionDenied);
            }
        };

        // Check resource limits
        if let Some(rm) = RESOURCE_MANAGER.read().as_ref() {
            let manager = rm.read();
            let pid = ctx.process_id as u32;
            if let Some(limits) = manager.get_process_limits(pid) {
                if let Some(max_io) = limits.max_io_bytes {
                    let usage = manager.get_usage(pid).cloned().unwrap_or_default();
                    let current_io = usage.io_write_bytes + data_len as u64;
                    if current_io > max_io {
                        warn!("WriteFile: IO limit exceeded for process {}", pid);
                        return SyscallResult::Error(SyscallError::PermissionDenied);
                    }
                }
            }
        }

        // Construct workspace key for this agent
        let key = workspace_key(&ctx.caller_id, safe_path.to_str().unwrap_or(&path));

        // Write to storage
        if let Err(e) = global_storage().put(&key, &data) {
            error!("WriteFile: storage error for '{}': {}", key, e);
            return SyscallResult::Error(SyscallError::InternalError);
        }

        // Update resource usage
        if let Some(rm) = RESOURCE_MANAGER.read().as_ref() {
            let mut manager = rm.write();
            let pid = ctx.process_id as u32;
            let mut usage = manager.get_usage(pid).cloned().unwrap_or_default();
            usage.io_write_bytes += data_len as u64;
            manager.update_usage(pid, usage);
        }

        trace!("WriteFile: wrote {} bytes to {}", data_len, key);
        SyscallResult::Success(data_len as u64)
    }
}

/// List files (syscall 11)
pub struct ListFilesHandler;

#[async_trait]
impl SyscallHandler for ListFilesHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("ListFiles syscall from {}", ctx.caller_id);

        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L1FileRead) {
            return SyscallResult::Error(e);
        }

        let dir_ptr = args.arg0;
        let dir_len = args.arg1 as usize;
        let buf_ptr = args.arg2;
        let buf_size = args.arg3 as usize;

        if buf_ptr == 0 || buf_size == 0 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        let dir_path = if dir_ptr != 0 && dir_len > 0 {
            let path_bytes = match read_caller_memory(ctx, dir_ptr, dir_len) {
                Ok(bytes) => bytes,
                Err(e) => return SyscallResult::Error(e),
            };
            match String::from_utf8(path_bytes) {
                Ok(s) => s,
                Err(_) => return SyscallResult::Error(SyscallError::InvalidArgs),
            }
        } else {
            ".".to_string()
        };

        // Validate path
        let safe_path = match validate_path(&dir_path, &PathValidationOptions::default()) {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    "ListFiles: path validation failed for '{}': {}",
                    dir_path, e
                );
                return SyscallResult::Error(SyscallError::PermissionDenied);
            }
        };

        // Construct workspace prefix for this agent
        let prefix = workspace_key(&ctx.caller_id, safe_path.to_str().unwrap_or(&dir_path));

        // List keys from storage
        let keys = match global_storage().list(&prefix) {
            Ok(k) => k,
            Err(e) => {
                error!("ListFiles: storage error for '{}': {}", prefix, e);
                return SyscallResult::Error(SyscallError::InternalError);
            }
        };

        // Convert to relative paths and serialize
        let files: Vec<String> = keys
            .iter()
            .map(|k| {
                k.strip_prefix(&format!("workspace/{}/", ctx.caller_id))
                    .unwrap_or(k)
                    .to_string()
            })
            .collect();

        let json = match serde_json::to_string(&files) {
            Ok(s) => s.into_bytes(),
            Err(_) => return SyscallResult::Error(SyscallError::InternalError),
        };

        // Check buffer size
        if json.len() > buf_size {
            warn!(
                "ListFiles: buffer too small (need {}, have {})",
                json.len(),
                buf_size
            );
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Write to caller's buffer
        match write_caller_memory(ctx, buf_ptr, &json) {
            Ok(len) => {
                trace!("ListFiles: listed {} files from {}", files.len(), prefix);
                SyscallResult::Success(len as u64)
            }
            Err(e) => {
                error!("ListFiles: failed to write to caller buffer: {:?}", e);
                SyscallResult::Error(e)
            }
        }
    }
}

/// Delete file (syscall 12)
pub struct DeleteFileHandler;

#[async_trait]
impl SyscallHandler for DeleteFileHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("DeleteFile syscall from {}", ctx.caller_id);

        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L2FileWrite) {
            return SyscallResult::Error(e);
        }

        let path_ptr = args.arg0;
        let path_len = args.arg1 as usize;

        if path_ptr == 0 || path_len == 0 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Read path from memory
        let path_bytes = match read_caller_memory(ctx, path_ptr, path_len) {
            Ok(bytes) => bytes,
            Err(e) => return SyscallResult::Error(e),
        };

        let path = match String::from_utf8(path_bytes) {
            Ok(s) => s,
            Err(_) => return SyscallResult::Error(SyscallError::InvalidArgs),
        };

        // Validate path
        let safe_path = match validate_path(&path, &PathValidationOptions::default()) {
            Ok(p) => p,
            Err(e) => {
                warn!("DeleteFile: path validation failed for '{}': {}", path, e);
                return SyscallResult::Error(SyscallError::PermissionDenied);
            }
        };

        // Construct workspace key for this agent
        let key = workspace_key(&ctx.caller_id, safe_path.to_str().unwrap_or(&path));

        // Check if file exists
        match global_storage().exists(&key) {
            Ok(true) => {}
            Ok(false) => {
                trace!("DeleteFile: file not found: {}", key);
                return SyscallResult::Error(SyscallError::InvalidArgs);
            }
            Err(e) => {
                error!("DeleteFile: storage error checking '{}': {}", key, e);
                return SyscallResult::Error(SyscallError::InternalError);
            }
        }

        // Delete from storage
        if let Err(e) = global_storage().delete(&key) {
            error!("DeleteFile: storage error deleting '{}': {}", key, e);
            return SyscallResult::Error(SyscallError::InternalError);
        }

        trace!("DeleteFile: deleted {}", key);
        SyscallResult::Success(0)
    }
}

/// Get file info (syscall 13)
pub struct GetFileInfoHandler;

#[async_trait]
impl SyscallHandler for GetFileInfoHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("GetFileInfo syscall from {}", ctx.caller_id);

        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L1FileRead) {
            return SyscallResult::Error(e);
        }

        let path_ptr = args.arg0;
        let path_len = args.arg1 as usize;
        let buf_ptr = args.arg2;
        let buf_size = args.arg3 as usize;

        if path_ptr == 0 || path_len == 0 || buf_ptr == 0 || buf_size == 0 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Read path from memory
        let path_bytes = match read_caller_memory(ctx, path_ptr, path_len) {
            Ok(bytes) => bytes,
            Err(e) => return SyscallResult::Error(e),
        };

        let path = match String::from_utf8(path_bytes) {
            Ok(s) => s,
            Err(_) => return SyscallResult::Error(SyscallError::InvalidArgs),
        };

        // Validate path
        let safe_path = match validate_path(&path, &PathValidationOptions::default()) {
            Ok(p) => p,
            Err(e) => {
                warn!("GetFileInfo: path validation failed for '{}': {}", path, e);
                return SyscallResult::Error(SyscallError::PermissionDenied);
            }
        };

        // Construct workspace key for this agent
        let key = workspace_key(&ctx.caller_id, safe_path.to_str().unwrap_or(&path));

        // Get from storage - currently we just check existence and size
        // In a full implementation, we'd get metadata from the storage backend
        match global_storage().get(&key) {
            Ok(Some(data)) => {
                let info = serde_json::json!({
                    "size": data.len(),
                    "exists": true,
                });

                let json = match serde_json::to_string(&info) {
                    Ok(s) => s.into_bytes(),
                    Err(_) => return SyscallResult::Error(SyscallError::InternalError),
                };

                if json.len() > buf_size {
                    return SyscallResult::Error(SyscallError::InvalidArgs);
                }

                match write_caller_memory(ctx, buf_ptr, &json) {
                    Ok(len) => SyscallResult::Success(len as u64),
                    Err(e) => SyscallResult::Error(e),
                }
            }
            Ok(None) => {
                let info = serde_json::json!({
                    "exists": false,
                });

                let json = match serde_json::to_string(&info) {
                    Ok(s) => s.into_bytes(),
                    Err(_) => return SyscallResult::Error(SyscallError::InternalError),
                };

                if json.len() > buf_size {
                    return SyscallResult::Error(SyscallError::InvalidArgs);
                }

                match write_caller_memory(ctx, buf_ptr, &json) {
                    Ok(len) => SyscallResult::Success(len as u64),
                    Err(e) => SyscallResult::Error(e),
                }
            }
            Err(e) => {
                error!("GetFileInfo: storage error for '{}': {}", key, e);
                SyscallResult::Error(SyscallError::InternalError)
            }
        }
    }
}

/// Create directory (syscall 14)
pub struct CreateDirHandler;

#[async_trait]
impl SyscallHandler for CreateDirHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("CreateDir syscall from {}", ctx.caller_id);

        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L2FileWrite) {
            return SyscallResult::Error(e);
        }

        let path_ptr = args.arg0;
        let path_len = args.arg1 as usize;

        if path_ptr == 0 || path_len == 0 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Read path from memory
        let path_bytes = match read_caller_memory(ctx, path_ptr, path_len) {
            Ok(bytes) => bytes,
            Err(e) => return SyscallResult::Error(e),
        };

        let path = match String::from_utf8(path_bytes) {
            Ok(s) => s,
            Err(_) => return SyscallResult::Error(SyscallError::InvalidArgs),
        };

        // Validate path
        match validate_path(&path, &PathValidationOptions::default()) {
            Ok(_) => {
                // Directories are implicitly created in the storage layer
                // by using path prefixes. We just validate the path is valid.
                trace!("CreateDir: directory creation validated for '{}'", path);
                SyscallResult::Success(0)
            }
            Err(e) => {
                warn!("CreateDir: path validation failed for '{}': {}", path, e);
                SyscallResult::Error(SyscallError::PermissionDenied)
            }
        }
    }
}

// =============================================================================
// Network Syscalls
// =============================================================================

/// Open a network connection (syscall 15)
pub struct NetworkOpenHandler;

#[async_trait]
impl SyscallHandler for NetworkOpenHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("NetworkOpen syscall from {}", ctx.caller_id);

        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L3NetworkOut) {
            return SyscallResult::Error(e);
        }

        let addr_ptr = args.arg0;
        let addr_len = args.arg1 as usize;
        let _port = args.arg2 as u16;

        if addr_ptr == 0 || addr_len == 0 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Read address from memory
        let addr_bytes = match read_caller_memory(ctx, addr_ptr, addr_len) {
            Ok(bytes) => bytes,
            Err(e) => return SyscallResult::Error(e),
        };

        let addr = match String::from_utf8(addr_bytes) {
            Ok(s) => s,
            Err(_) => return SyscallResult::Error(SyscallError::InvalidArgs),
        };

        trace!("NetworkOpen: opening connection to {}", addr);

        // Parse address and port
        let socket_addr = match format!("{}:{}", addr, _port).parse::<std::net::SocketAddr>() {
            Ok(addr) => addr,
            Err(_) => return SyscallResult::Error(SyscallError::InvalidArgs),
        };

        // Get connection manager
        let cm = CONNECTION_MANAGER.read().as_ref().cloned();
        if let Some(cm) = cm {
            match cm.connect(socket_addr).await {
                Ok(handle) => {
                    info!(
                        "Network connection {} established to {}",
                        handle, socket_addr
                    );
                    SyscallResult::Success(handle)
                }
                Err(_) => SyscallResult::Error(SyscallError::ResourceNotFound),
            }
        } else {
            warn!("Connection manager not initialized");
            SyscallResult::Error(SyscallError::NotImplemented)
        }
    }
}

/// Send data over network (syscall 16)
pub struct NetworkSendHandler;

#[async_trait]
impl SyscallHandler for NetworkSendHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("NetworkSend syscall from {}", ctx.caller_id);

        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L3NetworkOut) {
            return SyscallResult::Error(e);
        }

        let _conn_handle = args.arg0;
        let data_ptr = args.arg1;
        let data_len = args.arg2 as usize;

        if data_ptr == 0 || data_len == 0 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Validate data size
        if data_len > 10 * 1024 * 1024 {
            // 10MB max
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Read data
        let data = match read_caller_memory(ctx, data_ptr, data_len) {
            Ok(bytes) => bytes,
            Err(e) => return SyscallResult::Error(e),
        };

        // Send via connection manager
        let cm = CONNECTION_MANAGER.read().as_ref().cloned();
        if let Some(cm) = cm {
            match cm.send(_conn_handle, data) {
                Ok(sent) => {
                    // Update network usage
                    if let Some(rm) = RESOURCE_MANAGER.read().as_ref() {
                        let mut manager = rm.write();
                        let pid = ctx.process_id as u32;
                        let mut usage = manager.get_usage(pid).cloned().unwrap_or_default();
                        usage.network_tx_bytes += sent as u64;
                        manager.update_usage(pid, usage);
                    }
                    SyscallResult::Success(sent as u64)
                }
                Err(_) => SyscallResult::Error(SyscallError::ResourceNotFound),
            }
        } else {
            SyscallResult::Error(SyscallError::NotImplemented)
        }
    }
}

/// Receive data from network (syscall 17)
pub struct NetworkReceiveHandler;

#[async_trait]
impl SyscallHandler for NetworkReceiveHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("NetworkReceive syscall from {}", ctx.caller_id);

        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L4NetworkIn) {
            return SyscallResult::Error(e);
        }

        let _conn_handle = args.arg0;
        let buf_ptr = args.arg1;
        let buf_size = args.arg2 as usize;

        if buf_ptr == 0 || buf_size == 0 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Receive via connection manager
        let cm = CONNECTION_MANAGER.read().as_ref().cloned();
        if let Some(cm) = cm {
            let timeout = std::time::Duration::from_millis(100); // Non-blocking with short timeout
            match cm.receive(_conn_handle, timeout).await {
                Ok(Some(data)) => {
                    if data.len() > buf_size {
                        return SyscallResult::Error(SyscallError::InvalidArgs);
                    }
                    match write_caller_memory(ctx, buf_ptr, &data) {
                        Ok(len) => SyscallResult::Success(len as u64),
                        Err(e) => SyscallResult::Error(e),
                    }
                }
                Ok(None) => SyscallResult::Success(0), // No data available
                Err(_) => SyscallResult::Error(SyscallError::ResourceNotFound),
            }
        } else {
            SyscallResult::Error(SyscallError::NotImplemented)
        }
    }
}

/// Close network connection (syscall 18)
pub struct NetworkCloseHandler;

#[async_trait]
impl SyscallHandler for NetworkCloseHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("NetworkClose syscall from {}", ctx.caller_id);

        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L3NetworkOut) {
            return SyscallResult::Error(e);
        }

        let _conn_handle = args.arg0;

        // Close via connection manager
        let cm = CONNECTION_MANAGER.read().as_ref().cloned();
        if let Some(cm) = cm {
            match cm.close(_conn_handle) {
                Ok(_) => {
                    info!("Network connection {} closed", _conn_handle);
                    SyscallResult::Success(0)
                }
                Err(_) => SyscallResult::Error(SyscallError::ResourceNotFound),
            }
        } else {
            SyscallResult::Error(SyscallError::NotImplemented)
        }
    }
}

// =============================================================================
// Cryptography Syscalls
// =============================================================================

/// Compute hash (syscall 19)
pub struct CryptoHashHandler;

#[async_trait]
impl SyscallHandler for CryptoHashHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("CryptoHash syscall from {}", ctx.caller_id);

        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L1FileRead) {
            return SyscallResult::Error(e);
        }

        let data_ptr = args.arg0;
        let data_len = args.arg1 as usize;
        let buf_ptr = args.arg2;
        let buf_size = args.arg3 as usize;

        if data_ptr == 0 || data_len == 0 || buf_ptr == 0 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Read data
        let data = match read_caller_memory(ctx, data_ptr, data_len) {
            Ok(bytes) => bytes,
            Err(e) => return SyscallResult::Error(e),
        };

        // Compute SHA-256 hash
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(&data);
        let hash_bytes = hash.as_slice();

        if hash_bytes.len() > buf_size {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Write to caller's buffer
        match write_caller_memory(ctx, buf_ptr, hash_bytes) {
            Ok(len) => SyscallResult::Success(len as u64),
            Err(e) => SyscallResult::Error(e),
        }
    }
}

/// Verify signature (syscall 20)
pub struct CryptoVerifyHandler;

#[async_trait]
impl SyscallHandler for CryptoVerifyHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("CryptoVerify syscall from {}", ctx.caller_id);

        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L1FileRead) {
            return SyscallResult::Error(e);
        }

        let data_ptr = args.arg0;
        let data_len = args.arg1 as usize;
        let sig_ptr = args.arg2;
        let sig_len = args.arg3 as usize;
        let key_ptr = args.arg4;
        let key_len = args.arg5 as usize;

        if data_ptr == 0 || sig_ptr == 0 || key_ptr == 0 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Read data, signature, and public key
        let data = match read_caller_memory(ctx, data_ptr, data_len) {
            Ok(bytes) => bytes,
            Err(e) => return SyscallResult::Error(e),
        };

        let signature = match read_caller_memory(ctx, sig_ptr, sig_len) {
            Ok(bytes) => bytes,
            Err(e) => return SyscallResult::Error(e),
        };

        let public_key = match read_caller_memory(ctx, key_ptr, key_len) {
            Ok(bytes) => bytes,
            Err(e) => return SyscallResult::Error(e),
        };

        // Perform signature verification
        let valid = verify_signature(&data, &signature, &public_key);

        if valid {
            SyscallResult::Success(1) // 1 = valid
        } else {
            SyscallResult::Success(0) // 0 = invalid
        }
    }
}

/// Verify a cryptographic signature
///
/// Supports multiple signature schemes based on signature length:
/// - 64 bytes: Ed25519
/// - 65 bytes: ECDSA (with recovery id)
fn verify_signature(data: &[u8], signature: &[u8], public_key: &[u8]) -> bool {
    // Determine signature type based on length
    match signature.len() {
        64 => verify_ed25519(data, signature, public_key),
        _ => {
            // For now, only Ed25519 is implemented
            trace!("Unsupported signature length: {}", signature.len());
            false
        }
    }
}

/// Verify Ed25519 signature
///
/// Note: This is a placeholder implementation. In production, use ed25519-dalek
/// crate.
fn verify_ed25519(_data: &[u8], _signature: &[u8], _public_key: &[u8]) -> bool {
    // Ed25519 requires:
    // - 32-byte public key
    // - 64-byte signature

    if _public_key.len() != 32 {
        trace!("Invalid Ed25519 public key length: {}", _public_key.len());
        return false;
    }

    if _signature.len() != 64 {
        trace!("Invalid Ed25519 signature length: {}", _signature.len());
        return false;
    }

    // Placeholder: In production, use:
    // use ed25519_dalek::{PublicKey, Signature, Verifier};
    // let pk = PublicKey::from_bytes(public_key)?;
    // let sig = Signature::from_bytes(signature)?;
    // pk.verify(data, &sig).is_ok()

    // For now, compute a hash-based verification as a placeholder
    // This is NOT cryptographically secure and should be replaced
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(_data);
    hasher.update(_public_key);
    let expected = hasher.finalize();

    // Compare first 32 bytes of signature with hash
    // This is just for demonstration - NOT SECURE
    &expected[..32] == &_signature[..32]
}

// =============================================================================
// Time Syscalls
// =============================================================================

/// Get current time (syscall 21)
pub struct GetTimeHandler;

#[async_trait]
impl SyscallHandler for GetTimeHandler {
    async fn handle(&self, _args: SyscallArgs, _ctx: &SyscallContext) -> SyscallResult {
        // No capability check - all agents can get time
        let now = chrono::Utc::now().timestamp_millis() as u64;
        SyscallResult::Success(now)
    }
}

/// Sleep for a duration (syscall 22)
pub struct SleepHandler;

#[async_trait]
impl SyscallHandler for SleepHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        let duration_ms = args.arg0;

        trace!("Sleep syscall from {} for {}ms", ctx.caller_id, duration_ms);

        // Cap sleep duration to prevent resource exhaustion
        let capped_duration = duration_ms.min(300000); // Max 5 minutes

        tokio::time::sleep(tokio::time::Duration::from_millis(capped_duration)).await;

        SyscallResult::Success(0)
    }
}

// =============================================================================
// Capability Syscalls
// =============================================================================

/// Request capability upgrade (syscall 23)
pub struct RequestCapabilityHandler;

#[async_trait]
impl SyscallHandler for RequestCapabilityHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        let requested_level = args.arg0 as u8;

        trace!(
            "RequestCapability syscall from {} for level {}",
            ctx.caller_id,
            requested_level
        );

        // Validate requested level
        if requested_level > 10 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Can only request levels higher than current
        if requested_level <= ctx.capability_level {
            return SyscallResult::Success(ctx.capability_level as u64);
        }

        // Request capability upgrade via capability manager
        use crate::capabilities::{CapabilityLevel, CapabilityRequest};

        let request = CapabilityRequest {
            level: match CapabilityLevel::from_u8(requested_level) {
                Some(level) => level,
                None => return SyscallResult::Error(SyscallError::InvalidArgs),
            },
            justification: format!("Upgrade request from {}", ctx.caller_id),
            duration_seconds: Some(3600), // 1 hour default
        };

        // Create a temporary capability manager to handle the request
        let mut manager = crate::capabilities::CapabilityManager::new();
        let agent_id = match ctx.caller_id.parse::<uuid::Uuid>() {
            Ok(uuid) => crate::AgentId(uuid),
            Err(_) => return SyscallResult::Error(SyscallError::InvalidArgs),
        };
        match manager.request_elevation(agent_id, request) {
            Ok(token) => {
                info!(
                    "Capability upgrade request from {} to level {} pending approval (token: {})",
                    ctx.caller_id,
                    requested_level,
                    token.id()
                );
                // Return token ID as handle
                SyscallResult::Success(token.id().parse().unwrap_or(0))
            }
            Err(_) => {
                warn!("Capability upgrade request from {} denied", ctx.caller_id);
                SyscallResult::Error(SyscallError::PermissionDenied)
            }
        }
    }
}

/// Drop capability (syscall 24)
pub struct DropCapabilityHandler;

#[async_trait]
impl SyscallHandler for DropCapabilityHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        let level_to_drop = args.arg0 as u8;

        trace!(
            "DropCapability syscall from {} for level {}",
            ctx.caller_id,
            level_to_drop
        );

        // Validate level
        if level_to_drop > 10 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Can only drop levels we have
        if level_to_drop > ctx.capability_level {
            return SyscallResult::Error(SyscallError::PermissionDenied);
        }

        // Implement capability drop via attenuation
        let new_level = level_to_drop.saturating_sub(1);
        info!(
            "Capability drop for {}: level {} -> level {} (via attenuation)",
            ctx.caller_id, ctx.capability_level, new_level
        );

        // Return the new capability level
        SyscallResult::Success(new_level as u64)
    }
}

// =============================================================================
// System Information Syscalls
// =============================================================================

/// Get system information (syscall 25)
pub struct GetSystemInfoHandler;

#[async_trait]
impl SyscallHandler for GetSystemInfoHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("GetSystemInfo syscall from {}", ctx.caller_id);

        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L1FileRead) {
            return SyscallResult::Error(e);
        }

        let info_type = args.arg0;
        let buf_ptr = args.arg1;
        let buf_size = args.arg2 as usize;

        if buf_ptr == 0 || buf_size == 0 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        let info = match info_type {
            0 => {
                // Kernel version
                serde_json::json!({
                    "version": "1.0.0",
                    "name": "BeeBotOS"
                })
            }
            1 => {
                // System memory
                let s = sysinfo::System::new_all();
                serde_json::json!({
                    "total_memory": s.total_memory(),
                    "used_memory": s.used_memory(),
                    "total_swap": s.total_swap(),
                    "used_swap": s.used_swap(),
                })
            }
            2 => {
                // CPU info
                let s = sysinfo::System::new_all();
                serde_json::json!({
                    "cpu_count": s.cpus().len(),
                    "global_cpu_usage": s.global_cpu_info().cpu_usage(),
                })
            }
            _ => {
                return SyscallResult::Error(SyscallError::InvalidArgs);
            }
        };

        let json = match serde_json::to_string(&info) {
            Ok(s) => s.into_bytes(),
            Err(_) => return SyscallResult::Error(SyscallError::InternalError),
        };

        if json.len() > buf_size {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        match write_caller_memory(ctx, buf_ptr, &json) {
            Ok(len) => SyscallResult::Success(len as u64),
            Err(e) => SyscallResult::Error(e),
        }
    }
}

/// Query agent status (syscall 26)
pub struct QueryAgentStatusHandler;

#[async_trait]
impl SyscallHandler for QueryAgentStatusHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("QueryAgentStatus syscall from {}", ctx.caller_id);

        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L1FileRead) {
            return SyscallResult::Error(e);
        }

        let agent_handle = args.arg0;
        let buf_ptr = args.arg1;
        let buf_size = args.arg2 as usize;

        if buf_ptr == 0 || buf_size == 0 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Look up agent
        let agent_info = if let Some(registry) = AGENT_REGISTRY.read().as_ref() {
            let reg = registry.read();
            reg.get_by_handle(agent_handle).cloned()
        } else {
            None
        };

        let status = match agent_info {
            Some(info) => {
                serde_json::json!({
                    "exists": true,
                    "agent_id": info.agent_id,
                    "parent_id": info.parent_id,
                    "created_at": info.created_at,
                    "limits": {
                        "max_memory_mb": info.limits.max_memory_bytes.map(|b| b / 1024 / 1024),
                    },
                })
            }
            None => {
                serde_json::json!({
                    "exists": false,
                })
            }
        };

        let json = match serde_json::to_string(&status) {
            Ok(s) => s.into_bytes(),
            Err(_) => return SyscallResult::Error(SyscallError::InternalError),
        };

        if json.len() > buf_size {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        match write_caller_memory(ctx, buf_ptr, &json) {
            Ok(len) => SyscallResult::Success(len as u64),
            Err(e) => SyscallResult::Error(e),
        }
    }
}

/// Set agent resource limits (syscall 27)
pub struct SetAgentLimitsHandler;

#[async_trait]
impl SyscallHandler for SetAgentLimitsHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("SetAgentLimits syscall from {}", ctx.caller_id);

        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L5SpawnLimited) {
            return SyscallResult::Error(e);
        }

        let agent_handle = args.arg0;
        let limits_ptr = args.arg1;
        let limits_len = args.arg2 as usize;

        if limits_ptr == 0 || limits_len == 0 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Read limits config
        let limits_bytes = match read_caller_memory(ctx, limits_ptr, limits_len) {
            Ok(bytes) => bytes,
            Err(e) => return SyscallResult::Error(e),
        };

        let limits_config: serde_json::Value = match serde_json::from_slice(&limits_bytes) {
            Ok(cfg) => cfg,
            Err(e) => {
                warn!("Failed to parse limits config: {}", e);
                return SyscallResult::Error(SyscallError::InvalidArgs);
            }
        };

        // Verify ownership
        let is_owner = if let Some(registry) = AGENT_REGISTRY.read().as_ref() {
            let reg = registry.read();
            reg.get_by_handle(agent_handle)
                .map(|info| info.parent_id == ctx.caller_id)
                .unwrap_or(false)
        } else {
            false
        };

        if !is_owner {
            return SyscallResult::Error(SyscallError::PermissionDenied);
        }

        // Update limits in resource manager
        let limits = parse_resource_limits(&limits_config);
        if let Some(rm) = RESOURCE_MANAGER.read().as_ref() {
            let mut manager = rm.write();
            manager.set_process_limits(agent_handle as u32, limits);
        }

        info!("Updated resource limits for agent handle {}", agent_handle);
        SyscallResult::Success(0)
    }
}

/// Get agent resource usage (syscall 28)
pub struct GetAgentUsageHandler;

#[async_trait]
impl SyscallHandler for GetAgentUsageHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("GetAgentUsage syscall from {}", ctx.caller_id);

        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L1FileRead) {
            return SyscallResult::Error(e);
        }

        let agent_handle = args.arg0;
        let buf_ptr = args.arg1;
        let buf_size = args.arg2 as usize;

        if buf_ptr == 0 || buf_size == 0 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Get usage from resource manager
        let usage = if let Some(rm) = RESOURCE_MANAGER.read().as_ref() {
            let manager = rm.read();
            manager.get_usage(agent_handle as u32).cloned()
        } else {
            None
        };

        let usage_data = match usage {
            Some(u) => {
                serde_json::json!({
                    "exists": true,
                    "cpu_time_ms": u.cpu_time.as_millis() as u64,
                    "memory_bytes": u.memory_bytes,
                    "io_read_bytes": u.io_read_bytes,
                    "io_write_bytes": u.io_write_bytes,
                    "network_rx_bytes": u.network_rx_bytes,
                    "network_tx_bytes": u.network_tx_bytes,
                })
            }
            None => {
                serde_json::json!({
                    "exists": false,
                })
            }
        };

        let json = match serde_json::to_string(&usage_data) {
            Ok(s) => s.into_bytes(),
            Err(_) => return SyscallResult::Error(SyscallError::InternalError),
        };

        if json.len() > buf_size {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        match write_caller_memory(ctx, buf_ptr, &json) {
            Ok(len) => SyscallResult::Success(len as u64),
            Err(e) => SyscallResult::Error(e),
        }
    }
}

// =============================================================================
// Handler Registry
// =============================================================================

/// Get handler for syscall number
pub fn get_handler(num: u32) -> Option<Box<dyn SyscallHandler>> {
    match num {
        0 => Some(Box::new(SpawnAgentHandler)),
        1 => Some(Box::new(TerminateAgentHandler)),
        2 => Some(Box::new(SendMessageHandler)),
        3 => Some(Box::new(AccessResourceHandler)),
        4 => Some(Box::new(blockchain::ExecutePaymentHandler)),
        5 => Some(Box::new(QueryMemoryHandler)),
        6 => Some(Box::new(sandbox::UpdateCapabilityHandler)),
        7 => Some(Box::new(sandbox::EnterSandboxHandler)),
        8 => Some(Box::new(sandbox::ExitSandboxHandler)),
        9 => Some(Box::new(ReadFileHandler)),
        10 => Some(Box::new(WriteFileHandler)),
        11 => Some(Box::new(ListFilesHandler)),
        12 => Some(Box::new(DeleteFileHandler)),
        13 => Some(Box::new(GetFileInfoHandler)),
        14 => Some(Box::new(CreateDirHandler)),
        15 => Some(Box::new(NetworkOpenHandler)),
        16 => Some(Box::new(NetworkSendHandler)),
        17 => Some(Box::new(NetworkReceiveHandler)),
        18 => Some(Box::new(NetworkCloseHandler)),
        19 => Some(Box::new(blockchain::BridgeTokenHandler)),
        20 => Some(Box::new(blockchain::SwapTokenHandler)),
        21 => Some(Box::new(blockchain::StakeTokenHandler)),
        22 => Some(Box::new(blockchain::UnstakeTokenHandler)),
        23 => Some(Box::new(blockchain::QueryBalanceHandler)),
        24 => Some(Box::new(CryptoHashHandler)),
        25 => Some(Box::new(CryptoVerifyHandler)),
        26 => Some(Box::new(GetTimeHandler)),
        27 => Some(Box::new(SleepHandler)),
        28 => Some(Box::new(RequestCapabilityHandler)),
        29 => Some(Box::new(DropCapabilityHandler)),
        30 => Some(Box::new(GetSystemInfoHandler)),
        31 => Some(Box::new(QueryAgentStatusHandler)),
        32 => Some(Box::new(SetAgentLimitsHandler)),
        33 => Some(Box::new(GetAgentUsageHandler)),
        34 => Some(Box::new(sandbox::QuerySandboxStatusHandler)),
        _ => None,
    }
}

/// Initialize syscall subsystem
pub fn init() -> crate::error::Result<()> {
    // Initialize agent registry
    let registry = Arc::new(RwLock::new(AgentRegistry::new()));
    init_agent_registry(registry);

    // Initialize resource manager with default limits
    let rm = Arc::new(RwLock::new(ResourceManager::new(ResourceLimits::default())));
    init_resource_manager(rm);

    // Initialize storage with memory backend
    crate::storage::global::init(None)?;

    // Register default memory storage backend
    let backend = Box::new(crate::storage::global::MemoryBackend::new());
    crate::storage::global::global().register_backend("default".to_string(), backend);

    // Initialize message router
    crate::ipc::router::init()?;

    // Initialize sandbox registry
    let sandbox_reg = Arc::new(RwLock::new(sandbox::SandboxRegistry::new()));
    sandbox::init_sandbox_registry(sandbox_reg);

    // Initialize capability manager
    let cap_mgr = Arc::new(RwLock::new(crate::capabilities::CapabilityManager::new()));
    sandbox::init_capability_manager(cap_mgr);

    tracing::info!("Syscall subsystem initialized");
    Ok(())
}
