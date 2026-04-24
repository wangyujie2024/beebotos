//! BeeBotOS Unified Error Types
//!
//! Provides a centralized error hierarchy for all BeeBotOS crates.
//! This enables consistent error handling across the entire system.
//!
//! # Error Hierarchy
//!
//! ```text
//! BeeBotOSError (base)
//! ├── SystemError
//! │   ├── Configuration
//! │   ├── Database
//! │   ├── Network
//! │   └── Io
//! ├── RuntimeError
//! │   ├── Agent
//! │   ├── Task
//! │   ├── Kernel
//! │   └── Timeout
//! ├── ValidationError
//! │   ├── InvalidInput
//! │   ├── Schema
//! │   └── Constraint
//! ├── SecurityError
//! │   ├── Authentication
//! │   ├── Authorization
//! │   └── Crypto
//! ├── BlockchainError
//! │   ├── Transaction
//! │   ├── Contract
//! │   └── Wallet
//! └── ExternalError
//!     └── Upstream
//! ```

use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Unique error code for programmatic handling
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    // System (1xxx)
    /// Configuration error (code: 1001)
    Configuration = 1001,
    /// Database error (code: 1002)
    Database = 1002,
    /// Network error (code: 1003)
    Network = 1003,
    /// I/O error (code: 1004)
    Io = 1004,
    /// Resource not found (code: 1005)
    NotFound = 1005,
    /// Resource already exists (code: 1006)
    AlreadyExists = 1006,

    // Runtime (2xxx)
    /// Agent execution error (code: 2001)
    Agent = 2001,
    /// Task execution error (code: 2002)
    Task = 2002,
    /// Kernel error (code: 2003)
    Kernel = 2003,
    /// Operation timeout (code: 2004)
    Timeout = 2004,
    /// Operation cancelled (code: 2005)
    Cancelled = 2005,

    // Validation (3xxx)
    /// Invalid input (code: 3001)
    InvalidInput = 3001,
    /// Schema validation error (code: 3002)
    Schema = 3002,
    /// Constraint violation (code: 3003)
    Constraint = 3003,
    /// Serialization error (code: 3004)
    Serialization = 3004,

    // Security (4xxx)
    /// Authentication failed (code: 4001)
    Authentication = 4001,
    /// Authorization failed (code: 4002)
    Authorization = 4002,
    /// Permission denied (code: 4003)
    PermissionDenied = 4003,
    /// Cryptographic error (code: 4004)
    Crypto = 4004,

    // Blockchain (5xxx)
    /// Transaction error (code: 5001)
    Transaction = 5001,
    /// Smart contract error (code: 5002)
    Contract = 5002,
    /// Wallet error (code: 5003)
    Wallet = 5003,
    /// Insufficient funds (code: 5004)
    InsufficientFunds = 5004,

    // External (6xxx)
    /// Upstream service error (code: 6001)
    Upstream = 6001,
    /// Rate limit exceeded (code: 6002)
    RateLimited = 6002,
    /// Service unavailable (code: 6003)
    Unavailable = 6003,

    // Unknown (9999)
    /// Unknown error (code: 9999)
    Unknown = 9999,
}

impl ErrorCode {
    /// Get HTTP status code for this error
    pub fn http_status(&self) -> u16 {
        use ErrorCode::*;
        match self {
            Configuration => 500,
            Database => 500,
            Network => 503,
            Io => 500,
            NotFound => 404,
            AlreadyExists => 409,

            Agent => 500,
            Task => 500,
            Kernel => 500,
            Timeout => 504,
            Cancelled => 499,

            InvalidInput => 400,
            Schema => 400,
            Constraint => 422,
            Serialization => 400,

            Authentication => 401,
            Authorization => 403,
            PermissionDenied => 403,
            Crypto => 400,

            Transaction => 400,
            Contract => 400,
            Wallet => 400,
            InsufficientFunds => 402,

            Upstream => 502,
            RateLimited => 429,
            Unavailable => 503,

            Unknown => 500,
        }
    }

    /// Get error category
    pub fn category(&self) -> &'static str {
        use ErrorCode::*;
        match self {
            Configuration | Database | Network | Io | NotFound | AlreadyExists => "system",
            Agent | Task | Kernel | Timeout | Cancelled => "runtime",
            InvalidInput | Schema | Constraint | Serialization => "validation",
            Authentication | Authorization | PermissionDenied | Crypto => "security",
            Transaction | Contract | Wallet | InsufficientFunds => "blockchain",
            Upstream | RateLimited | Unavailable => "external",
            Unknown => "unknown",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Error severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Severity {
    /// Debug information
    Debug,
    /// Informational
    Info,
    /// Warning
    Warning,
    /// Error (default)
    #[default]
    Error,
    /// Critical error
    Critical,
}

/// Structured error context for tracing
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ErrorContext {
    /// Operation being performed
    pub operation: Option<String>,
    /// Resource being accessed
    pub resource: Option<String>,
    /// Resource ID
    pub resource_id: Option<String>,
    /// User ID
    pub user_id: Option<String>,
    /// Request ID for tracing
    pub request_id: Option<String>,
    /// Additional metadata
    #[serde(flatten)]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

impl ErrorContext {
    /// Create new context
    pub fn new() -> Self {
        Self::default()
    }

    /// Add operation
    pub fn with_operation(mut self, op: impl Into<String>) -> Self {
        self.operation = Some(op.into());
        self
    }

    /// Add resource
    pub fn with_resource(mut self, resource: impl Into<String>) -> Self {
        self.resource = Some(resource.into());
        self
    }

    /// Add resource ID
    pub fn with_resource_id(mut self, id: impl Into<String>) -> Self {
        self.resource_id = Some(id.into());
        self
    }

    /// Add user ID
    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Add request ID
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    /// Add metadata
    pub fn with_metadata(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// Base error type for all BeeBotOS errors
#[derive(Debug, Clone)]
pub struct BeeBotOSError {
    /// Error code
    pub code: ErrorCode,
    /// Human-readable message
    pub message: String,
    /// Detailed technical description (optional)
    pub details: Option<String>,
    /// Error context
    pub context: ErrorContext,
    /// Severity level
    pub severity: Severity,
    /// Source error chain
    pub source: Option<Arc<BeeBotOSError>>,
    /// Timestamp when error occurred
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl BeeBotOSError {
    /// Create new error
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
            context: ErrorContext::default(),
            severity: Severity::Error,
            source: None,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Create with details
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Add context
    pub fn with_context(mut self, context: ErrorContext) -> Self {
        self.context = context;
        self
    }

    /// Set severity
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Set source error
    pub fn with_source(mut self, source: BeeBotOSError) -> Self {
        self.source = Some(Arc::new(source));
        self
    }

    /// Create configuration error
    pub fn configuration(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Configuration, message)
    }

    /// Create database error
    pub fn database(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Database, message)
    }

    /// Create not found error
    pub fn not_found(resource: impl Into<String>, id: impl Into<String>) -> Self {
        let resource = resource.into();
        let id = id.into();
        Self::new(
            ErrorCode::NotFound,
            format!("{} '{}' not found", resource, id),
        )
        .with_context(
            ErrorContext::new()
                .with_resource(resource)
                .with_resource_id(id),
        )
    }

    /// Create already exists error
    pub fn already_exists(resource: impl Into<String>, id: impl Into<String>) -> Self {
        let resource = resource.into();
        let id = id.into();
        Self::new(
            ErrorCode::AlreadyExists,
            format!("{} '{}' already exists", resource, id),
        )
        .with_context(
            ErrorContext::new()
                .with_resource(resource)
                .with_resource_id(id),
        )
    }

    /// Create validation error
    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidInput, message).with_severity(Severity::Warning)
    }

    /// Create authentication error
    pub fn authentication(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Authentication, message)
    }

    /// Create authorization error
    pub fn authorization(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Authorization, message)
    }

    /// Create timeout error
    pub fn timeout(operation: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::Timeout,
            format!("Operation '{}' timed out", operation.into()),
        )
    }

    /// Create agent error
    pub fn agent(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Agent, message)
    }

    /// Create blockchain error
    pub fn blockchain(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Transaction, message)
    }

    /// Create upstream error
    pub fn upstream(service: impl Into<String>, message: impl Into<String>) -> Self {
        let service = service.into();
        Self::new(
            ErrorCode::Upstream,
            format!("Upstream service '{}' error: {}", service, message.into()),
        )
        .with_context(ErrorContext::new().with_resource(service))
    }

    /// Create constraint violation error
    pub fn constraint(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Constraint, message).with_severity(Severity::Warning)
    }

    /// Create schema error
    pub fn schema(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Schema, message).with_severity(Severity::Warning)
    }

    /// Get HTTP status code
    pub fn http_status(&self) -> u16 {
        self.code.http_status()
    }

    /// Get error category
    pub fn category(&self) -> &'static str {
        self.code.category()
    }

    /// Check if this is a client error (4xx)
    pub fn is_client_error(&self) -> bool {
        let status = self.http_status();
        status >= 400 && status < 500
    }

    /// Check if this is a server error (5xx)
    pub fn is_server_error(&self) -> bool {
        let status = self.http_status();
        status >= 500 && status < 600
    }

    /// Check if this is retryable
    pub fn is_retryable(&self) -> bool {
        use ErrorCode::*;
        matches!(
            self.code,
            Network | Timeout | Cancelled | Database | Upstream | RateLimited | Unavailable
        )
    }

    /// Convert to JSON response
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "error": {
                "code": self.code as i32,
                "code_name": format!("{:?}", self.code),
                "category": self.category(),
                "message": self.message,
                "details": self.details,
                "severity": format!("{:?}", self.severity),
                "context": self.context,
                "timestamp": self.timestamp,
            }
        })
    }
}

impl fmt::Display for BeeBotOSError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)?;

        if let Some(ref details) = self.details {
            write!(f, " | Details: {}", details)?;
        }

        if let Some(ref source) = self.source {
            write!(f, " | Caused by: {}", source)?;
        }

        Ok(())
    }
}

/// Convert from std::io::Error
impl From<std::io::Error> for BeeBotOSError {
    fn from(err: std::io::Error) -> Self {
        use std::io::ErrorKind;

        let code = match err.kind() {
            ErrorKind::NotFound => ErrorCode::NotFound,
            ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
            ErrorKind::TimedOut | ErrorKind::WouldBlock => ErrorCode::Timeout,
            ErrorKind::InvalidInput | ErrorKind::InvalidData => ErrorCode::InvalidInput,
            _ => ErrorCode::Io,
        };

        BeeBotOSError::new(code, err.to_string()).with_severity(Severity::Error)
    }
}

impl std::error::Error for BeeBotOSError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|e| e.as_ref() as _)
    }
}

/// Result type alias
pub type Result<T> = std::result::Result<T, BeeBotOSError>;

/// Error builder for fluent API
#[derive(Debug, Default)]
pub struct ErrorBuilder {
    code: Option<ErrorCode>,
    message: Option<String>,
    details: Option<String>,
    context: ErrorContext,
    severity: Severity,
    source: Option<Arc<BeeBotOSError>>,
}

impl ErrorBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set error code
    pub fn code(mut self, code: ErrorCode) -> Self {
        self.code = Some(code);
        self
    }

    /// Set message
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Set details
    pub fn details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Set context
    pub fn context(mut self, context: ErrorContext) -> Self {
        self.context = context;
        self
    }

    /// Set severity
    pub fn severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Set source
    pub fn source(mut self, source: BeeBotOSError) -> Self {
        self.source = Some(Arc::new(source));
        self
    }

    /// Build error
    pub fn build(self) -> BeeBotOSError {
        BeeBotOSError {
            code: self.code.unwrap_or(ErrorCode::Unknown),
            message: self.message.unwrap_or_else(|| "Unknown error".to_string()),
            details: self.details,
            context: self.context,
            severity: self.severity,
            source: self.source,
            timestamp: chrono::Utc::now(),
        }
    }
}

/// Macro for creating errors with context
#[macro_export]
macro_rules! err {
    ($code:expr, $msg:expr) => {
        $crate::error::BeeBotOSError::new($code, $msg)
    };
    ($code:expr, $fmt:expr, $($arg:tt)*) => {
        $crate::error::BeeBotOSError::new($code, format!($fmt, $($arg)*))
    };
}

/// Macro for returning errors
#[macro_export]
macro_rules! bail {
    ($code:expr, $msg:expr) => {
        return Err($crate::err!($code, $msg));
    };
    ($code:expr, $fmt:expr, $($arg:tt)*) => {
        return Err($crate::err!($code, $fmt, $($arg)*));
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = BeeBotOSError::configuration("Invalid config");
        assert_eq!(err.code, ErrorCode::Configuration);
        assert_eq!(err.message, "Invalid config");
    }

    #[test]
    fn test_not_found() {
        let err = BeeBotOSError::not_found("agent", "123");
        assert_eq!(err.code, ErrorCode::NotFound);
        assert_eq!(err.http_status(), 404);
        assert!(err.is_client_error());
    }

    #[test]
    fn test_retryable() {
        let timeout = BeeBotOSError::timeout("operation");
        assert!(timeout.is_retryable());

        let not_found = BeeBotOSError::not_found("x", "y");
        assert!(!not_found.is_retryable());
    }

    #[test]
    fn test_error_builder() {
        let err = ErrorBuilder::new()
            .code(ErrorCode::Database)
            .message("DB error")
            .details("Connection failed")
            .severity(Severity::Critical)
            .build();

        assert_eq!(err.code, ErrorCode::Database);
        assert_eq!(err.message, "DB error");
        assert_eq!(err.details, Some("Connection failed".to_string()));
        assert!(matches!(err.severity, Severity::Critical));
    }

    #[test]
    fn test_error_chain() {
        let source = BeeBotOSError::database("Connection failed");
        let err = BeeBotOSError::agent("Failed to spawn").with_source(source);

        assert!(err.source.is_some());
    }

    #[test]
    fn test_to_json() {
        let err = BeeBotOSError::validation("Invalid input");
        let json = err.to_json();

        assert!(json.get("error").is_some());
        assert_eq!(json["error"]["code"], 3001);
    }
}
