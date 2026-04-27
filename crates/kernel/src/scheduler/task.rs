//! Task Structure
//!
//! Task control block for scheduling.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

// Re-export canonical types to avoid duplication
pub use crate::capabilities::CapabilityLevel;
pub use crate::capabilities::CapabilitySet;
pub use crate::task::TaskId;

/// Task priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    /// Real-time priority (highest)
    RealTime = 0,
    /// High priority
    High = 1,
    /// Normal priority
    Normal = 2,
    /// Low priority
    Low = 3,
    /// Idle priority (lowest)
    Idle = 4,
}

impl Priority {
    /// Get numeric priority level
    pub fn level(&self) -> i32 {
        *self as i32
    }
}

// TaskId is re-exported from crate::task

/// Task control block
#[derive(Debug, Clone)]
pub struct Task {
    /// Task ID
    pub id: TaskId,
    /// Task name
    pub name: String,
    /// Task priority
    pub priority: Priority,
    /// Current task state
    pub state: TaskState,
    /// Virtual runtime for CFS
    pub vruntime: u64,
    /// CPU time consumed
    pub cpu_time: Duration,
    /// Creation timestamp
    pub created_at: Instant,
    /// Capability set
    pub capabilities: CapabilitySet,
    /// Optional deadline for EDF scheduling
    pub deadline: Option<Instant>,
}

/// Task state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskState {
    /// Task is currently executing
    Running,
    /// Task is ready to run but not currently executing
    Ready,
    /// Task is blocked waiting for some event or resource
    Blocked(BlockReason),
    /// Task has completed but not yet cleaned up
    Zombie,
}

/// Block reason
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockReason {
    /// Blocked waiting for I/O operation
    Io,
    /// Blocked due to sleep
    Sleep,
    /// Blocked waiting for an event
    WaitForEvent,
    /// Blocked waiting for a resource
    WaitForResource,
}

// CapabilityLevel and CapabilitySet are re-exported from crate::capabilities
// to ensure consistency across the kernel.

impl Task {
    /// Create new task
    pub fn new(id: TaskId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            priority: Priority::Normal,
            state: TaskState::Ready,
            vruntime: 0,
            cpu_time: Duration::ZERO,
            created_at: Instant::now(),
            capabilities: CapabilitySet::empty(),
            deadline: None,
        }
    }

    /// Set deadline for EDF scheduling
    pub fn with_deadline(mut self, deadline: Instant) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Set capabilities
    pub fn with_capabilities(mut self, caps: CapabilitySet) -> Self {
        self.capabilities = caps;
        self
    }

    /// Check if can execute with required capability
    pub fn can_execute(&self, required: CapabilityLevel) -> bool {
        self.capabilities.has(required)
    }

    /// Get elapsed time since creation
    pub fn elapsed(&self) -> Duration {
        self.created_at.elapsed()
    }
}

/// Task builder
pub struct TaskBuilder {
    task: Task,
}

impl TaskBuilder {
    /// Create a new task builder
    pub fn new(name: impl Into<String>) -> Self {
        let id = TaskId::new(rand::random::<u64>());
        Self {
            task: Task::new(id, name),
        }
    }

    /// Set task priority
    pub fn priority(mut self, priority: Priority) -> Self {
        self.task.priority = priority;
        self
    }

    /// Set capability level for the task
    pub fn capability(mut self, level: CapabilityLevel) -> Self {
        self.task.capabilities = self.task.capabilities.with_level(level);
        self
    }

    /// Build the task
    pub fn build(self) -> Task {
        self.task
    }
}
