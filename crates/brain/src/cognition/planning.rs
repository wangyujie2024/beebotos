//! Planning module - Goal-directed action planning
//!
//! This module provides HTN (Hierarchical Task Network) planning capabilities.
//! Currently a framework for future implementation.

#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub description: String,
    pub priority: u32,
    pub deadline: Option<u64>,
    pub subgoals: Vec<Goal>,
    pub success_criteria: Vec<SuccessCriterion>,
    pub status: GoalStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoalStatus {
    Pending,
    Active,
    Blocked,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessCriterion {
    pub metric: String,
    pub target_value: f32,
    pub tolerance: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    pub name: String,
    pub preconditions: Vec<Condition>,
    pub effects: Vec<Effect>,
    pub cost: f32,
    pub duration_ms: u32,
    pub resources_required: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    pub variable: String,
    pub operator: ComparisonOperator,
    pub value: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComparisonOperator {
    Equal,
    NotEqual,
    GreaterThan,
    LessThan,
    GreaterThanOrEqual,
    LessThanOrEqual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Effect {
    pub variable: String,
    pub change: f32,
    pub assignment: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub variables: HashMap<String, f32>,
    pub timestamp: u64,
}

impl State {
    pub fn satisfies(&self, condition: &Condition) -> bool {
        let value = self
            .variables
            .get(&condition.variable)
            .copied()
            .unwrap_or(0.0);

        match condition.operator {
            ComparisonOperator::Equal => (value - condition.value).abs() < 0.001,
            ComparisonOperator::NotEqual => (value - condition.value).abs() >= 0.001,
            ComparisonOperator::GreaterThan => value > condition.value,
            ComparisonOperator::LessThan => value < condition.value,
            ComparisonOperator::GreaterThanOrEqual => value >= condition.value,
            ComparisonOperator::LessThanOrEqual => value <= condition.value,
        }
    }

    pub fn apply(&mut self, effect: &Effect) {
        if let Some(assignment) = effect.assignment {
            self.variables.insert(effect.variable.clone(), assignment);
        } else {
            let current = self.variables.get(&effect.variable).copied().unwrap_or(0.0);
            self.variables
                .insert(effect.variable.clone(), current + effect.change);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub goal_id: String,
    pub actions: Vec<Action>,
    pub total_cost: f32,
    pub estimated_duration_ms: u32,
    pub required_resources: Vec<String>,
    pub contingency_plans: Vec<ContingencyPlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContingencyPlan {
    pub trigger_condition: Condition,
    pub alternative_actions: Vec<Action>,
}

pub trait Planner: Send + Sync {
    fn plan(
        &self,
        goal: &Goal,
        initial_state: &State,
        available_actions: &[Action],
    ) -> Option<Plan>;
    fn replan(
        &self,
        current_plan: &Plan,
        current_state: &State,
        failure_reason: &str,
    ) -> Option<Plan>;
}

pub struct ForwardStateSpacePlanner;

impl Planner for ForwardStateSpacePlanner {
    fn plan(
        &self,
        goal: &Goal,
        initial_state: &State,
        available_actions: &[Action],
    ) -> Option<Plan> {
        let mut queue = VecDeque::new();
        queue.push_back((initial_state.clone(), vec![]));

        let mut visited = std::collections::HashSet::new();

        while let Some((state, actions)) = queue.pop_front() {
            let state_key = format!("{:?}", state.variables);
            if visited.contains(&state_key) {
                continue;
            }
            visited.insert(state_key);

            if Self::goal_satisfied(goal, &state) {
                let total_cost: f32 = actions.iter().map(|a: &Action| a.cost).sum();
                let duration: u32 = actions.iter().map(|a: &Action| a.duration_ms).sum();

                return Some(Plan {
                    id: uuid::Uuid::new_v4().to_string(),
                    goal_id: goal.id.clone(),
                    actions,
                    total_cost,
                    estimated_duration_ms: duration,
                    required_resources: vec![],
                    contingency_plans: vec![],
                });
            }

            for action in available_actions {
                if action.preconditions.iter().all(|p| state.satisfies(p)) {
                    let mut new_state = state.clone();
                    for effect in &action.effects {
                        new_state.apply(effect);
                    }

                    let mut new_actions = actions.clone();
                    new_actions.push(action.clone());

                    queue.push_back((new_state, new_actions));
                }
            }
        }

        None
    }

    fn replan(
        &self,
        _current_plan: &Plan,
        _current_state: &State,
        _failure_reason: &str,
    ) -> Option<Plan> {
        None
    }
}

impl ForwardStateSpacePlanner {
    fn goal_satisfied(goal: &Goal, state: &State) -> bool {
        goal.success_criteria.iter().all(|c| {
            let condition = Condition {
                variable: c.metric.clone(),
                operator: ComparisonOperator::GreaterThanOrEqual,
                value: c.target_value - c.tolerance,
            };
            state.satisfies(&condition)
        })
    }
}

pub struct HierarchicalTaskNetworkPlanner {
    methods: HashMap<String, Vec<HTNMethod>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HTNMethod {
    pub name: String,
    pub task: String,
    pub preconditions: Vec<Condition>,
    pub subtasks: Vec<String>,
}

impl HierarchicalTaskNetworkPlanner {
    pub fn new() -> Self {
        Self {
            methods: HashMap::new(),
        }
    }

    pub fn add_method(&mut self, task: String, method: HTNMethod) {
        self.methods.entry(task).or_default().push(method);
    }

    pub fn decompose(&self, task: &str, state: &State) -> Option<Vec<String>> {
        if let Some(methods) = self.methods.get(task) {
            for method in methods {
                if method.preconditions.iter().all(|p| state.satisfies(p)) {
                    return Some(method.subtasks.clone());
                }
            }
        }
        None
    }
}

pub struct ExecutionMonitor {
    active_plans: HashMap<String, PlanExecution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanExecution {
    pub plan_id: String,
    pub current_action_index: usize,
    pub execution_state: ExecutionState,
    pub start_time: u64,
    pub action_results: Vec<ActionResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionState {
    NotStarted,
    InProgress,
    Paused,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub action_id: String,
    pub success: bool,
    pub actual_duration_ms: u32,
    pub output: Option<String>,
    pub error: Option<String>,
}

impl ExecutionMonitor {
    pub fn new() -> Self {
        Self {
            active_plans: HashMap::new(),
        }
    }

    pub fn start_plan(&mut self, plan: &Plan) -> String {
        let execution = PlanExecution {
            plan_id: plan.id.clone(),
            current_action_index: 0,
            execution_state: ExecutionState::InProgress,
            start_time: chrono::Utc::now().timestamp() as u64,
            action_results: vec![],
        };

        self.active_plans.insert(plan.id.clone(), execution);
        plan.id.clone()
    }

    pub fn report_action_result(&mut self, plan_id: &str, result: ActionResult) {
        if let Some(execution) = self.active_plans.get_mut(plan_id) {
            execution.action_results.push(result);
            execution.current_action_index += 1;
        }
    }

    pub fn get_execution(&self, plan_id: &str) -> Option<&PlanExecution> {
        self.active_plans.get(plan_id)
    }

    pub fn complete_plan(&mut self, plan_id: &str, success: bool) {
        if let Some(execution) = self.active_plans.get_mut(plan_id) {
            execution.execution_state = if success {
                ExecutionState::Completed
            } else {
                ExecutionState::Failed
            };
        }
    }
}
