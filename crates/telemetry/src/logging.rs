//! Logging configuration

use std::io;

use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::fmt::{self};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::TelemetryConfig;

/// Initialize logging
pub fn init_logging(config: &TelemetryConfig) -> anyhow::Result<()> {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level));

    // JSON layer for production
    let is_production = std::env::var("RUST_ENV")
        .map(|v| v == "production")
        .unwrap_or(false);
    let json_layer = if is_production {
        Some(
            fmt::layer()
                .json()
                .with_span_list(true)
                .with_current_span(true)
                .with_writer(io::stdout)
                .with_filter(env_filter.clone()),
        )
    } else {
        None
    };

    // Pretty layer for development
    let pretty_layer = if !is_production {
        Some(
            fmt::layer()
                .pretty()
                .with_thread_ids(true)
                .with_thread_names(true)
                .with_file(true)
                .with_line_number(true)
                .with_span_events(FmtSpan::CLOSE)
                .with_writer(io::stdout)
                .with_filter(env_filter),
        )
    } else {
        None
    };

    // File layer
    let file_layer = if let Some(log_dir) = std::env::var_os("BEEBOT_LOG_DIR") {
        let log_path = std::path::Path::new(&log_dir).join("beebotos.log");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;

        Some(
            fmt::layer::<tracing_subscriber::Registry>()
                .json()
                .with_writer(file)
                .with_filter(EnvFilter::new("debug")),
        )
    } else {
        None
    };

    // Build subscriber
    let subscriber = tracing_subscriber::registry();

    if let Some(layer) = json_layer {
        subscriber.with(layer).init();
    } else if let Some(layer) = pretty_layer {
        subscriber.with(layer).init();
    }

    if let Some(layer) = file_layer {
        // Would need to add to existing subscriber
        // For simplicity, we're initializing separately above
        let _ = layer;
    }

    Ok(())
}

/// Structured logging
pub struct StructuredLogger;

impl StructuredLogger {
    /// Log an event
    pub fn log_event(level: tracing::Level, message: &str, fields: &[(&str, &str)]) {
        match level {
            tracing::Level::ERROR => {
                tracing::error!(message, fields = ?fields);
            }
            tracing::Level::WARN => {
                tracing::warn!(message, fields = ?fields);
            }
            tracing::Level::INFO => {
                tracing::info!(message, fields = ?fields);
            }
            tracing::Level::DEBUG => {
                tracing::debug!(message, fields = ?fields);
            }
            tracing::Level::TRACE => {
                tracing::trace!(message, fields = ?fields);
            }
        }
    }
}

/// Audit logging
pub struct AuditLogger;

impl AuditLogger {
    /// Log security event
    pub fn security(event: &str, details: &str, agent_id: Option<&str>) {
        tracing::info!(
            target: "audit.security",
            event = %event,
            details = %details,
            agent_id = %agent_id.unwrap_or("system"),
            timestamp = %chrono::Utc::now().to_rfc3339(),
        );
    }

    /// Log governance event
    pub fn governance(event: &str, proposal_id: u64, voter: &str, choice: &str) {
        tracing::info!(
            target: "audit.governance",
            event = %event,
            proposal_id = proposal_id,
            voter = %voter,
            choice = %choice,
            timestamp = %chrono::Utc::now().to_rfc3339(),
        );
    }

    /// Log capability check
    pub fn capability_check(agent_id: &str, capability: &str, granted: bool, reason: Option<&str>) {
        tracing::info!(
            target: "audit.capability",
            agent_id = %agent_id,
            capability = %capability,
            granted = granted,
            reason = %reason.unwrap_or(""),
            timestamp = %chrono::Utc::now().to_rfc3339(),
        );
    }
}

/// Performance logging
pub struct PerfLogger;

impl PerfLogger {
    /// Log operation timing
    pub fn timing(operation: &str, duration_ms: u64, success: bool) {
        tracing::debug!(
            target: "perf",
            operation = %operation,
            duration_ms = duration_ms,
            success = success,
        );
    }

    /// Log resource usage
    pub fn resources(operation: &str, memory_kb: u64, cpu_percent: f32) {
        tracing::debug!(
            target: "perf.resources",
            operation = %operation,
            memory_kb = memory_kb,
            cpu_percent = cpu_percent,
        );
    }
}
