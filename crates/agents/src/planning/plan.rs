//! Plan Definition and Structure
//!
//! This module defines the core plan data structures including:
//! - Plan identification and metadata
//! - Plan steps and their types
//! - Plan status and lifecycle
//! - Actions that can be executed within steps

use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Unique plan identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlanId(pub String);

impl PlanId {
    /// Generate new plan ID
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Create from string
    pub fn from_string(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl Default for PlanId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for PlanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Plan status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanStatus {
    /// Plan created but not started
    Created,
    /// Plan is being executed
    InProgress,
    /// Plan paused waiting for condition
    Paused,
    /// All steps completed successfully
    Completed,
    /// Plan execution failed
    Failed,
    /// Plan was cancelled
    Cancelled,
    /// Plan is being replanned
    Replanning,
}

impl PlanStatus {
    /// Check if plan is active
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            PlanStatus::Created | PlanStatus::InProgress | PlanStatus::Replanning
        )
    }

    /// Check if plan is terminal
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            PlanStatus::Completed | PlanStatus::Failed | PlanStatus::Cancelled
        )
    }
}

impl fmt::Display for PlanStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanStatus::Created => write!(f, "created"),
            PlanStatus::InProgress => write!(f, "in_progress"),
            PlanStatus::Paused => write!(f, "paused"),
            PlanStatus::Completed => write!(f, "completed"),
            PlanStatus::Failed => write!(f, "failed"),
            PlanStatus::Cancelled => write!(f, "cancelled"),
            PlanStatus::Replanning => write!(f, "replanning"),
        }
    }
}

/// Priority level for plan steps
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    Critical = 0,
    High = 1,
    Normal = 2,
    Low = 3,
}

impl Default for Priority {
    fn default() -> Self {
        Priority::Normal
    }
}

/// A plan consists of multiple steps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    /// Plan ID
    pub id: PlanId,
    /// Plan name/description
    pub name: String,
    /// Plan goal
    pub goal: String,
    /// Plan steps
    pub steps: Vec<PlanStep>,
    /// Step dependencies (step_index -> [dependency_indices])
    pub dependencies: HashMap<usize, Vec<usize>>,
    /// Current status
    pub status: PlanStatus,
    /// Priority
    pub priority: Priority,
    /// Maximum execution time
    pub timeout: Option<Duration>,
    /// Metadata
    pub metadata: HashMap<String, serde_json::Value>,
    /// Created at
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Started at
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Completed at
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// ARCHITECTURE FIX: TTL for automatic cleanup (plans older than this are
    /// removed)
    pub ttl_secs: Option<u64>,
    /// ARCHITECTURE FIX: Last accessed time (for LRU eviction)
    pub last_accessed_at: chrono::DateTime<chrono::Utc>,
}

impl Plan {
    /// Create new plan
    pub fn new(name: impl Into<String>, goal: impl Into<String>) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: PlanId::new(),
            name: name.into(),
            goal: goal.into(),
            steps: Vec::new(),
            dependencies: HashMap::new(),
            status: PlanStatus::Created,
            priority: Priority::Normal,
            timeout: None,
            metadata: HashMap::new(),
            created_at: now,
            started_at: None,
            completed_at: None,
            // ARCHITECTURE FIX: Default TTL of 24 hours for automatic cleanup
            ttl_secs: Some(86400),
            last_accessed_at: now,
        }
    }

    /// ARCHITECTURE FIX: Check if plan has expired based on TTL
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl_secs {
            let elapsed = (chrono::Utc::now() - self.created_at).num_seconds() as u64;
            elapsed > ttl
        } else {
            false
        }
    }

    /// ARCHITECTURE FIX: Update last accessed time
    pub fn touch(&mut self) {
        self.last_accessed_at = chrono::Utc::now();
    }

    /// ARCHITECTURE FIX: Get age in seconds
    pub fn age_secs(&self) -> u64 {
        (chrono::Utc::now() - self.created_at).num_seconds() as u64
    }

    /// Add a step to the plan
    pub fn add_step(&mut self, step: PlanStep) -> usize {
        let index = self.steps.len();
        self.steps.push(step);
        index
    }

    /// Add step with dependencies
    pub fn add_step_with_deps(
        &mut self,
        step: PlanStep,
        deps: Vec<usize>,
    ) -> Result<usize, PlanningError> {
        // Validate dependencies exist
        for &dep in &deps {
            if dep >= self.steps.len() {
                return Err(PlanningError::InvalidDependency {
                    step: self.steps.len(),
                    dependency: dep,
                });
            }
        }

        let index = self.steps.len();
        self.steps.push(step);
        if !deps.is_empty() {
            self.dependencies.insert(index, deps);
        }
        Ok(index)
    }

    /// Get ready steps (dependencies satisfied)
    pub fn get_ready_steps(&self, completed: &[usize]) -> Vec<usize> {
        self.steps
            .iter()
            .enumerate()
            .filter(|(i, step)| {
                !step.is_completed()
                    && !completed.contains(i)
                    && self
                        .dependencies
                        .get(i)
                        .map_or(true, |deps| deps.iter().all(|d| completed.contains(d)))
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Get completion percentage
    pub fn completion_pct(&self) -> f32 {
        if self.steps.is_empty() {
            return 0.0;
        }
        let completed = self.steps.iter().filter(|s| s.is_completed()).count();
        (completed as f32 / self.steps.len() as f32) * 100.0
    }

    /// Check if all steps completed
    pub fn is_complete(&self) -> bool {
        self.steps.iter().all(|s| s.is_completed())
    }

    /// Mark plan as started
    pub fn mark_started(&mut self) {
        self.status = PlanStatus::InProgress;
        self.started_at = Some(chrono::Utc::now());
    }

    /// Mark plan as completed
    pub fn mark_completed(&mut self) {
        self.status = PlanStatus::Completed;
        self.completed_at = Some(chrono::Utc::now());
    }

    /// Mark plan as failed
    pub fn mark_failed(&mut self) {
        self.status = PlanStatus::Failed;
        self.completed_at = Some(chrono::Utc::now());
    }
}

/// Individual plan step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    /// Step ID
    pub id: String,
    /// Step description
    pub description: String,
    /// Step type
    pub step_type: StepType,
    /// Step status
    pub status: StepStatus,
    /// Required tools/actions
    pub actions: Vec<Action>,
    /// Expected output
    pub expected_output: Option<String>,
    /// Priority
    pub priority: Priority,
    /// Estimated duration
    pub estimated_duration: Option<Duration>,
    /// Actual start time
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Actual completion time
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Result data
    pub result: Option<serde_json::Value>,
    /// Error if failed
    pub error: Option<String>,
}

impl PlanStep {
    /// Create new step
    pub fn new(id: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            step_type: StepType::Action,
            status: StepStatus::Pending,
            actions: Vec::new(),
            expected_output: None,
            priority: Priority::Normal,
            estimated_duration: None,
            started_at: None,
            completed_at: None,
            result: None,
            error: None,
        }
    }

    /// Create reasoning step
    pub fn reasoning(description: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            description: description.into(),
            step_type: StepType::Reasoning,
            status: StepStatus::Pending,
            actions: Vec::new(),
            expected_output: None,
            priority: Priority::Normal,
            estimated_duration: None,
            started_at: None,
            completed_at: None,
            result: None,
            error: None,
        }
    }

    /// Create decision step
    pub fn decision(description: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            description: description.into(),
            step_type: StepType::Decision,
            status: StepStatus::Pending,
            actions: Vec::new(),
            expected_output: None,
            priority: Priority::High,
            estimated_duration: None,
            started_at: None,
            completed_at: None,
            result: None,
            error: None,
        }
    }

    /// Add action to step
    pub fn with_action(mut self, action: Action) -> Self {
        self.actions.push(action);
        self
    }

    /// Check if step completed
    pub fn is_completed(&self) -> bool {
        matches!(self.status, StepStatus::Completed | StepStatus::Skipped)
    }

    /// Mark as started
    pub fn mark_started(&mut self) {
        self.status = StepStatus::InProgress;
        self.started_at = Some(chrono::Utc::now());
    }

    /// Mark as completed with result
    pub fn mark_completed(&mut self, result: Option<serde_json::Value>) {
        self.status = StepStatus::Completed;
        self.completed_at = Some(chrono::Utc::now());
        self.result = result;
    }

    /// Mark as failed
    pub fn mark_failed(&mut self, error: impl Into<String>) {
        self.status = StepStatus::Failed;
        self.completed_at = Some(chrono::Utc::now());
        self.error = Some(error.into());
    }
}

/// Step type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepType {
    /// Reasoning/thinking step
    Reasoning,
    /// Action execution step
    Action,
    /// Decision making step
    Decision,
    /// Information gathering step
    Information,
    /// Validation step
    Validation,
}

/// Step execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
    Blocked,
}

/// Action to be executed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    /// Use a tool
    ToolUse {
        tool_name: String,
        parameters: HashMap<String, serde_json::Value>,
    },
    /// Call LLM for reasoning
    LLMReasoning {
        prompt: String,
        context: HashMap<String, serde_json::Value>,
    },
    /// Execute sub-plan
    SubPlan { plan_id: PlanId },
    /// Delegate to another agent
    Delegate { agent_id: String, task: String },
    /// Wait for condition
    Wait {
        condition: String,
        timeout: Option<Duration>,
    },
    /// User interaction
    UserInteraction { question: String },
}

/// Planning error types
#[derive(Debug, Clone)]
pub enum PlanningError {
    InvalidPlan(String),
    InvalidDependency { step: usize, dependency: usize },
    CircularDependency,
    StepFailed { step: usize, reason: String },
    Timeout,
    RePlanningRequired(String),
    ExecutionFailed(String),
}

impl fmt::Display for PlanningError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanningError::InvalidPlan(msg) => write!(f, "Invalid plan: {}", msg),
            PlanningError::InvalidDependency { step, dependency } => {
                write!(f, "Step {} has invalid dependency {}", step, dependency)
            }
            PlanningError::CircularDependency => write!(f, "Circular dependency detected"),
            PlanningError::StepFailed { step, reason } => {
                write!(f, "Step {} failed: {}", step, reason)
            }
            PlanningError::Timeout => write!(f, "Plan execution timed out"),
            PlanningError::RePlanningRequired(reason) => {
                write!(f, "Re-planning required: {}", reason)
            }
            PlanningError::ExecutionFailed(msg) => write!(f, "Execution failed: {}", msg),
        }
    }
}

impl std::error::Error for PlanningError {}

/// Planning result type
pub type PlanningResult<T> = Result<T, PlanningError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_creation() {
        let plan = Plan::new("Test Plan", "Test goal");
        assert_eq!(plan.status, PlanStatus::Created);
        assert!(plan.steps.is_empty());
    }

    #[test]
    fn test_add_steps() {
        let mut plan = Plan::new("Test", "Goal");
        let step1 = PlanStep::new("1", "First step");
        let step2 = PlanStep::new("2", "Second step");

        let idx1 = plan.add_step(step1);
        let idx2 = plan.add_step_with_deps(step2, vec![idx1]).unwrap();

        assert_eq!(idx1, 0);
        assert_eq!(idx2, 1);
        assert_eq!(plan.dependencies.get(&1), Some(&vec![0]));
    }

    #[test]
    fn test_ready_steps() {
        let mut plan = Plan::new("Test", "Goal");
        plan.add_step(PlanStep::new("1", "Step 1"));
        plan.add_step(PlanStep::new("2", "Step 2"));
        plan.add_step_with_deps(PlanStep::new("3", "Step 3"), vec![0, 1])
            .unwrap();

        let ready = plan.get_ready_steps(&[]);
        assert_eq!(ready.len(), 2); // Steps 0 and 1
        assert!(ready.contains(&0));
        assert!(ready.contains(&1));

        let ready = plan.get_ready_steps(&[0, 1]);
        assert_eq!(ready, vec![2]);
    }

    #[test]
    fn test_invalid_dependency() {
        let mut plan = Plan::new("Test", "Goal");
        plan.add_step(PlanStep::new("1", "Step 1"));

        let result = plan.add_step_with_deps(PlanStep::new("2", "Step 2"), vec![5]);
        assert!(result.is_err());
    }

    #[test]
    fn test_step_lifecycle() {
        let mut step = PlanStep::new("1", "Test step");
        assert_eq!(step.status, StepStatus::Pending);

        step.mark_started();
        assert_eq!(step.status, StepStatus::InProgress);

        step.mark_completed(Some(serde_json::json!("result")));
        assert_eq!(step.status, StepStatus::Completed);
        assert!(step.is_completed());
    }

    #[test]
    fn test_plan_status() {
        assert!(PlanStatus::InProgress.is_active());
        assert!(PlanStatus::Completed.is_terminal());
        assert!(!PlanStatus::Created.is_terminal());
    }

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::Critical < Priority::High);
        assert!(Priority::High < Priority::Normal);
        assert!(Priority::Normal < Priority::Low);
    }
}
