//! Planning Module Example
//!
//! Demonstrates the new planning capabilities of the BeeBotOS agents module.

use beebotos_agents::{
    ChainOfThoughtPlanner, Decomposer, DecompositionContext, DecompositionStrategy,
    ExecutionConfig, ExecutionStrategy, GoalBasedPlanner, HybridPlanner, Plan, PlanContext,
    PlanExecutor, PlanStatus, PlanStep, PlanStrategy, Planner, PlanningEngine, Priority,
    ReActPlanner, StepType,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🤖 BeeBotOS Planning Module Demo\n");

    // Example 1: Quick Plan using PlanningEngine
    println!("📋 Example 1: Quick Plan Creation");
    let engine = PlanningEngine::new();

    let plan = engine
        .quick_plan("Analyze system performance and generate optimization report")
        .await?;
    println!("Created plan: {}", plan.name);
    println!("Goal: {}", plan.goal);
    println!("Steps: {}", plan.steps.len());
    for (i, step) in plan.steps.iter().enumerate() {
        println!("  {}. {} ({:?})", i + 1, step.description, step.step_type);
    }
    println!();

    // Example 2: Different Planning Strategies
    println!("📋 Example 2: Different Planning Strategies");

    let context = PlanContext::new("demo-agent")
        .with_tool("search")
        .with_tool("analyze")
        .with_tool("report");

    // ReAct Strategy
    let react_plan = engine
        .create_plan(
            "Fix the authentication bug",
            &context,
            Some(PlanStrategy::ReAct),
        )
        .await?;
    println!("ReAct Plan: {} steps", react_plan.steps.len());

    // Chain-of-Thought Strategy
    let cot_plan = engine
        .create_plan(
            "Solve complex mathematical optimization",
            &context,
            Some(PlanStrategy::ChainOfThought),
        )
        .await?;
    println!("CoT Plan: {} steps", cot_plan.steps.len());

    // Goal-Based Strategy
    let goal_plan = engine
        .create_plan(
            "Achieve 99.9% system availability",
            &context,
            Some(PlanStrategy::GoalBased),
        )
        .await?;
    println!("Goal-Based Plan: {} steps", goal_plan.steps.len());
    println!();

    // Example 3: Task Decomposition
    println!("📋 Example 3: Task Decomposition");
    let decomposer = Decomposer::new();

    let goals = vec![
        "Implement user authentication system",
        "Research and compare database options",
        "Build data analysis pipeline",
    ];

    for goal in goals {
        let plan = decomposer.quick_decompose(goal)?;
        println!("Goal: {}", goal);
        println!("  Decomposed into {} steps", plan.steps.len());
        for step in &plan.steps {
            println!("    - {}", step.description);
        }
    }
    println!();

    // Example 4: Custom Plan with Dependencies
    println!("📋 Example 4: Custom Plan with Dependencies");
    let mut custom_plan = Plan::new("Custom Workflow", "Deploy microservices");

    // Add steps with dependencies
    let step1 = custom_plan.add_step(PlanStep::new("design", "Design service architecture"));
    let step2 = custom_plan.add_step(PlanStep::new("implement", "Implement services"));
    let step3 = custom_plan
        .add_step_with_deps(PlanStep::new("test", "Test services"), vec![step1, step2])?;
    let _ = custom_plan
        .add_step_with_deps(PlanStep::new("deploy", "Deploy to production"), vec![step3])?;

    println!("Plan: {}", custom_plan.name);
    println!("Dependencies:");
    for (step_idx, deps) in &custom_plan.dependencies {
        println!("  Step {} depends on: {:?}", step_idx, deps);
    }
    println!();

    // Example 5: Plan Execution
    println!("📋 Example 5: Plan Execution");
    let mut exec_plan = Plan::new("Execution Demo", "Simple task");
    exec_plan.add_step(PlanStep::new("gather", "Gather requirements"));
    exec_plan.add_step(PlanStep::new("analyze", "Analyze requirements"));
    exec_plan.add_step(PlanStep::new("implement", "Implement solution"));

    let executor = PlanExecutor::with_config(ExecutionConfig {
        strategy: ExecutionStrategy::Adaptive,
        max_retries: 2,
        ..Default::default()
    });

    println!("Executing plan: {}", exec_plan.name);
    let result = executor.execute(&mut exec_plan).await?;
    println!("Execution result: success={}", result.success);
    println!("Final plan status: {:?}", exec_plan.status);
    println!("Completion: {:.1}%", exec_plan.completion_pct());
    println!();

    // Example 6: Priority-based Planning
    println!("📋 Example 6: Priority-based Planning");
    let mut priority_plan = Plan::new("Priority Demo", "Handle critical incident");
    priority_plan.priority = Priority::Critical;

    let mut critical_step = PlanStep::new("fix", "Fix critical bug");
    critical_step.priority = Priority::Critical;
    priority_plan.add_step(critical_step);

    let mut normal_step = PlanStep::new("document", "Document the fix");
    normal_step.priority = Priority::Normal;
    priority_plan.add_step(normal_step);

    println!("Plan priority: {:?}", priority_plan.priority);
    for step in &priority_plan.steps {
        println!("  Step '{}' priority: {:?}", step.id, step.priority);
    }
    println!();

    println!("✅ Planning module demo completed!");
    Ok(())
}
