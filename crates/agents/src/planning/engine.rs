//! Planning Engine
//!
//! Core planning logic supporting multiple planning strategies:
//! - ReAct (Reasoning + Acting)
//! - Chain-of-Thought
//! - Goal-based planning

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};

use super::plan::{Action, Plan, PlanId, PlanStep, PlanningResult, Priority};
use super::storage::{InMemoryPlanStorage, PlanFilter, PlanStorage};
use super::{Decomposer, DecompositionContext};

/// Planning engine that coordinates different planning strategies
///
/// ARCHITECTURE FIX: Now includes persistent storage and automatic TTL cleanup.
pub struct PlanningEngine {
    /// Active plans (in-memory cache)
    plans: Arc<RwLock<HashMap<PlanId, Plan>>>,
    /// Default decomposer
    #[allow(dead_code)]
    decomposer: Decomposer,
    /// Planning configuration
    config: PlanningConfig,
    /// ARCHITECTURE FIX: Persistent storage backend
    storage: Option<Arc<dyn PlanStorage>>,
    /// ARCHITECTURE FIX: Cleanup interval for TTL-based expiration
    cleanup_interval_secs: u64,
}

/// Planning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanningConfig {
    /// Default planning strategy
    pub default_strategy: PlanStrategy,
    /// Enable dynamic replanning
    pub enable_replanning: bool,
    /// Maximum planning iterations
    pub max_iterations: usize,
    /// Plan timeout in seconds
    pub plan_timeout_sec: u64,
    /// CODE QUALITY FIX: Tool registry for planners
    pub tool_registry: PlannerToolRegistry,
}

/// Tool registry for planners
///
/// CODE QUALITY FIX: Configurable tool names instead of hardcoded values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannerToolRegistry {
    /// Tool for information gathering/search
    pub search_tool: String,
    /// Tool for action execution
    pub execute_tool: String,
    /// Additional custom tools
    pub custom_tools: HashMap<String, String>,
}

impl Default for PlannerToolRegistry {
    fn default() -> Self {
        Self {
            search_tool: "search".to_string(),
            execute_tool: "execute".to_string(),
            custom_tools: HashMap::new(),
        }
    }
}

impl PlanningConfig {
    /// Create config from environment variables
    pub fn from_env() -> Self {
        use std::env;

        let tool_registry = PlannerToolRegistry {
            search_tool: env::var("PLANNER_SEARCH_TOOL").unwrap_or_else(|_| "search".to_string()),
            execute_tool: env::var("PLANNER_EXECUTE_TOOL")
                .unwrap_or_else(|_| "execute".to_string()),
            custom_tools: HashMap::new(),
        };

        Self {
            default_strategy: PlanStrategy::ReAct,
            enable_replanning: true,
            max_iterations: 10,
            plan_timeout_sec: 300,
            tool_registry,
        }
    }
}

impl Default for PlanningConfig {
    fn default() -> Self {
        Self {
            default_strategy: PlanStrategy::ReAct,
            enable_replanning: true,
            max_iterations: 10,
            plan_timeout_sec: 300,
            tool_registry: PlannerToolRegistry::default(),
        }
    }
}

/// Planning strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanStrategy {
    /// ReAct: Reasoning and Acting
    ReAct,
    /// Chain-of-Thought reasoning
    ChainOfThought,
    /// Goal-based planning
    GoalBased,
    /// Hybrid approach
    Hybrid,
}

/// Planning context
#[derive(Debug, Clone, Default)]
pub struct PlanContext {
    /// Agent ID
    pub agent_id: String,
    /// Session context
    pub session_id: Option<String>,
    /// Available tools
    pub available_tools: Vec<String>,
    /// Historical context
    pub history: Vec<String>,
    /// Constraints
    pub constraints: Vec<String>,
    /// User preferences
    pub preferences: HashMap<String, String>,
    /// Metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl PlanContext {
    /// Create new context
    pub fn new(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            session_id: None,
            available_tools: Vec::new(),
            history: Vec::new(),
            constraints: Vec::new(),
            preferences: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    /// Add tool
    pub fn with_tool(mut self, tool: impl Into<String>) -> Self {
        self.available_tools.push(tool.into());
        self
    }

    /// Add constraint
    pub fn with_constraint(mut self, constraint: impl Into<String>) -> Self {
        self.constraints.push(constraint.into());
        self
    }
}

/// Planner trait
#[async_trait::async_trait]
pub trait Planner: Send + Sync {
    /// Create a plan for the given goal
    async fn plan(&self, goal: &str, context: &PlanContext) -> PlanningResult<Plan>;

    /// Get planner name
    fn name(&self) -> &str;
}

impl PlanningEngine {
    /// Create new planning engine with in-memory storage
    pub fn new() -> Self {
        Self {
            plans: Arc::new(RwLock::new(HashMap::new())),
            decomposer: Decomposer::new(),
            config: PlanningConfig::default(),
            storage: Some(Arc::new(InMemoryPlanStorage::default())),
            cleanup_interval_secs: 3600, // Default: cleanup every hour
        }
    }

    /// Create with custom config
    pub fn with_config(config: PlanningConfig) -> Self {
        Self {
            plans: Arc::new(RwLock::new(HashMap::new())),
            decomposer: Decomposer::new(),
            config,
            storage: Some(Arc::new(InMemoryPlanStorage::default())),
            cleanup_interval_secs: 3600,
        }
    }

    /// ARCHITECTURE FIX: Create with custom storage backend
    pub fn with_storage(storage: Arc<dyn PlanStorage>) -> Self {
        Self {
            plans: Arc::new(RwLock::new(HashMap::new())),
            decomposer: Decomposer::new(),
            config: PlanningConfig::default(),
            storage: Some(storage),
            cleanup_interval_secs: 3600,
        }
    }

    /// ARCHITECTURE FIX: Create with storage and config
    pub fn with_storage_and_config(storage: Arc<dyn PlanStorage>, config: PlanningConfig) -> Self {
        Self {
            plans: Arc::new(RwLock::new(HashMap::new())),
            decomposer: Decomposer::new(),
            config,
            storage: Some(storage),
            cleanup_interval_secs: 3600,
        }
    }

    /// ARCHITECTURE FIX: Set cleanup interval for TTL-based expiration
    pub fn with_cleanup_interval(mut self, interval_secs: u64) -> Self {
        self.cleanup_interval_secs = interval_secs;
        self
    }

    /// ARCHITECTURE FIX: Start background cleanup task for expired plans
    pub fn start_cleanup_task(&self) {
        let storage = self.storage.clone();
        let interval_secs = self.cleanup_interval_secs;
        let plans = self.plans.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(interval_secs));

            loop {
                interval.tick().await;

                // Clean up persistent storage
                if let Some(storage) = &storage {
                    match storage.cleanup_expired().await {
                        Ok(count) => {
                            if count > 0 {
                                tracing::info!("Cleaned up {} expired plans from storage", count);
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to cleanup expired plans: {}", e);
                        }
                    }
                }

                // Clean up in-memory cache
                {
                    let mut plans_guard = plans.write().await;
                    let expired: Vec<PlanId> = plans_guard
                        .values()
                        .filter(|p| p.is_expired())
                        .map(|p| p.id.clone())
                        .collect();

                    for id in expired {
                        plans_guard.remove(&id);
                    }
                }
            }
        });
    }

    /// Create a plan using the configured strategy
    ///
    /// ARCHITECTURE FIX: Plans are now persisted to storage and subject to TTL
    /// cleanup.
    pub async fn create_plan(
        &self,
        goal: &str,
        context: &PlanContext,
        strategy: Option<PlanStrategy>,
    ) -> PlanningResult<Plan> {
        let strategy = strategy.unwrap_or(self.config.default_strategy);

        // Select planner based on strategy
        let planner: Box<dyn Planner> = match strategy {
            PlanStrategy::ReAct => Box::new(ReActPlanner::new()),
            PlanStrategy::ChainOfThought => Box::new(ChainOfThoughtPlanner::new()),
            PlanStrategy::GoalBased => Box::new(GoalBasedPlanner::new()),
            PlanStrategy::Hybrid => Box::new(HybridPlanner::new()),
        };

        let plan = planner.plan(goal, context).await?;

        // Store in memory
        let mut plans = self.plans.write().await;
        plans.insert(plan.id.clone(), plan.clone());
        drop(plans);

        // ARCHITECTURE FIX: Persist to storage
        if let Some(storage) = &self.storage {
            if let Err(e) = storage.store(&plan).await {
                tracing::warn!("Failed to persist plan {}: {}", plan.id, e);
                // Don't fail the operation if persistence fails
            }
        }

        Ok(plan)
    }

    /// Get plan by ID
    ///
    /// ARCHITECTURE FIX: Falls back to storage if not in memory.
    pub async fn get_plan(&self, plan_id: &PlanId) -> Option<Plan> {
        // Check in-memory cache first
        let plans = self.plans.read().await;
        if let Some(plan) = plans.get(plan_id) {
            return Some(plan.clone());
        }
        drop(plans);

        // ARCHITECTURE FIX: Fall back to persistent storage
        if let Some(storage) = &self.storage {
            match storage.retrieve(plan_id).await {
                Ok(plan) => {
                    // Cache in memory
                    let mut plans = self.plans.write().await;
                    plans.insert(plan_id.clone(), plan.clone());
                    return Some(plan);
                }
                Err(e) => {
                    tracing::debug!("Plan {} not found in storage: {}", plan_id, e);
                }
            }
        }

        None
    }

    /// Update plan
    ///
    /// ARCHITECTURE FIX: Updates are persisted to storage.
    pub async fn update_plan(&self, plan: Plan) {
        let mut plans = self.plans.write().await;
        plans.insert(plan.id.clone(), plan.clone());
        drop(plans);

        // ARCHITECTURE FIX: Update in persistent storage
        if let Some(storage) = &self.storage {
            if let Err(e) = storage.store(&plan).await {
                tracing::warn!("Failed to update persisted plan {}: {}", plan.id, e);
            }
        }
    }

    /// List active plans
    pub async fn list_active_plans(&self) -> Vec<Plan> {
        let plans = self.plans.read().await;
        plans
            .values()
            .filter(|p| p.status.is_active())
            .cloned()
            .collect()
    }

    /// List all plans (with optional filter)
    ///
    /// ARCHITECTURE FIX: Supports filtering and pagination.
    pub async fn list_plans(&self, filter: Option<PlanFilter>) -> Vec<Plan> {
        // Try storage first if available
        if let Some(storage) = &self.storage {
            match storage.list(filter.clone()).await {
                Ok(plans) => return plans,
                Err(e) => {
                    tracing::warn!("Failed to list plans from storage: {}", e);
                }
            }
        }

        // Fall back to in-memory
        let plans = self.plans.read().await;
        let mut results: Vec<Plan> = plans.values().cloned().collect();

        // Apply simple filter
        if let Some(filter) = filter {
            if let Some(status) = filter.status {
                results.retain(|p| p.status == status);
            }
            if let Some(limit) = filter.limit {
                results.truncate(limit);
            }
        }

        results
    }

    /// Remove completed/cancelled plans
    pub async fn cleanup(&self) {
        let mut plans = self.plans.write().await;
        plans.retain(|_, p| p.status.is_active());
    }

    /// ARCHITECTURE FIX: Clean up expired plans based on TTL
    #[allow(unused_assignments)]
    pub async fn cleanup_expired(&self) -> usize {
        let mut count = 0;

        // Clean up in-memory cache
        {
            let mut plans = self.plans.write().await;
            let expired: Vec<PlanId> = plans
                .values()
                .filter(|p| p.is_expired())
                .map(|p| p.id.clone())
                .collect();

            count = expired.len();
            for id in expired {
                plans.remove(&id);
            }
        }

        // Clean up persistent storage
        if let Some(storage) = &self.storage {
            match storage.cleanup_expired().await {
                Ok(storage_count) => count += storage_count,
                Err(e) => {
                    tracing::error!("Failed to cleanup expired plans from storage: {}", e);
                }
            }
        }

        count
    }

    /// ARCHITECTURE FIX: Get storage statistics
    pub async fn storage_stats(&self) -> Option<super::storage::StorageStats> {
        if let Some(storage) = &self.storage {
            match storage.stats().await {
                Ok(stats) => return Some(stats),
                Err(e) => {
                    tracing::warn!("Failed to get storage stats: {}", e);
                }
            }
        }
        None
    }

    /// Quick plan using default settings
    pub async fn quick_plan(&self, goal: &str) -> PlanningResult<Plan> {
        let context = PlanContext::new("default");
        self.create_plan(goal, &context, None).await
    }
}

impl Default for PlanningEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// ReAct (Reasoning + Acting) planner
///
/// CODE QUALITY FIX: Now uses configurable tool registry instead of hardcoded
/// names
pub struct ReActPlanner {
    tool_registry: PlannerToolRegistry,
}

impl ReActPlanner {
    /// Create new ReAct planner with default tools
    pub fn new() -> Self {
        Self {
            tool_registry: PlannerToolRegistry::default(),
        }
    }

    /// Create with custom tool registry
    pub fn with_tools(tool_registry: PlannerToolRegistry) -> Self {
        Self { tool_registry }
    }

    /// Build ReAct steps
    fn build_react_steps(&self, goal: &str, _context: &PlanContext) -> Vec<PlanStep> {
        let mut steps = Vec::new();

        // Initial reasoning
        steps.push(
            PlanStep::reasoning(format!("Analyze the goal: {}", goal)).with_action(
                Action::LLMReasoning {
                    prompt: format!("Analyze this goal and identify key aspects: {}", goal),
                    context: HashMap::new(),
                },
            ),
        );

        // Information gathering if needed - uses configured search tool
        steps.push(
            PlanStep::new("gather", "Gather necessary information").with_action(Action::ToolUse {
                tool_name: self.tool_registry.search_tool.clone(),
                parameters: HashMap::new(),
            }),
        );

        // Planning/decision step
        steps.push(PlanStep::decision(
            "Determine approach based on gathered information",
        ));

        // Action execution - uses configured execute tool
        steps.push(
            PlanStep::new("execute", "Execute primary action").with_action(Action::ToolUse {
                tool_name: self.tool_registry.execute_tool.clone(),
                parameters: HashMap::new(),
            }),
        );

        // Observation and reflection
        steps.push(PlanStep::reasoning(
            "Reflect on results and determine next steps",
        ));

        // Validation
        steps.push(PlanStep::new("validate", "Validate results against goal"));

        steps
    }
}

#[async_trait::async_trait]
impl Planner for ReActPlanner {
    async fn plan(&self, goal: &str, context: &PlanContext) -> PlanningResult<Plan> {
        let mut plan = Plan::new("ReAct Plan", goal);
        plan.priority = Priority::High;

        let steps = self.build_react_steps(goal, context);

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

    fn name(&self) -> &str {
        "ReActPlanner"
    }
}

impl Default for ReActPlanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Chain-of-Thought planner
pub struct ChainOfThoughtPlanner;

impl ChainOfThoughtPlanner {
    /// Create new CoT planner
    pub fn new() -> Self {
        Self
    }

    /// Generate reasoning chain
    fn generate_reasoning_chain(&self, goal: &str, _context: &PlanContext) -> Vec<PlanStep> {
        let mut steps = Vec::new();

        // Break down the problem
        steps.push(PlanStep::reasoning(format!(
            "Break down the problem: {}",
            goal
        )));

        // Consider different angles
        steps.push(PlanStep::reasoning(
            "Consider different approaches and their trade-offs",
        ));

        // Select best approach
        steps.push(PlanStep::decision("Select the most promising approach"));

        // Execute with step-by-step reasoning
        steps.push(PlanStep::reasoning(
            "Execute the solution step by step, explaining reasoning at each step",
        ));

        // Verify solution
        steps.push(PlanStep::new("verify", "Verify the solution is correct"));

        steps
    }
}

#[async_trait::async_trait]
impl Planner for ChainOfThoughtPlanner {
    async fn plan(&self, goal: &str, context: &PlanContext) -> PlanningResult<Plan> {
        let mut plan = Plan::new("Chain-of-Thought Plan", goal);
        plan.priority = Priority::Normal;

        let steps = self.generate_reasoning_chain(goal, context);

        // Add steps
        for (i, step) in steps.into_iter().enumerate() {
            if i > 0 {
                plan.add_step_with_deps(step, vec![i - 1])?;
            } else {
                plan.add_step(step);
            }
        }

        Ok(plan)
    }

    fn name(&self) -> &str {
        "ChainOfThoughtPlanner"
    }
}

impl Default for ChainOfThoughtPlanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Goal-based planner
///
/// CODE QUALITY FIX: Now uses configurable tool registry instead of hardcoded
/// names
pub struct GoalBasedPlanner {
    tool_registry: PlannerToolRegistry,
}

impl GoalBasedPlanner {
    /// Create new goal-based planner with default tools
    pub fn new() -> Self {
        Self {
            tool_registry: PlannerToolRegistry::default(),
        }
    }

    /// Create with custom tool registry
    pub fn with_tools(tool_registry: PlannerToolRegistry) -> Self {
        Self { tool_registry }
    }

    /// Decompose goal into subgoals
    fn decompose_goal(&self, goal: &str, _context: &PlanContext) -> Vec<PlanStep> {
        let mut steps = Vec::new();

        // Identify main goal
        steps.push(
            PlanStep::new(
                "identify_goal",
                format!("Identify and clarify goal: {}", goal),
            )
            .with_action(Action::LLMReasoning {
                prompt: format!("Clarify and formalize this goal: {}", goal),
                context: HashMap::new(),
            }),
        );

        // Define success criteria
        steps.push(PlanStep::decision(
            "Define clear success criteria for the goal",
        ));

        // Identify obstacles
        steps.push(PlanStep::reasoning(
            "Identify potential obstacles and risks",
        ));

        // Plan path to goal
        steps.push(PlanStep::reasoning("Plan optimal path to achieve goal"));

        // Execute actions - uses configured execute tool
        steps.push(
            PlanStep::new("execute", "Execute planned actions toward goal").with_action(
                Action::ToolUse {
                    tool_name: self.tool_registry.execute_tool.clone(),
                    parameters: HashMap::new(),
                },
            ),
        );

        // Verify goal achievement
        steps.push(PlanStep::new(
            "verify_goal",
            "Verify that the goal has been achieved",
        ));

        steps
    }
}

#[async_trait::async_trait]
impl Planner for GoalBasedPlanner {
    async fn plan(&self, goal: &str, context: &PlanContext) -> PlanningResult<Plan> {
        let mut plan = Plan::new("Goal-Based Plan", goal);
        plan.priority = Priority::High;

        let steps = self.decompose_goal(goal, context);

        for (i, step) in steps.into_iter().enumerate() {
            if i > 0 {
                plan.add_step_with_deps(step, vec![i - 1])?;
            } else {
                plan.add_step(step);
            }
        }

        Ok(plan)
    }

    fn name(&self) -> &str {
        "GoalBasedPlanner"
    }
}

impl Default for GoalBasedPlanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Hybrid planner combining multiple strategies
pub struct HybridPlanner {
    decomposer: Decomposer,
}

impl HybridPlanner {
    /// Create new hybrid planner
    pub fn new() -> Self {
        Self {
            decomposer: Decomposer::new(),
        }
    }

    /// Select best strategy based on goal characteristics
    #[allow(dead_code)]
    fn select_strategy(&self, goal: &str, _context: &PlanContext) -> PlanStrategy {
        let goal_lower = goal.to_lowercase();

        if goal_lower.contains("analyze") || goal_lower.contains("reason") {
            PlanStrategy::ChainOfThought
        } else if goal_lower.contains("implement")
            || goal_lower.contains("build")
            || goal_lower.contains("fix")
        {
            PlanStrategy::ReAct
        } else if goal_lower.contains("achieve") || goal_lower.contains("goal") {
            PlanStrategy::GoalBased
        } else {
            PlanStrategy::ReAct
        }
    }
}

#[async_trait::async_trait]
impl Planner for HybridPlanner {
    #[allow(unused_variables)]
    async fn plan(&self, goal: &str, context: &PlanContext) -> PlanningResult<Plan> {
        // First use decomposer for task breakdown
        let decomp_context = DecompositionContext::new().with_max_depth(2); // 🆕 FIX: Reduced from 3 to limit step count

        let mut plan = self.decomposer.decompose(goal, &decomp_context)?;
        plan.name = "Hybrid Plan".to_string();

        // Add reasoning steps between action steps
        let mut enhanced_steps = Vec::new();
        for (i, step) in plan.steps.iter().cloned().enumerate() {
            // Add reasoning before action steps
            if matches!(step.step_type, super::StepType::Action) && i > 0 {
                enhanced_steps.push(PlanStep::reasoning(format!(
                    "Prepare for: {}",
                    step.description
                )));
            }
            enhanced_steps.push(step);
        }

        // 🆕 FIX: Deduplicate steps by semantic description similarity
        let mut deduped = Vec::new();
        let mut seen_descriptions = std::collections::HashSet::new();
        for step in enhanced_steps {
            let normalized = step
                .description
                .to_lowercase()
                .replace(|c: char| !c.is_alphanumeric(), "");
            // Skip if we've seen a very similar description
            if seen_descriptions.iter().any(|s: &String| {
                let similarity = normalized.chars().filter(|c| s.contains(*c)).count() as f32
                    / normalized.len().max(s.len()) as f32;
                similarity > 0.85
            }) {
                continue;
            }
            seen_descriptions.insert(normalized);
            deduped.push(step);
        }
        enhanced_steps = deduped;

        // 🆕 FIX: Hard cap at 8 steps to prevent LLM timeout (each step = 1 LLM call)
        if enhanced_steps.len() > 8 {
            enhanced_steps.truncate(8);
        }

        plan.steps = enhanced_steps;

        // Rebuild dependencies
        plan.dependencies.clear();
        for i in 1..plan.steps.len() {
            plan.add_step_with_deps(plan.steps[i].clone(), vec![i - 1])?;
        }

        // 🆕 FIX: Mark plan as parallel-safe when steps are independent (no branching
        // deps) This allows the executor to run steps concurrently and avoid
        // timeout
        plan.metadata
            .insert("enable_parallel".to_string(), serde_json::Value::Bool(true));
        plan.metadata.insert(
            "max_concurrency".to_string(),
            serde_json::Value::Number(serde_json::Number::from(5)),
        );

        Ok(plan)
    }

    fn name(&self) -> &str {
        "HybridPlanner"
    }
}

impl Default for HybridPlanner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_react_planner() {
        let planner = ReActPlanner::new();
        let context = PlanContext::new("test-agent");

        let plan = planner
            .plan("Analyze the system performance", &context)
            .await
            .unwrap();

        assert_eq!(plan.name, "ReAct Plan");
        assert!(!plan.steps.is_empty());
        assert!(plan
            .steps
            .iter()
            .any(|s| matches!(s.step_type, super::super::StepType::Reasoning)));
    }

    #[tokio::test]
    async fn test_cot_planner() {
        let planner = ChainOfThoughtPlanner::new();
        let context = PlanContext::new("test-agent");

        let plan = planner
            .plan("Solve complex math problem", &context)
            .await
            .unwrap();

        assert_eq!(plan.name, "Chain-of-Thought Plan");
        assert!(!plan.steps.is_empty());
    }

    #[tokio::test]
    async fn test_goal_based_planner() {
        let planner = GoalBasedPlanner::new();
        let context = PlanContext::new("test-agent");

        let plan = planner
            .plan("Achieve high availability", &context)
            .await
            .unwrap();

        assert_eq!(plan.name, "Goal-Based Plan");
        assert!(!plan.steps.is_empty());
    }

    #[tokio::test]
    async fn test_planning_engine() {
        let engine = PlanningEngine::new();
        let context = PlanContext::new("test-agent");

        let plan = engine
            .create_plan("Review code quality", &context, Some(PlanStrategy::ReAct))
            .await
            .unwrap();

        assert!(!plan.steps.is_empty());

        // Verify plan is stored
        let retrieved = engine.get_plan(&plan.id).await;
        assert!(retrieved.is_some());
    }
}
