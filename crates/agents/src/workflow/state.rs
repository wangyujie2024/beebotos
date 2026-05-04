//! Workflow State Management
//!
//! Runtime state models for workflow instances and step states.
//!
//! Note: Uses `WorkflowInstance` and `WorkflowStatus` to avoid
//! naming conflicts with `dag_scheduler::WorkflowInstance`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Type alias for workflow identifiers
pub type WorkflowId = String;

/// Type alias for workflow instance identifiers
pub type WorkflowInstanceId = String;

/// Workflow execution status (distinct from dag_scheduler::WorkflowStatus)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl WorkflowStatus {
    /// Check if workflow is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self, WorkflowStatus::Completed | WorkflowStatus::Failed | WorkflowStatus::Cancelled)
    }

    /// Check if workflow is active
    pub fn is_active(&self) -> bool {
        matches!(self, WorkflowStatus::Pending | WorkflowStatus::Running)
    }
}

impl std::fmt::Display for WorkflowStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkflowStatus::Pending => write!(f, "pending"),
            WorkflowStatus::Running => write!(f, "running"),
            WorkflowStatus::Completed => write!(f, "completed"),
            WorkflowStatus::Failed => write!(f, "failed"),
            WorkflowStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Individual step execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Ready,
    Running,
    Completed,
    Failed,
    Skipped,
    Cancelled,
}

impl StepStatus {
    /// Check if step is completed (successfully or skipped)
    pub fn is_completed(&self) -> bool {
        matches!(self, StepStatus::Completed | StepStatus::Skipped)
    }

    /// Check if step failed
    pub fn is_failed(&self) -> bool {
        matches!(self, StepStatus::Failed)
    }

    /// Check if step is terminal
    pub fn is_terminal(&self) -> bool {
        matches!(self, StepStatus::Completed | StepStatus::Failed | StepStatus::Skipped | StepStatus::Cancelled)
    }
}

impl std::fmt::Display for StepStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StepStatus::Pending => write!(f, "pending"),
            StepStatus::Ready => write!(f, "ready"),
            StepStatus::Running => write!(f, "running"),
            StepStatus::Completed => write!(f, "completed"),
            StepStatus::Failed => write!(f, "failed"),
            StepStatus::Skipped => write!(f, "skipped"),
            StepStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Workflow error record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowError {
    pub step_id: Option<String>,
    pub message: String,
    pub timestamp: DateTime<Utc>,
}

/// Runtime state of a workflow execution instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowInstance {
    pub id: WorkflowInstanceId,
    pub workflow_id: WorkflowId,
    pub status: WorkflowStatus,
    pub step_states: HashMap<String, StepState>,
    pub trigger_context: serde_json::Value,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_log: Vec<WorkflowError>,
}

impl WorkflowInstance {
    /// Create a new workflow execution instance
    pub fn new(workflow_id: String, trigger_context: serde_json::Value) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            workflow_id,
            status: WorkflowStatus::Pending,
            step_states: HashMap::new(),
            trigger_context,
            started_at: Utc::now(),
            completed_at: None,
            error_log: Vec::new(),
        }
    }

    /// Mark workflow as running
    pub fn mark_running(&mut self) {
        self.status = WorkflowStatus::Running;
    }

    /// Mark workflow as completed
    pub fn mark_completed(&mut self) {
        self.status = WorkflowStatus::Completed;
        self.completed_at = Some(Utc::now());
    }

    /// Mark workflow as failed
    pub fn mark_failed(&mut self) {
        self.status = WorkflowStatus::Failed;
        self.completed_at = Some(Utc::now());
    }

    /// Mark workflow as cancelled
    pub fn mark_cancelled(&mut self) {
        self.status = WorkflowStatus::Cancelled;
        self.completed_at = Some(Utc::now());
    }

    /// Add an error to the log
    pub fn add_error(&mut self, step_id: Option<String>, message: impl Into<String>) {
        self.error_log.push(WorkflowError {
            step_id,
            message: message.into(),
            timestamp: Utc::now(),
        });
    }

    /// Get a step state (create if missing)
    pub fn get_step_state(&mut self, step_id: &str) -> &mut StepState {
        self.step_states
            .entry(step_id.to_string())
            .or_insert_with(|| StepState::new(step_id))
    }

    /// Check if any step has failed
    pub fn any_failed(&self) -> bool {
        self.step_states.values().any(|s| s.status.is_failed())
    }

    /// Get duration in seconds (if completed)
    pub fn duration_secs(&self) -> u64 {
        let end = self.completed_at.unwrap_or_else(Utc::now);
        (end - self.started_at).num_seconds().max(0) as u64
    }

    /// Calculate completion percentage
    pub fn completion_pct(&self) -> f32 {
        if self.step_states.is_empty() {
            return 0.0;
        }
        let terminal = self.step_states.values().filter(|s| s.status.is_terminal()).count();
        (terminal as f32 / self.step_states.len() as f32) * 100.0
    }
}

/// Runtime state of a single workflow step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepState {
    pub step_id: String,
    pub status: StepStatus,
    pub input: serde_json::Value,
    pub output: Option<serde_json::Value>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub execution_time_ms: Option<u64>,
    pub retry_count: u32,
    pub error: Option<String>,
}

impl StepState {
    /// Create a new step state
    pub fn new(step_id: &str) -> Self {
        Self {
            step_id: step_id.to_string(),
            status: StepStatus::Pending,
            input: serde_json::Value::Null,
            output: None,
            started_at: None,
            completed_at: None,
            execution_time_ms: None,
            retry_count: 0,
            error: None,
        }
    }

    /// Mark step as ready (dependencies satisfied)
    pub fn mark_ready(&mut self) {
        self.status = StepStatus::Ready;
    }

    /// Mark step as running
    pub fn mark_running(&mut self) {
        self.status = StepStatus::Running;
        self.started_at = Some(Utc::now());
    }

    /// Mark step as completed with output
    pub fn mark_completed(&mut self, output: serde_json::Value) {
        self.status = StepStatus::Completed;
        self.output = Some(output);
        self.completed_at = Some(Utc::now());
    }

    /// Mark step as failed
    pub fn mark_failed(&mut self, error: impl Into<String>) {
        self.status = StepStatus::Failed;
        self.error = Some(error.into());
        self.completed_at = Some(Utc::now());
    }

    /// Mark step as skipped
    pub fn mark_skipped(&mut self) {
        self.status = StepStatus::Skipped;
        self.completed_at = Some(Utc::now());
    }

    /// Mark step as cancelled
    pub fn mark_cancelled(&mut self) {
        self.status = StepStatus::Cancelled;
        self.completed_at = Some(Utc::now());
    }

    /// Increment retry count
    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
    }

    /// Get duration in seconds
    pub fn duration_secs(&self) -> u64 {
        match (self.started_at, self.completed_at) {
            (Some(start), Some(end)) => (end - start).num_seconds().max(0) as u64,
            (Some(start), None) => (Utc::now() - start).num_seconds().max(0) as u64,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_instance_lifecycle() {
        let mut instance = WorkflowInstance::new("test_workflow".to_string(), serde_json::Value::Null);
        assert_eq!(instance.status, WorkflowStatus::Pending);

        instance.mark_running();
        assert_eq!(instance.status, WorkflowStatus::Running);

        instance.mark_completed();
        assert!(instance.status.is_terminal());
        assert!(instance.completed_at.is_some());
    }

    #[test]
    fn test_step_state_lifecycle() {
        let mut step = StepState::new("step1");
        assert_eq!(step.status, StepStatus::Pending);

        step.mark_running();
        assert_eq!(step.status, StepStatus::Running);

        step.mark_completed(serde_json::json!("result"));
        assert_eq!(step.status, StepStatus::Completed);
        assert_eq!(step.output, Some(serde_json::json!("result")));
    }

    #[test]
    fn test_any_failed() {
        let mut instance = WorkflowInstance::new("test".to_string(), serde_json::Value::Null);
        let mut step1 = StepState::new("step1");
        step1.mark_completed(serde_json::Value::Null);
        instance.step_states.insert("step1".to_string(), step1);

        let mut step2 = StepState::new("step2");
        step2.mark_failed("error");
        instance.step_states.insert("step2".to_string(), step2);

        assert!(instance.any_failed());
    }
}
