//! RePlanner
//!
//! Dynamic plan adaptation and replanning capabilities.
//! Handles failures, changing conditions, and new information.

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use super::plan::{Action, Plan, PlanStatus, PlanStep, PlanningResult, Priority, StepStatus};
use super::{Decomposer, DecompositionContext};

/// RePlanner trait for dynamic plan adaptation
#[async_trait::async_trait]
pub trait RePlanner: Send + Sync {
    /// Evaluate if replanning is needed
    async fn should_replan(&self, plan: &Plan, trigger: &RePlanTrigger) -> bool;

    /// Adapt the plan based on trigger
    async fn replan(&self, plan: &mut Plan, trigger: &RePlanTrigger) -> PlanningResult<()>;

    /// Get replanner name
    fn name(&self) -> &str;
}

/// Triggers for replanning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RePlanTrigger {
    /// Step failed
    StepFailed {
        step_index: usize,
        error: String,
        attempt: u32,
    },
    /// Condition changed
    ConditionChanged {
        condition: String,
        old_value: serde_json::Value,
        new_value: serde_json::Value,
    },
    /// New information available
    NewInformation {
        source: String,
        data: serde_json::Value,
    },
    /// Timeout approaching
    TimeoutWarning { remaining: Duration },
    /// External feedback
    ExternalFeedback {
        source: String,
        feedback: String,
        priority: Priority,
    },
    /// Resource constraint
    ResourceConstraint {
        resource: String,
        required: u64,
        available: u64,
    },
    /// Goal changed
    GoalChanged { new_goal: String, reason: String },
}

impl RePlanTrigger {
    /// Get severity level
    pub fn severity(&self) -> Priority {
        match self {
            RePlanTrigger::StepFailed { attempt, .. } if *attempt >= 3 => Priority::Critical,
            RePlanTrigger::StepFailed { .. } => Priority::High,
            RePlanTrigger::GoalChanged { .. } => Priority::Critical,
            RePlanTrigger::ResourceConstraint { .. } => Priority::High,
            RePlanTrigger::TimeoutWarning { .. } => Priority::High,
            RePlanTrigger::ExternalFeedback { priority, .. } => *priority,
            _ => Priority::Normal,
        }
    }

    /// Get description
    pub fn description(&self) -> String {
        match self {
            RePlanTrigger::StepFailed {
                step_index,
                error,
                attempt,
            } => {
                format!(
                    "Step {} failed (attempt {}): {}",
                    step_index, attempt, error
                )
            }
            RePlanTrigger::ConditionChanged { condition, .. } => {
                format!("Condition changed: {}", condition)
            }
            RePlanTrigger::NewInformation { source, .. } => {
                format!("New information from: {}", source)
            }
            RePlanTrigger::TimeoutWarning { remaining } => {
                format!("Timeout warning: {:?} remaining", remaining)
            }
            RePlanTrigger::ExternalFeedback { source, .. } => {
                format!("External feedback from: {}", source)
            }
            RePlanTrigger::ResourceConstraint { resource, .. } => {
                format!("Resource constraint: {}", resource)
            }
            RePlanTrigger::GoalChanged { new_goal, .. } => {
                format!("Goal changed to: {}", new_goal)
            }
        }
    }
}

/// Adaptation strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdaptationStrategy {
    /// Retry failed step
    Retry,
    /// Skip failed step
    Skip,
    /// Replace step with alternative
    Replace,
    /// Insert additional steps
    Insert,
    /// Reorder steps
    Reorder,
    /// Split step into sub-steps
    Split,
    /// Merge steps
    Merge,
    /// Complete replan
    Replan,
    /// Abort plan
    Abort,
}

/// Condition-based replanner
pub struct ConditionRePlanner;

impl ConditionRePlanner {
    /// Create new condition replanner
    pub fn new() -> Self {
        Self
    }

    /// Handle condition change
    fn handle_condition_change(
        &self,
        plan: &mut Plan,
        condition: &str,
        _new_value: &serde_json::Value,
    ) -> PlanningResult<()> {
        info!(
            "Adapting plan {} for condition change: {}",
            plan.id, condition
        );

        // Mark plan as replanning
        plan.status = PlanStatus::Replanning;

        // Add evaluation step
        let eval_step = PlanStep::reasoning(format!(
            "Evaluate impact of condition change: {}",
            condition
        ));

        // Find current position and insert
        let current_idx = plan
            .steps
            .iter()
            .position(|s| s.status == StepStatus::InProgress)
            .unwrap_or(plan.steps.len());

        plan.steps.insert(current_idx, eval_step);

        // Update dependencies
        plan.dependencies.clear();
        for i in 1..plan.steps.len() {
            plan.dependencies.insert(i, vec![i - 1]);
        }

        plan.status = PlanStatus::InProgress;
        Ok(())
    }
}

#[async_trait::async_trait]
impl RePlanner for ConditionRePlanner {
    async fn should_replan(&self, _plan: &Plan, trigger: &RePlanTrigger) -> bool {
        matches!(trigger, RePlanTrigger::ConditionChanged { .. })
            || matches!(trigger, RePlanTrigger::NewInformation { .. })
    }

    async fn replan(&self, plan: &mut Plan, trigger: &RePlanTrigger) -> PlanningResult<()> {
        match trigger {
            RePlanTrigger::ConditionChanged {
                condition,
                new_value,
                ..
            } => self.handle_condition_change(plan, condition, new_value),
            _ => Ok(()),
        }
    }

    fn name(&self) -> &str {
        "ConditionRePlanner"
    }
}

impl Default for ConditionRePlanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Feedback-based replanner
pub struct FeedbackRePlanner {
    decomposer: Decomposer,
}

impl FeedbackRePlanner {
    /// Create new feedback replanner
    pub fn new() -> Self {
        Self {
            decomposer: Decomposer::new(),
        }
    }

    /// Handle step failure
    async fn handle_step_failure(
        &self,
        plan: &mut Plan,
        step_index: usize,
        error: &str,
        attempt: u32,
    ) -> PlanningResult<()> {
        warn!(
            "Handling step {} failure in plan {}: {} (attempt {})",
            step_index, plan.id, error, attempt
        );

        if attempt >= 3 {
            // Too many failures, need significant replanning
            return self.major_replan(plan, step_index).await;
        }

        // Try alternative approach
        let step = &mut plan.steps[step_index];

        // Add retry action
        step.actions.push(Action::Wait {
            condition: format!("Recover from: {}", error),
            timeout: Some(Duration::from_secs(30)),
        });

        // Reset step status
        step.status = StepStatus::Pending;
        step.error = None;

        Ok(())
    }

    /// Major replanning for critical failures
    async fn major_replan(&self, plan: &mut Plan, failed_step: usize) -> PlanningResult<()> {
        info!("Performing major replan for plan {}", plan.id);

        plan.status = PlanStatus::Replanning;

        // Get remaining goal
        let remaining_goal = plan
            .steps
            .iter()
            .skip(failed_step)
            .map(|s| s.description.clone())
            .collect::<Vec<_>>()
            .join(", ");

        // Create new subplan for remaining work
        let _context = DecompositionContext::new();
        let subplan = self.decomposer.quick_decompose(&remaining_goal)?;

        // Insert subplan steps
        let insert_idx = failed_step + 1;
        for (i, step) in subplan.steps.into_iter().enumerate() {
            let mut new_step = step;
            new_step.id = format!("replan-{}-{}", failed_step, i);
            plan.steps.insert(insert_idx + i, new_step);
        }

        // Mark original failed step as skipped
        plan.steps[failed_step].status = StepStatus::Skipped;

        // Rebuild dependencies
        plan.dependencies.clear();
        for i in 1..plan.steps.len() {
            if plan.steps[i].status != StepStatus::Skipped {
                plan.dependencies.insert(i, vec![i - 1]);
            }
        }

        plan.status = PlanStatus::InProgress;
        Ok(())
    }

    /// Handle goal change
    fn handle_goal_change(&self, plan: &mut Plan, new_goal: &str) -> PlanningResult<()> {
        info!("Adapting plan {} to new goal: {}", plan.id, new_goal);

        plan.status = PlanStatus::Replanning;
        plan.goal = new_goal.to_string();

        // Mark incomplete steps as pending for re-evaluation
        for step in &mut plan.steps {
            if !step.is_completed() {
                step.status = StepStatus::Pending;
            }
        }

        plan.status = PlanStatus::InProgress;
        Ok(())
    }
}

#[async_trait::async_trait]
impl RePlanner for FeedbackRePlanner {
    async fn should_replan(&self, _plan: &Plan, trigger: &RePlanTrigger) -> bool {
        matches!(trigger, RePlanTrigger::StepFailed { .. })
            || matches!(trigger, RePlanTrigger::ExternalFeedback { .. })
            || matches!(trigger, RePlanTrigger::GoalChanged { .. })
    }

    async fn replan(&self, plan: &mut Plan, trigger: &RePlanTrigger) -> PlanningResult<()> {
        match trigger {
            RePlanTrigger::StepFailed {
                step_index,
                error,
                attempt,
            } => {
                self.handle_step_failure(plan, *step_index, error, *attempt)
                    .await
            }
            RePlanTrigger::GoalChanged { new_goal, .. } => self.handle_goal_change(plan, new_goal),
            _ => Ok(()),
        }
    }

    fn name(&self) -> &str {
        "FeedbackRePlanner"
    }
}

impl Default for FeedbackRePlanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Resource-aware replanner
pub struct ResourceRePlanner;

impl ResourceRePlanner {
    /// Create new resource replanner
    pub fn new() -> Self {
        Self
    }

    /// Handle resource constraint
    fn handle_resource_constraint(
        &self,
        plan: &mut Plan,
        resource: &str,
        required: u64,
        available: u64,
    ) -> PlanningResult<()> {
        warn!(
            "Resource constraint in plan {}: {} required {} but only {} available",
            plan.id, resource, required, available
        );

        plan.status = PlanStatus::Replanning;

        // Strategy: reduce parallelism, extend timeouts, or skip non-critical steps
        let strategies = vec![
            "Reduce parallel execution",
            "Extend step timeouts",
            "Skip low-priority steps",
            "Request additional resources",
        ];

        // Add resource management step
        let resource_step = PlanStep::decision(format!(
            "Resolve resource constraint: {} (need {}, have {})",
            resource, required, available
        ))
        .with_action(Action::LLMReasoning {
            prompt: format!("Choose strategy for resource constraint: {:?}", strategies),
            context: HashMap::new(),
        });

        let current_idx = plan
            .steps
            .iter()
            .position(|s| s.status == StepStatus::InProgress)
            .unwrap_or(plan.steps.len());

        plan.steps.insert(current_idx, resource_step);

        plan.status = PlanStatus::InProgress;
        Ok(())
    }

    /// Handle timeout warning
    fn handle_timeout_warning(&self, plan: &mut Plan, remaining: Duration) -> PlanningResult<()> {
        warn!(
            "Timeout warning for plan {}: {:?} remaining",
            plan.id, remaining
        );

        // Identify non-critical steps that can be skipped
        let skip_candidates: Vec<usize> = plan
            .steps
            .iter()
            .enumerate()
            .filter(|(_, s)| s.status == StepStatus::Pending && s.priority == Priority::Low)
            .map(|(i, _)| i)
            .collect();

        if !skip_candidates.is_empty() {
            info!("Can skip {} low-priority steps", skip_candidates.len());

            // Add decision step for timeout handling
            let timeout_step = PlanStep::decision(format!(
                "Timeout approaching ({:?}). Consider skipping {} low-priority steps?",
                remaining,
                skip_candidates.len()
            ));

            let current_idx = plan
                .steps
                .iter()
                .position(|s| s.status == StepStatus::InProgress)
                .unwrap_or(plan.steps.len());

            plan.steps.insert(current_idx, timeout_step);
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl RePlanner for ResourceRePlanner {
    async fn should_replan(&self, _plan: &Plan, trigger: &RePlanTrigger) -> bool {
        matches!(trigger, RePlanTrigger::ResourceConstraint { .. })
            || matches!(trigger, RePlanTrigger::TimeoutWarning { .. })
    }

    async fn replan(&self, plan: &mut Plan, trigger: &RePlanTrigger) -> PlanningResult<()> {
        match trigger {
            RePlanTrigger::ResourceConstraint {
                resource,
                required,
                available,
            } => self.handle_resource_constraint(plan, resource, *required, *available),
            RePlanTrigger::TimeoutWarning { remaining } => {
                self.handle_timeout_warning(plan, *remaining)
            }
            _ => Ok(()),
        }
    }

    fn name(&self) -> &str {
        "ResourceRePlanner"
    }
}

impl Default for ResourceRePlanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Composite replanner using multiple strategies
pub struct CompositeRePlanner {
    replanners: Vec<Box<dyn RePlanner>>,
}

impl CompositeRePlanner {
    /// Create new composite replanner
    pub fn new() -> Self {
        Self {
            replanners: vec![
                Box::new(FeedbackRePlanner::new()),
                Box::new(ConditionRePlanner::new()),
                Box::new(ResourceRePlanner::new()),
            ],
        }
    }

    /// Add custom replanner
    pub fn add_replanner(&mut self, replanner: Box<dyn RePlanner>) {
        self.replanners.push(replanner);
    }
}

#[async_trait::async_trait]
impl RePlanner for CompositeRePlanner {
    async fn should_replan(&self, plan: &Plan, trigger: &RePlanTrigger) -> bool {
        for replanner in &self.replanners {
            if replanner.should_replan(plan, trigger).await {
                return true;
            }
        }
        false
    }

    async fn replan(&self, plan: &mut Plan, trigger: &RePlanTrigger) -> PlanningResult<()> {
        for replanner in &self.replanners {
            if replanner.should_replan(plan, trigger).await {
                replanner.replan(plan, trigger).await?;
            }
        }
        Ok(())
    }

    fn name(&self) -> &str {
        "CompositeRePlanner"
    }
}

impl Default for CompositeRePlanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Adaptation result
#[derive(Debug, Clone)]
pub struct AdaptationResult {
    /// Whether adaptation was successful
    pub success: bool,
    /// Strategy used
    pub strategy: AdaptationStrategy,
    /// Changes made
    pub changes: Vec<String>,
    /// New plan status
    pub new_status: PlanStatus,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_condition_replanner() {
        let replanner = ConditionRePlanner::new();
        let mut plan = Plan::new("Test", "Test goal");
        plan.add_step(PlanStep::new("1", "Step 1"));

        let trigger = RePlanTrigger::ConditionChanged {
            condition: "weather".to_string(),
            old_value: serde_json::json!("sunny"),
            new_value: serde_json::json!("rainy"),
        };

        assert!(replanner.should_replan(&plan, &trigger).await);

        let result = replanner.replan(&mut plan, &trigger).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_feedback_replanner() {
        let replanner = FeedbackRePlanner::new();
        let mut plan = Plan::new("Test", "Test goal");
        plan.add_step(PlanStep::new("1", "Step 1"));

        let trigger = RePlanTrigger::StepFailed {
            step_index: 0,
            error: "Network error".to_string(),
            attempt: 1,
        };

        assert!(replanner.should_replan(&plan, &trigger).await);

        let result = replanner.replan(&mut plan, &trigger).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_resource_replanner() {
        let replanner = ResourceRePlanner::new();
        let mut plan = Plan::new("Test", "Test goal");
        plan.add_step(PlanStep::new("1", "Step 1"));

        let trigger = RePlanTrigger::ResourceConstraint {
            resource: "memory".to_string(),
            required: 2048,
            available: 1024,
        };

        assert!(replanner.should_replan(&plan, &trigger).await);

        let result = replanner.replan(&mut plan, &trigger).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_trigger_severity() {
        let trigger = RePlanTrigger::StepFailed {
            step_index: 0,
            error: "Error".to_string(),
            attempt: 1,
        };
        assert_eq!(trigger.severity(), Priority::High);

        let trigger = RePlanTrigger::StepFailed {
            step_index: 0,
            error: "Error".to_string(),
            attempt: 3,
        };
        assert_eq!(trigger.severity(), Priority::Critical);

        let trigger = RePlanTrigger::GoalChanged {
            new_goal: "New goal".to_string(),
            reason: "Test".to_string(),
        };
        assert_eq!(trigger.severity(), Priority::Critical);
    }
}
