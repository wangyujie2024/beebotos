//! Error Integration Module
//!
//! 🔧 FIX: Integration between AgentError and BeeBotOSError for unified error
//! handling.
//!
//! This module provides conversions between local AgentError and the unified
//! BeeBotOSError from beebotos_core.
//!
//! NOTE: The From implementations are in error.rs to avoid conflicts.
//! This module provides extension traits and helper functions.

use beebotos_core::{BeeBotOSError, ErrorContext};

use crate::error::AgentError;

/// 🔧 FIX: Extension trait for AgentError to add BeeBotOSError features
pub trait AgentErrorExt {
    /// Convert to BeeBotOSError with additional context
    fn to_beebotos_error_with_context(self, operation: &str, resource: &str) -> BeeBotOSError;

    /// Get suggested HTTP status code
    fn http_status(&self) -> u16;

    /// Check if error is retryable
    fn is_retryable(&self) -> bool;
}

impl AgentErrorExt for AgentError {
    fn to_beebotos_error_with_context(self, operation: &str, resource: &str) -> BeeBotOSError {
        let base: BeeBotOSError = self.into();
        base.with_context(
            ErrorContext::new()
                .with_operation(operation)
                .with_resource(resource),
        )
    }

    fn http_status(&self) -> u16 {
        match self {
            AgentError::NotFound(_) => 404,
            AgentError::AgentNotFound(_) => 404,
            AgentError::InvalidConfig(_) => 400,
            AgentError::Execution(_) => 500,
            AgentError::CommunicationFailed(_) => 502,
            AgentError::Platform(_) => 502,
            AgentError::Database(_) => 500,
            AgentError::Serialization(_) => 400,
            AgentError::MCPError(_) => 500,
            AgentError::TimeoutMsg(_) => 504,
            AgentError::Timeout(_) => 504,
            AgentError::RateLimited(_) => 429,
            AgentError::Wallet(_) => 400,
            AgentError::AgentExists(_) => 409,
            AgentError::CapabilityDenied(_) => 403,
            _ => 500,
        }
    }

    fn is_retryable(&self) -> bool {
        matches!(
            self,
            AgentError::CommunicationFailed(_)
                | AgentError::Platform(_)
                | AgentError::TimeoutMsg(_)
                | AgentError::Timeout(_)
                | AgentError::RateLimited(_)
                | AgentError::Database(_)
        )
    }
}

/// 🔧 FIX: Result type alias using BeeBotOSError
pub type UnifiedResult<T> = std::result::Result<T, BeeBotOSError>;

/// 🔧 FIX: Helper macro for creating unified errors
#[macro_export]
macro_rules! unified_err {
    ($code:expr, $msg:expr) => {
        BeeBotOSError::new($code, $msg)
    };
    ($code:expr, $fmt:expr, $($arg:tt)*) => {
        BeeBotOSError::new($code, format!($fmt, $($arg)*))
    };
}

/// 🔧 FIX: Helper macro for unified bail
#[macro_export]
macro_rules! unified_bail {
    ($code:expr, $msg:expr) => {
        return Err($crate::unified_err!($code, $msg));
    };
    ($code:expr, $fmt:expr, $($arg:tt)*) => {
        return Err($crate::unified_err!($code, $fmt, $($arg)*));
    };
}

#[cfg(test)]
mod tests {
    use beebotos_core::ErrorCode;

    use super::*;

    #[test]
    fn test_error_conversion() {
        let agent_err = AgentError::AgentNotFound("test agent".to_string());
        let beebotos_err: BeeBotOSError = agent_err.into();

        assert!(matches!(beebotos_err.code, ErrorCode::NotFound));
        // BeeBotOSError::not_found formats message as "resource 'id' not found"
        assert_eq!(beebotos_err.message, "agent 'test agent' not found");
    }

    #[test]
    fn test_http_status() {
        let err = AgentError::NotFound("test".to_string());
        assert_eq!(err.http_status(), 404);

        let err = AgentError::RateLimited("test".to_string());
        assert_eq!(err.http_status(), 429);
    }

    #[test]
    fn test_is_retryable() {
        let err = AgentError::Timeout("test".to_string());
        assert!(err.is_retryable());

        let err = AgentError::NotFound("test".to_string());
        assert!(!err.is_retryable());
    }
}
