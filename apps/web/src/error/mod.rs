//! Error Handling Architecture
//!
//! Provides:
//! - Error classification (User/System/Network)
//! - Global error handling
//! - User-friendly error messages
//! - Error tracking and reporting

use std::error::Error;
use std::fmt;

/// Error severity level
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorSeverity {
    /// Informational - can be ignored
    Info,
    /// Warning - user should be notified
    Warning,
    /// Error - operation failed but app can continue
    Error,
    /// Critical - app may need to restart
    Critical,
}

/// Error category for classification
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCategory {
    /// User input errors
    User,
    /// Network/connection errors
    Network,
    /// Server/API errors
    Server,
    /// Authentication/authorization errors
    Auth,
    /// Client-side logic errors
    Client,
    /// System/platform errors
    System,
}

/// Application error with metadata
#[derive(Clone, Debug)]
pub struct AppError {
    /// Error code for programmatic handling
    pub code: ErrorCode,
    /// User-friendly message
    pub message: String,
    /// Technical details (not shown to user)
    pub details: Option<String>,
    /// Error category
    pub category: ErrorCategory,
    /// Severity level
    pub severity: ErrorSeverity,
    /// Whether error is recoverable
    pub recoverable: bool,
    /// Suggested action for user
    pub action: Option<String>,
}

/// Error codes for programmatic handling
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    // Network errors (1000-1999)
    NetworkOffline = 1000,
    NetworkTimeout = 1001,
    NetworkDnsError = 1002,
    NetworkSslError = 1003,

    // Server errors (2000-2999)
    ServerInternal = 2000,
    ServerMaintenance = 2001,
    ServerRateLimit = 2002,
    ServerUnavailable = 2003,

    // Auth errors (3000-3999)
    AuthInvalidCredentials = 3000,
    AuthTokenExpired = 3001,
    AuthUnauthorized = 3002,
    AuthForbidden = 3003,
    AuthSessionExpired = 3004,

    // User errors (4000-4999)
    UserInvalidInput = 4000,
    UserValidationFailed = 4001,
    UserResourceNotFound = 4002,
    UserDuplicateEntry = 4003,
    UserOperationConflict = 4004,

    // Client errors (5000-5999)
    ClientUnknown = 5000,
    ClientSerialization = 5001,
    ClientStateError = 5002,

    // System errors (6000-6999)
    SystemStorageFull = 6000,
    SystemMemoryLow = 6001,
    SystemCompatibility = 6002,
}

impl AppError {
    /// Create a new error
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        let (category, severity, recoverable, action) = code.classify();

        Self {
            code,
            message: message.into(),
            details: None,
            category,
            severity,
            recoverable,
            action,
        }
    }

    /// Add technical details
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Set custom action
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }

    /// Create network offline error
    pub fn network_offline() -> Self {
        Self::new(ErrorCode::NetworkOffline, "You appear to be offline")
            .with_action("Please check your internet connection and try again")
    }

    /// Create timeout error
    pub fn network_timeout() -> Self {
        Self::new(ErrorCode::NetworkTimeout, "Request timed out")
            .with_action("Please try again later")
    }

    /// Create auth expired error
    pub fn auth_expired() -> Self {
        Self::new(ErrorCode::AuthTokenExpired, "Your session has expired")
            .with_action("Please log in again")
    }

    /// Create validation error
    pub fn validation_failed(field: &str, message: &str) -> Self {
        Self::new(
            ErrorCode::UserValidationFailed,
            format!("Validation failed for {}", field),
        )
        .with_details(message.to_string())
    }

    /// Check if error should be reported to user
    pub fn should_notify_user(&self) -> bool {
        matches!(
            self.severity,
            ErrorSeverity::Warning | ErrorSeverity::Error | ErrorSeverity::Critical
        )
    }

    /// Check if error should be logged
    pub fn should_log(&self) -> bool {
        true // Log all errors
    }

    /// Get error title for display
    pub fn title(&self) -> &'static str {
        match self.category {
            ErrorCategory::Network => "Connection Error",
            ErrorCategory::Server => "Server Error",
            ErrorCategory::Auth => "Authentication Error",
            ErrorCategory::User => "Input Error",
            ErrorCategory::Client => "Application Error",
            ErrorCategory::System => "System Error",
        }
    }

    /// Get icon for error category
    pub fn icon(&self) -> &'static str {
        match self.category {
            ErrorCategory::Network => "🌐",
            ErrorCategory::Server => "🔧",
            ErrorCategory::Auth => "🔒",
            ErrorCategory::User => "⚠️",
            ErrorCategory::Client => "🐛",
            ErrorCategory::System => "⚙️",
        }
    }
}

impl ErrorCode {
    /// Classify error code
    fn classify(self) -> (ErrorCategory, ErrorSeverity, bool, Option<String>) {
        match self as u32 {
            1000..=1999 => match self {
                ErrorCode::NetworkOffline => (
                    ErrorCategory::Network,
                    ErrorSeverity::Warning,
                    true,
                    Some("Check your internet connection".to_string()),
                ),
                ErrorCode::NetworkTimeout => (
                    ErrorCategory::Network,
                    ErrorSeverity::Warning,
                    true,
                    Some("Try again later".to_string()),
                ),
                _ => (
                    ErrorCategory::Network,
                    ErrorSeverity::Error,
                    true,
                    Some("Check your connection and retry".to_string()),
                ),
            },
            2000..=2999 => (
                ErrorCategory::Server,
                ErrorSeverity::Error,
                true,
                Some("Please try again later".to_string()),
            ),
            3000..=3999 => match self {
                ErrorCode::AuthTokenExpired | ErrorCode::AuthSessionExpired => (
                    ErrorCategory::Auth,
                    ErrorSeverity::Warning,
                    true,
                    Some("Please log in again".to_string()),
                ),
                ErrorCode::AuthUnauthorized => (
                    ErrorCategory::Auth,
                    ErrorSeverity::Error,
                    false,
                    Some("Contact administrator if you need access".to_string()),
                ),
                _ => (
                    ErrorCategory::Auth,
                    ErrorSeverity::Error,
                    true,
                    Some("Check your credentials".to_string()),
                ),
            },
            4000..=4999 => (
                ErrorCategory::User,
                ErrorSeverity::Warning,
                true,
                Some("Please check your input and try again".to_string()),
            ),
            5000..=5999 => (
                ErrorCategory::Client,
                ErrorSeverity::Error,
                true,
                Some("Please refresh the page".to_string()),
            ),
            _ => (
                ErrorCategory::System,
                ErrorSeverity::Critical,
                false,
                Some("Please contact support".to_string()),
            ),
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}] {}", self.code, self.message)
    }
}

impl Error for AppError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

/// Convert from API error
impl From<crate::api::ApiError> for AppError {
    fn from(err: crate::api::ApiError) -> Self {
        match err {
            crate::api::ApiError::Network(msg) => AppError::network_offline().with_details(msg),
            crate::api::ApiError::Timeout => AppError::network_timeout(),
            crate::api::ApiError::Unauthorized => {
                AppError::new(ErrorCode::AuthUnauthorized, "Unauthorized")
            }
            crate::api::ApiError::Forbidden => {
                AppError::new(ErrorCode::AuthForbidden, "Access denied")
            }
            crate::api::ApiError::NotFound => {
                AppError::new(ErrorCode::UserResourceNotFound, "Resource not found")
            }
            crate::api::ApiError::ServerError(code, msg) => {
                AppError::new(ErrorCode::ServerInternal, msg.unwrap_or_else(|| format!("Server error: {}", code)))
            }
            crate::api::ApiError::ClientError(code, msg) => AppError::new(
                ErrorCode::UserInvalidInput,
                msg.unwrap_or_else(|| format!("Invalid request: {}", code)),
            ),
            _ => AppError::new(ErrorCode::ClientUnknown, "An unexpected error occurred"),
        }
    }
}

/// Global error handler
#[derive(Clone)]
pub struct ErrorHandler {
    reporter: std::rc::Rc<dyn Fn(&AppError)>,
}

impl ErrorHandler {
    pub fn new() -> Self {
        Self {
            reporter: std::rc::Rc::new(|err: &AppError| {
                // Default: log to console
                web_sys::console::error_1(&format!("Error: {:?}", err).into());
            }),
        }
    }

    pub fn with_reporter<F: Fn(&AppError) + 'static>(mut self, reporter: F) -> Self {
        self.reporter = std::rc::Rc::new(reporter);
        self
    }

    pub fn handle(&self, error: AppError) {
        // Log error
        if error.should_log() {
            (self.reporter)(&error);
        }

        // Report critical errors
        if matches!(error.severity, ErrorSeverity::Critical) {
            // Could send to error tracking service
            self.report_critical(&error);
        }
    }

    fn report_critical(&self, error: &AppError) {
        // Placeholder for critical error reporting
        web_sys::console::error_1(&format!("CRITICAL ERROR: {:?}", error).into());
    }
}

impl Default for ErrorHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Result type alias
pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = AppError::new(ErrorCode::NetworkOffline, "Test error");
        assert_eq!(err.category, ErrorCategory::Network);
        assert!(err.recoverable);
    }

    #[test]
    fn test_validation_error() {
        let err = AppError::validation_failed("email", "Invalid format");
        assert_eq!(err.code, ErrorCode::UserValidationFailed);
        assert!(err.details.is_some());
    }
}
