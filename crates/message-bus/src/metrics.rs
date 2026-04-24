//! Metrics and observability for the message bus

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Message bus metrics
#[derive(Debug)]
pub struct MessageBusMetrics {
    /// Total messages published
    messages_published: AtomicU64,
    /// Total messages delivered to subscribers
    messages_delivered: AtomicU64,
    /// Total subscribe operations
    subscribe_count: AtomicU64,
    /// Total unsubscribe operations
    unsubscribe_count: AtomicU64,
    /// Total request-reply operations
    request_count: AtomicU64,
    /// Total failed requests (timeouts)
    request_timeouts: AtomicU64,
    /// Total bytes published
    bytes_published: AtomicU64,
    /// Total bytes delivered
    bytes_delivered: AtomicU64,
    /// Publish latency histogram (microseconds)
    /// Stored as: [count, sum] for simple average calculation
    publish_latency: AtomicU64,
    publish_latency_count: AtomicU64,
    /// Topic-specific metrics
    topic_metrics: Arc<parking_lot::RwLock<HashMap<String, TopicMetrics>>>,
}

/// Per-topic metrics
#[derive(Debug, Default, Clone)]
pub struct TopicMetrics {
    /// Messages published to this topic
    pub messages_published: u64,
    /// Messages delivered from this topic
    pub messages_delivered: u64,
    /// Current subscriber count
    pub subscriber_count: u64,
    /// Average message size
    pub avg_message_size: u64,
    /// Total bytes for this topic
    pub total_bytes: u64,
}

impl MessageBusMetrics {
    /// Create new metrics collector
    pub fn new() -> Self {
        Self {
            messages_published: AtomicU64::new(0),
            messages_delivered: AtomicU64::new(0),
            subscribe_count: AtomicU64::new(0),
            unsubscribe_count: AtomicU64::new(0),
            request_count: AtomicU64::new(0),
            request_timeouts: AtomicU64::new(0),
            bytes_published: AtomicU64::new(0),
            bytes_delivered: AtomicU64::new(0),
            publish_latency: AtomicU64::new(0),
            publish_latency_count: AtomicU64::new(0),
            topic_metrics: Arc::new(parking_lot::RwLock::new(HashMap::new())),
        }
    }

    /// Record a publish operation
    pub fn record_publish(&self, topic: &str, latency: Duration) {
        self.messages_published.fetch_add(1, Ordering::Relaxed);
        self.publish_latency_count.fetch_add(1, Ordering::Relaxed);
        self.publish_latency
            .fetch_add(latency.as_micros() as u64, Ordering::Relaxed);

        // Update topic metrics
        let mut metrics = self.topic_metrics.write();
        let topic_metric = metrics.entry(topic.to_string()).or_default();
        topic_metric.messages_published += 1;
    }

    /// Record message delivery
    pub fn record_delivery(&self, topic: &str, bytes: usize) {
        self.messages_delivered.fetch_add(1, Ordering::Relaxed);
        self.bytes_delivered
            .fetch_add(bytes as u64, Ordering::Relaxed);

        let mut metrics = self.topic_metrics.write();
        let topic_metric = metrics.entry(topic.to_string()).or_default();
        topic_metric.messages_delivered += 1;
        topic_metric.total_bytes += bytes as u64;
        topic_metric.avg_message_size = topic_metric.total_bytes / topic_metric.messages_delivered;
    }

    /// Record bytes published
    pub fn record_bytes_published(&self, bytes: usize) {
        self.bytes_published
            .fetch_add(bytes as u64, Ordering::Relaxed);
    }

    /// Record subscribe operation
    pub fn record_subscribe(&self) {
        self.subscribe_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record unsubscribe operation
    pub fn record_unsubscribe(&self) {
        self.unsubscribe_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record request operation
    pub fn record_request(&self) {
        self.request_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Record request timeout
    pub fn record_request_timeout(&self) {
        self.request_timeouts.fetch_add(1, Ordering::Relaxed);
    }

    /// Update topic subscriber count
    pub fn update_subscriber_count(&self, topic: &str, count: u64) {
        let mut metrics = self.topic_metrics.write();
        let topic_metric = metrics.entry(topic.to_string()).or_default();
        topic_metric.subscriber_count = count;
    }

    /// Get total messages published
    pub fn messages_published(&self) -> u64 {
        self.messages_published.load(Ordering::Relaxed)
    }

    /// Get total messages delivered
    pub fn messages_delivered(&self) -> u64 {
        self.messages_delivered.load(Ordering::Relaxed)
    }

    /// Get current subscriber count
    pub fn subscriber_count(&self) -> u64 {
        self.subscribe_count.load(Ordering::Relaxed)
            - self.unsubscribe_count.load(Ordering::Relaxed)
    }

    /// Get total request count
    pub fn request_count(&self) -> u64 {
        self.request_count.load(Ordering::Relaxed)
    }

    /// Get request timeout count
    pub fn request_timeouts(&self) -> u64 {
        self.request_timeouts.load(Ordering::Relaxed)
    }

    /// Get average publish latency in microseconds
    pub fn avg_publish_latency_us(&self) -> u64 {
        let count = self.publish_latency_count.load(Ordering::Relaxed);
        let sum = self.publish_latency.load(Ordering::Relaxed);
        if count > 0 {
            sum / count
        } else {
            0
        }
    }

    /// Get total bytes published
    pub fn bytes_published(&self) -> u64 {
        self.bytes_published.load(Ordering::Relaxed)
    }

    /// Get total bytes delivered
    pub fn bytes_delivered(&self) -> u64 {
        self.bytes_delivered.load(Ordering::Relaxed)
    }

    /// Get topic-specific metrics
    pub fn topic_metrics(&self) -> HashMap<String, TopicMetrics> {
        self.topic_metrics.read().clone()
    }

    /// Get metrics for a specific topic
    pub fn topic_metric(&self, topic: &str) -> Option<TopicMetrics> {
        self.topic_metrics.read().get(topic).cloned()
    }

    /// Get snapshot of all metrics
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            messages_published: self.messages_published(),
            messages_delivered: self.messages_delivered(),
            subscriber_count: self.subscriber_count(),
            request_count: self.request_count(),
            request_timeouts: self.request_timeouts(),
            avg_publish_latency_us: self.avg_publish_latency_us(),
            bytes_published: self.bytes_published(),
            bytes_delivered: self.bytes_delivered(),
            topic_metrics: self.topic_metrics(),
        }
    }

    /// Reset all metrics
    pub fn reset(&self) {
        self.messages_published.store(0, Ordering::Relaxed);
        self.messages_delivered.store(0, Ordering::Relaxed);
        self.subscribe_count.store(0, Ordering::Relaxed);
        self.unsubscribe_count.store(0, Ordering::Relaxed);
        self.request_count.store(0, Ordering::Relaxed);
        self.request_timeouts.store(0, Ordering::Relaxed);
        self.bytes_published.store(0, Ordering::Relaxed);
        self.bytes_delivered.store(0, Ordering::Relaxed);
        self.publish_latency.store(0, Ordering::Relaxed);
        self.publish_latency_count.store(0, Ordering::Relaxed);
        self.topic_metrics.write().clear();
    }
}

impl Default for MessageBusMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Metrics snapshot for reporting
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub messages_published: u64,
    pub messages_delivered: u64,
    pub subscriber_count: u64,
    pub request_count: u64,
    pub request_timeouts: u64,
    pub avg_publish_latency_us: u64,
    pub bytes_published: u64,
    pub bytes_delivered: u64,
    pub topic_metrics: HashMap<String, TopicMetrics>,
}

impl MetricsSnapshot {
    /// Calculate delivery rate (delivered / published)
    pub fn delivery_rate(&self) -> f64 {
        if self.messages_published > 0 {
            self.messages_delivered as f64 / self.messages_published as f64
        } else {
            1.0
        }
    }

    /// Calculate request timeout rate
    pub fn timeout_rate(&self) -> f64 {
        if self.request_count > 0 {
            self.request_timeouts as f64 / self.request_count as f64
        } else {
            0.0
        }
    }

    /// Format as human-readable string
    pub fn format(&self) -> String {
        format!(
            "Messages: {} published, {} delivered ({:.1}% rate) | Latency: {}μs avg | \
             Subscribers: {} | Requests: {} ({} timeouts) | Bytes: {} published, {} delivered",
            self.messages_published,
            self.messages_delivered,
            self.delivery_rate() * 100.0,
            self.avg_publish_latency_us,
            self.subscriber_count,
            self.request_count,
            self.request_timeouts,
            format_bytes(self.bytes_published),
            format_bytes(self.bytes_delivered)
        )
    }
}

/// Format bytes to human-readable string
fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    format!("{:.2} {}", size, UNITS[unit_index])
}

/// Metrics reporter trait
pub trait MetricsReporter: Send + Sync {
    /// Report metrics snapshot
    fn report(&self, snapshot: &MetricsSnapshot);
}

/// Console metrics reporter
pub struct ConsoleMetricsReporter;

impl MetricsReporter for ConsoleMetricsReporter {
    fn report(&self, snapshot: &MetricsSnapshot) {
        tracing::info!("MessageBus Metrics: {}", snapshot.format());
    }
}

/// Metrics collection interval runner
pub struct MetricsCollector {
    metrics: Arc<MessageBusMetrics>,
    reporter: Box<dyn MetricsReporter>,
    interval: Duration,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new(
        metrics: Arc<MessageBusMetrics>,
        reporter: Box<dyn MetricsReporter>,
        interval: Duration,
    ) -> Self {
        Self {
            metrics,
            reporter,
            interval,
        }
    }

    /// Start collecting metrics
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.interval);

            loop {
                interval.tick().await;

                let snapshot = self.metrics.snapshot();
                self.reporter.report(&snapshot);
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_basic() {
        let metrics = MessageBusMetrics::new();

        // Record some operations
        metrics.record_publish("test/topic", Duration::from_micros(100));
        metrics.record_publish("test/topic", Duration::from_micros(200));
        metrics.record_delivery("test/topic", 1024);
        metrics.record_subscribe();

        // Check totals
        assert_eq!(metrics.messages_published(), 2);
        assert_eq!(metrics.messages_delivered(), 1);
        assert_eq!(metrics.subscriber_count(), 1);
        assert_eq!(metrics.avg_publish_latency_us(), 150);
    }

    #[test]
    fn test_topic_metrics() {
        let metrics = MessageBusMetrics::new();

        metrics.record_publish("topic/a", Duration::from_micros(100));
        metrics.record_publish("topic/b", Duration::from_micros(200));
        metrics.record_delivery("topic/a", 512);
        metrics.record_delivery("topic/a", 512);

        let topic_a = metrics.topic_metric("topic/a").unwrap();
        assert_eq!(topic_a.messages_published, 1);
        assert_eq!(topic_a.messages_delivered, 2);
        assert_eq!(topic_a.total_bytes, 1024);
        assert_eq!(topic_a.avg_message_size, 512);

        let topic_b = metrics.topic_metric("topic/b").unwrap();
        assert_eq!(topic_b.messages_published, 1);
        assert_eq!(topic_b.messages_delivered, 0);
    }

    #[test]
    fn test_metrics_snapshot() {
        let metrics = MessageBusMetrics::new();

        metrics.record_publish("test", Duration::from_micros(100));
        metrics.record_delivery("test", 1024);
        metrics.record_subscribe();
        metrics.record_request();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.messages_published, 1);
        assert_eq!(snapshot.messages_delivered, 1);
        assert_eq!(snapshot.subscriber_count, 1);
        assert_eq!(snapshot.request_count, 1);
        assert_eq!(snapshot.delivery_rate(), 1.0);
        assert_eq!(snapshot.timeout_rate(), 0.0);
    }

    #[test]
    fn test_metrics_reset() {
        let metrics = MessageBusMetrics::new();

        metrics.record_publish("test", Duration::from_micros(100));
        metrics.record_subscribe();

        assert_eq!(metrics.messages_published(), 1);

        metrics.reset();

        assert_eq!(metrics.messages_published(), 0);
        assert_eq!(metrics.subscriber_count(), 0);
        assert!(metrics.topic_metric("test").is_none());
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0.00 B");
        assert_eq!(format_bytes(512), "512.00 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn test_snapshot_format() {
        let snapshot = MetricsSnapshot {
            messages_published: 1000,
            messages_delivered: 950,
            subscriber_count: 10,
            request_count: 100,
            request_timeouts: 5,
            avg_publish_latency_us: 150,
            bytes_published: 1024 * 1024,
            bytes_delivered: 512 * 1024,
            topic_metrics: HashMap::new(),
        };

        let formatted = snapshot.format();
        assert!(formatted.contains("1000 published"));
        assert!(formatted.contains("950 delivered"));
        assert!(formatted.contains("95.0% rate"));
        assert!(formatted.contains("150μs"));
    }
}
