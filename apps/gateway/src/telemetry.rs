//! Telemetry and Observability
//!
//! Structured logging, metrics, and distributed tracing support.

use std::time::Duration;

use axum::http::Request;
use opentelemetry::global;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::Config;
use opentelemetry_sdk::{runtime, Resource};
use tracing::Span;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use crate::config::{LoggingConfig, TracingConfig};

/// Initialize telemetry (logging, metrics, tracing)
pub fn init_telemetry(logging: &LoggingConfig, tracing: &TracingConfig) {
    // Initialize tracing subscriber based on format
    match logging.format.as_str() {
        "json" => init_json_logging(logging),
        "pretty" => init_pretty_logging(logging),
        _ => init_compact_logging(logging),
    };

    // Initialize OpenTelemetry if enabled
    if tracing.enabled {
        init_opentelemetry(tracing);
    }
}

/// Initialize JSON structured logging
fn init_json_logging(config: &LoggingConfig) {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.level));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_current_span(true)
        .with_span_list(true)
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .flatten_event(true);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();
}

/// Initialize pretty (human-readable) logging
fn init_pretty_logging(config: &LoggingConfig) {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.level));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .pretty()
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();
}

/// Initialize compact logging
fn init_compact_logging(config: &LoggingConfig) {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.level));

    let fmt_layer = tracing_subscriber::fmt::layer().compact().with_target(true);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();
}

/// Initialize OpenTelemetry tracing
fn init_opentelemetry(config: &TracingConfig) {
    if let Some(endpoint) = &config.otel_endpoint {
        // Create OTLP exporter
        let exporter = opentelemetry_otlp::new_exporter()
            .tonic()
            .with_endpoint(endpoint)
            .with_timeout(Duration::from_secs(3));

        // Create tracer provider
        match opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(exporter)
            .with_trace_config(Config::default().with_resource(Resource::new(vec![
                opentelemetry::KeyValue::new("service.name", "beebotos-gateway"),
                opentelemetry::KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
            ])))
            .install_batch(runtime::Tokio)
        {
            Ok(provider) => {
                // Set global tracer provider
                global::set_tracer_provider(provider);
                tracing::info!("OpenTelemetry tracer installed successfully");
            }
            Err(e) => {
                tracing::error!("Failed to install OpenTelemetry tracer: {}", e);
                tracing::warn!("Continuing without OpenTelemetry tracing");
            }
        }

        // Note: In a real implementation, you'd add the tracing-opentelemetry
        // layer to the subscriber. This requires restructuring the
        // subscriber initialization
        // to use opentelemetry_sdk::trace::Tracer instead of global::tracer()
    }
}

/// Metrics endpoint handler
pub async fn metrics_handler() -> String {
    // Generate Prometheus-compatible metrics output
    let mut output = String::new();

    // Add basic process metrics
    output.push_str(
        "# HELP process_cpu_seconds_total Total user and system CPU time spent in seconds.\n",
    );
    output.push_str("# TYPE process_cpu_seconds_total counter\n");
    output.push_str(&format!("process_cpu_seconds_total {}\n", 0.0));

    output.push_str("# HELP process_resident_memory_bytes Resident memory size in bytes.\n");
    output.push_str("# TYPE process_resident_memory_bytes gauge\n");
    output.push_str(&format!(
        "process_resident_memory_bytes {}\n",
        get_memory_usage()
    ));

    // Add custom gateway metrics
    output.push_str("# HELP beebotos_gateway_info Gateway information\n");
    output.push_str("# TYPE beebotos_gateway_info gauge\n");
    output.push_str(&format!(
        "beebotos_gateway_info{{version=\"{}\"}} 1\n",
        env!("CARGO_PKG_VERSION")
    ));

    output
}

/// Get current memory usage in bytes
fn get_memory_usage() -> u64 {
    // Simplified - in production, use proper system metrics
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/self/statm")
            .ok()
            .and_then(|s| {
                s.split_whitespace()
                    .nth(1)
                    .map(|v| v.parse::<u64>().ok())
                    .flatten()
            })
            .map(|v| v * 4096) // Convert pages to bytes
            .unwrap_or(0)
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

/// Custom metrics
pub struct Metrics {
    /// Total HTTP requests
    pub http_requests_total: prometheus::CounterVec,
    /// HTTP request duration
    pub http_request_duration_seconds: prometheus::HistogramVec,
    /// Active agent count
    pub agents_active: prometheus::Gauge,
    /// Database connection pool size
    pub db_pool_size: prometheus::Gauge,
    /// Rate limit hits
    pub rate_limit_hits: prometheus::CounterVec,
    // OBS-002: Additional business metrics
    /// Webhook requests processed
    pub webhook_requests_total: prometheus::CounterVec,
    /// LLM requests processed
    pub llm_requests_total: prometheus::CounterVec,
    /// LLM request duration
    pub llm_request_duration_seconds: prometheus::HistogramVec,
    /// Chain operations
    pub chain_operations_total: prometheus::CounterVec,
    /// Chain operation duration
    pub chain_operation_duration_seconds: prometheus::HistogramVec,
    /// Agent lifecycle events
    pub agent_lifecycle_events_total: prometheus::CounterVec,
    /// Message processing duration
    pub message_processing_duration_seconds: prometheus::Histogram,
    /// Cache hits/misses
    pub cache_operations_total: prometheus::CounterVec,
}

impl Metrics {
    /// Create new metrics
    ///
    /// # Panics
    /// Panics if metrics registration fails. This is a fatal error that
    /// indicates a configuration problem or duplicate metric registration.
    pub fn new() -> Self {
        let http_requests_total = prometheus::CounterVec::new(
            prometheus::Opts::new("http_requests_total", "Total number of HTTP requests"),
            &["method", "path", "status"],
        )
        .expect("Failed to create http_requests_total counter - possible duplicate registration");

        let http_request_duration_seconds = prometheus::HistogramVec::new(
            prometheus::HistogramOpts::new(
                "http_request_duration_seconds",
                "HTTP request duration in seconds",
            ),
            &["method", "path"],
        )
        .expect(
            "Failed to create http_request_duration_seconds histogram - possible duplicate \
             registration",
        );

        let agents_active = prometheus::Gauge::new("agents_active", "Number of active agents")
            .expect("Failed to create agents_active gauge - possible duplicate registration");

        let db_pool_size = prometheus::Gauge::new("db_pool_size", "Database connection pool size")
            .expect("Failed to create db_pool_size gauge - possible duplicate registration");

        let rate_limit_hits = prometheus::CounterVec::new(
            prometheus::Opts::new("rate_limit_hits_total", "Total number of rate limit hits"),
            &["client_id"],
        )
        .expect("Failed to create rate_limit_hits counter - possible duplicate registration");

        // OBS-002: Business metrics
        let webhook_requests_total = prometheus::CounterVec::new(
            prometheus::Opts::new(
                "beebotos_webhook_requests_total",
                "Total number of webhook requests processed",
            ),
            &["platform", "status"],
        )
        .expect("Failed to create webhook_requests_total counter");

        let llm_requests_total = prometheus::CounterVec::new(
            prometheus::Opts::new(
                "beebotos_llm_requests_total",
                "Total number of LLM API requests",
            ),
            &["provider", "status"],
        )
        .expect("Failed to create llm_requests_total counter");

        let llm_request_duration_seconds = prometheus::HistogramVec::new(
            prometheus::HistogramOpts::new(
                "beebotos_llm_request_duration_seconds",
                "LLM request duration in seconds",
            ),
            &["provider"],
        )
        .expect("Failed to create llm_request_duration_seconds histogram");

        let chain_operations_total = prometheus::CounterVec::new(
            prometheus::Opts::new(
                "beebotos_chain_operations_total",
                "Total number of blockchain operations",
            ),
            &["operation", "status"],
        )
        .expect("Failed to create chain_operations_total counter");

        let chain_operation_duration_seconds = prometheus::HistogramVec::new(
            prometheus::HistogramOpts::new(
                "beebotos_chain_operation_duration_seconds",
                "Blockchain operation duration in seconds",
            ),
            &["operation"],
        )
        .expect("Failed to create chain_operation_duration_seconds histogram");

        let agent_lifecycle_events_total = prometheus::CounterVec::new(
            prometheus::Opts::new(
                "beebotos_agent_lifecycle_events_total",
                "Total number of agent lifecycle events",
            ),
            &["event_type"],
        )
        .expect("Failed to create agent_lifecycle_events_total counter");

        let message_processing_duration_seconds =
            prometheus::Histogram::with_opts(prometheus::HistogramOpts::new(
                "beebotos_message_processing_duration_seconds",
                "Message processing duration in seconds",
            ))
            .expect("Failed to create message_processing_duration_seconds histogram");

        let cache_operations_total = prometheus::CounterVec::new(
            prometheus::Opts::new(
                "beebotos_cache_operations_total",
                "Total number of cache operations",
            ),
            &["operation", "result"],
        )
        .expect("Failed to create cache_operations_total counter");

        Self {
            http_requests_total,
            http_request_duration_seconds,
            agents_active,
            db_pool_size,
            rate_limit_hits,
            webhook_requests_total,
            llm_requests_total,
            llm_request_duration_seconds,
            chain_operations_total,
            chain_operation_duration_seconds,
            agent_lifecycle_events_total,
            message_processing_duration_seconds,
            cache_operations_total,
        }
    }

    /// Record HTTP request
    pub fn record_http_request(&self, method: &str, path: &str, status: u16, duration: Duration) {
        let status_label = format!("{}", status);
        self.http_requests_total
            .with_label_values(&[method, path, &status_label])
            .inc();

        self.http_request_duration_seconds
            .with_label_values(&[method, path])
            .observe(duration.as_secs_f64());
    }

    /// Update active agent count
    pub fn set_active_agents(&self, count: i64) {
        self.agents_active.set(count as f64);
    }

    /// Update DB pool size
    pub fn set_db_pool_size(&self, size: i64) {
        self.db_pool_size.set(size as f64);
    }

    /// Record rate limit hit
    pub fn record_rate_limit_hit(&self, client_id: &str) {
        self.rate_limit_hits.with_label_values(&[client_id]).inc();
    }

    // OBS-002: Business metrics methods

    /// Record webhook request
    pub fn record_webhook_request(&self, platform: &str, success: bool) {
        let status = if success { "success" } else { "error" };
        self.webhook_requests_total
            .with_label_values(&[platform, status])
            .inc();
    }

    /// Record LLM request
    pub fn record_llm_request(&self, provider: &str, success: bool, duration: Duration) {
        let status = if success { "success" } else { "error" };
        self.llm_requests_total
            .with_label_values(&[provider, status])
            .inc();
        self.llm_request_duration_seconds
            .with_label_values(&[provider])
            .observe(duration.as_secs_f64());
    }

    /// Record chain operation
    pub fn record_chain_operation(&self, operation: &str, success: bool, duration: Duration) {
        let status = if success { "success" } else { "error" };
        self.chain_operations_total
            .with_label_values(&[operation, status])
            .inc();
        self.chain_operation_duration_seconds
            .with_label_values(&[operation])
            .observe(duration.as_secs_f64());
    }

    /// Record agent lifecycle event
    pub fn record_agent_lifecycle_event(&self, event_type: &str) {
        self.agent_lifecycle_events_total
            .with_label_values(&[event_type])
            .inc();
    }

    /// Record message processing duration
    pub fn record_message_processing(&self, duration: Duration) {
        self.message_processing_duration_seconds
            .observe(duration.as_secs_f64());
    }

    /// Record cache operation
    pub fn record_cache_operation(&self, operation: &str, hit: bool) {
        let result = if hit { "hit" } else { "miss" };
        self.cache_operations_total
            .with_label_values(&[operation, result])
            .inc();
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Request tracing span
#[allow(dead_code)]
pub fn make_span_with_request_id<B>(request: &Request<B>) -> Span {
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("unknown");

    tracing::info_span!(
        "http_request",
        request_id = %request_id,
        method = %request.method(),
        uri = %request.uri(),
        version = ?request.version(),
    )
}

/// Shutdown telemetry gracefully
pub fn shutdown_telemetry() {
    opentelemetry::global::shutdown_tracer_provider();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = Metrics::new();
        metrics.set_active_agents(5);
        metrics.set_db_pool_size(10);
        metrics.record_rate_limit_hit("client_1");
    }
}
