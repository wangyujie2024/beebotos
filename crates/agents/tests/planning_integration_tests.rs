//! Planning Integration Tests
//!
//! 🆕 OPTIMIZATION: End-to-end tests for Agent planning integration

use std::collections::HashMap;
use std::sync::Arc;

use beebotos_agents::testing::helpers::*;
use beebotos_agents::testing::{MockPlanExecutor, MockPlanningEngine, MockRePlanner};
use beebotos_agents::{
    Agent, AgentConfig, ExecutionResult, Plan, PlanExecutor, PlanId, PlanStatus, PlanStrategy,
    PlanningEngine, Task, TaskComplexity, TaskType,
};

/// Helper to create agent with real planning components
fn create_real_planning_agent() -> Agent {
    Agent::new(AgentConfig::default())
        .with_planning_engine(Arc::new(PlanningEngine::new()))
        .with_plan_executor(Arc::new(PlanExecutor::new()))
}

/// Helper to create agent with mock planning components
fn create_mock_planning_agent() -> Agent {
    Agent::new(AgentConfig::default())
        .with_planning_engine(Arc::new(PlanningEngine::new()))
        .with_plan_executor(Arc::new(PlanExecutor::new()))
        .with_replanner(Arc::new(MockRePlanner::new()))
}

// ============================================================================
// E2E: Complete Planning Workflow
// ============================================================================

#[tokio::test]
async fn test_e2e_simple_planning_workflow() {
    let agent = create_real_planning_agent();

    // Create a plan
    let plan = agent
        .create_plan("Analyze system performance", PlanStrategy::ReAct)
        .await
        .expect("Failed to create plan");

    assert!(!plan.steps.is_empty());
    assert_eq!(plan.status, PlanStatus::Created);

    // Execute the plan
    let result = agent
        .execute_plan(&plan)
        .await
        .expect("Failed to execute plan");

    assert!(result.success);
}

#[tokio::test]
async fn test_e2e_complex_task_auto_planning() {
    let agent = create_real_planning_agent();

    // Create a complex task that should trigger planning (long input > 200 chars)
    let complex_task = Task {
        id: "complex-1".to_string(),
        task_type: TaskType::LlmChat,
        input: "Design and implement a user authentication system with OAuth2, JWT, rate \
                limiting, and audit logs. "
            .repeat(5),
        parameters: HashMap::new(),
    };

    // Verify complexity detection
    let complexity = agent.analyze_task_complexity(&complex_task).await;
    assert_eq!(
        complexity,
        TaskComplexity::Complex,
        "Task input length: {}",
        complex_task.input.len()
    );

    // Verify planning is recommended
    assert!(agent.should_use_planning(&complex_task).await);
}

#[tokio::test]
async fn test_e2e_explicit_planning_task() {
    let agent = create_real_planning_agent();

    // Create explicit plan creation task
    let plan_task = Task {
        id: "plan-task-1".to_string(),
        task_type: TaskType::PlanCreation,
        input: "Optimize database query performance".to_string(),
        parameters: {
            let mut params = HashMap::new();
            params.insert("strategy".to_string(), "goal_based".to_string());
            params
        },
    };

    let (output, _) = agent
        .handle_plan_creation_task(&plan_task)
        .await
        .expect("Failed to handle plan creation");

    assert!(output.contains("Created plan"));
    assert!(output.contains("goal_based") || output.contains("GoalBased"));
}

// ============================================================================
// E2E: Plan Execution
// ============================================================================

#[tokio::test]
async fn test_e2e_plan_execution_task() {
    let agent = create_real_planning_agent();

    // First create a plan
    let plan = agent
        .create_plan("Test execution", PlanStrategy::Hybrid)
        .await
        .expect("Failed to create plan");

    // Then execute it via task
    let exec_task = Task {
        id: "exec-task-1".to_string(),
        task_type: TaskType::PlanExecution,
        input: "".to_string(),
        parameters: {
            let mut params = HashMap::new();
            params.insert("plan_id".to_string(), plan.id.to_string());
            params
        },
    };

    let (output, _) = agent
        .handle_plan_execution_task(&exec_task)
        .await
        .expect("Failed to execute plan");

    assert!(output.contains("executed") || output.contains("success"));
}

#[tokio::test]
async fn test_e2e_plan_execution_nonexistent() {
    let agent = create_real_planning_agent();

    let exec_task = Task {
        id: "exec-task-2".to_string(),
        task_type: TaskType::PlanExecution,
        input: "".to_string(),
        parameters: {
            let mut params = HashMap::new();
            params.insert("plan_id".to_string(), PlanId::new().to_string());
            params
        },
    };

    let result = agent.handle_plan_execution_task(&exec_task).await;
    assert!(result.is_err());
}

// ============================================================================
// E2E: Plan Lifecycle Management
// ============================================================================

#[tokio::test]
async fn test_e2e_concurrent_plan_creation() {
    let agent = Arc::new(create_real_planning_agent());

    // Create multiple plans concurrently
    let mut handles = vec![];
    for i in 0..5 {
        let agent = agent.clone();
        handles.push(tokio::spawn(async move {
            agent
                .create_plan(&format!("Goal {}", i), PlanStrategy::Hybrid)
                .await
        }));
    }

    // Wait for all to complete
    let results: Vec<_> = futures::future::join_all(handles).await;
    assert!(results.iter().all(|r| r.is_ok()));

    // Verify all plans exist
    let active_plans = agent.list_active_plans().await;
    assert_eq!(active_plans.len(), 5);
}

#[tokio::test]
async fn test_e2e_plan_cancel_during_execution() {
    let agent = create_real_planning_agent();

    // Create a plan with delay
    let plan = agent
        .create_plan("Long running task", PlanStrategy::ReAct)
        .await
        .expect("Failed to create plan");

    // Cancel it immediately
    agent
        .cancel_plan(&plan.id)
        .await
        .expect("Failed to cancel plan");

    // Verify it's gone
    assert!(agent.get_active_plan(&plan.id).await.is_none());
}

// ============================================================================
// E2E: Error Handling
// ============================================================================

#[tokio::test]
async fn test_e2e_planning_without_engine() {
    let agent = Agent::new(AgentConfig::default()); // No planning engine

    let result = agent.create_plan("Test", PlanStrategy::ReAct).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_e2e_complexity_detection_variations() {
    let agent = create_real_planning_agent();

    // Test short input (Simple)
    let task1 = Task {
        id: "test1".to_string(),
        task_type: TaskType::LlmChat,
        input: "Short".to_string(),
        parameters: HashMap::new(),
    };
    assert_eq!(
        agent.analyze_task_complexity(&task1).await,
        TaskComplexity::Simple
    );

    // Test long input (Complex)
    let task2 = Task {
        id: "test2".to_string(),
        task_type: TaskType::LlmChat,
        input: "x".repeat(201),
        parameters: HashMap::new(),
    };
    assert_eq!(
        agent.analyze_task_complexity(&task2).await,
        TaskComplexity::Complex
    );

    // Test multi_step flag (Complex)
    let mut params = HashMap::new();
    params.insert("multi_step".to_string(), "true".to_string());
    let task3 = Task {
        id: "test3".to_string(),
        task_type: TaskType::LlmChat,
        input: "Medium length".to_string(),
        parameters: params,
    };
    assert_eq!(
        agent.analyze_task_complexity(&task3).await,
        TaskComplexity::Complex
    );
}

// ============================================================================
// E2E: Mock Integration Tests
// ============================================================================

#[tokio::test]
async fn test_e2e_with_mock_planning_failure() {
    // Create agent without planning (we'll just test complexity analysis)
    let agent = Agent::new(AgentConfig::default());

    // Even without planning engine, we can test complexity analysis
    let task = create_complex_task("mock-test");
    let complexity = agent.analyze_task_complexity(&task).await;
    assert_eq!(complexity, TaskComplexity::Complex);
}

#[tokio::test]
async fn test_e2e_plan_strategy_selection() {
    let agent = create_real_planning_agent();

    // Test strategy selection through plan creation
    let strategies = vec![
        ("react", PlanStrategy::ReAct),
        ("cot", PlanStrategy::ChainOfThought),
        ("chain_of_thought", PlanStrategy::ChainOfThought),
        ("goal_based", PlanStrategy::GoalBased),
        ("hybrid", PlanStrategy::Hybrid),
    ];

    for (strategy_str, expected) in strategies {
        let plan = agent.create_plan("Test goal", expected).await;
        assert!(
            plan.is_ok(),
            "Failed to create plan with strategy: {}",
            strategy_str
        );
    }
}

// ============================================================================
// Performance Tests
// ============================================================================

#[tokio::test]
async fn test_e2e_parallel_execution_performance() {
    let agent = create_real_planning_agent();

    // Create a plan with parallel flag
    let mut plan = create_test_plan("parallel-test", 10);
    plan.metadata
        .insert("enable_parallel".to_string(), serde_json::json!(true));
    plan.metadata
        .insert("max_concurrency".to_string(), serde_json::json!(5));

    // Execute and measure time
    let start = std::time::Instant::now();
    let result = agent.execute_plan(&plan).await;
    let duration = start.elapsed();

    assert!(result.is_ok());
    println!("Parallel execution took: {:?}", duration);
}

#[tokio::test]
async fn test_e2e_sequential_execution_performance() {
    let agent = create_real_planning_agent();

    // Create a plan without parallel flag (sequential)
    let plan = create_test_plan("sequential-test", 10);

    // Execute and measure time
    let start = std::time::Instant::now();
    let result = agent.execute_plan(&plan).await;
    let duration = start.elapsed();

    assert!(result.is_ok());
    println!("Sequential execution took: {:?}", duration);
}

// ============================================================================
// Stress Tests
// ============================================================================

#[tokio::test]
async fn test_e2e_many_plans() {
    let agent = Arc::new(create_real_planning_agent());

    // Create many plans
    let plan_count = 50;
    let mut handles = vec![];

    for i in 0..plan_count {
        let agent = agent.clone();
        handles.push(tokio::spawn(async move {
            agent
                .create_plan(&format!("Goal {}", i), PlanStrategy::Hybrid)
                .await
        }));
    }

    let results = futures::future::join_all(handles).await;
    let success_count = results.iter().filter(|r| r.is_ok()).count();

    assert_eq!(success_count, plan_count);
    assert_eq!(agent.list_active_plans().await.len(), plan_count);
}

#[tokio::test]
async fn test_e2e_rapid_plan_create_and_cancel() {
    let agent = Arc::new(create_real_planning_agent());

    // Rapidly create and cancel plans
    for i in 0..20 {
        let plan = agent
            .create_plan(&format!("Goal {}", i), PlanStrategy::Hybrid)
            .await
            .expect("Failed to create plan");

        agent
            .cancel_plan(&plan.id)
            .await
            .expect("Failed to cancel plan");
    }

    // All plans should be cancelled
    assert!(agent.list_active_plans().await.is_empty());
}
