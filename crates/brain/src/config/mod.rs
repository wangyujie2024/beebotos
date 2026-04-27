//! Configuration Management
//!
//! Provides configuration loading from files, environment variables,
//! and runtime validation.
//!
//! # Example
//!
//! ```
//! use beebotos_brain::config::ConfigLoader;
//!
//! // Load from file
//! let config = ConfigLoader::from_file("config.toml").unwrap();
//!
//! // Load from environment
//! let config = ConfigLoader::from_env().unwrap();
//! ```

pub mod hot_reload;
pub mod loader;
pub mod validator;

use std::path::Path;

pub use hot_reload::{AutoReloadConfig, ConfigWatcher, HotReloadConfig};
pub use loader::{ConfigLoader, ConfigSource};
pub use validator::{ConfigValidator, ValidationError, ValidationResult};

use crate::neat::config::NeatConfig;
use crate::{
    BaselineEmotion, BrainConfig, FeatureToggles, MemoryConfig, PadConfig, ParallelConfig,
    PersonalityConfig,
};

/// Configuration builder for fluent API
#[derive(Debug, Clone)]
pub struct ConfigBuilder {
    brain: BrainConfig,
    source: ConfigSource,
}

impl ConfigBuilder {
    /// Create new config builder with defaults
    pub fn new() -> Self {
        Self {
            brain: BrainConfig::default(),
            source: ConfigSource::Default,
        }
    }

    /// Load from TOML file
    pub fn from_toml_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_toml(&content)
    }

    /// Load from TOML string
    ///
    /// Note: Requires `BrainConfig` to implement `Deserialize`.
    /// Currently not fully implemented - will return error.
    pub fn from_toml(_content: &str) -> Result<Self, ConfigError> {
        // TODO: Implement Deserialize for BrainConfig and all nested types
        Err(ConfigError::NotImplemented(
            "TOML deserialization requires BrainConfig to implement Deserialize".to_string(),
        ))
    }

    /// Load from JSON file
    ///
    /// Note: Requires `BrainConfig` to implement `Deserialize`.
    /// Currently not fully implemented - will return error.
    pub fn from_json_file<P: AsRef<Path>>(_path: P) -> Result<Self, ConfigError> {
        // TODO: Implement Deserialize for BrainConfig and all nested types
        Err(ConfigError::NotImplemented(
            "JSON deserialization requires BrainConfig to implement Deserialize".to_string(),
        ))
    }

    /// Load from JSON string
    ///
    /// Note: Requires `BrainConfig` to implement `Deserialize`.
    /// Currently not fully implemented - will return error.
    pub fn from_json(_content: &str) -> Result<Self, ConfigError> {
        // TODO: Implement Deserialize for BrainConfig and all nested types
        Err(ConfigError::NotImplemented(
            "JSON deserialization requires BrainConfig to implement Deserialize".to_string(),
        ))
    }

    /// Load from environment variables
    pub fn from_env() -> Result<Self, ConfigError> {
        let mut config = BrainConfig::default();

        // NEAT config
        if let Ok(val) = std::env::var("BRAIN_NEAT_POPULATION_SIZE") {
            config.neat.population_size = val
                .parse()
                .map_err(|e| ConfigError::InvalidValue(format!("NEAT population size: {}", e)))?;
        }

        if let Ok(val) = std::env::var("BRAIN_NEAT_MUTATION_RATE") {
            config.neat.mutation_rate = val
                .parse()
                .map_err(|e| ConfigError::InvalidValue(format!("NEAT mutation rate: {}", e)))?;
        }

        // PAD config
        if let Ok(val) = std::env::var("BRAIN_PAD_ENABLED") {
            config.pad.enabled = val
                .parse()
                .map_err(|e| ConfigError::InvalidValue(format!("PAD enabled: {}", e)))?;
        }

        if let Ok(val) = std::env::var("BRAIN_PAD_DECAY_RATE") {
            config.pad.decay_rate = val
                .parse()
                .map_err(|e| ConfigError::InvalidValue(format!("PAD decay rate: {}", e)))?;
        }

        if let Ok(val) = std::env::var("BRAIN_PAD_BASELINE") {
            config.pad.baseline = match val.to_lowercase().as_str() {
                "neutral" => BaselineEmotion::Neutral,
                "optimistic" => BaselineEmotion::Optimistic,
                "pessimistic" => BaselineEmotion::Pessimistic,
                "highenergy" => BaselineEmotion::HighEnergy,
                "calm" => BaselineEmotion::Calm,
                _ => {
                    return Err(ConfigError::InvalidValue(format!(
                        "Unknown baseline: {}",
                        val
                    )))
                }
            };
        }

        // Memory config
        if let Ok(val) = std::env::var("BRAIN_MEMORY_STM_CAPACITY") {
            config.memory.stm_capacity = val
                .parse()
                .map_err(|e| ConfigError::InvalidValue(format!("STM capacity: {}", e)))?;
        }

        if let Ok(val) = std::env::var("BRAIN_MEMORY_ENABLED") {
            config.memory.enabled = val
                .parse()
                .map_err(|e| ConfigError::InvalidValue(format!("Memory enabled: {}", e)))?;
        }

        // Personality config
        if let Ok(val) = std::env::var("BRAIN_PERSONALITY_ADAPTATION") {
            config.personality.adaptation_enabled = val
                .parse()
                .map_err(|e| ConfigError::InvalidValue(format!("Personality adaptation: {}", e)))?;
        }

        // Parallel config
        if let Ok(val) = std::env::var("BRAIN_PARALLEL_ENABLED") {
            config.parallel.enabled = val
                .parse()
                .map_err(|e| ConfigError::InvalidValue(format!("Parallel enabled: {}", e)))?;
        }

        if let Ok(val) = std::env::var("BRAIN_PARALLEL_WORKER_THREADS") {
            config.parallel.worker_threads = val
                .parse()
                .map_err(|e| ConfigError::InvalidValue(format!("Worker threads: {}", e)))?;
        }

        // Feature toggles
        if let Ok(val) = std::env::var("BRAIN_FEATURE_LEARNING") {
            config.features.learning = val
                .parse()
                .map_err(|e| ConfigError::InvalidValue(format!("Learning feature: {}", e)))?;
        }

        if let Ok(val) = std::env::var("BRAIN_FEATURE_SOCIAL") {
            config.features.social = val
                .parse()
                .map_err(|e| ConfigError::InvalidValue(format!("Social feature: {}", e)))?;
        }

        if let Ok(val) = std::env::var("BRAIN_FEATURE_METACOGNITION") {
            config.features.metacognition = val
                .parse()
                .map_err(|e| ConfigError::InvalidValue(format!("Metacognition feature: {}", e)))?;
        }

        Ok(Self {
            brain: config,
            source: ConfigSource::Environment,
        })
    }

    /// Set NEAT config
    pub fn with_neat(mut self, config: NeatConfig) -> Self {
        self.brain.neat = config;
        self
    }

    /// Set PAD config
    pub fn with_pad(mut self, config: PadConfig) -> Self {
        self.brain.pad = config;
        self
    }

    /// Set memory config
    pub fn with_memory(mut self, config: MemoryConfig) -> Self {
        self.brain.memory = config;
        self
    }

    /// Set personality config
    pub fn with_personality(mut self, config: PersonalityConfig) -> Self {
        self.brain.personality = config;
        self
    }

    /// Set parallel config
    pub fn with_parallel(mut self, config: ParallelConfig) -> Self {
        self.brain.parallel = config;
        self
    }

    /// Set feature toggles
    pub fn with_features(mut self, features: FeatureToggles) -> Self {
        self.brain.features = features;
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> ValidationResult {
        ConfigValidator::validate(&self.brain)
    }

    /// Build the final configuration
    pub fn build(self) -> Result<BrainConfig, ConfigError> {
        let result = self.validate();
        if result.is_valid() {
            Ok(self.brain)
        } else {
            Err(ConfigError::ValidationError(result.errors))
        }
    }

    /// Get the source of configuration
    pub fn source(&self) -> ConfigSource {
        self.source
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration errors
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigError {
    Io(String),
    ParseError(String),
    InvalidValue(String),
    ValidationError(Vec<ValidationError>),
    NotFound(String),
    NotImplemented(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(msg) => write!(f, "IO error: {}", msg),
            ConfigError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            ConfigError::InvalidValue(msg) => write!(f, "Invalid value: {}", msg),
            ConfigError::ValidationError(errors) => {
                write!(f, "Validation errors:")?;
                for error in errors {
                    write!(f, "\n  - {}", error)?;
                }
                Ok(())
            }
            ConfigError::NotFound(msg) => write!(f, "Not found: {}", msg),
            ConfigError::NotImplemented(msg) => write!(f, "Not implemented: {}", msg),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::Io(e.to_string())
    }
}

/// Configuration preset profiles
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigProfile {
    /// Minimal resource usage
    Lightweight,
    /// Balanced performance
    Standard,
    /// High performance
    HighPerformance,
    /// Research/experimental
    Experimental,
}

impl ConfigProfile {
    /// Apply profile to config builder
    pub fn apply(&self) -> ConfigBuilder {
        match self {
            ConfigProfile::Lightweight => ConfigBuilder {
                brain: BrainConfig::lightweight(),
                source: ConfigSource::Profile(*self),
            },
            ConfigProfile::Standard => ConfigBuilder {
                brain: BrainConfig::standard(),
                source: ConfigSource::Profile(*self),
            },
            ConfigProfile::HighPerformance => ConfigBuilder {
                brain: BrainConfig::high_performance(),
                source: ConfigSource::Profile(*self),
            },
            ConfigProfile::Experimental => {
                let mut config = BrainConfig::standard();
                config.features.metacognition = true;
                config.features.creativity = true;
                config.features.detailed_logging = true;
                ConfigBuilder {
                    brain: config,
                    source: ConfigSource::Profile(*self),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder_default() {
        let builder = ConfigBuilder::new();
        let config = builder.build().unwrap();
        assert!(config.memory.enabled);
        assert!(config.pad.enabled);
    }

    #[test]
    fn test_config_builder_from_toml() {
        let toml = r#"
[memory]
enabled = true
stm_capacity = 10
consolidation_threshold = 5
decay_rate = 0.1
"#;

        // Note: This would need proper TOML deserialization
        // For now just test that it doesn't panic
        let _result = ConfigBuilder::from_toml(toml);
        // Result depends on BrainConfig's Deserialize implementation
    }

    #[test]
    fn test_config_profile_lightweight() {
        let builder = ConfigProfile::Lightweight.apply();
        let config = builder.build().unwrap();
        assert!(!config.features.learning);
        assert!(!config.features.social);
    }

    #[test]
    fn test_config_profile_high_performance() {
        let builder = ConfigProfile::HighPerformance.apply();
        let config = builder.build().unwrap();
        assert!(config.parallel.enabled);
        assert_eq!(config.parallel.worker_threads, 4);
    }
}
