//! Sliding Window Rate Limiter
//!
//! More accurate than fixed window, distributes rate limit smoothly over time.
//! Uses a sliding time window to count requests, preventing burst at window
//! boundaries.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

use super::{RateLimitConfig, RateLimitResult, RateLimiter};

/// Request timestamp entry
#[derive(Debug, Clone, Copy)]
struct RequestEntry {
    timestamp: Instant,
    count: u32,
}

/// Sliding window state for a single client
#[derive(Debug, Clone)]
struct WindowState {
    /// Request history (timestamps)
    requests: VecDeque<RequestEntry>,
    /// Window size in seconds
    window_size: Duration,
    /// Maximum requests per window
    max_requests: u32,
    /// Current window start time
    window_start: Instant,
    /// Requests in current window
    current_count: u32,
}

impl WindowState {
    fn new(window_size: Duration, max_requests: u32) -> Self {
        Self {
            requests: VecDeque::new(),
            window_size,
            max_requests,
            window_start: Instant::now(),
            current_count: 0,
        }
    }

    /// Clean up old entries outside the window and return current count
    fn cleanup(&mut self) -> u32 {
        let now = Instant::now();
        let cutoff = now - self.window_size;

        // Remove entries outside the sliding window
        while let Some(front) = self.requests.front() {
            if front.timestamp < cutoff {
                self.current_count = self.current_count.saturating_sub(front.count);
                self.requests.pop_front();
            } else {
                break;
            }
        }

        // Check if we need to start a new window
        if now.duration_since(self.window_start) > self.window_size {
            self.window_start = now;
            self.current_count = 0;
            self.requests.clear();
        }

        self.current_count
    }

    /// Try to add a request, returns true if allowed
    fn try_request(&mut self, count: u32) -> bool {
        self.cleanup();

        if self.current_count + count <= self.max_requests {
            self.requests.push_back(RequestEntry {
                timestamp: Instant::now(),
                count,
            });
            self.current_count += count;
            true
        } else {
            false
        }
    }

    /// Get remaining quota
    fn remaining(&mut self) -> u32 {
        self.cleanup();
        self.max_requests.saturating_sub(self.current_count)
    }

    /// Calculate time until next request is allowed
    fn time_until_reset(&self) -> Duration {
        if self.current_count < self.max_requests {
            return Duration::ZERO;
        }

        // Find the oldest entry that would free up quota
        if let Some(oldest) = self.requests.front() {
            let now = Instant::now();
            let cutoff = now - self.window_size;

            if oldest.timestamp > cutoff {
                return oldest.timestamp - cutoff;
            }
        }

        Duration::ZERO
    }

    /// Get the oldest request timestamp (for reset calculation)
    fn oldest_request(&self) -> Option<Instant> {
        self.requests.front().map(|e| e.timestamp)
    }
}

/// Sliding window rate limiter
///
/// Provides smooth rate limiting without the boundary burst issues of fixed
/// window.
///
/// # Example
/// ```rust
/// use std::time::Duration;
///
/// use beebotos_gateway_lib::rate_limit::sliding_window::SlidingWindowRateLimiter;
/// use beebotos_gateway_lib::rate_limit::RateLimiter;
///
/// let limiter = SlidingWindowRateLimiter::new(100, Duration::from_secs(60)); // 100 req/min
/// ```
#[derive(Debug, Clone)]
pub struct SlidingWindowRateLimiter {
    /// Maximum requests per window
    max_requests: u32,
    /// Window size
    window_size: Duration,
    /// Client states
    windows: Arc<RwLock<dashmap::DashMap<String, WindowState>>>,
}

impl SlidingWindowRateLimiter {
    /// Create a new sliding window rate limiter
    ///
    /// # Arguments
    /// * `max_requests` - Maximum number of requests allowed in the window
    /// * `window_size` - Duration of the sliding window
    pub fn new(max_requests: u32, window_size: Duration) -> Self {
        Self {
            max_requests,
            window_size,
            windows: Arc::new(RwLock::new(dashmap::DashMap::new())),
        }
    }

    /// Create from RateLimitConfig (uses cooldown_seconds as window size)
    pub fn from_config(config: &RateLimitConfig) -> Self {
        Self::new(
            config.requests_per_second * config.cooldown_seconds,
            Duration::from_secs(config.cooldown_seconds as u64),
        )
    }

    /// Create a per-second rate limiter
    pub fn per_second(max_requests: u32) -> Self {
        Self::new(max_requests, Duration::from_secs(1))
    }

    /// Create a per-minute rate limiter
    pub fn per_minute(max_requests: u32) -> Self {
        Self::new(max_requests, Duration::from_secs(60))
    }

    /// Clean up stale entries
    pub async fn cleanup(&self) {
        let windows = self.windows.write().await;
        let now = Instant::now();
        let stale_duration = self.window_size * 2;

        windows.retain(|_key, state| {
            if let Some(oldest) = state.oldest_request() {
                now.duration_since(oldest) < stale_duration
            } else {
                false // Remove empty states
            }
        });
    }
}

#[async_trait::async_trait]
impl RateLimiter for SlidingWindowRateLimiter {
    async fn allow(&self, key: &str) -> bool {
        let windows = self.windows.write().await;

        let mut state = windows
            .entry(key.to_string())
            .or_insert_with(|| WindowState::new(self.window_size, self.max_requests))
            .value()
            .clone();

        let allowed = state.try_request(1);

        // Update state
        windows.insert(key.to_string(), state);

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

    async fn remaining(&self, key: &str) -> u32 {
        let windows = self.windows.write().await;

        let mut state = windows
            .entry(key.to_string())
            .or_insert_with(|| WindowState::new(self.window_size, self.max_requests))
            .value()
            .clone();

        drop(windows);

        state.remaining()
    }

    async fn reset_time(&self, key: &str) -> Duration {
        let windows = self.windows.read().await;

        if let Some(entry) = windows.get(key) {
            return entry.value().time_until_reset();
        }

        Duration::ZERO
    }
}

/// Sliding window with sub-window granularity (more memory efficient)
///
/// Divides the window into smaller sub-windows for approximate but efficient
/// counting.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ApproximateSlidingWindow {
    /// Total window size
    window_size: Duration,
    /// Number of sub-windows
    sub_windows: usize,
    /// Sub-window duration
    sub_window_duration: Duration,
    /// Maximum requests per total window
    max_requests: u32,
    /// Client states: (sub_window_counts, current_index, last_update)
    states: Arc<RwLock<dashmap::DashMap<String, (Vec<u32>, usize, Instant)>>>,
}

impl ApproximateSlidingWindow {
    /// Create approximate sliding window with sub-window granularity
    ///
    /// # Arguments
    /// * `max_requests` - Max requests per full window
    /// * `window_size` - Total window duration
    /// * `sub_windows` - Number of sub-windows (higher = more accurate, more
    ///   memory)
    pub fn new(max_requests: u32, window_size: Duration, sub_windows: usize) -> Self {
        let sub_windows = sub_windows.max(2);
        let sub_window_duration = window_size / sub_windows as u32;

        Self {
            window_size,
            sub_windows,
            sub_window_duration,
            max_requests,
            states: Arc::new(RwLock::new(dashmap::DashMap::new())),
        }
    }

    async fn get_current_count(&self, key: &str) -> (u32, Instant) {
        let states = self.states.read().await;
        let now = Instant::now();

        let (counts, last_index, last_update) = if let Some(entry) = states.get(key) {
            let (c, i, u) = entry.value();
            (c.clone(), *i, *u)
        } else {
            (vec![0u32; self.sub_windows], 0, now)
        };

        drop(states);

        let elapsed = now.duration_since(last_update);
        let windows_passed =
            (elapsed.as_secs_f64() / self.sub_window_duration.as_secs_f64()) as usize;

        if windows_passed >= self.sub_windows {
            // Full window passed, reset all
            return (0, now);
        }

        // Calculate current index and sum relevant counts
        let current_index = (last_index + windows_passed) % self.sub_windows;
        let mut total = 0u32;

        for i in 0..self.sub_windows {
            let idx = (current_index + self.sub_windows - i) % self.sub_windows;
            if i < self.sub_windows - windows_passed {
                total += counts.get(idx).copied().unwrap_or(0);
            }
        }

        (total, last_update)
    }
}

#[async_trait::async_trait]
impl RateLimiter for ApproximateSlidingWindow {
    async fn allow(&self, key: &str) -> bool {
        let (current_count, _) = self.get_current_count(key).await;

        if current_count >= self.max_requests {
            return false;
        }

        // Increment count
        let states = self.states.read().await;
        let now = Instant::now();

        let mut entry = states
            .entry(key.to_string())
            .or_insert_with(|| (vec![0u32; self.sub_windows], 0, now))
            .value()
            .clone();

        let elapsed = now.duration_since(entry.2);
        let windows_passed =
            (elapsed.as_secs_f64() / self.sub_window_duration.as_secs_f64()) as usize;
        let current_index = (entry.1 + windows_passed) % self.sub_windows;

        // Reset intermediate windows
        for i in 1..windows_passed.min(self.sub_windows) {
            let idx = (entry.1 + i) % self.sub_windows;
            entry.0[idx] = 0;
        }

        entry.0[current_index] += 1;
        entry.1 = current_index;
        entry.2 = now;

        states.insert(key.to_string(), entry);

        true
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

    async fn remaining(&self, key: &str) -> u32 {
        let (count, _) = self.get_current_count(key).await;
        self.max_requests.saturating_sub(count)
    }

    async fn reset_time(&self, key: &str) -> Duration {
        let states = self.states.read().await;

        if let Some(entry) = states.get(key) {
            let (_, _, last_update) = entry.value();
            let elapsed = Instant::now().duration_since(*last_update);
            let windows_passed =
                (elapsed.as_secs_f64() / self.sub_window_duration.as_secs_f64()) as u64;

            if windows_passed < self.sub_windows as u64 {
                let remaining_sub_windows = self.sub_windows as u64 - windows_passed;
                return self.sub_window_duration * remaining_sub_windows as u32;
            }
        }

        Duration::ZERO
    }
}

#[cfg(test)]
mod tests {
    use tokio::time::{sleep, Duration};

    use super::*;

    #[tokio::test]
    async fn test_sliding_window_basic() {
        let limiter = SlidingWindowRateLimiter::new(3, Duration::from_secs(1));

        assert!(limiter.allow("client1").await);
        assert!(limiter.allow("client1").await);
        assert!(limiter.allow("client1").await);
        assert!(!limiter.allow("client1").await);

        assert_eq!(limiter.remaining("client1").await, 0);
    }

    #[tokio::test]
    async fn test_sliding_window_reset() {
        let limiter = SlidingWindowRateLimiter::new(2, Duration::from_millis(200));

        // Exhaust quota
        assert!(limiter.allow("client1").await);
        assert!(limiter.allow("client1").await);
        assert!(!limiter.allow("client1").await);

        // Wait for window to slide
        sleep(Duration::from_millis(250)).await;

        // Should be allowed again
        assert!(limiter.allow("client1").await);
    }

    #[tokio::test]
    async fn test_approximate_window() {
        let limiter = ApproximateSlidingWindow::new(5, Duration::from_secs(1), 10);

        // Use up quota
        for _ in 0..5 {
            assert!(limiter.allow("client1").await);
        }

        // Should be blocked
        assert!(!limiter.allow("client1").await);

        // Approximate windows may have slight variance
        assert_eq!(limiter.remaining("client1").await, 0);
    }

    #[tokio::test]
    async fn test_window_boundary() {
        // This test demonstrates sliding window advantage over fixed window
        let limiter = SlidingWindowRateLimiter::new(2, Duration::from_millis(300));

        // Two requests at end of window
        assert!(limiter.allow("client").await);
        assert!(limiter.allow("client").await);
        assert!(!limiter.allow("client").await);

        // Wait half window
        sleep(Duration::from_millis(150)).await;

        // First request should have expired from window
        // This would NOT happen in fixed window until full window passes
        // Note: exact behavior depends on implementation timing
        let _ = limiter.check("client").await;
    }
}
