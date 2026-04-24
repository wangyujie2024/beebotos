//! Error Handling
//!
//! Production-ready error types with:
//! - HTTP status code mapping
//! - Structured error responses
//! - Error tracing/logging integration
//! - Generic error conversion support

use std::fmt;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, error, warn};

/// Gateway error types
#[derive(Debug, Clone, PartialEq)]
pub enum GatewayError {
    /// Authentication error (401)
    Unauthorized {
        /// Error message
        message: String,
        /// Error code
        code: String,
    },

    /// Authorization error (403)
    Forbidden {
        /// Error message
        message: String,
        /// Resource that was accessed
        resource: String,
    },

    /// Resource not found (404)
    NotFound {
        /// Resource type
        resource: String,
        /// Resource identifier
        id: String,
    },

    /// Rate limit exceeded (429)
    RateLimited {
        /// Seconds until retry is allowed
        retry_after: Option<u64>,
    },

    /// Bad request (400)
    BadRequest {
        /// Error message
        message: String,
        /// Field that caused the error
        field: Option<String>,
    },

    /// Validation error (422)
    Validation {
        /// List of validation errors
        errors: Vec<ValidationError>,
    },

    /// Internal server error (500)
    Internal {
        /// Error message
        message: String,
        /// Correlation ID for tracing
        correlation_id: String,
    },

    /// Service unavailable (503)
    ServiceUnavailable {
        /// Service name
        service: String,
        /// Reason for unavailability
        reason: String,
    },

    /// Gateway timeout (504)
    Timeout {
        /// Upstream service name
        upstream: String,
        /// Timeout duration in seconds
        timeout_secs: u64,
    },

    /// Configuration error
    Config {
        /// Error message
        message: String,
    },

    /// Upstream service error
    Upstream {
        /// Service name
        service: String,
        /// HTTP status code
        status: u16,
        /// Error message
        message: String,
    },

    /// Agent runtime error
    Agent {
        /// Error message
        message: String,
    },

    /// State management error
    State {
        /// Error message
        message: String,
    },
}

/// Validation error detail
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidationError {
    /// Field name that failed validation
    pub field: String,
    /// Validation error message
    pub message: String,
    /// Error code
    pub code: String,
}

/// Error response structure for JSON serialization
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Whether the request was successful
    pub success: bool,
    /// Error details
    pub error: ErrorDetail,
}

/// Error detail structure
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorDetail {
    /// Error code
    pub code: String,
    /// Error message
    pub message: String,
    /// Additional error details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    /// Correlation ID for tracing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    /// Seconds to wait before retry
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<u64>,
}

impl GatewayError {
    /// Create unauthorized error
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::Unauthorized {
            message: message.into(),
            code: "UNAUTHORIZED".to_string(),
        }
    }

    /// Create forbidden error
    pub fn forbidden(resource: impl Into<String>) -> Self {
        Self::Forbidden {
            message: "Access denied".to_string(),
            resource: resource.into(),
        }
    }

    /// Create not found error
    pub fn not_found(resource: impl Into<String>, id: impl Into<String>) -> Self {
        Self::NotFound {
            resource: resource.into(),
            id: id.into(),
        }
    }

    /// Create rate limited error
    pub fn rate_limited(retry_after: Option<u64>) -> Self {
        Self::RateLimited { retry_after }
    }

    /// Create bad request error
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest {
            message: message.into(),
            field: None,
        }
    }

    /// Create bad request with field
    pub fn bad_request_field(message: impl Into<String>, field: impl Into<String>) -> Self {
        Self::BadRequest {
            message: message.into(),
            field: Some(field.into()),
        }
    }

    /// Create validation error
    pub fn validation(errors: Vec<ValidationError>) -> Self {
        Self::Validation { errors }
    }

    /// Create internal error
    ///
    /// 🟠 HIGH SECURITY FIX: Logs detailed error internally but returns generic
    /// message to user This prevents sensitive internal details from being
    /// exposed to attackers
    pub fn internal(message: impl Into<String>) -> Self {
        let correlation_id = uuid::Uuid::new_v4().to_string();
        let internal_message = message.into();

        // Log detailed error internally for debugging (contains potentially sensitive
        // info)
        error!(
            correlation_id = %correlation_id,
            error_message = %internal_message,
            "Internal error occurred"
        );

        // Return generic message to user - never expose internal details
        Self::Internal {
            message: "Internal server error".to_string(),
            correlation_id,
        }
    }

    /// Create service unavailable error
    pub fn service_unavailable(service: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::ServiceUnavailable {
            service: service.into(),
            reason: reason.into(),
        }
    }

    /// Create timeout error
    pub fn timeout(upstream: impl Into<String>, timeout_secs: u64) -> Self {
        Self::Timeout {
            upstream: upstream.into(),
            timeout_secs,
        }
    }

    /// Create upstream error
    pub fn upstream(service: impl Into<String>, status: u16, message: impl Into<String>) -> Self {
        Self::Upstream {
            service: service.into(),
            status,
            message: message.into(),
        }
    }

    /// Create agent error
    pub fn agent(message: impl Into<String>) -> Self {
        Self::Agent {
            message: message.into(),
        }
    }

    /// Create state management error
    pub fn state(message: impl Into<String>) -> Self {
        Self::State {
            message: message.into(),
        }
    }

    /// Get HTTP status code
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::Unauthorized { .. } => StatusCode::UNAUTHORIZED,
            Self::Forbidden { .. } => StatusCode::FORBIDDEN,
            Self::NotFound { .. } => StatusCode::NOT_FOUND,
            Self::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::BadRequest { .. } => StatusCode::BAD_REQUEST,
            Self::Validation { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Internal { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            Self::ServiceUnavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Self::Timeout { .. } => StatusCode::GATEWAY_TIMEOUT,
            Self::Config { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Upstream { status, .. } => {
                StatusCode::from_u16(*status).unwrap_or(StatusCode::BAD_GATEWAY)
            }
            Self::Agent { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            Self::State { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Get error code string
    pub fn error_code(&self) -> &str {
        match self {
            Self::Unauthorized { code, .. } => code,
            Self::Forbidden { .. } => "FORBIDDEN",
            Self::NotFound { .. } => "NOT_FOUND",
            Self::RateLimited { .. } => "RATE_LIMITED",
            Self::BadRequest { .. } => "BAD_REQUEST",
            Self::Validation { .. } => "VALIDATION_ERROR",
            Self::Internal { .. } => "INTERNAL_ERROR",
            Self::ServiceUnavailable { .. } => "SERVICE_UNAVAILABLE",
            Self::Timeout { .. } => "GATEWAY_TIMEOUT",
            Self::Config { .. } => "CONFIG_ERROR",
            Self::Upstream { .. } => "UPSTREAM_ERROR",
            Self::Agent { .. } => "AGENT_ERROR",
            Self::State { .. } => "STATE_ERROR",
        }
    }

    /// Get user-facing message
    pub fn user_message(&self) -> String {
        match self {
            Self::Unauthorized { message, .. } => message.clone(),
            Self::Forbidden { message, .. } => message.clone(),
            Self::NotFound { resource, id } => format!("{} '{}' not found", resource, id),
            Self::RateLimited { .. } => "Rate limit exceeded. Please try again later.".to_string(),
            Self::BadRequest { message, .. } => message.clone(),
            Self::Validation { .. } => "Validation failed".to_string(),
            Self::Internal { .. } => {
                "An internal error occurred. Please try again later.".to_string()
            }
            Self::ServiceUnavailable { reason, .. } => format!("Service unavailable: {}", reason),
            Self::Timeout { upstream, .. } => format!("Request to {} timed out", upstream),
            Self::Config { .. } => "Configuration error".to_string(),
            Self::Upstream { message, .. } => format!("Upstream error: {}", message),
            Self::Agent { message } => format!("Agent error: {}", message),
            Self::State { message } => format!("State error: {}", message),
        }
    }

    /// Convert to error response
    pub fn to_response(&self) -> ErrorResponse {
        let details = match self {
            Self::Validation { errors } => Some(json!({"validation_errors": errors})),
            Self::BadRequest { field: Some(f), .. } => Some(json!({"field": f})),
            Self::NotFound { resource, id } => Some(json!({"resource": resource, "id": id})),
            _ => None,
        };

        let correlation_id = match self {
            Self::Internal { correlation_id, .. } => Some(correlation_id.clone()),
            _ => None,
        };

        let retry_after = match self {
            Self::RateLimited { retry_after } => *retry_after,
            _ => None,
        };

        ErrorResponse {
            success: false,
            error: ErrorDetail {
                code: self.error_code().to_string(),
                message: self.user_message(),
                details,
                correlation_id,
                retry_after,
            },
        }
    }
}

impl fmt::Display for GatewayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unauthorized { message, .. } => write!(f, "Unauthorized: {}", message),
            Self::Forbidden { resource, .. } => write!(f, "Forbidden access to {}", resource),
            Self::NotFound { resource, id } => write!(f, "{} '{}' not found", resource, id),
            Self::RateLimited { retry_after } => {
                if let Some(secs) = retry_after {
                    write!(f, "Rate limited, retry after {}s", secs)
                } else {
                    write!(f, "Rate limited")
                }
            }
            Self::BadRequest { message, .. } => write!(f, "Bad request: {}", message),
            Self::Validation { errors } => write!(f, "Validation failed: {} errors", errors.len()),
            Self::Internal {
                message,
                correlation_id,
            } => {
                write!(
                    f,
                    "Internal error: {} (correlation_id: {})",
                    message, correlation_id
                )
            }
            Self::ServiceUnavailable { service, .. } => {
                write!(f, "Service '{}' unavailable", service)
            }
            Self::Timeout {
                upstream,
                timeout_secs,
            } => {
                write!(f, "Timeout after {}s to {}", timeout_secs, upstream)
            }
            Self::Config { message } => write!(f, "Config error: {}", message),
            Self::Upstream {
                service, status, ..
            } => {
                write!(f, "Upstream '{}' returned {}", service, status)
            }
            Self::Agent { message } => write!(f, "Agent error: {}", message),
            Self::State { message } => write!(f, "State error: {}", message),
        }
    }
}

impl std::error::Error for GatewayError {}

impl IntoResponse for GatewayError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let response = self.to_response();

        // Log error appropriately
        match status {
            StatusCode::INTERNAL_SERVER_ERROR => {
                error!(
                    status = %status.as_u16(),
                    error_code = %self.error_code(),
                    "Server error response"
                );
            }
            StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => {
                debug!(
                    status = %status.as_u16(),
                    error_code = %self.error_code(),
                    "Client validation error"
                );
            }
            StatusCode::TOO_MANY_REQUESTS => {
                warn!(
                    status = %status.as_u16(),
                    error_code = %self.error_code(),
                    "Rate limit exceeded"
                );
            }
            _ => {
                debug!(
                    status = %status.as_u16(),
                    error_code = %self.error_code(),
                    "Error response"
                );
            }
        }

        (status, Json(response)).into_response()
    }
}

/// Result type alias
pub type Result<T> = std::result::Result<T, GatewayError>;

// Conversions from other error types

impl From<crate::config::ConfigError> for GatewayError {
    fn from(err: crate::config::ConfigError) -> Self {
        Self::Config {
            message: err.to_string(),
        }
    }
}

impl From<serde_json::Error> for GatewayError {
    fn from(err: serde_json::Error) -> Self {
        Self::BadRequest {
            message: format!("Invalid JSON: {}", err),
            field: None,
        }
    }
}

impl From<std::io::Error> for GatewayError {
    fn from(err: std::io::Error) -> Self {
        Self::Internal {
            message: format!("IO error: {}", err),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        }
    }
}

impl From<validator::ValidationErrors> for GatewayError {
    fn from(errors: validator::ValidationErrors) -> Self {
        let validation_errors: Vec<ValidationError> = errors
            .field_errors()
            .iter()
            .flat_map(|(field, errs)| {
                errs.iter().map(move |e| ValidationError {
                    field: field.to_string(),
                    message: e
                        .message
                        .clone()
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| "Validation failed".to_string()),
                    code: e.code.to_string(),
                })
            })
            .collect();

        Self::Validation {
            errors: validation_errors,
        }
    }
}

/// Helper macro for early returns with specific errors
#[macro_export]
macro_rules! bail {
    ($err:expr) => {
        return Err($err.into());
    };
    ($fmt:literal $(, $arg:expr)*) => {
        return Err($crate::error::GatewayError::internal(format!($fmt $(, $arg)*)));
    };
}

/// Helper macro for unauthorized errors
#[macro_export]
macro_rules! unauthorized {
    ($msg:expr) => {
        return Err($crate::error::GatewayError::unauthorized($msg));
    };
}

/// Helper macro for not found errors
#[macro_export]
macro_rules! not_found {
    ($resource:expr, $id:expr) => {
        return Err($crate::error::GatewayError::not_found($resource, $id));
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_status_codes() {
        assert_eq!(
            GatewayError::unauthorized("test").status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            GatewayError::not_found("user", "123").status_code(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            GatewayError::rate_limited(None).status_code(),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            GatewayError::internal("oops").status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn test_error_response_serialization() {
        let err = GatewayError::validation(vec![ValidationError {
            field: "email".to_string(),
            message: "Invalid email".to_string(),
            code: "invalid_email".to_string(),
        }]);

        let response = err.to_response();
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("VALIDATION_ERROR"));
        assert!(json.contains("email"));
    }

    #[test]
    fn test_rate_limit_retry_after() {
        let err = GatewayError::rate_limited(Some(60));
        let response = err.to_response();

        assert_eq!(response.error.retry_after, Some(60));
    }

    #[test]
    fn test_internal_error_correlation_id() {
        let err = GatewayError::internal("test error");

        match err {
            GatewayError::Internal { correlation_id, .. } => {
                assert!(!correlation_id.is_empty());
                // Should be a valid UUID
                assert!(uuid::Uuid::parse_str(&correlation_id).is_ok());
            }
            _ => panic!("Expected Internal error"),
        }
    }

    #[tokio::test]
    async fn test_error_into_response() {
        let err = GatewayError::not_found("user", "123");
        let response = err.into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
