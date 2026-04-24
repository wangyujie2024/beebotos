//! Reloadable Configuration Module
//!
//! Provides hot-reloading of configuration from files and environment
//! variables.

use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};
use tracing::info;
use validator::Validate;

use super::ChainConfig;
use crate::{ChainError, Result};

/// Configuration reload manager
pub struct ConfigReloader {
    config: Arc<RwLock<ChainConfig>>,
    file_path: Option<String>,
    #[allow(dead_code)]
    reload_tx: mpsc::Sender<()>,
}

impl ConfigReloader {
    /// Create new config reloader from a file
    pub async fn from_file(path: impl Into<String>) -> Result<(Self, ChainConfig)> {
        let path = path.into();
        let config = Self::load_config_from_file(&path).await?;

        let (reload_tx, _reload_rx) = mpsc::channel(10);
        let reloader = Self {
            config: Arc::new(RwLock::new(config.clone())),
            file_path: Some(path.clone()),
            reload_tx,
        };

        info!(path = %path, "Config reloader created from file");
        Ok((reloader, config))
    }

    /// Create new config reloader from initial config
    pub fn new(config: ChainConfig) -> Self {
        let (reload_tx, _reload_rx) = mpsc::channel(10);

        Self {
            config: Arc::new(RwLock::new(config)),
            file_path: None,
            reload_tx,
        }
    }

    /// Load configuration from file
    async fn load_config_from_file(path: &str) -> Result<ChainConfig> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| ChainError::InvalidConfig(format!("Failed to read config file: {}", e)))?;

        // Try JSON first, then YAML, then TOML
        if let Ok(config) = serde_json::from_str::<ChainConfig>(&content) {
            return Ok(config);
        }

        if let Ok(config) = serde_yaml::from_str::<ChainConfig>(&content) {
            return Ok(config);
        }

        if let Ok(config) = toml::from_str::<ChainConfig>(&content) {
            return Ok(config);
        }

        Err(ChainError::InvalidConfig(
            "Failed to parse config file (expected JSON, YAML, or TOML)".to_string(),
        ))
    }

    /// Get current configuration
    pub async fn get_config(&self) -> ChainConfig {
        self.config.read().await.clone()
    }

    /// Update configuration manually
    pub async fn update_config(&self, new_config: ChainConfig) -> Result<()> {
        // Validate new config
        new_config
            .validate()
            .map_err(|e| ChainError::InvalidConfig(format!("Config validation failed: {}", e)))?;

        let mut guard = self.config.write().await;
        *guard = new_config;

        info!("Configuration updated manually");
        Ok(())
    }

    /// Reload configuration from file
    pub async fn reload(&self) -> Result<()> {
        if let Some(ref path) = self.file_path {
            let new_config = Self::load_config_from_file(path).await?;
            let mut guard = self.config.write().await;
            *guard = new_config;

            info!("Configuration reloaded from file");
            Ok(())
        } else {
            Err(ChainError::InvalidConfig(
                "No file path configured for reload".to_string(),
            ))
        }
    }

    /// Reload from environment variables
    pub async fn reload_from_env(&self) -> Result<()> {
        let new_config = ChainConfig::from_env()?;

        let mut guard = self.config.write().await;
        *guard = new_config;

        info!("Configuration reloaded from environment");
        Ok(())
    }

    /// Get file path
    pub fn file_path(&self) -> Option<&str> {
        self.file_path.as_deref()
    }
}

/// Configuration snapshot for atomic updates
#[derive(Debug, Clone)]
pub struct ConfigSnapshot {
    pub config: ChainConfig,
    pub version: u64,
    pub timestamp: u64,
}

/// Versioned configuration manager
pub struct VersionedConfig {
    current: Arc<RwLock<ConfigSnapshot>>,
    history: Arc<RwLock<Vec<ConfigSnapshot>>>,
    max_history: usize,
}

impl VersionedConfig {
    /// Create new versioned config
    pub fn new(config: ChainConfig) -> Self {
        let snapshot = ConfigSnapshot {
            config,
            version: 1,
            timestamp: current_timestamp(),
        };

        Self {
            current: Arc::new(RwLock::new(snapshot)),
            history: Arc::new(RwLock::new(Vec::new())),
            max_history: 10,
        }
    }

    /// Get current config
    pub async fn get(&self) -> ConfigSnapshot {
        self.current.read().await.clone()
    }

    /// Update with new config
    pub async fn update(&self, new_config: ChainConfig) -> Result<()> {
        let mut current = self.current.write().await;

        // Archive current to history
        let mut history = self.history.write().await;
        history.push(current.clone());

        // Trim history
        if history.len() > self.max_history {
            history.remove(0);
        }

        // Update current
        *current = ConfigSnapshot {
            config: new_config,
            version: current.version + 1,
            timestamp: current_timestamp(),
        };

        Ok(())
    }

    /// Rollback to previous version
    pub async fn rollback(&self) -> Result<ChainConfig> {
        let mut history = self.history.write().await;
        let mut current = self.current.write().await;

        if let Some(previous) = history.pop() {
            *current = previous;
            Ok(current.config.clone())
        } else {
            Err(ChainError::InvalidConfig(
                "No previous version to rollback to".to_string(),
            ))
        }
    }

    /// Get configuration history
    pub async fn history(&self) -> Vec<ConfigSnapshot> {
        self.history.read().await.clone()
    }
}

/// Get current Unix timestamp
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_versioned_config() {
        let config = ChainConfig::new("https://rpc.example.com", 1337).unwrap();
        let versioned = VersionedConfig::new(config.clone());

        let snapshot = versioned.get().await;
        assert_eq!(snapshot.version, 1);

        // Update
        let new_config = ChainConfig::new("https://rpc2.example.com", 1338).unwrap();
        versioned.update(new_config).await.unwrap();

        let snapshot = versioned.get().await;
        assert_eq!(snapshot.version, 2);

        // Rollback
        versioned.rollback().await.unwrap();

        let snapshot = versioned.get().await;
        assert_eq!(snapshot.version, 1);
    }

    #[tokio::test]
    async fn test_config_reloader_new() {
        let config = ChainConfig::new("https://rpc.example.com", 1337).unwrap();
        let reloader = ConfigReloader::new(config.clone());

        let current = reloader.get_config().await;
        assert_eq!(current.rpc_url, config.rpc_url);
    }
}
