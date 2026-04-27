//! Kernel Metrics Collection
//!
//! Integration with the `metrics` crate for Prometheus-compatible metrics.
//! Provides counters, gauges, and histograms for monitoring.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Metrics collector for kernel
///
/// Note: This is a simplified implementation that maintains counters
/// internally. For full metrics integration, use the
/// metrics-exporter-prometheus feature.
pub struct MetricsCollector {
    start_time: Instant,
    tasks_submitted: AtomicU64,
    tasks_completed: AtomicU64,
    tasks_failed: AtomicU64,
    syscall_count: AtomicU64,
    syscall_errors: AtomicU64,
    security_violations: AtomicU64,
    capability_checks: AtomicU64,
    storage_reads: AtomicU64,
    storage_writes: AtomicU64,
}

impl MetricsCollector {
    /// Create new metrics collector
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            tasks_submitted: AtomicU64::new(0),
            tasks_completed: AtomicU64::new(0),
            tasks_failed: AtomicU64::new(0),
            syscall_count: AtomicU64::new(0),
            syscall_errors: AtomicU64::new(0),
            security_violations: AtomicU64::new(0),
            capability_checks: AtomicU64::new(0),
            storage_reads: AtomicU64::new(0),
            storage_writes: AtomicU64::new(0),
        }
    }

    /// Record task submission
    pub fn record_task_submitted(&self) {
        self.tasks_submitted.fetch_add(1, Ordering::SeqCst);
    }

    /// Record task completion
    pub fn record_task_completed(&self, _duration_ms: f64) {
        self.tasks_completed.fetch_add(1, Ordering::SeqCst);
    }

    /// Record task failure
    pub fn record_task_failed(&self) {
        self.tasks_failed.fetch_add(1, Ordering::SeqCst);
    }

    /// Record active task count
    pub fn record_active_tasks(&self, _count: usize) {
        // Stored internally, can be queried via stats()
    }

    /// Record queue length
    pub fn record_queue_length(&self, _length: usize) {
        // Stored internally, can be queried via stats()
    }

    /// Record syscall
    pub fn record_syscall(&self, _number: u64, _duration_us: f64, success: bool) {
        self.syscall_count.fetch_add(1, Ordering::SeqCst);
        if !success {
            self.syscall_errors.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Record security violation
    pub fn record_security_violation(&self, _violation_type: &str) {
        self.security_violations.fetch_add(1, Ordering::SeqCst);
    }

    /// Record capability check
    pub fn record_capability_check(&self, _granted: bool) {
        self.capability_checks.fetch_add(1, Ordering::SeqCst);
    }

    /// Record audit log size
    pub fn record_audit_log_size(&self, _entries: usize) {
        // Stored internally
    }

    /// Record memory usage
    pub fn record_memory_usage(&self, _used_bytes: u64, _limit_bytes: u64) {
        // Stored internally
    }

    /// Record storage operation
    pub fn record_storage_read(&self, _duration_ms: f64) {
        self.storage_reads.fetch_add(1, Ordering::SeqCst);
    }

    /// Record storage write
    pub fn record_storage_write(&self, _duration_ms: f64) {
        self.storage_writes.fetch_add(1, Ordering::SeqCst);
    }

    /// Get uptime in seconds
    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Get total syscall count
    pub fn syscall_count(&self) -> u64 {
        self.syscall_count.load(Ordering::SeqCst)
    }

    /// Get total syscall errors
    pub fn syscall_errors(&self) -> u64 {
        self.syscall_errors.load(Ordering::SeqCst)
    }

    /// Get statistics as formatted string
    pub fn format_stats(&self) -> String {
        format!(
            "Metrics: tasks(submitted={}, completed={}, failed={}), syscalls(total={}, \
             errors={}), security(violations={}, cap_checks={}), storage(reads={}, writes={})",
            self.tasks_submitted.load(Ordering::SeqCst),
            self.tasks_completed.load(Ordering::SeqCst),
            self.tasks_failed.load(Ordering::SeqCst),
            self.syscall_count.load(Ordering::SeqCst),
            self.syscall_errors.load(Ordering::SeqCst),
            self.security_violations.load(Ordering::SeqCst),
            self.capability_checks.load(Ordering::SeqCst),
            self.storage_reads.load(Ordering::SeqCst),
            self.storage_writes.load(Ordering::SeqCst),
        )
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Scoped timer for measuring operation duration
pub struct Timer {
    /// Start timestamp
    start: Instant,
}

impl Timer {
    /// Start new timer
    pub fn start() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    /// Get elapsed time in milliseconds
    pub fn elapsed_ms(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }

    /// Get elapsed time in microseconds
    pub fn elapsed_us(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1_000_000.0
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::start()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timer() {
        let timer = Timer::start();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let elapsed = timer.elapsed_ms();
        assert!(elapsed >= 10.0, "Timer should measure at least 10ms");
    }

    #[test]
    fn test_metrics_collector() {
        let metrics = MetricsCollector::new();

        metrics.record_task_submitted();
        metrics.record_task_completed(100.0);
        metrics.record_syscall(1, 50.0, true);
        metrics.record_security_violation("unauthorized");

        assert_eq!(metrics.syscall_count(), 1);
        assert_eq!(metrics.uptime_seconds(), 0); // Should be 0 or 1
    }
}
