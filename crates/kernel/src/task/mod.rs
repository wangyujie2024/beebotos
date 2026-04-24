//! Task Management
//!
//! Processes, threads, and scheduling.

pub mod fork;
pub mod process;
pub mod signal;
pub mod syscall;
pub mod thread;
pub mod wait;

pub use process::Process;
pub use thread::Thread;

use crate::error::Result;

/// Task ID - unique identifier for a task/process
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct TaskId(pub u64);

impl TaskId {
    /// Create a new TaskId
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the underlying u64 value
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Initialize task management subsystem
pub fn init() -> Result<()> {
    tracing::info!("Initializing task management");
    Ok(())
}
