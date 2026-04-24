//! ConfigCenter Integration (Simplified)
//!
//! Provides hot-reload for Gateway configuration using the local
//! BeeBotOSConfig TOML file as the source of truth.

use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{info, warn};

/// Simplified configuration manager for Gateway hot-reload
pub struct GatewayConfigManager {
    /// Current configuration (protected by RwLock for safe reload)
    config: RwLock<crate::config::BeeBotOSConfig>,
    /// Path to the configuration file
    source_path: Option<std::path::PathBuf>,
}

impl GatewayConfigManager {
    /// Create from an already-loaded config
    pub fn new(config: crate::config::BeeBotOSConfig) -> Self {
        Self {
            config: RwLock::new(config),
            source_path: Some(std::path::PathBuf::from("config/beebotos.toml")),
        }
    }

    /// Get a read lock on the current config
    pub async fn config(&self) -> tokio::sync::RwLockReadGuard<'_, crate::config::BeeBotOSConfig> {
        self.config.read().await
    }

    /// Check if reload is possible
    pub fn can_reload(&self) -> bool {
        self.source_path
            .as_ref()
            .map(|p| p.exists())
            .unwrap_or(false)
    }

    /// Reload configuration from the TOML source file
    ///
    /// Returns true if the configuration was actually changed.
    pub async fn reload(&self) -> Result<bool, ConfigError> {
        let path = self
            .source_path
            .as_ref()
            .ok_or_else(|| ConfigError::NoSource)?;

        info!("Reloading configuration from {:?}...", path);

        // Re-read the TOML file
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| ConfigError::Io(e.to_string()))?;

        let new_config: crate::config::BeeBotOSConfig =
            toml::from_str(&content).map_err(|e| ConfigError::Parse(e.to_string()))?;

        let mut config = self.config.write().await;

        // Compare serialized forms to detect changes
        let old_json = serde_json::to_string(&*config).unwrap_or_default();
        let new_json = serde_json::to_string(&new_config).unwrap_or_default();
        let changed = old_json != new_json;

        if changed {
            *config = new_config;
            info!("✅ Configuration reloaded (changes detected)");
        } else {
            info!("Configuration unchanged");
        }

        Ok(changed)
    }

    /// Export current configuration as JSON
    pub async fn export(&self) -> Result<serde_json::Value, ConfigError> {
        let config = self.config.read().await;
        serde_json::to_value(&*config).map_err(|e| ConfigError::Serialize(e.to_string()))
    }
}

/// Configuration errors
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("No configuration source path set")]
    NoSource,
    #[error("IO error: {0}")]
    Io(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Serialize error: {0}")]
    Serialize(String),
}
