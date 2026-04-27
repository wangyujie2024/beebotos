//! Rate Limiting Module
//!
//! Production-ready rate limiting with multiple algorithms:
//! - Fixed Window: Simple, memory efficient
//! - Token Bucket: Smooth rate limiting with burst support
//! - Sliding Window: Accurate distribution without boundary bursts
//!
//! All implementations use async-aware synchronization primitives.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub mod sliding_window;
pub mod token_bucket;

/// Rate limit configuration
#[derive(Debug, Clone, Serialize, Deserialize, validator::Validate)]
pub struct RateLimitConfig {
    /// Maximum requests per second (sustained rate)
    #[validate(range(min = 1, max = 100000))]
    pub requests_per_second: u32,
    /// Maximum burst size (short-term allowance)
    #[validate(range(min = 1, max = 1000000))]
    pub burst_size: u32,
    /// Cooldown/window size in seconds
    #[validate(range(min = 1, max = 3600))]
    pub cooldown_seconds: u32,
    /// Whether rate limiting is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_second: 10,
            burst_size: 20,
            cooldown_seconds: 60,
            enabled: true,
        }
    }
}

impl RateLimitConfig {
    /// Create a config for high-throughput endpoints
    pub fn high_throughput() -> Self {
        Self {
            requests_per_second: 1000,
            burst_size: 2000,
            cooldown_seconds: 1,
            enabled: true,
        }
    }

    /// Create a config for strict limiting
    pub fn strict() -> Self {
        Self {
            requests_per_second: 1,
            burst_size: 3,
            cooldown_seconds: 60,
            enabled: true,
        }
    }

    /// Disable rate limiting
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }
}

/// Core rate limiter trait
#[async_trait::async_trait]
pub trait RateLimiter: Send + Sync {
    /// Check if request should be allowed
    async fn allow(&self, key: &str) -> bool;

    /// Check and return detailed result
    async fn check(&self, key: &str) -> RateLimitResult;

    /// Get remaining quota
    async fn remaining(&self, key: &str) -> u32;

    /// Get time until reset
    async fn reset_time(&self, key: &str) -> Duration;
}

/// Rate limit check result
#[derive(Debug, Clone)]
pub struct RateLimitResult {
    /// Whether the request is allowed
    pub allowed: bool,
    /// Remaining requests in current window
    pub remaining: u32,
    /// Time until rate limit resets
    pub reset_time: Duration,
    /// Time to wait before retry (if denied)
    pub retry_after: Option<Duration>,
}

impl RateLimitResult {
    /// Create headers for HTTP response
    pub fn to_headers(&self) -> Vec<(String, String)> {
        let mut headers = vec![
            (
                "X-RateLimit-Remaining".to_string(),
                self.remaining.to_string(),
            ),
            (
                "X-RateLimit-Reset".to_string(),
                self.reset_time.as_secs().to_string(),
            ),
        ];

        if let Some(retry_after) = self.retry_after {
            headers.push(("Retry-After".to_string(), retry_after.as_secs().to_string()));
        }

        if self.allowed {
            headers.push(("X-RateLimit-Limit".to_string(), "allowed".to_string()));
        }

        headers
    }
}

/// Rate limit manager supporting multiple limiters per route
///
/// 🟡 MEDIUM PERFORMANCE FIX: Uses DashMap directly without outer RwLock
/// DashMap is already thread-safe, so the extra RwLock was unnecessary overhead
#[derive(Clone)]
pub struct RateLimitManager {
    /// Route-specific limiters - using DashMap directly for lock-free
    /// concurrent access
    limiters: Arc<dashmap::DashMap<String, Arc<dyn RateLimiter>>>,
    /// Default fallback limiter
    default_limiter: Arc<dyn RateLimiter>,
}

impl std::fmt::Debug for RateLimitManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimitManager").finish_non_exhaustive()
    }
}

impl RateLimitManager {
    /// Create new manager with default limiter
    pub fn new(default: Arc<dyn RateLimiter>) -> Self {
        Self {
            // 🟡 MEDIUM PERFORMANCE FIX: Use DashMap directly, no outer RwLock needed
            limiters: Arc::new(dashmap::DashMap::new()),
            default_limiter: default,
        }
    }

    /// Register a limiter for specific route pattern
    ///
    /// 🟡 MEDIUM PERFORMANCE FIX: No async needed, DashMap handles concurrency
    pub fn register(&self, route: impl Into<String>, limiter: Arc<dyn RateLimiter>) {
        self.limiters.insert(route.into(), limiter);
    }

    /// Check rate limit for route and client
    ///
    /// 🟡 MEDIUM PERFORMANCE FIX: Lock-free read using DashMap
    pub async fn check(&self, route: &str, client_id: &str) -> RateLimitResult {
        // Find most specific matching route - lock-free with DashMap
        let limiter = self
            .limiters
            .iter()
            .filter(|entry| route.starts_with(entry.key()))
            .max_by_key(|entry| entry.key().len())
            .map(|entry| entry.value().clone())
            .unwrap_or_else(|| self.default_limiter.clone());

        limiter.check(client_id).await
    }

    /// Quick allow check
    pub async fn allow(&self, route: &str, client_id: &str) -> bool {
        self.check(route, client_id).await.allowed
    }

    /// Get or create per-client limiter (for tiered limiting)
    pub async fn get_client_limiter(&self, client_tier: ClientTier) -> Arc<dyn RateLimiter> {
        match client_tier {
            ClientTier::Premium => {
                Arc::new(token_bucket::TokenBucketRateLimiter::new(1000.0, 2000))
            }
            ClientTier::Standard => self.default_limiter.clone(),
            ClientTier::Free => Arc::new(token_bucket::TokenBucketRateLimiter::new(10.0, 20)),
        }
    }
}

/// Client tier for differentiated rate limiting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientTier {
    /// Free tier with lowest limits
    Free,
    /// Standard tier with normal limits
    Standard,
    /// Premium tier with highest limits
    Premium,
}

impl ClientTier {
    /// Parse from string
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "premium" => Self::Premium,
            "free" => Self::Free,
            _ => Self::Standard,
        }
    }
}

/// Fixed window rate limiter (simple but has boundary issues)
///
/// 🟡 MEDIUM PERFORMANCE FIX: Uses DashMap directly without outer RwLock
#[derive(Debug, Clone)]
pub struct FixedWindowRateLimiter {
    /// Window duration
    window_size: Duration,
    /// Max requests per window
    max_requests: u32,
    /// Client window states: (count, reset_at)
    /// 🟡 MEDIUM PERFORMANCE FIX: Direct DashMap for lock-free access
    windows: Arc<dashmap::DashMap<String, WindowState>>,
}

#[derive(Debug, Clone, Copy)]
struct WindowState {
    count: u32,
    reset_at: tokio::time::Instant,
}

impl FixedWindowRateLimiter {
    /// Create new fixed window limiter
    pub fn new(max_requests: u32, window_size: Duration) -> Self {
        Self {
            window_size,
            max_requests,
            // 🟡 MEDIUM PERFORMANCE FIX: Use DashMap directly
            windows: Arc::new(dashmap::DashMap::new()),
        }
    }

    /// Create from config
    pub fn from_config(config: &RateLimitConfig) -> Self {
        Self::new(
            config.requests_per_second * config.cooldown_seconds,
            Duration::from_secs(config.cooldown_seconds as u64),
        )
    }

    /// Get or create window for client
    ///
    /// 🟡 MEDIUM PERFORMANCE FIX: Lock-free access using DashMap
    #[allow(dead_code)]
    fn get_window(&self, key: &str) -> WindowState {
        let now = tokio::time::Instant::now();

        if let Some(entry) = self.windows.get(key) {
            let state = *entry.value();
            if state.reset_at > now {
                return state;
            }
        }

        // Create new window
        let new_state = WindowState {
            count: 0,
            reset_at: now + self.window_size,
        };

        self.windows.insert(key.to_string(), new_state);
        new_state
    }
}

#[async_trait::async_trait]
impl RateLimiter for FixedWindowRateLimiter {
    /// 🟡 MEDIUM PERFORMANCE FIX: Lock-free allow check using DashMap
    async fn allow(&self, key: &str) -> bool {
        let now = tokio::time::Instant::now();

        let mut state = *self
            .windows
            .entry(key.to_string())
            .or_insert(WindowState {
                count: 0,
                reset_at: now + self.window_size,
            })
            .value();

        // Check if window has expired
        if now > state.reset_at {
            state.count = 0;
            state.reset_at = now + self.window_size;
        }

        // Check quota
        if state.count < self.max_requests {
            state.count += 1;
            self.windows.insert(key.to_string(), state);
            true
        } else {
            false
        }
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
        let now = tokio::time::Instant::now();

        if let Some(entry) = self.windows.get(key) {
            let state = entry.value();
            if now < state.reset_at {
                return self.max_requests.saturating_sub(state.count);
            }
        }

        self.max_requests
    }

    /// 🟡 MEDIUM PERFORMANCE FIX: Lock-free reset time check using DashMap
    async fn reset_time(&self, key: &str) -> Duration {
        let now = tokio::time::Instant::now();

        if let Some(entry) = self.windows.get(key) {
            let state = entry.value();
            if state.reset_at > now {
                return state.reset_at - now;
            }
        }

        Duration::ZERO
    }
}

/// No-op rate limiter (always allows)
pub struct NoopRateLimiter;

#[async_trait::async_trait]
impl RateLimiter for NoopRateLimiter {
    async fn allow(&self, _key: &str) -> bool {
        true
    }

    async fn check(&self, _key: &str) -> RateLimitResult {
        RateLimitResult {
            allowed: true,
            remaining: u32::MAX,
            reset_time: Duration::ZERO,
            retry_after: None,
        }
    }

    async fn remaining(&self, _key: &str) -> u32 {
        u32::MAX
    }

    async fn reset_time(&self, _key: &str) -> Duration {
        Duration::ZERO
    }
}

#[cfg(test)]
mod tests {
    use tokio::time::{sleep, Duration};

    use super::*;

    #[tokio::test]
    async fn test_fixed_window_basic() {
        let limiter = FixedWindowRateLimiter::new(2, Duration::from_secs(60));

        assert!(limiter.allow("client1").await);
        assert!(limiter.allow("client1").await);
        assert!(!limiter.allow("client1").await);

        assert_eq!(limiter.remaining("client1").await, 0);
    }

    #[tokio::test]
    async fn test_fixed_window_reset() {
        let limiter = FixedWindowRateLimiter::new(1, Duration::from_millis(100));

        // First request
        assert!(limiter.allow("client1").await);
        assert!(!limiter.allow("client1").await);

        // Wait for window reset
        sleep(Duration::from_millis(150)).await;

        // Should be allowed again
        assert!(limiter.allow("client1").await);
    }

    #[tokio::test]
    async fn test_manager_route_matching() {
        let default = Arc::new(FixedWindowRateLimiter::new(10, Duration::from_secs(60)));
        let manager = RateLimitManager::new(default);

        // Register specific route
        let api_limiter = Arc::new(FixedWindowRateLimiter::new(5, Duration::from_secs(60)));
        manager.register("/api/v1", api_limiter);

        // Should use specific limiter for matching route
        let result = manager.check("/api/v1/users", "client1").await;
        assert!(result.allowed);
        assert_eq!(result.remaining, 4); // 5 - 1 = 4
    }

    #[tokio::test]
    async fn test_rate_limit_result_headers() {
        let result = RateLimitResult {
            allowed: true,
            remaining: 42,
            reset_time: Duration::from_secs(60),
            retry_after: None,
        };

        let headers = result.to_headers();
        assert!(headers.iter().any(|(k, _)| k == "X-RateLimit-Remaining"));
        assert!(headers.iter().any(|(k, _)| k == "X-RateLimit-Reset"));
    }

    #[tokio::test]
    async fn test_noop_limiter() {
        let limiter = NoopRateLimiter;

        assert!(limiter.allow("any").await);
        assert_eq!(limiter.remaining("any").await, u32::MAX);

        let result = limiter.check("any").await;
        assert!(result.allowed);
    }

    #[tokio::test]
    async fn test_client_tier() {
        assert_eq!(ClientTier::from_str("premium"), ClientTier::Premium);
        assert_eq!(ClientTier::from_str("free"), ClientTier::Free);
        assert_eq!(ClientTier::from_str("unknown"), ClientTier::Standard);
    }
}
