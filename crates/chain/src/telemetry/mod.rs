//! OpenTelemetry Integration Module
//!
//! Provides distributed tracing and metrics export via OpenTelemetry.

use std::time::Duration;

use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::{Config, Sampler, Tracer};
use opentelemetry_sdk::Resource;
use tracing::info;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;

/// Telemetry configuration
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// Service name for tracing
    pub service_name: String,
    /// Service version
    pub service_version: String,
    /// OTLP endpoint (e.g., "http://localhost:4317")
    pub otlp_endpoint: Option<String>,
    /// Sampling ratio (0.0 to 1.0)
    pub sampling_ratio: f64,
    /// Enable stdout exporter for debugging
    pub stdout_exporter: bool,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            service_name: "beebotos-chain".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            otlp_endpoint: std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok(),
            sampling_ratio: 1.0,
            stdout_exporter: false,
        }
    }
}

impl TelemetryConfig {
    /// Create with service name
    pub fn with_service_name(mut self, name: impl Into<String>) -> Self {
        self.service_name = name.into();
        self
    }

    /// Create with OTLP endpoint
    pub fn with_otlp_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.otlp_endpoint = Some(endpoint.into());
        self
    }

    /// Create with sampling ratio
    pub fn with_sampling_ratio(mut self, ratio: f64) -> Self {
        self.sampling_ratio = ratio.clamp(0.0, 1.0);
        self
    }

    /// Enable stdout exporter
    pub fn with_stdout_exporter(mut self) -> Self {
        self.stdout_exporter = true;
        self
    }

    /// Create from environment variables
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(ratio) = std::env::var("OTEL_TRACES_SAMPLER_ARG") {
            if let Ok(ratio) = ratio.parse::<f64>() {
                config.sampling_ratio = ratio.clamp(0.0, 1.0);
            }
        }

        if let Ok(name) = std::env::var("OTEL_SERVICE_NAME") {
            config.service_name = name;
        }

        config
    }
}

/// Telemetry initializer
pub struct Telemetry {
    _tracer: Option<Tracer>,
}

impl Telemetry {
    /// Initialize telemetry with configuration
    pub fn init(config: TelemetryConfig) -> anyhow::Result<Self> {
        info!(
            service_name = %config.service_name,
            sampling_ratio = config.sampling_ratio,
            "Initializing OpenTelemetry"
        );

        let resource = Resource::default().merge(&Resource::new(vec![
            opentelemetry::KeyValue::new("service.name", config.service_name.clone()),
            opentelemetry::KeyValue::new("service.version", config.service_version.clone()),
        ]));

        let mut tracer = None;

        // Configure OTLP exporter if endpoint is set
        if let Some(endpoint) = config.otlp_endpoint {
            info!(endpoint = %endpoint, "Configuring OTLP exporter");

            let otlp_exporter = opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(endpoint)
                .with_timeout(Duration::from_secs(3));

            let tracer_provider = opentelemetry_otlp::new_pipeline()
                .tracing()
                .with_exporter(otlp_exporter)
                .with_trace_config(
                    Config::default()
                        .with_sampler(Sampler::TraceIdRatioBased(config.sampling_ratio))
                        .with_resource(resource.clone()),
                )
                .install_batch(opentelemetry_sdk::runtime::Tokio)?;

            let tracer_instance = tracer_provider.tracer("beebotos-chain");

            // Create OpenTelemetry layer for tracing
            let telemetry_layer =
                tracing_opentelemetry::layer().with_tracer(tracer_instance.clone());

            // Initialize subscriber
            let subscriber = Registry::default().with(telemetry_layer);

            tracing::subscriber::set_global_default(subscriber)?;

            tracer = Some(tracer_instance);
        }

        Ok(Self { _tracer: tracer })
    }

    /// Initialize with default configuration
    pub fn init_default() -> anyhow::Result<Self> {
        Self::init(TelemetryConfig::default())
    }

    /// Initialize from environment
    pub fn init_from_env() -> anyhow::Result<Self> {
        Self::init(TelemetryConfig::from_env())
    }

    /// Shutdown telemetry
    pub fn shutdown(&self) {
        info!("Shutting down OpenTelemetry");
        opentelemetry::global::shutdown_tracer_provider();
    }
}

impl Drop for Telemetry {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Trace context for distributed tracing
#[derive(Debug, Clone)]
pub struct TraceContext {
    pub trace_id: String,
    pub span_id: String,
}

impl TraceContext {
    /// Create from current span
    pub fn current() -> Option<Self> {
        use opentelemetry::trace::TraceContextExt;
        use tracing_opentelemetry::OpenTelemetrySpanExt;

        let span = tracing::Span::current();
        let cx = span.context();
        let span_ref = cx.span();
        let span_context = span_ref.span_context();

        if span_context.is_valid() {
            Some(Self {
                trace_id: span_context.trace_id().to_string(),
                span_id: span_context.span_id().to_string(),
            })
        } else {
            None
        }
    }

    /// Get trace ID
    pub fn trace_id(&self) -> &str {
        &self.trace_id
    }

    /// Get span ID
    pub fn span_id(&self) -> &str {
        &self.span_id
    }
}

// Re-export tracing Instrument trait for convenience
pub use tracing::Instrument;

/// Create a span for chain operations
#[macro_export]
macro_rules! chain_span {
    ($name:expr, $($key:ident = $value:expr),*) => {
        tracing::info_span!(
            target: "chain",
            $name,
            $($key = %$value),*,
            trace_id = tracing::field::Empty,
            span_id = tracing::field::Empty,
        )
    };
}

/// Initialize telemetry with standard configuration
pub fn init_telemetry() -> anyhow::Result<Telemetry> {
    Telemetry::init_from_env()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_config_default() {
        let config = TelemetryConfig::default();
        assert_eq!(config.service_name, "beebotos-chain");
        assert!(config.sampling_ratio >= 0.0 && config.sampling_ratio <= 1.0);
    }

    #[test]
    fn test_telemetry_config_builder() {
        let config = TelemetryConfig::default()
            .with_service_name("test-service")
            .with_sampling_ratio(0.5)
            .with_stdout_exporter();

        assert_eq!(config.service_name, "test-service");
        assert_eq!(config.sampling_ratio, 0.5);
        assert!(config.stdout_exporter);
    }

    #[test]
    fn test_telemetry_config_sampling_clamping() {
        let config = TelemetryConfig::default().with_sampling_ratio(1.5);
        assert_eq!(config.sampling_ratio, 1.0);

        let config = TelemetryConfig::default().with_sampling_ratio(-0.5);
        assert_eq!(config.sampling_ratio, 0.0);
    }
}
