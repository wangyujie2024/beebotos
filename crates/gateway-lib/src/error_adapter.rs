//! Error Adapter
//!
//! Provides conversion between GatewayError and BeeBotOSError (unified error
//! type). This enables gradual migration from GatewayError to BeeBotOSError.

use beebotos_core::{BeeBotOSError, ErrorCode, ErrorContext, Severity};

use crate::error::GatewayError;

/// Convert GatewayError to BeeBotOSError
impl From<GatewayError> for BeeBotOSError {
    fn from(err: GatewayError) -> Self {
        match err {
            GatewayError::Unauthorized { message, .. } => {
                BeeBotOSError::authentication(message).with_severity(Severity::Warning)
            }
            GatewayError::Forbidden { message, .. } => {
                BeeBotOSError::authorization(message).with_severity(Severity::Warning)
            }
            GatewayError::NotFound { resource, id } => {
                BeeBotOSError::not_found(resource, id).with_severity(Severity::Info)
            }
            GatewayError::RateLimited { .. } => {
                BeeBotOSError::new(ErrorCode::RateLimited, "Rate limit exceeded")
            }
            GatewayError::BadRequest { message, .. } => {
                BeeBotOSError::validation(message).with_severity(Severity::Warning)
            }
            GatewayError::Validation { errors } => BeeBotOSError::new(
                ErrorCode::Schema,
                format!("Validation failed: {} errors", errors.len()),
            )
            .with_details(format!("{:?}", errors)),
            GatewayError::Internal {
                message,
                correlation_id,
            } => BeeBotOSError::new(ErrorCode::Unknown, message)
                .with_details(format!("Correlation ID: {}", correlation_id))
                .with_severity(Severity::Error),
            GatewayError::ServiceUnavailable { service, reason } => {
                BeeBotOSError::upstream(service, format!("Service unavailable: {}", reason))
            }
            GatewayError::Timeout {
                upstream,
                timeout_secs,
            } => BeeBotOSError::new(
                ErrorCode::Timeout,
                format!("Request to {} timed out after {}s", upstream, timeout_secs),
            ),
            GatewayError::Config { message } => BeeBotOSError::configuration(message),
            GatewayError::Upstream {
                service,
                status,
                message,
            } => BeeBotOSError::new(
                ErrorCode::Upstream,
                format!("Upstream '{}' returned {}: {}", service, status, message),
            ),
            GatewayError::Agent { message } => BeeBotOSError::agent(message),
            GatewayError::State { message } => BeeBotOSError::database(message)
                .with_context(ErrorContext::new().with_operation("state_management")),
        }
    }
}

/// Convert BeeBotOSError to GatewayError
impl From<BeeBotOSError> for GatewayError {
    fn from(err: BeeBotOSError) -> Self {
        use beebotos_core::ErrorCode;

        match err.code {
            ErrorCode::Configuration => GatewayError::Config {
                message: err.message,
            },
            ErrorCode::Database => GatewayError::State {
                message: err.message,
            },
            ErrorCode::Network => GatewayError::ServiceUnavailable {
                service: "network".to_string(),
                reason: err.message,
            },
            ErrorCode::NotFound => {
                if let Some(ref ctx) = err.context.resource {
                    GatewayError::NotFound {
                        resource: ctx.clone(),
                        id: err.context.resource_id.clone().unwrap_or_default(),
                    }
                } else {
                    GatewayError::not_found("resource", "unknown")
                }
            }
            ErrorCode::AlreadyExists => GatewayError::bad_request(format!(
                "{}: {}",
                err.context.resource.as_deref().unwrap_or("Resource"),
                err.message
            )),
            ErrorCode::Agent => GatewayError::Agent {
                message: err.message,
            },
            ErrorCode::Task => GatewayError::bad_request(err.message),
            ErrorCode::Kernel => GatewayError::ServiceUnavailable {
                service: "kernel".to_string(),
                reason: err.message,
            },
            ErrorCode::Timeout => GatewayError::Timeout {
                upstream: err
                    .context
                    .operation
                    .unwrap_or_else(|| "unknown".to_string()),
                timeout_secs: 30,
            },
            ErrorCode::InvalidInput | ErrorCode::Schema | ErrorCode::Constraint => {
                GatewayError::BadRequest {
                    message: err.message,
                    field: None,
                }
            }
            ErrorCode::Authentication => GatewayError::unauthorized(err.message),
            ErrorCode::Authorization | ErrorCode::PermissionDenied => GatewayError::Forbidden {
                message: err.message,
                resource: err.context.resource.unwrap_or_default(),
            },
            ErrorCode::Transaction | ErrorCode::Contract | ErrorCode::Wallet => {
                GatewayError::Upstream {
                    service: "blockchain".to_string(),
                    status: 400,
                    message: err.message,
                }
            }
            ErrorCode::InsufficientFunds => GatewayError::Upstream {
                service: "blockchain".to_string(),
                status: 402,
                message: err.message,
            },
            ErrorCode::Upstream | ErrorCode::RateLimited | ErrorCode::Unavailable => {
                GatewayError::ServiceUnavailable {
                    service: err
                        .context
                        .resource
                        .unwrap_or_else(|| "upstream".to_string()),
                    reason: err.message,
                }
            }
            _ => GatewayError::Internal {
                message: err.message,
                correlation_id: uuid::Uuid::new_v4().to_string(),
            },
        }
    }
}

/// Extension trait for BeeBotOSError to add HTTP status
pub trait BeeBotOSErrorExt {
    /// Get HTTP status code
    fn http_status(&self) -> u16;

    /// Check if should be logged
    fn should_log(&self) -> bool;

    /// Get log level
    fn log_level(&self) -> &'static str;
}

impl BeeBotOSErrorExt for BeeBotOSError {
    fn http_status(&self) -> u16 {
        self.code.http_status()
    }

    fn should_log(&self) -> bool {
        !matches!(self.severity, Severity::Debug)
    }

    fn log_level(&self) -> &'static str {
        match self.severity {
            Severity::Debug => "debug",
            Severity::Info => "info",
            Severity::Warning => "warn",
            Severity::Error | Severity::Critical => "error",
        }
    }
}

/// Result type that can hold either error type
pub type UnifiedResult<T> = std::result::Result<T, BeeBotOSError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_to_beebotos() {
        let gateway_err = GatewayError::not_found("agent", "123");
        let beebotos_err: BeeBotOSError = gateway_err.into();

        assert_eq!(beebotos_err.code, ErrorCode::NotFound);
        assert!(beebotos_err.message.contains("agent"));
        assert!(beebotos_err.message.contains("123"));
    }

    #[test]
    fn test_beebotos_to_gateway() {
        let beebotos_err = BeeBotOSError::authentication("Invalid token");
        let gateway_err: GatewayError = beebotos_err.into();

        match gateway_err {
            GatewayError::Unauthorized { message, .. } => {
                assert_eq!(message, "Invalid token");
            }
            _ => panic!("Expected Unauthorized error"),
        }
    }

    #[test]
    fn test_roundtrip() {
        let original = GatewayError::Agent {
            message: "Test".to_string(),
        };
        let converted: BeeBotOSError = original.clone().into();
        let back: GatewayError = converted.into();

        match back {
            GatewayError::Agent { message } => assert_eq!(message, "Test"),
            _ => panic!("Expected Agent error"),
        }
    }

    #[test]
    fn test_http_status() {
        let err = BeeBotOSError::not_found("x", "y");
        assert_eq!(err.http_status(), 404);

        let err = BeeBotOSError::authentication("fail");
        assert_eq!(err.http_status(), 401);
    }
}
