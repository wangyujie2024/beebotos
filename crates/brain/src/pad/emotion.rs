//! PAD Emotional Model
//!
//! Pleasure-Arousal-Dominance 3D emotional state.

use std::fmt;

use serde::{Deserialize, Serialize};

/// PAD emotional state
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Pad {
    pub pleasure: f32,  // -1.0 (unpleasant) to +1.0 (pleasant)
    pub arousal: f32,   // 0.0 (calm) to 1.0 (excited)
    pub dominance: f32, // 0.0 (submissive) to 1.0 (dominant)
}

impl Default for Pad {
    fn default() -> Self {
        Self {
            pleasure: 0.0,
            arousal: 0.0,
            dominance: 0.5,
        }
    }
}

/// 16 basic emotions based on PAD combinations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BasicEmotion {
    Excited,    // (+P, +A, +D)
    Delighted,  // (+P, +A, -D)
    Happy,      // (+P, -A, +D)
    Content,    // (+P, -A, -D)
    Surprised,  // (0P, +A, 0D)
    Angry,      // (-P, +A, +D)
    Afraid,     // (-P, +A, -D)
    Distressed, // (-P, -A, -D)
    Sad,        // (-P, -A, +D)
    Bored,      // (0P, -A, 0D)
    Relaxed,    // (+P, -A, 0D)
    Depressed,  // (-P, -A, 0D)
    Disgusted,  // (-P, 0A, +D)
    Anxious,    // (-P, +A, 0D)
    Serene,     // (+P, 0A, -D)
    Confident,  // (+P, 0A, +D)
}

impl Pad {
    /// Neutral state
    pub const NEUTRAL: Self = Self {
        pleasure: 0.0,
        arousal: 0.5,
        dominance: 0.5,
    };

    /// Maximum pleasure
    pub const JOY: Self = Self {
        pleasure: 1.0,
        arousal: 0.5,
        dominance: 0.5,
    };

    /// Fear
    pub const FEAR: Self = Self {
        pleasure: -0.7,
        arousal: 0.8,
        dominance: 0.2,
    };

    /// Anger
    pub const ANGER: Self = Self {
        pleasure: -0.5,
        arousal: 0.8,
        dominance: 0.7,
    };

    /// Sadness
    pub const SADNESS: Self = Self {
        pleasure: -0.7,
        arousal: 0.2,
        dominance: 0.3,
    };

    /// Trust
    pub const TRUST: Self = Self {
        pleasure: 0.7,
        arousal: 0.4,
        dominance: 0.5,
    };

    /// Anticipation
    pub const ANTICIPATION: Self = Self {
        pleasure: 0.4,
        arousal: 0.7,
        dominance: 0.5,
    };

    /// Create neutral state
    pub fn neutral() -> Self {
        Self::NEUTRAL
    }

    /// Create new PAD state
    pub fn new(pleasure: f32, arousal: f32, dominance: f32) -> Self {
        Self {
            pleasure: pleasure.clamp(-1.0, 1.0),
            arousal: arousal.clamp(0.0, 1.0),
            dominance: dominance.clamp(0.0, 1.0),
        }
    }

    /// Get intensity (magnitude)
    pub fn intensity(&self) -> f32 {
        (self.pleasure.powi(2) + self.arousal.powi(2) + self.dominance.powi(2)).sqrt()
            / 3.0_f32.sqrt()
    }

    /// Check if positive
    pub fn is_positive(&self) -> bool {
        self.pleasure > 0.0
    }

    /// Check if negative
    pub fn is_negative(&self) -> bool {
        self.pleasure < 0.0
    }

    /// Check if high arousal
    pub fn is_aroused(&self) -> bool {
        self.arousal > 0.7
    }

    /// Check if low arousal
    pub fn is_calm(&self) -> bool {
        self.arousal < 0.3
    }

    /// Check if dominant
    pub fn is_dominant(&self) -> bool {
        self.dominance > 0.7
    }

    /// Check if submissive
    pub fn is_submissive(&self) -> bool {
        self.dominance < 0.3
    }

    /// Blend with another PAD state
    pub fn blend(&self, other: &Self, factor: f32) -> Self {
        Self::new(
            self.pleasure * (1.0 - factor) + other.pleasure * factor,
            self.arousal * (1.0 - factor) + other.arousal * factor,
            self.dominance * (1.0 - factor) + other.dominance * factor,
        )
    }

    /// Move toward neutral
    pub fn toward_neutral(&self, factor: f32) -> Self {
        self.blend(&Self::NEUTRAL, factor)
    }

    /// Create from basic emotion
    pub fn from_basic_emotion(emotion: BasicEmotion) -> Self {
        match emotion {
            BasicEmotion::Excited => Self {
                pleasure: 0.8,
                arousal: 0.9,
                dominance: 0.8,
            },
            BasicEmotion::Delighted => Self {
                pleasure: 0.8,
                arousal: 0.9,
                dominance: 0.2,
            },
            BasicEmotion::Happy => Self {
                pleasure: 0.8,
                arousal: 0.3,
                dominance: 0.8,
            },
            BasicEmotion::Content => Self {
                pleasure: 0.6,
                arousal: 0.2,
                dominance: 0.3,
            },
            BasicEmotion::Surprised => Self {
                pleasure: 0.0,
                arousal: 0.9,
                dominance: 0.5,
            },
            BasicEmotion::Angry => Self {
                pleasure: -0.8,
                arousal: 0.9,
                dominance: 0.8,
            },
            BasicEmotion::Afraid => Self {
                pleasure: -0.8,
                arousal: 0.9,
                dominance: 0.2,
            },
            BasicEmotion::Distressed => Self {
                pleasure: -0.8,
                arousal: 0.3,
                dominance: 0.2,
            },
            BasicEmotion::Sad => Self {
                pleasure: -0.8,
                arousal: 0.2,
                dominance: 0.8,
            },
            BasicEmotion::Bored => Self {
                pleasure: 0.0,
                arousal: 0.1,
                dominance: 0.5,
            },
            BasicEmotion::Relaxed => Self {
                pleasure: 0.6,
                arousal: 0.2,
                dominance: 0.5,
            },
            BasicEmotion::Depressed => Self {
                pleasure: -0.8,
                arousal: 0.1,
                dominance: 0.5,
            },
            BasicEmotion::Disgusted => Self {
                pleasure: -0.6,
                arousal: 0.5,
                dominance: 0.8,
            },
            BasicEmotion::Anxious => Self {
                pleasure: -0.6,
                arousal: 0.8,
                dominance: 0.5,
            },
            BasicEmotion::Serene => Self {
                pleasure: 0.6,
                arousal: 0.5,
                dominance: 0.2,
            },
            BasicEmotion::Confident => Self {
                pleasure: 0.6,
                arousal: 0.5,
                dominance: 0.8,
            },
        }
    }

    /// Convert to basic emotion (nearest match)
    pub fn to_basic_emotion(self) -> BasicEmotion {
        let emotions = [
            BasicEmotion::Excited,
            BasicEmotion::Delighted,
            BasicEmotion::Happy,
            BasicEmotion::Content,
            BasicEmotion::Surprised,
            BasicEmotion::Angry,
            BasicEmotion::Afraid,
            BasicEmotion::Distressed,
            BasicEmotion::Sad,
            BasicEmotion::Bored,
            BasicEmotion::Relaxed,
            BasicEmotion::Depressed,
            BasicEmotion::Disgusted,
            BasicEmotion::Anxious,
            BasicEmotion::Serene,
            BasicEmotion::Confident,
        ];

        emotions
            .iter()
            .min_by(|&&a, &&b| {
                let pad_a = Pad::from_basic_emotion(a);
                let pad_b = Pad::from_basic_emotion(b);
                let dist_a = self.distance(&pad_a);
                let dist_b = self.distance(&pad_b);
                crate::utils::compare_f32(&dist_a, &dist_b)
            })
            .copied()
            .unwrap_or(BasicEmotion::Content)
    }

    /// Euclidean distance to another PAD state
    pub fn distance(&self, other: &Pad) -> f32 {
        ((self.pleasure - other.pleasure).powi(2)
            + (self.arousal - other.arousal).powi(2)
            + (self.dominance - other.dominance).powi(2))
        .sqrt()
    }

    /// Decay to baseline
    pub fn decay(&mut self, baseline: &Pad, rate: f32) {
        self.pleasure = self.pleasure * (1.0 - rate) + baseline.pleasure * rate;
        self.arousal = self.arousal * (1.0 - rate) + baseline.arousal * rate;
        self.dominance = self.dominance * (1.0 - rate) + baseline.dominance * rate;
    }

    /// Convert to named emotion
    pub fn to_emotion(self) -> Emotion {
        Emotion::from_pad(self)
    }

    /// Influence decision (risk adjustment)
    pub fn risk_bias(&self) -> f32 {
        // High arousal + negative pleasure = risk averse
        // High arousal + positive pleasure = risk seeking
        self.arousal * self.pleasure
    }

    /// Influence memory consolidation
    pub fn memory_enhancement(&self) -> f32 {
        // High arousal enhances memory
        self.arousal
    }

    /// Linear interpolation between two PAD states
    pub fn lerp(&self, other: &Self, t: f32) -> Self {
        Self::new(
            self.pleasure * (1.0 - t) + other.pleasure * t,
            self.arousal * (1.0 - t) + other.arousal * t,
            self.dominance * (1.0 - t) + other.dominance * t,
        )
    }

    /// Clamp values to valid ranges
    pub fn clamp(&self) -> Self {
        Self::new(self.pleasure, self.arousal, self.dominance)
    }
}

/// Emotional intelligence
pub struct EmotionalIntelligence {
    current: Pad,
    baseline: Pad,
    history: Vec<(u64, Pad)>,
    decay_rate: f32,
}

impl EmotionalIntelligence {
    pub fn new() -> Self {
        let neutral = Pad::neutral();
        Self {
            current: neutral,
            baseline: neutral,
            history: vec![],
            decay_rate: 0.05,
        }
    }

    pub fn current(&self) -> &Pad {
        &self.current
    }

    pub fn update(&mut self, event: &EmotionalEvent) {
        // Record history
        self.history.push((Self::now(), self.current));

        // Update current state
        self.current.pleasure = (self.current.pleasure + event.pleasure_impact).clamp(-1.0, 1.0);
        self.current.arousal = (self.current.arousal + event.arousal_impact).clamp(0.0, 1.0);
        self.current.dominance = (self.current.dominance + event.dominance_impact).clamp(0.0, 1.0);
    }

    pub fn tick(&mut self) {
        // Decay toward baseline
        self.current.decay(&self.baseline, self.decay_rate);
    }

    pub fn empathize(&mut self, other: &Pad) {
        // Partial emotional contagion
        let contagion_factor = 0.3;
        self.current.pleasure =
            self.current.pleasure * (1.0 - contagion_factor) + other.pleasure * contagion_factor;
        self.current.arousal =
            self.current.arousal * (1.0 - contagion_factor) + other.arousal * contagion_factor;
    }

    fn now() -> u64 {
        crate::utils::current_timestamp_secs()
    }
}

impl Default for EmotionalIntelligence {
    fn default() -> Self {
        Self::new()
    }
}

/// Emotion type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Emotion {
    Happy,
    Sad,
    Angry,
    Afraid,
    Surprised,
    Disgusted,
    Neutral,
    Joy,
    Distress,
    Surprise,
    Disgust,
    Fear,
    Trust,
    Sadness,
}

impl Emotion {
    /// Create a new Emotion with name and intensity
    pub fn new(name: &str, _intensity: f32) -> Self {
        // Map name to emotion type, default to Neutral if unknown
        match name.to_lowercase().as_str() {
            "happy" | "happiness" => Emotion::Happy,
            "sad" | "sadness" => Emotion::Sad,
            "angry" | "anger" => Emotion::Angry,
            "afraid" | "fear" => Emotion::Afraid,
            "surprised" | "surprise" => Emotion::Surprised,
            "disgusted" | "disgust" => Emotion::Disgusted,
            "joy" => Emotion::Joy,
            "distress" => Emotion::Distress,
            "trust" => Emotion::Trust,
            _ => Emotion::Neutral,
        }
    }

    /// Convert from PAD state
    pub fn from_pad(pad: Pad) -> Self {
        pad.to_basic_emotion().into()
    }
}

impl BasicEmotion {
    /// Convert to PAD representation
    pub fn to_pad(self) -> Pad {
        Pad::from_basic_emotion(self)
    }
}

impl fmt::Display for BasicEmotion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl From<BasicEmotion> for Emotion {
    fn from(be: BasicEmotion) -> Self {
        match be {
            BasicEmotion::Happy
            | BasicEmotion::Delighted
            | BasicEmotion::Excited
            | BasicEmotion::Content => Emotion::Happy,
            BasicEmotion::Sad | BasicEmotion::Depressed => Emotion::Sad,
            BasicEmotion::Angry => Emotion::Angry,
            BasicEmotion::Afraid | BasicEmotion::Anxious => Emotion::Afraid,
            BasicEmotion::Surprised => Emotion::Surprised,
            BasicEmotion::Disgusted => Emotion::Disgusted,
            BasicEmotion::Distressed => Emotion::Distress,
            BasicEmotion::Bored
            | BasicEmotion::Relaxed
            | BasicEmotion::Serene
            | BasicEmotion::Confident => Emotion::Neutral,
        }
    }
}

impl std::ops::Add for Pad {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self::new(
            self.pleasure + other.pleasure,
            self.arousal + other.arousal,
            self.dominance + other.dominance,
        )
    }
}

impl std::ops::Mul<f32> for Pad {
    type Output = Self;

    fn mul(self, scalar: f32) -> Self {
        Self::new(
            self.pleasure * scalar,
            self.arousal * scalar,
            self.dominance * scalar,
        )
    }
}

/// Emotional trait for personality
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmotionalTrait {
    Optimistic,
    Pessimistic,
    HighEnergy,
    LowEnergy,
    Assertive,
    Passive,
}

impl EmotionalTrait {
    /// Get baseline PAD offset for this trait
    pub fn baseline_offset(&self) -> Pad {
        match self {
            EmotionalTrait::Optimistic => Pad::new(0.3, 0.5, 0.5),
            EmotionalTrait::Pessimistic => Pad::new(-0.3, 0.5, 0.5),
            EmotionalTrait::HighEnergy => Pad::new(0.0, 0.8, 0.5),
            EmotionalTrait::LowEnergy => Pad::new(0.0, 0.2, 0.5),
            EmotionalTrait::Assertive => Pad::new(0.0, 0.5, 0.8),
            EmotionalTrait::Passive => Pad::new(0.0, 0.5, 0.2),
        }
    }
}

/// Emotion category for organizing emotions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmotionCategory {
    Joy,
    Trust,
    Fear,
    Surprise,
    Sadness,
    Disgust,
    Anger,
    Anticipation,
}

impl EmotionCategory {
    /// Get the PAD center for this emotion category
    pub fn pad_center(&self) -> Pad {
        match self {
            EmotionCategory::Joy => Pad::new(0.8, 0.3, 0.5),
            EmotionCategory::Trust => Pad::new(0.7, 0.4, 0.5),
            EmotionCategory::Fear => Pad::new(-0.7, 0.8, 0.2),
            EmotionCategory::Surprise => Pad::new(0.0, 0.9, 0.5),
            EmotionCategory::Sadness => Pad::new(-0.8, 0.2, 0.3),
            EmotionCategory::Disgust => Pad::new(-0.6, 0.5, 0.7),
            EmotionCategory::Anger => Pad::new(-0.5, 0.8, 0.7),
            EmotionCategory::Anticipation => Pad::new(0.4, 0.7, 0.5),
        }
    }

    /// Get the opposite emotion category
    pub fn opposite(&self) -> Self {
        match self {
            EmotionCategory::Joy => EmotionCategory::Sadness,
            EmotionCategory::Trust => EmotionCategory::Disgust,
            EmotionCategory::Fear => EmotionCategory::Anger,
            EmotionCategory::Surprise => EmotionCategory::Anticipation,
            EmotionCategory::Sadness => EmotionCategory::Joy,
            EmotionCategory::Disgust => EmotionCategory::Trust,
            EmotionCategory::Anger => EmotionCategory::Fear,
            EmotionCategory::Anticipation => EmotionCategory::Surprise,
        }
    }
}

/// Emotional event
#[derive(Debug, Clone)]
pub struct EmotionalEvent {
    pub description: String,
    pub pleasure_impact: f32,
    pub arousal_impact: f32,
    pub dominance_impact: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pad_neutral() {
        let pad = Pad::neutral();
        assert_eq!(pad.pleasure, 0.0);
        assert_eq!(pad.arousal, 0.5);
        assert_eq!(pad.dominance, 0.5);
    }

    #[test]
    fn test_basic_emotion_conversion() {
        let happy = Pad::from_basic_emotion(BasicEmotion::Happy);
        assert!(happy.pleasure > 0.0);

        let roundtrip = happy.to_basic_emotion();
        assert!(matches!(
            roundtrip,
            BasicEmotion::Happy | BasicEmotion::Content
        ));
    }

    #[test]
    fn test_decay() {
        let mut pad = Pad::from_basic_emotion(BasicEmotion::Angry);
        let baseline = Pad::neutral();

        pad.decay(&baseline, 0.5);

        assert!(pad.pleasure > -0.8); // Moved toward neutral
        assert!(pad.pleasure < 0.0); // Still negative
    }
}
