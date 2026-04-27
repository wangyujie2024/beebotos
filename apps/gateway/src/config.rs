//! Unified Configuration Management (TOML Style)
//!
//! Single TOML configuration file for all channels and LLM settings.
//! Follows the same pattern as beebot project.

use std::collections::HashMap;
use std::net::SocketAddr;

use config::{Config, ConfigError, Environment, File};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::color_theme::{ColorTheme, WizardConfig};

/// Unified BeeBotOS configuration (TOML format)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BeeBotOSConfig {
    #[serde(default = "default_system_name")]
    pub system_name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub jwt: JwtConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsConfig>,
    #[serde(default)]
    pub models: ModelsConfig,
    #[serde(default)]
    pub channels: ChannelsConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub metrics: MetricsConfig,
    #[serde(default)]
    pub tracing: TracingConfig,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub services: Option<ServicesConfig>,
    #[serde(default)]
    pub blockchain: BlockchainConfig,
    /// Wizard configuration for interactive setup
    #[serde(default)]
    pub wizard: WizardConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_grpc_port")]
    pub grpc_port: u16,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(default = "default_max_body_size_mb")]
    pub max_body_size_mb: usize,
    #[serde(default)]
    pub cors: CorsConfig,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CorsConfig {
    #[serde(default = "default_allowed_origins")]
    pub allowed_origins: Vec<String>,
    #[serde(default = "default_allowed_methods")]
    pub allowed_methods: Vec<String>,
    #[serde(default = "default_allowed_headers")]
    pub allowed_headers: Vec<String>,
    #[serde(default)]
    pub allow_credentials: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    #[serde(default = "default_database_url")]
    pub url: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_seconds: u64,
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_seconds: u64,
    #[serde(default = "default_run_migrations")]
    pub run_migrations: bool,
}

fn default_max_connections() -> u32 {
    20
}
fn default_min_connections() -> u32 {
    5
}
fn default_connect_timeout() -> u64 {
    10
}
fn default_idle_timeout() -> u64 {
    600
}
fn default_run_migrations() -> bool {
    true
}
fn default_database_url() -> String {
    "sqlite:data/beebotos.db".to_string()
}

impl Default for BeeBotOSConfig {
    fn default() -> Self {
        Self {
            system_name: default_system_name(),
            version: default_version(),
            server: ServerConfig::default(),
            database: DatabaseConfig::default(),
            jwt: JwtConfig::default(),
            tls: None,
            models: ModelsConfig::default(),
            channels: ChannelsConfig::default(),
            logging: LoggingConfig::default(),
            metrics: MetricsConfig::default(),
            tracing: TracingConfig::default(),
            rate_limit: RateLimitConfig::default(),
            security: SecurityConfig::default(),
            services: None,
            blockchain: BlockchainConfig::default(),
            wizard: WizardConfig::default(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            grpc_port: default_grpc_port(),
            timeout_seconds: default_timeout_seconds(),
            max_body_size_mb: default_max_body_size_mb(),
            cors: CorsConfig::default(),
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: default_database_url(),
            max_connections: default_max_connections(),
            min_connections: default_min_connections(),
            connect_timeout_seconds: default_connect_timeout(),
            idle_timeout_seconds: default_idle_timeout(),
            run_migrations: default_run_migrations(),
        }
    }
}

fn default_enabled() -> bool {
    true
}
fn default_max_size_mb() -> u32 {
    100
}
fn default_max_files() -> u32 {
    10
}
fn default_metrics_endpoint() -> String {
    "0.0.0.0:9090".to_string()
}
fn default_interval_seconds() -> u64 {
    60
}
fn default_sample_rate() -> f32 {
    0.1
}
fn default_requests_per_second() -> u32 {
    10
}
fn default_burst_size() -> u32 {
    50
}
fn default_cooldown_seconds() -> u64 {
    60
}
fn default_default_provider() -> String {
    "kimi".to_string()
}
fn default_max_tokens() -> u32 {
    4096
}
fn default_system_prompt() -> String {
    "You are a helpful assistant.".to_string()
}
fn default_request_timeout() -> u64 {
    90
}
fn default_media_storage_path() -> String {
    "./data/media".to_string()
}
fn default_max_file_size_mb() -> u32 {
    50
}
fn default_context_window_size() -> usize {
    20
}
fn default_auto_reply() -> bool {
    true
}
fn default_enable_typing_indicator() -> bool {
    true
}
fn default_log_level() -> String {
    "info".to_string()
}
fn default_log_format() -> String {
    "json".to_string()
}
fn default_log_file() -> String {
    "./data/logs/beebotos.log".to_string()
}
fn default_allowed_webhook_ips() -> Vec<String> {
    vec!["0.0.0.0/0".to_string()]
}
fn default_verify_signatures() -> bool {
    true
}
fn default_encryption_enabled() -> bool {
    true
}
fn default_expiry_hours() -> i64 {
    24
}
fn default_refresh_expiry_hours() -> i64 {
    168
}
fn default_issuer() -> String {
    "beebotos-gateway".to_string()
}
fn default_audience() -> String {
    "beebotos-api".to_string()
}
fn default_host() -> String {
    "0.0.0.0".to_string()
}
fn default_port() -> u16 {
    8080
}
fn default_grpc_port() -> u16 {
    50051
}
fn default_timeout_seconds() -> u64 {
    30
}
fn default_max_body_size_mb() -> usize {
    10
}
fn default_allowed_origins() -> Vec<String> {
    vec!["*".to_string()]
}
fn default_allowed_methods() -> Vec<String> {
    vec![
        "GET".to_string(),
        "POST".to_string(),
        "PUT".to_string(),
        "DELETE".to_string(),
        "OPTIONS".to_string(),
    ]
}
fn default_allowed_headers() -> Vec<String> {
    vec!["Content-Type".to_string(), "Authorization".to_string()]
}
fn default_kernel_url() -> String {
    "http://localhost:9000".to_string()
}
fn default_kernel_timeout() -> u64 {
    30
}
fn default_chain_url() -> String {
    "http://localhost:8545".to_string()
}
fn default_chain_timeout() -> u64 {
    30
}
fn default_system_name() -> String {
    "BeeBotOS".to_string()
}
fn default_version() -> String {
    "2.0.0".to_string()
}
fn default_jwt_secret() -> SecretString {
    SecretString::new(String::new())
}

#[derive(Clone, Deserialize, Serialize)]
pub struct JwtConfig {
    #[serde(skip_serializing, default = "default_jwt_secret")]
    pub secret: SecretString,
    #[serde(default = "default_expiry_hours")]
    pub expiry_hours: i64,
    #[serde(default = "default_refresh_expiry_hours")]
    pub refresh_expiry_hours: i64,
    #[serde(default = "default_issuer")]
    pub issuer: String,
    #[serde(default = "default_audience")]
    pub audience: String,
}

impl Default for JwtConfig {
    fn default() -> Self {
        Self {
            secret: SecretString::new("placeholder-jwt-secret-minimum-32-characters".to_string()),
            expiry_hours: default_expiry_hours(),
            refresh_expiry_hours: default_refresh_expiry_hours(),
            issuer: default_issuer(),
            audience: default_audience(),
        }
    }
}

impl std::fmt::Debug for JwtConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JwtConfig")
            .field("secret", &"[REDACTED]")
            .field("expiry_hours", &self.expiry_hours)
            .field("refresh_expiry_hours", &self.refresh_expiry_hours)
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .finish()
    }
}

/// Models/LLM configuration
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ModelsConfig {
    #[serde(default = "default_default_provider")]
    pub default_provider: String,
    #[serde(default)]
    pub fallback_chain: Vec<String>,
    #[serde(default)]
    pub cost_optimization: bool,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
    #[serde(default = "default_request_timeout")]
    pub request_timeout: u64,
    #[serde(flatten)]
    pub providers: HashMap<String, ModelProviderConfig>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ModelProviderConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default)]
    pub temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<usize>,
}

/// Channels configuration - flattened structure like beebot
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ChannelsConfig {
    // Global settings
    #[serde(default)]
    pub auto_download_media: bool,
    #[serde(default = "default_media_storage_path")]
    pub media_storage_path: String,
    #[serde(default = "default_max_file_size_mb")]
    pub max_file_size_mb: u32,
    #[serde(default = "default_context_window_size")]
    pub context_window_size: usize,
    #[serde(default = "default_auto_reply")]
    pub auto_reply: bool,
    #[serde(default = "default_enable_typing_indicator")]
    pub enable_typing_indicator: bool,
    #[serde(default)]
    pub enabled_platforms: Vec<String>,
    /// Default agent ID for channel messages when no specific binding exists
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_agent_id: Option<String>,

    // Individual channel configs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lark: Option<ChannelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dingtalk: Option<ChannelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telegram: Option<ChannelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discord: Option<ChannelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slack: Option<ChannelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wechat: Option<ChannelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub personal_wechat: Option<ChannelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webchat: Option<ChannelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub teams: Option<ChannelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub twitter: Option<ChannelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub whatsapp: Option<ChannelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal: Option<ChannelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matrix: Option<ChannelConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imessage: Option<ChannelConfig>,
}

/// Individual channel configuration
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ChannelConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(flatten)]
    pub settings: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_format")]
    pub format: String,
    #[serde(default = "default_log_file")]
    pub file: String,
    #[serde(default)]
    pub rotation: LogRotationConfig,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LogRotationConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_max_size_mb")]
    pub max_size_mb: u32,
    #[serde(default = "default_max_files")]
    pub max_files: u32,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MetricsConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_metrics_endpoint")]
    pub endpoint: String,
    #[serde(default = "default_interval_seconds")]
    pub interval_seconds: u64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TracingConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otel_endpoint: Option<String>,
    #[serde(default = "default_sample_rate")]
    pub sample_rate: f32,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RateLimitConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_requests_per_second")]
    pub requests_per_second: u32,
    #[serde(default = "default_burst_size")]
    pub burst_size: u32,
    #[serde(default = "default_cooldown_seconds")]
    pub cooldown_seconds: u64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SecurityConfig {
    #[serde(default = "default_allowed_webhook_ips")]
    pub allowed_webhook_ips: Vec<String>,
    #[serde(default = "default_verify_signatures")]
    pub verify_webhook_signatures: bool,
    #[serde(default = "default_encryption_enabled")]
    pub encryption_enabled: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TlsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cert_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ca_path: Option<String>,
    #[serde(default)]
    pub mutual_tls: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ServicesConfig {
    #[serde(default = "default_kernel_url")]
    pub kernel_url: String,
    #[serde(default = "default_kernel_timeout")]
    pub kernel_timeout_seconds: u64,
    #[serde(default = "default_chain_url")]
    pub chain_url: String,
    #[serde(default = "default_chain_timeout")]
    pub chain_timeout_seconds: u64,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct BlockchainConfig {
    pub enabled: bool,
    pub chain_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpc_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_wallet_mnemonic: Option<String>,
    /// AgentIdentity contract address (hex string with 0x prefix)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_contract_address: Option<String>,
    /// AgentRegistry contract address (hex string with 0x prefix)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry_contract_address: Option<String>,
    /// AgentDAO contract address (hex string with 0x prefix)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dao_contract_address: Option<String>,
    /// SkillNFT contract address (hex string with 0x prefix)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_nft_contract_address: Option<String>,
}

impl BeeBotOSConfig {
    /// Load configuration from unified TOML file
    pub fn load() -> Result<Self, ConfigError> {
        // 统一从项目根目录 config/beebotos.toml 读取主配置
        let config_paths = ["config/beebotos.toml"];
        let mut config_content = String::new();
        let mut config_dir = std::path::PathBuf::from(".");
        for path in &config_paths {
            if std::path::Path::new(path).exists() {
                config_content = std::fs::read_to_string(path).unwrap_or_else(|e| {
                    eprintln!("[Config] Failed to read {}: {}", path, e);
                    String::new()
                });
                config_dir = std::path::Path::new(path)
                    .parent()
                    .map(|p| {
                        std::env::current_dir()
                            .map(|cwd| cwd.join(p))
                            .unwrap_or_else(|_| p.to_path_buf())
                    })
                    .unwrap_or_else(|| std::path::PathBuf::from("."));
                break;
            }
        }

        let config = Config::builder()
            // Main configuration file (TOML)
            .add_source(File::from_str(&config_content, config::FileFormat::Toml).required(false))
            // Local configuration override
            .add_source(File::with_name("config/local").required(false))
            // Environment variables with prefix BEE (e.g., BEE_MODELS__DEFAULT_PROVIDER=zhipu)
            .add_source(
                Environment::with_prefix("BEE")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?;

        let mut cfg: Self = config.try_deserialize()?;

        // 数据库路径归一化：如果是相对路径，则转换为基于配置文件目录的绝对路径
        if cfg.database.url.starts_with("sqlite:") && !cfg.database.url.starts_with("sqlite://") {
            let path_part = cfg
                .database
                .url
                .strip_prefix("sqlite:")
                .unwrap_or(&cfg.database.url);
            let path = std::path::Path::new(path_part);
            let abs_path = if path.is_relative() {
                let gateway_dir = config_dir.parent().unwrap_or(&config_dir);
                std::env::current_dir()
                    .map_err(|e| {
                        ConfigError::Message(format!("Failed to get current directory: {}", e))
                    })?
                    .join(gateway_dir)
                    .join(path)
            } else {
                path.to_path_buf()
            };
            // 规范化路径：解析 .. 和 . 组件，避免 sqlx 无法处理未规范化的路径
            let normalized = Self::normalize_path(&abs_path);
            // sqlx 要求绝对路径使用 sqlite:/// 格式（三个斜杠）
            let path_str = normalized.display().to_string().replace('\\', "/");
            let path_str = path_str.trim_start_matches('/');
            cfg.database.url = format!("sqlite:///{}", path_str);
        }

        Ok(cfg)
    }

    /// 规范化路径：解析 . 和 .. 组件，去除冗余
    fn normalize_path(path: &std::path::Path) -> std::path::PathBuf {
        let mut result = std::path::PathBuf::new();
        for component in path.components() {
            match component {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    result.pop();
                }
                std::path::Component::RootDir => {
                    result.push(std::path::Component::RootDir);
                }
                std::path::Component::Prefix(p) => {
                    result.push(p.as_os_str());
                }
                std::path::Component::Normal(c) => {
                    result.push(c);
                }
            }
        }
        result
    }

    /// Migrate non-prefixed environment variables to BEE__ prefixed ones
    /// This allows using .env files without the BEE__ prefix
    fn migrate_env_vars() {
        // List of environment variable prefixes that should be migrated
        let prefixes = [
            "SERVER__",
            "DATABASE__",
            "JWT__",
            "TLS__",
            "MODELS__",
            "CHANNELS__",
            "LOGGING__",
            "METRICS__",
            "TRACING__",
            "RATE_LIMIT__",
            "SECURITY__",
            "SERVICES__",
            "APP__",
        ];

        let mut migrated_count = 0;
        for (key, value) in std::env::vars() {
            // If key starts with any of the prefixes and doesn't already have BEE__ prefix
            for prefix in &prefixes {
                if key.starts_with(prefix) && !key.starts_with("BEE__") {
                    let new_key = format!("BEE__{}", key);
                    eprintln!("[Config] Migrating env var: {} -> {}", key, new_key);
                    std::env::set_var(&new_key, value);
                    migrated_count += 1;
                    break;
                }
            }
        }
        eprintln!("[Config] Migrated {} environment variables", migrated_count);
    }

    /// Get server socket address
    pub fn server_addr(&self) -> Result<SocketAddr, String> {
        format!("{}:{}", self.server.host, self.server.port)
            .parse()
            .map_err(|e| format!("Invalid server address: {}", e))
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        // Validate JWT secret
        if self.jwt.secret.expose_secret().len() < 32 {
            return Err(ConfigValidationError::JwtSecretTooShort);
        }

        // Validate database URL
        if self.database.url.is_empty() || self.database.url == "${DATABASE_URL}" {
            return Err(ConfigValidationError::DatabaseUrlMissing);
        }

        // Validate default provider exists
        if !self
            .models
            .providers
            .contains_key(&self.models.default_provider)
        {
            return Err(ConfigValidationError::InvalidModelProvider(
                self.models.default_provider.clone(),
            ));
        }

        Ok(())
    }

    /// Get enabled channels as list
    pub fn get_enabled_channels(&self) -> Vec<(&str, &ChannelConfig)> {
        let all_channels = [
            ("lark", self.channels.lark.as_ref()),
            ("dingtalk", self.channels.dingtalk.as_ref()),
            ("telegram", self.channels.telegram.as_ref()),
            ("discord", self.channels.discord.as_ref()),
            ("slack", self.channels.slack.as_ref()),
            ("wechat", self.channels.wechat.as_ref()),
            ("personal_wechat", self.channels.personal_wechat.as_ref()),
            ("webchat", self.channels.webchat.as_ref()),
            ("teams", self.channels.teams.as_ref()),
            ("twitter", self.channels.twitter.as_ref()),
            ("whatsapp", self.channels.whatsapp.as_ref()),
            ("signal", self.channels.signal.as_ref()),
            ("matrix", self.channels.matrix.as_ref()),
            ("imessage", self.channels.imessage.as_ref()),
        ];

        all_channels
            .into_iter()
            .filter_map(|(name, config)| config.filter(|c| c.enabled).map(|c| (name, c)))
            .collect()
    }

    /// Get model config for a provider
    pub fn get_model_config(&self, provider: &str) -> Option<&ModelProviderConfig> {
        self.models.providers.get(provider)
    }

    /// Convert to gateway-lib GatewayConfig
    pub fn to_gateway_config(&self) -> anyhow::Result<gateway::config::GatewayConfig> {
        use gateway::config::GatewayConfig;

        GatewayConfig::from_app_config(self)
            .map_err(|e| anyhow::anyhow!("Config conversion failed: {}", e))
    }
}

/// Configuration validation errors
#[derive(Debug, thiserror::Error)]
pub enum ConfigValidationError {
    #[error("JWT secret must be at least 32 characters long")]
    JwtSecretTooShort,

    #[error(
        "Database URL is required. Set DATABASE_URL environment variable or configure in config \
         file"
    )]
    DatabaseUrlMissing,

    #[error("Invalid model provider: {0}")]
    InvalidModelProvider(String),
}

// Backward compatibility aliases
pub type AppConfig = BeeBotOSConfig;

// ============================================================================
// Trait implementations for gateway-lib integration
// These are defined at module level to avoid non-local impl warnings
// ============================================================================

use gateway::config::app_config::*;

impl AppServerConfig for ServerConfig {
    fn host(&self) -> &str {
        &self.host
    }
    fn port(&self) -> u16 {
        self.port
    }
    fn timeout_seconds(&self) -> u64 {
        self.timeout_seconds
    }
    fn max_body_size(&self) -> usize {
        self.max_body_size_mb * 1024 * 1024
    }
}

impl AppJwtConfig for JwtConfig {
    fn secret(&self) -> String {
        self.secret.expose_secret().clone()
    }
    fn expiry_minutes(&self) -> u64 {
        (self.expiry_hours * 60) as u64
    }
    fn refresh_expiry_minutes(&self) -> u64 {
        (self.refresh_expiry_hours * 60) as u64
    }
    fn issuer(&self) -> &str {
        &self.issuer
    }
    fn audience(&self) -> &str {
        &self.audience
    }
}

impl AppCorsConfig for CorsConfig {
    fn allowed_origins(&self) -> &[String] {
        &self.allowed_origins
    }
    fn allowed_methods(&self) -> &[String] {
        &self.allowed_methods
    }
    fn allowed_headers(&self) -> &[String] {
        &self.allowed_headers
    }
    fn allow_credentials(&self) -> bool {
        self.allow_credentials
    }
    fn allow_any_origin(&self) -> bool {
        self.allowed_origins.contains(&"*".to_string())
    }
}

impl AppRateLimitConfig for RateLimitConfig {
    fn enabled(&self) -> bool {
        self.enabled
    }
    fn requests_per_second(&self) -> u32 {
        self.requests_per_second
    }
    fn burst_size(&self) -> u32 {
        self.burst_size
    }
    fn cooldown_seconds(&self) -> u32 {
        self.cooldown_seconds as u32
    }
}

impl gateway::config::app_config::AppConfig for BeeBotOSConfig {
    type Server = ServerConfig;
    type Jwt = JwtConfig;
    type Cors = CorsConfig;
    type RateLimit = RateLimitConfig;

    fn server(&self) -> &Self::Server {
        &self.server
    }
    fn jwt(&self) -> &Self::Jwt {
        &self.jwt
    }
    fn cors(&self) -> &Self::Cors {
        &self.server.cors
    }
    fn rate_limit(&self) -> &Self::RateLimit {
        &self.rate_limit
    }
    fn log_level(&self) -> &str {
        &self.logging.level
    }
    fn metrics_enabled(&self) -> bool {
        self.metrics.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_structure() {
        let config = BeeBotOSConfig {
            system_name: "BeeBotOS".to_string(),
            version: "2.0.0".to_string(),
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 8080,
                grpc_port: 50051,
                timeout_seconds: 30,
                max_body_size_mb: 10,
                cors: CorsConfig {
                    allowed_origins: vec!["*".to_string()],
                    allowed_methods: vec!["GET".to_string(), "POST".to_string()],
                    allowed_headers: vec!["Content-Type".to_string()],
                    allow_credentials: true,
                },
            },
            database: DatabaseConfig {
                url: "sqlite:data/beebotos.db".to_string(),
                max_connections: 20,
                min_connections: 5,
                connect_timeout_seconds: 10,
                idle_timeout_seconds: 600,
                run_migrations: true,
            },
            jwt: JwtConfig {
                secret: SecretString::new("a-very-long-secret-key-at-least-32-chars".to_string()),
                expiry_hours: 24,
                refresh_expiry_hours: 168,
                issuer: "beebotos".to_string(),
                audience: "api".to_string(),
            },
            models: ModelsConfig {
                default_provider: "kimi".to_string(),
                fallback_chain: vec!["openai".to_string()],
                request_timeout: default_request_timeout(),
                cost_optimization: false,
                max_tokens: 4096,
                system_prompt: "You are a helpful assistant.".to_string(),
                providers: {
                    let mut map = HashMap::new();
                    map.insert(
                        "kimi".to_string(),
                        ModelProviderConfig {
                            api_key: Some("test-key".to_string()),
                            base_url: Some("https://api.moonshot.cn".to_string()),
                            model: Some("moonshot-v1-8k".to_string()),
                            temperature: 0.7,
                            deployment: None,
                            context_window: Some(8192),
                        },
                    );
                    map
                },
            },
            channels: ChannelsConfig {
                auto_download_media: true,
                media_storage_path: "./data/media".to_string(),
                max_file_size_mb: 50,
                context_window_size: 20,
                auto_reply: true,
                enable_typing_indicator: true,
                enabled_platforms: vec!["lark".to_string()],
                default_agent_id: None,
                lark: None,
                dingtalk: None,
                telegram: None,
                discord: None,
                slack: None,
                wechat: None,
                personal_wechat: None,
                webchat: None,
                teams: None,
                twitter: None,
                whatsapp: None,
                signal: None,
                matrix: None,
                imessage: None,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: "json".to_string(),
                file: "./data/logs/beebotos.log".to_string(),
                rotation: LogRotationConfig {
                    enabled: true,
                    max_size_mb: 100,
                    max_files: 10,
                },
            },
            metrics: MetricsConfig {
                enabled: true,
                endpoint: "0.0.0.0:9090".to_string(),
                interval_seconds: 60,
            },
            tracing: TracingConfig {
                enabled: false,
                otel_endpoint: None,
                sample_rate: 0.1,
            },
            rate_limit: RateLimitConfig {
                enabled: true,
                requests_per_second: 10,
                burst_size: 50,
                cooldown_seconds: 60,
            },
            security: SecurityConfig {
                allowed_webhook_ips: vec!["0.0.0.0/0".to_string()],
                verify_webhook_signatures: true,
                encryption_enabled: true,
            },
            tls: None,
            services: None,
            blockchain: BlockchainConfig {
                enabled: false,
                chain_id: 10143,
                rpc_url: None,
                agent_wallet_mnemonic: None,
                identity_contract_address: None,
                registry_contract_address: None,
                dao_contract_address: None,
                skill_nft_contract_address: None,
            },
            wizard: WizardConfig::default(),
        };

        assert!(config.validate().is_ok());
    }
}
