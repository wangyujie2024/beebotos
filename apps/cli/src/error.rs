//! Error handling for BeeBotOS CLI
//!
//! Provides structured error types, user-friendly error messages,
//! and automatic retry mechanisms for transient failures.

#![allow(dead_code)]

use std::time::Duration;

use thiserror::Error;
use tokio::time::sleep;

/// CLI error types
#[derive(Error, Debug)]
pub enum CliError {
    /// Configuration error
    #[error("Configuration error: {message}")]
    Config { message: String },

    /// API error with status code
    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },

    /// Network/connection error
    #[error("Network error: {message}. Please check your connection and try again.")]
    Network { message: String },

    /// Authentication error
    #[error("Authentication failed: {message}. Please check your API key.")]
    Auth { message: String },

    /// Timeout error
    #[error("Request timed out after {duration}s. The server may be busy.")]
    Timeout { duration: u64 },

    /// Validation error
    #[error("Validation error: {field} - {message}")]
    Validation { field: String, message: String },

    /// Resource not found
    #[error("Not found: {resource} '{id}'")]
    NotFound { resource: String, id: String },

    /// Rate limited
    #[error("Rate limit exceeded. Please wait {retry_after}s before retrying.")]
    RateLimited { retry_after: u64 },

    /// Server error
    #[error("Server error ({status}): {message}. Please try again later.")]
    Server { status: u16, message: String },

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Parse error
    #[error("Parse error: {message}")]
    Parse { message: String },

    /// WebSocket error
    #[error("WebSocket error: {message}")]
    WebSocket { message: String },

    /// Unknown error
    #[error("Unknown error: {0}")]
    Unknown(String),
}

impl CliError {
    /// Check if the error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            CliError::Network { .. }
                | CliError::Timeout { .. }
                | CliError::Server { .. }
                | CliError::RateLimited { .. }
        )
    }

    /// Get retry after duration for rate limited errors
    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            CliError::RateLimited { retry_after } => Some(Duration::from_secs(*retry_after)),
            CliError::Timeout { .. } => Some(Duration::from_secs(5)),
            _ => None,
        }
    }

    /// Convert anyhow::Error to CliError
    pub fn from_anyhow(err: anyhow::Error) -> Self {
        let err_str = err.to_string();

        if err_str.contains("Connection refused") || err_str.contains("dns error") {
            CliError::Network { message: err_str }
        } else if err_str.contains("timed out") {
            CliError::Timeout { duration: 30 }
        } else if err_str.contains("401") || err_str.contains("Unauthorized") {
            CliError::Auth {
                message: "Invalid or expired API key".to_string(),
            }
        } else if err_str.contains("404") || err_str.contains("Not Found") {
            CliError::NotFound {
                resource: "Resource".to_string(),
                id: "unknown".to_string(),
            }
        } else if err_str.contains("429") || err_str.contains("Too Many Requests") {
            CliError::RateLimited { retry_after: 60 }
        } else if err_str.contains("500") || err_str.contains("502") || err_str.contains("503") {
            CliError::Server {
                status: 500,
                message: err_str,
            }
        } else {
            CliError::Unknown(err_str)
        }
    }
}

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Initial retry delay
    pub initial_delay: Duration,
    /// Maximum retry delay
    pub max_delay: Duration,
    /// Exponential backoff multiplier
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
        }
    }
}

/// Execute an async function with exponential backoff retry
pub async fn with_retry<F, Fut, T>(config: &RetryConfig, operation: F) -> Result<T, CliError>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, anyhow::Error>>,
{
    let mut delay = config.initial_delay;
    let mut last_error = None;

    for attempt in 0..=config.max_retries {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                let cli_err = CliError::from_anyhow(e);

                if !cli_err.is_retryable() || attempt == config.max_retries {
                    return Err(cli_err);
                }

                last_error = Some(cli_err);

                // Calculate next delay with exponential backoff
                let next_delay = std::cmp::min(
                    Duration::from_millis(
                        (delay.as_millis() as f64 * config.backoff_multiplier) as u64,
                    ),
                    config.max_delay,
                );

                eprintln!(
                    "⚠️  Request failed (attempt {}/{}), retrying in {:?}...",
                    attempt + 1,
                    config.max_retries + 1,
                    delay
                );

                sleep(delay).await;
                delay = next_delay;
            }
        }
    }

    Err(last_error.unwrap_or_else(|| CliError::Unknown("Max retries exceeded".to_string())))
}

/// User-friendly error display
pub fn print_error(err: &CliError) {
    use colored::Colorize;

    match err {
        CliError::Auth { .. } => {
            eprintln!("{}", "✗ Authentication Error".red().bold());
            eprintln!("{}", err);
            eprintln!("\nTo fix this:");
            eprintln!("  1. Run: beebot config set api_key <your-api-key>");
            eprintln!("  2. Or set BEEBOTOS_API_KEY environment variable");
        }
        CliError::Network { .. } => {
            eprintln!("{}", "✗ Network Error".red().bold());
            eprintln!("{}", err);
            eprintln!("\nTroubleshooting:");
            eprintln!("  - Check your internet connection");
            eprintln!("  - Verify the API endpoint is correct: beebot config show");
            eprintln!("  - Check if the server is running");
        }
        CliError::NotFound { resource, id } => {
            eprintln!("{}", format!("✗ {} Not Found", resource).red().bold());
            eprintln!("'{}' does not exist or you don't have access to it.", id);
        }
        CliError::RateLimited { retry_after } => {
            eprintln!("{}", "✗ Rate Limited".yellow().bold());
            eprintln!("Please wait {} seconds before trying again.", retry_after);
        }
        CliError::Validation { field, message } => {
            eprintln!("{}", "✗ Validation Error".red().bold());
            eprintln!("Field '{}': {}", field, message);
        }
        CliError::Config { message } => {
            eprintln!("{}", "✗ Configuration Error".red().bold());
            eprintln!("{}", message);
            eprintln!("\nRun 'beebot config validate' to check your configuration.");
        }
        _ => {
            eprintln!("{}", "✗ Error".red().bold());
            eprintln!("{}", err);
        }
    }
}

/// Result type alias
pub type CliResult<T> = Result<T, CliError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_error_display() {
        let err = CliError::Auth {
            message: "Invalid key".to_string(),
        };
        assert!(err.to_string().contains("Authentication failed"));
    }

    #[test]
    fn test_retryable_errors() {
        assert!(CliError::Network {
            message: "test".to_string(),
        }
        .is_retryable());

        assert!(CliError::Timeout { duration: 30 }.is_retryable());

        assert!(!CliError::Auth {
            message: "test".to_string(),
        }
        .is_retryable());

        assert!(!CliError::Validation {
            field: "test".to_string(),
            message: "test".to_string(),
        }
        .is_retryable());
    }

    #[test]
    fn test_from_anyhow_network() {
        let anyhow_err = anyhow::anyhow!("Connection refused");
        let cli_err = CliError::from_anyhow(anyhow_err);
        assert!(matches!(cli_err, CliError::Network { .. }));
    }

    #[test]
    fn test_from_anyhow_timeout() {
        let anyhow_err = anyhow::anyhow!("Request timed out");
        let cli_err = CliError::from_anyhow(anyhow_err);
        assert!(matches!(cli_err, CliError::Timeout { .. }));
    }

    #[test]
    fn test_from_anyhow_auth() {
        let anyhow_err = anyhow::anyhow!("401 Unauthorized");
        let cli_err = CliError::from_anyhow(anyhow_err);
        assert!(matches!(cli_err, CliError::Auth { .. }));
    }

    #[tokio::test]
    async fn test_retry_success_first_attempt() {
        let config = RetryConfig {
            max_retries: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
        };

        let result = with_retry(&config, || async { Ok::<_, anyhow::Error>(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_exhausted() {
        let config = RetryConfig {
            max_retries: 1,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
        };

        let result = with_retry(&config, || async {
            Err::<i32, anyhow::Error>(anyhow::anyhow!("Connection refused"))
        })
        .await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CliError::Network { .. }));
    }

    #[tokio::test]
    async fn test_retry_eventual_success() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let config = RetryConfig {
            max_retries: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
        };

        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts_clone = attempts.clone();
        let result = with_retry(&config, move || {
            let attempts_inner = attempts_clone.clone();
            async move {
                let count = attempts_inner.fetch_add(1, Ordering::SeqCst) + 1;
                if count < 3 {
                    Err::<i32, anyhow::Error>(anyhow::anyhow!("Connection refused"))
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }
}
