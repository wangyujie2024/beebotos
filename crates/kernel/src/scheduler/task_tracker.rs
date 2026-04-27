//! Task status tracking for monitoring

use std::collections::HashMap;

use tokio::sync::{broadcast, RwLock};

use crate::scheduler::TaskId;

/// Task status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    /// Task is pending execution
    Pending,
    /// Task is currently running
    Running,
    /// Task completed successfully
    Completed,
    /// Task failed
    Failed,
    /// Task was cancelled
    Cancelled,
    /// Task timed out
    TimedOut,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "pending"),
            TaskStatus::Running => write!(f, "running"),
            TaskStatus::Completed => write!(f, "completed"),
            TaskStatus::Failed => write!(f, "failed"),
            TaskStatus::Cancelled => write!(f, "cancelled"),
            TaskStatus::TimedOut => write!(f, "timed_out"),
        }
    }
}

/// Task information
#[derive(Debug, Clone)]
pub struct TaskInfo {
    /// Task ID
    pub task_id: TaskId,
    /// Task name
    pub name: String,
    /// Current task status
    pub status: TaskStatus,
    /// Task priority
    pub priority: crate::scheduler::Priority,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Start timestamp (if started)
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Completion timestamp (if completed)
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Worker ID assigned (if assigned)
    pub worker_id: Option<usize>,
    /// Task result (if completed)
    pub result: Option<Result<(), String>>,
}

impl TaskInfo {
    /// Get duration if completed
    pub fn duration(&self) -> Option<std::time::Duration> {
        match (self.started_at, self.completed_at) {
            (Some(start), Some(end)) => Some((end - start).to_std().unwrap_or_default()),
            _ => None,
        }
    }
}

/// Task status change event
#[derive(Debug, Clone)]
pub enum TaskStatusEvent {
    /// Task was registered
    TaskRegistered {
        /// Task ID
        task_id: TaskId,
    },
    /// Task status changed
    StatusChanged {
        /// Task ID
        task_id: TaskId,
        /// Previous status
        old_status: TaskStatus,
        /// New status
        new_status: TaskStatus,
    },
    /// Task completed
    TaskCompleted {
        /// Task ID
        task_id: TaskId,
        /// Whether task succeeded
        success: bool,
    },
}

/// Task status tracker
#[allow(dead_code)]
pub struct TaskStatusTracker {
    tasks: RwLock<HashMap<TaskId, TaskInfo>>,
    event_sender: broadcast::Sender<TaskStatusEvent>,
}

impl TaskStatusTracker {
    /// Create new tracker
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1000);
        Self {
            tasks: RwLock::new(HashMap::new()),
            event_sender: sender,
        }
    }
}

impl Default for TaskStatusTracker {
    fn default() -> Self {
        Self::new()
    }
}
