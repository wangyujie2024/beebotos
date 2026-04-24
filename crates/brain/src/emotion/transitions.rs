//! Emotion Transitions
//!
//! Manages smooth transitions between emotional states.

#![allow(dead_code)]

use std::time::Duration;

use super::state::EmotionState;

/// Transition configuration
#[derive(Debug, Clone)]
pub struct TransitionConfig {
    pub duration: Duration,
    pub easing: EasingFunction,
}

impl Default for TransitionConfig {
    fn default() -> Self {
        Self {
            duration: Duration::from_millis(500),
            easing: EasingFunction::EaseInOut,
        }
    }
}

/// Easing functions for transitions
#[derive(Debug, Clone, Copy)]
pub enum EasingFunction {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
}

impl EasingFunction {
    /// Apply easing to t (0.0 to 1.0)
    pub fn apply(&self, t: f64) -> f64 {
        match self {
            EasingFunction::Linear => t,
            EasingFunction::EaseIn => t * t,
            EasingFunction::EaseOut => 1.0 - (1.0 - t).powi(2),
            EasingFunction::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }
        }
    }
}

/// Active transition
#[derive(Debug)]
pub struct ActiveTransition {
    from: EmotionState,
    to: EmotionState,
    config: TransitionConfig,
    progress: Duration,
}

impl ActiveTransition {
    pub fn new(from: EmotionState, to: EmotionState, config: TransitionConfig) -> Self {
        Self {
            from,
            to,
            config,
            progress: Duration::ZERO,
        }
    }

    /// Update transition progress
    pub fn update(&mut self, dt: Duration) -> Option<EmotionState> {
        self.progress += dt;

        let total_secs = self.config.duration.as_secs_f64();
        let progress_secs = self.progress.as_secs_f64();

        if progress_secs >= total_secs {
            return None; // Transition complete
        }

        let t = (progress_secs / total_secs).clamp(0.0, 1.0);
        let eased_t = self.config.easing.apply(t);

        Some(self.from.lerp(&self.to, eased_t))
    }

    /// Get current state
    pub fn current(&self) -> EmotionState {
        let total_secs = self.config.duration.as_secs_f64();
        let progress_secs = self.progress.as_secs_f64();
        let t = (progress_secs / total_secs).clamp(0.0, 1.0);
        let eased_t = self.config.easing.apply(t);

        self.from.lerp(&self.to, eased_t)
    }
}
