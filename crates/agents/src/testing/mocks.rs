//! Mock implementations for testing planning integration
//!
//! 🆕 OPTIMIZATION: Provides test doubles for PlanningEngine, PlanExecutor, and
//! RePlanner

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::planning::*;
use crate::task::{Task, TaskType};

/// Mock PlanningEngine for testing
///
/// 🆕 OPTIMIZATION: Provides controllable behavior for testing planning
/// workflows
pub struct MockPlanningEngine {
    plan_count: AtomicUsize,
    should_fail: bool,
    fail_message: String,
}

impl MockPlanningEngine {
    pub fn new() -> Self {
        Self {
            plan_count: AtomicUsize::new(0),
            should_fail: false,
            fail_message: "Mock planning failure".to_string(),
        }
    }

    pub fn with_failure(self, message: impl Into<String>) -> Self {
        Self {
            should_fail: true,
            fail_message: message.into(),
            ..self
        }
    }

    pub fn get_plan_count(&self) -> usize {
        self.plan_count.load(Ordering::SeqCst)
    }

    pub fn reset(&self) {
        self.plan_count.store(0, Ordering::SeqCst);
    }
}

impl Default for MockPlanningEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl crate::planning::Planner for MockPlanningEngine {
    async fn plan(&self, goal: &str, _context: &PlanContext) -> PlanningResult<Plan> {
        if self.should_fail {
            return Err(PlanningError::ExecutionFailed(self.fail_message.clone()));
        }

        self.plan_count.fetch_add(1, Ordering::SeqCst);

        let mut plan = Plan::new(
            format!("Mock Plan {}", self.plan_count.load(Ordering::SeqCst)),
            goal,
        );

        // Add mock steps based on goal content
        if goal.contains("multi") || goal.contains("complex") {
            plan.add_step(PlanStep::new("step1", "First step"));
            plan.add_step(PlanStep::new("step2", "Second step"));
            plan.add_step(PlanStep::new("step3", "Third step"));
        } else if goal.contains("error") || goal.contains("fail") {
            plan.add_step(PlanStep::new("error_step", "Step that will fail"));
        } else {
            plan.add_step(PlanStep::new("single", "Single step"));
        }

        Ok(plan)
    }

    fn name(&self) -> &str {
        "MockPlanningEngine"
    }
}

/// Mock PlanExecutor for testing
///
/// 🆕 OPTIMIZATION: Provides controllable execution behavior for testing
pub struct MockPlanExecutor {
    execution_count: AtomicUsize,
    fail_on_step: Option<usize>,
    execution_delay_ms: u64,
}

impl MockPlanExecutor {
    pub fn new() -> Self {
        Self {
            execution_count: AtomicUsize::new(0),
            fail_on_step: None,
            execution_delay_ms: 0,
        }
    }

    pub fn with_failure_on_step(self, step: usize) -> Self {
        Self {
            fail_on_step: Some(step),
            ..self
        }
    }

    pub fn with_delay(self, delay_ms: u64) -> Self {
        Self {
            execution_delay_ms: delay_ms,
            ..self
        }
    }

    pub fn get_execution_count(&self) -> usize {
        self.execution_count.load(Ordering::SeqCst)
    }

    pub fn reset(&self) {
        self.execution_count.store(0, Ordering::SeqCst);
    }
}

impl Default for MockPlanExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl MockPlanExecutor {
    /// Execute plan with mock behavior
    pub async fn execute(&self, plan: &mut Plan) -> PlanningResult<ExecutionResult> {
        self.execution_count.fetch_add(1, Ordering::SeqCst);

        if self.execution_delay_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(self.execution_delay_ms)).await;
        }

        for (idx, step) in plan.steps.iter_mut().enumerate() {
            if self.fail_on_step == Some(idx) {
                step.status = StepStatus::Failed;
                plan.status = PlanStatus::Failed;
                return Ok(ExecutionResult {
                    success: false,
                    error: Some(format!("Failed at step {}", idx)),
                    data: None,
                    duration_ms: 0,
                    attempts: 1,
                });
            }
            step.status = StepStatus::Completed;
        }

        plan.status = PlanStatus::Completed;

        Ok(ExecutionResult {
            success: true,
            data: Some(serde_json::json!({
                "steps_executed": plan.steps.len(),
                "mock_execution": true
            })),
            error: None,
            duration_ms: self.execution_delay_ms,
            attempts: 1,
        })
    }
}

/// Mock RePlanner for testing
///
/// 🆕 OPTIMIZATION: Provides controllable replanning behavior
pub struct MockRePlanner {
    should_adapt: bool,
    adaptation_count: AtomicUsize,
}

impl MockRePlanner {
    pub fn new() -> Self {
        Self {
            should_adapt: true,
            adaptation_count: AtomicUsize::new(0),
        }
    }

    pub fn with_no_adaptation(self) -> Self {
        Self {
            should_adapt: false,
            ..self
        }
    }

    pub fn get_adaptation_count(&self) -> usize {
        self.adaptation_count.load(Ordering::SeqCst)
    }
}

impl Default for MockRePlanner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl RePlanner for MockRePlanner {
    async fn should_replan(&self, _plan: &Plan, _trigger: &RePlanTrigger) -> bool {
        self.should_adapt
    }

    async fn replan(&self, plan: &mut Plan, trigger: &RePlanTrigger) -> PlanningResult<()> {
        if !self.should_adapt {
            return Err(PlanningError::ExecutionFailed(
                "Adaptation disabled".to_string(),
            ));
        }

        self.adaptation_count.fetch_add(1, Ordering::SeqCst);

        // Add an adaptation marker step
        plan.steps.push(PlanStep::new(
            "adaptation",
            format!("Adapted due to: {:?}", trigger),
        ));

        Ok(())
    }

    fn name(&self) -> &str {
        "MockRePlanner"
    }
}

/// Test helper functions
///
/// 🆕 OPTIMIZATION: Convenient helpers for creating test scenarios
pub mod helpers {
    use super::*;

    /// Create a simple test task
    pub fn create_simple_task(id: &str, input: &str) -> Task {
        Task {
            id: id.to_string(),
            task_type: TaskType::LlmChat,
            input: input.to_string(),
            parameters: std::collections::HashMap::new(),
        }
    }

    /// Create a complex test task (will trigger planning)
    pub fn create_complex_task(id: &str) -> Task {
        Task {
            id: id.to_string(),
            task_type: TaskType::LlmChat,
            input: "x".repeat(250), // Long input triggers complexity
            parameters: std::collections::HashMap::new(),
        }
    }

    /// Create a task with explicit planning flag
    pub fn create_task_with_planning(id: &str, input: &str) -> Task {
        let mut params = std::collections::HashMap::new();
        params.insert("use_planning".to_string(), "true".to_string());

        Task {
            id: id.to_string(),
            task_type: TaskType::LlmChat,
            input: input.to_string(),
            parameters: params,
        }
    }

    /// Create a test plan with specified number of steps
    pub fn create_test_plan(name: &str, num_steps: usize) -> Plan {
        let mut plan = Plan::new(name, format!("Test goal for {}", name));

        for i in 0..num_steps {
            plan.add_step(PlanStep::new(
                format!("step{}", i),
                format!("Test step {}", i),
            ));
        }

        plan
    }

    /// Create a test plan with dependencies
    pub fn create_test_plan_with_deps(name: &str) -> Plan {
        let mut plan = Plan::new(name, "Test goal with dependencies");

        // Add steps with dependencies
        let step0 = plan.add_step(PlanStep::new("step0", "Independent step 0"));
        let step1 = plan.add_step(PlanStep::new("step1", "Independent step 1"));

        // Step 2 depends on step 0 and 1
        let step2 = plan
            .add_step_with_deps(
                PlanStep::new("step2", "Depends on 0 and 1"),
                vec![step0, step1],
            )
            .unwrap();

        // Step 3 depends on step 2
        plan.add_step_with_deps(PlanStep::new("step3", "Depends on 2"), vec![step2])
            .unwrap();

        plan
    }
}

#[cfg(test)]
mod mock_tests {
    use super::helpers::*;
    use super::*;

    #[tokio::test]
    async fn test_mock_planning_engine() {
        let engine = MockPlanningEngine::new();

        let context = PlanContext::new("test-agent");
        let plan = engine.plan("test goal", &context).await.unwrap();

        assert_eq!(plan.steps.len(), 1);
        assert_eq!(engine.get_plan_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_planning_engine_failure() {
        let engine = MockPlanningEngine::new().with_failure("Expected failure");

        let context = PlanContext::new("test-agent");
        let result = engine.plan("test goal", &context).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_plan_executor() {
        let executor = MockPlanExecutor::new();
        let mut plan = create_test_plan("test", 3);

        let result = executor.execute(&mut plan).await.unwrap();

        assert!(result.success);
        assert_eq!(executor.get_execution_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_plan_executor_failure() {
        let executor = MockPlanExecutor::new().with_failure_on_step(1);
        let mut plan = create_test_plan("test", 3);

        let result = executor.execute(&mut plan).await.unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("Failed at step 1"));
    }

    #[tokio::test]
    async fn test_mock_replanner() {
        let replanner = MockRePlanner::new();
        let mut plan = create_test_plan("test", 2);
        let trigger = RePlanTrigger::GoalChanged {
            new_goal: "New goal".to_string(),
            reason: "Test".to_string(),
        };

        assert!(replanner.should_replan(&plan, &trigger).await);

        let result = replanner.replan(&mut plan, &trigger).await;
        assert!(result.is_ok());
        assert_eq!(plan.steps.len(), 3); // Original 2 + 1 adaptation step
        assert_eq!(replanner.get_adaptation_count(), 1);
    }

    #[test]
    fn test_helper_functions() {
        let simple = create_simple_task("t1", "Hello");
        assert_eq!(simple.input, "Hello");

        let complex = create_complex_task("t2");
        assert!(complex.input.len() > 200);

        let with_planning = create_task_with_planning("t3", "Test");
        assert_eq!(
            with_planning.parameters.get("use_planning"),
            Some(&"true".to_string())
        );

        let plan = create_test_plan("test", 5);
        assert_eq!(plan.steps.len(), 5);

        let plan_with_deps = create_test_plan_with_deps("deps");
        assert_eq!(plan_with_deps.steps.len(), 4);
        assert!(!plan_with_deps.dependencies.is_empty());
    }
}
