//! Test Utilities
//!
//! Helper functions and types for testing the brain module.
//! Only compiled in test mode.

#![cfg(test)]

use serde_json::json;

use crate::cognition::{Action, CognitiveState, Goal, MemoryItem, WorkingMemory};
use crate::emotion::state::EmotionState;
use crate::memory::{EpisodicMemory, MemoryQuery, SemanticMemory, ShortTermMemory, UnifiedMemory};
use crate::neat::{Genome, NeatConfig, NeuralNetwork};
use crate::pad::{EmotionalIntelligence, Pad};
use crate::personality::OceanProfile;
use crate::{ApiConfig, SocialBrainApi};

/// Create a test API instance with default configuration
pub fn create_test_api() -> SocialBrainApi {
    SocialBrainApi::new()
}

/// Create a test API with custom configuration
pub fn create_test_api_with_config(config: ApiConfig) -> SocialBrainApi {
    SocialBrainApi::with_config(config)
}

/// Create a disabled API (all features off)
pub fn create_disabled_api() -> SocialBrainApi {
    let config = ApiConfig {
        memory_enabled: false,
        emotion_enabled: false,
        learning_enabled: false,
        personality_influence: 0.0,
    };
    SocialBrainApi::with_config(config)
}

/// Create a test PAD state
pub fn create_test_pad() -> Pad {
    Pad::new(0.5, 0.3, 0.2)
}

/// Create a neutral PAD state
pub fn create_neutral_pad() -> Pad {
    Pad::new(0.0, 0.0, 0.5)
}

/// Create an emotional intelligence instance
pub fn create_test_emotional_intelligence() -> EmotionalIntelligence {
    EmotionalIntelligence::new()
}

/// Create a balanced OCEAN personality
pub fn create_balanced_personality() -> OceanProfile {
    OceanProfile::balanced()
}

/// Create an extroverted personality
pub fn create_extroverted_personality() -> OceanProfile {
    OceanProfile {
        openness: 0.8,
        conscientiousness: 0.5,
        extraversion: 0.9,
        agreeableness: 0.6,
        neuroticism: 0.3,
    }
}

/// Create an introverted personality
pub fn create_introverted_personality() -> OceanProfile {
    OceanProfile {
        openness: 0.4,
        conscientiousness: 0.7,
        extraversion: 0.2,
        agreeableness: 0.5,
        neuroticism: 0.6,
    }
}

/// Create a test short-term memory
pub fn create_test_stm() -> ShortTermMemory {
    ShortTermMemory::new()
}

/// Create a test episodic memory
pub fn create_test_episodic_memory() -> EpisodicMemory {
    EpisodicMemory::new()
}

/// Create a test semantic memory
pub fn create_test_semantic_memory() -> SemanticMemory {
    SemanticMemory::new()
}

/// Create a test unified memory
pub fn create_test_unified_memory() -> UnifiedMemory {
    UnifiedMemory::new()
}

/// Create a memory query for testing
pub fn create_test_memory_query(query: &str) -> MemoryQuery {
    MemoryQuery::new(query)
}

/// Create a test cognitive state
pub fn create_test_cognitive_state() -> CognitiveState {
    CognitiveState::new()
}

/// Create a test goal
pub fn create_test_goal(description: &str, priority: f32) -> Goal {
    Goal::new(description, priority)
}

/// Create a test working memory with capacity
pub fn create_test_working_memory(capacity: usize) -> WorkingMemory {
    WorkingMemory::new(capacity)
}

/// Create a test memory item
pub fn create_test_memory_item(key: &str, value: serde_json::Value, activation: f32) -> MemoryItem {
    MemoryItem {
        key: key.to_string(),
        value,
        activation,
        timestamp: crate::utils::current_timestamp_secs(),
    }
}

/// Create a test action
pub fn create_test_action(name: &str) -> Action {
    Action::new(name)
}

/// Create a minimal NEAT genome
pub fn create_minimal_genome() -> Genome {
    Genome::new_minimal(0, 4, 2)
}

/// Create a NEAT genome with random weights
pub fn create_random_genome(id: u64, input_size: usize, output_size: usize) -> Genome {
    let mut genome = Genome::new_minimal(id, input_size, output_size);
    let config = NeatConfig::standard();
    genome.mutate_weights(&config);
    genome
}

/// Create a neural network from genome
pub fn create_test_network(genome: &Genome) -> NeuralNetwork {
    NeuralNetwork::from_genome(genome)
}

/// Create an emotion state
pub fn create_emotion_state(pleasure: f64, arousal: f64, dominance: f64) -> EmotionState {
    EmotionState::new(pleasure, arousal, dominance)
}

/// Create a neutral emotion state
pub fn create_neutral_emotion() -> EmotionState {
    EmotionState::neutral()
}

/// Assert that two f32 values are approximately equal
pub fn assert_f32_approx_eq(a: f32, b: f32, epsilon: f32) {
    let diff = (a - b).abs();
    assert!(
        diff < epsilon,
        "Expected {} to be approximately equal to {} (diff: {}, epsilon: {})",
        a,
        b,
        diff,
        epsilon
    );
}

/// Assert that two f64 values are approximately equal
pub fn assert_f64_approx_eq(a: f64, b: f64, epsilon: f64) {
    let diff = (a - b).abs();
    assert!(
        diff < epsilon,
        "Expected {} to be approximately equal to {} (diff: {}, epsilon: {})",
        a,
        b,
        diff,
        epsilon
    );
}

/// Assert that two PAD states are approximately equal
pub fn assert_pad_approx_eq(a: Pad, b: Pad, epsilon: f32) {
    assert_f32_approx_eq(a.pleasure, b.pleasure, epsilon);
    assert_f32_approx_eq(a.arousal, b.arousal, epsilon);
    assert_f32_approx_eq(a.dominance, b.dominance, epsilon);
}

/// Assert that two emotion states are approximately equal
pub fn assert_emotion_approx_eq(a: EmotionState, b: EmotionState, epsilon: f64) {
    assert_f64_approx_eq(a.pleasure, b.pleasure, epsilon);
    assert_f64_approx_eq(a.arousal, b.arousal, epsilon);
    assert_f64_approx_eq(a.dominance, b.dominance, epsilon);
}

/// Check if a result is within a range (inclusive)
pub fn assert_in_range<T: PartialOrd + std::fmt::Debug>(value: T, min: T, max: T, name: &str) {
    assert!(
        value >= min && value <= max,
        "Expected {} to be in range [{:?}, {:?}], got {:?}",
        name,
        min,
        max,
        value
    );
}

/// Generate a range of test strings
pub fn test_strings() -> Vec<&'static str> {
    vec![
        "Hello, world!",
        "Test message",
        "",
        "A",
        "中文测试",
        "🎉 Emoji test 🚀",
        "Special chars: @#$%^&*()",
        "New\nLine\tTab",
        "   Spaces   ",
    ]
}

/// Generate test priorities
pub fn test_priorities() -> Vec<f32> {
    vec![0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0]
}

/// Generate test importance values
pub fn test_importance_values() -> Vec<f32> {
    vec![0.0, 0.33, 0.5, 0.66, 1.0]
}

/// Generate boundary values for f32 testing
pub fn boundary_f32_values() -> Vec<f32> {
    vec![
        0.0,
        1.0,
        -1.0,
        f32::MIN,
        f32::MAX,
        f32::EPSILON,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ]
}

/// Create a test scenario with pre-populated memory
pub fn create_memory_test_scenario() -> SocialBrainApi {
    let mut api = create_test_api();

    // Store some memories
    api.store_memory("First memory", 0.8).unwrap();
    api.store_memory("Second memory", 0.5).unwrap();
    api.store_memory("Important: third memory", 0.9).unwrap();

    api
}

/// Create a test scenario with goals
pub fn create_goals_test_scenario() -> SocialBrainApi {
    let mut api = create_test_api();

    // Set some goals
    api.set_goal("Complete project", 0.9).unwrap();
    api.set_goal("Learn Rust", 0.7).unwrap();
    api.set_goal("Write tests", 0.8).unwrap();

    api
}

/// Create a test scenario with emotional history
pub fn create_emotion_test_scenario() -> SocialBrainApi {
    let mut api = create_test_api();

    // Apply various emotional stimuli
    api.apply_emotional_stimulus(Pad::new(0.8, 0.5, 0.3), 0.5); // Positive
    api.apply_emotional_stimulus(Pad::new(-0.3, 0.2, 0.1), 0.3); // Slightly negative
    api.apply_emotional_stimulus(Pad::new(0.0, 0.8, 0.5), 0.7); // High arousal

    api
}

/// Macro for asserting approximate equality
#[macro_export]
macro_rules! assert_approx_eq {
    ($a:expr, $b:expr, $epsilon:expr) => {
        assert!(
            ($a as f64 - $b as f64).abs() < ($epsilon as f64),
            "Expected {} to be approximately {} (diff: {})",
            $a,
            $b,
            ($a as f64 - $b as f64).abs()
        );
    };
}

/// Macro for asserting PAD equality
#[macro_export]
macro_rules! assert_pad_eq {
    ($a:expr, $b:expr, $epsilon:expr) => {
        assert_approx_eq!($a.pleasure, $b.pleasure, $epsilon);
        assert_approx_eq!($a.arousal, $b.arousal, $epsilon);
        assert_approx_eq!($a.dominance, $b.dominance, $epsilon);
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_test_api() {
        let api = create_test_api();
        let stats = api.stats();
        assert_eq!(stats.active_goals, 0);
    }

    #[test]
    fn test_create_test_pad() {
        let pad = create_test_pad();
        assert_eq!(pad.pleasure, 0.5);
        assert_eq!(pad.arousal, 0.3);
        assert_eq!(pad.dominance, 0.2);
    }

    #[test]
    fn test_create_neutral_pad() {
        let pad = create_neutral_pad();
        assert_eq!(pad.pleasure, 0.0);
        assert_eq!(pad.arousal, 0.0);
    }

    #[test]
    fn test_assert_f32_approx_eq() {
        assert_f32_approx_eq(1.0, 1.0001, 0.001);
    }

    #[test]
    #[should_panic]
    fn test_assert_f32_approx_eq_fails() {
        assert_f32_approx_eq(1.0, 1.1, 0.001);
    }

    #[test]
    fn test_assert_pad_approx_eq() {
        let pad1 = Pad::new(0.5, 0.3, 0.2);
        let pad2 = Pad::new(0.5001, 0.3001, 0.2001);
        assert_pad_approx_eq(pad1, pad2, 0.001);
    }

    #[test]
    fn test_assert_in_range() {
        assert_in_range(0.5, 0.0, 1.0, "value");
    }

    #[test]
    fn test_create_test_goal() {
        let goal = create_test_goal("Test goal", 0.8);
        assert_eq!(goal.description, "Test goal");
        assert!((goal.priority - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_create_test_memory_item() {
        let item = create_test_memory_item("key", json!("value"), 0.9);
        assert_eq!(item.key, "key");
        assert_eq!(item.value, json!("value"));
        assert!((item.activation - 0.9).abs() < 0.001);
    }

    #[test]
    fn test_create_memory_test_scenario() {
        let api = create_memory_test_scenario();
        let stats = api.stats();
        assert!(stats.memory_items >= 3);
    }

    #[test]
    fn test_create_goals_test_scenario() {
        let api = create_goals_test_scenario();
        let stats = api.stats();
        assert_eq!(stats.active_goals, 3);
    }

    #[test]
    fn test_macro_assert_approx_eq() {
        assert_approx_eq!(1.0, 1.0001, 0.001);
    }

    #[test]
    fn test_macro_assert_pad_eq() {
        let pad1 = Pad::new(0.5, 0.3, 0.2);
        let pad2 = Pad::new(0.5001, 0.3001, 0.2001);
        assert_pad_eq!(pad1, pad2, 0.001);
    }
}
