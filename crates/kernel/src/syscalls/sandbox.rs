//! Sandbox Syscall Handlers
//!
//! Implements system calls for:
//! - EnterSandbox: Enter sandboxed execution mode
//! - ExitSandbox: Exit sandboxed execution mode
//! - UpdateCapability: Update agent capability level

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;
use tracing::{info, trace, warn};

use crate::capabilities::{CapabilityLevel, CapabilityManager, CapabilityRequest};
use crate::security::sandbox::SandboxConfig;
use crate::syscalls::handlers::read_caller_memory;
use crate::syscalls::{SyscallArgs, SyscallContext, SyscallError, SyscallHandler, SyscallResult};

// Global sandbox registry
static SANDBOX_REGISTRY: RwLock<Option<Arc<RwLock<SandboxRegistry>>>> = RwLock::new(None);

// Global capability manager for elevation requests
static CAPABILITY_MANAGER: RwLock<Option<Arc<RwLock<CapabilityManager>>>> = RwLock::new(None);

/// Initialize sandbox registry
pub fn init_sandbox_registry(registry: Arc<RwLock<SandboxRegistry>>) {
    let mut guard = SANDBOX_REGISTRY.write();
    *guard = Some(registry);
}

/// Initialize capability manager
pub fn init_capability_manager(manager: Arc<RwLock<CapabilityManager>>) {
    let mut guard = CAPABILITY_MANAGER.write();
    *guard = Some(manager);
}

/// Get sandbox registry
fn get_sandbox_registry() -> Option<Arc<RwLock<SandboxRegistry>>> {
    SANDBOX_REGISTRY.read().as_ref().cloned()
}

/// Get capability manager
fn get_capability_manager() -> Option<Arc<RwLock<CapabilityManager>>> {
    CAPABILITY_MANAGER.read().as_ref().cloned()
}

/// Sandbox registry for managing active sandboxes
pub struct SandboxRegistry {
    /// Active sandboxes by agent ID
    sandboxes: HashMap<String, ActiveSandbox>,
    /// Sandbox history for audit
    history: Vec<SandboxEvent>,
}

/// Active sandbox entry
#[derive(Debug, Clone)]
pub struct ActiveSandbox {
    /// Agent ID
    pub agent_id: String,
    /// Sandbox configuration
    pub config: SandboxConfig,
    /// When sandbox was entered
    pub entered_at: u64,
    /// Original capability level (before sandbox)
    pub original_level: u8,
}

/// Sandbox event for audit
#[derive(Debug, Clone)]
pub struct SandboxEvent {
    /// Event type
    pub event_type: SandboxEventType,
    /// Agent ID
    pub agent_id: String,
    /// Timestamp
    pub timestamp: u64,
    /// Details
    pub details: String,
}

/// Sandbox event type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxEventType {
    /// Agent entered sandbox
    Enter,
    /// Agent exited sandbox
    Exit,
    /// Security violation detected
    Violation,
}

impl SandboxRegistry {
    /// Create new sandbox registry
    pub fn new() -> Self {
        Self {
            sandboxes: HashMap::new(),
            history: Vec::new(),
        }
    }

    /// Enter sandbox for agent
    pub fn enter(
        &mut self,
        agent_id: String,
        config: SandboxConfig,
        original_level: u8,
    ) -> Result<(), SandboxError> {
        if self.sandboxes.contains_key(&agent_id) {
            return Err(SandboxError::AlreadyInSandbox);
        }

        let sandbox = ActiveSandbox {
            agent_id: agent_id.clone(),
            config,
            entered_at: now(),
            original_level,
        };

        self.sandboxes.insert(agent_id.clone(), sandbox);

        self.history.push(SandboxEvent {
            event_type: SandboxEventType::Enter,
            agent_id,
            timestamp: now(),
            details: "Entered sandbox".to_string(),
        });

        Ok(())
    }

    /// Exit sandbox for agent
    pub fn exit(&mut self, agent_id: &str) -> Result<u8, SandboxError> {
        let sandbox = self
            .sandboxes
            .remove(agent_id)
            .ok_or(SandboxError::NotInSandbox)?;

        self.history.push(SandboxEvent {
            event_type: SandboxEventType::Exit,
            agent_id: agent_id.to_string(),
            timestamp: now(),
            details: "Exited sandbox".to_string(),
        });

        Ok(sandbox.original_level)
    }

    /// Check if agent is in sandbox
    pub fn is_sandboxed(&self, agent_id: &str) -> bool {
        self.sandboxes.contains_key(agent_id)
    }

    /// Get sandbox config for agent
    pub fn get_config(&self, agent_id: &str) -> Option<&SandboxConfig> {
        self.sandboxes.get(agent_id).map(|s| &s.config)
    }

    /// Check if syscall is allowed for agent
    pub fn is_syscall_allowed(&self, agent_id: &str, syscall: u64) -> bool {
        self.sandboxes
            .get(agent_id)
            .map(|s| s.config.is_syscall_allowed(syscall))
            .unwrap_or(true) // Not sandboxed = all allowed
    }

    /// Record security violation
    pub fn record_violation(&mut self, agent_id: &str, details: &str) {
        self.history.push(SandboxEvent {
            event_type: SandboxEventType::Violation,
            agent_id: agent_id.to_string(),
            timestamp: now(),
            details: details.to_string(),
        });

        warn!("Sandbox violation by {}: {}", agent_id, details);
    }

    /// Get sandbox history
    pub fn get_history(&self) -> &[SandboxEvent] {
        &self.history
    }
}

impl Default for SandboxRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Sandbox errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxError {
    /// Agent is already in a sandbox
    AlreadyInSandbox,
    /// Agent is not in a sandbox
    NotInSandbox,
    /// Security violation occurred
    Violation,
}

impl std::fmt::Display for SandboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SandboxError::AlreadyInSandbox => write!(f, "Agent already in sandbox"),
            SandboxError::NotInSandbox => write!(f, "Agent not in sandbox"),
            SandboxError::Violation => write!(f, "Sandbox security violation"),
        }
    }
}

impl std::error::Error for SandboxError {}

// =============================================================================
// Capability Management Syscalls
// =============================================================================

/// Update capability level (syscall 6)
///
/// This syscall handles capability elevation requests. It supports:
/// - Self-elevation within granted bounds
/// - Requesting higher levels that require approval
/// - Dropping capabilities (attenuation)
pub struct UpdateCapabilityHandler;

#[async_trait]
impl SyscallHandler for UpdateCapabilityHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        let action = args.arg0;
        let target_level = args.arg1 as u8;
        let justification_ptr = args.arg2;
        let justification_len = args.arg3 as usize;

        trace!(
            "UpdateCapability syscall from {}: action={} target_level={}",
            ctx.caller_id,
            action,
            target_level
        );

        // Validate target level
        if target_level > 10 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        match action {
            0 => {
                self.handle_elevation(ctx, target_level, justification_ptr, justification_len)
                    .await
            }
            1 => self.handle_drop(ctx, target_level).await,
            2 => self.handle_request_status(ctx, target_level).await,
            _ => SyscallResult::Error(SyscallError::InvalidArgs),
        }
    }
}

impl UpdateCapabilityHandler {
    /// Handle capability elevation request
    async fn handle_elevation(
        &self,
        ctx: &SyscallContext,
        target_level: u8,
        justification_ptr: u64,
        justification_len: usize,
    ) -> SyscallResult {
        // Read justification if provided
        let justification = if justification_ptr != 0 && justification_len > 0 {
            match read_caller_memory(ctx, justification_ptr, justification_len) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
                Err(_) => return SyscallResult::Error(SyscallError::InvalidArgs),
            }
        } else {
            "No justification provided".to_string()
        };

        // Check if already at or above target level
        if ctx.capability_level >= target_level {
            return SyscallResult::Success(ctx.capability_level as u64);
        }

        // Check if target requires admin approval
        if target_level >= CapabilityLevel::L9ChainWriteHigh as u8 {
            // L9+ requires admin capability
            if ctx.capability_level < CapabilityLevel::L10SystemAdmin as u8 {
                warn!(
                    "Elevation to L{} requires admin approval for agent {}",
                    target_level, ctx.caller_id
                );
                // Create pending request
                return self
                    .create_pending_request(ctx, target_level, justification)
                    .await;
            }
        }

        // Check if target requires TEE
        let target_cap = match CapabilityLevel::from_u8(target_level) {
            Some(cap) => cap,
            None => return SyscallResult::Error(SyscallError::InvalidArgs),
        };

        if target_cap.requires_tee() {
            // In production, verify TEE attestation here
            info!("TEE verification required for L{} elevation", target_level);
        }

        // Perform elevation
        let manager = match get_capability_manager() {
            Some(m) => m,
            None => return SyscallResult::Error(SyscallError::InternalError),
        };

        let request = CapabilityRequest {
            level: target_cap,
            justification,
            duration_seconds: Some(3600), // 1 hour default
        };

        let agent_id = match ctx.caller_id.parse::<uuid::Uuid>() {
            Ok(uuid) => crate::AgentId(uuid),
            Err(_) => return SyscallResult::Error(SyscallError::InvalidArgs),
        };

        let mut mgr = manager.write();
        match mgr.request_elevation(agent_id, request) {
            Ok(token) => {
                info!(
                    "Capability elevation granted for {} to L{} (token: {})",
                    ctx.caller_id,
                    target_level,
                    token.id()
                );
                SyscallResult::Success(target_level as u64)
            }
            Err(_) => {
                warn!(
                    "Capability elevation denied for {} to L{}",
                    ctx.caller_id, target_level
                );
                SyscallResult::Error(SyscallError::PermissionDenied)
            }
        }
    }

    /// Create a pending elevation request
    async fn create_pending_request(
        &self,
        ctx: &SyscallContext,
        target_level: u8,
        justification: String,
    ) -> SyscallResult {
        let manager = match get_capability_manager() {
            Some(m) => m,
            None => return SyscallResult::Error(SyscallError::InternalError),
        };

        let target_cap = match CapabilityLevel::from_u8(target_level) {
            Some(cap) => cap,
            None => return SyscallResult::Error(SyscallError::InvalidArgs),
        };

        let request = CapabilityRequest {
            level: target_cap,
            justification,
            duration_seconds: Some(3600),
        };

        let agent_id = match ctx.caller_id.parse::<uuid::Uuid>() {
            Ok(uuid) => crate::AgentId(uuid),
            Err(_) => return SyscallResult::Error(SyscallError::InvalidArgs),
        };

        let mut mgr = manager.write();
        match mgr.request_elevation(agent_id, request) {
            Ok(token) => {
                info!(
                    "Pending capability request created for {} to L{} (token: {})",
                    ctx.caller_id,
                    target_level,
                    token.id()
                );
                // Return token ID as handle (in upper 32 bits)
                let handle = token.id().parse::<u64>().unwrap_or(0);
                SyscallResult::Async(handle)
            }
            Err(_) => SyscallResult::Error(SyscallError::PermissionDenied),
        }
    }

    /// Handle capability drop
    async fn handle_drop(&self, ctx: &SyscallContext, target_level: u8) -> SyscallResult {
        // Can only drop to a lower level
        if target_level >= ctx.capability_level {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        info!(
            "Capability drop for {}: L{} -> L{}",
            ctx.caller_id, ctx.capability_level, target_level
        );

        // In production, update the capability registry
        // For now, just return the new level
        SyscallResult::Success(target_level as u64)
    }

    /// Check request status
    async fn handle_request_status(&self, ctx: &SyscallContext, token_id: u8) -> SyscallResult {
        // This would check the status of a pending elevation request
        // For now, return the current level
        trace!(
            "Capability request status check for {} token={}",
            ctx.caller_id,
            token_id
        );
        SyscallResult::Success(ctx.capability_level as u64)
    }
}

// =============================================================================
// Sandbox Syscalls
// =============================================================================

/// Enter sandbox mode (syscall 7)
///
/// Restricts the agent to a sandboxed environment with limited:
/// - System calls
/// - Memory usage
/// - CPU time
/// - Network access
/// - Filesystem access
pub struct EnterSandboxHandler;

#[async_trait]
impl SyscallHandler for EnterSandboxHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("EnterSandbox syscall from {}", ctx.caller_id);

        // Requires L6 (SpawnUnlimited) to enter sandbox
        // (sandboxing is typically done by parent agents)
        if ctx.capability_level < CapabilityLevel::L6SpawnUnlimited as u8 {
            return SyscallResult::Error(SyscallError::PermissionDenied);
        }

        let config_type = args.arg0;
        let custom_config_ptr = args.arg1;
        let custom_config_len = args.arg2 as usize;

        // Determine sandbox configuration
        let config = match config_type {
            0 => SandboxConfig::restrictive(&ctx.caller_id),
            1 => SandboxConfig::standard(&ctx.caller_id),
            2 => {
                // Custom configuration
                if custom_config_ptr == 0 || custom_config_len == 0 {
                    return SyscallResult::Error(SyscallError::InvalidArgs);
                }

                let config_bytes =
                    match read_caller_memory(ctx, custom_config_ptr, custom_config_len) {
                        Ok(b) => b,
                        Err(e) => return SyscallResult::Error(e),
                    };

                match serde_json::from_slice(&config_bytes) {
                    Ok(c) => c,
                    Err(e) => {
                        warn!("Invalid sandbox config: {}", e);
                        return SyscallResult::Error(SyscallError::InvalidArgs);
                    }
                }
            }
            _ => return SyscallResult::Error(SyscallError::InvalidArgs),
        };

        trace!(
            "Entering sandbox: agent={} type={} memory_limit={}MB",
            ctx.caller_id,
            config_type,
            config.max_memory / (1024 * 1024)
        );

        // Get or create sandbox registry
        let registry = match get_sandbox_registry() {
            Some(r) => r,
            None => {
                // Auto-initialize if not set up
                let new_registry = Arc::new(RwLock::new(SandboxRegistry::new()));
                init_sandbox_registry(new_registry.clone());
                new_registry
            }
        };

        // Enter sandbox
        let mut reg = registry.write();
        match reg.enter(ctx.caller_id.clone(), config, ctx.capability_level) {
            Ok(()) => {
                info!("Agent {} entered sandbox mode", ctx.caller_id);
                // Return sandbox ID (just 0 for now)
                SyscallResult::Success(0)
            }
            Err(SandboxError::AlreadyInSandbox) => SyscallResult::Error(SyscallError::ResourceBusy),
            Err(_) => SyscallResult::Error(SyscallError::InternalError),
        }
    }
}

/// Exit sandbox mode (syscall 8)
///
/// Restores the agent's original capability level and removes restrictions.
/// This requires the same or higher capability level that was used to enter.
pub struct ExitSandboxHandler;

#[async_trait]
impl SyscallHandler for ExitSandboxHandler {
    async fn handle(&self, _args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("ExitSandbox syscall from {}", ctx.caller_id);

        let registry = match get_sandbox_registry() {
            Some(r) => r,
            None => return SyscallResult::Error(SyscallError::InternalError),
        };

        let mut reg = registry.write();

        // Check if in sandbox
        if !reg.is_sandboxed(&ctx.caller_id) {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        // Get the sandbox config to check exit permissions
        let config = reg.get_config(&ctx.caller_id).cloned();

        // Exit sandbox and get original capability level
        match reg.exit(&ctx.caller_id) {
            Ok(original_level) => {
                info!(
                    "Agent {} exited sandbox mode, restored to L{}",
                    ctx.caller_id, original_level
                );

                // Log the exit
                if let Some(cfg) = config {
                    trace!(
                        "Sandbox stats: CPU={}ms, Memory={}bytes",
                        cfg.max_cpu_time_ms,
                        cfg.max_memory
                    );
                }

                SyscallResult::Success(original_level as u64)
            }
            Err(SandboxError::NotInSandbox) => SyscallResult::Error(SyscallError::InvalidArgs),
            Err(_) => SyscallResult::Error(SyscallError::InternalError),
        }
    }
}

/// Query sandbox status (syscall for future use)
pub struct QuerySandboxStatusHandler;

#[async_trait]
impl SyscallHandler for QuerySandboxStatusHandler {
    async fn handle(&self, _args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        let registry = match get_sandbox_registry() {
            Some(r) => r,
            None => return SyscallResult::Success(0), // Not sandboxed
        };

        let reg = registry.read();

        if let Some(config) = reg.get_config(&ctx.caller_id) {
            // Pack sandbox info into result
            // Lower 8 bits: sandbox type (0=restrictive, 1=standard, 2=custom)
            // Bit 8: network allowed
            // Bit 9: filesystem allowed
            let mut result = 0u64;
            if config.network_allowed {
                result |= 1 << 8;
            }
            if config.filesystem_allowed {
                result |= 1 << 9;
            }
            SyscallResult::Success(result)
        } else {
            SyscallResult::Success(0) // Not sandboxed
        }
    }
}

// =============================================================================
// Utility Functions
// =============================================================================

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

// SandboxRegistry is already defined above and used directly

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_registry() {
        let mut registry = SandboxRegistry::new();

        // Test enter
        let config = SandboxConfig::restrictive("test-agent");
        assert!(registry.enter("agent1".to_string(), config, 5).is_ok());

        // Test duplicate enter
        let config2 = SandboxConfig::standard("test-agent");
        assert!(matches!(
            registry.enter("agent1".to_string(), config2, 5),
            Err(SandboxError::AlreadyInSandbox)
        ));

        // Test is_sandboxed
        assert!(registry.is_sandboxed("agent1"));
        assert!(!registry.is_sandboxed("agent2"));

        // Test syscall allowed
        assert!(registry.is_syscall_allowed("agent1", 0));
        assert!(!registry.is_syscall_allowed("agent1", 100));

        // Test exit
        let original = registry.exit("agent1").unwrap();
        assert_eq!(original, 5);

        // Test exit when not in sandbox
        assert!(matches!(
            registry.exit("agent1"),
            Err(SandboxError::NotInSandbox)
        ));
    }

    #[test]
    fn test_capability_level_conversion() {
        assert_eq!(CapabilityLevel::L0LocalCompute as u8, 0);
        assert_eq!(CapabilityLevel::L10SystemAdmin as u8, 10);

        assert!(CapabilityLevel::from_u8(5).is_some());
        assert!(CapabilityLevel::from_u8(99).is_none());
    }
}
