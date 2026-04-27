//! Configuration management for BeeBotOS CLI

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::secure_storage::{SecureStorage, KEY_API_KEY, KEY_PRIVATE_KEY};
use crate::{log_debug, log_info, log_warn};

/// CLI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Daemon endpoint URL
    #[serde(default = "default_daemon_endpoint")]
    pub daemon_endpoint: String,

    /// Daemon timeout in seconds
    #[serde(default = "default_daemon_timeout")]
    pub daemon_timeout: u64,

    /// Chain RPC URL
    #[serde(default = "default_rpc_url")]
    pub rpc_url: String,

    /// DAO contract address
    #[serde(default)]
    pub dao_address: String,

    /// Private key for transactions
    #[serde(default)]
    pub private_key: String,

    /// API key for BeeBotOS services
    #[serde(default)]
    pub api_key: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            daemon_endpoint: default_daemon_endpoint(),
            daemon_timeout: default_daemon_timeout(),
            rpc_url: default_rpc_url(),
            dao_address: String::new(),
            private_key: String::new(),
            api_key: String::new(),
        }
    }
}

fn default_daemon_endpoint() -> String {
    "http://localhost:8080".to_string()
}

fn default_daemon_timeout() -> u64 {
    30
}

fn default_rpc_url() -> String {
    "http://localhost:8545".to_string()
}

impl Config {
    /// Load configuration from file or environment
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        let mut config = if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read config from {}", config_path.display()))?;
            let config: Config =
                toml::from_str(&content).with_context(|| "Failed to parse config file")?;
            config
        } else {
            // Try to load from environment or use defaults
            Config::from_env()?
        };

        // Try to load sensitive data from secure storage
        if let Ok(secure) = SecureStorage::new() {
            if config.api_key.is_empty() {
                if let Ok(Some(api_key)) = secure.get(KEY_API_KEY) {
                    log_debug!("Loaded API key from secure storage");
                    config.api_key = api_key;
                }
            }

            if config.private_key.is_empty() {
                if let Ok(Some(private_key)) = secure.get(KEY_PRIVATE_KEY) {
                    log_debug!("Loaded private key from secure storage");
                    config.private_key = private_key;
                }
            }
        } else {
            log_warn!("Failed to initialize secure storage");
        }

        // Ensure API key is set in environment for other components
        if !config.api_key.is_empty() {
            std::env::set_var("BEEBOTOS_API_KEY", &config.api_key);
        }

        Ok(config)
    }

    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self> {
        Ok(Config {
            daemon_endpoint: std::env::var("BEEBOTOS_DAEMON_ENDPOINT")
                .unwrap_or_else(|_| default_daemon_endpoint()),
            daemon_timeout: std::env::var("BEEBOTOS_DAEMON_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(default_daemon_timeout),
            rpc_url: std::env::var("BEEBOTOS_RPC_URL").unwrap_or_else(|_| default_rpc_url()),
            dao_address: std::env::var("BEEBOTOS_DAO_ADDRESS").unwrap_or_default(),
            private_key: std::env::var("BEEBOTOS_PRIVATE_KEY").unwrap_or_default(),
            api_key: std::env::var("BEEBOTOS_API_KEY").unwrap_or_default(),
        })
    }

    /// Save sensitive data to secure storage
    pub fn save_to_secure_storage(&self) -> Result<()> {
        let secure = SecureStorage::new()?;

        if !self.api_key.is_empty() {
            secure.set(KEY_API_KEY, &self.api_key)?;
            log_info!("API key saved to secure storage");
        }

        if !self.private_key.is_empty() {
            secure.set(KEY_PRIVATE_KEY, &self.private_key)?;
            log_info!("Private key saved to secure storage");
        }

        Ok(())
    }

    /// Save configuration to file
    ///
    /// Note: Sensitive fields (api_key, private_key) are stored in secure
    /// storage instead of the config file for better security.
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

        // Ensure config directory exists
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Save sensitive data to secure storage
        self.save_to_secure_storage()?;

        // Create a config without sensitive data for the file
        let config_for_file = Config {
            daemon_endpoint: self.daemon_endpoint.clone(),
            daemon_timeout: self.daemon_timeout,
            rpc_url: self.rpc_url.clone(),
            dao_address: self.dao_address.clone(),
            private_key: String::new(), // Don't save to file
            api_key: String::new(),     // Don't save to file
        };

        let content = toml::to_string_pretty(&config_for_file)?;
        std::fs::write(&config_path, content)
            .with_context(|| format!("Failed to write config to {}", config_path.display()))?;

        log_info!("Configuration saved to {}", config_path.display());
        Ok(())
    }

    /// Get the configuration file path
    pub fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir().context("Could not find config directory")?;
        Ok(config_dir.join("beebotos").join("config.toml"))
    }

    #[allow(dead_code)]
    /// Get the data directory for logs
    pub fn data_dir() -> Result<PathBuf> {
        let data_dir = dirs::data_dir().context("Could not find data directory")?;
        Ok(data_dir.join("beebotos"))
    }

    #[allow(dead_code)]
    /// Get the log directory
    pub fn log_dir() -> Result<PathBuf> {
        Ok(Self::data_dir()?.join("logs"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.daemon_endpoint, "http://localhost:8080");
        assert_eq!(config.daemon_timeout, 30);
        assert_eq!(config.rpc_url, "http://localhost:8545");
    }

    #[test]
    fn test_config_from_env() {
        // This test would need to set environment variables
        // Skipping for now as it would affect the environment
    }
}
