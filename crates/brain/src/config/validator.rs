//! Configuration Validator
//!
//! Validates configuration values and provides helpful error messages.

use std::fmt;

use crate::BrainConfig;

/// Validation error
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
    pub suggestion: Option<String>,
}

impl ValidationError {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
            suggestion: None,
        }
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.message)?;
        if let Some(suggestion) = &self.suggestion {
            write!(f, " (suggestion: {})", suggestion)?;
        }
        Ok(())
    }
}

/// Validation result
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationError>,
}

impl ValidationResult {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    pub fn add_error(&mut self, error: ValidationError) {
        self.errors.push(error);
    }

    pub fn add_warning(&mut self, warning: ValidationError) {
        self.warnings.push(warning);
    }

    pub fn merge(&mut self, other: ValidationResult) {
        self.errors.extend(other.errors);
        self.warnings.extend(other.warnings);
    }
}

impl Default for ValidationResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration validator
pub struct ConfigValidator;

impl ConfigValidator {
    /// Validate a BrainConfig
    pub fn validate(config: &BrainConfig) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Validate NEAT config
        result.merge(Self::validate_neat(&config.neat));

        // Validate PAD config
        result.merge(Self::validate_pad(&config.pad));

        // Validate memory config
        result.merge(Self::validate_memory(&config.memory));

        // Validate personality config
        result.merge(Self::validate_personality(&config.personality));

        // Validate parallel config
        result.merge(Self::validate_parallel(&config.parallel));

        // Validate feature toggles
        result.merge(Self::validate_features(&config.features));

        // Cross-field validations
        result.merge(Self::validate_cross_fields(config));

        result
    }

    /// Validate NEAT configuration
    fn validate_neat(config: &crate::neat::config::NeatConfig) -> ValidationResult {
        let mut result = ValidationResult::new();

        if config.population_size == 0 {
            result.add_error(
                ValidationError::new(
                    "neat.population_size",
                    "Population size must be greater than 0",
                )
                .with_suggestion("Set population_size to at least 10"),
            );
        }

        if config.population_size > 10000 {
            result.add_warning(
                ValidationError::new(
                    "neat.population_size",
                    "Very large population size may cause performance issues",
                )
                .with_suggestion("Consider using a smaller population (100-1000)"),
            );
        }

        if !(0.0..=1.0).contains(&config.mutation_rate) {
            result.add_error(
                ValidationError::new(
                    "neat.mutation_rate",
                    "Mutation rate must be between 0.0 and 1.0",
                )
                .with_suggestion("Use a value between 0.01 and 0.3"),
            );
        }

        if config.min_species_size < 1 {
            result.add_error(
                ValidationError::new(
                    "neat.min_species_size",
                    "Minimum species size must be at least 1",
                )
                .with_suggestion("Set min_species_size to at least 2"),
            );
        }

        result
    }

    /// Validate PAD configuration
    fn validate_pad(config: &crate::PadConfig) -> ValidationResult {
        let mut result = ValidationResult::new();

        if !(0.0..=1.0).contains(&config.decay_rate) {
            result.add_error(
                ValidationError::new("pad.decay_rate", "Decay rate must be between 0.0 and 1.0")
                    .with_suggestion("Use a value between 0.001 and 0.1"),
            );
        }

        if !(0.0..=1.0).contains(&config.contagion_rate) {
            result.add_error(
                ValidationError::new(
                    "pad.contagion_rate",
                    "Contagion rate must be between 0.0 and 1.0",
                )
                .with_suggestion("Use a value between 0.1 and 0.5"),
            );
        }

        result
    }

    /// Validate memory configuration
    fn validate_memory(config: &crate::MemoryConfig) -> ValidationResult {
        let mut result = ValidationResult::new();

        if config.stm_capacity == 0 {
            result.add_error(
                ValidationError::new("memory.stm_capacity", "STM capacity must be greater than 0")
                    .with_suggestion("Use a value between 5 and 9 (7±2 rule)"),
            );
        }

        if config.stm_capacity > 100 {
            result.add_warning(
                ValidationError::new(
                    "memory.stm_capacity",
                    "Very large STM capacity may not be biologically plausible",
                )
                .with_suggestion("Consider using a smaller capacity (5-15)"),
            );
        }

        if !(0.0..=1.0).contains(&config.decay_rate) {
            result.add_error(
                ValidationError::new(
                    "memory.decay_rate",
                    "Decay rate must be between 0.0 and 1.0",
                )
                .with_suggestion("Use a value between 0.01 and 0.5"),
            );
        }

        result
    }

    /// Validate personality configuration
    fn validate_personality(config: &crate::PersonalityConfig) -> ValidationResult {
        let mut result = ValidationResult::new();

        if !(0.0..=1.0).contains(&config.learning_rate) {
            result.add_error(
                ValidationError::new(
                    "personality.learning_rate",
                    "Learning rate must be between 0.0 and 1.0",
                )
                .with_suggestion("Use a value between 0.001 and 0.1"),
            );
        }

        // Validate initial profile if provided
        if let Some((o, c, e, a, n)) = config.initial_profile {
            let dims = [
                ("openness", o),
                ("conscientiousness", c),
                ("extraversion", e),
                ("agreeableness", a),
                ("neuroticism", n),
            ];

            for (name, value) in dims {
                if !(0.0..=1.0).contains(&value) {
                    result.add_error(ValidationError::new(
                        format!("personality.initial_profile.{}", name),
                        format!("{} must be between 0.0 and 1.0", name),
                    ));
                }
            }
        }

        result
    }

    /// Validate parallel configuration
    fn validate_parallel(config: &crate::ParallelConfig) -> ValidationResult {
        let mut result = ValidationResult::new();

        if config.worker_threads > 128 {
            result.add_warning(
                ValidationError::new(
                    "parallel.worker_threads",
                    "Very high thread count may cause resource contention",
                )
                .with_suggestion("Use 0 (auto) or match CPU cores"),
            );
        }

        if config.min_batch_size == 0 {
            result.add_error(
                ValidationError::new(
                    "parallel.min_batch_size",
                    "Minimum batch size must be greater than 0",
                )
                .with_suggestion("Use a value between 10 and 1000"),
            );
        }

        result
    }

    /// Validate feature toggles
    fn validate_features(config: &crate::FeatureToggles) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Check if all features are disabled
        if !config.learning && !config.social && !config.metacognition && !config.creativity {
            result.add_warning(
                ValidationError::new("features", "All advanced features are disabled")
                    .with_suggestion("Enable at least one feature for better functionality"),
            );
        }

        result
    }

    /// Validate cross-field constraints
    fn validate_cross_fields(config: &BrainConfig) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Check if memory is disabled but features requiring memory are enabled
        if !config.memory.enabled {
            if config.features.metacognition {
                result.add_warning(
                    ValidationError::new(
                        "cross_field",
                        "Metacognition requires memory but memory is disabled",
                    )
                    .with_suggestion("Enable memory or disable metacognition"),
                );
            }
        }

        // Check if parallel is enabled but worker threads is 0
        if config.parallel.enabled && config.parallel.worker_threads == 0 {
            result.add_warning(
                ValidationError::new(
                    "cross_field",
                    "Parallel processing enabled but worker_threads is 0 (auto)",
                )
                .with_suggestion("Set explicit worker_threads for reproducibility"),
            );
        }

        result
    }

    /// Validate a single value range
    pub fn validate_range<T: PartialOrd + std::fmt::Debug>(
        value: T,
        min: T,
        max: T,
        field: &str,
    ) -> Result<(), ValidationError> {
        if value < min || value > max {
            Err(ValidationError::new(
                field,
                format!("Value must be between {:?} and {:?}", min, max),
            ))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MemoryConfig, PadConfig};

    #[test]
    fn test_validation_result_valid() {
        let result = ValidationResult::new();
        assert!(result.is_valid());
        assert!(!result.has_warnings());
    }

    #[test]
    fn test_validation_result_with_errors() {
        let mut result = ValidationResult::new();
        result.add_error(ValidationError::new("field", "error"));
        assert!(!result.is_valid());
    }

    #[test]
    fn test_validation_result_with_warnings() {
        let mut result = ValidationResult::new();
        result.add_warning(ValidationError::new("field", "warning"));
        assert!(result.is_valid());
        assert!(result.has_warnings());
    }

    #[test]
    fn test_validate_pad_config() {
        let config = PadConfig {
            enabled: true,
            decay_rate: 0.5,
            contagion_rate: 0.3,
            baseline: crate::BaselineEmotion::Neutral,
        };

        let result = ConfigValidator::validate_pad(&config);
        assert!(result.is_valid());
    }

    #[test]
    fn test_validate_pad_config_invalid_decay() {
        let config = PadConfig {
            enabled: true,
            decay_rate: 1.5, // Invalid
            contagion_rate: 0.3,
            baseline: crate::BaselineEmotion::Neutral,
        };

        let result = ConfigValidator::validate_pad(&config);
        assert!(!result.is_valid());
    }

    #[test]
    fn test_validate_memory_config() {
        let config = MemoryConfig::default();
        let result = ConfigValidator::validate_memory(&config);
        assert!(result.is_valid());
    }

    #[test]
    fn test_validate_memory_config_zero_capacity() {
        let config = MemoryConfig {
            enabled: true,
            stm_capacity: 0, // Invalid
            consolidation_threshold: 3,
            decay_rate: 0.1,
        };

        let result = ConfigValidator::validate_memory(&config);
        assert!(!result.is_valid());
    }

    #[test]
    fn test_validate_range() {
        assert!(ConfigValidator::validate_range(0.5, 0.0, 1.0, "test").is_ok());
        assert!(ConfigValidator::validate_range(1.5, 0.0, 1.0, "test").is_err());
    }

    #[test]
    fn test_validation_error_display() {
        let error = ValidationError::new("field", "message").with_suggestion("try this");
        let display = format!("{}", error);
        assert!(display.contains("field"));
        assert!(display.contains("message"));
        assert!(display.contains("try this"));
    }
}
