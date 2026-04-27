//! Structured logging for BeeBotOS CLI
//!
//! Provides configurable logging with support for both human-readable
//! and structured (JSON) output formats.

#![allow(dead_code)]

use std::io::Write;
use std::sync::Mutex;
use std::time::SystemTime;

use serde::Serialize;

/// Log level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl LogLevel {
    /// Parse log level from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "trace" => Some(LogLevel::Trace),
            "debug" => Some(LogLevel::Debug),
            "info" => Some(LogLevel::Info),
            "warn" | "warning" => Some(LogLevel::Warn),
            "error" => Some(LogLevel::Error),
            _ => None,
        }
    }

    /// Convert to string
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Trace => "TRACE",
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }

    /// Get ANSI color code
    pub fn color(&self) -> &'static str {
        match self {
            LogLevel::Trace => "\x1b[90m", // Gray
            LogLevel::Debug => "\x1b[36m", // Cyan
            LogLevel::Info => "\x1b[32m",  // Green
            LogLevel::Warn => "\x1b[33m",  // Yellow
            LogLevel::Error => "\x1b[31m", // Red
        }
    }
}

/// Log output format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogFormat {
    /// Human-readable text format
    #[default]
    Text,
    /// JSON structured format
    Json,
}

/// Logger configuration
#[derive(Debug, Clone)]
pub struct LoggerConfig {
    /// Minimum log level
    pub level: LogLevel,
    /// Output format
    pub format: LogFormat,
    /// Enable colors (only for text format)
    pub colors: bool,
    /// Include timestamp
    pub timestamp: bool,
    /// Include target/module path
    pub target: bool,
}

impl Default for LoggerConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
            format: LogFormat::Text,
            colors: true,
            timestamp: true,
            target: false,
        }
    }
}

/// Structured log entry
#[derive(Debug, Serialize)]
struct LogEntry {
    timestamp: String,
    level: String,
    target: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    span_id: Option<String>,
    #[serde(flatten)]
    fields: serde_json::Map<String, serde_json::Value>,
}

/// Logger instance
pub struct Logger {
    config: LoggerConfig,
    output: Mutex<Box<dyn Write + Send>>,
}

impl Logger {
    /// Create a new logger with default config
    pub fn new() -> Self {
        Self::with_config(LoggerConfig::default())
    }

    /// Create a new logger with custom config
    pub fn with_config(config: LoggerConfig) -> Self {
        Self {
            config,
            output: Mutex::new(Box::new(std::io::stderr())),
        }
    }

    /// Check if debug level is enabled
    pub fn is_debug_enabled(&self) -> bool {
        self.config.level <= LogLevel::Debug
    }

    /// Check if a specific log level is enabled
    pub fn is_enabled(&self, level: LogLevel) -> bool {
        level >= self.config.level
    }

    /// Log a message
    pub fn log(&self, level: LogLevel, target: &str, message: &str) {
        if level < self.config.level {
            return;
        }

        match self.config.format {
            LogFormat::Text => self.log_text(level, target, message),
            LogFormat::Json => self.log_json(level, target, message),
        }
    }

    fn log_text(&self, level: LogLevel, target: &str, message: &str) {
        let mut output = self.output.lock().unwrap();

        if self.config.timestamp {
            let timestamp = format_timestamp();
            write!(output, "[{}] ", timestamp).unwrap();
        }

        if self.config.colors {
            write!(output, "{}{}\x1b[0m", level.color(), level.as_str()).unwrap();
        } else {
            write!(output, "{}", level.as_str()).unwrap();
        }

        if self.config.target {
            write!(output, " [{}]", target).unwrap();
        }

        writeln!(output, ": {}", message).unwrap();
    }

    fn log_json(&self, level: LogLevel, target: &str, message: &str) {
        let entry = LogEntry {
            timestamp: format_timestamp_iso(),
            level: level.as_str().to_string(),
            target: target.to_string(),
            message: message.to_string(),
            span_id: None,
            fields: serde_json::Map::new(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let mut output = self.output.lock().unwrap();
        writeln!(output, "{}", json).unwrap();
    }

    /// Set minimum log level
    pub fn set_level(&mut self, level: LogLevel) {
        self.config.level = level;
    }

    /// Set output format
    pub fn set_format(&mut self, format: LogFormat) {
        self.config.format = format;
    }
}

impl Default for Logger {
    fn default() -> Self {
        Self::new()
    }
}

/// Format timestamp for display
fn format_timestamp() -> String {
    let now = SystemTime::now();
    let duration = now.duration_since(SystemTime::UNIX_EPOCH).unwrap();
    let secs = duration.as_secs();
    let millis = duration.subsec_millis();

    // Simple formatting without chrono dependency
    let datetime = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
    let time = std::time::SystemTime::now();
    let _elapsed = time.duration_since(std::time::UNIX_EPOCH).unwrap();

    format!("{}.{:03}", format_system_time(datetime), millis)
}

fn format_system_time(time: SystemTime) -> String {
    let duration = time.duration_since(std::time::UNIX_EPOCH).unwrap();
    let secs = duration.as_secs();

    // Format as YYYY-MM-DD HH:MM:SS (simplified)
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    let secs = secs % 60;

    // Rough approximation - for production use chrono crate
    format!(
        "2024-01-{:02} {:02}:{:02}:{:02}",
        (days % 30) + 1,
        hours,
        mins,
        secs
    )
}

/// Format timestamp in ISO 8601 format
fn format_timestamp_iso() -> String {
    let now = SystemTime::now();
    let duration = now.duration_since(SystemTime::UNIX_EPOCH).unwrap();
    let secs = duration.as_secs();

    // Simplified ISO format
    format!(
        "2024-01-15T{:02}:{:02}:{:02}Z",
        (secs / 3600) % 24,
        (secs / 60) % 60,
        secs % 60
    )
}

use std::sync::OnceLock;

/// Global logger instance
static LOGGER: OnceLock<Logger> = OnceLock::new();

/// Initialize the global logger
pub fn init(config: LoggerConfig) {
    let _ = LOGGER.set(Logger::with_config(config));
}

/// Get the global logger
pub fn logger() -> &'static Logger {
    LOGGER.get_or_init(Logger::new)
}

/// Log macros
#[macro_export]
macro_rules! log_trace {
    ($($arg:tt)*) => {
        $crate::logging::logger().log($crate::logging::LogLevel::Trace, module_path!(), &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        $crate::logging::logger().log($crate::logging::LogLevel::Debug, module_path!(), &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::logging::logger().log($crate::logging::LogLevel::Info, module_path!(), &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        $crate::logging::logger().log($crate::logging::LogLevel::Warn, module_path!(), &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::logging::logger().log($crate::logging::LogLevel::Error, module_path!(), &format!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_ordering() {
        assert!(LogLevel::Trace < LogLevel::Debug);
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Error);
    }

    #[test]
    fn test_log_level_from_str() {
        assert_eq!(LogLevel::from_str("debug"), Some(LogLevel::Debug));
        assert_eq!(LogLevel::from_str("DEBUG"), Some(LogLevel::Debug));
        assert_eq!(LogLevel::from_str("invalid"), None);
    }

    #[test]
    fn test_logger_config_default() {
        let config = LoggerConfig::default();
        assert_eq!(config.level, LogLevel::Info);
        assert_eq!(config.format, LogFormat::Text);
        assert!(config.colors);
    }

    #[test]
    fn test_format_timestamp() {
        let ts = format_timestamp();
        assert!(ts.contains("202")); // Should start with year 202x
    }
}
