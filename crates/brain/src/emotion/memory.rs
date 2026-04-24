//! Emotion Memory
//!
//! Associates emotions with memories for retrieval.

#![allow(dead_code)]

use std::collections::HashMap;

use uuid::Uuid;

use super::state::{EmotionState, EmotionType};

/// Emotional memory entry
#[derive(Debug, Clone)]
pub struct EmotionalMemory {
    pub memory_id: Uuid,
    pub emotion: EmotionState,
    pub emotion_type: EmotionType,
    pub intensity: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Emotion-based memory index
pub struct EmotionMemory {
    memories: HashMap<Uuid, EmotionalMemory>,
    by_emotion: HashMap<EmotionType, Vec<Uuid>>,
}

impl EmotionMemory {
    pub fn new() -> Self {
        Self {
            memories: HashMap::new(),
            by_emotion: HashMap::new(),
        }
    }

    /// Store memory with emotion
    pub fn store(&mut self, memory_id: Uuid, emotion: EmotionState, intensity: f64) {
        use super::computing::EmotionComputing;

        let emotion_type = EmotionComputing::nearest_named(&emotion);

        let entry = EmotionalMemory {
            memory_id,
            emotion,
            emotion_type,
            intensity,
            timestamp: chrono::Utc::now(),
        };

        self.memories.insert(memory_id, entry);
        self.by_emotion
            .entry(emotion_type)
            .or_default()
            .push(memory_id);
    }

    /// Retrieve memories by emotion type
    pub fn get_by_emotion(&self, emotion_type: EmotionType) -> Vec<&EmotionalMemory> {
        self.by_emotion
            .get(&emotion_type)
            .map(|ids| ids.iter().filter_map(|id| self.memories.get(id)).collect())
            .unwrap_or_default()
    }

    /// Find memories with similar emotion
    pub fn find_similar(&self, target: &EmotionState, threshold: f64) -> Vec<&EmotionalMemory> {
        self.memories
            .values()
            .filter(|m| m.emotion.distance(target) < threshold)
            .collect()
    }
}

impl Default for EmotionMemory {
    fn default() -> Self {
        Self::new()
    }
}
