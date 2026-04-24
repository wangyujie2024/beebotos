//! Episodic Memory
//!
//! Long-term memory for events with spatiotemporal context.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Episodic memory store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodicMemory {
    episodes: BTreeMap<u64, Episode>, // timestamp -> episode
    spatial_index: std::collections::HashMap<String, Vec<u64>>, // location -> timestamps
}

/// Episode (event memory)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub id: String,
    pub timestamp: u64,
    pub location: Option<Location>,
    pub what: String,
    pub who: Vec<String>,
    pub emotions: Vec<EmotionalValence>,
    pub importance: f32, // 0.0 to 1.0
    pub tags: Vec<String>,
}

/// Location
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub name: String,
    pub coordinates: Option<(f64, f64)>,
}

/// Emotional valence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionalValence {
    pub emotion: String,
    pub intensity: f32, // 0.0 to 1.0
}

impl EpisodicMemory {
    pub fn new() -> Self {
        Self {
            episodes: BTreeMap::new(),
            spatial_index: std::collections::HashMap::new(),
        }
    }

    /// Encode new episode
    pub fn encode(
        &mut self,
        what: impl Into<String>,
        when: u64,
        where_loc: Option<Location>,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();

        let episode = Episode {
            id: id.clone(),
            timestamp: when,
            location: where_loc.clone(),
            what: what.into(),
            who: vec![],
            emotions: vec![],
            importance: 0.5,
            tags: vec![],
        };

        // Index by location
        if let Some(loc) = where_loc {
            self.spatial_index.entry(loc.name).or_default().push(when);
        }

        self.episodes.insert(when, episode);
        id
    }

    /// Retrieve by time range
    pub fn query_time_range(&self, start: u64, end: u64) -> Vec<&Episode> {
        self.episodes.range(start..=end).map(|(_, e)| e).collect()
    }

    /// Retrieve by location
    pub fn query_location(&self, location: &str) -> Vec<&Episode> {
        self.spatial_index
            .get(location)
            .map(|timestamps| {
                timestamps
                    .iter()
                    .filter_map(|t| self.episodes.get(t))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Retrieve recent episodes
    pub fn recent(&self, count: usize) -> Vec<&Episode> {
        self.episodes.values().rev().take(count).collect()
    }

    /// Search by content
    pub fn search(&self, query: &str) -> Vec<&Episode> {
        let query_lower = query.to_lowercase();
        self.episodes
            .values()
            .filter(|e| {
                e.what.to_lowercase().contains(&query_lower)
                    || e.tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect()
    }

    /// Add emotional tag
    pub fn add_emotion(&mut self, timestamp: u64, emotion: &str, intensity: f32) {
        if let Some(episode) = self.episodes.get_mut(&timestamp) {
            episode.emotions.push(EmotionalValence {
                emotion: emotion.to_string(),
                intensity: intensity.clamp(0.0, 1.0),
            });
        }
    }

    /// Set importance
    pub fn set_importance(&mut self, timestamp: u64, importance: f32) {
        if let Some(episode) = self.episodes.get_mut(&timestamp) {
            episode.importance = importance.clamp(0.0, 1.0);
        }
    }

    /// Get episode count
    pub fn len(&self) -> usize {
        self.episodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.episodes.is_empty()
    }

    /// Consolidate episodes into summary (sleep mode)
    pub fn consolidate(&mut self, time_range: (u64, u64)) -> Option<Episode> {
        let episodes = self.query_time_range(time_range.0, time_range.1);

        if episodes.is_empty() {
            return None;
        }

        // Create summary episode
        let summary = format!("Consolidated memory of {} events", episodes.len());

        Some(Episode {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: time_range.1,
            location: None,
            what: summary,
            who: vec![],
            emotions: vec![],
            importance: episodes.iter().map(|e| e.importance).sum::<f32>() / episodes.len() as f32,
            tags: vec!["consolidated".to_string()],
        })
    }
}

impl Default for EpisodicMemory {
    fn default() -> Self {
        Self::new()
    }
}
