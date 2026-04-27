//! Error Handling
//!
//! Re-exports from gateway-lib and minimal app-specific error types.
//!
//! For most error handling, use gateway::error::GatewayError directly.

pub use gateway::error::GatewayError;

/// Result type alias using GatewayError
#[allow(dead_code)]
pub type Result<T> = std::result::Result<T, GatewayError>;

/// Application-specific errors
///
/// These are internal errors that get converted to GatewayError for HTTP
/// responses.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// Database error
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Not found
    #[error("{0} not found")]
    NotFound(String),

    /// Kernel error
    #[error("Kernel error: {0}")]
    Kernel(String),

    /// Agent error
    #[error("Agent error: {0}")]
    Agent(#[from] beebotos_agents::error::AgentError),

    /// Chain/Blockchain error
    #[error("Chain error: {0}")]
    Chain(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Not implemented
    #[error("Not implemented: {0}")]
    NotImplemented(String),

    /// Validation error
    #[error("Validation error")]
    Validation(Vec<ValidationError>),

    /// Unauthorized
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Validation error details
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
    pub code: String,
}

impl AppError {
    /// Create a database error
    pub fn database(e: sqlx::Error) -> Self {
        Self::Database(e)
    }

    /// Create a not found error
    pub fn not_found(resource: &str, id: &str) -> Self {
        Self::NotFound(format!("{}: {}", resource, id))
    }

    /// Create a kernel error
    pub fn kernel(msg: impl Into<String>) -> Self {
        Self::Kernel(msg.into())
    }

    /// Create a chain error
    pub fn chain(msg: impl Into<String>) -> Self {
        Self::Chain(msg.into())
    }

    /// Create a validation error
    pub fn validation(errors: Vec<ValidationError>) -> Self {
        Self::Validation(errors)
    }
}

impl From<AppError> for GatewayError {
    fn from(err: AppError) -> Self {
        match err {
            AppError::Database(e) => GatewayError::internal(format!("Database error: {}", e)),
            AppError::NotFound(msg) => GatewayError::not_found("resource", &msg),
            AppError::Kernel(msg) => GatewayError::internal(format!("Kernel error: {}", msg)),
            AppError::Agent(agent_err) => convert_agent_error(agent_err),
            AppError::Chain(msg) => GatewayError::internal(format!("Chain error: {}", msg)),
            AppError::Configuration(msg) => GatewayError::service_unavailable("Configuration", msg),
            AppError::NotImplemented(msg) => GatewayError::bad_request(msg),
            AppError::Validation(errors) => GatewayError::validation(
                errors
                    .into_iter()
                    .map(|e| gateway::error::ValidationError {
                        field: e.field,
                        message: e.message,
                        code: e.code,
                    })
                    .collect(),
            ),
            AppError::Unauthorized(msg) => GatewayError::Unauthorized {
                message: msg,
                code: "AUTH_FAILED".to_string(),
            },
            AppError::Internal(msg) => GatewayError::internal(msg),
        }
    }
}

/// 🟢 P1 FIX: Direct conversion from AgentError to GatewayError
///
/// Convert AgentError to GatewayError
///
/// This function provides detailed mapping from AgentError to appropriate
/// GatewayError
///
/// 🟢 P1 FIX: Comprehensive error conversion covering all AgentError variants
/// with appropriate HTTP status codes and user-facing messages.
///
/// Usage:
/// ```rust
/// let result = agent_runtime_manager
///     .execute_task(&id, task)
///     .await
///     .map_err(convert_agent_error)?;
/// ```

/// Convert AgentError to GatewayError
/// This function provides detailed mapping from AgentError to appropriate
/// GatewayError
///
/// 🟢 P1 FIX: Comprehensive error conversion covering all AgentError variants
/// with appropriate HTTP status codes and user-facing messages.
pub fn convert_agent_error(err: beebotos_agents::error::AgentError) -> GatewayError {
    use beebotos_agents::error::AgentError;

    // Log the original error for debugging (with correlation ID)
    let correlation_id = uuid::Uuid::new_v4().to_string();
    tracing::debug!(
        correlation_id = %correlation_id,
        agent_error = %err,
        "Converting AgentError to GatewayError"
    );

    match err {
        // 4xx Client Errors
        AgentError::AgentNotFound(msg) => GatewayError::not_found("Agent", msg),
        AgentError::SkillNotFound(msg) => GatewayError::not_found("Skill", msg),
        AgentError::InvalidConfig(msg) => {
            GatewayError::bad_request(format!("Invalid configuration: {}", msg))
        }
        AgentError::NotConfigured(msg) => {
            GatewayError::bad_request(format!("Not configured: {}", msg))
        }
        AgentError::UnsupportedTaskType(msg) => {
            GatewayError::bad_request(format!("Unsupported task type: {}", msg))
        }
        AgentError::AgentExists(msg) => GatewayError::Validation {
            errors: vec![gateway::error::ValidationError {
                field: "agent".to_string(),
                message: format!("Agent already exists: {}", msg),
                code: "ALREADY_EXISTS".to_string(),
            }],
        },

        // 401 Unauthorized
        AgentError::Authentication(msg) | AgentError::AuthenticationFailed(msg) => {
            GatewayError::Unauthorized {
                message: msg,
                code: "AGENT_AUTH_FAILED".to_string(),
            }
        }

        // 403 Forbidden
        AgentError::CapabilityDenied(msg) => {
            GatewayError::forbidden(format!("Capability denied: {}", msg))
        }

        // 429 Rate Limited
        AgentError::RateLimited(msg) => GatewayError::rate_limited(Some(extract_retry_after(&msg))),

        // 503 Service Unavailable
        AgentError::NotConnected(msg) => GatewayError::service_unavailable("Agent", msg),

        // 500 Internal Server Errors
        AgentError::Timeout(_) => GatewayError::timeout("Agent operation", 30),
        AgentError::Platform(msg) => GatewayError::internal(format!("Platform error: {}", msg)),
        AgentError::Execution(msg) => GatewayError::internal(format!("Execution error: {}", msg)),
        AgentError::TaskExecutionFailed(msg) => GatewayError::Internal {
            message: format!("Task execution failed: {}", msg),
            correlation_id,
        },
        AgentError::A2A(msg) => GatewayError::internal(format!("A2A communication error: {}", msg)),
        AgentError::Wasm(msg) => GatewayError::internal(format!("WASM execution error: {}", msg)),
        AgentError::MCPError(msg) => GatewayError::internal(format!("MCP tool error: {}", msg)),
        AgentError::ServiceMesh(msg) => {
            GatewayError::internal(format!("Service mesh error: {}", msg))
        }
        AgentError::DIDResolution(msg) => {
            GatewayError::internal(format!("DID resolution error: {}", msg))
        }
        AgentError::CommunicationFailed(msg) => {
            GatewayError::internal(format!("Communication failed: {}", msg))
        }
        AgentError::Planning(msg) => GatewayError::internal(format!("Planning error: {}", msg)),
        AgentError::MessageReceiveFailed(msg) => {
            GatewayError::internal(format!("Message receive failed: {}", msg))
        }
        AgentError::MessageSendFailed(msg) => {
            GatewayError::internal(format!("Message send failed: {}", msg))
        }
        AgentError::Internal(msg) => GatewayError::internal(msg),
        AgentError::Database(msg) => GatewayError::internal(format!("Database error: {}", msg)),
        AgentError::Serialization(msg) => {
            GatewayError::internal(format!("Serialization error: {}", msg))
        }
        AgentError::ResourceLimit(msg) => {
            GatewayError::internal(format!("Resource limit: {}", msg))
        }
        AgentError::NotFound(msg) => GatewayError::not_found("resource", msg),
        AgentError::Wallet(msg) => GatewayError::internal(format!("Wallet error: {}", msg)),
        AgentError::TimeoutMsg(_msg) => GatewayError::timeout("Agent operation", 30),
    }
}

/// Extract retry after seconds from rate limit message
fn extract_retry_after(msg: &str) -> u64 {
    // Try to extract number from message like "Rate limited: retry after 60
    // seconds"
    msg.chars()
        .filter(|c| c.is_numeric())
        .collect::<String>()
        .parse::<u64>()
        .unwrap_or(60) // Default 60 seconds
}

/// Helper macros for early returns
#[macro_export]
macro_rules! bail {
    ($err:expr) => {
        return Err($err.into());
    };
}

#[macro_export]
macro_rules! not_found {
    ($resource:expr, $id:expr) => {
        return Err($crate::error::GatewayError::not_found($resource, $id));
    };
}
