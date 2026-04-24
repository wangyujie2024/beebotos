//! BeeBotOS Brain Module
//!
//! Cognitive architecture providing:
//! - NEAT neural evolution
//! - PAD emotional model
//! - OCEAN personality
//! - Multi-modal memory system
//! - Deductive reasoning
//! - Metacognition and self-reflection
//!
//! # Quick Start
//!
//! ```
//! use beebotos_brain::{ApiConfig, BrainConfig, SocialBrainApi};
//!
//! // Create API with default configuration
//! let mut api = SocialBrainApi::new();
//!
//! // Process a stimulus
//! let response = api.process_stimulus("Hello, world!").unwrap();
//!
//! // Set a goal
//! let goal_id = api.set_goal("Complete task", 0.8).unwrap();
//! ```

// =============================================================================
// Public Modules - Core API surface
// =============================================================================

/// Main public API
pub mod api;

/// Error types and handling
pub mod error;

/// Utility functions
pub mod utils;

/// Performance metrics collection
pub mod metrics;

/// Configuration management
pub mod config;

/// Performance optimizations
pub mod optimization;

// =============================================================================
// Internal Modules - Implementation details
// =============================================================================

mod attention;
mod cognition;
mod creativity;
mod emotion;
mod evolution;
mod knowledge;
mod language;
mod learning;
mod memory;
mod metacognition;
mod neat;
mod pad;
mod personality;
mod reasoning;
mod social;

// =============================================================================
// Test Utilities - Only compiled in test mode
// =============================================================================

#[cfg(test)]
pub mod test_utils;

// =============================================================================
// Public Re-exports - Clean API surface
// =============================================================================

// Core API types
pub use api::{ApiConfig, ApiStats, SocialBrainApi, StimulusResponse};
// Attention types
pub use attention::{
    Attention, AttentionFilter, Focus, FocusType, SaliencyMap, SelectiveAttention,
};
// Decision types
pub use cognition::decision::{
    Constraint, ConstraintType, Decision, DecisionContext, DecisionEngine, DecisionOption,
    ExpectedValueStrategy, MinimaxStrategy, Objective, RiskLevel, SatisficingStrategy, TimeHorizon,
};
// Perception types
pub use cognition::perception::{
    FeatureType, FeatureValue, Percept, PerceptualFeature, PerceptualInput, PerceptualSystem,
    SensoryModality, TextFeatureExtractor,
};
// Cognition types
pub use cognition::{
    Action, Belief, CognitiveError, CognitiveState, Goal, GoalStatus, Intention, IntentionStatus,
    MemoryItem, WorkingMemory,
};
// Config re-exports
pub use config::{
    loader, validator, ConfigBuilder, ConfigError, ConfigLoader, ConfigProfile, ConfigSource,
    ConfigValidator, ValidationError, ValidationResult,
};
// Creativity types
pub use creativity::{
    BrainstormingSession, BrainstormingStats, CreativeEngine, CreativeProcess, CreativeStage, Idea,
    Solution,
};
// PAD/Emotion types
pub use emotion::{EmotionConfig, EmotionEngine, EmotionState, EmotionType};
// Error types
pub use error::{
    helpers as error_helpers, BrainError, BrainResult, MemoryError, NeatError, ReasoningError,
    ResultExt as BrainResultExt,
};
// Evolution types (re-exports from neat)
pub use evolution::*;
// Knowledge types
pub use knowledge::{
    Concept as KnowledgeConcept, Edge, KnowledgeEngine, KnowledgeGraph, Node, Ontology,
};
// Language types
pub use language::{nlp, sentiment, translation};
// Learning types
pub use learning::{
    CompositeSkill, LearningExperience, PolicyGradient, PrimitiveSkill, QLearning, ReplayBuffer,
    SkillLearner,
};
// Memory types
pub use memory::{
    embeddings::Embedding,
    index::{IndexStats, MemoryIndex, QueryPreprocessor},
    Concept, ConsolidationConfig, ConsolidationEngine, EmotionalTag, EmotionalValence, Episode,
    EpisodicMemory, Location, MemoryChunk, MemoryQuery, MemoryResults, MemoryType, Priority,
    ProceduralMemory, Procedure, PropertyValue, Relation, RelationType, SemanticMemory,
    ShortTermMemory, Step, UnifiedMemory,
};
// Metacognition types
pub use metacognition::{
    reflection::{ReflectionDepth, ReflectionType, ReflectiveSystem, ReflectiveThought},
    AdjustmentTrigger, AwarenessLevel, MetacognitionEngine, StrategyAdjustment, StrategyAssessment,
    StrategyEffectiveness,
};
// Metrics re-exports
pub use metrics::{
    global_metrics, increment_counter, record_timing, set_gauge, Histogram, MetricsCollector,
    MetricsSnapshot, Timer, TimingStats,
};
// Performance monitoring types (从 metrics 模块导出)
pub use metrics::{MetricPoint, MetricsCollector as PerformanceMonitor};
// Configuration types (defined in this module, no re-export needed)
// BrainConfig, PadConfig, MemoryConfig, PersonalityConfig, ParallelConfig, FeatureToggles,
// BaselineEmotion, VERSION

// NEAT types
pub use neat::{
    genome::{ActivationFn, ConnectionGene, LayerGene, LayerType, LearningParams},
    AgentBrain, FitnessResult, Genome, InnovationTracker, NeatConfig, NeuralNetwork, Population,
    Species,
};
// Optimization re-exports
pub use optimization::{
    batch_process, fast_contains, BufferPool, EfficientStringBuilder, FastClearVec, Lazy,
    StringPool, TimedCache,
};
pub use pad::{
    BasicEmotion, Emotion, EmotionCategory, EmotionalEvent, EmotionalIntelligence, EmotionalTrait,
    Pad,
};
// Personality types
pub use personality::{
    Behavior, DecisionStyle, Experience as PersonalityExperience, LearningStrategy, OceanEngine,
    OceanProfile, Outcome,
};
// Reasoning types
pub use reasoning::{Atom, Fact, InferenceResult, KnowledgeBase, ProofNode, Rule, Term};
// Social types
pub use social::{
    InteractionOutcome, InteractionType, Relationship, SocialAgent, SocialCognition, SocialContext,
    SocialGraph,
};
// Utility re-exports
pub use utils::{
    choose,
    choose_mut,
    clamp_f32,
    clamp_f64,
    clear_seed,
    compare_f32,
    compare_f64,
    current_timestamp_millis,
    current_timestamp_secs,
    max_f32,
    max_f64,
    min_f32,
    min_f64,
    random_bool,
    random_bool_seeded,
    // Random number generation
    random_f32,
    random_f32_range,
    random_f32_seeded,
    random_f64,
    random_f64_range,
    random_i32_range,
    random_u64,
    random_usize,
    set_seed,
    shuffle,
    truncate_with_ellipsis,
    validate_importance,
    validate_input_length,
    validate_priority,
};

/// Module version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// =============================================================================
// Configuration Types
// =============================================================================

/// Brain configuration
///
/// Central configuration for all cognitive modules.
#[derive(Debug, Clone)]
pub struct BrainConfig {
    /// NEAT neural evolution configuration
    pub neat: neat::config::NeatConfig,
    /// PAD emotion model configuration
    pub pad: PadConfig,
    /// Memory system configuration
    pub memory: MemoryConfig,
    /// Personality configuration
    pub personality: PersonalityConfig,
    /// Parallel processing configuration
    pub parallel: ParallelConfig,
    /// Feature toggles
    pub features: FeatureToggles,
}

/// PAD emotion model configuration
#[derive(Debug, Clone, Copy)]
pub struct PadConfig {
    /// Whether PAD emotion processing is enabled
    pub enabled: bool,
    /// Emotion decay rate (0.0 - 1.0)
    pub decay_rate: f64,
    /// Emotional contagion rate
    pub contagion_rate: f64,
    /// Baseline emotion (neutral, optimistic, pessimistic)
    pub baseline: BaselineEmotion,
}

impl Default for PadConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            decay_rate: 0.01,
            contagion_rate: 0.3,
            baseline: BaselineEmotion::Neutral,
        }
    }
}

/// Baseline emotion presets
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineEmotion {
    Neutral,
    Optimistic,
    Pessimistic,
    HighEnergy,
    Calm,
}

/// Memory system configuration
#[derive(Debug, Clone, Copy)]
pub struct MemoryConfig {
    /// Whether memory system is enabled
    pub enabled: bool,
    /// Short-term memory capacity (7±2 typical)
    pub stm_capacity: usize,
    /// Episodic memory consolidation threshold
    pub consolidation_threshold: u32,
    /// Memory decay rate
    pub decay_rate: f32,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            stm_capacity: 7,
            consolidation_threshold: 3,
            decay_rate: 0.1,
        }
    }
}

/// Personality configuration
#[derive(Debug, Clone, Copy)]
pub struct PersonalityConfig {
    /// Whether personality adaptation is enabled
    pub adaptation_enabled: bool,
    /// Learning rate for personality adaptation
    pub learning_rate: f32,
    /// Initial OCEAN profile
    pub initial_profile: Option<(f32, f32, f32, f32, f32)>,
}

impl Default for PersonalityConfig {
    fn default() -> Self {
        Self {
            adaptation_enabled: true,
            learning_rate: 0.01,
            initial_profile: None,
        }
    }
}

/// Parallel processing configuration
#[derive(Debug, Clone, Copy)]
pub struct ParallelConfig {
    /// Whether parallel processing is enabled
    pub enabled: bool,
    /// Number of worker threads (0 = use rayon default)
    pub worker_threads: usize,
    /// Minimum batch size for parallelization
    pub min_batch_size: usize,
}

impl Default for ParallelConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            worker_threads: 0,
            min_batch_size: 100,
        }
    }
}

/// Feature toggles for optional functionality
#[derive(Debug, Clone, Copy)]
pub struct FeatureToggles {
    /// Enable learning module
    pub learning: bool,
    /// Enable social cognition
    pub social: bool,
    /// Enable metacognition (self-reflection)
    pub metacognition: bool,
    /// Enable creativity module
    pub creativity: bool,
    /// Enable detailed logging
    pub detailed_logging: bool,
}

impl Default for FeatureToggles {
    fn default() -> Self {
        Self {
            learning: true,
            social: true,
            metacognition: true,
            creativity: true,
            detailed_logging: false,
        }
    }
}

impl BrainConfig {
    /// Create standard configuration
    pub fn standard() -> Self {
        Self::default()
    }

    /// Create lightweight configuration (fewer features, lower resource usage)
    pub fn lightweight() -> Self {
        Self {
            neat: neat::config::NeatConfig::conservative(),
            pad: PadConfig::default(),
            memory: MemoryConfig {
                enabled: true,
                stm_capacity: 5,
                consolidation_threshold: 5,
                decay_rate: 0.2,
            },
            personality: PersonalityConfig::default(),
            parallel: ParallelConfig {
                enabled: false,
                ..Default::default()
            },
            features: FeatureToggles {
                learning: false,
                social: false,
                metacognition: false,
                creativity: false,
                detailed_logging: false,
            },
        }
    }

    /// Create high-performance configuration
    pub fn high_performance() -> Self {
        Self {
            neat: neat::config::NeatConfig::aggressive(),
            pad: PadConfig::default(),
            memory: MemoryConfig {
                stm_capacity: 9,
                consolidation_threshold: 2,
                ..Default::default()
            },
            personality: PersonalityConfig::default(),
            parallel: ParallelConfig {
                enabled: true,
                worker_threads: 4,
                min_batch_size: 50,
            },
            features: FeatureToggles {
                detailed_logging: true,
                ..Default::default()
            },
        }
    }
}

impl Default for BrainConfig {
    fn default() -> Self {
        Self {
            neat: neat::config::NeatConfig::standard(),
            pad: PadConfig::default(),
            memory: MemoryConfig::default(),
            personality: PersonalityConfig::default(),
            parallel: ParallelConfig::default(),
            features: FeatureToggles::default(),
        }
    }
}

// =============================================================================
// Prelude Module
// =============================================================================

/// Prelude module for convenient imports
///
/// Import common types with: `use beebotos_brain::prelude::*;`
pub mod prelude {
    pub use crate::{
        current_timestamp_secs, increment_counter, random_bool, random_f32, random_usize,
        record_timing, set_seed, validate_importance, validate_priority, Action, ApiConfig,
        ApiStats, Attention, Belief, BrainConfig, BrainError, BrainResult, CognitiveState,
        DecisionContext, DecisionEngine, DecisionOption, EmotionState, EpisodicMemory, Fact, Focus,
        FocusType, Genome, Goal, KnowledgeBase, MemoryItem, MemoryQuery, MemoryResults, MemoryType,
        MetricsCollector, NeatConfig, NeuralNetwork, OceanProfile, Pad, PerceptualInput,
        PerceptualSystem, QLearning, ReplayBuffer, RiskLevel, Rule, SemanticMemory,
        SensoryModality, ShortTermMemory, SocialBrainApi, StimulusResponse, Timer, VERSION,
    };
}
