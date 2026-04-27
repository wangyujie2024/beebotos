//! Retry and Circuit Breaker Tests
//!
//! Tests for retry logic, circuit breaker, and rate limiting.

use std::time::Duration;

use beebotos_chain::compat::retry::{with_retry, CircuitBreaker, RateLimiter, RetryConfig};
use beebotos_chain::ChainError;

/// Test retry config default values
#[test]
fn test_retry_config_default() {
    let config = RetryConfig::default();
    assert_eq!(config.max_retries, 5);
    assert_eq!(config.initial_interval, Duration::from_millis(100));
    assert_eq!(config.max_interval, Duration::from_secs(30));
    assert_eq!(config.multiplier, 2.0);
    assert_eq!(config.randomization_factor, 0.1);
}

/// Test retry config aggressive preset
#[test]
fn test_retry_config_aggressive() {
    let config = RetryConfig::aggressive();
    assert_eq!(config.max_retries, 10);
    assert_eq!(config.initial_interval, Duration::from_millis(50));
    assert_eq!(config.max_interval, Duration::from_secs(10));
    assert_eq!(config.multiplier, 1.5);
}

/// Test retry config conservative preset
#[test]
fn test_retry_config_conservative() {
    let config = RetryConfig::conservative();
    assert_eq!(config.max_retries, 3);
    assert_eq!(config.initial_interval, Duration::from_millis(500));
    assert_eq!(config.max_interval, Duration::from_secs(60));
    assert_eq!(config.multiplier, 2.5);
}

/// Test retry config none preset
#[test]
fn test_retry_config_none() {
    let config = RetryConfig::none();
    assert_eq!(config.max_retries, 0);
}

/// Test circuit breaker initial state
#[test]
fn test_circuit_breaker_initial_state() {
    let mut cb = CircuitBreaker::new(3, 2, 60);

    assert!(cb.allow_request());
    assert_eq!(cb.state(), "closed");
}

/// Test circuit breaker opens after failures
#[test]
fn test_circuit_breaker_opens_after_failures() {
    let mut cb = CircuitBreaker::new(3, 2, 60);

    // Record failures up to threshold
    cb.record_failure();
    assert!(cb.allow_request()); // Still closed

    cb.record_failure();
    assert!(cb.allow_request()); // Still closed

    cb.record_failure();
    assert!(!cb.allow_request()); // Now open
    assert_eq!(cb.state(), "open");
}

/// Test circuit breaker success reset
#[test]
fn test_circuit_breaker_success_reset() {
    let mut cb = CircuitBreaker::new(3, 2, 60);

    cb.record_failure();
    cb.record_failure();
    assert!(cb.allow_request());

    // Success should reset failure count
    cb.record_success();
    assert!(cb.allow_request());
    assert_eq!(cb.state(), "closed");
}

/// Test circuit breaker half-open state
#[test]
fn test_circuit_breaker_half_open() {
    let mut cb = CircuitBreaker::new(1, 1, 1); // 1 second timeout

    // Open the circuit
    cb.record_failure();
    assert!(!cb.allow_request());
    assert_eq!(cb.state(), "open");

    // Wait for timeout
    // Circuit should transition to half-open
    std::thread::sleep(Duration::from_secs(1));
    assert!(cb.allow_request());
    assert_eq!(cb.state(), "half-open");
}

/// Test rate limiter
#[test]
fn test_rate_limiter() {
    let _limiter = RateLimiter::new(10); // 10 requests per second

    // Verify initial state
    // Note: We can't easily test the timing without async runtime
    // but we can verify the structure
}

/// Test is_retryable_error for connection errors
#[test]
fn test_is_retryable_error_connection() {
    use beebotos_chain::compat::retry::is_retryable_error;

    assert!(is_retryable_error(&ChainError::Connection(
        "timeout".to_string()
    )));
    assert!(is_retryable_error(&ChainError::Connection(
        "connection reset".to_string()
    )));
}

/// Test is_retryable_error for provider errors
#[test]
fn test_is_retryable_error_provider() {
    use beebotos_chain::compat::retry::is_retryable_error;

    assert!(is_retryable_error(&ChainError::Provider(
        "rate limit exceeded".to_string()
    )));
    assert!(is_retryable_error(&ChainError::Provider(
        "503 service unavailable".to_string()
    )));
    assert!(is_retryable_error(&ChainError::Provider(
        "429 too many requests".to_string()
    )));
}

/// Test is_retryable_error for non-retryable errors
#[test]
fn test_is_retryable_error_non_retryable() {
    use beebotos_chain::compat::retry::is_retryable_error;

    assert!(!is_retryable_error(&ChainError::InvalidAddress(
        "invalid".to_string()
    )));
    assert!(!is_retryable_error(&ChainError::InsufficientBalance));
    assert!(!is_retryable_error(&ChainError::Validation(
        "invalid input".to_string()
    )));
}

/// Test is_retryable_error for transaction errors
#[test]
fn test_is_retryable_error_transaction() {
    use beebotos_chain::compat::retry::is_retryable_error;

    assert!(!is_retryable_error(&ChainError::TransactionFailed {
        tx_hash: B256::ZERO,
        reason: "out of gas".to_string(),
    }));
}

/// Test retry with immediate success
#[tokio::test]
async fn test_retry_immediate_success() {
    let config = RetryConfig::default();

    let result = with_retry(&config, || async { Ok::<_, ChainError>(42) }).await;

    assert_eq!(result.unwrap(), 42);
}

/// Test retry with eventual success
#[tokio::test]
async fn test_retry_eventual_success() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let config = RetryConfig {
        max_retries: 3,
        initial_interval: Duration::from_millis(1),
        ..Default::default()
    };

    let counter = AtomicUsize::new(0);

    let result = with_retry(&config, || async {
        let count = counter.fetch_add(1, Ordering::SeqCst);
        if count < 2 {
            Err(ChainError::Connection("temporary error".to_string()))
        } else {
            Ok(42)
        }
    })
    .await;

    assert_eq!(result.unwrap(), 42);
    assert_eq!(counter.load(Ordering::SeqCst), 3);
}

/// Test retry gives up after max retries
#[tokio::test]
async fn test_retry_max_retries_exceeded() {
    let config = RetryConfig {
        max_retries: 2,
        initial_interval: Duration::from_millis(1),
        ..Default::default()
    };

    let result = with_retry(&config, || async {
        Err::<(), _>(ChainError::Connection("persistent error".to_string()))
    })
    .await;

    assert!(result.is_err());
}

/// Test retry doesn't retry non-retryable errors
#[tokio::test]
async fn test_retry_no_retry_non_retryable() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let config = RetryConfig::default();
    let counter = AtomicUsize::new(0);

    let result = with_retry(&config, || async {
        counter.fetch_add(1, Ordering::SeqCst);
        Err::<(), _>(ChainError::InvalidAddress("invalid".to_string()))
    })
    .await;

    assert!(result.is_err());
    assert_eq!(counter.load(Ordering::SeqCst), 1); // Should only try once
}

/// Test circuit breaker closes after success threshold in half-open
#[test]
fn test_circuit_breaker_closes_after_successes() {
    let mut cb = CircuitBreaker::new(1, 2, 1); // 1 second timeout

    // Open circuit
    cb.record_failure();
    assert!(!cb.allow_request());
    assert_eq!(cb.state(), "open");

    // Wait for timeout, then go to half-open
    std::thread::sleep(Duration::from_secs(1));
    assert!(cb.allow_request());
    assert_eq!(cb.state(), "half-open");

    // Record successes to close
    cb.record_success();
    assert!(cb.allow_request());

    cb.record_success();
    assert!(cb.allow_request());
    assert_eq!(cb.state(), "closed");
}

/// Test circuit breaker reopens on failure in half-open
#[test]
fn test_circuit_breaker_reopens_on_failure() {
    let mut cb = CircuitBreaker::new(1, 2, 1); // 1 second timeout

    // Open circuit
    cb.record_failure();
    assert!(!cb.allow_request());
    assert_eq!(cb.state(), "open");

    // Wait for timeout, then go to half-open
    std::thread::sleep(Duration::from_secs(1));
    assert!(cb.allow_request());
    assert_eq!(cb.state(), "half-open");

    // Failure in half-open should reopen
    cb.record_failure();
    assert!(!cb.allow_request());
    assert_eq!(cb.state(), "open");
}

use beebotos_chain::compat::B256;
