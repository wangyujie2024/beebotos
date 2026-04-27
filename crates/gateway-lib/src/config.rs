//! Gateway Configuration
//!
//! Production-ready configuration management with:
//! - Environment variable support
//! - Configuration file loading (JSON/YAML)
//! - Validation and defaults
//! - Secrets protection

use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;
use std::path::Path;

use secrecy::{ExposeSecret, Secret};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{info, warn};
use validator::Validate;

use crate::rate_limit::RateLimitConfig;

/// Configuration errors
#[derive(Error, Debug)]
pub enum ConfigError {
    /// IO error during config loading
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON parsing error
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    /// YAML parsing error
    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    /// Validation error
    #[error("Validation error: {0}")]
    Validation(String),

    /// Missing required environment variable
    #[error("Missing required environment variable: {0}")]
    MissingEnv(String),

    /// Invalid configuration value
    #[error("Invalid configuration: {0}")]
    Invalid(String),
}

/// Gateway configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct GatewayConfig {
    /// Server configuration
    #[validate(nested)]
    pub server: ServerConfig,

    /// JWT authentication configuration
    #[validate(nested)]
    pub jwt: JwtConfig,

    /// CORS configuration
    #[validate(nested)]
    pub cors: CorsConfig,

    /// Rate limiting configuration
    #[validate(nested)]
    pub rate_limit: RateLimitConfig,

    /// WebSocket configuration
    #[validate(nested)]
    pub websocket: WebSocketConfig,

    /// Service discovery configuration
    #[validate(nested)]
    pub discovery: DiscoveryConfig,

    /// Health check configuration
    #[validate(nested)]
    pub health: HealthConfig,

    /// Logging configuration
    pub log_level: String,

    /// Metrics configuration
    pub metrics_enabled: bool,
}

/// Trait for types that can be converted to GatewayConfig
///
/// This allows application-level configurations to be converted
/// to gateway-lib's internal configuration format.
pub trait IntoGatewayConfig {
    /// Convert to GatewayConfig
    fn into_gateway_config(&self) -> Result<GatewayConfig, ConfigError>;
}

/// Application configuration traits that can be provided by upstream crates
///
/// These are minimal trait definitions that allow gateway-lib to extract
/// configuration without depending on specific application types.
pub mod app_config {
    // Note: Deliberately not using `use super::*;` to avoid importing unused items

    /// Server configuration from application
    pub trait AppServerConfig {
        /// Returns the server host address
        fn host(&self) -> &str;
        /// Returns the server port
        fn port(&self) -> u16;
        /// Returns the request timeout in seconds
        fn timeout_seconds(&self) -> u64;
        /// Returns the maximum request body size in bytes
        fn max_body_size(&self) -> usize;
    }

    /// JWT configuration from application
    pub trait AppJwtConfig {
        /// Returns the JWT secret key
        fn secret(&self) -> String;
        /// Returns the token expiry time in minutes
        fn expiry_minutes(&self) -> u64;
        /// Returns the refresh token expiry time in minutes
        fn refresh_expiry_minutes(&self) -> u64;
        /// Returns the token issuer
        fn issuer(&self) -> &str;
        /// Returns the token audience
        fn audience(&self) -> &str;
    }

    /// CORS configuration from application
    pub trait AppCorsConfig {
        /// Returns the list of allowed origins
        fn allowed_origins(&self) -> &[String];
        /// Returns the list of allowed HTTP methods
        fn allowed_methods(&self) -> &[String];
        /// Returns the list of allowed headers
        fn allowed_headers(&self) -> &[String];
        /// Returns whether credentials are allowed
        fn allow_credentials(&self) -> bool;
        /// Returns whether any origin is allowed
        fn allow_any_origin(&self) -> bool;
    }

    /// Rate limit configuration from application
    pub trait AppRateLimitConfig {
        /// Returns whether rate limiting is enabled
        fn enabled(&self) -> bool;
        /// Returns the maximum requests per second
        fn requests_per_second(&self) -> u32;
        /// Returns the burst size for rate limiting
        fn burst_size(&self) -> u32;
        /// Returns the cooldown period in seconds
        fn cooldown_seconds(&self) -> u32;
    }

    /// Root application configuration
    pub trait AppConfig {
        /// Server configuration type
        type Server: AppServerConfig;
        /// JWT configuration type
        type Jwt: AppJwtConfig;
        /// CORS configuration type
        type Cors: AppCorsConfig;
        /// Rate limit configuration type
        type RateLimit: AppRateLimitConfig;

        /// Returns the server configuration
        fn server(&self) -> &Self::Server;
        /// Returns the JWT configuration
        fn jwt(&self) -> &Self::Jwt;
        /// Returns the CORS configuration
        fn cors(&self) -> &Self::Cors;
        /// Returns the rate limit configuration
        fn rate_limit(&self) -> &Self::RateLimit;
        /// Returns the log level
        fn log_level(&self) -> &str;
        /// Returns whether metrics are enabled
        fn metrics_enabled(&self) -> bool;
    }
}

impl GatewayConfig {
    /// Create GatewayConfig from any type implementing AppConfig
    ///
    /// This eliminates the need for manual field-by-field conversion
    /// in application code.
    pub fn from_app_config<C>(config: &C) -> Result<Self, ConfigError>
    where
        C: app_config::AppConfig,
    {
        // Import traits to make methods available
        use app_config::{AppCorsConfig, AppJwtConfig, AppRateLimitConfig, AppServerConfig};

        Ok(Self {
            server: ServerConfig {
                host: config.server().host().to_string(),
                port: config.server().port(),
                timeout_seconds: config.server().timeout_seconds(),
                max_body_size: config.server().max_body_size(),
                worker_threads: 0,
            },
            jwt: JwtConfig {
                secret: Secret::new(config.jwt().secret()),
                expiry_minutes: config.jwt().expiry_minutes(),
                refresh_expiry_minutes: config.jwt().refresh_expiry_minutes(),
                issuer: config.jwt().issuer().to_string(),
                audience: config.jwt().audience().to_string(),
            },
            cors: CorsConfig {
                allow_any_origin: config.cors().allow_any_origin(),
                allowed_origins: config.cors().allowed_origins().to_vec(),
                allowed_methods: config.cors().allowed_methods().to_vec(),
                allowed_headers: config.cors().allowed_headers().to_vec(),
                allow_credentials: config.cors().allow_credentials(),
                max_age_seconds: 3600,
            },
            rate_limit: RateLimitConfig {
                requests_per_second: config.rate_limit().requests_per_second(),
                burst_size: config.rate_limit().burst_size(),
                cooldown_seconds: config.rate_limit().cooldown_seconds(),
                enabled: config.rate_limit().enabled(),
            },
            websocket: WebSocketConfig::default(),
            discovery: DiscoveryConfig::default(),
            health: HealthConfig::default(),
            log_level: config.log_level().to_string(),
            metrics_enabled: config.metrics_enabled(),
        })
    }

    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self, ConfigError> {
        info!("Loading configuration from environment variables");

        let config = Self {
            server: ServerConfig::from_env()?,
            jwt: JwtConfig::from_env()?,
            cors: CorsConfig::from_env()?,
            rate_limit: load_rate_limit_config(),
            websocket: WebSocketConfig::from_env()?,
            discovery: DiscoveryConfig::from_env()?,
            health: HealthConfig::from_env()?,
            log_level: env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
            metrics_enabled: env::var("METRICS_ENABLED")
                .map(|v| v.parse().unwrap_or(true))
                .unwrap_or(true),
        };

        config.do_validate()?;
        Ok(config)
    }

    /// Load configuration from file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        info!("Loading configuration from file: {}", path.display());

        let content = std::fs::read_to_string(path)?;

        let config: GatewayConfig = if path
            .extension()
            .map_or(false, |e| e == "yaml" || e == "yml")
        {
            serde_yaml::from_str(&content)?
        } else {
            serde_json::from_str(&content)?
        };

        config.do_validate()?;
        Ok(config)
    }

    /// Load with env override (file + env vars)
    pub fn load<P: AsRef<Path>>(path: Option<P>) -> Result<Self, ConfigError> {
        let mut config = match path {
            Some(p) => Self::from_file(p)?,
            None => Self::from_env()?,
        };

        // Apply environment variable overrides
        config.apply_env_overrides()?;
        config.do_validate()?;

        info!("Configuration loaded successfully");
        Ok(config)
    }

    /// Apply environment variable overrides
    fn apply_env_overrides(&mut self) -> Result<(), ConfigError> {
        if let Ok(port) = env::var("PORT") {
            self.server.port = port
                .parse()
                .map_err(|e| ConfigError::Invalid(format!("Invalid PORT: {}", e)))?;
        }

        if let Ok(log_level) = env::var("LOG_LEVEL") {
            self.log_level = log_level;
        }

        if let Ok(secret) = env::var("JWT_SECRET") {
            self.jwt.secret = Secret::new(secret);
        }

        Ok(())
    }

    /// Validate the configuration
    fn do_validate(&self) -> Result<(), ConfigError> {
        use validator::Validate;

        // Validate using the derive macro
        Validate::validate(self).map_err(|e| ConfigError::Validation(format!("{}", e)))?;

        // Additional custom validations
        if self.jwt.secret.expose_secret().len() < 32 {
            warn!("JWT_SECRET is shorter than 32 characters, consider using a stronger secret");
        }

        if self.cors.allowed_origins.contains(&"*".to_string()) && !self.cors.allow_any_origin {
            return Err(ConfigError::Validation(
                "CORS allowed_origins contains '*' but allow_any_origin is false".to_string(),
            ));
        }

        // Security check: disallow allow_any_origin in production (non-debug) mode
        #[cfg(not(debug_assertions))]
        if self.cors.allow_any_origin {
            return Err(ConfigError::Validation(
                "CORS allow_any_origin is not allowed in production (non-debug) mode".to_string(),
            ));
        }

        Ok(())
    }
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            jwt: JwtConfig::default(),
            cors: CorsConfig::default(),
            rate_limit: RateLimitConfig::default(),
            websocket: WebSocketConfig::default(),
            discovery: DiscoveryConfig::default(),
            health: HealthConfig::default(),
            log_level: "info".to_string(),
            metrics_enabled: true,
        }
    }
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ServerConfig {
    /// Host to bind
    #[validate(length(min = 1))]
    pub host: String,

    /// Port to listen on
    #[validate(range(min = 1, max = 65535))]
    pub port: u16,

    /// Request timeout in seconds
    #[validate(range(min = 1, max = 300))]
    pub timeout_seconds: u64,

    /// Maximum request body size in bytes
    #[validate(range(min = 1024, max = 104857600))] // 1KB - 100MB
    pub max_body_size: usize,

    /// Number of worker threads (0 = auto)
    pub worker_threads: usize,
}

impl ServerConfig {
    fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            host: env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: env::var("PORT")
                .or_else(|_| env::var("SERVER_PORT"))
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .map_err(|e| ConfigError::Invalid(format!("Invalid port: {}", e)))?,
            timeout_seconds: env::var("SERVER_TIMEOUT")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .unwrap_or(30),
            max_body_size: env::var("SERVER_MAX_BODY_SIZE")
                .unwrap_or_else(|_| "10485760".to_string()) // 10MB default
                .parse()
                .unwrap_or(10 * 1024 * 1024),
            worker_threads: env::var("SERVER_WORKER_THREADS")
                .unwrap_or_else(|_| "0".to_string())
                .parse()
                .unwrap_or(0),
        })
    }

    /// Get socket address
    pub fn socket_addr(&self) -> Result<SocketAddr, ConfigError> {
        format!("{}:{}", self.host, self.port)
            .parse()
            .map_err(|e| ConfigError::Invalid(format!("Invalid socket address: {}", e)))
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            timeout_seconds: 30,
            max_body_size: 10 * 1024 * 1024, // 10MB
            worker_threads: 0,
        }
    }
}

/// JWT configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct JwtConfig {
    /// JWT signing secret (protect this!)
    #[serde(skip_serializing)]
    pub secret: Secret<String>,

    /// Token expiration time in minutes
    #[validate(range(min = 1, max = 10080))] // Max 1 week
    pub expiry_minutes: u64,

    /// Refresh token expiration in minutes
    #[validate(range(min = 1, max = 43200))] // Max 30 days
    pub refresh_expiry_minutes: u64,

    /// Token issuer
    #[validate(length(min = 1, max = 100))]
    pub issuer: String,

    /// Token audience
    #[validate(length(min = 1, max = 100))]
    pub audience: String,
}

impl JwtConfig {
    fn from_env() -> Result<Self, ConfigError> {
        let secret = env::var("JWT_SECRET")
            .map_err(|_| ConfigError::MissingEnv("JWT_SECRET".to_string()))?;

        if secret.len() < 32 {
            warn!("JWT_SECRET should be at least 32 characters for security");
        }

        Ok(Self {
            secret: Secret::new(secret),
            expiry_minutes: env::var("JWT_EXPIRY_MINUTES")
                .unwrap_or_else(|_| "60".to_string())
                .parse()
                .unwrap_or(60),
            refresh_expiry_minutes: env::var("JWT_REFRESH_EXPIRY_MINUTES")
                .unwrap_or_else(|_| "10080".to_string()) // 7 days
                .parse()
                .unwrap_or(10080),
            issuer: env::var("JWT_ISSUER").unwrap_or_else(|_| "beebotos".to_string()),
            audience: env::var("JWT_AUDIENCE").unwrap_or_else(|_| "beebotos-gateway".to_string()),
        })
    }

    /// Get secret for signing/verification
    pub fn secret_bytes(&self) -> &[u8] {
        self.secret.expose_secret().as_bytes()
    }
}

impl Default for JwtConfig {
    fn default() -> Self {
        // 🔴 CRITICAL SECURITY FIX: Non-debug mode requires explicit JWT_SECRET
        // Auto-generated secrets are ONLY allowed in debug/development builds
        if !cfg!(debug_assertions) {
            panic!(
                "SECURITY ERROR: JWT_SECRET must be explicitly configured in production.\nSet the \
                 JWT_SECRET environment variable with a strong secret (at least 32 \
                 characters).\nDo NOT use auto-generated secrets in production builds."
            );
        }

        // Generate a random secret for development only
        let dev_secret = format!("dev-secret-{}", uuid::Uuid::new_v4());
        warn!(
            "SECURITY WARNING: Using auto-generated JWT secret. \nThis is ONLY acceptable in \
             development. \nFor production, set JWT_SECRET environment variable."
        );

        Self {
            secret: Secret::new(dev_secret),
            expiry_minutes: 60,
            refresh_expiry_minutes: 10080,
            issuer: "beebotos".to_string(),
            audience: "beebotos-gateway".to_string(),
        }
    }
}

/// CORS configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CorsConfig {
    /// Whether to allow any origin (dangerous in production!)
    #[serde(default)]
    pub allow_any_origin: bool,

    /// List of allowed origins
    #[validate(length(max = 100))]
    pub allowed_origins: Vec<String>,

    /// Allowed HTTP methods
    pub allowed_methods: Vec<String>,

    /// Allowed headers
    pub allowed_headers: Vec<String>,

    /// Whether to allow credentials
    #[serde(default = "default_true")]
    pub allow_credentials: bool,

    /// Max age for preflight cache in seconds
    #[validate(range(max = 86400))]
    pub max_age_seconds: u32,
}

fn default_true() -> bool {
    true
}

impl CorsConfig {
    fn from_env() -> Result<Self, ConfigError> {
        let origins_str = env::var("CORS_ALLOWED_ORIGINS")
            .unwrap_or_else(|_| "http://localhost:8000".to_string());

        let allowed_origins: Vec<String> = origins_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let allow_any = env::var("CORS_ALLOW_ANY")
            .map(|v| v.parse().unwrap_or(false))
            .unwrap_or(false);

        #[cfg(not(debug_assertions))]
        if allow_any {
            panic!("CORS_ALLOW_ANY cannot be enabled in production (non-debug) mode");
        }

        #[cfg(debug_assertions)]
        if allow_any {
            warn!("CORS_ALLOW_ANY is enabled - this is insecure for production!");
        }

        Ok(Self {
            allow_any_origin: allow_any,
            allowed_origins,
            allowed_methods: env::var("CORS_ALLOWED_METHODS")
                .unwrap_or_else(|_| "GET,POST,PUT,DELETE,OPTIONS".to_string())
                .split(',')
                .map(|s| s.trim().to_string())
                .collect(),
            allowed_headers: env::var("CORS_ALLOWED_HEADERS")
                .unwrap_or_else(|_| "Content-Type,Authorization,X-Request-ID".to_string())
                .split(',')
                .map(|s| s.trim().to_string())
                .collect(),
            allow_credentials: env::var("CORS_ALLOW_CREDENTIALS")
                .map(|v| v.parse().unwrap_or(true))
                .unwrap_or(true),
            max_age_seconds: env::var("CORS_MAX_AGE")
                .unwrap_or_else(|_| "3600".to_string())
                .parse()
                .unwrap_or(3600),
        })
    }
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allow_any_origin: false,
            allowed_origins: vec!["http://localhost:8000".to_string()],
            allowed_methods: vec![
                "GET".to_string(),
                "POST".to_string(),
                "PUT".to_string(),
                "DELETE".to_string(),
                "OPTIONS".to_string(),
            ],
            allowed_headers: vec![
                "Content-Type".to_string(),
                "Authorization".to_string(),
                "X-Request-ID".to_string(),
            ],
            allow_credentials: true,
            max_age_seconds: 3600,
        }
    }
}

/// WebSocket configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct WebSocketConfig {
    /// Maximum connections allowed
    #[validate(range(min = 1, max = 100000))]
    pub max_connections: usize,

    /// Heartbeat interval in seconds
    #[validate(range(min = 5, max = 300))]
    pub heartbeat_interval_seconds: u64,

    /// Connection timeout without heartbeat
    #[validate(range(min = 10, max = 600))]
    pub heartbeat_timeout_seconds: u64,

    /// Maximum message size in bytes
    #[validate(range(min = 1024, max = 10485760))]
    pub max_message_size: usize,

    /// Whether to enable compression
    pub enable_compression: bool,
}

impl WebSocketConfig {
    fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            max_connections: env::var("WS_MAX_CONNECTIONS")
                .unwrap_or_else(|_| "10000".to_string())
                .parse()
                .unwrap_or(10000),
            heartbeat_interval_seconds: env::var("WS_HEARTBEAT_INTERVAL")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .unwrap_or(30),
            heartbeat_timeout_seconds: env::var("WS_HEARTBEAT_TIMEOUT")
                .unwrap_or_else(|_| "60".to_string())
                .parse()
                .unwrap_or(60),
            max_message_size: env::var("WS_MAX_MESSAGE_SIZE")
                .unwrap_or_else(|_| "1048576".to_string()) // 1MB
                .parse()
                .unwrap_or(1024 * 1024),
            enable_compression: env::var("WS_ENABLE_COMPRESSION")
                .map(|v| v.parse().unwrap_or(true))
                .unwrap_or(true),
        })
    }
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            max_connections: 10000,
            heartbeat_interval_seconds: 30,
            heartbeat_timeout_seconds: 60,
            max_message_size: 1024 * 1024, // 1MB
            enable_compression: true,
        }
    }
}

/// Service discovery configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct DiscoveryConfig {
    /// Discovery type: static, consul, etcd, kubernetes
    #[validate(length(min = 1))]
    pub provider: String,

    /// Static service definitions (for provider = static)
    pub static_services: HashMap<String, ServiceDefinition>,

    /// Consul configuration
    pub consul_url: Option<String>,

    /// Kubernetes namespace
    pub k8s_namespace: Option<String>,

    /// Service refresh interval in seconds
    #[validate(range(min = 5))]
    pub refresh_interval_seconds: u64,
}

impl DiscoveryConfig {
    fn from_env() -> Result<Self, ConfigError> {
        let provider = env::var("DISCOVERY_PROVIDER").unwrap_or_else(|_| "static".to_string());

        let mut static_services = HashMap::new();

        // Parse static services from env if provided
        if let Ok(services_str) = env::var("STATIC_SERVICES") {
            // Format: "service1=http://host1:8080,service2=http://host2:8080"
            for service_def in services_str.split(',') {
                if let Some((name, url)) = service_def.split_once('=') {
                    static_services.insert(
                        name.trim().to_string(),
                        ServiceDefinition {
                            name: name.trim().to_string(),
                            url: url.trim().to_string(),
                            health_path: "/health".to_string(),
                            timeout_seconds: 30,
                        },
                    );
                }
            }
        }

        Ok(Self {
            provider,
            static_services,
            consul_url: env::var("CONSUL_URL").ok(),
            k8s_namespace: env::var("K8S_NAMESPACE").ok(),
            refresh_interval_seconds: env::var("DISCOVERY_REFRESH_INTERVAL")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .unwrap_or(30),
        })
    }
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            provider: "static".to_string(),
            static_services: HashMap::new(),
            consul_url: None,
            k8s_namespace: None,
            refresh_interval_seconds: 30,
        }
    }
}

/// Service definition for static discovery
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ServiceDefinition {
    /// Service name
    #[validate(length(min = 1))]
    pub name: String,

    /// Service URL
    #[validate(url)]
    pub url: String,

    /// Health check endpoint path
    #[validate(length(min = 1))]
    pub health_path: String,

    /// Request timeout in seconds
    #[validate(range(min = 1))]
    pub timeout_seconds: u64,
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct HealthConfig {
    /// Health check endpoint path
    #[validate(length(min = 1))]
    pub path: String,

    /// Readiness check path
    #[validate(length(min = 1))]
    pub ready_path: String,

    /// Liveness check path
    #[validate(length(min = 1))]
    pub live_path: String,

    /// Check interval in seconds
    #[validate(range(min = 5))]
    pub check_interval_seconds: u64,
}

impl HealthConfig {
    fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            path: env::var("HEALTH_PATH").unwrap_or_else(|_| "/health".to_string()),
            ready_path: env::var("READY_PATH").unwrap_or_else(|_| "/ready".to_string()),
            live_path: env::var("LIVE_PATH").unwrap_or_else(|_| "/live".to_string()),
            check_interval_seconds: env::var("HEALTH_CHECK_INTERVAL")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .unwrap_or(30),
        })
    }
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            path: "/health".to_string(),
            ready_path: "/ready".to_string(),
            live_path: "/live".to_string(),
            check_interval_seconds: 30,
        }
    }
}

/// Load rate limit config with env override
fn load_rate_limit_config() -> RateLimitConfig {
    RateLimitConfig {
        requests_per_second: env::var("RATE_LIMIT_RPS")
            .unwrap_or_else(|_| "100".to_string())
            .parse()
            .unwrap_or(100),
        burst_size: env::var("RATE_LIMIT_BURST")
            .unwrap_or_else(|_| "200".to_string())
            .parse()
            .unwrap_or(200),
        cooldown_seconds: env::var("RATE_LIMIT_WINDOW")
            .unwrap_or_else(|_| "60".to_string())
            .parse()
            .unwrap_or(60),
        enabled: env::var("RATE_LIMIT_ENABLED")
            .map(|v| v.parse().unwrap_or(true))
            .unwrap_or(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = GatewayConfig::default();
        assert_eq!(config.server.port, 8080);
        assert!(!config.cors.allow_any_origin);
    }

    #[test]
    fn test_server_socket_addr() {
        let config = ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 8080,
            ..Default::default()
        };

        let addr = config.socket_addr().unwrap();
        assert_eq!(addr.to_string(), "127.0.0.1:8080");
    }

    #[test]
    fn test_jwt_config_secret() {
        let config = JwtConfig {
            secret: Secret::new("my-super-secret-key-32-chars-long".to_string()),
            ..Default::default()
        };

        assert_eq!(config.secret_bytes(), b"my-super-secret-key-32-chars-long");
    }

    #[test]
    fn test_cors_config_validation() {
        let config = CorsConfig {
            allow_any_origin: false,
            allowed_origins: vec!["https://example.com".to_string()],
            ..Default::default()
        };

        assert!(!config.allow_any_origin);
        assert!(config
            .allowed_origins
            .contains(&"https://example.com".to_string()));
    }
}
