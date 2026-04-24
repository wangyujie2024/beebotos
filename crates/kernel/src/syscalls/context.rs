//! Syscall Context with Dependency Injection
//!
//! Provides a clean dependency injection container for system calls,
//! replacing global static variables with proper dependency management.

use std::sync::Arc;

use parking_lot::RwLock;

use crate::capabilities::{CapabilityLevel, CapabilityManager};
use crate::ipc::router::MessageRouter;
use crate::network::transport::ConnectionManager;
use crate::resource::{ResourceLimits, ResourceManager};
use crate::storage::global::GlobalStorage;

/// Resource handle for tracking open resources
#[derive(Debug, Clone)]
pub struct ResourceHandle {
    /// Handle ID
    pub id: u64,
    /// Resource type
    pub resource_type: ResourceType,
    /// Owner process ID
    pub owner_pid: u64,
    /// Permissions
    pub permissions: ResourcePermissions,
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
    /// Custom resource
    Custom(u32),
}

/// Resource permissions
#[derive(Debug, Clone, Copy, Default)]
pub struct ResourcePermissions {
    /// Read permission
    pub read: bool,
    /// Write permission
    pub write: bool,
    /// Execute permission
    pub execute: bool,
}

/// Resource registry for tracking open resources
#[derive(Debug, Default)]
pub struct ResourceRegistry {
    resources: std::collections::HashMap<u64, ResourceHandle>,
    next_handle: u64,
}

impl ResourceRegistry {
    /// Create new resource registry
    pub fn new() -> Self {
        Self {
            resources: std::collections::HashMap::new(),
            next_handle: 1,
        }
    }

    /// Register a resource
    pub fn register(
        &mut self,
        resource_type: ResourceType,
        owner_pid: u64,
        permissions: ResourcePermissions,
    ) -> u64 {
        let handle = self.next_handle;
        self.next_handle += 1;

        let handle_info = ResourceHandle {
            id: handle,
            resource_type,
            owner_pid,
            permissions,
        };

        self.resources.insert(handle, handle_info);
        handle
    }

    /// Get resource info
    pub fn get(&self, handle: u64) -> Option<&ResourceHandle> {
        self.resources.get(&handle)
    }

    /// Check if caller owns the resource
    pub fn check_owner(&self, handle: u64, pid: u64) -> bool {
        self.resources
            .get(&handle)
            .map(|info| info.owner_pid == pid)
            .unwrap_or(false)
    }

    /// Remove a resource
    pub fn remove(&mut self, handle: u64) -> Option<ResourceHandle> {
        self.resources.remove(&handle)
    }

    /// Get all resources owned by a process
    pub fn get_process_resources(&self, pid: u64) -> Vec<&ResourceHandle> {
        self.resources
            .values()
            .filter(|info| info.owner_pid == pid)
            .collect()
    }

    /// Cleanup resources for a process
    pub fn cleanup_process(&mut self, pid: u64) -> Vec<ResourceHandle> {
        let handles: Vec<u64> = self
            .resources
            .values()
            .filter(|info| info.owner_pid == pid)
            .map(|info| info.id)
            .collect();

        let mut removed = Vec::new();
        for handle in handles {
            if let Some(info) = self.resources.remove(&handle) {
                removed.push(info);
            }
        }
        removed
    }
}

/// Syscall service container with dependency injection
#[derive(Clone)]
pub struct SyscallServices {
    /// Capability manager
    pub capability_manager: Arc<RwLock<CapabilityManager>>,
    /// Resource manager
    pub resource_manager: Arc<RwLock<ResourceManager>>,
    /// Resource registry for open handles
    pub resource_registry: Arc<RwLock<ResourceRegistry>>,
    /// Storage backend
    pub storage: Arc<GlobalStorage>,
    /// Message router
    pub message_router: Arc<MessageRouter>,
    /// Connection manager
    pub connection_manager: Arc<RwLock<ConnectionManager>>,
}

impl std::fmt::Debug for SyscallServices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyscallServices")
            .field("capability_manager", &"<CapabilityManager>")
            .field("resource_manager", &"<ResourceManager>")
            .field("resource_registry", &self.resource_registry)
            .field("storage", &self.storage)
            .field("message_router", &"<MessageRouter>")
            .field("connection_manager", &"<ConnectionManager>")
            .finish()
    }
}

impl SyscallServices {
    /// Create new service container
    pub fn new() -> Self {
        Self {
            capability_manager: Arc::new(RwLock::new(CapabilityManager::new())),
            resource_manager: Arc::new(RwLock::new(
                ResourceManager::new(ResourceLimits::default()),
            )),
            resource_registry: Arc::new(RwLock::new(ResourceRegistry::new())),
            storage: Arc::new(GlobalStorage::new()),
            message_router: Arc::new(MessageRouter::new()),
            connection_manager: Arc::new(RwLock::new(ConnectionManager::new())),
        }
    }

    /// Create with custom components
    pub fn with_components(
        capability_manager: Arc<RwLock<CapabilityManager>>,
        resource_manager: Arc<RwLock<ResourceManager>>,
        storage: Arc<GlobalStorage>,
        message_router: Arc<MessageRouter>,
    ) -> Self {
        Self {
            capability_manager,
            resource_manager,
            resource_registry: Arc::new(RwLock::new(ResourceRegistry::new())),
            storage,
            message_router,
            connection_manager: Arc::new(RwLock::new(ConnectionManager::new())),
        }
    }

    /// Register a resource
    pub fn register_resource(
        &self,
        resource_type: ResourceType,
        owner_pid: u64,
        permissions: ResourcePermissions,
    ) -> u64 {
        self.resource_registry
            .write()
            .register(resource_type, owner_pid, permissions)
    }

    /// Get resource info
    pub fn get_resource(&self, handle: u64) -> Option<ResourceHandle> {
        self.resource_registry.read().get(handle).cloned()
    }

    /// Check resource ownership
    pub fn check_resource_owner(&self, handle: u64, pid: u64) -> bool {
        self.resource_registry.read().check_owner(handle, pid)
    }

    /// Close a resource
    pub fn close_resource(&self, handle: u64) -> Option<ResourceHandle> {
        self.resource_registry.write().remove(handle)
    }
}

impl Default for SyscallServices {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-local syscall execution context with DI
pub struct SyscallExecutionContext {
    /// Caller agent ID
    pub caller_id: String,
    /// Process ID
    pub process_id: u64,
    /// Capability level
    pub capability_level: u8,
    /// Workspace ID
    pub workspace_id: String,
    /// Session ID
    pub session_id: String,
    /// Services container
    pub services: Arc<SyscallServices>,
}

impl SyscallExecutionContext {
    /// Create new context
    pub fn new(
        caller_id: impl Into<String>,
        process_id: u64,
        capability_level: u8,
        services: Arc<SyscallServices>,
    ) -> Self {
        Self {
            caller_id: caller_id.into(),
            process_id,
            capability_level,
            workspace_id: String::new(),
            session_id: String::new(),
            services,
        }
    }

    /// Set workspace
    pub fn with_workspace(mut self, workspace_id: impl Into<String>) -> Self {
        self.workspace_id = workspace_id.into();
        self
    }

    /// Set session
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = session_id.into();
        self
    }

    /// Check capability
    pub fn check_capability(&self, required: CapabilityLevel) -> bool {
        self.capability_level >= required as u8
    }

    /// Get capability manager
    pub fn capability_manager(&self) -> Arc<RwLock<CapabilityManager>> {
        self.services.capability_manager.clone()
    }

    /// Get resource manager
    pub fn resource_manager(&self) -> Arc<RwLock<ResourceManager>> {
        self.services.resource_manager.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_registry() {
        let mut registry = ResourceRegistry::new();

        // Register a resource
        let handle = registry.register(ResourceType::File, 123, ResourcePermissions::default());
        assert_eq!(handle, 1);

        // Check ownership
        assert!(registry.check_owner(handle, 123));
        assert!(!registry.check_owner(handle, 456));

        // Get resource info
        let info = registry.get(handle).unwrap();
        assert_eq!(info.owner_pid, 123);
        assert_eq!(info.resource_type, ResourceType::File);

        // Remove resource
        let removed = registry.remove(handle).unwrap();
        assert_eq!(removed.id, handle);
        assert!(registry.get(handle).is_none());
    }

    #[test]
    fn test_resource_registry_cleanup() {
        let mut registry = ResourceRegistry::new();

        // Register resources for different processes
        registry.register(ResourceType::File, 1, ResourcePermissions::default());
        registry.register(ResourceType::File, 1, ResourcePermissions::default());
        registry.register(ResourceType::File, 2, ResourcePermissions::default());

        // Cleanup process 1
        let removed = registry.cleanup_process(1);
        assert_eq!(removed.len(), 2);

        // Verify process 2's resource remains
        assert_eq!(registry.get_process_resources(2).len(), 1);
    }

    #[test]
    fn test_syscall_context() {
        let services = Arc::new(SyscallServices::new());
        let ctx = SyscallExecutionContext::new("agent1", 123, 5, services);

        assert_eq!(ctx.caller_id, "agent1");
        assert_eq!(ctx.process_id, 123);
        assert_eq!(ctx.capability_level, 5);
        assert!(ctx.check_capability(CapabilityLevel::L5SpawnLimited));
        assert!(!ctx.check_capability(CapabilityLevel::L10SystemAdmin));
    }
}
