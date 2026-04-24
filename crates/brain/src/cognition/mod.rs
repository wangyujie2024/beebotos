//! Cognitive System
//!
//! Perception, decision making, and planning.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::utils::compare_f32;

pub mod decision;
pub mod perception;
pub mod planning;

/// Cognitive state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveState {
    pub attention_focus: Vec<String>,
    pub working_memory: WorkingMemory,
    pub goals: Vec<Goal>,
    pub current_intention: Option<Intention>,
}

impl CognitiveState {
    pub fn new() -> Self {
        Self {
            attention_focus: vec![],
            working_memory: WorkingMemory::new(10),
            goals: vec![],
            current_intention: None,
        }
    }

    /// Add to working memory
    pub fn memorize(&mut self, item: MemoryItem) {
        self.working_memory.add(item);
    }

    /// Set current goal
    pub fn set_goal(&mut self, goal: Goal) {
        self.goals.push(goal);
        self.goals
            .sort_by(|a, b| compare_f32(&b.priority, &a.priority));
    }

    /// Form intention from top goal
    pub fn form_intention(&mut self) -> Option<Intention> {
        self.goals.first().map(|goal| {
            let intention = Intention {
                id: uuid::Uuid::new_v4().to_string(),
                goal_id: goal.id.clone(),
                plan: vec![],
                status: IntentionStatus::Formed,
            };
            self.current_intention = Some(intention.clone());
            intention
        })
    }
}

impl Default for CognitiveState {
    fn default() -> Self {
        Self::new()
    }
}

/// Working memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkingMemory {
    capacity: usize,
    items: Vec<MemoryItem>,
    decay_rate: f32,
}

impl WorkingMemory {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            items: vec![],
            decay_rate: 0.1,
        }
    }

    pub fn add(&mut self, item: MemoryItem) {
        if self.items.len() >= self.capacity {
            // Remove lowest activation item
            if let Some(min_idx) = self
                .items
                .iter()
                .enumerate()
                .min_by(|a, b| compare_f32(&a.1.activation, &b.1.activation))
                .map(|(i, _)| i)
            {
                self.items.remove(min_idx);
            }
        }
        self.items.push(item);
    }

    pub fn get(&self, key: &str) -> Option<&MemoryItem> {
        self.items.iter().find(|i| i.key == key)
    }

    pub fn decay(&mut self) {
        for item in &mut self.items {
            item.activation *= 1.0 - self.decay_rate;
        }
        // Remove items below threshold
        self.items.retain(|i| i.activation > 0.1);
    }

    pub fn items(&self) -> &[MemoryItem] {
        &self.items
    }
}

/// Memory item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryItem {
    pub key: String,
    pub value: serde_json::Value,
    pub activation: f32,
    pub timestamp: u64,
}

/// Goal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub description: String,
    pub priority: f32,
    pub deadline: Option<u64>,
    pub subgoals: Vec<Goal>,
    pub status: GoalStatus,
}

impl Goal {
    pub fn new(description: impl Into<String>, priority: f32) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            description: description.into(),
            priority,
            deadline: None,
            subgoals: vec![],
            status: GoalStatus::Active,
        }
    }

    pub fn with_deadline(mut self, deadline: u64) -> Self {
        self.deadline = Some(deadline);
        self
    }
}

/// Goal status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GoalStatus {
    Active,
    Suspended,
    Achieved,
    Failed,
}

/// Intention
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intention {
    pub id: String,
    pub goal_id: String,
    pub plan: Vec<Action>,
    pub status: IntentionStatus,
}

/// Intention status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum IntentionStatus {
    Formed,
    Executing,
    Suspended,
    Completed,
    Failed,
}

/// Action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    pub name: String,
    pub params: HashMap<String, serde_json::Value>,
    pub preconditions: Vec<String>,
    pub effects: Vec<String>,
}

impl Action {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            params: HashMap::new(),
            preconditions: vec![],
            effects: vec![],
        }
    }

    pub fn with_param(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.params.insert(key.into(), value.into());
        self
    }
}

/// Belief
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Belief {
    pub id: String,
    pub proposition: String,
    pub confidence: f32, // 0.0 - 1.0
    pub source: BeliefSource,
    pub timestamp: u64,
}

impl Belief {
    pub fn new(proposition: impl Into<String>, confidence: f32) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            proposition: proposition.into(),
            confidence,
            source: BeliefSource::Inference,
            timestamp: crate::utils::current_timestamp_secs(),
        }
    }
}

/// Belief source
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BeliefSource {
    Perception,
    Inference,
    Communication,
    Memory,
}

/// Cognitive errors
#[derive(Debug, Clone)]
pub enum CognitiveError {
    InvalidState(String),
    PlanningFailed(String),
    MemoryFull,
    GoalConflict(String),
}

impl std::fmt::Display for CognitiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CognitiveError::InvalidState(s) => write!(f, "Invalid state: {}", s),
            CognitiveError::PlanningFailed(s) => write!(f, "Planning failed: {}", s),
            CognitiveError::MemoryFull => write!(f, "Working memory full"),
            CognitiveError::GoalConflict(s) => write!(f, "Goal conflict: {}", s),
        }
    }
}

impl std::error::Error for CognitiveError {}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
