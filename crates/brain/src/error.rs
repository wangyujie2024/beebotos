//! Error handling for brain module
//!
//! Provides centralized error types and result aliases for the cognitive
//! architecture.

use std::fmt;

/// Main error type for brain operations
#[derive(Debug, Clone, PartialEq)]
pub enum BrainError {
    /// Memory operation errors
    MemoryError(String),

    /// Invalid state or configuration
    InvalidState(String),

    /// IO errors (wrapped)
    Io(String),

    /// Knowledge graph errors
    KnowledgeError(String),

    /// NEAT evolution errors
    EvolutionError(String),

    /// Emotion processing errors
    EmotionError(String),

    /// Reasoning/inference errors
    ReasoningError(String),

    /// Item not found
    NotFound(String),

    /// Invalid parameters
    InvalidParameter(String),

    /// Configuration errors
    ConfigError(String),

    /// External API errors
    ExternalApiError(String),

    /// Feature not implemented
    NotImplemented(String),

    /// Generic error with message
    Generic(String),
}

impl fmt::Display for BrainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BrainError::MemoryError(msg) => write!(f, "Memory error: {}", msg),
            BrainError::InvalidState(msg) => write!(f, "Invalid state: {}", msg),
            BrainError::Io(msg) => write!(f, "IO error: {}", msg),
            BrainError::KnowledgeError(msg) => write!(f, "Knowledge error: {}", msg),
            BrainError::EvolutionError(msg) => write!(f, "Evolution error: {}", msg),
            BrainError::EmotionError(msg) => write!(f, "Emotion error: {}", msg),
            BrainError::ReasoningError(msg) => write!(f, "Reasoning error: {}", msg),
            BrainError::NotFound(msg) => write!(f, "Not found: {}", msg),
            BrainError::InvalidParameter(msg) => write!(f, "Invalid parameter: {}", msg),
            BrainError::ConfigError(msg) => write!(f, "Configuration error: {}", msg),
            BrainError::ExternalApiError(msg) => write!(f, "External API error: {}", msg),
            BrainError::NotImplemented(msg) => write!(f, "Not implemented: {}", msg),
            BrainError::Generic(msg) => write!(f, "Error: {}", msg),
        }
    }
}

impl std::error::Error for BrainError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // BrainError variants don't wrap other errors directly,
        // they store String messages. Return None for source.
        None
    }
}

// =============================================================================
// From trait implementations for ergonomic error handling
// =============================================================================

impl From<&str> for BrainError {
    /// Convert a string slice to a generic BrainError
    ///
    /// # Example
    /// ```
    /// use beebotos_brain::error::BrainError;
    ///
    /// let err: BrainError = "something went wrong".into();
    /// assert!(matches!(err, BrainError::Generic(_)));
    /// ```
    fn from(msg: &str) -> Self {
        BrainError::Generic(msg.to_string())
    }
}

impl From<String> for BrainError {
    /// Convert a String to a generic BrainError
    fn from(msg: String) -> Self {
        BrainError::Generic(msg)
    }
}

impl From<MemoryError> for BrainError {
    /// Convert MemoryError to BrainError::MemoryError
    fn from(err: MemoryError) -> Self {
        BrainError::MemoryError(err.to_string())
    }
}

impl From<NeatError> for BrainError {
    /// Convert NeatError to BrainError::EvolutionError
    fn from(err: NeatError) -> Self {
        BrainError::EvolutionError(err.to_string())
    }
}

impl From<ReasoningError> for BrainError {
    /// Convert ReasoningError to BrainError::ReasoningError
    fn from(err: ReasoningError) -> Self {
        BrainError::ReasoningError(err.to_string())
    }
}

impl From<std::io::Error> for BrainError {
    /// Convert std::io::Error to BrainError::Io
    fn from(err: std::io::Error) -> Self {
        BrainError::Io(err.to_string())
    }
}

impl From<serde_json::Error> for BrainError {
    /// Convert serde_json::Error to BrainError::InvalidState
    fn from(err: serde_json::Error) -> Self {
        BrainError::InvalidState(format!("Serialization error: {}", err))
    }
}

// =============================================================================
// Result type alias
// =============================================================================

/// Convenience type alias for Results with BrainError
pub type BrainResult<T> = Result<T, BrainError>;

// =============================================================================
// Specialized error types
// =============================================================================

/// Memory-specific errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryError {
    ItemNotFound,
    CapacityExceeded,
    ConceptNotFound,
    DuplicateConcept,
    ConsolidationFailed,
}

impl fmt::Display for MemoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MemoryError::ItemNotFound => write!(f, "Memory item not found"),
            MemoryError::CapacityExceeded => write!(f, "Memory capacity exceeded"),
            MemoryError::ConceptNotFound => write!(f, "Concept not found"),
            MemoryError::DuplicateConcept => write!(f, "Duplicate concept"),
            MemoryError::ConsolidationFailed => write!(f, "Memory consolidation failed"),
        }
    }
}

impl std::error::Error for MemoryError {}

/// NEAT evolution errors
#[derive(Debug, Clone, PartialEq)]
pub enum NeatError {
    InvalidGenome(String),
    IncompatibleGenomes,
    PopulationExtinct,
    InvalidConfiguration(String),
}

impl fmt::Display for NeatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NeatError::InvalidGenome(msg) => write!(f, "Invalid genome: {}", msg),
            NeatError::IncompatibleGenomes => write!(f, "Genomes are incompatible"),
            NeatError::PopulationExtinct => write!(f, "Population has gone extinct"),
            NeatError::InvalidConfiguration(msg) => write!(f, "Invalid NEAT config: {}", msg),
        }
    }
}

impl std::error::Error for NeatError {}

/// Reasoning errors
#[derive(Debug, Clone, PartialEq)]
pub enum ReasoningError {
    InvalidRule(String),
    Contradiction(String),
    InsufficientInformation,
    InferenceFailed(String),
}

impl fmt::Display for ReasoningError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReasoningError::InvalidRule(msg) => write!(f, "Invalid rule: {}", msg),
            ReasoningError::Contradiction(msg) => write!(f, "Contradiction detected: {}", msg),
            ReasoningError::InsufficientInformation => write!(f, "Insufficient information"),
            ReasoningError::InferenceFailed(msg) => write!(f, "Inference failed: {}", msg),
        }
    }
}

impl std::error::Error for ReasoningError {}

// =============================================================================
// Helper functions
// =============================================================================

/// Helper functions for creating common errors
pub mod helpers {
    use super::*;

    /// Create a not found error
    ///
    /// # Example
    /// ```
    /// use beebotos_brain::error::helpers;
    ///
    /// let err = helpers::not_found("user_123");
    /// assert_eq!(format!("{}", err), "Not found: user_123");
    /// ```
    pub fn not_found<T: Into<String>>(item: T) -> BrainError {
        BrainError::NotFound(item.into())
    }

    /// Create an invalid state error
    pub fn invalid_state<T: Into<String>>(msg: T) -> BrainError {
        BrainError::InvalidState(msg.into())
    }

    /// Create a memory error
    pub fn memory_error<T: Into<String>>(msg: T) -> BrainError {
        BrainError::MemoryError(msg.into())
    }

    /// Create a parameter error
    pub fn invalid_param<T: Into<String>>(msg: T) -> BrainError {
        BrainError::InvalidParameter(msg.into())
    }

    /// Create a "not implemented" error
    pub fn not_implemented<T: Into<String>>(feature: T) -> BrainError {
        BrainError::NotImplemented(feature.into())
    }

    /// Create a config error
    pub fn config_error<T: Into<String>>(msg: T) -> BrainError {
        BrainError::ConfigError(msg.into())
    }
}

// =============================================================================
// Extension traits for Result
// =============================================================================

/// Extension trait for Result types to provide convenient error conversion
pub trait ResultExt<T, E> {
    /// Convert the error type to BrainError
    fn into_brain_error(self) -> BrainResult<T>;
}

impl<T> ResultExt<T, MemoryError> for Result<T, MemoryError> {
    fn into_brain_error(self) -> BrainResult<T> {
        self.map_err(BrainError::from)
    }
}

impl<T> ResultExt<T, NeatError> for Result<T, NeatError> {
    fn into_brain_error(self) -> BrainResult<T> {
        self.map_err(BrainError::from)
    }
}

impl<T> ResultExt<T, ReasoningError> for Result<T, ReasoningError> {
    fn into_brain_error(self) -> BrainResult<T> {
        self.map_err(BrainError::from)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    #[test]
    fn test_error_display() {
        let err = BrainError::MemoryError("test".to_string());
        assert_eq!(format!("{}", err), "Memory error: test");
    }

    #[test]
    fn test_brain_result_ok() {
        let result: BrainResult<i32> = Ok(42);
        assert!(result.is_ok());
    }

    #[test]
    fn test_brain_result_err() {
        let result: BrainResult<i32> = Err(BrainError::NotFound("item".to_string()));
        assert!(result.is_err());
    }

    #[test]
    fn test_memory_error_display() {
        let err = MemoryError::ItemNotFound;
        assert_eq!(format!("{}", err), "Memory item not found");
    }

    #[test]
    fn test_helpers() {
        let err = helpers::not_found("test");
        assert!(matches!(err, BrainError::NotFound(_)));
    }

    // =============================================================================
    // From trait tests
    // =============================================================================

    #[test]
    fn test_from_str() {
        let err: BrainError = "test error".into();
        assert!(matches!(err, BrainError::Generic(msg) if msg == "test error"));
    }

    #[test]
    fn test_from_string() {
        let err: BrainError = "test error".to_string().into();
        assert!(matches!(err, BrainError::Generic(msg) if msg == "test error"));
    }

    #[test]
    fn test_from_memory_error() {
        let mem_err = MemoryError::CapacityExceeded;
        let brain_err: BrainError = mem_err.into();
        assert!(matches!(brain_err, BrainError::MemoryError(_)));
        assert!(brain_err.to_string().contains("capacity exceeded"));
    }

    #[test]
    fn test_from_neat_error() {
        let neat_err = NeatError::PopulationExtinct;
        let brain_err: BrainError = neat_err.into();
        assert!(matches!(brain_err, BrainError::EvolutionError(_)));
    }

    #[test]
    fn test_from_reasoning_error() {
        let reason_err = ReasoningError::InsufficientInformation;
        let brain_err: BrainError = reason_err.into();
        assert!(matches!(brain_err, BrainError::ReasoningError(_)));
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let brain_err: BrainError = io_err.into();
        assert!(matches!(brain_err, BrainError::Io(_)));
    }

    #[test]
    fn test_question_mark_operator() {
        fn may_fail() -> Result<i32, MemoryError> {
            Err(MemoryError::ItemNotFound)
        }

        fn caller() -> BrainResult<i32> {
            let value = may_fail()?; // Should work with From trait
            Ok(value)
        }

        let result = caller();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), BrainError::MemoryError(_)));
    }

    #[test]
    fn test_result_ext() {
        let mem_result: Result<i32, MemoryError> = Err(MemoryError::ItemNotFound);
        let brain_result = mem_result.into_brain_error();
        assert!(brain_result.is_err());
    }

    #[test]
    fn test_helper_not_implemented() {
        let err = helpers::not_implemented("quantum neural networks");
        assert!(matches!(err, BrainError::NotImplemented(_)));
        assert!(err.to_string().contains("quantum neural networks"));
    }

    #[test]
    fn test_error_source() {
        let err = BrainError::Generic("test".to_string());
        // BrainError variants with String don't have a source
        assert!(err.source().is_none());
    }
}
