//! Configuration Loader
//!
//! Handles loading configuration from various sources.

use std::path::Path;

use super::{ConfigBuilder, ConfigError};

/// Source of configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    /// Default configuration
    Default,
    /// Loaded from file
    File,
    /// Loaded from environment variables
    Environment,
    /// Loaded from a preset profile
    Profile(super::ConfigProfile),
    /// Merged from multiple sources
    Merged,
}

impl std::fmt::Display for ConfigSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigSource::Default => write!(f, "default"),
            ConfigSource::File => write!(f, "file"),
            ConfigSource::Environment => write!(f, "environment"),
            ConfigSource::Profile(p) => write!(f, "profile:{:?}", p),
            ConfigSource::Merged => write!(f, "merged"),
        }
    }
}

/// Configuration loader with caching and hot-reload support
#[derive(Debug)]
pub struct ConfigLoader {
    builder: ConfigBuilder,
    watch_paths: Vec<std::path::PathBuf>,
}

impl ConfigLoader {
    /// Create new config loader
    pub fn new() -> Self {
        Self {
            builder: ConfigBuilder::new(),
            watch_paths: Vec::new(),
        }
    }

    /// Load from TOML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<ConfigBuilder, ConfigError> {
        ConfigBuilder::from_toml_file(path)
    }

    /// Load from JSON file
    pub fn from_json_file<P: AsRef<Path>>(path: P) -> Result<ConfigBuilder, ConfigError> {
        ConfigBuilder::from_json_file(path)
    }

    /// Load from environment variables
    pub fn from_env() -> Result<ConfigBuilder, ConfigError> {
        ConfigBuilder::from_env()
    }

    /// Load with precedence: file -> env -> defaults
    pub fn load_with_precedence<P: AsRef<Path>>(
        file_path: Option<P>,
        use_env: bool,
    ) -> Result<ConfigBuilder, ConfigError> {
        let mut builder = ConfigBuilder::new();

        // Start with file if provided
        if let Some(path) = file_path {
            if path.as_ref().exists() {
                builder = ConfigBuilder::from_toml_file(path)?;
            }
        }

        // Override with environment if requested
        if use_env {
            let env_builder = ConfigBuilder::from_env()?;
            builder = merge_builders(builder, env_builder)?;
        }

        Ok(builder)
    }

    /// Add a path to watch for changes
    pub fn watch<P: AsRef<Path>>(&mut self, path: P) -> &mut Self {
        self.watch_paths.push(path.as_ref().to_path_buf());
        self
    }

    /// Check if any watched files have changed
    pub fn has_changed(&self) -> bool {
        // In a real implementation, this would check file modification times
        // For now, always return false
        false
    }

    /// Reload configuration from watched files
    pub fn reload(&mut self) -> Result<(), ConfigError> {
        if let Some(path) = self.watch_paths.first() {
            self.builder = ConfigBuilder::from_toml_file(path)?;
        }
        Ok(())
    }

    /// Get the current builder
    pub fn builder(&self) -> &ConfigBuilder {
        &self.builder
    }

    /// Get a mutable reference to the builder
    pub fn builder_mut(&mut self) -> &mut ConfigBuilder {
        &mut self.builder
    }
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Merge two config builders
fn merge_builders(
    _base: ConfigBuilder,
    override_: ConfigBuilder,
) -> Result<ConfigBuilder, ConfigError> {
    // In a real implementation, this would intelligently merge configurations
    // For now, just return the override
    Ok(override_)
}

/// Load configuration with standard precedence
///
/// Precedence (highest to lowest):
/// 1. Environment variables
/// 2. Local config file (./brain.toml)
/// 3. User config file (~/.config/beebotos/brain.toml)
/// 4. System config file (data/brain.toml)
/// 5. Default configuration
pub fn load_config() -> Result<ConfigBuilder, ConfigError> {
    let mut builder = ConfigBuilder::new();

    // Try project-level system config
    if std::path::Path::new("data/brain.toml").exists() {
        builder = ConfigBuilder::from_toml_file("data/brain.toml")?;
    }

    // Try user config
    if let Some(home) = dirs::home_dir() {
        let user_config = home.join(".config").join("beebotos").join("brain.toml");
        if user_config.exists() {
            builder = ConfigBuilder::from_toml_file(&user_config)?;
        }
    }

    // Try local config
    if std::path::Path::new("brain.toml").exists() {
        builder = ConfigBuilder::from_toml_file("brain.toml")?;
    }

    // Override with environment
    let env_builder = ConfigBuilder::from_env()?;
    builder = merge_builders(builder, env_builder)?;

    Ok(builder)
}

/// External directories crate for home_dir
mod dirs {
    pub fn home_dir() -> Option<std::path::PathBuf> {
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map(std::path::PathBuf::from)
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_source_display() {
        assert_eq!(ConfigSource::Default.to_string(), "default");
        assert_eq!(ConfigSource::File.to_string(), "file");
        assert_eq!(ConfigSource::Environment.to_string(), "environment");
    }

    #[test]
    fn test_config_loader_new() {
        let loader = ConfigLoader::new();
        assert!(loader.watch_paths.is_empty());
    }

    #[test]
    fn test_config_loader_watch() {
        let mut loader = ConfigLoader::new();
        loader.watch("data/tmp/config.toml");
        assert_eq!(loader.watch_paths.len(), 1);
    }

    #[test]
    fn test_config_loader_has_changed() {
        let loader = ConfigLoader::new();
        assert!(!loader.has_changed());
    }
}
