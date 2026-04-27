//! Task Decomposer
//!
//! Provides strategies for breaking down complex goals into manageable steps.
//! Supports hierarchical, parallel, and recursive decomposition.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::plan::{Action, Plan, PlanStep, PlanningResult, StepType};

/// Task decomposer trait
pub trait TaskDecomposer: Send + Sync {
    /// Decompose a goal into a plan
    fn decompose(&self, goal: &str, context: &DecompositionContext) -> PlanningResult<Plan>;

    /// Check if this decomposer can handle the goal
    fn can_handle(&self, goal: &str, context: &DecompositionContext) -> bool;
}

/// Decomposition context
#[derive(Debug, Clone, Default)]
pub struct DecompositionContext {
    /// Available tools
    pub available_tools: Vec<String>,
    /// Agent capabilities
    pub capabilities: Vec<String>,
    /// Historical patterns
    pub patterns: Vec<String>,
    /// Constraints
    pub constraints: Vec<String>,
    /// Max decomposition depth
    pub max_depth: usize,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl DecompositionContext {
    /// Create new context
    pub fn new() -> Self {
        Self {
            available_tools: Vec::new(),
            capabilities: Vec::new(),
            patterns: Vec::new(),
            constraints: Vec::new(),
            max_depth: 2, // 🆕 FIX: Reduced from 5 to prevent exponential step explosion
            metadata: HashMap::new(),
        }
    }

    /// Add available tool
    pub fn with_tool(mut self, tool: impl Into<String>) -> Self {
        self.available_tools.push(tool.into());
        self
    }

    /// Add capability
    pub fn with_capability(mut self, capability: impl Into<String>) -> Self {
        self.capabilities.push(capability.into());
        self
    }

    /// Set max depth
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }
}

/// Decomposition strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecompositionStrategy {
    /// Hierarchical breakdown (goal -> subgoals -> tasks)
    Hierarchical,
    /// Parallel decomposition (independent tasks)
    Parallel,
    /// Sequential decomposition (ordered steps)
    Sequential,
    /// Domain-specific decomposition
    DomainSpecific,
    /// LLM-assisted decomposition
    LLMAssisted,
}

/// Hierarchical decomposer
pub struct HierarchicalDecomposer;

impl HierarchicalDecomposer {
    /// Create new decomposer
    pub fn new() -> Self {
        Self
    }

    /// Decompose using hierarchical approach
    fn decompose_hierarchical(
        &self,
        goal: &str,
        context: &DecompositionContext,
        depth: usize,
    ) -> PlanningResult<Vec<PlanStep>> {
        if depth > context.max_depth {
            return Ok(vec![PlanStep::new(
                format!("leaf-{}", depth),
                goal.to_string(),
            )]);
        }

        // 🔧 FIX: Only perform pattern-based decomposition at depth 0 (original user
        // query). Sub-steps generated from decomposition should NOT be
        // recursively decomposed to avoid exponential plan explosion (e.g.
        // "plan" keyword matching sub-step descriptions).
        let steps = if depth == 0 {
            self.identify_subgoals(goal, context)
        } else {
            Vec::new()
        };

        // 🆕 FIX: Hard cap at 6 subgoals to prevent LLM call explosion
        let steps: Vec<PlanStep> = steps.into_iter().take(6).collect();

        if steps.is_empty() {
            // Leaf task - cannot decompose further
            Ok(vec![PlanStep::new(
                format!("task-{}-leaf", depth),
                goal.to_string(),
            )])
        } else {
            // Recursively decompose each subgoal (only one level deep due to depth == 0
            // guard)
            let mut all_steps = Vec::new();
            for (i, subgoal) in steps.iter().enumerate() {
                let sub_steps =
                    self.decompose_hierarchical(&subgoal.description, context, depth + 1)?;

                // Add reasoning step before subgoal execution
                all_steps.push(PlanStep::reasoning(format!(
                    "Analyzing: {}",
                    subgoal.description
                )));

                for (j, mut step) in sub_steps.into_iter().enumerate() {
                    step.id = format!("{}-{}-{}", depth, i, j);
                    all_steps.push(step);
                }
            }
            Ok(all_steps)
        }
    }

    /// Identify subgoals based on patterns
    fn identify_subgoals(&self, goal: &str, _context: &DecompositionContext) -> Vec<PlanStep> {
        let mut steps = Vec::new();
        let goal_lower = goal.to_lowercase();

        // Pattern matching for common goal types (English + Chinese)
        if goal_lower.contains("analyze") && goal_lower.contains("report")
            || (goal_lower.contains("分析") && goal_lower.contains("报告"))
        {
            steps.push(PlanStep::new("gather", "Gather required data"));
            steps.push(PlanStep::new("analyze", "Perform analysis"));
            steps.push(PlanStep::new("compile", "Compile findings"));
            steps.push(PlanStep::new("format", "Format report"));
        } else if goal_lower.contains("implement")
            || goal_lower.contains("build")
            || goal_lower.contains("开发")
            || goal_lower.contains("实现")
            || goal_lower.contains("构建")
        {
            steps.push(PlanStep::new("design", "Design solution"));
            steps.push(PlanStep::new("implement", "Implement solution"));
            steps.push(PlanStep::new("test", "Test implementation"));
            steps.push(PlanStep::new("deploy", "Deploy solution"));
        } else if goal_lower.contains("research")
            || goal_lower.contains("investigate")
            || goal_lower.contains("研究")
            || goal_lower.contains("调查")
            || goal_lower.contains("搜索")
        {
            steps.push(PlanStep::new("search", "Search for information"));
            steps.push(PlanStep::new("evaluate", "Evaluate sources"));
            steps.push(PlanStep::new("synthesize", "Synthesize findings"));
        } else if goal_lower.contains("compare")
            || goal_lower.contains("evaluate")
            || goal_lower.contains("比较")
            || goal_lower.contains("评估")
            || goal_lower.contains("对比")
        {
            steps.push(PlanStep::new("identify", "Identify options"));
            steps.push(PlanStep::new("criteria", "Define evaluation criteria"));
            steps.push(PlanStep::new("compare", "Compare options"));
            steps.push(PlanStep::new("recommend", "Make recommendation"));
        } else if goal_lower.contains("计划")
            || goal_lower.contains("规划")
            || goal_lower.contains("安排")
            || goal_lower.contains("步骤")
            || goal_lower.contains("攻略")
            || goal_lower.contains("行程")
            || goal_lower.contains("plan")
            || goal_lower.contains("schedule")
            || goal_lower.contains("itinerary")
        {
            steps.push(PlanStep::new(
                "gather_info",
                "Gather relevant information and constraints",
            ));
            steps.push(PlanStep::new(
                "formulate",
                "Formulate detailed plan with timeline",
            ));
            steps.push(PlanStep::new("refine", "Refine and optimize the plan"));
            steps.push(PlanStep::new(
                "present",
                "Present the final plan with actionable steps",
            ));
        }

        steps
    }
}

impl TaskDecomposer for HierarchicalDecomposer {
    fn decompose(&self, goal: &str, context: &DecompositionContext) -> PlanningResult<Plan> {
        let mut plan = Plan::new("Hierarchical Plan", goal);
        let steps = self.decompose_hierarchical(goal, context, 0)?;

        // Add steps with sequential dependencies
        for (i, step) in steps.into_iter().enumerate() {
            if i > 0 {
                plan.add_step_with_deps(step, vec![i - 1])?;
            } else {
                plan.add_step(step);
            }
        }

        Ok(plan)
    }

    fn can_handle(&self, goal: &str, _context: &DecompositionContext) -> bool {
        // Can handle most goals
        !goal.is_empty()
    }
}

impl Default for HierarchicalDecomposer {
    fn default() -> Self {
        Self::new()
    }
}

/// Parallel decomposer - identifies independent tasks
pub struct ParallelDecomposer;

impl ParallelDecomposer {
    /// Create new decomposer
    pub fn new() -> Self {
        Self
    }

    /// Analyze goal for parallelizable components
    fn identify_parallel_tasks(&self, goal: &str, context: &DecompositionContext) -> Vec<PlanStep> {
        let mut steps = Vec::new();
        let goal_lower = goal.to_lowercase();

        // Check for "and" or list patterns indicating parallelism
        if goal_lower.contains(" and ") || goal_lower.contains(", ") {
            // Split by common separators
            let parts: Vec<&str> = goal.split(" and ").flat_map(|s| s.split(", ")).collect();

            for (i, part) in parts.iter().enumerate() {
                let part = part.trim();
                if !part.is_empty() {
                    let mut step = PlanStep::new(format!("parallel-{}", i), part.to_string());
                    step.step_type = StepType::Action;
                    steps.push(step);
                }
            }
        }

        // If no parallel components found, fall back to hierarchical
        if steps.is_empty() {
            let hierarchical = HierarchicalDecomposer::new();
            if let Ok(plan) = hierarchical.decompose(goal, context) {
                // Try to identify independent steps
                return self.find_independent_steps(&plan);
            }
        }

        steps
    }

    /// Find independent steps in a plan
    fn find_independent_steps(&self, plan: &Plan) -> Vec<PlanStep> {
        // Identify steps with no dependencies
        plan.steps
            .iter()
            .enumerate()
            .filter(|(i, _)| !plan.dependencies.contains_key(i))
            .map(|(_, s)| s.clone())
            .collect()
    }
}

impl TaskDecomposer for ParallelDecomposer {
    fn decompose(&self, goal: &str, context: &DecompositionContext) -> PlanningResult<Plan> {
        let mut plan = Plan::new("Parallel Plan", goal);
        let steps = self.identify_parallel_tasks(goal, context);

        if steps.is_empty() {
            // Fall back to single step
            plan.add_step(PlanStep::new("main", goal));
        } else {
            // Add all steps without dependencies (parallel execution)
            for step in steps {
                plan.add_step(step);
            }
        }

        // Add a final aggregation step if needed
        if plan.steps.len() > 1 {
            let agg_step = PlanStep::new(
                "aggregate",
                format!("Aggregate results from {} parallel tasks", plan.steps.len()),
            )
            .with_action(Action::LLMReasoning {
                prompt: "Synthesize results from all parallel tasks".to_string(),
                context: HashMap::new(),
            });

            let deps: Vec<usize> = (0..plan.steps.len()).collect();
            plan.add_step_with_deps(agg_step, deps)?;
        }

        Ok(plan)
    }

    fn can_handle(&self, goal: &str, _context: &DecompositionContext) -> bool {
        goal.to_lowercase().contains(" and ") || goal.to_lowercase().contains(", ")
    }
}

impl Default for ParallelDecomposer {
    fn default() -> Self {
        Self::new()
    }
}

/// Domain-specific decomposer with predefined patterns
pub struct DomainDecomposer {
    patterns: HashMap<String, Vec<PlanStep>>,
}

impl DomainDecomposer {
    /// Create new domain decomposer with default patterns
    pub fn new() -> Self {
        let mut patterns = HashMap::new();

        // Code review pattern
        patterns.insert(
            "code_review".to_string(),
            vec![
                PlanStep::new("understand", "Understand code purpose and context"),
                PlanStep::new("check_style", "Check code style and conventions"),
                PlanStep::new("check_logic", "Review logic and algorithms"),
                PlanStep::new("check_security", "Check for security issues"),
                PlanStep::new("check_tests", "Review test coverage"),
                PlanStep::new("feedback", "Provide constructive feedback"),
            ],
        );

        // Data analysis pattern
        patterns.insert(
            "data_analysis".to_string(),
            vec![
                PlanStep::new("explore", "Explore and understand the dataset"),
                PlanStep::new("clean", "Clean and preprocess data"),
                PlanStep::new("analyze", "Perform statistical analysis"),
                PlanStep::new("visualize", "Create visualizations"),
                PlanStep::new("interpret", "Interpret results"),
                PlanStep::new("report", "Generate analysis report"),
            ],
        );

        // Bug fixing pattern
        patterns.insert(
            "bug_fix".to_string(),
            vec![
                PlanStep::new("reproduce", "Reproduce the bug"),
                PlanStep::new("diagnose", "Diagnose root cause"),
                PlanStep::new("design_fix", "Design fix solution"),
                PlanStep::new("implement", "Implement the fix"),
                PlanStep::new("test", "Test the fix"),
                PlanStep::new("regression", "Check for regressions"),
            ],
        );

        Self { patterns }
    }

    /// Add custom pattern
    pub fn add_pattern(&mut self, name: impl Into<String>, steps: Vec<PlanStep>) {
        self.patterns.insert(name.into(), steps);
    }

    /// Detect domain from goal
    fn detect_domain(&self, goal: &str) -> Option<&str> {
        let goal_lower = goal.to_lowercase();

        if goal_lower.contains("review") && goal_lower.contains("code") {
            Some("code_review")
        } else if goal_lower.contains("analy") && goal_lower.contains("data") {
            Some("data_analysis")
        } else if goal_lower.contains("bug") || goal_lower.contains("fix") {
            Some("bug_fix")
        } else {
            None
        }
    }
}

impl TaskDecomposer for DomainDecomposer {
    fn decompose(&self, goal: &str, _context: &DecompositionContext) -> PlanningResult<Plan> {
        let mut plan = Plan::new("Domain Plan", goal);

        if let Some(domain) = self.detect_domain(goal) {
            if let Some(steps) = self.patterns.get(domain) {
                // Add steps with sequential dependencies
                for (i, step) in steps.iter().cloned().enumerate() {
                    if i > 0 {
                        plan.add_step_with_deps(step, vec![i - 1])?;
                    } else {
                        plan.add_step(step);
                    }
                }
                return Ok(plan);
            }
        }

        // Fallback to hierarchical
        HierarchicalDecomposer::new().decompose(goal, _context)
    }

    fn can_handle(&self, goal: &str, _context: &DecompositionContext) -> bool {
        self.detect_domain(goal).is_some()
    }
}

impl Default for DomainDecomposer {
    fn default() -> Self {
        Self::new()
    }
}

/// Composite decomposer that tries multiple strategies
pub struct CompositeDecomposer {
    decomposers: Vec<Box<dyn TaskDecomposer>>,
}

impl CompositeDecomposer {
    /// Create new composite decomposer with default strategies
    pub fn new() -> Self {
        Self {
            decomposers: vec![
                Box::new(DomainDecomposer::new()),
                Box::new(ParallelDecomposer::new()),
                Box::new(HierarchicalDecomposer::new()),
            ],
        }
    }

    /// Add custom decomposer
    pub fn add_decomposer(&mut self, decomposer: Box<dyn TaskDecomposer>) {
        self.decomposers.push(decomposer);
    }
}

impl TaskDecomposer for CompositeDecomposer {
    fn decompose(&self, goal: &str, context: &DecompositionContext) -> PlanningResult<Plan> {
        // Try each decomposer in order
        for decomposer in &self.decomposers {
            if decomposer.can_handle(goal, context) {
                return decomposer.decompose(goal, context);
            }
        }

        // Fallback to hierarchical
        HierarchicalDecomposer::new().decompose(goal, context)
    }

    fn can_handle(&self, goal: &str, context: &DecompositionContext) -> bool {
        self.decomposers.iter().any(|d| d.can_handle(goal, context))
    }
}

impl Default for CompositeDecomposer {
    fn default() -> Self {
        Self::new()
    }
}

/// Main decomposer interface
pub struct Decomposer {
    inner: CompositeDecomposer,
}

impl Decomposer {
    /// Create new decomposer
    pub fn new() -> Self {
        Self {
            inner: CompositeDecomposer::new(),
        }
    }

    /// Decompose a goal into a plan
    pub fn decompose(&self, goal: &str, context: &DecompositionContext) -> PlanningResult<Plan> {
        self.inner.decompose(goal, context)
    }

    /// Quick decompose with default context
    pub fn quick_decompose(&self, goal: &str) -> PlanningResult<Plan> {
        self.decompose(goal, &DecompositionContext::default())
    }
}

impl Default for Decomposer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hierarchical_decomposition() {
        let decomposer = HierarchicalDecomposer::new();
        let context = DecompositionContext::new();

        let plan = decomposer
            .decompose("Analyze data and create a report", &context)
            .unwrap();

        assert!(!plan.steps.is_empty());
        assert!(plan.steps.iter().any(|s| s.description.contains("data")));
    }

    #[test]
    fn test_parallel_decomposition() {
        let decomposer = ParallelDecomposer::new();
        let context = DecompositionContext::new();

        let plan = decomposer
            .decompose("Task A and Task B and Task C", &context)
            .unwrap();

        // Should have parallel tasks plus aggregation
        assert!(plan.steps.len() >= 2);
    }

    #[test]
    fn test_domain_decomposition() {
        let decomposer = DomainDecomposer::new();
        let context = DecompositionContext::new();

        let plan = decomposer
            .decompose("Fix the authentication bug", &context)
            .unwrap();

        assert!(plan
            .steps
            .iter()
            .any(|s| s.description.to_lowercase().contains("reproduce")));
        assert!(plan
            .steps
            .iter()
            .any(|s| s.description.to_lowercase().contains("fix")));
    }

    #[test]
    fn test_composite_decomposition() {
        let decomposer = CompositeDecomposer::new();
        let context = DecompositionContext::new();

        let plan = decomposer
            .decompose("Review this code for issues", &context)
            .unwrap();

        assert!(!plan.steps.is_empty());
    }
}
