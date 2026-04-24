//! System Calls
//!
//! Kernel syscall interface with 64 available syscalls.

pub mod blockchain;
pub mod context;
pub mod handlers;
pub mod sandbox;

// context module available but not re-exported to avoid conflicts

use std::collections::HashMap;

/// Syscall numbers
#[repr(u64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyscallNumber {
    /// Spawn a new agent (syscall 0)
    SpawnAgent = 0,
    /// Terminate an agent (syscall 1)
    TerminateAgent = 1,
    /// Send a message to another agent (syscall 2)
    SendMessage = 2,
    /// Access a system resource (syscall 3)
    AccessResource = 3,
    /// Execute a payment (syscall 4)
    ExecutePayment = 4,
    /// Query memory usage (syscall 5)
    QueryMemory = 5,
    /// Update capability level (syscall 6)
    UpdateCapability = 6,
    /// Enter sandbox mode (syscall 7)
    EnterSandbox = 7,
    /// Exit sandbox mode (syscall 8)
    ExitSandbox = 8,
    /// Read a file (syscall 9)
    ReadFile = 9,
    /// Write to a file (syscall 10)
    WriteFile = 10,
    /// List files in directory (syscall 11)
    ListFiles = 11,
    /// Create a workspace (syscall 12)
    CreateWorkspace = 12,
    /// Delete a workspace (syscall 13)
    DeleteWorkspace = 13,
    /// Query agent state (syscall 14)
    QueryState = 14,
    /// Update agent state (syscall 15)
    UpdateState = 15,
    /// Schedule a task (syscall 16)
    ScheduleTask = 16,
    /// Cancel a scheduled task (syscall 17)
    CancelTask = 17,
    /// Query scheduler state (syscall 18)
    QuerySchedule = 18,
    /// Bridge tokens between chains (syscall 19)
    BridgeToken = 19,
    /// Swap tokens (syscall 20)
    SwapToken = 20,
    /// Stake tokens (syscall 21)
    StakeToken = 21,
    /// Unstake tokens (syscall 22)
    UnstakeToken = 22,
    /// Query token balance (syscall 23)
    QueryBalance = 23,
    /// Request attestation (syscall 24)
    RequestAttestation = 24,
    /// Verify attestation (syscall 25)
    VerifyAttestation = 25,
    /// Log an event (syscall 26)
    LogEvent = 26,
    /// Emit a metric (syscall 27)
    EmitMetric = 27,
    /// Query metrics (syscall 28)
    QueryMetrics = 28,
    // Reserved for future use: 29-63
}

impl SyscallNumber {
    /// Convert u64 to SyscallNumber
    pub fn from_u64(n: u64) -> Option<Self> {
        match n {
            0 => Some(SyscallNumber::SpawnAgent),
            1 => Some(SyscallNumber::TerminateAgent),
            2 => Some(SyscallNumber::SendMessage),
            3 => Some(SyscallNumber::AccessResource),
            4 => Some(SyscallNumber::ExecutePayment),
            5 => Some(SyscallNumber::QueryMemory),
            6 => Some(SyscallNumber::UpdateCapability),
            7 => Some(SyscallNumber::EnterSandbox),
            8 => Some(SyscallNumber::ExitSandbox),
            9 => Some(SyscallNumber::ReadFile),
            10 => Some(SyscallNumber::WriteFile),
            11 => Some(SyscallNumber::ListFiles),
            12 => Some(SyscallNumber::CreateWorkspace),
            13 => Some(SyscallNumber::DeleteWorkspace),
            14 => Some(SyscallNumber::QueryState),
            15 => Some(SyscallNumber::UpdateState),
            16 => Some(SyscallNumber::ScheduleTask),
            17 => Some(SyscallNumber::CancelTask),
            18 => Some(SyscallNumber::QuerySchedule),
            19 => Some(SyscallNumber::BridgeToken),
            20 => Some(SyscallNumber::SwapToken),
            21 => Some(SyscallNumber::StakeToken),
            22 => Some(SyscallNumber::UnstakeToken),
            23 => Some(SyscallNumber::QueryBalance),
            24 => Some(SyscallNumber::RequestAttestation),
            25 => Some(SyscallNumber::VerifyAttestation),
            26 => Some(SyscallNumber::LogEvent),
            27 => Some(SyscallNumber::EmitMetric),
            28 => Some(SyscallNumber::QueryMetrics),
            _ => None,
        }
    }
}

/// Syscall arguments
#[derive(Debug, Clone, Default)]
pub struct SyscallArgs {
    /// First argument
    pub arg0: u64,
    /// Second argument
    pub arg1: u64,
    /// Third argument
    pub arg2: u64,
    /// Fourth argument
    pub arg3: u64,
    /// Fifth argument
    pub arg4: u64,
    /// Sixth argument
    pub arg5: u64,
}

/// Syscall result
#[derive(Debug, Clone)]
pub enum SyscallResult {
    /// Syscall completed successfully with return value
    Success(u64),
    /// Syscall failed with error
    Error(SyscallError),
    /// Async operation handle (for async operations)
    Async(u64),
}

/// Syscall error codes
#[repr(i64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyscallError {
    /// Success (no error)
    Success = 0,
    /// Invalid syscall number
    InvalidSyscall = -1,
    /// Invalid arguments
    InvalidArgs = -2,
    /// Permission denied
    PermissionDenied = -3,
    /// Resource not found
    ResourceNotFound = -4,
    /// Resource is busy
    ResourceBusy = -5,
    /// Out of memory
    OutOfMemory = -6,
    /// Operation timed out
    Timeout = -7,
    /// Operation was cancelled
    Cancelled = -8,
    /// Internal kernel error
    InternalError = -9,
    /// Syscall not implemented
    NotImplemented = -10,
    /// Quota exceeded
    QuotaExceeded = -11,
    /// Invalid capability
    InvalidCapability = -12,
}

/// Syscall handler trait
#[async_trait::async_trait]
pub trait SyscallHandler: Send + Sync {
    /// Handle the syscall
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult;
}

use std::sync::Arc;

use crate::memory::ProcessMemorySpace;

/// Syscall context for handlers
#[derive(Debug, Clone)]
pub struct SyscallContext {
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
    /// Process memory space for validation
    pub memory_space: Option<Arc<ProcessMemorySpace>>,
}

/// Syscall dispatcher
pub struct SyscallDispatcher {
    handlers: HashMap<SyscallNumber, Box<dyn SyscallHandler>>,
}

impl SyscallDispatcher {
    /// Create new dispatcher
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    /// Register handler
    pub fn register(&mut self, num: SyscallNumber, handler: Box<dyn SyscallHandler>) {
        self.handlers.insert(num, handler);
    }

    /// Dispatch syscall with full context
    pub async fn dispatch_with_context(
        &self,
        num: u64,
        args: SyscallArgs,
        ctx: SyscallContext,
    ) -> SyscallResult {
        let syscall_num = match SyscallNumber::from_u64(num) {
            Some(n) => n,
            None => return SyscallResult::Error(SyscallError::InvalidSyscall),
        };

        // Check capability level
        let required_level = self.required_capability(syscall_num);
        if ctx.capability_level < required_level {
            return SyscallResult::Error(SyscallError::PermissionDenied);
        }

        match self.handlers.get(&syscall_num) {
            Some(handler) => handler.handle(args, &ctx).await,
            None => SyscallResult::Error(SyscallError::NotImplemented),
        }
    }

    /// Dispatch syscall (legacy compatibility)
    ///
    /// Note: This method creates a minimal context without memory isolation.
    /// For production use, use `dispatch_with_context` with proper memory
    /// space.
    pub async fn dispatch(
        &self,
        num: u64,
        args: SyscallArgs,
        caller: crate::AgentId,
    ) -> SyscallResult {
        let ctx = SyscallContext {
            caller_id: caller.to_string(),
            process_id: 0,       // Unknown process
            capability_level: 0, // Would look up from registry
            workspace_id: String::new(),
            session_id: String::new(),
            memory_space: None, // No memory isolation
        };
        self.dispatch_with_context(num, args, ctx).await
    }

    /// Get required capability for syscall
    fn required_capability(&self, num: SyscallNumber) -> u8 {
        use crate::capabilities::CapabilityLevel::*;
        match num {
            // L0: Basic operations
            SyscallNumber::QueryMemory | SyscallNumber::LogEvent | SyscallNumber::QueryMetrics => {
                L0LocalCompute as u8
            }

            // L1: File read operations
            SyscallNumber::ReadFile | SyscallNumber::ListFiles | SyscallNumber::QueryState => {
                L1FileRead as u8
            }

            // L2: File write operations
            SyscallNumber::WriteFile
            | SyscallNumber::UpdateState
            | SyscallNumber::EmitMetric
            | SyscallNumber::CreateWorkspace
            | SyscallNumber::DeleteWorkspace => L2FileWrite as u8,

            // L3: Network outbound
            SyscallNumber::SendMessage
            | SyscallNumber::AccessResource
            | SyscallNumber::RequestAttestation
            | SyscallNumber::VerifyAttestation => L3NetworkOut as u8,

            // L4: Network inbound
            SyscallNumber::QuerySchedule
            | SyscallNumber::ScheduleTask
            | SyscallNumber::CancelTask => L4NetworkIn as u8,

            // L5: Spawn limited
            SyscallNumber::SpawnAgent | SyscallNumber::TerminateAgent => L5SpawnLimited as u8,

            // L6: Sandbox control (requires SpawnUnlimited)
            SyscallNumber::EnterSandbox | SyscallNumber::ExitSandbox => L6SpawnUnlimited as u8,

            // L7: Chain read
            SyscallNumber::QueryBalance => L7ChainRead as u8,

            // L8: Chain write (low value)
            SyscallNumber::ExecutePayment
            | SyscallNumber::BridgeToken
            | SyscallNumber::SwapToken
            | SyscallNumber::StakeToken
            | SyscallNumber::UnstakeToken => L8ChainWriteLow as u8,

            // L9: Chain write (high value) + Capability updates
            SyscallNumber::UpdateCapability => L9ChainWriteHigh as u8,
        }
    }
}

impl Default for SyscallDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Capability token
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityToken {
    /// Capability level
    pub level: u8,
    /// Permissions
    pub permissions: Vec<String>,
}

impl CapabilityToken {
    /// Create a new capability token with the given level
    pub fn new(level: u8) -> Self {
        Self {
            level,
            permissions: vec![],
        }
    }

    /// Add a permission to this token (builder pattern)
    pub fn with_permission(mut self, perm: impl Into<String>) -> Self {
        self.permissions.push(perm.into());
        self
    }

    /// Check if this token has the specified permission
    pub fn has_permission(&self, perm: &str) -> bool {
        self.permissions.contains(&perm.to_string()) || self.permissions.contains(&"*".to_string())
    }
}
