//! Security Module
//!
//! Provides security features for agent operations:
//! - Webhook signature verification
//! - Permission/Capability system
//! - Session isolation
//! - Secure memory handling

pub mod permission_system;
pub mod session_isolation;
pub mod webhook_security;

pub use permission_system::{
    capabilities as common_capabilities, Capability, PermissionAuditEvent, PermissionChecker,
    PermissionContext, PermissionResult,
};
pub use session_isolation::{
    IsolatedSession, IsolationError, IsolationLevel, ResourceLimits, ResourceUsage,
    SessionIsolationManager, SessionSecurityConfig,
};
pub use webhook_security::{ReplayProtection, WebhookSignatureVerifier};

use crate::error::Result;

/// Security manager for coordinating security features
pub struct SecurityManager {
    pub permission_checker: Arc<PermissionChecker>,
    pub session_isolation: Arc<SessionIsolationManager>,
}

use std::sync::Arc;

impl SecurityManager {
    pub fn new() -> Self {
        let permission_checker = Arc::new(PermissionChecker::new());
        let session_isolation = Arc::new(SessionIsolationManager::new(permission_checker.clone()));

        Self {
            permission_checker,
            session_isolation,
        }
    }

    /// Create with custom permission checker
    pub fn with_permission_checker(mut self, checker: Arc<PermissionChecker>) -> Self {
        self.permission_checker = checker;
        self
    }

    /// Initialize security subsystem
    pub async fn initialize(&self) -> Result<()> {
        tracing::info!("Security manager initialized");
        Ok(())
    }

    /// Shutdown security subsystem
    pub async fn shutdown(&self) -> Result<()> {
        tracing::info!("Security manager shutdown");
        Ok(())
    }
}

impl Default for SecurityManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Security configuration
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// Enable webhook signature verification
    pub verify_webhook_signatures: bool,
    /// Enable permission system
    pub enable_permissions: bool,
    /// Enable session isolation
    pub enable_session_isolation: bool,
    /// Default isolation level
    pub default_isolation: IsolationLevel,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            verify_webhook_signatures: true,
            enable_permissions: true,
            enable_session_isolation: true,
            default_isolation: IsolationLevel::Wasm,
        }
    }
}
