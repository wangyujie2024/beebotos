//! Distributed tracing

use std::collections::HashMap;

use rand::Rng;
use serde::{Deserialize, Serialize};

/// Trace ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TraceId(pub String);

impl TraceId {
    /// Generate new trace ID
    pub fn new() -> Self {
        let mut rng = rand::thread_rng();
        let bytes: [u8; 16] = rng.gen();
        Self(hex::encode(bytes))
    }

    /// Generate from string
    pub fn from_string(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl Default for TraceId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TraceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Span ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpanId(pub String);

impl SpanId {
    /// Generate new span ID
    pub fn new() -> Self {
        let mut rng = rand::thread_rng();
        let bytes: [u8; 8] = rng.gen();
        Self(hex::encode(bytes))
    }
}

impl Default for SpanId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SpanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Trace context
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TraceContext {
    pub trace_id: TraceId,
    pub span_id: SpanId,
    pub parent_span_id: Option<SpanId>,
    pub sampled: bool,
    pub baggage: HashMap<String, String>,
}

impl TraceContext {
    /// Create new context
    pub fn new() -> Self {
        Self {
            trace_id: TraceId::new(),
            span_id: SpanId::new(),
            parent_span_id: None,
            sampled: true,
            baggage: HashMap::new(),
        }
    }

    /// Create child context
    pub fn child(&self) -> Self {
        Self {
            trace_id: self.trace_id.clone(),
            span_id: SpanId::new(),
            parent_span_id: Some(self.span_id.clone()),
            sampled: self.sampled,
            baggage: self.baggage.clone(),
        }
    }

    /// Add baggage
    pub fn with_baggage(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.baggage.insert(key.into(), value.into());
        self
    }

    /// Convert to W3C traceparent format
    pub fn to_traceparent(&self) -> String {
        // Ensure trace_id is 32 hex chars and span_id is 16 hex chars
        let trace_id = format!("{:0>32}", self.trace_id.0);
        let span_id = format!("{:0>16}", self.span_id.0);
        format!(
            "00-{}-{}-{:02x}",
            trace_id,
            span_id,
            if self.sampled { 1 } else { 0 }
        )
    }

    /// Parse from W3C traceparent format
    pub fn from_traceparent(traceparent: &str) -> Option<Self> {
        let parts: Vec<_> = traceparent.split('-').collect();
        if parts.len() != 4 || parts[0] != "00" {
            return None;
        }

        Some(Self {
            trace_id: TraceId(parts[1].to_string()),
            span_id: SpanId(parts[2].to_string()),
            parent_span_id: None,
            sampled: parts[3] == "01",
            baggage: HashMap::new(),
        })
    }
}

/// Initialize tracing
#[allow(unused_variables)]
pub fn init_tracing(config: &crate::TelemetryConfig) -> anyhow::Result<()> {
    #[cfg(feature = "jaeger")]
    {
        if let Some(endpoint) = &config.jaeger_endpoint {
            init_jaeger(config, endpoint)?;
        }
    }

    Ok(())
}

#[cfg(feature = "jaeger")]
fn init_jaeger(config: &crate::TelemetryConfig, endpoint: &str) -> anyhow::Result<()> {
    use opentelemetry::KeyValue;
    use opentelemetry_otlp::WithExportConfig;
    use tracing_subscriber::layer::SubscriberExt;

    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(endpoint),
        )
        .with_trace_config(opentelemetry_sdk::trace::config().with_resource(
            opentelemetry_sdk::Resource::new(vec![
                KeyValue::new("service.name", config.service_name.clone()),
                KeyValue::new("service.version", config.service_version.clone()),
            ]),
        ))
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;

    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

    use tracing_subscriber::util::SubscriberInitExt;
    tracing_subscriber::registry().with(telemetry).try_init()?;

    Ok(())
}

/// Propagate context to outgoing request
pub fn inject_context(ctx: &TraceContext, headers: &mut HashMap<String, String>) {
    headers.insert("traceparent".to_string(), ctx.to_traceparent());

    // Add baggage
    for (key, value) in &ctx.baggage {
        headers.insert(format!("baggage-{}", key), value.clone());
    }
}

/// Extract context from incoming request
pub fn extract_context(headers: &HashMap<String, String>) -> TraceContext {
    if let Some(traceparent) = headers.get("traceparent") {
        if let Some(ctx) = TraceContext::from_traceparent(traceparent) {
            // Extract baggage
            let mut ctx = ctx;
            for (key, value) in headers {
                if key.starts_with("baggage-") {
                    let baggage_key = key.trim_start_matches("baggage-");
                    ctx.baggage.insert(baggage_key.to_string(), value.clone());
                }
            }
            return ctx;
        }
    }

    TraceContext::new()
}

/// Span kind
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    Server,
    Client,
    Producer,
    Consumer,
    Internal,
}

impl SpanKind {
    /// As string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Client => "client",
            Self::Producer => "producer",
            Self::Consumer => "consumer",
            Self::Internal => "internal",
        }
    }
}

/// Status code
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusCode {
    Unset,
    Ok,
    Error,
}

impl StatusCode {
    /// As string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unset => "unset",
            Self::Ok => "ok",
            Self::Error => "error",
        }
    }
}
