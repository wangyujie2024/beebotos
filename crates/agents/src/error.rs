//! Agent error types
//!
//! 🟡 P1 FIX: Unified error types using beebotos_core::Error

/// 🟡 P1 FIX: Re-export core error for unified error handling
pub use beebotos_core::Error as CoreError;
pub use beebotos_core::{Error as BeeBotOSError, ErrorCode};
use thiserror::Error;

/// Agent result type - now uses core::Error for consistency
pub type Result<T> = std::result::Result<T, AgentError>;

/// 🟡 P1 FIX: Result type alias using core::Error
pub type CoreResult<T> = beebotos_core::Result<T>;

/// Agent errors
#[derive(Error, Debug, Clone)]
pub enum AgentError {
    #[error("Agent not found: {0}")]
    AgentNotFound(String),

    #[error("Agent already exists: {0}")]
    AgentExists(String),

    #[error("Invalid agent configuration: {0}")]
    InvalidConfig(String),

    #[error("Execution error: {0}")]
    Execution(String),

    #[error("WASM error: {0}")]
    Wasm(String),

    #[error("A2A communication error: {0}")]
    A2A(String),

    #[error("Skill not found: {0}")]
    SkillNotFound(String),

    #[error("Capability denied: {0}")]
    CapabilityDenied(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Platform error: {0}")]
    Platform(String),

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("Task execution failed: {0}")]
    TaskExecutionFailed(String),

    #[error("Communication failed: {0}")]
    CommunicationFailed(String),

    #[error("MCP error: {0}")]
    MCPError(String),

    #[error("Not configured: {0}")]
    NotConfigured(String),

    #[error("Unsupported task type: {0}")]
    UnsupportedTaskType(String),

    /// 🟢 P1 FIX: Service Mesh error
    #[error("Service mesh error: {0}")]
    ServiceMesh(String),

    /// 🟢 P1 FIX: DID resolution error
    #[error("DID resolution error: {0}")]
    DIDResolution(String),

    /// Authentication failed
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    /// Message receive failed
    #[error("Message receive failed: {0}")]
    MessageReceiveFailed(String),

    /// Message send failed
    #[error("Message send failed: {0}")]
    MessageSendFailed(String),

    /// Not connected
    #[error("Not connected: {0}")]
    NotConnected(String),

    /// Rate limited
    #[error("Rate limited: {0}")]
    RateLimited(String),

    /// Database error
    #[error("Database error: {0}")]
    Database(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Resource limit exceeded
    #[error("Resource limit exceeded: {0}")]
    ResourceLimit(String),

    /// 🔧 FIX: Not found error (alias for AgentNotFound for compatibility)
    #[error("Not found: {0}")]
    NotFound(String),

    /// 🆕 PLANNING FIX: Planning error
    #[error("Planning error: {0}")]
    Planning(String),

    /// 🔧 FIX: Wallet error
    #[error("Wallet error: {0}")]
    Wallet(String),

    /// 🔧 FIX: Timeout with message
    #[error("Timeout: {0}")]
    TimeoutMsg(String),
}

impl AgentError {
    /// Create a new platform error
    pub fn platform<S: Into<String>>(msg: S) -> Self {
        Self::Platform(msg.into())
    }

    /// Create a new authentication error
    pub fn authentication<S: Into<String>>(msg: S) -> Self {
        Self::Authentication(msg.into())
    }

    /// Create a new not found error
    pub fn not_found<S: Into<String>>(msg: S) -> Self {
        Self::AgentNotFound(msg.into())
    }

    /// Create a new configuration error
    pub fn configuration<S: Into<String>>(msg: S) -> Self {
        Self::InvalidConfig(msg.into())
    }

    /// Create a new communication error
    pub fn communication<S: Into<String>>(msg: S) -> Self {
        Self::A2A(msg.into())
    }

    /// Create a new authentication failed error
    pub fn authentication_failed<S: Into<String>>(msg: S) -> Self {
        Self::AuthenticationFailed(msg.into())
    }

    /// Create a new message receive failed error
    pub fn message_receive_failed<S: Into<String>>(msg: S) -> Self {
        Self::MessageReceiveFailed(msg.into())
    }

    /// Create a new message send failed error
    pub fn message_send_failed<S: Into<String>>(msg: S) -> Self {
        Self::MessageSendFailed(msg.into())
    }

    /// Create a new storage error
    pub fn storage<S: Into<String>>(msg: S) -> Self {
        Self::Database(msg.into())
    }

    /// Create a new serialization error  
    pub fn serialization<S: Into<String>>(msg: S) -> Self {
        Self::Serialization(msg.into())
    }

    /// Create a new IO error
    pub fn io<S: Into<String>>(msg: S) -> Self {
        Self::Platform(format!("IO error: {}", msg.into()))
    }

    /// 🔧 P0 FIX: Create an invalid input error
    pub fn invalid_input<S: Into<String>>(msg: S) -> Self {
        Self::InvalidConfig(format!("Invalid input: {}", msg.into()))
    }

    /// 🟡 P1 FIX: Convert to core::Error for unified error handling
    pub fn to_core_error(&self) -> CoreError {
        use beebotos_core::ErrorCode;
        match self {
            AgentError::AgentNotFound(msg) => BeeBotOSError::not_found("agent", msg),
            AgentError::AgentExists(msg) => {
                BeeBotOSError::new(ErrorCode::AlreadyExists, format!("Agent exists: {}", msg))
            }
            AgentError::InvalidConfig(msg) => CoreError::configuration(msg.clone()),
            AgentError::Execution(msg) => BeeBotOSError::new(ErrorCode::Agent, msg.clone()),
            AgentError::Wasm(msg) => BeeBotOSError::new(ErrorCode::Agent, format!("WASM: {}", msg)),
            AgentError::A2A(msg) => BeeBotOSError::new(ErrorCode::Network, format!("A2A: {}", msg)),
            AgentError::SkillNotFound(msg) => BeeBotOSError::not_found("skill", msg),
            AgentError::CapabilityDenied(msg) => {
                CoreError::authorization(format!("Capability: {}", msg))
            }
            AgentError::Timeout(_) => CoreError::timeout("Agent operation"),
            AgentError::Internal(msg) => BeeBotOSError::new(ErrorCode::Unknown, msg.clone()),
            AgentError::Platform(msg) => CoreError::agent(msg.clone()),
            AgentError::Authentication(msg) => CoreError::authentication(format!("Auth: {}", msg)),
            AgentError::TaskExecutionFailed(msg) => CoreError::agent(format!("Task: {}", msg)),
            AgentError::CommunicationFailed(msg) => {
                BeeBotOSError::new(ErrorCode::Network, format!("Comm: {}", msg))
            }
            AgentError::MCPError(msg) => CoreError::agent(format!("MCP: {}", msg)),
            AgentError::NotConfigured(msg) => {
                CoreError::configuration(format!("Not configured: {}", msg))
            }
            AgentError::UnsupportedTaskType(msg) => {
                CoreError::validation(format!("Task type: {}", msg))
            }
            AgentError::ServiceMesh(msg) => CoreError::agent(format!("Service mesh: {}", msg)),
            AgentError::DIDResolution(msg) => {
                CoreError::authentication(format!("DID resolution: {}", msg))
            }
            AgentError::AuthenticationFailed(msg) => {
                CoreError::authentication(format!("Auth failed: {}", msg))
            }
            AgentError::MessageReceiveFailed(msg) => {
                BeeBotOSError::new(ErrorCode::Network, format!("Receive failed: {}", msg))
            }
            AgentError::MessageSendFailed(msg) => {
                BeeBotOSError::new(ErrorCode::Network, format!("Send failed: {}", msg))
            }
            AgentError::NotConnected(msg) => {
                BeeBotOSError::new(ErrorCode::Network, format!("Not connected: {}", msg))
            }
            AgentError::RateLimited(msg) => {
                BeeBotOSError::new(ErrorCode::RateLimited, format!("Rate limited: {}", msg))
            }
            AgentError::Database(msg) => CoreError::database(msg.clone()),
            AgentError::Serialization(msg) => {
                BeeBotOSError::new(ErrorCode::Serialization, format!("Serialization: {}", msg))
            }
            AgentError::ResourceLimit(msg) => {
                BeeBotOSError::new(ErrorCode::Constraint, format!("Resource limit: {}", msg))
            }
            AgentError::NotFound(msg) => BeeBotOSError::not_found("resource", msg),
            AgentError::Planning(msg) => {
                BeeBotOSError::new(ErrorCode::Agent, format!("Planning: {}", msg))
            }
            AgentError::Wallet(msg) => {
                BeeBotOSError::new(ErrorCode::Configuration, format!("Wallet error: {}", msg))
            }
            AgentError::TimeoutMsg(msg) => CoreError::timeout(msg),
        }
    }
}

/// 🟡 P1 FIX: Convert AgentError to CoreError automatically
impl From<AgentError> for CoreError {
    fn from(err: AgentError) -> Self {
        err.to_core_error()
    }
}

/// 🟡 P1 FIX: Convert core::Error to AgentError where appropriate
impl From<CoreError> for AgentError {
    fn from(err: CoreError) -> Self {
        use beebotos_core::ErrorCode;
        match err.code {
            ErrorCode::NotFound => AgentError::AgentNotFound(err.message),
            ErrorCode::AlreadyExists => AgentError::AgentExists(err.message),
            ErrorCode::Configuration => AgentError::InvalidConfig(err.message),
            ErrorCode::Agent => AgentError::Platform(err.message),
            ErrorCode::Task => AgentError::Execution(err.message),
            ErrorCode::Timeout => AgentError::Timeout("Operation timed out".to_string()),
            ErrorCode::Database => AgentError::Execution(format!("Database: {}", err.message)),
            ErrorCode::Network => AgentError::CommunicationFailed(err.message),
            ErrorCode::InvalidInput | ErrorCode::Schema | ErrorCode::Constraint => {
                AgentError::InvalidConfig(err.message)
            }
            ErrorCode::Authentication | ErrorCode::Authorization | ErrorCode::PermissionDenied => {
                AgentError::Authentication(err.message)
            }
            ErrorCode::Io => AgentError::Execution(format!("IO: {}", err.message)),
            ErrorCode::Serialization => {
                AgentError::Execution(format!("Serialization: {}", err.message))
            }
            ErrorCode::Contract
            | ErrorCode::Transaction
            | ErrorCode::Wallet
            | ErrorCode::InsufficientFunds => {
                AgentError::Execution(format!("Blockchain: {}", err.message))
            }
            _ => AgentError::Internal(format!("Unknown: {}", err.message)),
        }
    }
}

impl From<serde_json::Error> for AgentError {
    fn from(err: serde_json::Error) -> Self {
        AgentError::Execution(format!("JSON error: {}", err))
    }
}

impl From<std::io::Error> for AgentError {
    fn from(err: std::io::Error) -> Self {
        AgentError::Execution(format!("IO error: {}", err))
    }
}
