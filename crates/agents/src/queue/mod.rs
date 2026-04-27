//! Queue Management Module
//!
//! Multi-queue concurrency system:
//! - Main queue: Sequential execution
//! - Cron queue: Scheduled tasks (using `crate::scheduling::cron`)
//! - Subagent queue: Parallel execution (max 5)
//! - Nested queue: Recursion prevention
//! - 🟢 P0 FIX: DAG Scheduler for explicit task dependencies

pub mod dead_letter;
pub mod main_queue;
pub mod manager;
pub mod nested;
pub mod subagent;

// RELIABILITY FIX: Worker task functions for panic recovery
pub mod worker_tasks;

// 🟢 P0 FIX: DAG Task Scheduler
pub mod dag_scheduler;

// 🟢 P0 FIX: Re-export DAG scheduler types
pub use dag_scheduler::{
    DagScheduler, DagTask, DagWorkflow, DagWorkflowBuilder, ResourceRequirements, SchedulerConfig,
    SchedulerError, SchedulerEvent, SchedulerMetrics, TaskExecutionRequest, TaskExecutionStatus,
    TaskExecutor, TaskPriority, TaskRetryPolicy, WorkflowConfig, WorkflowInstance, WorkflowStatus,
};
// ARCHITECTURE FIX: Re-export dead letter queue types
pub use dead_letter::{DLQConfig, DLQStats, DLQTaskProcessor, DeadLetterEntry, DeadLetterQueue};
pub use manager::{
    Priority, QueueError, QueueManager, QueueStats, QueueTask, TaskProcessor, TaskResult, TaskType,
};

// 🟢 P1 FIX: Re-export cron types from scheduling module
pub use crate::scheduling::cron::{
    ContextMode, CronError, CronJob, CronPersistence, CronScheduler, JobId, ScheduleType,
};

// Example module (optional, for documentation)
#[cfg(feature = "examples")]
pub mod example;
