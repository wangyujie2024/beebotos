//! Trust System
//!
//! Trust calculation and management.

#![allow(dead_code)]

use std::collections::HashMap;

use uuid::Uuid;

/// Trust value for an entity
#[derive(Debug, Clone)]
pub struct TrustValue {
    pub direct: f32,
    pub reputation: f32,
    pub history: Vec<TrustEvent>,
}

#[derive(Debug, Clone)]
pub struct TrustEvent {
    pub positive: bool,
    pub magnitude: f32,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Trust calculator
pub struct TrustCalculator {
    values: HashMap<Uuid, TrustValue>,
}

impl TrustCalculator {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }

    pub fn record_interaction(&mut self, entity: Uuid, positive: bool, magnitude: f32) {
        let value = self.values.entry(entity).or_insert(TrustValue {
            direct: 0.5,
            reputation: 0.5,
            history: Vec::new(),
        });

        value.history.push(TrustEvent {
            positive,
            magnitude,
            timestamp: chrono::Utc::now(),
        });

        // Update direct trust
        let impact = if positive {
            magnitude * 0.1
        } else {
            -magnitude * 0.1
        };
        value.direct = (value.direct + impact).clamp(0.0, 1.0);
    }

    pub fn calculate_trust(&self, entity: Uuid) -> f32 {
        self.values
            .get(&entity)
            .map(|v| (v.direct * 0.6) + (v.reputation * 0.4))
            .unwrap_or(0.5)
    }
}

impl Default for TrustCalculator {
    fn default() -> Self {
        Self::new()
    }
}
