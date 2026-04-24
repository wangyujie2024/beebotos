//! Configuration management for Message Bus
//!
//! Supports loading configuration from:
//! - Environment variables
//! - Configuration files (YAML, JSON, TOML)
//! - Programmatic configuration
//!
//! # Example
//!
//! ```rust,ignore
//! use beebotos_message_bus::config::MessageBusConfig;
//!
//! async fn example() -> Result<(), Box<dyn std::error::Error>> {
//!     // Load from file
//!     let config = MessageBusConfig::from_file("config/message-bus.yaml").await?;
//!
//!     // Or from environment
//!     let config = MessageBusConfig::from_env()?;
//!     Ok(())
//! }
//! ```

use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Main configuration struct for message bus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageBusConfig {
    /// Transport type: "memory" or "grpc"
    #[serde(default = "default_transport")]
    pub transport: String,

    /// gRPC transport configuration
    #[serde(default)]
    pub grpc: GrpcConfig,

    /// Resource limits
    #[serde(default)]
    pub limits: LimitsConfig,

    /// Metrics configuration
    #[serde(default)]
    pub metrics: MetricsConfig,

    /// Logging configuration
    #[serde(default)]
    pub logging: LoggingConfig,
}

impl MessageBusConfig {
    /// Create default configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Load configuration from a YAML file
    pub async fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| ConfigError::FileRead(e.to_string()))?;

        let config: MessageBusConfig =
            serde_yaml::from_str(&content).map_err(|e| ConfigError::Parse(e.to_string()))?;

        Ok(config)
    }

    /// Load configuration from environment variables
    ///
    /// Variables:
    /// - MESSAGE_BUS_TRANSPORT - Transport type
    /// - MESSAGE_BUS_GRPC_BIND_ADDR - gRPC bind address
    /// - MESSAGE_BUS_GRPC_NODE_ID - Node ID
    /// - MESSAGE_BUS_LIMITS_MAX_MESSAGE_SIZE - Max message size
    pub fn from_env() -> Result<Self, ConfigError> {
        let mut config = MessageBusConfig::default();

        if let Ok(transport) = std::env::var("MESSAGE_BUS_TRANSPORT") {
            config.transport = transport;
        }

        if let Ok(addr) = std::env::var("MESSAGE_BUS_GRPC_BIND_ADDR") {
            config.grpc.bind_addr = addr
                .parse()
                .map_err(|e| ConfigError::Parse(format!("Invalid bind address: {}", e)))?;
        }

        if let Ok(node_id) = std::env::var("MESSAGE_BUS_GRPC_NODE_ID") {
            config.grpc.node_id = Some(node_id);
        }

        if let Ok(seeds) = std::env::var("MESSAGE_BUS_GRPC_CLUSTER_SEEDS") {
            config.grpc.cluster_seeds = seeds
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
        }

        if let Ok(size) = std::env::var("MESSAGE_BUS_LIMITS_MAX_MESSAGE_SIZE") {
            config.limits.max_message_size = parse_size(&size)
                .map_err(|e| ConfigError::Parse(format!("Invalid size: {}", e)))?;
        }

        Ok(config)
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        match self.transport.as_str() {
            "memory" | "grpc" => {}
            _ => {
                return Err(ConfigError::Invalid(format!(
                    "Invalid transport: {}. Must be 'memory' or 'grpc'",
                    self.transport
                )))
            }
        }

        if self.limits.max_message_size == 0 {
            return Err(ConfigError::Invalid(
                "max_message_size must be greater than 0".to_string(),
            ));
        }

        if self.limits.max_topics == 0 {
            return Err(ConfigError::Invalid(
                "max_topics must be greater than 0".to_string(),
            ));
        }

        Ok(())
    }
}

impl Default for MessageBusConfig {
    fn default() -> Self {
        Self {
            transport: default_transport(),
            grpc: GrpcConfig::default(),
            limits: LimitsConfig::default(),
            metrics: MetricsConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

/// gRPC transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcConfig {
    /// Bind address for gRPC server
    #[serde(default = "default_bind_addr")]
    pub bind_addr: SocketAddr,

    /// Node ID (auto-generated if not set)
    #[serde(default)]
    pub node_id: Option<String>,

    /// Cluster seed nodes
    #[serde(default)]
    pub cluster_seeds: Vec<SocketAddr>,

    /// Keepalive interval
    #[serde(default = "default_keepalive_interval")]
    #[serde(with = "humantime_serde")]
    pub keepalive_interval: Duration,

    /// Connection timeout
    #[serde(default = "default_connect_timeout")]
    #[serde(with = "humantime_serde")]
    pub connect_timeout: Duration,

    /// Maximum message size
    #[serde(default = "default_max_message_size")]
    pub max_message_size: usize,

    /// TLS configuration
    #[serde(default)]
    pub tls: TlsConfig,
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            bind_addr: default_bind_addr(),
            node_id: None,
            cluster_seeds: vec![],
            keepalive_interval: default_keepalive_interval(),
            connect_timeout: default_connect_timeout(),
            max_message_size: default_max_message_size(),
            tls: TlsConfig::default(),
        }
    }
}

/// Resource limits configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitsConfig {
    /// Maximum message size in bytes
    #[serde(default = "default_max_message_size")]
    pub max_message_size: usize,

    /// Maximum number of topics
    #[serde(default = "default_max_topics")]
    pub max_topics: usize,

    /// Maximum subscriptions per topic
    #[serde(default = "default_max_subscriptions")]
    pub max_subscriptions_per_topic: usize,

    /// Maximum queue depth per subscription
    #[serde(default = "default_queue_depth")]
    pub queue_depth: usize,

    /// Maximum number of connections (gRPC)
    #[serde(default = "default_max_connections")]
    pub max_connections: usize,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_message_size: default_max_message_size(),
            max_topics: default_max_topics(),
            max_subscriptions_per_topic: default_max_subscriptions(),
            queue_depth: default_queue_depth(),
            max_connections: default_max_connections(),
        }
    }
}

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Enable metrics collection
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Metrics endpoint bind address
    #[serde(default = "default_metrics_addr")]
    pub endpoint: SocketAddr,

    /// Metrics export interval
    #[serde(default = "default_metrics_interval")]
    #[serde(with = "humantime_serde")]
    pub interval: Duration,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            endpoint: default_metrics_addr(),
            interval: default_metrics_interval(),
        }
    }
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level
    #[serde(default = "default_log_level")]
    pub level: String,

    /// Log format: "json" or "pretty"
    #[serde(default = "default_log_format")]
    pub format: String,

    /// Optional log file path
    pub file: Option<String>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_log_format(),
            file: None,
        }
    }
}

/// TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Enable TLS
    #[serde(default)]
    pub enabled: bool,

    /// Certificate file path
    pub cert_file: Option<String>,

    /// Private key file path
    pub key_file: Option<String>,

    /// CA certificate file path (for client verification)
    pub ca_file: Option<String>,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cert_file: None,
            key_file: None,
            ca_file: None,
        }
    }
}

/// Configuration errors
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    FileRead(String),

    #[error("Failed to parse config: {0}")]
    Parse(String),

    #[error("Invalid configuration: {0}")]
    Invalid(String),
}

// Default value functions
fn default_transport() -> String {
    "memory".to_string()
}

fn default_bind_addr() -> SocketAddr {
    "0.0.0.0:50051"
        .parse()
        .expect("hardcoded default bind address should be valid")
}

fn default_keepalive_interval() -> Duration {
    Duration::from_secs(30)
}

fn default_connect_timeout() -> Duration {
    Duration::from_secs(10)
}

fn default_max_message_size() -> usize {
    10 * 1024 * 1024 // 10 MB
}

fn default_max_topics() -> usize {
    10000
}

fn default_max_subscriptions() -> usize {
    1000
}

fn default_queue_depth() -> usize {
    10000
}

fn default_max_connections() -> usize {
    1000
}

fn default_true() -> bool {
    true
}

fn default_metrics_addr() -> SocketAddr {
    "0.0.0.0:9090"
        .parse()
        .expect("hardcoded default metrics address should be valid")
}

fn default_metrics_interval() -> Duration {
    Duration::from_secs(60)
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> String {
    "pretty".to_string()
}

/// Parse size string like "10MB", "1GB" to bytes
fn parse_size(s: &str) -> Result<usize, String> {
    let s = s.trim().to_uppercase();

    if let Some(num) = s.strip_suffix("GB") {
        num.trim()
            .parse::<usize>()
            .map(|n| n * 1024 * 1024 * 1024)
            .map_err(|e| e.to_string())
    } else if let Some(num) = s.strip_suffix("MB") {
        num.trim()
            .parse::<usize>()
            .map(|n| n * 1024 * 1024)
            .map_err(|e| e.to_string())
    } else if let Some(num) = s.strip_suffix("KB") {
        num.trim()
            .parse::<usize>()
            .map(|n| n * 1024)
            .map_err(|e| e.to_string())
    } else {
        s.parse::<usize>().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MessageBusConfig::default();
        assert_eq!(config.transport, "memory");
        assert_eq!(config.limits.max_message_size, 10 * 1024 * 1024);
    }

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("10").unwrap(), 10);
        assert_eq!(parse_size("10KB").unwrap(), 10 * 1024);
        assert_eq!(parse_size("10MB").unwrap(), 10 * 1024 * 1024);
        assert_eq!(parse_size("1GB").unwrap(), 1024 * 1024 * 1024);
    }

    #[test]
    fn test_config_validation() {
        let config = MessageBusConfig::default();
        assert!(config.validate().is_ok());

        let mut invalid = config.clone();
        invalid.transport = "invalid".to_string();
        assert!(invalid.validate().is_err());

        let mut invalid = config;
        invalid.limits.max_message_size = 0;
        assert!(invalid.validate().is_err());
    }
}
