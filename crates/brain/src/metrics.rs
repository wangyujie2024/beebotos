//! Performance Metrics Collection
//!
//! Provides comprehensive performance monitoring and metrics collection
//! for the brain module.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Global metrics collector (singleton)
static METRICS: once_cell::sync::Lazy<Arc<Mutex<MetricsCollector>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(MetricsCollector::new())));

/// Get the global metrics collector
pub fn global_metrics() -> Arc<Mutex<MetricsCollector>> {
    METRICS.clone()
}

/// Metrics collector for brain operations
#[derive(Debug, Clone)]
pub struct MetricsCollector {
    /// Operation counters
    counters: HashMap<String, u64>,
    /// Operation timings (in milliseconds)
    timings: HashMap<String, Vec<f64>>,
    /// Gauge values (current state)
    gauges: HashMap<String, f64>,
    /// Histogram buckets
    histograms: HashMap<String, Histogram>,
    /// When metrics collection started
    start_time: Instant,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self {
            counters: HashMap::new(),
            timings: HashMap::new(),
            gauges: HashMap::new(),
            histograms: HashMap::new(),
            start_time: Instant::now(),
        }
    }

    /// Increment a counter
    pub fn increment_counter(&mut self, name: &str) {
        *self.counters.entry(name.to_string()).or_insert(0) += 1;
    }

    /// Increment a counter by a specific amount
    pub fn add_counter(&mut self, name: &str, value: u64) {
        *self.counters.entry(name.to_string()).or_insert(0) += value;
    }

    /// Record a timing in milliseconds
    pub fn record_timing(&mut self, name: &str, duration_ms: f64) {
        self.timings
            .entry(name.to_string())
            .or_default()
            .push(duration_ms);

        // Keep only last 1000 measurements to prevent unbounded growth
        if let Some(v) = self.timings.get_mut(name) {
            if v.len() > 1000 {
                v.remove(0);
            }
        }
    }

    /// Set a gauge value
    pub fn set_gauge(&mut self, name: &str, value: f64) {
        self.gauges.insert(name.to_string(), value);
    }

    /// Record a value in a histogram
    pub fn record_histogram(&mut self, name: &str, value: f64) {
        self.histograms
            .entry(name.to_string())
            .or_default()
            .record(value);
    }

    /// Get counter value
    pub fn get_counter(&self, name: &str) -> u64 {
        self.counters.get(name).copied().unwrap_or(0)
    }

    /// Get average timing
    pub fn get_average_timing(&self, name: &str) -> Option<f64> {
        self.timings.get(name).map(|v| {
            if v.is_empty() {
                0.0
            } else {
                v.iter().sum::<f64>() / v.len() as f64
            }
        })
    }

    /// Record a generic metric value (alias for record_timing, for
    /// compatibility)
    pub fn record(&mut self, name: &str, value: f64) {
        self.record_timing(name, value);
    }

    /// Get average value for a metric (alias for get_average_timing, for
    /// compatibility)
    pub fn average(&self, name: &str) -> Option<f64> {
        self.get_average_timing(name)
    }

    /// Get timing statistics (min, max, avg, p95, p99)
    pub fn get_timing_stats(&self, name: &str) -> Option<TimingStats> {
        self.timings.get(name).map(|v| {
            if v.is_empty() {
                return TimingStats::default();
            }

            let mut sorted = v.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

            let len = sorted.len();
            TimingStats {
                min: sorted[0],
                max: sorted[len - 1],
                avg: sorted.iter().sum::<f64>() / len as f64,
                p95: sorted[((len as f64 * 0.95) as usize).min(len - 1)],
                p99: sorted[((len as f64 * 0.99) as usize).min(len - 1)],
                count: len as u64,
            }
        })
    }

    /// Get gauge value
    pub fn get_gauge(&self, name: &str) -> Option<f64> {
        self.gauges.get(name).copied()
    }

    /// Get histogram
    pub fn get_histogram(&self, name: &str) -> Option<&Histogram> {
        self.histograms.get(name)
    }

    /// Get all counter names
    pub fn counter_names(&self) -> Vec<String> {
        self.counters.keys().cloned().collect()
    }

    /// Get all timing names
    pub fn timing_names(&self) -> Vec<String> {
        self.timings.keys().cloned().collect()
    }

    /// Get uptime
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Get a snapshot of all metrics
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            counters: self.counters.clone(),
            gauges: self.gauges.clone(),
            timing_stats: self
                .timings
                .keys()
                .map(|k| (k.clone(), self.get_timing_stats(k).unwrap_or_default()))
                .collect(),
            histograms: self.histograms.clone(),
            uptime_secs: self.uptime().as_secs(),
        }
    }

    /// Clear all metrics
    pub fn clear(&mut self) {
        self.counters.clear();
        self.timings.clear();
        self.gauges.clear();
        self.histograms.clear();
        self.start_time = Instant::now();
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Single metric data point (for compatibility with old monitoring module)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricPoint {
    pub value: f64,
    pub timestamp: u64,
}

/// Timing statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct TimingStats {
    pub min: f64,
    pub max: f64,
    pub avg: f64,
    pub p95: f64,
    pub p99: f64,
    pub count: u64,
}

/// Histogram for value distribution
#[derive(Debug, Clone)]
pub struct Histogram {
    buckets: Vec<(f64, u64)>, // (upper_bound, count)
    total_count: u64,
    sum: f64,
}

impl Histogram {
    /// Create a new histogram with linear buckets
    pub fn new_linear(start: f64, width: f64, count: usize) -> Self {
        let mut buckets = Vec::with_capacity(count);
        for i in 0..count {
            buckets.push((start + width * (i + 1) as f64, 0));
        }
        Self {
            buckets,
            total_count: 0,
            sum: 0.0,
        }
    }

    /// Create a new histogram with exponential buckets
    pub fn new_exponential(start: f64, factor: f64, count: usize) -> Self {
        let mut buckets = Vec::with_capacity(count);
        let mut bound = start;
        for _ in 0..count {
            bound *= factor;
            buckets.push((bound, 0));
        }
        Self {
            buckets,
            total_count: 0,
            sum: 0.0,
        }
    }

    /// Record a value
    pub fn record(&mut self, value: f64) {
        self.total_count += 1;
        self.sum += value;

        for (bound, count) in &mut self.buckets {
            if value <= *bound {
                *count += 1;
                break;
            }
        }
    }

    /// Get percentile
    pub fn percentile(&self, p: f64) -> f64 {
        if self.total_count == 0 {
            return 0.0;
        }

        let target = (self.total_count as f64 * p / 100.0) as u64;
        let mut cumulative = 0;

        for (bound, count) in &self.buckets {
            cumulative += count;
            if cumulative >= target {
                return *bound;
            }
        }

        self.buckets.last().map(|(b, _)| *b).unwrap_or(0.0)
    }

    /// Get average
    pub fn average(&self) -> f64 {
        if self.total_count == 0 {
            0.0
        } else {
            self.sum / self.total_count as f64
        }
    }
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new_linear(0.0, 10.0, 10)
    }
}

/// Metrics snapshot for reporting
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub counters: HashMap<String, u64>,
    pub gauges: HashMap<String, f64>,
    pub timing_stats: HashMap<String, TimingStats>,
    pub histograms: HashMap<String, Histogram>,
    pub uptime_secs: u64,
}

/// Timer for measuring operation duration
pub struct Timer {
    start: Instant,
    name: String,
}

impl Timer {
    /// Start a new timer
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            start: Instant::now(),
            name: name.into(),
        }
    }

    /// Record the elapsed time and return the duration
    pub fn record(self) -> Duration {
        let elapsed = self.start.elapsed();
        if let Ok(mut metrics) = METRICS.lock() {
            metrics.record_timing(&self.name, elapsed.as_secs_f64() * 1000.0);
        }
        elapsed
    }

    /// Get elapsed time without recording
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

/// Convenience macro for timing a block
#[macro_export]
macro_rules! timed {
    ($name:expr, $block:expr) => {{
        let _timer = $crate::metrics::Timer::new($name);
        $block
    }};
}

/// Convenience function to increment a counter
pub fn increment_counter(name: &str) {
    if let Ok(mut metrics) = METRICS.lock() {
        metrics.increment_counter(name);
    }
}

/// Convenience function to record a timing
pub fn record_timing(name: &str, duration_ms: f64) {
    if let Ok(mut metrics) = METRICS.lock() {
        metrics.record_timing(name, duration_ms);
    }
}

/// Convenience function to set a gauge
pub fn set_gauge(name: &str, value: f64) {
    if let Ok(mut metrics) = METRICS.lock() {
        metrics.set_gauge(name, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter() {
        let mut m = MetricsCollector::new();
        m.increment_counter("test");
        m.increment_counter("test");
        assert_eq!(m.get_counter("test"), 2);

        m.add_counter("test", 3);
        assert_eq!(m.get_counter("test"), 5);
    }

    #[test]
    fn test_timing() {
        let mut m = MetricsCollector::new();
        m.record_timing("op", 10.0);
        m.record_timing("op", 20.0);

        let stats = m.get_timing_stats("op").unwrap();
        assert_eq!(stats.avg, 15.0);
        assert_eq!(stats.min, 10.0);
        assert_eq!(stats.max, 20.0);
        assert_eq!(stats.count, 2);
    }

    #[test]
    fn test_gauge() {
        let mut m = MetricsCollector::new();
        m.set_gauge("memory", 1024.0);
        assert_eq!(m.get_gauge("memory"), Some(1024.0));
    }

    #[test]
    fn test_histogram() {
        let mut h = Histogram::new_linear(0.0, 10.0, 10);
        h.record(5.0);
        h.record(15.0);
        h.record(25.0);

        assert_eq!(h.total_count, 3);
        assert!(h.average() > 0.0);
    }

    #[test]
    fn test_timer() {
        let timer = Timer::new("test_op");
        std::thread::sleep(Duration::from_millis(1));
        let elapsed = timer.record();
        assert!(elapsed.as_millis() >= 1);
    }

    #[test]
    fn test_snapshot() {
        let mut m = MetricsCollector::new();
        m.increment_counter("c1");
        m.set_gauge("g1", 100.0);

        let snapshot = m.snapshot();
        assert_eq!(snapshot.counters.get("c1"), Some(&1));
        assert_eq!(snapshot.gauges.get("g1"), Some(&100.0));
    }
}
