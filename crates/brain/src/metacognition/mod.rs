//! Metacognition Module
//!
//! Self-reflection, monitoring, and cognitive regulation capabilities.
//! This module enables the agent to reason about its own thinking processes,
//! monitor performance, and adjust strategies based on self-assessment.

pub mod reflection;

pub use reflection::{
    LearningEntry, ReflectionType, ReflectiveSystem, ReflectiveThought, SelfAssessment,
};

use crate::cognition::{CognitiveState, Goal};
use crate::error::BrainResult;
// Performance monitoring 现在使用 metrics 模块的 MetricsCollector
pub use crate::metrics::MetricsCollector as PerformanceMonitor;

/// Metacognitive awareness level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AwarenessLevel {
    /// No self-awareness
    Unaware,
    /// Basic awareness of own state
    Basic,
    /// Can monitor performance
    Monitoring,
    /// Can reflect on strategies
    Reflective,
    /// Can adapt strategies based on reflection
    Adaptive,
}

/// Metacognitive engine
pub struct MetacognitionEngine {
    /// Self-reflection system
    reflective_system: ReflectiveSystem,
    /// Performance monitoring (使用 metrics 模块)
    monitor: crate::metrics::MetricsCollector,
    /// Current awareness level
    awareness_level: AwarenessLevel,
    /// Confidence in own capabilities (0.0 - 1.0)
    self_confidence: f32,
    /// Strategy adjustment history
    strategy_adjustments: Vec<StrategyAdjustment>,
}

/// Strategy adjustment record
#[derive(Debug, Clone)]
pub struct StrategyAdjustment {
    pub timestamp: u64,
    pub trigger: AdjustmentTrigger,
    pub previous_strategy: String,
    pub new_strategy: String,
    pub reason: String,
}

/// Trigger for strategy adjustment
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdjustmentTrigger {
    PerformanceDrop,
    RepeatedFailure,
    HighUncertainty,
    UserFeedback,
    SelfReflection,
    Timeout,
}

impl MetacognitionEngine {
    /// Create a new metacognition engine
    pub fn new() -> Self {
        Self {
            reflective_system: ReflectiveSystem::new(),
            monitor: crate::metrics::MetricsCollector::new(),
            awareness_level: AwarenessLevel::Reflective,
            self_confidence: 0.7,
            strategy_adjustments: vec![],
        }
    }

    /// Create with specific awareness level
    pub fn with_awareness(level: AwarenessLevel) -> Self {
        Self {
            reflective_system: ReflectiveSystem::new(),
            monitor: PerformanceMonitor::new(),
            awareness_level: level,
            self_confidence: 0.7,
            strategy_adjustments: vec![],
        }
    }

    /// Get current awareness level
    pub fn awareness_level(&self) -> AwarenessLevel {
        self.awareness_level
    }

    /// Set awareness level
    pub fn set_awareness_level(&mut self, level: AwarenessLevel) {
        self.awareness_level = level;
    }

    /// Check if agent has capability at current awareness level
    pub fn has_capability(&self, required_level: AwarenessLevel) -> bool {
        self.awareness_level >= required_level
    }

    /// Record performance metric (使用 gauge 存储最新值)
    pub fn record_metric(&mut self, name: &str, value: f64) {
        self.monitor.set_gauge(name, value);
        // 同时记录为 timing 以便计算平均值
        self.monitor.record_timing(name, value);
    }

    /// Get average metric value
    pub fn average_metric(&self, name: &str) -> Option<f64> {
        self.monitor.get_average_timing(name)
    }

    /// Create a reflection about current state
    pub fn reflect_on_state(
        &mut self,
        cognitive_state: &CognitiveState,
    ) -> BrainResult<ReflectiveThought> {
        if !self.has_capability(AwarenessLevel::Reflective) {
            return Err(crate::error::BrainError::InvalidState(
                "Current awareness level does not support reflection".to_string(),
            ));
        }

        let subject = format!(
            "Cognitive state with {} active goals",
            cognitive_state.goals.len()
        );
        let content = self.analyze_cognitive_state(cognitive_state);

        Ok(self
            .reflective_system
            .reflect(subject, content, ReflectionType::SelfAssessment))
    }

    /// Analyze cognitive state and produce reflection content
    fn analyze_cognitive_state(&self, state: &CognitiveState) -> String {
        let mut analysis = format!(
            "Currently focusing on {} items. Working memory has {} items. ",
            state.attention_focus.len(),
            state.working_memory.items().len()
        );

        if !state.goals.is_empty() {
            analysis.push_str(&format!("Top goal: {}. ", state.goals[0].description));
        }

        if state.current_intention.is_none() && !state.goals.is_empty() {
            analysis.push_str("No current intention formed despite having goals. ");
        }

        analysis
    }

    /// Evaluate performance and suggest strategy adjustment
    pub fn evaluate_and_adjust(
        &mut self,
        task_name: &str,
        success: bool,
        duration_ms: u64,
    ) -> Option<StrategyAdjustment> {
        if !self.has_capability(AwarenessLevel::Adaptive) {
            return None;
        }

        // Record performance
        self.monitor.record(
            &format!("{}_success", task_name),
            if success { 1.0 } else { 0.0 },
        );
        self.monitor
            .record(&format!("{}_duration", task_name), duration_ms as f64);

        // Check if adjustment needed
        let avg_success = self.monitor.average(&format!("{}_success", task_name))?;

        if avg_success < 0.5 {
            let adjustment = StrategyAdjustment {
                timestamp: crate::utils::current_timestamp_secs(),
                trigger: if success {
                    AdjustmentTrigger::SelfReflection
                } else {
                    AdjustmentTrigger::RepeatedFailure
                },
                previous_strategy: "current".to_string(),
                new_strategy: "conservative".to_string(),
                reason: format!(
                    "Low success rate ({:.2}) for task {}",
                    avg_success, task_name
                ),
            };

            self.strategy_adjustments.push(adjustment.clone());
            self.self_confidence *= 0.9; // Reduce confidence

            return Some(adjustment);
        }

        // Success boosts confidence
        if success {
            self.self_confidence = (self.self_confidence * 0.9 + 0.1).min(1.0);
        }

        None
    }

    /// Get self-confidence level
    pub fn self_confidence(&self) -> f32 {
        self.self_confidence
    }

    /// Conduct comprehensive self-assessment
    pub fn self_assess(&mut self) -> SelfAssessment {
        self.reflective_system.conduct_self_assessment()
    }

    /// Record learning from experience
    pub fn record_learning(
        &mut self,
        situation: String,
        action: String,
        outcome: String,
    ) -> LearningEntry {
        self.reflective_system
            .record_learning(situation, action, outcome)
    }

    /// Get reflection system reference
    pub fn reflective_system(&self) -> &ReflectiveSystem {
        &self.reflective_system
    }

    /// Get mutable reflection system reference
    pub fn reflective_system_mut(&mut self) -> &mut ReflectiveSystem {
        &mut self.reflective_system
    }

    /// Get performance monitor reference
    pub fn monitor(&self) -> &PerformanceMonitor {
        &self.monitor
    }

    /// Get strategy adjustment history
    pub fn strategy_adjustments(&self) -> &[StrategyAdjustment] {
        &self.strategy_adjustments
    }

    /// Assess whether current strategy is effective for given goal
    pub fn assess_strategy_for_goal(
        &self,
        goal: &Goal,
        recent_success_rate: f32,
    ) -> StrategyAssessment {
        let effectiveness = if recent_success_rate > 0.7 {
            StrategyEffectiveness::Effective
        } else if recent_success_rate > 0.4 {
            StrategyEffectiveness::NeedsImprovement
        } else {
            StrategyEffectiveness::Ineffective
        };

        StrategyAssessment {
            goal_id: goal.id.clone(),
            effectiveness,
            confidence: self.self_confidence,
            recommendation: self.generate_recommendation(effectiveness),
        }
    }

    fn generate_recommendation(&self, effectiveness: StrategyEffectiveness) -> String {
        match effectiveness {
            StrategyEffectiveness::Effective => "Continue current strategy".to_string(),
            StrategyEffectiveness::NeedsImprovement => "Consider minor adjustments".to_string(),
            StrategyEffectiveness::Ineffective => "Significant strategy change needed".to_string(),
        }
    }
}

impl Default for MetacognitionEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Strategy effectiveness assessment
#[derive(Debug, Clone)]
pub struct StrategyAssessment {
    pub goal_id: String,
    pub effectiveness: StrategyEffectiveness,
    pub confidence: f32,
    pub recommendation: String,
}

/// Strategy effectiveness levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyEffectiveness {
    Effective,
    NeedsImprovement,
    Ineffective,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metacognition_engine_creation() {
        let engine = MetacognitionEngine::new();
        assert_eq!(engine.awareness_level(), AwarenessLevel::Reflective);
        assert!(engine.self_confidence() > 0.0);
    }

    #[test]
    fn test_awareness_capability_check() {
        let engine = MetacognitionEngine::with_awareness(AwarenessLevel::Monitoring);
        assert!(engine.has_capability(AwarenessLevel::Basic));
        assert!(engine.has_capability(AwarenessLevel::Monitoring));
        assert!(!engine.has_capability(AwarenessLevel::Reflective));
    }

    #[test]
    fn test_performance_recording() {
        let mut engine = MetacognitionEngine::new();
        engine.record_metric("test_metric", 0.8);

        let avg = engine.average_metric("test_metric");
        assert!(avg.is_some());
        assert!((avg.unwrap() - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_strategy_adjustment() {
        let mut engine = MetacognitionEngine::with_awareness(AwarenessLevel::Adaptive);

        // Simulate repeated failures
        for _ in 0..5 {
            engine.evaluate_and_adjust("test_task", false, 1000);
        }

        // Should have triggered adjustment
        assert!(!engine.strategy_adjustments().is_empty());
    }

    #[test]
    fn test_self_assessment() {
        let mut engine = MetacognitionEngine::new();
        let assessment = engine.self_assess();

        assert!(!assessment.assessment_id.is_empty());
        assert!(!assessment.assessed_capabilities.is_empty());
    }

    #[test]
    fn test_learning_record() {
        let mut engine = MetacognitionEngine::new();
        let entry = engine.record_learning(
            "Test situation".to_string(),
            "Test action".to_string(),
            "Test outcome".to_string(),
        );

        assert!(!entry.entry_id.is_empty());
        assert_eq!(entry.situation, "Test situation");
    }

    #[test]
    fn test_strategy_assessment() {
        let engine = MetacognitionEngine::new();
        let goal = Goal::new("Test goal", 0.8);

        let assessment = engine.assess_strategy_for_goal(&goal, 0.8);
        assert_eq!(assessment.effectiveness, StrategyEffectiveness::Effective);

        let assessment = engine.assess_strategy_for_goal(&goal, 0.3);
        assert_eq!(assessment.effectiveness, StrategyEffectiveness::Ineffective);
    }
}
