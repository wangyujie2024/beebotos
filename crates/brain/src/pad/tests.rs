//! PAD Module Tests
//!
//! Tests for PAD (Pleasure-Arousal-Dominance) emotional model.

#[cfg(test)]
mod tests {
    use super::super::*;

    // =============================================================================
    // Pad Tests
    // =============================================================================

    #[test]
    fn test_pad_creation() {
        let pad = Pad::new(0.5, 0.3, 0.2);
        assert!((pad.pleasure - 0.5).abs() < 0.001);
        assert!((pad.arousal - 0.3).abs() < 0.001);
        assert!((pad.dominance - 0.2).abs() < 0.001);
    }

    #[test]
    fn test_pad_default() {
        let pad = Pad::default();
        assert!((pad.pleasure - 0.0).abs() < 0.001);
        assert!((pad.arousal - 0.0).abs() < 0.001);
        assert!((pad.dominance - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_pad_constants() {
        // Test predefined emotional states
        assert!(Pad::JOY.pleasure > 0.0);
        assert!(Pad::JOY.arousal > 0.0);

        assert!(Pad::SADNESS.pleasure < 0.0);

        assert!(Pad::ANGER.pleasure < 0.0);
        assert!(Pad::ANGER.arousal > 0.5);

        assert!(Pad::FEAR.pleasure < 0.0);
        assert!(Pad::FEAR.arousal > 0.5);

        assert!(Pad::TRUST.pleasure > 0.0);

        assert!(Pad::ANTICIPATION.arousal > 0.0);
    }

    #[test]
    fn test_pad_distance() {
        let pad1 = Pad::new(0.0, 0.0, 0.0);
        let pad2 = Pad::new(1.0, 1.0, 1.0);

        let distance = pad1.distance(&pad2);
        assert!(distance > 0.0);

        // Distance to self should be 0
        assert!(pad1.distance(&pad1).abs() < 0.001);
    }

    #[test]
    fn test_pad_lerp() {
        let pad1 = Pad::new(0.0, 0.0, 0.0);
        let pad2 = Pad::new(1.0, 1.0, 1.0);

        let mid = pad1.lerp(&pad2, 0.5);
        assert!((mid.pleasure - 0.5).abs() < 0.001);
        assert!((mid.arousal - 0.5).abs() < 0.001);
        assert!((mid.dominance - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_pad_lerp_t0() {
        let pad1 = Pad::new(0.0, 0.0, 0.0);
        let pad2 = Pad::new(1.0, 1.0, 1.0);

        let result = pad1.lerp(&pad2, 0.0);
        assert!((result.pleasure - pad1.pleasure).abs() < 0.001);
    }

    #[test]
    fn test_pad_lerp_t1() {
        let pad1 = Pad::new(0.0, 0.0, 0.0);
        let pad2 = Pad::new(1.0, 1.0, 1.0);

        let result = pad1.lerp(&pad2, 1.0);
        assert!((result.pleasure - pad2.pleasure).abs() < 0.001);
    }

    #[test]
    fn test_pad_clamp() {
        let pad = Pad::new(2.0, -1.0, 0.5);
        let clamped = pad.clamp();

        assert!(clamped.pleasure <= 1.0);
        assert!(clamped.arousal >= 0.0);
    }

    // =============================================================================
    // EmotionalTrait Tests
    // =============================================================================

    #[test]
    fn test_emotional_trait_baseline_offsets() {
        let optimistic = EmotionalTrait::Optimistic.baseline_offset();
        assert!(
            optimistic.pleasure > 0.0,
            "Optimistic pleasure should be > 0, got {}",
            optimistic.pleasure
        );

        let pessimistic = EmotionalTrait::Pessimistic.baseline_offset();
        assert!(
            pessimistic.pleasure < 0.0,
            "Pessimistic pleasure should be < 0, got {}",
            pessimistic.pleasure
        );

        let high_energy = EmotionalTrait::HighEnergy.baseline_offset();
        assert!(
            high_energy.arousal > 0.5,
            "HighEnergy arousal should be > 0.5, got {}",
            high_energy.arousal
        );

        let low_energy = EmotionalTrait::LowEnergy.baseline_offset();
        assert!(
            low_energy.arousal < 0.5,
            "LowEnergy arousal should be < 0.5, got {}",
            low_energy.arousal
        );

        let assertive = EmotionalTrait::Assertive.baseline_offset();
        assert!(
            assertive.dominance > 0.5,
            "Assertive dominance should be > 0.5, got {}",
            assertive.dominance
        );

        let passive = EmotionalTrait::Passive.baseline_offset();
        assert!(
            passive.dominance < 0.5,
            "Passive dominance should be < 0.5, got {}",
            passive.dominance
        );
    }

    // =============================================================================
    // EmotionCategory Tests
    // =============================================================================

    #[test]
    fn test_emotion_category_pad_centers() {
        let joy = EmotionCategory::Joy.pad_center();
        assert!(joy.pleasure > 0.0);

        let sadness = EmotionCategory::Sadness.pad_center();
        assert!(sadness.pleasure < 0.0);

        let anger = EmotionCategory::Anger.pad_center();
        assert!(anger.pleasure < 0.0);
        assert!(anger.arousal > 0.0);

        let fear = EmotionCategory::Fear.pad_center();
        assert!(fear.pleasure < 0.0);
        assert!(fear.arousal > 0.0);
    }

    #[test]
    fn test_emotion_category_opposites() {
        assert_eq!(EmotionCategory::Joy.opposite(), EmotionCategory::Sadness);
        assert_eq!(EmotionCategory::Sadness.opposite(), EmotionCategory::Joy);
        assert_eq!(EmotionCategory::Trust.opposite(), EmotionCategory::Disgust);
        assert_eq!(EmotionCategory::Disgust.opposite(), EmotionCategory::Trust);
        assert_eq!(EmotionCategory::Fear.opposite(), EmotionCategory::Anger);
        assert_eq!(EmotionCategory::Anger.opposite(), EmotionCategory::Fear);
        assert_eq!(
            EmotionCategory::Anticipation.opposite(),
            EmotionCategory::Surprise
        );
        assert_eq!(
            EmotionCategory::Surprise.opposite(),
            EmotionCategory::Anticipation
        );
    }

    #[test]
    fn test_emotion_category_opposite_is_reflexive() {
        let categories = [
            EmotionCategory::Joy,
            EmotionCategory::Trust,
            EmotionCategory::Fear,
            EmotionCategory::Surprise,
            EmotionCategory::Sadness,
            EmotionCategory::Disgust,
            EmotionCategory::Anger,
            EmotionCategory::Anticipation,
        ];

        for category in &categories {
            let opposite = category.opposite();
            let opposite_of_opposite = opposite.opposite();
            assert_eq!(*category, opposite_of_opposite);
        }
    }

    // =============================================================================
    // EmotionalIntelligence Tests
    // =============================================================================

    #[test]
    fn test_emotional_intelligence_creation() {
        let ei = EmotionalIntelligence::new();
        let current = ei.current();
        // Default should be near neutral
        assert!(current.pleasure.abs() < 0.1);
    }

    #[test]
    fn test_emotional_intelligence_update() {
        let mut ei = EmotionalIntelligence::new();

        let event = EmotionalEvent {
            description: "Good news".to_string(),
            pleasure_impact: 0.5,
            arousal_impact: 0.3,
            dominance_impact: 0.1,
        };

        ei.update(&event);

        let current = ei.current();
        assert!(current.pleasure > 0.0);
    }

    #[test]
    fn test_emotional_intelligence_multiple_updates() {
        let mut ei = EmotionalIntelligence::new();

        // Multiple positive events
        for _ in 0..5 {
            ei.update(&EmotionalEvent {
                description: "Good".to_string(),
                pleasure_impact: 0.2,
                arousal_impact: 0.1,
                dominance_impact: 0.0,
            });
        }

        let current = ei.current();
        assert!(current.pleasure > 0.0);
    }

    // =============================================================================
    // BasicEmotion Tests
    // =============================================================================

    #[test]
    fn test_basic_emotion_pad_mapping() {
        let happy = BasicEmotion::Happy;
        let pad = happy.to_pad();
        assert!(pad.pleasure > 0.0);

        let sad = BasicEmotion::Sad;
        let pad = sad.to_pad();
        assert!(pad.pleasure < 0.0);

        let angry = BasicEmotion::Angry;
        let pad = angry.to_pad();
        assert!(pad.pleasure < 0.0);
        assert!(pad.arousal > 0.0);

        let afraid = BasicEmotion::Afraid;
        let pad = afraid.to_pad();
        assert!(pad.pleasure < 0.0);
        assert!(pad.arousal > 0.0);
    }

    #[test]
    fn test_basic_emotion_display() {
        assert_eq!(format!("{}", BasicEmotion::Happy), "Happy");
        assert_eq!(format!("{}", BasicEmotion::Sad), "Sad");
        assert_eq!(format!("{}", BasicEmotion::Angry), "Angry");
    }

    // =============================================================================
    // Emotion Tests
    // =============================================================================

    #[test]
    fn test_emotion_creation() {
        let emotion = Emotion::new("Joy", 0.8);
        assert!(matches!(emotion, Emotion::Joy));
    }

    // =============================================================================
    // EmotionalEvent Tests
    // =============================================================================

    #[test]
    fn test_emotional_event_creation() {
        let event = EmotionalEvent {
            description: "Test event".to_string(),
            pleasure_impact: 0.5,
            arousal_impact: 0.3,
            dominance_impact: 0.2,
        };

        assert_eq!(event.description, "Test event");
        assert!((event.pleasure_impact - 0.5).abs() < 0.001);
    }

    // =============================================================================
    // Boundary Tests
    // =============================================================================

    #[test]
    fn test_pad_extreme_values() {
        let max_pad = Pad::new(1.0, 1.0, 1.0);
        assert!((max_pad.pleasure - 1.0).abs() < 0.001);
        assert!((max_pad.arousal - 1.0).abs() < 0.001);
        assert!((max_pad.dominance - 1.0).abs() < 0.001);

        let min_pad = Pad::new(-1.0, 0.0, 0.0);
        assert!((min_pad.pleasure - (-1.0)).abs() < 0.001);
        assert!(min_pad.arousal.abs() < 0.001);
    }

    #[test]
    fn test_pad_zero_values() {
        let zero_pad = Pad::new(0.0, 0.0, 0.0);
        assert!(zero_pad.pleasure.abs() < 0.001);
        assert!(zero_pad.arousal.abs() < 0.001);
        assert!(zero_pad.dominance.abs() < 0.001);
    }
}
