//! Reputation System
//!
//! Tracks and manages agent reputations.

#![allow(dead_code)]

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Reputation score for an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reputation {
    pub agent_id: String,
    pub score: f32, // 0.0 to 1.0
    pub review_count: u32,
    pub history: Vec<ReputationEvent>,
}

/// Reputation change event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationEvent {
    pub delta: f32,
    pub reason: String,
    pub timestamp: u64,
}

/// Reputation manager
pub struct ReputationManager {
    reputations: HashMap<String, Reputation>,
}

impl ReputationManager {
    pub fn new() -> Self {
        Self {
            reputations: HashMap::new(),
        }
    }

    /// Get or create reputation for an agent
    pub fn get_reputation(&mut self, agent_id: &str) -> &mut Reputation {
        self.reputations
            .entry(agent_id.to_string())
            .or_insert(Reputation {
                agent_id: agent_id.to_string(),
                score: 0.5, // Neutral starting score
                review_count: 0,
                history: vec![],
            })
    }

    /// Update reputation
    pub fn update_reputation(&mut self, agent_id: &str, delta: f32, reason: &str) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let rep = self.get_reputation(agent_id);
        rep.score = (rep.score + delta).clamp(0.0, 1.0);
        rep.review_count += 1;
        rep.history.push(ReputationEvent {
            delta,
            reason: reason.to_string(),
            timestamp: now,
        });
    }
}

impl Default for ReputationManager {
    fn default() -> Self {
        Self::new()
    }
}
