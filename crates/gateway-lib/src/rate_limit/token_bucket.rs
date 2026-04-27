//! Token Bucket Rate Limiter
//!
//! Smooth rate limiting algorithm that allows bursts up to bucket capacity.
//! Production-ready implementation with async support and proper concurrency
//! control.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{RateLimitConfig, RateLimitResult, RateLimiter};

/// Token bucket state for a single client
#[derive(Debug, Clone)]
struct BucketState {
    /// Current number of tokens in the bucket
    tokens: f64,
    /// Last time tokens were added
    last_update: Instant,
    /// Maximum bucket capacity
    capacity: f64,
    /// Token refill rate per second
    refill_rate: f64,
}

impl BucketState {
    fn new(capacity: f64, refill_rate: f64) -> Self {
        Self {
            tokens: capacity,
            last_update: Instant::now(),
            capacity,
            refill_rate,
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();
        let tokens_to_add = elapsed * self.refill_rate;

        self.tokens = (self.tokens + tokens_to_add).min(self.capacity);
        self.last_update = now;
    }

    /// Try to consume tokens, returns true if successful
    fn consume(&mut self, tokens: f64) -> bool {
        self.refill();

        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }

    /// Get current token count
    fn tokens(&mut self) -> f64 {
        self.refill();
        self.tokens
    }

    /// Calculate time until next token is available
    fn time_until_next_token(&self) -> Duration {
        if self.tokens >= 1.0 {
            return Duration::ZERO;
        }

        let tokens_needed = 1.0 - self.tokens;
        let seconds_needed = tokens_needed / self.refill_rate;
        Duration::from_secs_f64(seconds_needed)
    }
}

/// Token bucket rate limiter
///
/// 🟡 MEDIUM PERFORMANCE FIX: Uses DashMap directly without outer RwLock
/// for lock-free concurrent access to bucket states.
///
/// # Example
/// ```rust,ignore
/// use std::time::Duration;
/// use beebotos_gateway_lib::rate_limit::token_bucket::TokenBucketRateLimiter;
/// use beebotos_gateway_lib::rate_limit::RateLimiter;
///
/// async fn example() {
///     let limiter = TokenBucketRateLimiter::new(10.0, 20); // 10 req/s, burst 20
///     assert!(limiter.allow("client1").await);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct TokenBucketRateLimiter {
    /// Default bucket capacity (burst size)
    capacity: f64,
    /// Token refill rate per second
    refill_rate: f64,
    /// Token cost per request (default: 1.0)
    token_cost: f64,
    /// Bucket states per client - using DashMap for lock-free access
    /// 🟡 MEDIUM PERFORMANCE FIX: No outer RwLock needed, DashMap is
    /// thread-safe
    buckets: Arc<dashmap::DashMap<String, BucketState>>,
}

impl TokenBucketRateLimiter {
    /// Create a new token bucket rate limiter
    ///
    /// # Arguments
    /// * `refill_rate` - Tokens added per second (sustained rate)
    /// * `capacity` - Maximum bucket size (burst capacity)
    pub fn new(refill_rate: f64, capacity: u32) -> Self {
        Self {
            capacity: capacity as f64,
            refill_rate,
            token_cost: 1.0,
            // 🟡 MEDIUM PERFORMANCE FIX: Use DashMap directly
            buckets: Arc::new(dashmap::DashMap::new()),
        }
    }

    /// Create from RateLimitConfig
    pub fn from_config(config: &RateLimitConfig) -> Self {
        Self::new(config.requests_per_second as f64, config.burst_size)
    }

    /// Set custom token cost per request
    pub fn with_token_cost(mut self, cost: f64) -> Self {
        self.token_cost = cost.max(0.01);
        self
    }

    /// Get or create bucket for a client
    ///
    /// 🟡 MEDIUM PERFORMANCE FIX: Lock-free access using DashMap
    #[allow(dead_code)]
    fn get_bucket(&self, key: &str) -> BucketState {
        // Try to get existing bucket - lock-free with DashMap
        if let Some(entry) = self.buckets.get(key) {
            let mut state = entry.value().clone();
            state.refill();
            return state;
        }

        // Create new bucket
        let new_state = BucketState::new(self.capacity, self.refill_rate);
        self.buckets.insert(key.to_string(), new_state.clone());

        new_state
    }

    /// Clean up expired buckets (optional maintenance)
    ///
    /// 🟡 MEDIUM PERFORMANCE FIX: Lock-free cleanup using DashMap
    pub fn cleanup(&self) {
        let now = Instant::now();
        let stale_duration = Duration::from_secs(3600); // 1 hour

        self.buckets
            .retain(|_key, state| now.duration_since(state.last_update) < stale_duration);
    }
}

#[async_trait::async_trait]
impl RateLimiter for TokenBucketRateLimiter {
    /// 🟡 MEDIUM PERFORMANCE FIX: Lock-free allow check using DashMap
    async fn allow(&self, key: &str) -> bool {
        let mut state = self
            .buckets
            .entry(key.to_string())
            .or_insert_with(|| BucketState::new(self.capacity, self.refill_rate))
            .value()
            .clone();

        let allowed = state.consume(self.token_cost);

        // Update the bucket state
        self.buckets.insert(key.to_string(), state);

        allowed
    }

    async fn check(&self, key: &str) -> RateLimitResult {
        let allowed = self.allow(key).await;
        let remaining = self.remaining(key).await;
        let reset_time = self.reset_time(key).await;

        RateLimitResult {
            allowed,
            remaining,
            reset_time,
            retry_after: if allowed { None } else { Some(reset_time) },
        }
    }

    /// 🟡 MEDIUM PERFORMANCE FIX: Lock-free remaining check using DashMap
    async fn remaining(&self, key: &str) -> u32 {
        let mut state = self
            .buckets
            .entry(key.to_string())
            .or_insert_with(|| BucketState::new(self.capacity, self.refill_rate))
            .value()
            .clone();

        state.tokens() as u32
    }

    /// 🟡 MEDIUM PERFORMANCE FIX: Lock-free reset time check using DashMap
    async fn reset_time(&self, key: &str) -> Duration {
        if let Some(entry) = self.buckets.get(key) {
            let state = entry.value();
            return state.time_until_next_token();
        }

        Duration::ZERO
    }
}

#[cfg(test)]
mod tests {
    use tokio::time::{sleep, Duration};

    use super::*;

    #[tokio::test]
    async fn test_token_bucket_basic() {
        let limiter = TokenBucketRateLimiter::new(10.0, 5);

        // Should allow burst up to capacity
        for i in 0..5 {
            assert!(
                limiter.allow("client1").await,
                "Request {} should be allowed",
                i
            );
        }

        // Should reject when bucket is empty
        assert!(!limiter.allow("client1").await);
    }

    #[tokio::test]
    async fn test_token_bucket_refill() {
        let limiter = TokenBucketRateLimiter::new(100.0, 2); // 100 req/s, burst 2

        // Exhaust bucket
        assert!(limiter.allow("client1").await);
        assert!(limiter.allow("client1").await);
        assert!(!limiter.allow("client1").await);

        // Wait for refill (100 tokens/s = 1 token per 10ms)
        sleep(Duration::from_millis(20)).await;

        // Should have at least 1 token now
        assert!(limiter.allow("client1").await);
    }

    #[tokio::test]
    async fn test_different_clients() {
        let limiter = TokenBucketRateLimiter::new(10.0, 3);

        // Client 1 exhausts their bucket
        assert!(limiter.allow("client1").await);
        assert!(limiter.allow("client1").await);
        assert!(limiter.allow("client1").await);
        assert!(!limiter.allow("client1").await);

        // Client 2 should still have full bucket
        assert!(limiter.allow("client2").await);
        assert!(limiter.allow("client2").await);
        assert!(limiter.allow("client2").await);
    }

    #[tokio::test]
    async fn test_remaining_count() {
        let limiter = TokenBucketRateLimiter::new(10.0, 5);

        assert_eq!(limiter.remaining("client1").await, 5);

        limiter.allow("client1").await;
        assert_eq!(limiter.remaining("client1").await, 4);

        limiter.allow("client1").await;
        assert_eq!(limiter.remaining("client1").await, 3);
    }

    #[tokio::test]
    async fn test_check_result() {
        let limiter = TokenBucketRateLimiter::new(10.0, 2);

        let result = limiter.check("client1").await;
        assert!(result.allowed);
        assert_eq!(result.remaining, 1);

        limiter.allow("client1").await;
        let result = limiter.check("client1").await;
        assert!(!result.allowed);
        assert!(result.retry_after.is_some());
    }
}
