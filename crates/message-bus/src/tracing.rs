//! OpenTelemetry integration for distributed tracing
//!
//! This module provides distributed tracing capabilities for the message bus,
//! allowing end-to-end tracing of messages across services.

use std::collections::HashMap;

use tracing::Span;

use crate::Message;

/// Tracing configuration
#[derive(Debug, Clone)]
pub struct TracingConfig {
    /// Enable distributed tracing
    pub enabled: bool,
    /// Sampling rate (0.0 - 1.0)
    pub sampling_rate: f64,
    /// Include message payload in traces (use with caution)
    pub include_payload: bool,
    /// Max payload size to include in bytes
    pub max_payload_size: usize,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sampling_rate: 1.0,
            include_payload: false,
            max_payload_size: 1024,
        }
    }
}

/// Trace context for message propagation
#[derive(Debug, Clone)]
pub struct TraceContext {
    /// Trace ID
    pub trace_id: String,
    /// Span ID
    pub span_id: String,
    /// Whether the trace is sampled
    pub sampled: bool,
    /// Additional baggage
    pub baggage: HashMap<String, String>,
}

impl TraceContext {
    /// Create a new trace context
    pub fn new() -> Self {
        Self {
            trace_id: generate_id(),
            span_id: generate_id(),
            sampled: true,
            baggage: HashMap::new(),
        }
    }

    /// Create from message headers
    pub fn from_message(message: &Message) -> Option<Self> {
        let headers = &message.metadata.headers;

        let trace_id = headers.get("x-trace-id")?;
        let span_id = headers.get("x-span-id")?;
        let sampled = headers
            .get("x-trace-sampled")
            .map(|v| v == "true")
            .unwrap_or(true);

        // Extract baggage (headers starting with "x-baggage-")
        let baggage: HashMap<String, String> = headers
            .iter()
            .filter(|(k, _)| k.starts_with("x-baggage-"))
            .map(|(k, v)| (k.replace("x-baggage-", ""), v.clone()))
            .collect();

        Some(Self {
            trace_id: trace_id.clone(),
            span_id: span_id.clone(),
            sampled,
            baggage,
        })
    }

    /// Inject into message headers
    pub fn inject_into(&self, message: &mut Message) {
        message
            .metadata
            .headers
            .insert("x-trace-id".to_string(), self.trace_id.clone());
        message
            .metadata
            .headers
            .insert("x-span-id".to_string(), self.span_id.clone());
        message
            .metadata
            .headers
            .insert("x-trace-sampled".to_string(), self.sampled.to_string());

        // Inject baggage
        for (key, value) in &self.baggage {
            message
                .metadata
                .headers
                .insert(format!("x-baggage-{}", key), value.clone());
        }
    }

    /// Create child span context
    pub fn child(&self) -> Self {
        Self {
            trace_id: self.trace_id.clone(),
            span_id: generate_id(),
            sampled: self.sampled,
            baggage: self.baggage.clone(),
        }
    }

    /// Add baggage
    pub fn with_baggage(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.baggage.insert(key.into(), value.into());
        self
    }
}

impl Default for TraceContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a unique ID
fn generate_id() -> String {
    uuid::Uuid::new_v4().to_string().replace("-", "")
}

/// Traced message wrapper
pub struct TracedMessage {
    pub message: Message,
    pub trace_context: TraceContext,
}

impl TracedMessage {
    /// Create a new traced message
    pub fn new(mut message: Message) -> Self {
        let trace_context = TraceContext::from_message(&message).unwrap_or_else(TraceContext::new);
        trace_context.inject_into(&mut message);

        Self {
            message,
            trace_context,
        }
    }

    /// Get trace ID
    pub fn trace_id(&self) -> &str {
        &self.trace_context.trace_id
    }

    /// Get span ID
    pub fn span_id(&self) -> &str {
        &self.trace_context.span_id
    }
}

/// Message tracer
pub struct MessageTracer {
    config: TracingConfig,
}

impl MessageTracer {
    /// Create a new message tracer
    pub fn new(config: TracingConfig) -> Self {
        Self { config }
    }

    /// Check if tracing is enabled for this message
    pub fn should_trace(&self) -> bool {
        if !self.config.enabled {
            return false;
        }

        // Apply sampling
        if self.config.sampling_rate < 1.0 {
            let rand: f64 = rand::random();
            return rand < self.config.sampling_rate;
        }

        true
    }

    /// Start a publish span
    pub fn start_publish_span(&self, topic: &str, message: &Message) -> Span {
        let trace_context = TraceContext::from_message(message);

        let span = tracing::info_span!(
            "message.publish",
            trace_id = trace_context.as_ref().map(|t| t.trace_id.clone()).unwrap_or_default(),
            topic = topic,
            message_id = %message.metadata.message_id,
            correlation_id = message.metadata.correlation_id.as_ref().unwrap_or(&"".to_string()),
        );

        span
    }

    /// Start a receive span
    pub fn start_receive_span(&self, message: &Message) -> Span {
        let trace_context = TraceContext::from_message(message);

        let span = tracing::info_span!(
            "message.receive",
            trace_id = trace_context.as_ref().map(|t| t.trace_id.clone()).unwrap_or_default(),
            topic = message.metadata.topic,
            message_id = %message.metadata.message_id,
        );

        span
    }

    /// Start a process span
    pub fn start_process_span(&self, message: &Message, handler_name: &str) -> Span {
        let span = tracing::info_span!(
            "message.process",
            topic = message.metadata.topic,
            message_id = %message.metadata.message_id,
            handler = handler_name,
        );

        span
    }

    /// Trace message delivery
    pub fn trace_delivery(&self, message: &Message, from_topic: &str, to_topic: &str) {
        if !self.should_trace() {
            return;
        }

        tracing::info!(
            trace_id = TraceContext::from_message(message).map(|t| t.trace_id).unwrap_or_default(),
            message_id = %message.metadata.message_id,
            from = from_topic,
            to = to_topic,
            "Message routed"
        );
    }
}

/// Extension trait for adding trace context to messages
pub trait MessageTracingExt {
    /// Add or update trace context
    fn with_trace_context(self, context: &TraceContext) -> Self;

    /// Get trace context if present
    fn trace_context(&self) -> Option<TraceContext>;

    /// Create child trace context
    fn child_trace_context(&self) -> TraceContext;
}

impl MessageTracingExt for Message {
    fn with_trace_context(mut self, context: &TraceContext) -> Self {
        context.inject_into(&mut self);
        self
    }

    fn trace_context(&self) -> Option<TraceContext> {
        TraceContext::from_message(self)
    }

    fn child_trace_context(&self) -> TraceContext {
        self.trace_context()
            .map(|ctx| ctx.child())
            .unwrap_or_else(TraceContext::new)
    }
}

/// Span kinds for message operations
#[derive(Debug, Clone, Copy)]
pub enum MessageSpanKind {
    Publish,
    Receive,
    Process,
    Route,
}

impl MessageSpanKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageSpanKind::Publish => "message.publish",
            MessageSpanKind::Receive => "message.receive",
            MessageSpanKind::Process => "message.process",
            MessageSpanKind::Route => "message.route",
        }
    }
}

/// Trace exporter trait for custom backends
#[async_trait::async_trait]
pub trait TraceExporter: Send + Sync {
    /// Export a span
    async fn export_span(&self, span: &ExportedSpan) -> Result<(), String>;

    /// Flush pending spans
    async fn flush(&self) -> Result<(), String>;
}

/// Exported span data
#[derive(Debug, Clone)]
pub struct ExportedSpan {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub name: String,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub end_time: chrono::DateTime<chrono::Utc>,
    pub attributes: HashMap<String, String>,
    pub events: Vec<SpanEvent>,
}

/// Span event
#[derive(Debug, Clone)]
pub struct SpanEvent {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub name: String,
    pub attributes: HashMap<String, String>,
}

/// Jaeger trace exporter
pub struct JaegerExporter {
    #[allow(dead_code)]
    endpoint: String,
    #[allow(dead_code)]
    service_name: String,
}

impl JaegerExporter {
    /// Create a new Jaeger exporter
    pub fn new(endpoint: impl Into<String>, service_name: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            service_name: service_name.into(),
        }
    }
}

#[async_trait::async_trait]
impl TraceExporter for JaegerExporter {
    async fn export_span(&self, span: &ExportedSpan) -> Result<(), String> {
        // Implementation would use Jaeger client library
        // For now, just log
        tracing::debug!(
            "Exporting span {} for trace {}",
            span.span_id,
            span.trace_id
        );
        Ok(())
    }

    async fn flush(&self) -> Result<(), String> {
        Ok(())
    }
}

/// Trace context propagation for inter-service communication
pub mod propagation {
    use super::*;

    /// Extract trace context from HTTP headers
    pub fn extract_from_http_headers(headers: &http::HeaderMap) -> Option<TraceContext> {
        let trace_id = headers.get("x-trace-id")?.to_str().ok()?;
        let span_id = headers.get("x-span-id")?.to_str().ok()?;
        let sampled = headers
            .get("x-trace-sampled")
            .and_then(|v| v.to_str().ok())
            .map(|v| v == "true")
            .unwrap_or(true);

        // Extract baggage items (headers starting with "x-baggage-")
        let mut baggage = HashMap::new();
        for (name, value) in headers.iter() {
            let name_str = name.as_str();
            if name_str.starts_with("x-baggage-") {
                let key = name_str.trim_start_matches("x-baggage-").to_string();
                if let Ok(val) = value.to_str() {
                    baggage.insert(key, val.to_string());
                }
            }
        }

        Some(TraceContext {
            trace_id: trace_id.to_string(),
            span_id: span_id.to_string(),
            sampled,
            baggage,
        })
    }

    /// Inject trace context into HTTP headers
    pub fn inject_into_http_headers(context: &TraceContext, headers: &mut http::HeaderMap) {
        if let Ok(trace_id) = context.trace_id.parse() {
            headers.insert("x-trace-id", trace_id);
        }
        if let Ok(span_id) = context.span_id.parse() {
            headers.insert("x-span-id", span_id);
        }
        if let Ok(sampled) = context.sampled.to_string().parse() {
            headers.insert("x-trace-sampled", sampled);
        }

        // Inject baggage items
        for (key, value) in &context.baggage {
            let header_name = format!("x-baggage-{}", key);
            if let Ok(name) = header_name.parse::<http::header::HeaderName>() {
                if let Ok(header_value) = value.parse::<http::HeaderValue>() {
                    headers.insert(name, header_value);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_context_creation() {
        let ctx = TraceContext::new();
        assert!(!ctx.trace_id.is_empty());
        assert!(!ctx.span_id.is_empty());
        assert!(ctx.sampled);
    }

    #[test]
    fn test_trace_context_inject_extract() {
        let mut message = Message::new("test/topic", b"data".to_vec());

        let ctx = TraceContext::new()
            .with_baggage("user_id", "12345")
            .with_baggage("request_id", "abc");

        ctx.inject_into(&mut message);

        let extracted = TraceContext::from_message(&message).unwrap();
        assert_eq!(extracted.trace_id, ctx.trace_id);
        assert_eq!(extracted.baggage.get("user_id"), Some(&"12345".to_string()));
    }

    #[test]
    fn test_trace_context_child() {
        let parent = TraceContext::new().with_baggage("key", "value");
        let child = parent.child();

        assert_eq!(parent.trace_id, child.trace_id);
        assert_ne!(parent.span_id, child.span_id);
        assert_eq!(parent.baggage, child.baggage);
    }

    #[test]
    fn test_message_tracing_ext() {
        let ctx = TraceContext::new().with_baggage("test", "value");
        let message = Message::new("test/topic", b"data".to_vec()).with_trace_context(&ctx);

        let extracted = message.trace_context().unwrap();
        assert_eq!(extracted.trace_id, ctx.trace_id);
    }

    #[test]
    fn test_tracer_sampling() {
        let config = TracingConfig {
            enabled: true,
            sampling_rate: 0.0,
            ..Default::default()
        };
        let tracer = MessageTracer::new(config);
        assert!(!tracer.should_trace());

        let config = TracingConfig {
            enabled: true,
            sampling_rate: 1.0,
            ..Default::default()
        };
        let tracer = MessageTracer::new(config);
        assert!(tracer.should_trace());
    }
}
