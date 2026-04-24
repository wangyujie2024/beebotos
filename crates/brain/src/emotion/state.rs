//! Emotion State
//!
//! PAD (Pleasure-Arousal-Dominance) emotional state representation.
//!
//! **注意**: EmotionState 现在基于 Pad 类型实现，以统一情感模型。
//! 为了保持向后兼容，EmotionState 仍然是独立类型，但内部使用 Pad。

use serde::{Deserialize, Serialize};

use crate::pad::Pad;

/// PAD emotional state (基于 Pad 的包装类型)
///
/// 为了向后兼容，保持 f64 API，但内部使用 Pad (f32)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EmotionState {
    /// Pleasure dimension (-1.0 to 1.0)
    pub pleasure: f64,
    /// Arousal dimension (-1.0 to 1.0), 内部存储映射到 [0, 1]
    pub arousal: f64,
    /// Dominance dimension (-1.0 to 1.0), 内部存储映射到 [0, 1]
    pub dominance: f64,
}

impl EmotionState {
    /// Create new emotion state
    pub fn new(pleasure: f64, arousal: f64, dominance: f64) -> Self {
        Self {
            pleasure: pleasure.clamp(-1.0, 1.0),
            arousal: arousal.clamp(-1.0, 1.0),
            dominance: dominance.clamp(-1.0, 1.0),
        }
    }

    /// Create from Pad (内部转换)
    pub fn from_pad(pad: Pad) -> Self {
        Self {
            pleasure: pad.pleasure as f64,
            arousal: (pad.arousal as f64 - 0.5) * 2.0, // [0,1] -> [-1,1]
            dominance: (pad.dominance as f64 - 0.5) * 2.0, // [0,1] -> [-1,1]
        }
    }

    /// Convert to Pad (内部转换)
    pub fn to_pad(&self) -> Pad {
        Pad::new(
            self.pleasure as f32,
            (self.arousal / 2.0 + 0.5) as f32,   // [-1,1] -> [0,1]
            (self.dominance / 2.0 + 0.5) as f32, // [-1,1] -> [0,1]
        )
    }

    /// Neutral emotion
    pub fn neutral() -> Self {
        Self::new(0.0, 0.0, 0.0)
    }

    /// Happy emotion
    pub fn happy() -> Self {
        Self::new(0.8, 0.4, 0.2)
    }

    /// Sad emotion
    pub fn sad() -> Self {
        Self::new(-0.8, -0.4, -0.4)
    }

    /// Angry emotion
    pub fn angry() -> Self {
        Self::new(-0.6, 0.6, 0.6)
    }

    /// Afraid emotion
    pub fn afraid() -> Self {
        Self::new(-0.7, 0.7, -0.6)
    }

    /// Linear interpolation between two emotions
    pub fn lerp(&self, target: &EmotionState, t: f64) -> EmotionState {
        let t = t.clamp(0.0, 1.0);
        EmotionState::new(
            self.pleasure + (target.pleasure - self.pleasure) * t,
            self.arousal + (target.arousal - self.arousal) * t,
            self.dominance + (target.dominance - self.dominance) * t,
        )
    }

    /// Distance to another emotion
    pub fn distance(&self, other: &EmotionState) -> f64 {
        ((self.pleasure - other.pleasure).powi(2)
            + (self.arousal - other.arousal).powi(2)
            + (self.dominance - other.dominance).powi(2))
        .sqrt()
    }

    /// Convert to color for visualization
    pub fn to_color(&self) -> (u8, u8, u8) {
        let r = ((self.pleasure + 1.0) / 2.0 * 255.0) as u8;
        let g = ((self.arousal + 1.0) / 2.0 * 255.0) as u8;
        let b = ((self.dominance + 1.0) / 2.0 * 255.0) as u8;
        (r, g, b)
    }
}

impl From<Pad> for EmotionState {
    fn from(pad: Pad) -> Self {
        Self::from_pad(pad)
    }
}

impl From<EmotionState> for Pad {
    fn from(state: EmotionState) -> Self {
        state.to_pad()
    }
}

/// Named emotion types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EmotionType {
    Neutral,
    Happy,
    Sad,
    Angry,
    Afraid,
    Disgusted,
    Surprised,
    Bored,
    Excited,
    Content,
}

impl EmotionType {
    /// Convert to PAD state
    pub fn to_state(&self) -> EmotionState {
        match self {
            EmotionType::Neutral => EmotionState::neutral(),
            EmotionType::Happy => EmotionState::happy(),
            EmotionType::Sad => EmotionState::sad(),
            EmotionType::Angry => EmotionState::angry(),
            EmotionType::Afraid => EmotionState::afraid(),
            EmotionType::Disgusted => EmotionState::new(-0.6, 0.2, 0.1),
            EmotionType::Surprised => EmotionState::new(0.2, 0.9, 0.0),
            EmotionType::Bored => EmotionState::new(-0.3, -0.6, -0.4),
            EmotionType::Excited => EmotionState::new(0.6, 0.9, 0.3),
            EmotionType::Content => EmotionState::new(0.6, -0.2, 0.3),
        }
    }

    /// Convert from BasicEmotion
    pub fn from_basic(emotion: crate::pad::BasicEmotion) -> Self {
        use crate::pad::BasicEmotion as Be;
        match emotion {
            Be::Happy | Be::Delighted | Be::Excited | Be::Content => Self::Happy,
            Be::Sad | Be::Depressed | Be::Distressed => Self::Sad,
            Be::Angry => Self::Angry,
            Be::Afraid | Be::Anxious => Self::Afraid,
            Be::Surprised => Self::Surprised,
            Be::Disgusted => Self::Disgusted,
            Be::Bored | Be::Relaxed | Be::Serene | Be::Confident => Self::Neutral,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emotion_creation() {
        let emotion = EmotionState::new(0.5, 0.3, 0.2);
        assert_eq!(emotion.pleasure, 0.5);
    }

    #[test]
    fn test_emotion_lerp() {
        let e1 = EmotionState::neutral();
        let e2 = EmotionState::happy();
        let mid = e1.lerp(&e2, 0.5);
        assert!(mid.pleasure > 0.0);
    }

    #[test]
    fn test_emotion_clamping() {
        let emotion = EmotionState::new(2.0, -2.0, 1.5);
        assert_eq!(emotion.pleasure, 1.0);
        assert_eq!(emotion.arousal, -1.0);
        assert_eq!(emotion.dominance, 1.0);
    }

    #[test]
    fn test_pad_conversion() {
        let pad = Pad::new(0.5, 0.8, 0.3);
        let emotion = EmotionState::from_pad(pad);
        let back_to_pad = emotion.to_pad();

        assert!((pad.pleasure - back_to_pad.pleasure).abs() < 0.01);
    }

    #[test]
    fn test_from_into_traits() {
        let pad = Pad::neutral();
        let emotion: EmotionState = pad.into();
        let back: Pad = emotion.into();

        assert!((pad.pleasure - back.pleasure).abs() < 0.01);
    }
}
