//! Boundary Condition Tests
//!
//! Tests for edge cases, boundary conditions, and error handling.

use beebotos_brain::{
    compare_f32, validate_importance, validate_input_length, validate_priority, ApiConfig,
    EpisodicMemory, Genome, MemoryQuery, MemoryType, NeatConfig, NeuralNetwork, OceanProfile, Pad,
    Priority, SemanticMemory, ShortTermMemory, SocialBrainApi,
};

// =============================================================================
// Pad/Emotion Boundary Tests
// =============================================================================

#[test]
fn test_pad_boundary_values() {
    // Test extreme values
    let pad1 = Pad::new(1.0, 1.0, 1.0);
    assert_eq!(pad1.pleasure, 1.0);
    assert_eq!(pad1.arousal, 1.0);
    assert_eq!(pad1.dominance, 1.0);

    let pad2 = Pad::new(-1.0, 0.0, 0.0);
    assert_eq!(pad2.pleasure, -1.0);
    assert_eq!(pad2.arousal, 0.0);
    assert_eq!(pad2.dominance, 0.0);
}

#[test]
fn test_pad_nan_handling() {
    // NaN should be handled gracefully
    let pad = Pad::new(f32::NAN, 0.5, 0.5);
    // The pad module should handle or propagate NaN appropriately
    assert!(pad.pleasure.is_nan() || pad.pleasure == 0.0); // Depends on
                                                           // implementation
}

// =============================================================================
// Memory Boundary Tests
// =============================================================================

#[test]
fn test_stm_zero_capacity() {
    // Edge case: capacity of 0
    let mut stm = ShortTermMemory::with_capacity(0);
    assert_eq!(stm.len(), 0);

    // Pushing to zero capacity should handle gracefully
    let evicted = stm.push("test");
    // Depending on implementation, might return Some(evicted) or handle
    // differently
}

#[test]
fn test_stm_boundary_capacity() {
    // Test at exact capacity limit
    let mut stm = ShortTermMemory::with_capacity(1);
    stm.push("first");
    assert_eq!(stm.len(), 1);

    let evicted = stm.push("second");
    assert!(evicted.is_some());
    assert_eq!(stm.len(), 1);
}

#[test]
fn test_stm_empty_retrieve() {
    let stm = ShortTermMemory::new();
    let results = stm.retrieve("anything");
    assert!(results.is_empty());
}

#[test]
fn test_stm_unicode_content() {
    let mut stm = ShortTermMemory::new();
    stm.push("中文内容测试");
    stm.push("🎉 Emoji test 🚀");
    stm.push("Arabic: مرحبا");

    let results = stm.retrieve("中文");
    assert_eq!(results.len(), 1);
}

#[test]
fn test_episodic_memory_time_boundaries() {
    let mut em = EpisodicMemory::new();

    // Test events at time boundaries
    em.encode("Event at 0", 0, None);
    em.encode("Event at u64::MAX", u64::MAX, None);

    let results = em.query_time_range(0, u64::MAX);
    assert_eq!(results.len(), 2);
}

#[test]
fn test_episodic_memory_empty_search() {
    let em = EpisodicMemory::new();
    let results = em.search("nonexistent");
    assert!(results.is_empty());
}

#[test]
fn test_semantic_memory_empty_name() {
    let mut sm = SemanticMemory::new();

    // Edge case: empty concept name
    let id = sm.learn_concept("", "Empty name concept", "Test");
    assert!(!id.is_empty());

    let found = sm.find_by_name("");
    assert!(found.is_some());
}

#[test]
fn test_semantic_memory_very_long_definition() {
    let mut sm = SemanticMemory::new();
    let long_def = "a".repeat(10000);

    let id = sm.learn_concept("LongDef", &long_def, "Test");
    assert!(!id.is_empty());

    let concept = sm.get(&id);
    assert!(concept.is_some());
    assert_eq!(concept.unwrap().definition.len(), 10000);
}

// =============================================================================
// API Boundary Tests
// =============================================================================

#[test]
fn test_api_empty_stimulus() {
    let mut api = SocialBrainApi::new();

    // Empty stimulus should now be rejected
    let result = api.process_stimulus("");
    assert!(result.is_err());
}

#[test]
fn test_api_very_long_stimulus() {
    let mut api = SocialBrainApi::new();
    let long_stimulus = "a".repeat(10001);

    // Should handle long input appropriately
    let result = api.process_stimulus(&long_stimulus);
    assert!(result.is_err()); // Should be rejected due to length limit
}

#[test]
fn test_api_invalid_priorities() {
    let mut api = SocialBrainApi::new();

    // Test boundary priority values
    assert!(api.set_goal("test", -0.01).is_err());
    assert!(api.set_goal("test", 1.01).is_err());
    assert!(api.set_goal("test", f32::NAN).is_err());
    assert!(api.set_goal("test", f32::INFINITY).is_err());
    assert!(api.set_goal("test", f32::NEG_INFINITY).is_err());
}

#[test]
fn test_api_valid_priority_boundaries() {
    let mut api = SocialBrainApi::new();

    // Valid boundary values
    assert!(api.set_goal("test", 0.0).is_ok());
    assert!(api.set_goal("test", 1.0).is_ok());
}

#[test]
fn test_api_disabled_memory() {
    let config = ApiConfig {
        memory_enabled: false,
        emotion_enabled: true,
        learning_enabled: true,
        personality_influence: 0.5,
    };
    let mut api = SocialBrainApi::with_config(config);

    // Should work even with memory disabled
    let result = api.process_stimulus("test");
    assert!(result.is_ok());
}

#[test]
fn test_api_disabled_emotion() {
    let config = ApiConfig {
        memory_enabled: true,
        emotion_enabled: false,
        learning_enabled: true,
        personality_influence: 0.5,
    };
    let mut api = SocialBrainApi::with_config(config);

    let emotion = api.current_emotion();
    // Should return neutral when disabled
    assert_eq!(emotion.pleasure, 0.0);
}

// =============================================================================
// Personality Boundary Tests
// =============================================================================

#[test]
fn test_ocean_profile_boundary_values() {
    // Test extreme but valid values
    let profile = OceanProfile::new(0.0, 0.0, 0.0, 0.0, 0.0);
    assert_eq!(profile.openness, 0.0);
    assert_eq!(profile.neuroticism, 0.0);

    let profile = OceanProfile::new(1.0, 1.0, 1.0, 1.0, 1.0);
    assert_eq!(profile.openness, 1.0);
    assert_eq!(profile.neuroticism, 1.0);
}

#[test]
fn test_ocean_profile_clamping() {
    // Values outside [0, 1] should be clamped
    let profile = OceanProfile::new(-0.5, 1.5, 2.0, -1.0, 0.5);
    assert_eq!(profile.openness, 0.0);
    assert_eq!(profile.conscientiousness, 1.0);
    assert_eq!(profile.extraversion, 1.0);
    assert_eq!(profile.agreeableness, 0.0);
    assert_eq!(profile.neuroticism, 0.5);
}

#[test]
fn test_ocean_profile_distance_identical() {
    let profile = OceanProfile::balanced();
    let distance = profile.distance(&profile);
    assert_eq!(distance, 0.0);
}

#[test]
fn test_ocean_profile_distance_opposite() {
    let p1 = OceanProfile::new(0.0, 0.0, 0.0, 0.0, 0.0);
    let p2 = OceanProfile::new(1.0, 1.0, 1.0, 1.0, 1.0);
    let distance = p1.distance(&p2);
    assert!(distance > 0.0);
    assert!(distance <= 5.0f32.sqrt()); // Maximum possible distance
}

// =============================================================================
// NEAT Boundary Tests
// =============================================================================

#[test]
fn test_genome_minimal_size() {
    // Minimal genome: 1 input, 1 output
    let genome = Genome::new_minimal(1, 1, 1);
    assert_eq!(genome.node_count(), 2);
}

#[test]
fn test_genome_zero_inputs() {
    // Edge case: zero inputs
    let genome = Genome::new_minimal(1, 0, 1);
    assert_eq!(genome.node_count(), 1); // Just output
}

#[test]
fn test_neural_network_empty_input() {
    let genome = Genome::new_minimal(1, 3, 2);
    let network = NeuralNetwork::from_genome(&genome);

    // Empty input should be handled
    let outputs = network.predict(&[]);
    assert_eq!(outputs.len(), 2);
}

#[test]
fn test_neural_network_oversized_input() {
    let genome = Genome::new_minimal(1, 3, 2);
    let network = NeuralNetwork::from_genome(&genome);

    // More inputs than expected
    let outputs = network.predict(&[0.5, 0.5, 0.5, 0.5, 0.5]);
    assert_eq!(outputs.len(), 2);
}

// =============================================================================
// Utils Validation Tests
// =============================================================================

#[test]
fn test_validate_priority_boundaries() {
    assert!(validate_priority(0.0).is_ok());
    assert!(validate_priority(1.0).is_ok());
    assert!(validate_priority(0.5).is_ok());

    assert!(validate_priority(-0.01).is_err());
    assert!(validate_priority(1.01).is_err());
    assert!(validate_priority(f32::NAN).is_err());
}

#[test]
fn test_validate_importance_boundaries() {
    assert!(validate_importance(0.0).is_ok());
    assert!(validate_importance(1.0).is_ok());

    assert!(validate_importance(f32::NAN).is_err());
    assert!(validate_importance(f32::INFINITY).is_err());
}

#[test]
fn test_validate_input_length_boundaries() {
    assert!(validate_input_length("", 10, true).is_ok());
    assert!(validate_input_length("", 10, false).is_err());

    let exact = "a".repeat(10);
    assert!(validate_input_length(&exact, 10, false).is_ok());

    let too_long = "a".repeat(11);
    assert!(validate_input_length(&too_long, 10, false).is_err());
}

#[test]
fn test_compare_f32_with_nan() {
    use std::cmp::Ordering;

    // NaN should be less than any other value
    assert_eq!(compare_f32(&f32::NAN, &1.0), Ordering::Less);
    assert_eq!(compare_f32(&1.0, &f32::NAN), Ordering::Greater);
    assert_eq!(compare_f32(&f32::NAN, &f32::NAN), Ordering::Equal);

    // Normal comparisons should work
    assert_eq!(compare_f32(&1.0, &2.0), Ordering::Less);
    assert_eq!(compare_f32(&2.0, &1.0), Ordering::Greater);
    assert_eq!(compare_f32(&1.0, &1.0), Ordering::Equal);
}

#[test]
fn test_compare_f32_with_infinity() {
    use std::cmp::Ordering;

    assert_eq!(compare_f32(&f32::INFINITY, &1.0), Ordering::Greater);
    assert_eq!(compare_f32(&f32::NEG_INFINITY, &1.0), Ordering::Less);
    assert_eq!(
        compare_f32(&f32::INFINITY, &f32::NEG_INFINITY),
        Ordering::Greater
    );
}

// =============================================================================
// Memory Query Boundary Tests
// =============================================================================

#[test]
fn test_memory_query_empty() {
    let query = MemoryQuery::new("");
    assert_eq!(query.query, "");
}

#[test]
fn test_memory_query_limit_zero() {
    let query = MemoryQuery::new("test").with_limit(0);
    assert_eq!(query.limit, 0);
}

#[test]
fn test_memory_query_time_range_reverse() {
    // End time before start time
    let query = MemoryQuery::new("test").with_time_range(1000, 100);
    assert_eq!(query.time_range, Some((1000, 100)));
    // Implementation should handle this gracefully
}

#[test]
fn test_memory_query_no_types() {
    let query = MemoryQuery::new("test").with_types(vec![]);
    assert!(query.memory_types.is_empty());
}

// =============================================================================
// Error Handling Boundary Tests
// =============================================================================

#[test]
fn test_brain_error_from_str() {
    let err: beebotos_brain::error::BrainError = "test error".into();
    assert_eq!(format!("{}", err), "Error: test error");
}

#[test]
fn test_brain_error_from_string() {
    let err: beebotos_brain::error::BrainError = "test error".to_string().into();
    assert_eq!(format!("{}", err), "Error: test error");
}

#[test]
fn test_memory_error_conversion() {
    use beebotos_brain::error::MemoryError;

    let mem_err = MemoryError::CapacityExceeded;
    let brain_err: beebotos_brain::error::BrainError = mem_err.into();

    assert!(matches!(
        brain_err,
        beebotos_brain::error::BrainError::MemoryError(_)
    ));
}

// =============================================================================
// Concurrency Boundary Tests
// =============================================================================

#[test]
fn test_api_thread_safety_compilation() {
    // This test ensures SocialBrainApi is Send + Sync
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SocialBrainApi>();
}

// =============================================================================
// Serialization Boundary Tests
// =============================================================================

#[test]
fn test_pad_serialization_roundtrip() {
    let original = Pad::new(0.5, 0.3, 0.2);
    let json = serde_json::to_string(&original).unwrap();
    let deserialized: Pad = serde_json::from_str(&json).unwrap();

    assert!((original.pleasure - deserialized.pleasure).abs() < 0.001);
    assert!((original.arousal - deserialized.arousal).abs() < 0.001);
    assert!((original.dominance - deserialized.dominance).abs() < 0.001);
}

#[test]
fn test_ocean_profile_serialization_roundtrip() {
    let original = OceanProfile::new(0.7, 0.5, 0.3, 0.8, 0.2);
    let json = serde_json::to_string(&original).unwrap();
    let deserialized: OceanProfile = serde_json::from_str(&json).unwrap();

    assert_eq!(original.openness, deserialized.openness);
    assert_eq!(original.neuroticism, deserialized.neuroticism);
}

// =============================================================================
// Configuration Boundary Tests
// =============================================================================

#[test]
fn test_api_config_boundary_values() {
    let config = ApiConfig {
        memory_enabled: true,
        emotion_enabled: true,
        learning_enabled: true,
        personality_influence: 0.0, // Minimum
    };
    let api = SocialBrainApi::with_config(config);
    assert_eq!(api.stats().active_goals, 0);

    let config = ApiConfig {
        memory_enabled: true,
        emotion_enabled: true,
        learning_enabled: true,
        personality_influence: 1.0, // Maximum
    };
    let api = SocialBrainApi::with_config(config);
    assert_eq!(api.stats().active_goals, 0);
}

#[test]
fn test_neat_config_presets() {
    let standard = NeatConfig::standard();
    let conservative = NeatConfig::conservative();
    let aggressive = NeatConfig::aggressive();

    // All presets should be valid
    assert!(standard.mutation_rate >= 0.0 && standard.mutation_rate <= 1.0);
    assert!(conservative.mutation_rate >= 0.0 && conservative.mutation_rate <= 1.0);
    assert!(aggressive.mutation_rate >= 0.0 && aggressive.mutation_rate <= 1.0);
}

// =============================================================================
// Performance Boundary Tests
// =============================================================================

#[test]
fn test_stm_many_items() {
    let mut stm = ShortTermMemory::with_capacity(9); // Max capacity

    // Fill to capacity
    for i in 0..100 {
        stm.push(format!("item {}", i));
    }

    // Should maintain capacity limit
    assert!(stm.len() <= 9);
}

#[test]
fn test_episodic_many_episodes() {
    let mut em = EpisodicMemory::new();

    // Add many episodes
    for i in 0..1000 {
        em.encode(format!("Event {}", i), i as u64 * 1000, None);
    }

    assert_eq!(em.len(), 1000);

    // Search should still work
    let results = em.search("Event 500");
    assert_eq!(results.len(), 1);
}
