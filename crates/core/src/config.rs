//! Configuration management for BeeBotOS

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Main configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Node identifier
    pub node_id: String,
    /// Environment
    pub environment: Environment,
    /// Log level
    pub log_level: String,
    /// Kernel configuration
    pub kernel: KernelConfig,
    /// Social brain configuration
    pub social_brain: SocialBrainConfig,
    /// Agent runtime configuration
    pub agents: AgentsConfig,
    /// Blockchain configuration
    pub chain: ChainConfig,
    /// DAO configuration
    pub dao: DaoConfig,
    /// Gateway configuration
    pub gateway: GatewayConfig,
}

impl Config {
    /// Load configuration from file
    pub fn from_file<P: AsRef<Path>>(path: P) -> crate::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)
            .map_err(|e| crate::BeeBotOSError::configuration(e.to_string()))?;
        Ok(config)
    }

    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        Self::default()
    }

    /// Validate configuration
    pub fn validate(&self) -> crate::Result<()> {
        // Validate kernel config
        if self.kernel.max_agents == 0 {
            return Err(crate::BeeBotOSError::configuration(
                "kernel.max_agents must be > 0".to_string(),
            ));
        }

        // Validate agent config
        if self.agents.max_subagent_depth > 10 {
            return Err(crate::BeeBotOSError::configuration(
                "agents.max_subagent_depth must be <= 10".to_string(),
            ));
        }

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            node_id: format!("node-{}", uuid::Uuid::new_v4().simple()),
            environment: Environment::Development,
            log_level: "info".to_string(),
            kernel: KernelConfig::default(),
            social_brain: SocialBrainConfig::default(),
            agents: AgentsConfig::default(),
            chain: ChainConfig::default(),
            dao: DaoConfig::default(),
            gateway: GatewayConfig::default(),
        }
    }
}

/// Environment types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Environment {
    /// Development
    Development,
    /// Testing
    Testing,
    /// Staging
    Staging,
    /// Production
    Production,
}

/// Kernel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelConfig {
    /// Maximum number of agents
    pub max_agents: usize,
    /// Memory limit per agent (MB)
    pub memory_limit_mb: usize,
    /// CPU quota per agent (ms/s)
    pub cpu_quota_ms: u64,
    /// Enable TEE
    pub enable_tee: bool,
    /// WASM configuration
    pub wasm: WasmConfig,
}

impl Default for KernelConfig {
    fn default() -> Self {
        Self {
            max_agents: 1000,
            memory_limit_mb: 512,
            cpu_quota_ms: 100,
            enable_tee: false,
            wasm: WasmConfig::default(),
        }
    }
}

/// WASM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmConfig {
    /// Maximum memory (MB)
    pub max_memory_mb: usize,
    /// Fuel limit
    pub fuel_limit: u64,
    /// Enable AOT compilation
    pub enable_aot: bool,
}

impl Default for WasmConfig {
    fn default() -> Self {
        Self {
            max_memory_mb: 128,
            fuel_limit: 10_000_000,
            enable_aot: true,
        }
    }
}

/// Social brain configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialBrainConfig {
    /// Enable NEAT evolution
    pub enable_evolution: bool,
    /// Population size
    pub population_size: usize,
    /// Enable emotional processing
    pub enable_emotion: bool,
    /// Memory configuration
    pub memory: MemoryConfig,
}

impl Default for SocialBrainConfig {
    fn default() -> Self {
        Self {
            enable_evolution: true,
            population_size: 50,
            enable_emotion: true,
            memory: MemoryConfig::default(),
        }
    }
}

/// Memory configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Short-term memory capacity
    pub short_term_capacity: usize,
    /// Embedding dimension
    pub embedding_dim: usize,
    /// Consolidation interval (seconds)
    pub consolidation_interval_secs: u64,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            short_term_capacity: 9,
            embedding_dim: 384,
            consolidation_interval_secs: 3600,
        }
    }
}

/// Agents configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentsConfig {
    /// Maximum subagent nesting depth
    pub max_subagent_depth: u8,
    /// Default heartbeat interval (seconds)
    pub heartbeat_interval_secs: u64,
    /// Enable skills
    pub enable_skills: bool,
    /// Model configuration
    pub models: ModelConfig,
}

impl Default for AgentsConfig {
    fn default() -> Self {
        Self {
            max_subagent_depth: 5,
            heartbeat_interval_secs: 1800,
            enable_skills: true,
            models: ModelConfig::default(),
        }
    }
}

/// Model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Default provider
    pub default_provider: String,
    /// Provider endpoints
    pub endpoints: HashMap<String, String>,
    /// Cost optimization enabled
    pub cost_optimization: bool,
}

impl Default for ModelConfig {
    fn default() -> Self {
        let mut endpoints = HashMap::new();
        endpoints.insert("openai".to_string(), "https://api.openai.com".to_string());
        endpoints.insert(
            "anthropic".to_string(),
            "https://api.anthropic.com".to_string(),
        );

        Self {
            default_provider: "openai".to_string(),
            endpoints,
            cost_optimization: true,
        }
    }
}

/// Chain configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    /// Monad RPC URL
    pub monad_rpc_url: String,
    /// Chain ID
    pub chain_id: u64,
    /// Contract addresses
    pub contracts: ContractAddresses,
}

impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            monad_rpc_url: "https://rpc.monad.xyz".to_string(),
            chain_id: 1,
            contracts: ContractAddresses::default(),
        }
    }
}

/// Contract addresses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractAddresses {
    /// Agent registry
    pub agent_registry: String,
    /// Agent identity
    pub agent_identity: String,
    /// A2A commerce
    pub a2a_commerce: String,
    /// DAO
    pub dao: String,
}

impl Default for ContractAddresses {
    fn default() -> Self {
        Self {
            agent_registry: "0x0000000000000000000000000000000000000000".to_string(),
            agent_identity: "0x0000000000000000000000000000000000000000".to_string(),
            a2a_commerce: "0x0000000000000000000000000000000000000000".to_string(),
            dao: "0x0000000000000000000000000000000000000000".to_string(),
        }
    }
}

/// DAO configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaoConfig {
    /// DAO address
    pub dao_address: String,
    /// Governance token
    pub governance_token: String,
    /// Voting period (seconds)
    pub voting_period_secs: u64,
    /// Quorum percentage
    pub quorum_bps: u16,
}

impl Default for DaoConfig {
    fn default() -> Self {
        Self {
            dao_address: "0x0000000000000000000000000000000000000000".to_string(),
            governance_token: "0x0000000000000000000000000000000000000000".to_string(),
            voting_period_secs: 604800, // 7 days
            quorum_bps: 400,            // 4%
        }
    }
}

/// Gateway configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// HTTP bind address
    pub http_bind: String,
    /// HTTP port
    pub http_port: u16,
    /// WebSocket port
    pub ws_port: u16,
    /// gRPC port
    pub grpc_port: u16,
    /// Rate limit (requests per second)
    pub rate_limit_rps: u32,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            http_bind: "0.0.0.0".to_string(),
            http_port: 8080,
            ws_port: 8081,
            grpc_port: 9090,
            rate_limit_rps: 1000,
        }
    }
}

/// Configuration Center
///
/// Provides unified configuration management with:
/// - Environment-based configuration loading
/// - Configuration validation
/// - Hot-reload support
/// - Change notifications
///
/// # Usage
///
/// ```rust
/// use beebotos_core::config::{Config, ConfigCenter, Environment};
///
/// // Load with environment detection
/// let config = ConfigCenter::load().unwrap();
///
/// // Or load specific environment
/// let config = ConfigCenter::load_env(Environment::Production).unwrap();
/// ```
#[derive(Debug)]
pub struct ConfigCenter {
    /// Current configuration
    config: Config,
    /// Configuration source path
    source_path: Option<std::path::PathBuf>,
    /// Last loaded timestamp
    last_loaded: chrono::DateTime<chrono::Utc>,
}

impl ConfigCenter {
    /// Load configuration from default locations
    ///
    /// Tries (in order):
    /// 1. Environment-specific config file (config/{env}.toml)
    /// 2. Local config file (config/local.toml)
    /// 3. Environment variables
    /// 4. Default configuration
    pub fn load() -> crate::Result<Self> {
        let env = Self::detect_environment();
        Self::load_env(env)
    }

    /// Load configuration for specific environment
    pub fn load_env(env: Environment) -> crate::Result<Self> {
        // Try environment-specific config first
        let env_path = format!("config/{}.toml", env.as_str());
        if std::path::Path::new(&env_path).exists() {
            return Self::load_from_file(&env_path);
        }

        // Try local config
        let local_path = "config/local.toml";
        if std::path::Path::new(local_path).exists() {
            return Self::load_from_file(local_path);
        }

        // Fall back to environment variables
        Ok(Self::from_env())
    }

    /// Load configuration from file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> crate::Result<Self> {
        let config = Config::from_file(&path)?;

        // Validate after loading
        config.validate()?;

        Ok(Self {
            config,
            source_path: Some(path.as_ref().to_path_buf()),
            last_loaded: chrono::Utc::now(),
        })
    }

    /// Create configuration from environment variables
    pub fn from_env() -> Self {
        Self {
            config: Config::from_env(),
            source_path: None,
            last_loaded: chrono::Utc::now(),
        }
    }

    /// Get current configuration
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Get mutable configuration reference (for hot-reload)
    pub fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    /// Check if configuration can be reloaded
    pub fn can_reload(&self) -> bool {
        self.source_path.is_some()
    }

    /// Reload configuration from source
    ///
    /// Returns true if configuration changed, false otherwise
    pub fn reload(&mut self) -> crate::Result<bool> {
        let Some(ref path) = self.source_path else {
            return Err(crate::BeeBotOSError::configuration(
                "No source path configured for reload".to_string(),
            ));
        };

        let new_config = Config::from_file(path)?;
        new_config.validate()?;

        // Check if configuration actually changed
        let changed = self.has_config_changed(&new_config);

        if changed {
            self.config = new_config;
            self.last_loaded = chrono::Utc::now();
        }

        Ok(changed)
    }

    /// Get the last loaded timestamp
    pub fn last_loaded(&self) -> chrono::DateTime<chrono::Utc> {
        self.last_loaded
    }

    /// Get configuration source path
    pub fn source_path(&self) -> Option<&std::path::Path> {
        self.source_path.as_deref()
    }

    /// Detect environment from environment variables
    fn detect_environment() -> Environment {
        if let Ok(env) = std::env::var("BEEBOTOS_ENV") {
            match env.to_lowercase().as_str() {
                "development" | "dev" => Environment::Development,
                "testing" | "test" => Environment::Testing,
                "staging" | "stage" => Environment::Staging,
                "production" | "prod" => Environment::Production,
                _ => Environment::Development,
            }
        } else {
            Environment::Development
        }
    }

    /// Check if configuration has changed
    fn has_config_changed(&self, new_config: &Config) -> bool {
        // Compare serialized forms
        let current = serde_json::to_string(&self.config).unwrap_or_default();
        let new = serde_json::to_string(new_config).unwrap_or_default();
        current != new
    }
}

impl Default for ConfigCenter {
    fn default() -> Self {
        Self::from_env()
    }
}

impl Environment {
    /// Get environment as string
    pub fn as_str(&self) -> &'static str {
        match self {
            Environment::Development => "development",
            Environment::Testing => "testing",
            Environment::Staging => "staging",
            Environment::Production => "production",
        }
    }

    /// Check if this is a production environment
    pub fn is_production(&self) -> bool {
        matches!(self, Environment::Production)
    }

    /// Check if this is a development environment
    pub fn is_development(&self) -> bool {
        matches!(self, Environment::Development)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.environment, Environment::Development);
        assert_eq!(config.kernel.max_agents, 1000);
    }

    #[test]
    fn test_validate_config() {
        let config = Config::default();
        assert!(config.validate().is_ok());

        let mut invalid = config.clone();
        invalid.kernel.max_agents = 0;
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_config_center_from_env() {
        let center = ConfigCenter::from_env();
        assert!(center.source_path().is_none());
        assert_eq!(center.config().environment, Environment::Development);
    }

    #[test]
    fn test_environment_as_str() {
        assert_eq!(Environment::Development.as_str(), "development");
        assert_eq!(Environment::Production.as_str(), "production");
        assert!(Environment::Production.is_production());
        assert!(!Environment::Development.is_production());
    }
}
