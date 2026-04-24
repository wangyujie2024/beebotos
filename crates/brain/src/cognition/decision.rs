use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionContext {
    pub situation: String,
    pub available_options: Vec<DecisionOption>,
    pub constraints: Vec<Constraint>,
    pub objectives: Vec<Objective>,
    pub time_pressure: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionOption {
    pub id: String,
    pub description: String,
    pub expected_outcomes: Vec<Outcome>,
    pub resource_requirements: ResourceRequirements,
    pub risk_level: RiskLevel,
    pub time_horizon: TimeHorizon,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outcome {
    pub description: String,
    pub probability: f32,
    pub utility: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceRequirements {
    pub compute_units: u32,
    pub memory_mb: u32,
    pub tokens: u32,
    pub time_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskLevel {
    Negligible = 0,
    Low = 1,
    Medium = 2,
    High = 3,
    Critical = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeHorizon {
    Immediate,
    ShortTerm,
    MediumTerm,
    LongTerm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraint {
    pub constraint_type: ConstraintType,
    pub description: String,
    pub weight: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintType {
    Hard,
    Soft,
    Preferential,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Objective {
    pub name: String,
    pub priority: u32,
    pub target_value: f32,
    pub current_value: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub chosen_option_id: String,
    pub confidence: f32,
    pub reasoning: String,
    pub expected_value: f32,
    pub alternative_considered: Vec<String>,
    pub decision_timestamp: u64,
}

pub trait DecisionStrategy: Send + Sync {
    fn decide(&self, context: &DecisionContext) -> Decision;
    fn name(&self) -> &'static str;
}

pub struct ExpectedValueStrategy;

impl DecisionStrategy for ExpectedValueStrategy {
    fn decide(&self, context: &DecisionContext) -> Decision {
        let mut best_option: Option<&DecisionOption> = None;
        let mut best_ev = f32::NEG_INFINITY;

        for option in &context.available_options {
            let ev = option
                .expected_outcomes
                .iter()
                .map(|o| o.probability * o.utility)
                .sum();

            if ev > best_ev {
                best_ev = ev;
                best_option = Some(option);
            }
        }

        let chosen = best_option.unwrap_or(&context.available_options[0]);

        Decision {
            chosen_option_id: chosen.id.clone(),
            confidence: 0.7,
            reasoning: format!("Maximized expected value: {}", best_ev),
            expected_value: best_ev,
            alternative_considered: context
                .available_options
                .iter()
                .filter(|o| o.id != chosen.id)
                .map(|o| o.id.clone())
                .collect(),
            decision_timestamp: chrono::Utc::now().timestamp() as u64,
        }
    }

    fn name(&self) -> &'static str {
        "ExpectedValue"
    }
}

pub struct MinimaxStrategy;

impl DecisionStrategy for MinimaxStrategy {
    fn decide(&self, context: &DecisionContext) -> Decision {
        let mut best_option: Option<&DecisionOption> = None;
        let mut best_worst_case = f32::NEG_INFINITY;

        for option in &context.available_options {
            let worst_case = option
                .expected_outcomes
                .iter()
                .map(|o| o.utility)
                .fold(f32::INFINITY, f32::min);

            if worst_case > best_worst_case {
                best_worst_case = worst_case;
                best_option = Some(option);
            }
        }

        let chosen = best_option.unwrap_or(&context.available_options[0]);

        Decision {
            chosen_option_id: chosen.id.clone(),
            confidence: 0.6,
            reasoning: format!("Maximized minimum utility: {}", best_worst_case),
            expected_value: best_worst_case,
            alternative_considered: vec![],
            decision_timestamp: chrono::Utc::now().timestamp() as u64,
        }
    }

    fn name(&self) -> &'static str {
        "Minimax"
    }
}

pub struct SatisficingStrategy {
    aspiration_level: f32,
}

impl SatisficingStrategy {
    pub fn new(aspiration_level: f32) -> Self {
        Self { aspiration_level }
    }
}

impl DecisionStrategy for SatisficingStrategy {
    fn decide(&self, context: &DecisionContext) -> Decision {
        let chosen = context
            .available_options
            .iter()
            .find(|opt| {
                let ev: f32 = opt
                    .expected_outcomes
                    .iter()
                    .map(|o| o.probability * o.utility)
                    .sum();
                ev >= self.aspiration_level
            })
            .unwrap_or(&context.available_options[0]);

        Decision {
            chosen_option_id: chosen.id.clone(),
            confidence: 0.65,
            reasoning: format!(
                "First option meeting aspiration level {}",
                self.aspiration_level
            ),
            expected_value: self.aspiration_level,
            alternative_considered: vec![],
            decision_timestamp: chrono::Utc::now().timestamp() as u64,
        }
    }

    fn name(&self) -> &'static str {
        "Satisficing"
    }
}

#[allow(dead_code)]
pub struct MultiCriteriaDecisionAnalysis {
    criteria_weights: HashMap<String, f32>,
}

#[allow(dead_code)]
impl MultiCriteriaDecisionAnalysis {
    pub fn new(criteria_weights: HashMap<String, f32>) -> Self {
        Self { criteria_weights }
    }

    fn evaluate_option(&self, option: &DecisionOption) -> f32 {
        let mut score = 0.0;

        let risk_score = 1.0 - (option.risk_level as i32 as f32 / 4.0);
        score += self.criteria_weights.get("risk").unwrap_or(&0.25) * risk_score;

        let ev: f32 = option
            .expected_outcomes
            .iter()
            .map(|o| o.probability * o.utility)
            .sum();
        let normalized_ev = (ev + 1.0) / 2.0;
        score += self.criteria_weights.get("expected_value").unwrap_or(&0.35) * normalized_ev;

        let time_score = match option.time_horizon {
            TimeHorizon::Immediate => 1.0,
            TimeHorizon::ShortTerm => 0.8,
            TimeHorizon::MediumTerm => 0.6,
            TimeHorizon::LongTerm => 0.4,
        };
        score += self.criteria_weights.get("speed").unwrap_or(&0.2) * time_score;

        let resource_efficiency =
            1.0 / (1.0 + (option.resource_requirements.compute_units as f32 / 1000.0));
        score += self.criteria_weights.get("efficiency").unwrap_or(&0.2) * resource_efficiency;

        score
    }
}

impl DecisionStrategy for MultiCriteriaDecisionAnalysis {
    fn decide(&self, context: &DecisionContext) -> Decision {
        let mut best_option: Option<&DecisionOption> = None;
        let mut best_score = f32::NEG_INFINITY;

        for option in &context.available_options {
            let score = self.evaluate_option(option);
            if score > best_score {
                best_score = score;
                best_option = Some(option);
            }
        }

        let chosen = best_option.unwrap_or(&context.available_options[0]);

        Decision {
            chosen_option_id: chosen.id.clone(),
            confidence: 0.75,
            reasoning: format!("Highest MCDA score: {:.3}", best_score),
            expected_value: best_score,
            alternative_considered: vec![],
            decision_timestamp: chrono::Utc::now().timestamp() as u64,
        }
    }

    fn name(&self) -> &'static str {
        "MCDA"
    }
}

pub struct DecisionEngine {
    strategies: Vec<Box<dyn DecisionStrategy>>,
    current_strategy: usize,
}

impl DecisionEngine {
    pub fn new() -> Self {
        let mut engine = Self {
            strategies: vec![],
            current_strategy: 0,
        };
        // Register default strategies
        engine.register_strategy(Box::new(ExpectedValueStrategy));
        engine.register_strategy(Box::new(MinimaxStrategy));
        engine.register_strategy(Box::new(SatisficingStrategy::new(0.7)));
        engine
    }

    pub fn register_strategy(&mut self, strategy: Box<dyn DecisionStrategy>) {
        self.strategies.push(strategy);
    }

    pub fn set_strategy(&mut self, index: usize) {
        if index < self.strategies.len() {
            self.current_strategy = index;
        }
    }

    pub fn decide(&self, context: &DecisionContext) -> Decision {
        if let Some(strategy) = self.strategies.get(self.current_strategy) {
            strategy.decide(context)
        } else {
            Decision {
                chosen_option_id: context.available_options[0].id.clone(),
                confidence: 0.5,
                reasoning: "No strategy selected, defaulting to first option".to_string(),
                expected_value: 0.0,
                alternative_considered: vec![],
                decision_timestamp: chrono::Utc::now().timestamp() as u64,
            }
        }
    }

    pub fn compare_strategies(&self, context: &DecisionContext) -> Vec<(String, Decision)> {
        self.strategies
            .iter()
            .map(|s| (s.name().to_string(), s.decide(context)))
            .collect()
    }

    /// Get the number of available strategies
    pub fn strategy_count(&self) -> usize {
        self.strategies.len()
    }
}
