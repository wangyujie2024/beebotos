//! Agent with Planning Integration Example
//!
//! 🆕 PLANNING FIX: This example demonstrates how to use the integrated
//! planning module with the Agent for autonomous task planning and execution.

use std::sync::Arc;

use beebotos_agents::{
    Agent, AgentConfig, ExecutionConfig, ExecutionStrategy, PlanExecutor, PlanStrategy,
    PlanningEngine, PlanningResult,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🤖 BeeBotOS Agent with Planning Integration Demo\n");

    // Create agent configuration
    let config = AgentConfig::default();

    // Create planning components
    let planning_engine = Arc::new(PlanningEngine::new());
    let plan_executor = Arc::new(PlanExecutor::with_config(ExecutionConfig {
        strategy: ExecutionStrategy::Sequential,
        max_retries: 2,
        ..Default::default()
    }));

    // Create agent with planning capabilities
    let agent = Agent::new(config)
        .with_planning_engine(planning_engine)
        .with_plan_executor(plan_executor);

    println!("✅ Agent created with planning capabilities\n");

    // Example 1: Direct plan creation and execution
    println!("📋 Example 1: Direct Plan Creation");
    let plan = agent
        .create_plan(
            "Analyze system logs and generate report",
            PlanStrategy::ReAct,
        )
        .await?;
    println!(
        "Created plan: {} with {} steps",
        plan.name,
        plan.steps.len()
    );

    // Example 2: Execute plan
    println!("\n📋 Example 2: Execute Plan");
    let result = agent.execute_plan(&plan).await?;
    println!("Plan execution result: success={}", result.success);

    // Example 3: List active plans
    println!("\n📋 Example 3: List Active Plans");
    let active_plans = agent.list_active_plans().await;
    println!("Active plans: {}", active_plans.len());

    // Example 4: Execute task with automatic planning (complex task)
    println!("\n📋 Example 4: Automatic Planning for Complex Task");
    use std::collections::HashMap;

    use beebotos_agents::{Task, TaskType};

    let complex_task = Task {
        id: "task-001".to_string(),
        task_type: TaskType::Custom("complex_analysis".to_string()),
        input: "Design and implement a user authentication system with the following \
                requirements:\n1. Support OAuth2 and SAML\n2. Implement JWT token management\n3. \
                Add rate limiting\n4. Create audit logs"
            .to_string(),
        parameters: {
            let mut params = HashMap::new();
            params.insert("use_planning".to_string(), "true".to_string());
            params.insert("strategy".to_string(), "hybrid".to_string());
            params
        },
    };

    // Analyze task complexity
    let complexity = agent.analyze_task_complexity(&complex_task).await;
    println!("Task complexity: {:?}", complexity);

    let should_plan = agent.should_use_planning(&complex_task).await;
    println!("Should use planning: {}", should_plan);

    // Example 5: Planning-specific tasks
    println!("\n📋 Example 5: Planning-Specific Task Types");

    // Create plan task
    let create_plan_task = Task {
        id: "task-002".to_string(),
        task_type: TaskType::PlanCreation,
        input: "Optimize database query performance".to_string(),
        parameters: {
            let mut params = HashMap::new();
            params.insert("strategy".to_string(), "goal_based".to_string());
            params
        },
    };

    println!("Created plan task: {:?}", create_plan_task.task_type);

    // Example 6: Execute plan task
    let exec_plan_task = Task {
        id: "task-003".to_string(),
        task_type: TaskType::PlanExecution,
        input: "".to_string(),
        parameters: {
            let mut params = HashMap::new();
            params.insert("plan_id".to_string(), plan.id.to_string());
            params
        },
    };

    println!("Execute plan task: {:?}", exec_plan_task.task_type);

    println!("\n✅ Agent Planning Integration Demo completed!");

    Ok(())
}
