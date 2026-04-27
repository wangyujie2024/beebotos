//! Retry Logic for Blockchain Operations
//!
//! Provides configurable retry mechanisms with exponential backoff for RPC
//! calls.

use std::time::Duration;

use backoff::ExponentialBackoff;

use crate::constants::{
    AGGRESSIVE_MAX_RETRIES, CIRCUIT_BREAKER_FAILURE_THRESHOLD, CIRCUIT_BREAKER_SUCCESS_THRESHOLD,
    CIRCUIT_BREAKER_TIMEOUT_SECS, CONSERVATIVE_MAX_RETRIES, DEFAULT_MAX_RETRIES,
    DEFAULT_RETRY_INITIAL_MS, DEFAULT_RETRY_MAX_INTERVAL_SECS,
};
use crate::ChainError;

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Initial retry interval
    pub initial_interval: Duration,
    /// Maximum retry interval
    pub max_interval: Duration,
    /// Multiplier for exponential backoff
    pub multiplier: f64,
    /// Randomization factor (0.0 - 1.0)
    pub randomization_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: DEFAULT_MAX_RETRIES,
            initial_interval: Duration::from_millis(DEFAULT_RETRY_INITIAL_MS),
            max_interval: Duration::from_secs(DEFAULT_RETRY_MAX_INTERVAL_SECS),
            multiplier: 2.0,
            randomization_factor: 0.1,
        }
    }
}

impl RetryConfig {
    /// Create config for aggressive retry (more attempts, faster)
    pub fn aggressive() -> Self {
        Self {
            max_retries: AGGRESSIVE_MAX_RETRIES,
            initial_interval: Duration::from_millis(50),
            max_interval: Duration::from_secs(10),
            multiplier: 1.5,
            randomization_factor: 0.2,
        }
    }

    /// Create config for conservative retry (fewer attempts, slower)
    pub fn conservative() -> Self {
        Self {
            max_retries: CONSERVATIVE_MAX_RETRIES,
            initial_interval: Duration::from_millis(500),
            max_interval: Duration::from_secs(60),
            multiplier: 2.5,
            randomization_factor: 0.1,
        }
    }

    /// Create config with no retries
    pub fn none() -> Self {
        Self {
            max_retries: 0,
            ..Default::default()
        }
    }

    /// Convert to ExponentialBackoff
    pub fn to_backoff(&self) -> ExponentialBackoff {
        ExponentialBackoff {
            current_interval: self.initial_interval,
            initial_interval: self.initial_interval,
            max_interval: self.max_interval,
            multiplier: self.multiplier,
            randomization_factor: self.randomization_factor,
            max_elapsed_time: Some(self.initial_interval * self.max_retries + self.max_interval),
            ..Default::default()
        }
    }
}

/// Check if an error is retryable
pub fn is_retryable_error(error: &ChainError) -> bool {
    match error {
        ChainError::Connection(_) => true,
        ChainError::Provider(msg) => {
            // Retry on common transient errors
            let retryable_patterns = [
                "timeout",
                "rate limit",
                "too many requests",
                "connection reset",
                "broken pipe",
                "temporary",
                "503",
                "504",
                "429",
            ];
            let msg_lower = msg.to_lowercase();
            retryable_patterns.iter().any(|&p| msg_lower.contains(p))
        }
        ChainError::AlloyProvider(msg) => {
            let retryable_patterns = ["timeout", "rate limit", "connection", "503", "504", "429"];
            let msg_lower = msg.to_lowercase();
            retryable_patterns.iter().any(|&p| msg_lower.contains(p))
        }
        // Don't retry transaction errors or invalid input
        ChainError::TransactionFailed { .. } => false,
        ChainError::InvalidAddress(_) => false,
        ChainError::InsufficientBalance => false,
        _ => false,
    }
}

/// Execute an operation with retry logic
pub async fn with_retry<F, Fut, T>(config: &RetryConfig, mut operation: F) -> Result<T, ChainError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, ChainError>>,
{
    if config.max_retries == 0 {
        return operation().await;
    }

    let mut last_error = None;
    let mut current_interval = config.initial_interval;

    for attempt in 0..=config.max_retries {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if !is_retryable_error(&e) || attempt == config.max_retries {
                    return Err(e);
                }

                last_error = Some(e);

                if attempt < config.max_retries {
                    tokio::time::sleep(current_interval).await;

                    // Exponential backoff
                    current_interval = Duration::from_millis(
                        ((current_interval.as_millis() as f64 * config.multiplier)
                            .min(config.max_interval.as_millis() as f64))
                            as u64,
                    );
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| ChainError::Provider("Max retries exceeded".to_string())))
}

/// Execute an operation with retry and custom error handler
pub async fn with_retry_and_handler<F, Fut, T, H>(
    config: &RetryConfig,
    mut operation: F,
    mut error_handler: H,
) -> Result<T, ChainError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, ChainError>>,
    H: FnMut(&ChainError),
{
    let mut last_error = None;
    let mut current_interval = config.initial_interval;

    for attempt in 0..=config.max_retries {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                error_handler(&e);

                if !is_retryable_error(&e) || attempt == config.max_retries {
                    return Err(e);
                }

                last_error = Some(e);

                // Calculate next interval with jitter
                let jitter = if config.randomization_factor > 0.0 {
                    let jitter_range =
                        current_interval.as_millis() as f64 * config.randomization_factor;
                    let jitter_val = (rand::random::<f64>() - 0.5) * 2.0 * jitter_range;
                    Duration::from_millis(
                        (current_interval.as_millis() as f64 + jitter_val).max(0.0) as u64,
                    )
                } else {
                    current_interval
                };

                tracing::info!(
                    "Retrying after {:?} (attempt {}/{})",
                    jitter,
                    attempt + 1,
                    config.max_retries
                );

                tokio::time::sleep(jitter).await;

                // Exponential backoff
                current_interval = Duration::from_millis(
                    ((current_interval.as_millis() as f64 * config.multiplier)
                        .min(config.max_interval.as_millis() as f64)) as u64,
                );
            }
        }
    }

    Err(last_error.unwrap_or_else(|| ChainError::Provider("Max retries exceeded".to_string())))
}

/// Circuit breaker for handling repeated failures
pub struct CircuitBreaker {
    failure_threshold: u32,
    success_threshold: u32,
    timeout: Duration,
    state: CircuitState,
    consecutive_failures: u32,
    consecutive_successes: u32,
    last_failure_time: Option<std::time::Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CircuitState {
    Closed,   // Normal operation
    Open,     // Failing, reject requests
    HalfOpen, // Testing if recovered
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self {
            failure_threshold: CIRCUIT_BREAKER_FAILURE_THRESHOLD,
            success_threshold: CIRCUIT_BREAKER_SUCCESS_THRESHOLD,
            timeout: Duration::from_secs(CIRCUIT_BREAKER_TIMEOUT_SECS),
            state: CircuitState::Closed,
            consecutive_failures: 0,
            consecutive_successes: 0,
            last_failure_time: None,
        }
    }
}

impl CircuitBreaker {
    /// Create new circuit breaker
    pub fn new(failure_threshold: u32, success_threshold: u32, timeout_secs: u64) -> Self {
        Self {
            failure_threshold,
            success_threshold,
            timeout: Duration::from_secs(timeout_secs),
            state: CircuitState::Closed,
            consecutive_failures: 0,
            consecutive_successes: 0,
            last_failure_time: None,
        }
    }

    /// Create with default settings
    pub fn new_default() -> Self {
        Self::default()
    }

    /// Check if request is allowed
    pub fn allow_request(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has passed
                if let Some(last_failure) = self.last_failure_time {
                    if last_failure.elapsed() > self.timeout {
                        self.state = CircuitState::HalfOpen;
                        self.consecutive_successes = 0;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Record success
    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;

        match self.state {
            CircuitState::HalfOpen => {
                self.consecutive_successes += 1;
                if self.consecutive_successes >= self.success_threshold {
                    self.state = CircuitState::Closed;
                    self.consecutive_successes = 0;
                    tracing::info!("Circuit breaker closed");
                }
            }
            CircuitState::Closed => {
                self.consecutive_successes += 1;
            }
            CircuitState::Open => {}
        }
    }

    /// Record failure
    pub fn record_failure(&mut self) {
        self.consecutive_successes = 0;
        self.consecutive_failures += 1;
        self.last_failure_time = Some(std::time::Instant::now());

        match self.state {
            CircuitState::Closed | CircuitState::HalfOpen => {
                if self.consecutive_failures >= self.failure_threshold {
                    self.state = CircuitState::Open;
                    tracing::warn!("Circuit breaker opened");
                }
            }
            CircuitState::Open => {}
        }
    }

    /// Get current state
    pub fn state(&self) -> &str {
        match self.state {
            CircuitState::Closed => "closed",
            CircuitState::Open => "open",
            CircuitState::HalfOpen => "half-open",
        }
    }
}

/// Rate limiter for RPC calls
pub struct RateLimiter {
    #[allow(dead_code)]
    max_requests_per_second: u32,
    min_interval: Duration,
    last_request: Option<std::time::Instant>,
}

impl RateLimiter {
    /// Create new rate limiter
    pub fn new(max_requests_per_second: u32) -> Self {
        Self {
            max_requests_per_second,
            min_interval: Duration::from_millis(1000 / max_requests_per_second.max(1) as u64),
            last_request: None,
        }
    }

    /// Wait if necessary before making request
    pub async fn acquire(&mut self) {
        if let Some(last) = self.last_request {
            let elapsed = last.elapsed();
            if elapsed < self.min_interval {
                let wait = self.min_interval - elapsed;
                tokio::time::sleep(wait).await;
            }
        }
        self.last_request = Some(std::time::Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, DEFAULT_MAX_RETRIES);
        assert_eq!(
            config.initial_interval,
            Duration::from_millis(DEFAULT_RETRY_INITIAL_MS)
        );
    }

    #[test]
    fn test_retry_config_aggressive() {
        let config = RetryConfig::aggressive();
        assert_eq!(config.max_retries, AGGRESSIVE_MAX_RETRIES);
        assert_eq!(config.initial_interval, Duration::from_millis(50));
    }

    #[test]
    fn test_circuit_breaker() {
        let mut cb = CircuitBreaker::new(
            CIRCUIT_BREAKER_FAILURE_THRESHOLD,
            CIRCUIT_BREAKER_SUCCESS_THRESHOLD,
            CIRCUIT_BREAKER_TIMEOUT_SECS,
        );

        // Initially closed
        assert!(cb.allow_request());
        assert_eq!(cb.state(), "closed");

        // Record failures
        cb.record_failure();
        cb.record_failure();
        assert!(cb.allow_request()); // Still closed

        cb.record_failure();
        assert!(!cb.allow_request()); // Now open
        assert_eq!(cb.state(), "open");

        // Can't make requests while open
        assert!(!cb.allow_request());
    }

    #[test]
    fn test_rate_limiter() {
        let limiter = RateLimiter::new(10); // 10 req/sec
        assert_eq!(limiter.min_interval, Duration::from_millis(100));
    }

    #[test]
    fn test_is_retryable_error() {
        assert!(is_retryable_error(&ChainError::Connection(
            "timeout".to_string()
        )));
        assert!(is_retryable_error(&ChainError::Provider(
            "rate limit exceeded".to_string()
        )));
        assert!(!is_retryable_error(&ChainError::InvalidAddress(
            "invalid".to_string()
        )));
        assert!(!is_retryable_error(&ChainError::InsufficientBalance));
    }
}
