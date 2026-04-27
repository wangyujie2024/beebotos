//! Circuit Breaker Pattern Implementation
//!
//! Provides fault tolerance by preventing cascade failures.
//!
//! States:
//! - Closed: Normal operation, requests pass through
//! - Open: Failure threshold reached, requests fail fast
//! - HalfOpen: Testing if service has recovered

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CircuitState {
    /// Normal operation
    #[default]
    Closed,
    /// Failing fast
    Open,
    /// Testing recovery
    HalfOpen,
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Failure threshold to open circuit
    pub failure_threshold: u32,
    /// Success threshold to close circuit in half-open state
    pub success_threshold: u32,
    /// Timeout before attempting recovery (half-open)
    pub timeout: Duration,
    /// Half-open max attempts
    pub half_open_max_calls: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            timeout: Duration::from_secs(30),
            half_open_max_calls: 3,
        }
    }
}

impl CircuitBreakerConfig {
    /// Quick recovery (for testing)
    pub fn fast_recovery() -> Self {
        Self {
            failure_threshold: 3,
            success_threshold: 2,
            timeout: Duration::from_secs(5),
            half_open_max_calls: 2,
        }
    }

    /// Conservative settings for critical services
    pub fn conservative() -> Self {
        Self {
            failure_threshold: 10,
            success_threshold: 5,
            timeout: Duration::from_secs(60),
            half_open_max_calls: 5,
        }
    }
}

/// Circuit breaker statistics
#[derive(Debug, Default, Clone)]
pub struct CircuitStats {
    /// Current circuit state
    pub state: CircuitState,
    /// Total number of failures
    pub failures: u64,
    /// Total number of successes
    pub successes: u64,
    /// Total number of rejected requests
    pub rejects: u64,
    /// Number of state transitions
    pub state_changes: u64,
    /// Timestamp of last failure
    pub last_failure_time: Option<Instant>,
    /// Timestamp of last success
    pub last_success_time: Option<Instant>,
}

/// Circuit breaker for fault tolerance
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Mutex<CircuitState>,
    failure_count: AtomicU64,
    success_count: AtomicU64,
    reject_count: AtomicU64,
    state_changes: AtomicU64,
    last_failure_time: Mutex<Option<Instant>>,
    last_success_time: Mutex<Option<Instant>>,
    last_state_change: Mutex<Instant>,
    half_open_calls: AtomicU64,
}

impl std::fmt::Debug for CircuitBreaker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CircuitBreaker")
            .field("config", &self.config)
            .field("state", &*self.state.lock())
            .field("stats", &self.stats())
            .finish()
    }
}

impl CircuitBreaker {
    /// Create new circuit breaker
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: Mutex::new(CircuitState::Closed),
            failure_count: AtomicU64::new(0),
            success_count: AtomicU64::new(0),
            reject_count: AtomicU64::new(0),
            state_changes: AtomicU64::new(0),
            last_failure_time: Mutex::new(None),
            last_success_time: Mutex::new(None),
            last_state_change: Mutex::new(Instant::now()),
            half_open_calls: AtomicU64::new(0),
        }
    }

    /// Check if request should be allowed
    pub fn allow(&self) -> bool {
        let mut state = self.state.lock();

        match *state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has passed
                let last_change = *self.last_state_change.lock();
                if last_change.elapsed() >= self.config.timeout {
                    tracing::info!("Circuit breaker transitioning to HalfOpen");
                    *state = CircuitState::HalfOpen;
                    self.half_open_calls.store(0, Ordering::SeqCst);
                    self.state_changes.fetch_add(1, Ordering::SeqCst);
                    true
                } else {
                    self.reject_count.fetch_add(1, Ordering::SeqCst);
                    false
                }
            }
            CircuitState::HalfOpen => {
                let calls = self.half_open_calls.fetch_add(1, Ordering::SeqCst);
                if calls < self.config.half_open_max_calls as u64 {
                    true
                } else {
                    self.reject_count.fetch_add(1, Ordering::SeqCst);
                    false
                }
            }
        }
    }

    /// Record successful operation
    pub fn record_success(&self) {
        self.success_count.fetch_add(1, Ordering::SeqCst);
        *self.last_success_time.lock() = Some(Instant::now());

        let mut state = self.state.lock();

        match *state {
            CircuitState::HalfOpen => {
                // Count consecutive successes
                let successes = self.success_count.load(Ordering::SeqCst);
                if successes >= self.config.success_threshold as u64 {
                    tracing::info!("Circuit breaker transitioning to Closed");
                    *state = CircuitState::Closed;
                    self.failure_count.store(0, Ordering::SeqCst);
                    self.state_changes.fetch_add(1, Ordering::SeqCst);
                    *self.last_state_change.lock() = Instant::now();
                }
            }
            CircuitState::Closed => {
                // Reset failure count on success in closed state
                self.failure_count.store(0, Ordering::SeqCst);
            }
            _ => {}
        }
    }

    /// Record failed operation
    pub fn record_failure(&self) {
        self.failure_count.fetch_add(1, Ordering::SeqCst);
        *self.last_failure_time.lock() = Some(Instant::now());

        let mut state = self.state.lock();

        match *state {
            CircuitState::Closed => {
                let failures = self.failure_count.load(Ordering::SeqCst);
                if failures >= self.config.failure_threshold as u64 {
                    tracing::warn!(
                        "Circuit breaker transitioning to Open after {} failures",
                        failures
                    );
                    *state = CircuitState::Open;
                    self.state_changes.fetch_add(1, Ordering::SeqCst);
                    *self.last_state_change.lock() = Instant::now();
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in half-open immediately goes back to open
                tracing::warn!("Circuit breaker returning to Open from HalfOpen");
                *state = CircuitState::Open;
                self.state_changes.fetch_add(1, Ordering::SeqCst);
                *self.last_state_change.lock() = Instant::now();
            }
            _ => {}
        }
    }

    /// Get current state
    pub fn state(&self) -> CircuitState {
        *self.state.lock()
    }

    /// Get statistics
    pub fn stats(&self) -> CircuitStats {
        CircuitStats {
            state: self.state(),
            failures: self.failure_count.load(Ordering::SeqCst),
            successes: self.success_count.load(Ordering::SeqCst),
            rejects: self.reject_count.load(Ordering::SeqCst),
            state_changes: self.state_changes.load(Ordering::SeqCst),
            last_failure_time: *self.last_failure_time.lock(),
            last_success_time: *self.last_success_time.lock(),
        }
    }

    /// Reset circuit breaker to closed state
    pub fn reset(&self) {
        let mut state = self.state.lock();
        *state = CircuitState::Closed;
        self.failure_count.store(0, Ordering::SeqCst);
        self.success_count.store(0, Ordering::SeqCst);
        self.reject_count.store(0, Ordering::SeqCst);
        *self.last_state_change.lock() = Instant::now();
        tracing::info!("Circuit breaker manually reset");
    }

    /// Force circuit breaker to open state
    pub fn force_open(&self) {
        let mut state = self.state.lock();
        *state = CircuitState::Open;
        self.state_changes.fetch_add(1, Ordering::SeqCst);
        *self.last_state_change.lock() = Instant::now();
        tracing::info!("Circuit breaker manually opened");
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(CircuitBreakerConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_closed_allows_requests() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::fast_recovery());
        assert!(cb.allow());
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_opens_after_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);

        // Record failures
        for _ in 0..3 {
            assert!(cb.allow());
            cb.record_failure();
        }

        // Circuit should be open now
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.allow());
    }

    #[test]
    fn test_circuit_successes_reset_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 5,
            ..Default::default()
        };
        let cb = CircuitBreaker::new(config);

        // Record some failures
        for _ in 0..3 {
            cb.record_failure();
        }

        // Success should reset failure count
        cb.record_success();

        // Should still be closed
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.allow());
    }

    #[test]
    fn test_stats_tracking() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig::default());

        cb.record_success();
        cb.record_success();
        cb.record_failure();

        let stats = cb.stats();
        assert_eq!(stats.successes, 2);
        assert_eq!(stats.failures, 1);
    }
}
