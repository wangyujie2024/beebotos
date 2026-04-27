//! Advanced Planning Module Example
//!
//! Demonstrates advanced features:
//! - Dynamic replanning
//! - Domain-specific decomposition
//! - Execution events
//! - Composite strategies

use std::time::Duration;

use beebotos_agents::{
    CompositeDecomposer, CompositeRePlanner, ConditionRePlanner, Decomposer, DecompositionContext,
    DecompositionStrategy, DomainDecomposer, ExecutionConfig, ExecutionEvent, ExecutionResult,
    ExecutionStrategy, FeedbackRePlanner, HybridPlanner, ParallelDecomposer, ParallelExecutor,
    Plan, PlanContext, PlanExecutor, PlanId, PlanStatus, PlanStep, PlanStrategy, Planner,
    PlanningEngine, PlanningResult, Priority, RePlanTrigger, RePlanner, ResourceRePlanner,
    SequentialExecutor, StepStatus, StepType, TaskDecomposer,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🤖 BeeBotOS Advanced Planning Demo\n");

    // Example 1: Domain-Specific Decomposition
    println!("📋 Example 1: Domain-Specific Decomposition");
    let mut domain_decomposer = DomainDecomposer::new();

    // Add custom pattern
    domain_decomposer.add_pattern(
        "security_audit",
        vec![
            PlanStep::new("scope", "Define audit scope"),
            PlanStep::new("discover", "Discover assets"),
            PlanStep::new("scan", "Run vulnerability scans"),
            PlanStep::new("analyze", "Analyze findings"),
            PlanStep::new("report", "Generate security report"),
        ],
    );

    let context = DecompositionContext::new()
        .with_tool("vuln_scanner")
        .with_tool("asset_discovery")
        .with_max_depth(3);

    let plan = domain_decomposer.decompose("Perform security audit of the API", &context)?;

    println!("Security audit plan:");
    for (i, step) in plan.steps.iter().enumerate() {
        println!("  {}. {}", i + 1, step.description);
    }
    println!();

    // Example 2: Parallel Decomposition
    println!("📋 Example 2: Parallel Decomposition");
    let parallel_decomposer = ParallelDecomposer::new();

    let plan = parallel_decomposer.decompose(
        "Update frontend, backend, and database simultaneously",
        &DecompositionContext::new(),
    )?;

    println!("Parallel tasks:");
    for step in &plan.steps {
        println!("  - {}", step.description);
    }
    println!("All tasks can run in parallel!\n");

    // Example 3: Hybrid Planning
    println!("📋 Example 3: Hybrid Planning with Adaptive Strategy");
    let hybrid_planner = HybridPlanner::new();
    let context = PlanContext::new("adaptive-agent")
        .with_tool("analyze")
        .with_tool("implement")
        .with_tool("test");

    let plan = hybrid_planner
        .plan("Implement a new feature with fallback strategy", &context)
        .await?;

    println!("Hybrid plan with {} steps:", plan.steps.len());
    for (i, step) in plan.steps.iter().enumerate() {
        let icon = match step.step_type {
            StepType::Reasoning => "💭",
            StepType::Action => "⚡",
            StepType::Decision => "🎯",
            _ => "•",
        };
        println!("  {} {}. {}", icon, i + 1, step.description);
    }
    println!();

    // Example 4: Execution with Event Monitoring
    println!("📋 Example 4: Execution with Event Monitoring");
    let mut plan = Plan::new("Monitored Execution", "Process data pipeline");
    plan.add_step(PlanStep::new("extract", "Extract data from sources"));
    plan.add_step(PlanStep::new("transform", "Transform data format"));
    plan.add_step(PlanStep::new("load", "Load into database"));

    let executor = PlanExecutor::with_config(ExecutionConfig {
        strategy: ExecutionStrategy::Sequential,
        step_timeout: Duration::from_secs(30),
        max_retries: 2,
        ..Default::default()
    });

    // Subscribe to events
    let mut event_rx = executor.subscribe();

    // Monitor events in background
    let monitor = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                ExecutionEvent::PlanStarted { plan_id } => {
                    println!("  📊 Plan {} started", plan_id);
                }
                ExecutionEvent::StepStarted { step_index, .. } => {
                    println!("  ▶️  Step {} started", step_index + 1);
                }
                ExecutionEvent::StepCompleted {
                    step_index, result, ..
                } => {
                    let status = if result.success { "✅" } else { "❌" };
                    println!("  {} Step {} completed", status, step_index + 1);
                }
                ExecutionEvent::PlanCompleted { success, .. } => {
                    let status = if success { "✅" } else { "❌" };
                    println!("  {} Plan completed", status);
                    break;
                }
                _ => {}
            }
        }
    });

    println!("Executing with monitoring...");
    let _ = executor.execute(&mut plan).await;
    let _ = tokio::time::timeout(Duration::from_secs(5), monitor).await;
    println!();

    // Example 5: Replanning Scenarios
    println!("📋 Example 5: Replanning Triggers");
    let mut replan_plan = Plan::new("Adaptive Plan", "Complete critical task");
    replan_plan.add_step(PlanStep::new("step1", "Initial step"));
    replan_plan.add_step(PlanStep::new("step2", "Secondary step"));
    replan_plan.add_step(PlanStep::new("step3", "Final step"));

    let composite_replanner = CompositeRePlanner::new();

    // Simulate various replanning triggers
    let triggers = vec![
        RePlanTrigger::StepFailed {
            step_index: 1,
            error: "Network timeout".to_string(),
            attempt: 2,
        },
        RePlanTrigger::ConditionChanged {
            condition: "API rate limit".to_string(),
            old_value: serde_json::json!(100),
            new_value: serde_json::json!(10),
        },
        RePlanTrigger::ResourceConstraint {
            resource: "memory".to_string(),
            required: 8192,
            available: 4096,
        },
    ];

    for trigger in &triggers {
        println!("  Trigger: {}", trigger.description());
        println!("  Severity: {:?}", trigger.severity());

        if composite_replanner
            .should_replan(&replan_plan, trigger)
            .await
        {
            println!("  🔄 Replanning recommended");
        } else {
            println!("  ⏭️  Continue with current plan");
        }
        println!();
    }

    // Example 6: Priority Management
    println!("📋 Example 6: Multi-Priority Plan Management");
    let mut priority_plan = Plan::new("Multi-Priority Tasks", "Handle various tasks");
    priority_plan.priority = Priority::High;

    // Add steps with different priorities
    let mut critical = PlanStep::new("p0", "Fix production outage");
    critical.priority = Priority::Critical;
    priority_plan.add_step(critical);

    let mut high = PlanStep::new("p1", "Implement urgent feature");
    high.priority = Priority::High;
    priority_plan.add_step(high);

    let mut normal = PlanStep::new("p2", "Update documentation");
    normal.priority = Priority::Normal;
    priority_plan.add_step(normal);

    let mut low = PlanStep::new("p3", "Refactor code style");
    low.priority = Priority::Low;
    priority_plan.add_step(low);

    println!("Plan with mixed priorities:");
    for step in &priority_plan.steps {
        let emoji = match step.priority {
            Priority::Critical => "🔴",
            Priority::High => "🟠",
            Priority::Normal => "🟡",
            Priority::Low => "🟢",
        };
        println!(
            "  {} [{}] {}",
            emoji,
            format!("{:?}", step.priority).to_uppercase(),
            step.description
        );
    }
    println!();

    // Example 7: Plan Persistence (Serialization)
    println!("📋 Example 7: Plan Serialization");
    let mut plan = Plan::new("Serializable Plan", "Deploy application");
    plan.metadata
        .insert("version".to_string(), serde_json::json!("1.0.0"));
    plan.metadata
        .insert("environment".to_string(), serde_json::json!("production"));
    plan.add_step(PlanStep::new("build", "Build application"));
    plan.add_step(PlanStep::new("test", "Run tests"));
    plan.add_step(PlanStep::new("deploy", "Deploy to production"));

    // Serialize to JSON
    let json = serde_json::to_string_pretty(&plan)?;
    println!("Serialized plan ({} bytes):", json.len());
    println!("{}", json.lines().take(15).collect::<Vec<_>>().join("\n"));
    println!("  ...\n");

    // Deserialize
    let restored: Plan = serde_json::from_str(&json)?;
    println!(
        "Restored plan: {} with {} steps\n",
        restored.name,
        restored.steps.len()
    );

    println!("✅ Advanced planning demo completed!");
    Ok(())
}
