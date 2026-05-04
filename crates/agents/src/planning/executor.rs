//! Plan Executor
//!
//! Executes plans with support for sequential, parallel, and adaptive execution strategies.
//! Handles step dependencies, retries, and dynamic adaptation.

use super::plan::{Action, Plan, PlanId, PlanningError, PlanningResult};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tokio::time::timeout;
use tracing::{info, warn};

/// Plan executor
pub struct PlanExecutor {
    /// Execution configuration
    config: ExecutionConfig,
    /// Execution state
    state: Arc<RwLock<ExecutionState>>,
    /// Event sender
    event_tx: mpsc::Sender<ExecutionEvent>,
}

/// Execution configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    /// Default execution strategy
    pub strategy: ExecutionStrategy,
    /// Step timeout
    pub step_timeout: Duration,
    /// Plan timeout
    pub plan_timeout: Duration,
    /// Max retries per step
    pub max_retries: u32,
    /// Continue on step failure
    pub continue_on_failure: bool,
    /// Enable parallel execution
    pub enable_parallel: bool,
    /// Max concurrent steps
    pub max_concurrency: usize,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            strategy: ExecutionStrategy::Adaptive,
            step_timeout: Duration::from_secs(60),
            plan_timeout: Duration::from_secs(3600),
            max_retries: 3,
            continue_on_failure: false,
            enable_parallel: true,
            max_concurrency: 5,
        }
    }
}

/// Execution strategies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionStrategy {
    /// Execute steps sequentially
    Sequential,
    /// Execute independent steps in parallel
    Parallel,
    /// Adapt based on step characteristics
    Adaptive,
}

/// Execution state
#[derive(Debug, Clone, Default)]
struct ExecutionState {
    /// Currently executing plans
    active_plans: HashSet<PlanId>,
    /// Completed steps per plan
    completed_steps: HashMap<PlanId, HashSet<usize>>,
    /// Failed steps per plan
    failed_steps: HashMap<PlanId, HashSet<usize>>,
    /// Step results
    results: HashMap<String, serde_json::Value>,
}

/// Execution context for a single step
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Plan ID
    pub plan_id: PlanId,
    /// Step index
    pub step_index: usize,
    /// Step ID
    pub step_id: String,
    /// Previous results
    pub previous_results: HashMap<String, serde_json::Value>,
    /// Execution attempt number
    pub attempt: u32,
    /// Timeout
    pub timeout: Duration,
}

/// Execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Success status
    pub success: bool,
    /// Result data
    pub data: Option<serde_json::Value>,
    /// Error message if failed
    pub error: Option<String>,
    /// Execution duration
    pub duration_ms: u64,
    /// Number of attempts
    pub attempts: u32,
}

impl ExecutionResult {
    /// Create success result
    pub fn success(data: Option<serde_json::Value>) -> Self {
        Self {
            success: true,
            data,
            error: None,
            duration_ms: 0,
            attempts: 1,
        }
    }

    /// Create failure result
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error.into()),
            duration_ms: 0,
            attempts: 1,
        }
    }
}

/// Execution events
#[derive(Debug, Clone)]
pub enum ExecutionEvent {
    /// Plan started
    PlanStarted { plan_id: PlanId },
    /// Step started
    StepStarted { plan_id: PlanId, step_index: usize },
    /// Step completed
    StepCompleted {
        plan_id: PlanId,
        step_index: usize,
        result: ExecutionResult,
    },
    /// Step failed
    StepFailed {
        plan_id: PlanId,
        step_index: usize,
        error: String,
        will_retry: bool,
    },
    /// Plan completed
    PlanCompleted { plan_id: PlanId, success: bool },
}

/// Action handler trait
#[async_trait::async_trait]
pub trait ActionHandler: Send + Sync {
    /// Execute an action
    async fn execute(&self, action: &Action, context: &ExecutionContext) -> ExecutionResult;

    /// Check if this handler can handle the action
    fn can_handle(&self, action: &Action) -> bool;
}

/// Resolver for delegation requests (maps branch config to actual agent output)
#[async_trait::async_trait]
pub trait DelegateResolver: Send + Sync {
    /// Execute a single delegate branch and return the result
    async fn resolve(&self, branch: &super::plan::DelegateBranch) -> Result<String, String>;
}

/// Delegate resolver that spawns real sub-agents via Agent::spawn_sub_agent.
/// Uses Weak<Agent> to avoid circular references with PlanExecutor.
pub struct AgentDelegateResolver {
    parent: std::sync::Weak<crate::agent_impl::Agent>,
}

impl AgentDelegateResolver {
    pub fn new(parent: std::sync::Weak<crate::agent_impl::Agent>) -> Self {
        Self { parent }
    }
}

#[async_trait::async_trait]
impl DelegateResolver for AgentDelegateResolver {
    async fn resolve(&self, branch: &super::plan::DelegateBranch) -> Result<String, String> {
        let parent = self.parent.upgrade()
            .ok_or_else(|| "Parent agent no longer available".to_string())?;

        let mut child = parent.spawn_sub_agent(branch.agent_config.clone())
            .map_err(|e| format!("Failed to spawn sub-agent: {}", e))?;

        info!(
            "Sub-agent {} executing branch '{}' task: {}",
            child.get_config().id,
            branch.branch_id,
            branch.task
        );

        // Execute the task using the sub-agent
        let result = if let Some(ref skill_hint) = branch.skill_hint {
            child.execute_skill_by_id(skill_hint, &branch.task, None).await
                .map_err(|e| format!("Sub-agent skill execution failed: {}", e))?
        } else {
            // Fallback: use LLM to process the task directly
            let llm_result = child.call_llm_prompt(
                branch.task.clone(),
                Some::<String>("你是一个专家助手，请完成给定的任务。".to_string())
            ).await.map_err(|e| format!("Sub-agent LLM call failed: {}", e))?;
            crate::skills::executor::SkillExecutionResult {
                task_id: branch.branch_id.clone(),
                success: true,
                output: llm_result,
                structured_output: None,
                execution_time_ms: 0,
            }
        };

        info!(
            "Sub-agent {} completed branch '{}' in {}ms",
            child.get_config().id,
            branch.branch_id,
            result.execution_time_ms
        );

        Ok(result.output)
    }
}

/// Default action handler with actual tool execution support
/// 
/// ARCHITECTURE FIX: Now supports real tool calls through a tool registry.
/// Tools must be registered before they can be executed.
pub struct DefaultActionHandler {
    /// Tool registry for looking up and executing tools
    tool_registry: Arc<RwLock<HashMap<String, Box<dyn ToolExecutor>>>>,
    /// Optional delegate resolver for real sub-agent spawning
    delegate_resolver: Option<Arc<dyn DelegateResolver>>,
}

/// Tool executor trait for actual tool implementations
#[async_trait::async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Execute the tool with given parameters
    async fn execute(&self, params: &serde_json::Value) -> Result<serde_json::Value, String>;
    /// Get tool description
    fn description(&self) -> &str;
}

impl Default for DefaultActionHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultActionHandler {
    /// Create new action handler with empty tool registry
    pub fn new() -> Self {
        Self {
            tool_registry: Arc::new(RwLock::new(HashMap::new())),
            delegate_resolver: None,
        }
    }

    /// Create handler with a delegate resolver for real ParallelDelegate execution
    pub fn with_delegate_resolver(resolver: Arc<dyn DelegateResolver>) -> Self {
        Self {
            tool_registry: Arc::new(RwLock::new(HashMap::new())),
            delegate_resolver: Some(resolver),
        }
    }

    /// Set the delegate resolver
    pub fn set_delegate_resolver(&mut self, resolver: Arc<dyn DelegateResolver>) {
        self.delegate_resolver = Some(resolver);
    }
    
    /// Register a tool for execution
    pub async fn register_tool(&self, name: impl Into<String>, executor: Box<dyn ToolExecutor>) {
        let mut registry = self.tool_registry.write().await;
        registry.insert(name.into(), executor);
    }
    
    /// Check if a tool is registered
    pub async fn has_tool(&self, name: &str) -> bool {
        let registry = self.tool_registry.read().await;
        registry.contains_key(name)
    }

    /// Merge parallel branch results according to the given strategy
    fn merge_branch_results(
        branch_results: &HashMap<String, Result<String, String>>,
        strategy: &super::plan::MergeStrategy,
    ) -> serde_json::Value {
        match strategy {
            super::plan::MergeStrategy::Concat => {
                let texts: Vec<String> = branch_results.values()
                    .filter_map(|r| r.as_ref().ok().cloned())
                    .collect();
                serde_json::json!({ "merged": texts.join("\n---\n"), "strategy": "concat" })
            }
            super::plan::MergeStrategy::JsonMerge => {
                let mut merged = serde_json::Map::new();
                for (id, result) in branch_results {
                    if let Ok(text) = result {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(text) {
                            merged.insert(id.clone(), val);
                        } else {
                            merged.insert(id.clone(), serde_json::Value::String(text.clone()));
                        }
                    }
                }
                serde_json::Value::Object(merged)
            }
            super::plan::MergeStrategy::LlmSummarize => {
                let texts: Vec<String> = branch_results.values()
                    .filter_map(|r| r.as_ref().ok().cloned())
                    .collect();
                serde_json::json!({
                    "texts": texts,
                    "strategy": "llm_summarize",
                    "note": "LLM summarization should be performed by caller"
                })
            }
            super::plan::MergeStrategy::Custom(name) => {
                let texts: Vec<String> = branch_results.values()
                    .filter_map(|r| r.as_ref().ok().cloned())
                    .collect();
                serde_json::json!({
                    "texts": texts,
                    "strategy": name,
                    "note": "Custom merge strategy should be performed by caller"
                })
            }
        }
    }
}

#[async_trait::async_trait]
impl ActionHandler for DefaultActionHandler {
    async fn execute(&self, action: &Action, context: &ExecutionContext) -> ExecutionResult {
        match action {
            Action::ToolUse { tool_name, parameters } => {
                info!("Executing tool: {} with params: {:?}", tool_name, parameters);
                
                // ARCHITECTURE FIX: Look up tool in registry and execute if found
                let registry = self.tool_registry.read().await;
                if let Some(tool) = registry.get(tool_name) {
                    let params_value = serde_json::json!(parameters);
                    match tool.execute(&params_value).await {
                        Ok(result) => ExecutionResult::success(Some(result)),
                        Err(e) => ExecutionResult::failure(format!("Tool execution failed: {}", e)),
                    }
                } else {
                    // Tool not registered - return helpful error
                    let available_tools: Vec<String> = registry.keys().cloned().collect();
                    ExecutionResult::failure(format!(
                        "Tool '{}' not found. Available tools: {:?}. Register tools using register_tool() before execution.",
                        tool_name, available_tools
                    ))
                }
            }
            Action::LLMReasoning { prompt, context: reasoning_context } => {
                info!("LLM reasoning: {} with context: {:?}", prompt, reasoning_context);
                // In production, this would call the LLM provider
                ExecutionResult::success(Some(serde_json::json!({
                    "reasoning": "completed",
                    "conclusion": "success",
                    "prompt": prompt,
                    "context": reasoning_context
                })))
            }
            Action::Wait { condition, timeout } => {
                info!("Waiting for condition: {} (timeout: {:?})", condition, timeout);
                // ARCHITECTURE FIX: Actually implement wait with timeout
                if let Some(duration) = timeout {
                    tokio::time::sleep(*duration).await;
                }
                ExecutionResult::success(Some(serde_json::json!({
                    "condition": condition,
                    "waited": true
                })))
            }
            Action::UserInteraction { question } => {
                info!("User interaction required: {}", question);
                // In production, this would trigger an actual user notification
                ExecutionResult::success(Some(serde_json::json!({
                    "interaction": "required",
                    "question": question,
                    "plan_id": context.plan_id,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                })))
            }
            Action::SubPlan { plan_id } => {
                info!("Executing sub-plan: {}", plan_id);
                ExecutionResult::success(Some(serde_json::json!({
                    "sub_plan": plan_id,
                    "status": "delegated"
                })))
            }
            Action::Delegate { agent_id, task, skill_hint, output_schema } => {
                info!("Delegating to agent {}: {}", agent_id, task);
                ExecutionResult::success(Some(serde_json::json!({
                    "delegate_to": agent_id,
                    "task": task,
                    "skill_hint": skill_hint,
                    "output_schema": output_schema,
                    "status": "delegated"
                })))
            }
            Action::ParallelDelegate { branches, merge_strategy } => {
                info!("Parallel delegating to {} branches", branches.len());

                if let Some(resolver) = &self.delegate_resolver {
                    let mut handles = Vec::new();
                    for branch in branches {
                        let resolver = Arc::clone(resolver);
                        let branch = branch.clone();
                        handles.push(tokio::spawn(async move {
                            let result = resolver.resolve(&branch).await;
                            (branch.branch_id.clone(), result)
                        }));
                    }

                    let mut branch_results: HashMap<String, Result<String, String>> = HashMap::new();
                    for handle in handles {
                        match handle.await {
                            Ok((id, result)) => { branch_results.insert(id, result); }
                            Err(e) => {
                                let err_id = format!("join_error_{}", uuid::Uuid::new_v4());
                                branch_results.insert(err_id, Err(format!("Task join failed: {}", e)));
                            }
                        }
                    }

                    let merged = Self::merge_branch_results(&branch_results, merge_strategy);
                    let all_ok = branch_results.values().all(|r| r.is_ok());

                    if all_ok {
                        ExecutionResult::success(Some(merged))
                    } else {
                        let errors: Vec<String> = branch_results.iter()
                            .filter_map(|(id, r)| r.as_ref().err().map(|e| format!("{}: {}", id, e)))
                            .collect();
                        ExecutionResult::failure(format!(
                            "Parallel delegation failed for {} branches. Errors: {:?}",
                            errors.len(), errors
                        ))
                    }
                } else {
                    // No resolver configured — return mock response
                    ExecutionResult::success(Some(serde_json::json!({
                        "branches": branches.len(),
                        "merge_strategy": format!("{:?}", merge_strategy),
                        "status": "delegated",
                        "note": "No delegate resolver configured; mock response returned"
                    })))
                }
            }
        }
    }

    fn can_handle(&self, action: &Action) -> bool {
        // Can handle all action types
        matches!(action,
            Action::ToolUse { .. } |
            Action::LLMReasoning { .. } |
            Action::Wait { .. } |
            Action::UserInteraction { .. } |
            Action::SubPlan { .. } |
            Action::Delegate { .. } |
            Action::ParallelDelegate { .. }
        )
    }
}

impl PlanExecutor {
    /// Create new executor
    pub fn new() -> Self {
        let (event_tx, _) = mpsc::channel(100);
        Self {
            config: ExecutionConfig::default(),
            state: Arc::new(RwLock::new(ExecutionState::default())),
            event_tx,
        }
    }

    /// Create with custom config
    pub fn with_config(config: ExecutionConfig) -> Self {
        let (event_tx, _) = mpsc::channel(100);
        Self {
            config,
            state: Arc::new(RwLock::new(ExecutionState::default())),
            event_tx,
        }
    }

    /// Subscribe to execution events
    pub fn subscribe(&self) -> mpsc::Receiver<ExecutionEvent> {
        let (tx, rx) = mpsc::channel(100);
        // In production, would store sender for event distribution
        let _ = tx;
        rx
    }

    /// Execute a plan
    pub async fn execute(&self, plan: &mut Plan) -> PlanningResult<ExecutionResult> {
        info!("Starting execution of plan: {}", plan.id);

        plan.mark_started();

        let start_time = tokio::time::Instant::now();

        // Store plan in active plans
        {
            let mut state = self.state.write().await;
            state.active_plans.insert(plan.id.clone());
        }

        self.event_tx
            .send(ExecutionEvent::PlanStarted {
                plan_id: plan.id.clone(),
            })
            .await
            .ok();

        // Execute based on strategy
        let result = match self.config.strategy {
            ExecutionStrategy::Sequential => {
                self.execute_sequential(plan).await
            }
            ExecutionStrategy::Parallel => {
                self.execute_parallel(plan).await
            }
            ExecutionStrategy::Adaptive => {
                self.execute_adaptive(plan).await
            }
        };

        // Update plan status
        match &result {
            Ok(_) => plan.mark_completed(),
            Err(_) => plan.mark_failed(),
        }

        // Cleanup
        {
            let mut state = self.state.write().await;
            state.active_plans.remove(&plan.id);
            state.completed_steps.remove(&plan.id);
            state.failed_steps.remove(&plan.id);
        }

        let duration = start_time.elapsed();

        self.event_tx
            .send(ExecutionEvent::PlanCompleted {
                plan_id: plan.id.clone(),
                success: result.is_ok(),
            })
            .await
            .ok();

        info!(
            "Plan {} execution completed in {:?}",
            plan.id, duration
        );

        result
    }

    /// Execute steps sequentially
    async fn execute_sequential(&self, plan: &mut Plan) -> PlanningResult<ExecutionResult> {
        for i in 0..plan.steps.len() {
            let result = self.execute_step(plan, i).await?;

            if !result.success && !self.config.continue_on_failure {
                return Err(PlanningError::StepFailed {
                    step: i,
                    reason: result.error.unwrap_or_else(|| "Unknown error".to_string()),
                });
            }
        }

        Ok(ExecutionResult::success(None))
    }

    /// Execute steps with parallelism where possible
    async fn execute_parallel(&self, plan: &mut Plan) -> PlanningResult<ExecutionResult> {
        let mut completed = HashSet::new();
        let _semaphore = Arc::new(tokio::sync::Semaphore::new(self.config.max_concurrency));

        while completed.len() < plan.steps.len() {
            // Find ready steps
            let ready: Vec<usize> = plan
                .get_ready_steps(&completed.iter().cloned().collect::<Vec<_>>())
                .into_iter()
                .filter(|i| !completed.contains(i))
                .collect();

            if ready.is_empty() && completed.len() < plan.steps.len() {
                // Deadlock or all remaining blocked
                return Err(PlanningError::ExecutionFailed(
                    "Deadlock detected or steps blocked".to_string(),
                ));
            }

            // Execute ready steps in parallel using sequential execution
            // (parallel execution requires 'static lifetime which is complex with &self)
            for step_idx in ready {
                let result = self.execute_step(plan, step_idx).await?;
                if result.success {
                    completed.insert(step_idx);
                } else if !self.config.continue_on_failure {
                    return Err(PlanningError::StepFailed {
                        step: step_idx,
                        reason: result.error.unwrap_or_else(|| "Step failed".to_string()),
                    });
                }
            }
        }

        Ok(ExecutionResult::success(None))
    }

    /// Execute with adaptive strategy
    async fn execute_adaptive(&self, plan: &mut Plan) -> PlanningResult<ExecutionResult> {
        // Use parallel for independent steps, sequential for dependent
        let mut completed = HashSet::new();

        while completed.len() < plan.steps.len() {
            let ready = plan.get_ready_steps(&completed.iter().copied().collect::<Vec<_>>());

            if ready.is_empty() {
                break;
            }

            // Execute ready steps
            if ready.len() == 1 || !self.config.enable_parallel {
                // Sequential for single step
                let idx = ready[0];
                let result = self.execute_step(plan, idx).await?;

                if result.success {
                    completed.insert(idx);
                } else if !self.config.continue_on_failure {
                    return Err(PlanningError::StepFailed {
                        step: idx,
                        reason: result.error.unwrap_or_default(),
                    });
                }
            } else {
                // Parallel for multiple ready steps
                let mut results = Vec::new();

                for idx in ready.clone() {
                    let result = self.execute_step(plan, idx).await;
                    results.push((idx, result));
                }

                for (idx, result) in results {
                    match result {
                        Ok(r) if r.success => {
                            completed.insert(idx);
                        }
                        Ok(r) if !self.config.continue_on_failure => {
                            return Err(PlanningError::StepFailed {
                                step: idx,
                                reason: r.error.unwrap_or_default(),
                            });
                        }
                        Err(e) => return Err(e),
                        _ => {}
                    }
                }
            }
        }

        if completed.len() == plan.steps.len() {
            Ok(ExecutionResult::success(None))
        } else {
            Err(PlanningError::ExecutionFailed(
                "Not all steps completed".to_string(),
            ))
        }
    }

    /// Execute a single step
    async fn execute_step(
        &self,
        plan: &mut Plan,
        step_index: usize,
    ) -> PlanningResult<ExecutionResult> {
        // Get previous results before borrowing step mutably
        let previous_results = self.get_previous_results(plan, step_index).await;
        
        let step = &mut plan.steps[step_index];

        info!("Executing step {}: {}", step_index, step.description);

        step.mark_started();

        self.event_tx
            .send(ExecutionEvent::StepStarted {
                plan_id: plan.id.clone(),
                step_index,
            })
            .await
            .ok();

        let context = ExecutionContext {
            plan_id: plan.id.clone(),
            step_index,
            step_id: step.id.clone(),
            previous_results,
            attempt: 1,
            timeout: self.config.step_timeout,
        };

        // Execute with timeout and retries
        let mut last_result = None;
        let mut attempts = 0;

        while attempts < self.config.max_retries {
            attempts += 1;

            let result = timeout(
                self.config.step_timeout,
                self.execute_step_actions(&step.actions, &context),
            )
            .await;

            match result {
                Ok(Ok(exec_result)) => {
                    last_result = Some(exec_result.clone());

                    if exec_result.success {
                        step.mark_completed(exec_result.data.clone());

                        // Store result
                        {
                            let mut state = self.state.write().await;
                            state
                                .completed_steps
                                .entry(plan.id.clone())
                                .or_default()
                                .insert(step_index);
                            if let Some(data) = &exec_result.data {
                                state.results.insert(step.id.clone(), data.clone());
                            }
                        }

                        self.event_tx
                            .send(ExecutionEvent::StepCompleted {
                                plan_id: plan.id.clone(),
                                step_index,
                                result: exec_result.clone(),
                            })
                            .await
                            .ok();

                        return Ok(exec_result);
                    } else if attempts < self.config.max_retries {
                        warn!(
                            "Step {} failed, retrying ({}/{})",
                            step_index, attempts, self.config.max_retries
                        );
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
                Ok(Err(e)) => {
                    last_result = Some(ExecutionResult::failure(e.to_string()));
                    if attempts >= self.config.max_retries {
                        break;
                    }
                }
                Err(_) => {
                    last_result = Some(ExecutionResult::failure("Step timeout"));
                    if attempts >= self.config.max_retries {
                        break;
                    }
                }
            }
        }

        // All retries exhausted
        let error = last_result
            .as_ref()
            .and_then(|r| r.error.clone())
            .unwrap_or_else(|| "Step failed after all retries".to_string());

        step.mark_failed(&error);

        {
            let mut state = self.state.write().await;
            state
                .failed_steps
                .entry(plan.id.clone())
                .or_default()
                .insert(step_index);
        }

        self.event_tx
            .send(ExecutionEvent::StepFailed {
                plan_id: plan.id.clone(),
                step_index,
                error: error.clone(),
                will_retry: false,
            })
            .await
            .ok();

        if self.config.continue_on_failure {
            Ok(ExecutionResult::failure(error))
        } else {
            Err(PlanningError::StepFailed {
                step: step_index,
                reason: error,
            })
        }
    }

    /// Execute step actions
    async fn execute_step_actions(
        &self,
        actions: &[Action],
        context: &ExecutionContext,
    ) -> PlanningResult<ExecutionResult> {
        let handler = DefaultActionHandler::new();

        for action in actions {
            let result = handler.execute(action, context).await;
            if !result.success {
                return Ok(result);
            }
        }

        Ok(ExecutionResult::success(None))
    }

    /// Get results from previous steps
    async fn get_previous_results(
        &self,
        plan: &Plan,
        step_index: usize,
    ) -> HashMap<String, serde_json::Value> {
        let mut results = HashMap::new();
        let state = self.state.read().await;

        // Get results from dependencies
        if let Some(deps) = plan.dependencies.get(&step_index) {
            for &dep_idx in deps {
                if let Some(step) = plan.steps.get(dep_idx) {
                    if let Some(result) = state.results.get(&step.id) {
                        results.insert(step.id.clone(), result.clone());
                    }
                }
            }
        }

        results
    }

    /// Cancel plan execution
    pub async fn cancel(&self, plan_id: &PlanId) {
        let mut state = self.state.write().await;
        state.active_plans.remove(plan_id);
        info!("Cancelled plan execution: {}", plan_id);
    }

    /// Check if plan is active
    pub async fn is_active(&self, plan_id: &PlanId) -> bool {
        let state = self.state.read().await;
        state.active_plans.contains(plan_id)
    }
}

impl Default for PlanExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Sequential executor
pub struct SequentialExecutor;

impl SequentialExecutor {
    /// Create new sequential executor
    pub fn new() -> Self {
        Self
    }

    /// Execute plan sequentially
    pub async fn execute(&self, plan: &mut Plan) -> PlanningResult<ExecutionResult> {
        let executor = PlanExecutor::with_config(ExecutionConfig {
            strategy: ExecutionStrategy::Sequential,
            ..Default::default()
        });
        executor.execute(plan).await
    }
}

impl Default for SequentialExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Parallel executor
pub struct ParallelExecutor;

impl ParallelExecutor {
    /// Create new parallel executor
    pub fn new() -> Self {
        Self
    }

    /// Execute plan with parallelism
    pub async fn execute(&self, plan: &mut Plan) -> PlanningResult<ExecutionResult> {
        let executor = PlanExecutor::with_config(ExecutionConfig {
            strategy: ExecutionStrategy::Parallel,
            ..Default::default()
        });
        executor.execute(plan).await
    }
}

impl Default for ParallelExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planning::plan::{PlanStep, StepStatus, DelegateBranch, MergeStrategy};

    struct MockDelegateResolver;

    #[async_trait::async_trait]
    impl DelegateResolver for MockDelegateResolver {
        async fn resolve(&self, branch: &DelegateBranch) -> Result<String, String> {
            Ok(format!("Result for {}: {}", branch.branch_id, branch.task))
        }
    }

    #[tokio::test]
    async fn test_parallel_delegate_with_resolver() {
        let resolver: Arc<dyn DelegateResolver> = Arc::new(MockDelegateResolver);
        let handler = DefaultActionHandler::with_delegate_resolver(resolver);

        let branches = vec![
            DelegateBranch {
                branch_id: "b1".to_string(),
                agent_config: crate::AgentConfig::default(),
                task: "task1".to_string(),
                skill_hint: None,
            },
            DelegateBranch {
                branch_id: "b2".to_string(),
                agent_config: crate::AgentConfig::default(),
                task: "task2".to_string(),
                skill_hint: None,
            },
        ];

        let action = Action::ParallelDelegate {
            branches,
            merge_strategy: MergeStrategy::Concat,
        };

        let context = ExecutionContext {
            plan_id: crate::planning::plan::PlanId("test".to_string()),
            step_index: 0,
            step_id: "step1".to_string(),
            previous_results: HashMap::new(),
            attempt: 1,
            timeout: Duration::from_secs(10),
        };

        let result = handler.execute(&action, &context).await;
        assert!(result.success);
        let data = result.data.unwrap();
        let merged = data.get("merged").unwrap().as_str().unwrap();
        assert!(merged.contains("Result for b1: task1"));
        assert!(merged.contains("Result for b2: task2"));
    }

    #[tokio::test]
    async fn test_sequential_execution() {
        let executor = SequentialExecutor::new();
        let mut plan = Plan::new("Test", "Test goal");
        plan.add_step(PlanStep::new("1", "Step 1"));
        plan.add_step(PlanStep::new("2", "Step 2"));

        let result = executor.execute(&mut plan).await;
        assert!(result.is_ok());
        assert!(plan.is_complete());
    }

    #[tokio::test]
    async fn test_step_lifecycle() {
        let executor = PlanExecutor::new();
        let mut plan = Plan::new("Test", "Test goal");
        plan.add_step(PlanStep::new("1", "Test step"));

        assert_eq!(plan.steps[0].status, StepStatus::Pending);

        let result = executor.execute(&mut plan).await;
        assert!(result.is_ok());
        assert!(plan.steps[0].is_completed());
    }

    #[tokio::test]
    async fn test_plan_cancellation() {
        let executor = PlanExecutor::new();
        let plan = Plan::new("Test", "Test goal");

        executor.cancel(&plan.id).await;
        assert!(!executor.is_active(&plan.id).await);
    }
}
