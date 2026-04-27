//! PAD Module - Core Emotional State Model
//!
//! This module provides the **PAD (Pleasure-Arousal-Dominance)** emotional
//! model, a dimensional approach to representing emotional states in 3D space.
//!
//! ## Purpose
//! - Low-level emotional state representation
//! - Mathematical operations on emotions (blending, interpolation, distance)
//! - Conversion between PAD space and categorical emotions
//!
//! ## When to use
//! Use this module when you need:
//! - Direct manipulation of emotional dimensions
//! - Numerical operations on emotional states
//! - A compact representation for neural network inputs
//!
//! ## Relation to `emotion` module
//! The `emotion` module builds on top of PAD and adds higher-level concepts
//! like:
//! - Emotion dynamics and decay over time
//! - Emotional contagion between agents
//! - Emotion-based memory tagging
//!
//! For most use cases, prefer the `SocialBrainApi` which combines both.

pub mod emotion;
pub mod transition;

pub use emotion::{
    BasicEmotion, Emotion, EmotionCategory, EmotionalEvent, EmotionalIntelligence, EmotionalTrait,
    Pad,
};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
