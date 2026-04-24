//! Learning System
//!
//! Reinforcement learning and skill acquisition.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Q-Learning agent
pub struct QLearning {
    q_table: HashMap<(String, String), f64>,
    learning_rate: f64,
    discount_factor: f64,
    epsilon: f64, // Exploration rate
}

impl QLearning {
    /// Create new Q-Learning agent
    pub fn new() -> Self {
        Self {
            q_table: HashMap::new(),
            learning_rate: 0.1,
            discount_factor: 0.9,
            epsilon: 0.1,
        }
    }

    /// Choose action using epsilon-greedy
    pub fn choose_action(&self, state: &str, actions: &[String]) -> String {
        if actions.is_empty() {
            return String::new();
        }

        // Epsilon-greedy
        if rand::random::<f64>() < self.epsilon {
            // Explore: random action
            actions[rand::random::<usize>() % actions.len()].clone()
        } else {
            // Exploit: best known action
            let mut best_action = &actions[0];
            let mut best_value = f64::NEG_INFINITY;

            for action in actions {
                let q_value = self
                    .q_table
                    .get(&(state.to_string(), action.clone()))
                    .copied()
                    .unwrap_or(0.0);

                if q_value > best_value {
                    best_value = q_value;
                    best_action = action;
                }
            }

            best_action.clone()
        }
    }

    /// Update Q-value
    pub fn update(
        &mut self,
        state: &str,
        action: &str,
        reward: f64,
        next_state: &str,
        next_actions: &[String],
    ) {
        let current_q = self
            .q_table
            .get(&(state.to_string(), action.to_string()))
            .copied()
            .unwrap_or(0.0);

        // Maximum Q-value for next state
        let max_next_q = if next_actions.is_empty() {
            0.0
        } else {
            next_actions
                .iter()
                .map(|a| {
                    self.q_table
                        .get(&(next_state.to_string(), a.clone()))
                        .copied()
                        .unwrap_or(0.0)
                })
                .fold(f64::NEG_INFINITY, f64::max)
        };

        // Q-learning update rule
        let new_q = current_q
            + self.learning_rate * (reward + self.discount_factor * max_next_q - current_q);

        self.q_table
            .insert((state.to_string(), action.to_string()), new_q);
    }

    /// Get Q-value
    pub fn q_value(&self, state: &str, action: &str) -> f64 {
        self.q_table
            .get(&(state.to_string(), action.to_string()))
            .copied()
            .unwrap_or(0.0)
    }

    /// Decay exploration rate
    pub fn decay_epsilon(&mut self, decay: f64) {
        self.epsilon *= 1.0 - decay;
        self.epsilon = self.epsilon.max(0.01);
    }
}

impl Default for QLearning {
    fn default() -> Self {
        Self::new()
    }
}

/// Experience replay buffer for reinforcement learning
#[derive(Debug)]
pub struct ReplayBuffer {
    buffer: Vec<LearningExperience>,
    capacity: usize,
    position: usize,
}

/// Experience tuple for reinforcement learning
///
/// Represents a single (s, a, r, s') transition in RL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningExperience {
    pub state: String,
    pub action: String,
    pub reward: f64,
    pub next_state: String,
    pub done: bool,
}

impl ReplayBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity),
            capacity,
            position: 0,
        }
    }

    pub fn push(&mut self, exp: LearningExperience) {
        if self.buffer.len() < self.capacity {
            self.buffer.push(exp);
        } else {
            self.buffer[self.position] = exp;
        }
        self.position = (self.position + 1) % self.capacity;
    }

    pub fn sample(&self, batch_size: usize) -> Vec<&LearningExperience> {
        use rand::seq::SliceRandom;

        let mut rng = rand::thread_rng();
        self.buffer
            .choose_multiple(&mut rng, batch_size.min(self.buffer.len()))
            .collect()
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

/// Policy gradient learning
pub struct PolicyGradient {
    policy: HashMap<String, Vec<f64>>,
    learning_rate: f64,
}

impl PolicyGradient {
    pub fn new() -> Self {
        Self {
            policy: HashMap::new(),
            learning_rate: 0.01,
        }
    }

    /// Get action probabilities for a state
    pub fn action_probs(&self, state: &str, num_actions: usize) -> Vec<f64> {
        self.policy
            .get(state)
            .cloned()
            .unwrap_or_else(|| vec![1.0 / num_actions as f64; num_actions])
    }

    /// Update policy based on reward
    pub fn update(&mut self, state: &str, action: usize, reward: f64, num_actions: usize) {
        let probs = self
            .policy
            .entry(state.to_string())
            .or_insert_with(|| vec![1.0 / num_actions as f64; num_actions]);

        if action < probs.len() {
            // Gradient ascent
            probs[action] += self.learning_rate * reward;

            // Normalize
            let sum: f64 = probs.iter().sum();
            for p in probs.iter_mut() {
                *p /= sum;
            }
        }
    }
}

impl Default for PolicyGradient {
    fn default() -> Self {
        Self::new()
    }
}

/// Hierarchical skill learning
pub struct SkillLearner {
    primitives: Vec<PrimitiveSkill>,
    composite_skills: Vec<CompositeSkill>,
}

#[derive(Debug, Clone)]
pub struct PrimitiveSkill {
    pub id: String,
    pub name: String,
    pub success_rate: f64,
}

#[derive(Debug, Clone)]
pub struct CompositeSkill {
    pub id: String,
    pub name: String,
    pub components: Vec<String>,
    pub mastery_level: f64,
}

impl SkillLearner {
    pub fn new() -> Self {
        Self {
            primitives: vec![],
            composite_skills: vec![],
        }
    }

    pub fn add_primitive(&mut self, skill: PrimitiveSkill) {
        self.primitives.push(skill);
    }

    /// Learn composite skill from primitives
    pub fn learn_composite(
        &mut self,
        name: impl Into<String>,
        components: Vec<String>,
    ) -> CompositeSkill {
        let skill = CompositeSkill {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            components,
            mastery_level: 0.0,
        };

        self.composite_skills.push(skill.clone());
        skill
    }

    /// Practice skill and improve mastery
    pub fn practice(&mut self, skill_id: &str, success: bool) {
        // Update success rate or mastery
        if let Some(primitive) = self.primitives.iter_mut().find(|s| s.id == skill_id) {
            primitive.success_rate =
                primitive.success_rate * 0.9 + (if success { 1.0 } else { 0.0 }) * 0.1;
        }

        if let Some(composite) = self.composite_skills.iter_mut().find(|s| s.id == skill_id) {
            composite.mastery_level =
                (composite.mastery_level + if success { 0.1 } else { 0.0 }).min(1.0);
        }
    }
}

impl Default for SkillLearner {
    fn default() -> Self {
        Self::new()
    }
}
