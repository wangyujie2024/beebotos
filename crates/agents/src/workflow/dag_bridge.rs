//! Workflow-to-DAG Scheduler Bridge
//!
//! Provides `WorkflowDagExecutor` (a `TaskExecutor` implementation) that
//! resolves workflow templates using upstream task results stored in the
//! DagScheduler, and converts `WorkflowDefinition`s into `DagWorkflow`s.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tracing::{debug, info, warn};

use crate::agent_impl::Agent;
use crate::error::AgentError;
use crate::queue::dag_scheduler::{
    DagScheduler, DagTask, DagWorkflow, DagWorkflowBuilder, TaskExecutionRequest, TaskExecutor,
    WorkflowConfig,
};
use crate::task::{TaskResult, TaskType};
use crate::workflow::{
    definition::WorkflowDefinition,
    state::{StepState, StepStatus, WorkflowInstance, WorkflowStatus},
    template::{resolve_value_templates, TemplateContext},
};

/// Executor that runs workflow steps via the Agent's skill execution,
/// resolving templates using upstream task results from the DagScheduler.
pub struct WorkflowDagExecutor {
    agent: Arc<Agent>,
    scheduler: Arc<DagScheduler>,
}

impl WorkflowDagExecutor {
    /// Create a new workflow DAG executor
    pub fn new(agent: Arc<Agent>, scheduler: Arc<DagScheduler>) -> Self {
        Self { agent, scheduler }
    }

    /// Build a template context from upstream task results in the scheduler
    async fn build_template_context(
        &self,
        instance_id: &str,
        trigger_context: Value,
    ) -> TemplateContext {
        let mut ctx = TemplateContext::with_trigger(trigger_context);

        // Read all completed task results from the scheduler instance
        if let Some(scheduler_instance) = self.scheduler.get_instance(instance_id).await {
            for (task_id, task_state) in &scheduler_instance.task_states {
                if let Some(ref result) = task_state.result {
                    let output = serde_json::from_str(&result.output)
                        .unwrap_or_else(|_| serde_json::Value::String(result.output.clone()));
                    ctx.add_step_output(task_id, output);
                    ctx.add_step_status(
                        task_id,
                        if result.success { "completed" } else { "failed" },
                    );
                }
            }
        }

        ctx
    }
}

#[async_trait::async_trait]
impl TaskExecutor for WorkflowDagExecutor {
    async fn execute(&self, request: TaskExecutionRequest) -> Result<TaskResult, AgentError> {
        let task = &request.task;
        let instance_id = &request.instance_id;
        let task_id = &request.task_id;

        debug!(
            "WorkflowDagExecutor executing task {} for instance {}",
            task_id, instance_id
        );

        // Extract workflow metadata encoded in parameters
        let skill_id = task
            .parameters
            .get("_skill")
            .and_then(|v| v.as_str())
            .unwrap_or(task_id)
            .to_string();

        let condition = task
            .parameters
            .get("_condition")
            .and_then(|v| v.as_str());

        let trigger_context = task
            .parameters
            .get("_trigger_context")
            .cloned()
            .unwrap_or(Value::Null);

        // Build template context from upstream results
        let template_ctx = self.build_template_context(instance_id, trigger_context).await;

        // Evaluate condition (if present)
        if let Some(condition_str) = condition {
            let resolved = crate::workflow::template::resolve_template(condition_str, &template_ctx)
                .unwrap_or_else(|_| "false".to_string());
            if resolved.trim() != "true" {
                info!(
                    "Task '{}' condition not met ('{}' != 'true'), skipping",
                    task_id, resolved
                );
                return Ok(TaskResult {
                    task_id: task_id.clone(),
                    success: true,
                    output: serde_json::json!({"skipped": true, "reason": "condition_not_met"})
                        .to_string(),
                    artifacts: vec![],
                    execution_time_ms: 0,
                });
            }
        }

        // Resolve templates in parameters (excluding metadata keys)
        let mut resolved_params = Value::Object(
            task
                .parameters
                .iter()
                .filter(|(k, _)| !k.starts_with('_'))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        );

        if let Err(e) = resolve_value_templates(&mut resolved_params, &template_ctx) {
            warn!("Task '{}' template resolution failed: {}", task_id, e);
            return Ok(TaskResult {
                task_id: task_id.clone(),
                success: false,
                output: serde_json::json!({"error": format!("Template resolution: {}", e)})
                    .to_string(),
                artifacts: vec![],
                execution_time_ms: 0,
            });
        }

        // Extract input and parameters for skill execution
        let (skill_input, skill_params) = extract_input_and_params(&resolved_params);

        // Execute skill
        let start_time = std::time::Instant::now();
        let execution_result = self
            .agent
            .execute_skill_by_id(&skill_id, &skill_input, Some(skill_params))
            .await;
        let duration_ms = start_time.elapsed().as_millis() as u64;

        match execution_result {
            Ok(result) => Ok(TaskResult {
                task_id: task_id.clone(),
                success: true,
                output: result.output,
                artifacts: vec![],
                execution_time_ms: duration_ms,
            }),
            Err(e) => Ok(TaskResult {
                task_id: task_id.clone(),
                success: false,
                output: serde_json::json!({"error": format!("{}", e)}).to_string(),
                artifacts: vec![],
                execution_time_ms: duration_ms,
            }),
        }
    }

    fn can_execute(&self, task: &DagTask) -> bool {
        task.task_type == TaskType::SkillExecution || task.task_type == TaskType::WorkflowExecution
    }

    fn executor_id(&self) -> &str {
        "workflow_dag_executor"
    }
}

/// Convert a `WorkflowDefinition` into a `DagWorkflow` for the scheduler.
pub fn to_dag_workflow(
    definition: &WorkflowDefinition,
    trigger_context: Value,
) -> Result<DagWorkflow, AgentError> {
    let mut builder = DagWorkflowBuilder::new(&definition.name)
        .description(&definition.description)
        .config(WorkflowConfig {
            max_concurrency: 5,
            task_timeout_sec: definition.config.timeout_sec.unwrap_or(300),
            workflow_timeout_sec: definition.config.timeout_sec.unwrap_or(300).saturating_mul(2),
            retry_policy: crate::queue::dag_scheduler::TaskRetryPolicy {
                max_retries: definition.config.max_retries.unwrap_or(0),
                ..Default::default()
            },
            enable_replanning: false,
            continue_on_failure: definition.config.continue_on_failure,
        });

    // Build task list preserving order; dependencies are set via builder
    let mut last_deps: Vec<String> = Vec::new();

    for step in &definition.steps {
        let mut parameters: HashMap<String, Value> = match &step.params {
            Value::Object(map) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            Value::String(s) => {
                let mut map = HashMap::new();
                map.insert("input".to_string(), Value::String(s.clone()));
                map
            }
            other => {
                let mut map = HashMap::new();
                map.insert("input".to_string(), other.clone());
                map
            }
        };

        // Embed workflow metadata for the executor
        parameters.insert("_skill".to_string(), Value::String(step.skill.clone()));
        if let Some(ref condition) = step.condition {
            parameters.insert("_condition".to_string(), Value::String(condition.clone()));
        }
        parameters.insert("_trigger_context".to_string(), trigger_context.clone());

        let task = DagTask {
            id: step.id.clone(),
            name: step.name.clone(),
            description: String::new(),
            task_type: TaskType::SkillExecution,
            parameters,
            priority: crate::queue::dag_scheduler::TaskPriority::Normal,
            estimated_duration_sec: step.timeout_sec,
            required_capabilities: vec!["skill:call".to_string()],
            resource_requirements: crate::queue::dag_scheduler::ResourceRequirements::default(),
        };

        let deps = step.depends_on.clone().unwrap_or_default();
        if !deps.is_empty() {
            builder = builder.depends_on(deps);
        } else if !last_deps.is_empty() {
            // Reset dependencies if this step has none but previous had some
            builder = builder.depends_on(Vec::new());
        }
        builder = builder.add_task(task);
        last_deps = step.depends_on.clone().unwrap_or_default();
    }

    Ok(builder.build())
}

/// Poll a scheduler workflow instance and mirror its state into a
/// `workflow::state::WorkflowInstance`.
pub async fn poll_scheduler_workflow(
    scheduler: &DagScheduler,
    scheduler_instance_id: &str,
    workflow_id: &str,
    trigger_context: Value,
) -> Result<WorkflowInstance, AgentError> {
    let mut instance = WorkflowInstance::new(workflow_id.to_string(), trigger_context);
    instance.mark_running();

    let workflow_timeout_sec = 300u64;
    let mut poll_interval_ms = 100u64;
    let max_poll_interval_ms = 2000u64;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(workflow_timeout_sec);

    loop {
        if tokio::time::Instant::now() >= deadline {
            break;
        }

        if let Some(scheduler_instance) = scheduler.get_instance(scheduler_instance_id).await {
            // Mirror task states into step states
            for (task_id, task_state) in &scheduler_instance.task_states {
                let step_status = match task_state.status {
                    crate::queue::dag_scheduler::TaskExecutionStatus::Pending => StepStatus::Pending,
                    crate::queue::dag_scheduler::TaskExecutionStatus::Ready => StepStatus::Ready,
                    crate::queue::dag_scheduler::TaskExecutionStatus::Running => StepStatus::Running,
                    crate::queue::dag_scheduler::TaskExecutionStatus::Completed => StepStatus::Completed,
                    crate::queue::dag_scheduler::TaskExecutionStatus::Failed => StepStatus::Failed,
                    crate::queue::dag_scheduler::TaskExecutionStatus::Cancelled => StepStatus::Cancelled,
                    crate::queue::dag_scheduler::TaskExecutionStatus::WaitingRetry => StepStatus::Ready,
                };

                let mut step_state = instance
                    .step_states
                    .get(task_id)
                    .cloned()
                    .unwrap_or_else(|| StepState::new(task_id));

                step_state.status = step_status;

                if let Some(ref result) = task_state.result {
                    let output = serde_json::Value::String(result.output.clone());
                    step_state.output = Some(output);
                    step_state.execution_time_ms = Some(result.execution_time_ms);
                }

                if let Some(ref error) = task_state.error {
                    step_state.error = Some(error.clone());
                    instance.add_error(Some(task_id.clone()), error.clone());
                }

                instance.step_states.insert(task_id.clone(), step_state);
            }

            // Check if workflow is complete
            match scheduler_instance.status {
                crate::queue::dag_scheduler::WorkflowStatus::Completed => {
                    instance.mark_completed();
                    return Ok(instance);
                }
                crate::queue::dag_scheduler::WorkflowStatus::Failed => {
                    instance.mark_failed();
                    return Ok(instance);
                }
                crate::queue::dag_scheduler::WorkflowStatus::Cancelled => {
                    instance.status = WorkflowStatus::Cancelled;
                    instance.completed_at = Some(chrono::Utc::now());
                    return Ok(instance);
                }
                _ => {
                    // Still running — continue polling
                }
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(poll_interval_ms)).await;
        poll_interval_ms = (poll_interval_ms * 2).min(max_poll_interval_ms);
    }

    // Timeout
    warn!(
        "Workflow {} (scheduler instance {}) timed out after {}s",
        workflow_id, scheduler_instance_id, workflow_timeout_sec
    );
    instance.mark_failed();
    instance.add_error(None, format!("Workflow timed out after {}s", workflow_timeout_sec));
    Ok(instance)
}

/// Extract skill input string and parameter map from resolved JSON params.
/// Mirrors `WorkflowEngine::extract_input_and_params`.
fn extract_input_and_params(params: &Value) -> (String, HashMap<String, String>) {
    match params {
        Value::Object(map) => {
            let mut params_map = HashMap::new();
            for (k, v) in map.iter() {
                params_map.insert(k.clone(), v.as_str().unwrap_or(&v.to_string()).to_string());
            }
            let input = params_map.remove("input").unwrap_or_default();
            (input, params_map)
        }
        Value::String(s) => (s.clone(), HashMap::new()),
        other => (other.to_string(), HashMap::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::definition::{TriggerDefinition, TriggerType, WorkflowGlobalConfig, WorkflowStep};

    fn sample_workflow() -> WorkflowDefinition {
        WorkflowDefinition {
            id: "test_wf".to_string(),
            name: "Test Workflow".to_string(),
            description: "A test workflow".to_string(),
            version: "1.0.0".to_string(),
            author: None,
            tags: vec![],
            triggers: vec![],
            config: WorkflowGlobalConfig {
                timeout_sec: Some(60),
                max_retries: Some(1),
                continue_on_failure: false,
                notify_on_complete: false,
            },
            steps: vec![
                WorkflowStep {
                    id: "step1".to_string(),
                    name: "Step 1".to_string(),
                    skill: "echo".to_string(),
                    params: serde_json::json!({"input": "hello"}),
                    depends_on: None,
                    condition: None,
                    timeout_sec: Some(10),
                    retries: Some(1),
                },
                WorkflowStep {
                    id: "step2".to_string(),
                    name: "Step 2".to_string(),
                    skill: "uppercase".to_string(),
                    params: serde_json::json!({"input": "{{steps.step1.output}}"}),
                    depends_on: Some(vec!["step1".to_string()]),
                    condition: None,
                    timeout_sec: None,
                    retries: None,
                },
            ],
        }
    }

    #[test]
    fn test_to_dag_workflow_conversion() {
        let def = sample_workflow();
        let dag = to_dag_workflow(&def, serde_json::Value::Null).unwrap();

        assert_eq!(dag.name, "Test Workflow");
        assert_eq!(dag.tasks.len(), 2);
        assert_eq!(dag.tasks[0].id, "step1");
        assert_eq!(dag.tasks[1].id, "step2");
        assert_eq!(
            dag.dependencies.get("step2"),
            Some(&vec!["step1".to_string()])
        );

        // Check metadata encoding
        assert_eq!(
            dag.tasks[0].parameters.get("_skill"),
            Some(&Value::String("echo".to_string()))
        );
        assert_eq!(
            dag.tasks[1].parameters.get("_skill"),
            Some(&Value::String("uppercase".to_string()))
        );
    }

    #[test]
    fn test_workflow_dag_executor_identity() {
        let agent = Arc::new(crate::AgentBuilder::new("test").build());
        let (scheduler, _rx) = crate::queue::dag_scheduler::DagScheduler::new(
            crate::queue::dag_scheduler::SchedulerConfig::default()
        );
        let executor = WorkflowDagExecutor::new(agent, Arc::new(scheduler));
        assert_eq!(executor.executor_id(), "workflow_dag_executor");
    }

    #[test]
    fn test_workflow_dag_executor_can_execute() {
        let agent = Arc::new(crate::AgentBuilder::new("test").build());
        let (scheduler, _rx) = crate::queue::dag_scheduler::DagScheduler::new(
            crate::queue::dag_scheduler::SchedulerConfig::default()
        );
        let executor = WorkflowDagExecutor::new(agent, Arc::new(scheduler));

        let skill_task = crate::queue::dag_scheduler::DagTask {
            id: "t1".to_string(),
            name: "Test".to_string(),
            description: String::new(),
            task_type: crate::task::TaskType::SkillExecution,
            parameters: HashMap::new(),
            priority: crate::queue::dag_scheduler::TaskPriority::Normal,
            estimated_duration_sec: None,
            required_capabilities: vec![],
            resource_requirements: crate::queue::dag_scheduler::ResourceRequirements::default(),
        };
        assert!(executor.can_execute(&skill_task));

        let wf_task = crate::queue::dag_scheduler::DagTask {
            task_type: crate::task::TaskType::WorkflowExecution,
            ..skill_task.clone()
        };
        assert!(executor.can_execute(&wf_task));

        let other_task = crate::queue::dag_scheduler::DagTask {
            task_type: crate::task::TaskType::Custom("other".to_string()),
            ..skill_task.clone()
        };
        assert!(!executor.can_execute(&other_task));
    }

    #[test]
    fn test_extract_input_and_params() {
        let params = serde_json::json!({
            "input": "hello world",
            "style": "bullet_points"
        });
        let (input, params_map) = extract_input_and_params(&params);
        assert_eq!(input, "hello world");
        assert!(!params_map.contains_key("input"));
        assert_eq!(params_map.get("style"), Some(&"bullet_points".to_string()));
    }
}
