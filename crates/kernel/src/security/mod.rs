//! Security Module
//!
//! Provides security primitives for access control, auditing, and sandboxing.
//!
//! ## Submodules
//!
//! - `acl`: Access control lists and role-based access control
//! - `audit`: Security auditing and logging
//! - `sandbox`: Process sandboxing and isolation
//! - `path`: Path validation and sandbox escape prevention
//!
//! ## Capabilities
//!
//! Capability types are re-exported from the main `capabilities` module:
//! - `CapabilityLevel`: 11-tier capability levels (L0-L10)
//! - `CapabilitySet`: Set of capabilities with permissions
//! - `CapabilityManager`: Manager for agent capabilities
//! - `CapabilityToken`: Token for temporary capability elevation

pub mod acl;
pub mod audit;
pub mod path;
pub mod sandbox;

/// Trusted Execution Environment (TEE) support
pub mod tee;

use std::collections::HashMap;
use std::sync::Arc;

pub use acl::{
    AccessCondition, AccessControlList, AclEntry, AclEntryType, MacPolicy, RbacManager, SubjectType,
};
pub use audit::{
    AuditBackend, AuditConfig, AuditEncryptionKey, AuditEntry, AuditFilter, AuditLog, AuditStats,
    EncryptedAuditEntry,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

// Re-export capabilities from the main capabilities module for security use
pub use crate::capabilities::{CapabilityLevel, CapabilityManager, CapabilitySet, CapabilityToken};
use crate::error::{KernelError, Result};

/// Process identifier used in security contexts
pub type ProcessId = u64;

/// Maximum length for client_ip field to prevent memory exhaustion attacks
///
/// IPv6 addresses with zone identifiers can be up to 45 characters,
/// so we allow some margin for formatting.
pub const MAX_CLIENT_IP_LEN: usize = 64;

/// Maximum length for user_id and group_id fields
pub const MAX_SECURITY_STRING_LEN: usize = 256;

/// Maximum length for session_id field
pub const MAX_SESSION_ID_LEN: usize = 128;

/// Security context for access control decisions
#[derive(Debug, Clone)]
pub struct SecurityContext {
    /// Unique user identifier
    pub user_id: String,
    /// Group membership identifier
    pub group_id: String,
    /// Granted capabilities for this context
    pub capabilities: Vec<Capability>,
    /// Information classification clearance level
    pub clearance_level: ClearanceLevel,
    /// Client IP address (for ABAC IP-based access control)
    pub client_ip: Option<String>,
    /// Session ID
    pub session_id: Option<String>,
}

/// Information classification levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClearanceLevel {
    /// Publicly accessible information
    Public = 0,
    /// Internal use only
    Internal = 1,
    /// Confidential information
    Confidential = 2,
    /// Secret information
    Secret = 3,
    /// Top secret information
    TopSecret = 4,
}

/// Individual capability granted to a security context
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    /// Read access to filesystem
    FileRead,
    /// Write access to filesystem
    FileWrite,
    /// Execute permission on files
    FileExecute,
    /// Network access permission
    NetworkAccess,
    /// Permission to spawn new processes
    ProcessSpawn,
    /// Permission to terminate processes
    ProcessKill,
    /// Permission to allocate memory
    MemoryAllocate,
    /// Access to specific device
    DeviceAccess(String),
    /// Permission to invoke specific system call
    SystemCall(String),
    /// Wildcard capability (grants all permissions)
    All,
}

impl Capability {
    /// Check if this capability matches (satisfies) a required capability
    pub fn matches(&self, required: &Capability) -> bool {
        // Wildcard matches everything
        if *self == Capability::All {
            return true;
        }

        // Exact match
        if self == required {
            return true;
        }

        // Device access patterns
        match (self, required) {
            (Capability::All, _) => true,
            (Capability::DeviceAccess(pattern), Capability::DeviceAccess(device)) => {
                // Simple pattern matching: "disk:*" matches "disk:sda1"
                if pattern.ends_with('*') {
                    let prefix = &pattern[..pattern.len() - 1];
                    device.starts_with(prefix)
                } else {
                    pattern == device
                }
            }
            (Capability::SystemCall(pattern), Capability::SystemCall(syscall)) => {
                // Pattern matching for syscalls
                if pattern.ends_with('*') {
                    let prefix = &pattern[..pattern.len() - 1];
                    syscall.starts_with(prefix)
                } else {
                    pattern == syscall
                }
            }
            _ => false,
        }
    }
}

/// Security policy for access control decisions
pub trait SecurityPolicy: Send + Sync {
    /// Check if access should be granted
    fn check_access(
        &self,
        subject: &SecurityContext,
        object: &str,
        action: AccessAction,
    ) -> AccessDecision;
    /// Check if subject has specific capability
    fn check_capability(&self, subject: &SecurityContext, capability: &Capability) -> bool;
}

/// Types of access actions that can be performed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AccessAction {
    /// Read access
    Read,
    /// Write access
    Write,
    /// Execute permission
    Execute,
    /// Delete permission
    Delete,
    /// Create permission
    Create,
}

/// Result of an access control decision
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessDecision {
    /// Access is granted
    Allow,
    /// Access is denied
    Deny,
    /// User confirmation required
    Ask,
}

/// Security manager with audit logging
pub struct SecurityManager {
    /// Registered security policies
    policies: Vec<Box<dyn SecurityPolicy>>,
    /// Audit log for security events
    audit_log: Arc<Mutex<AuditLog>>,
    /// Security contexts by agent ID
    contexts: Mutex<HashMap<String, SecurityContext>>,
}

impl Default for SecurityManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SecurityContext {
    /// Create a new validated security context
    ///
    /// Returns an error if any string field exceeds its maximum allowed length.
    pub fn new(
        user_id: impl Into<String>,
        group_id: impl Into<String>,
        capabilities: Vec<Capability>,
        clearance_level: ClearanceLevel,
    ) -> Result<Self> {
        let user_id = user_id.into();
        let group_id = group_id.into();

        if user_id.len() > MAX_SECURITY_STRING_LEN {
            return Err(KernelError::invalid_argument(format!(
                "user_id exceeds maximum length of {}",
                MAX_SECURITY_STRING_LEN
            )));
        }
        if group_id.len() > MAX_SECURITY_STRING_LEN {
            return Err(KernelError::invalid_argument(format!(
                "group_id exceeds maximum length of {}",
                MAX_SECURITY_STRING_LEN
            )));
        }

        Ok(Self {
            user_id,
            group_id,
            capabilities,
            clearance_level,
            client_ip: None,
            session_id: None,
        })
    }

    /// Set client IP with validation
    pub fn with_client_ip(mut self, client_ip: impl Into<String>) -> Result<Self> {
        let ip = client_ip.into();
        if ip.len() > MAX_CLIENT_IP_LEN {
            return Err(KernelError::invalid_argument(format!(
                "client_ip exceeds maximum length of {}",
                MAX_CLIENT_IP_LEN
            )));
        }
        self.client_ip = Some(ip);
        Ok(self)
    }

    /// Set session ID with validation
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Result<Self> {
        let sid = session_id.into();
        if sid.len() > MAX_SESSION_ID_LEN {
            return Err(KernelError::invalid_argument(format!(
                "session_id exceeds maximum length of {}",
                MAX_SESSION_ID_LEN
            )));
        }
        self.session_id = Some(sid);
        Ok(self)
    }

    /// Validate all fields meet length constraints
    pub fn validate(&self) -> Result<()> {
        if self.user_id.len() > MAX_SECURITY_STRING_LEN {
            return Err(KernelError::invalid_argument(format!(
                "user_id exceeds maximum length of {}",
                MAX_SECURITY_STRING_LEN
            )));
        }
        if self.group_id.len() > MAX_SECURITY_STRING_LEN {
            return Err(KernelError::invalid_argument(format!(
                "group_id exceeds maximum length of {}",
                MAX_SECURITY_STRING_LEN
            )));
        }
        if let Some(ref ip) = self.client_ip {
            if ip.len() > MAX_CLIENT_IP_LEN {
                return Err(KernelError::invalid_argument(format!(
                    "client_ip exceeds maximum length of {}",
                    MAX_CLIENT_IP_LEN
                )));
            }
        }
        if let Some(ref sid) = self.session_id {
            if sid.len() > MAX_SESSION_ID_LEN {
                return Err(KernelError::invalid_argument(format!(
                    "session_id exceeds maximum length of {}",
                    MAX_SESSION_ID_LEN
                )));
            }
        }
        Ok(())
    }
}

impl SecurityManager {
    /// Create new security manager with in-memory audit log
    pub fn new() -> Self {
        Self {
            policies: vec![],
            audit_log: Arc::new(Mutex::new(AuditLog::new())),
            contexts: Mutex::new(HashMap::new()),
        }
    }

    /// Create security manager with custom audit configuration
    pub fn with_audit_config(config: AuditConfig) -> Result<Self> {
        Ok(Self {
            policies: vec![],
            audit_log: Arc::new(Mutex::new(AuditLog::with_config(config)?)),
            contexts: Mutex::new(HashMap::new()),
        })
    }

    /// Register a security policy
    pub fn register_policy(&mut self, policy: Box<dyn SecurityPolicy>) {
        self.policies.push(policy);
    }

    /// Register a security context for an agent
    ///
    /// Validates the context before registration to prevent malformed
    /// or malicious data from entering the security subsystem.
    pub fn register(&self, context: SecurityContext) -> Result<()> {
        context.validate()?;
        let mut contexts = self.contexts.lock();
        contexts.insert(context.user_id.clone(), context);
        Ok(())
    }

    /// Unregister a security context
    pub fn unregister(&self, user_id: &str) {
        let mut contexts = self.contexts.lock();
        contexts.remove(user_id);
    }

    /// Get security context by user ID
    pub fn get_context(&self, user_id: &str) -> Option<SecurityContext> {
        let contexts = self.contexts.lock();
        contexts.get(user_id).cloned()
    }

    /// Request access to an object
    pub fn request_access(
        &self,
        subject: &SecurityContext,
        object: &str,
        action: AccessAction,
    ) -> AccessDecision {
        let decision = self.check_access_internal(subject, object, action);

        // Log the access attempt
        let audit_log = self.audit_log.lock();
        audit_log.log_access_attempt(subject, object, action, decision);

        decision
    }

    fn check_access_internal(
        &self,
        subject: &SecurityContext,
        object: &str,
        action: AccessAction,
    ) -> AccessDecision {
        for policy in &self.policies {
            let decision = policy.check_access(subject, object, action);

            match decision {
                AccessDecision::Deny => return AccessDecision::Deny,
                AccessDecision::Allow => continue,
                AccessDecision::Ask => return AccessDecision::Ask,
            }
        }

        AccessDecision::Allow
    }

    /// Check if subject has a specific capability
    pub fn check_capability(&self, subject: &SecurityContext, capability: &Capability) -> bool {
        self.policies
            .iter()
            .all(|policy| policy.check_capability(subject, capability))
    }

    /// Get audit log for queries
    pub fn audit_log(&self) -> Arc<Mutex<AuditLog>> {
        self.audit_log.clone()
    }

    /// Flush audit log to persistent storage
    pub fn flush_audit_log(&self) -> Result<()> {
        let audit_log = self.audit_log.lock();
        audit_log.flush()
    }
}

/// Discretionary Access Control implementation using ACLs
pub struct DiscretionaryAccessControl {
    acl_table: std::collections::HashMap<String, AccessControlList>,
    rbac: RbacManager,
}

impl Default for DiscretionaryAccessControl {
    fn default() -> Self {
        Self::new()
    }
}

impl DiscretionaryAccessControl {
    /// Create a new DAC security manager
    pub fn new() -> Self {
        Self {
            acl_table: std::collections::HashMap::new(),
            rbac: RbacManager::new(),
        }
    }

    /// Set ACL for an object
    pub fn set_acl(&mut self, object: &str, acl: AccessControlList) {
        self.acl_table.insert(object.to_string(), acl);
    }

    /// Get ACL for an object
    pub fn get_acl(&self, object: &str) -> Option<&AccessControlList> {
        self.acl_table.get(object)
    }

    /// Assign a role to a user
    pub fn assign_role(&mut self, user: impl Into<String>, role: impl Into<String>) {
        self.rbac.assign_role(user, role);
    }
}

impl SecurityPolicy for DiscretionaryAccessControl {
    fn check_access(
        &self,
        subject: &SecurityContext,
        object: &str,
        action: AccessAction,
    ) -> AccessDecision {
        // 1. Check ACL first
        if let Some(acl) = self.acl_table.get(object) {
            let decision = acl.check_access(subject, action);
            if decision != AccessDecision::Allow {
                return decision;
            }
        }

        // 2. Fall back to RBAC
        self.rbac.check_access(&subject.user_id, object, action)
    }

    fn check_capability(&self, subject: &SecurityContext, capability: &Capability) -> bool {
        subject.capabilities.contains(capability)
    }
}
