//! Attention Mechanism
//!
//! Focus and saliency computation.

use serde::{Deserialize, Serialize};

use crate::utils::compare_f32;

/// Attention focus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attention {
    /// Current focus targets
    pub focus_stack: Vec<Focus>,
    /// Maximum focus capacity
    pub capacity: usize,
    /// Decay rate for saliency
    pub decay_rate: f32,
}

/// Focus target
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Focus {
    pub target: String,
    pub saliency: f32,
    pub focus_type: FocusType,
}

/// Type of focus
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FocusType {
    /// Goal-directed
    Goal,
    /// Stimulus-driven
    Stimulus,
    /// Habitual
    Habit,
    /// Social
    Social,
}

impl Attention {
    /// Create new attention with given capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            focus_stack: Vec::with_capacity(capacity),
            capacity,
            decay_rate: 0.1,
        }
    }

    /// Attend to a target
    pub fn attend(&mut self, target: impl Into<String>, saliency: f32, focus_type: FocusType) {
        let target = target.into();

        // Check if already attending
        if let Some(pos) = self.focus_stack.iter().position(|f| f.target == target) {
            // Update saliency
            self.focus_stack[pos].saliency = (self.focus_stack[pos].saliency + saliency) / 2.0;
            return;
        }

        // Make room if at capacity
        if self.focus_stack.len() >= self.capacity {
            self.focus_stack.remove(0);
        }

        self.focus_stack.push(Focus {
            target,
            saliency: saliency.clamp(0.0, 1.0),
            focus_type,
        });

        // Sort by saliency
        self.focus_stack
            .sort_by(|a, b| compare_f32(&b.saliency, &a.saliency));
    }

    /// Release attention from target
    pub fn release(&mut self, target: &str) {
        self.focus_stack.retain(|f| f.target != target);
    }

    /// Get current primary focus
    pub fn primary(&self) -> Option<&Focus> {
        self.focus_stack.first()
    }

    /// Get all current foci
    pub fn foci(&self) -> &[Focus] {
        &self.focus_stack
    }

    /// Decay saliency over time
    pub fn decay(&mut self) {
        for focus in &mut self.focus_stack {
            focus.saliency *= 1.0 - self.decay_rate;
        }

        // Remove items below threshold
        self.focus_stack.retain(|f| f.saliency > 0.1);
    }

    /// Calculate attention load (0.0 - 1.0)
    pub fn load(&self) -> f32 {
        self.focus_stack.len() as f32 / self.capacity as f32
    }

    /// Check if overloaded
    pub fn is_overloaded(&self) -> bool {
        self.load() > 0.9
    }

    /// Compute saliency for input
    pub fn compute_saliency(&self, input: &str, context: &str) -> f32 {
        // Simple saliency: novelty + relevance to context
        let novelty = if self.focus_stack.iter().any(|f| f.target == input) {
            0.3 // Already known
        } else {
            0.8 // Novel
        };

        let relevance = if context.contains(input) || input.contains(context) {
            0.9
        } else {
            0.4
        };

        (novelty + relevance) / 2.0
    }
}

impl Default for Attention {
    fn default() -> Self {
        Self::new(5)
    }
}

/// Saliency map for visual/spatial attention
#[derive(Debug, Clone)]
pub struct SaliencyMap {
    width: usize,
    height: usize,
    values: Vec<f32>,
}

impl SaliencyMap {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            values: vec![0.0; width * height],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> f32 {
        if x < self.width && y < self.height {
            self.values[y * self.width + x]
        } else {
            0.0
        }
    }

    pub fn set(&mut self, x: usize, y: usize, value: f32) {
        if x < self.width && y < self.height {
            self.values[y * self.width + x] = value.clamp(0.0, 1.0);
        }
    }

    /// Get location of maximum saliency
    pub fn max_location(&self) -> Option<(usize, usize)> {
        self.values
            .iter()
            .enumerate()
            .max_by(|a, b| compare_f32(a.1, b.1))
            .map(|(idx, _)| (idx % self.width, idx / self.width))
    }

    /// Apply Gaussian blur
    pub fn blur(&mut self, sigma: f32) {
        // Simplified blur - in production use proper Gaussian
        let kernel_size = (sigma * 3.0) as usize * 2 + 1;
        let _half = kernel_size / 2;

        // Placeholder for actual blur implementation
        let _ = sigma;
    }
}

/// Selective attention mechanism
pub struct SelectiveAttention {
    attention: Attention,
    filter: AttentionFilter,
}

/// Attention filter criteria
#[derive(Debug, Clone)]
pub struct AttentionFilter {
    pub min_saliency: f32,
    pub allowed_types: Vec<FocusType>,
    pub excluded_patterns: Vec<String>,
}

impl Default for AttentionFilter {
    fn default() -> Self {
        Self {
            min_saliency: 0.3,
            allowed_types: vec![FocusType::Goal, FocusType::Stimulus],
            excluded_patterns: vec![],
        }
    }
}

impl SelectiveAttention {
    pub fn new() -> Self {
        Self {
            attention: Attention::default(),
            filter: AttentionFilter::default(),
        }
    }

    /// Filtered attention
    pub fn attend_filtered(
        &mut self,
        target: impl Into<String>,
        saliency: f32,
        focus_type: FocusType,
    ) {
        if saliency < self.filter.min_saliency {
            return;
        }

        if !self.filter.allowed_types.contains(&focus_type) {
            return;
        }

        let target = target.into();
        for pattern in &self.filter.excluded_patterns {
            if target.contains(pattern) {
                return;
            }
        }

        self.attention.attend(target, saliency, focus_type);
    }

    pub fn attention(&self) -> &Attention {
        &self.attention
    }

    pub fn attention_mut(&mut self) -> &mut Attention {
        &mut self.attention
    }
}

impl Default for SelectiveAttention {
    fn default() -> Self {
        Self::new()
    }
}
