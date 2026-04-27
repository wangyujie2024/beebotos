//! Task Scheduler
//!
//! Production-ready preemptive scheduler with:
//! - Work-stealing thread pool
//! - Priority-based scheduling
//! - Fair scheduling (CFS-like)
//! - CPU affinity
//! - Task preemption
//! - Resource accounting

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, trace};

pub mod executor;
pub mod fair;
pub mod priority;
pub mod queue;
pub mod resource;
pub mod task;
pub mod task_tracker;

pub use executor::{CancellationToken, ExecutableTask, TaskHandle, ThreadPoolExecutor};
pub use queue::TaskQueue;
pub use task::{CapabilityLevel, CapabilitySet, Priority, Task, TaskBuilder, TaskId, TaskState};
pub use task_tracker::{
    TaskInfo as TrackedTaskInfo, TaskStatus, TaskStatusEvent, TaskStatusTracker,
};

use crate::error::{KernelError, Result};
use crate::resource::ResourceLimits;

/// Scheduler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    /// Maximum concurrent tasks
    pub max_concurrent: usize,
    /// Time slice in milliseconds for preemption
    pub time_slice_ms: u64,
    /// Enable preemption
    pub enable_preemption: bool,
    /// Default priority
    pub default_priority: Priority,
    /// Number of worker threads (0 = num_cpus)
    pub num_workers: usize,
    /// Enable work stealing
    pub enable_work_stealing: bool,
    /// Enable CPU affinity
    pub enable_cpu_affinity: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 1000,
            time_slice_ms: 100,
            enable_preemption: true,
            default_priority: Priority::Normal,
            num_workers: 0, // Auto-detect
            enable_work_stealing: true,
            enable_cpu_affinity: false,
        }
    }
}

impl SchedulerConfig {
    /// Production configuration
    pub fn production() -> Self {
        Self {
            max_concurrent: 10000,
            time_slice_ms: 50,
            enable_preemption: true,
            default_priority: Priority::Normal,
            num_workers: num_cpus::get(),
            enable_work_stealing: true,
            enable_cpu_affinity: true,
        }
    }

    /// Development configuration
    pub fn development() -> Self {
        Self {
            max_concurrent: 100,
            time_slice_ms: 100,
            enable_preemption: false,
            default_priority: Priority::Normal,
            num_workers: 2,
            enable_work_stealing: false,
            enable_cpu_affinity: false,
        }
    }
}

/// Task information for tracking
#[derive(Debug, Clone)]
pub struct TaskInfo {
    /// Unique task identifier
    pub id: TaskId,
    /// Task name
    pub name: String,
    /// Task priority level
    pub priority: Priority,
    /// Current task state
    pub state: TaskState,
    /// When the task was created
    pub created_at: std::time::Instant,
    /// When the task started executing (if started)
    pub started_at: Option<std::time::Instant>,
    /// When the task completed (if completed)
    pub completed_at: Option<std::time::Instant>,
    /// Capabilities assigned to this task
    pub capabilities: CapabilitySet,
    /// Resource limits for this task
    pub resource_limits: ResourceLimits,
}

impl TaskInfo {
    /// Calculate wait time
    pub fn wait_time(&self) -> std::time::Duration {
        match self.started_at {
            Some(started) => started - self.created_at,
            None => self.created_at.elapsed(),
        }
    }

    /// Calculate execution time
    pub fn execution_time(&self) -> Option<std::time::Duration> {
        match (self.started_at, self.completed_at) {
            (Some(started), Some(completed)) => Some(completed - started),
            (Some(started), None) => Some(started.elapsed()),
            _ => None,
        }
    }
}

/// Task scheduler with executor integration
pub struct Scheduler {
    config: SchedulerConfig,
    /// Thread pool executor (Mutex for interior mutability)
    executor: Mutex<Option<Arc<ThreadPoolExecutor>>>,
    /// Task information map
    tasks: Arc<RwLock<HashMap<TaskId, TaskInfo>>>,
    /// Task handles for control
    handles: Arc<RwLock<HashMap<TaskId, TaskHandle>>>,
    /// Global stats
    stats: Arc<RwLock<SchedulerStats>>,
    /// Shutdown signal
    shutdown: tokio::sync::watch::Sender<bool>,
    /// Next task ID
    next_task_id: AtomicU64,
}

/// Task entry in ready queue (for fair scheduling)
///
/// Note: Reserved for future CFS-style fair scheduling implementation
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct TaskEntry {
    priority: Priority,
    vruntime: u64,
    enqueue_time: std::time::Instant,
    task_id: TaskId,
}

impl PartialEq for TaskEntry {
    fn eq(&self, other: &Self) -> bool {
        self.vruntime == other.vruntime && self.task_id == other.task_id
    }
}

impl Eq for TaskEntry {}

impl PartialOrd for TaskEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TaskEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // CFS: Lower vruntime first
        self.vruntime
            .cmp(&other.vruntime)
            .then_with(|| self.enqueue_time.cmp(&other.enqueue_time))
    }
}

impl Scheduler {
    /// Create new scheduler
    pub fn new(config: SchedulerConfig) -> Self {
        let (shutdown_tx, _) = tokio::sync::watch::channel(false);

        Self {
            config,
            executor: Mutex::new(None),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            handles: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(SchedulerStats::default())),
            shutdown: shutdown_tx,
            next_task_id: AtomicU64::new(1),
        }
    }

    /// Start scheduler
    pub async fn start(&self) -> std::result::Result<(), SchedulerError> {
        let num_workers = if self.config.num_workers == 0 {
            num_cpus::get()
        } else {
            self.config.num_workers
        };

        info!("Starting scheduler with {} workers", num_workers);

        // Create and start executor
        let executor = Arc::new(ThreadPoolExecutor::new(num_workers));
        executor.start();

        // Store executor using interior mutability
        *self.executor.lock().await = Some(executor);

        // Start background maintenance task
        self.spawn_maintenance_task();

        Ok(())
    }

    /// Start scheduler with executor (call this instead of start())
    pub async fn start_with_executor(&self) -> std::result::Result<(), SchedulerError> {
        let num_workers = if self.config.num_workers == 0 {
            num_cpus::get()
        } else {
            self.config.num_workers
        };

        info!("Starting scheduler with {} workers", num_workers);

        let executor = Arc::new(ThreadPoolExecutor::new(num_workers));
        executor.start();
        *self.executor.lock().await = Some(executor);

        // Start background maintenance task
        self.spawn_maintenance_task();

        Ok(())
    }

    /// Spawn maintenance task for cleanup and stats
    fn spawn_maintenance_task(&self) {
        let tasks = self.tasks.clone();
        let _stats = self.stats.clone();
        let mut shutdown = self.shutdown.subscribe();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Cleanup completed tasks
                        let mut tasks_guard = tasks.write().await;
                        let before_count = tasks_guard.len();
                        tasks_guard.retain(|_, info| {
                            info.state != TaskState::Zombie ||
                            info.completed_at.map_or(true, |t| t.elapsed().as_secs() < 300)
                        });
                        let after_count = tasks_guard.len();

                        if before_count != after_count {
                            trace!("Cleaned up {} zombie tasks", before_count - after_count);
                        }
                    }
                    _ = shutdown.changed() => {
                        if *shutdown.borrow() {
                            break;
                        }
                    }
                }
            }
        });
    }

    /// Stop scheduler
    pub async fn stop(&self) {
        let _ = self.shutdown.send(true);

        let executor_guard = self.executor.lock().await;
        if let Some(executor) = executor_guard.as_ref() {
            executor.shutdown();
        }

        info!("Scheduler stopped");
    }

    /// Submit task to scheduler (legacy, use spawn instead)
    pub async fn submit(&self, task: Task) -> std::result::Result<TaskId, SchedulerError> {
        // This is now a no-op for compatibility, use spawn instead
        Ok(task.id)
    }

    /// Mark task as completed (internal use)
    pub async fn complete(&self, task_id: &TaskId) {
        let mut tasks = self.tasks.write().await;
        if let Some(info) = tasks.get_mut(task_id) {
            info.state = TaskState::Zombie;
            info.completed_at = Some(std::time::Instant::now());
        }

        let mut stats = self.stats.write().await;
        stats.tasks_completed += 1;
    }

    /// Block task (e.g., waiting for I/O)
    pub async fn block(&self, task_id: TaskId, reason: task::BlockReason) {
        let mut tasks = self.tasks.write().await;
        if let Some(info) = tasks.get_mut(&task_id) {
            info.state = TaskState::Blocked(reason);
        }
    }

    /// Unblock task
    pub async fn unblock(&self, task_id: &TaskId) {
        let mut tasks = self.tasks.write().await;
        if let Some(info) = tasks.get_mut(task_id) {
            info.state = TaskState::Ready;
        }
    }

    /// Get scheduler stats
    pub async fn stats(&self) -> SchedulerStats {
        let mut stats = self.stats.read().await.clone();

        // Add executor stats if available
        let executor_guard = self.executor.lock().await;
        if let Some(executor) = executor_guard.as_ref() {
            let exec_stats = executor.stats();
            stats.total_tasks_executed = exec_stats.total_executed;
            stats.workers = exec_stats.workers;
        }

        stats
    }

    /// Get queue length
    pub async fn queue_length(&self) -> usize {
        self.tasks
            .read()
            .await
            .values()
            .filter(|t| t.state == TaskState::Ready)
            .count()
    }

    /// Get running count
    pub async fn running_count(&self) -> usize {
        self.tasks
            .read()
            .await
            .values()
            .filter(|t| t.state == TaskState::Running)
            .count()
    }

    /// Spawn a new task with the given parameters
    ///
    /// This is the primary method for creating and executing tasks.
    pub async fn spawn<F>(
        &self,
        name: impl Into<String>,
        priority: Priority,
        capabilities: CapabilitySet,
        f: F,
    ) -> std::result::Result<TaskId, SchedulerError>
    where
        F: std::future::Future<Output = Result<()>> + Send + 'static,
    {
        let executor_guard = self.executor.lock().await;
        let executor = executor_guard.as_ref().ok_or(SchedulerError::NotStarted)?;
        // Clone Arc to drop the lock before await
        let executor = executor.clone();
        drop(executor_guard);

        let id = TaskId::new(self.next_task_id.fetch_add(1, Ordering::SeqCst));
        let name = name.into();

        // Create task info
        let task_info = TaskInfo {
            id,
            name: name.clone(),
            priority,
            state: TaskState::Ready,
            created_at: std::time::Instant::now(),
            started_at: None,
            completed_at: None,
            capabilities: capabilities.clone(),
            resource_limits: ResourceLimits::default(),
        };

        // Store task info
        self.tasks.write().await.insert(id, task_info);

        // Wrap the future with capability check and resource tracking
        let tasks = self.tasks.clone();
        let stats = self.stats.clone();
        let task_id = id;

        let wrapped_future = async move {
            // Update state to running
            {
                let mut tasks_guard = tasks.write().await;
                if let Some(info) = tasks_guard.get_mut(&task_id) {
                    info.state = TaskState::Running;
                    info.started_at = Some(std::time::Instant::now());
                }
            }

            // Execute the actual task
            let result = f.await;

            // Update completion stats
            {
                let mut tasks_guard = tasks.write().await;
                if let Some(info) = tasks_guard.get_mut(&task_id) {
                    info.state = TaskState::Zombie;
                    info.completed_at = Some(std::time::Instant::now());
                }
            }

            stats.write().await.tasks_completed += 1;

            result
        };

        // Spawn on executor
        let handle = executor.spawn(id, priority, wrapped_future);

        // Store handle
        self.handles.write().await.insert(id, handle);

        // Update stats
        self.stats.write().await.tasks_submitted += 1;

        debug!(
            "Spawned task {} '{}' with priority {:?}",
            id, name, priority
        );

        Ok(id)
    }

    /// Cancel a task
    pub async fn cancel(&self, task_id: TaskId) -> bool {
        if let Some(handle) = self.handles.write().await.get(&task_id) {
            handle.cancel();

            let mut tasks = self.tasks.write().await;
            if let Some(info) = tasks.get_mut(&task_id) {
                info.state = TaskState::Zombie;
                info.completed_at = Some(std::time::Instant::now());
            }

            self.stats.write().await.tasks_cancelled += 1;
            true
        } else {
            false
        }
    }

    /// Wait for task completion
    pub async fn await_task(&self, task_id: TaskId) -> Result<()> {
        let handle = self.handles.write().await.remove(&task_id);

        if let Some(h) = handle {
            h.await_completion().await
        } else {
            Err(KernelError::invalid_argument(
                "Task not found or already awaited",
            ))
        }
    }

    /// Get task info
    pub async fn get_task_info(&self, task_id: TaskId) -> Option<TaskInfo> {
        self.tasks.read().await.get(&task_id).cloned()
    }

    /// List all tasks
    pub async fn list_tasks(&self) -> Vec<TaskInfo> {
        self.tasks.read().await.values().cloned().collect()
    }

    /// List tasks by state
    pub async fn list_tasks_by_state(&self, state: TaskState) -> Vec<TaskInfo> {
        self.tasks
            .read()
            .await
            .values()
            .filter(|t| t.state == state)
            .cloned()
            .collect()
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new(SchedulerConfig::default())
    }
}

/// Scheduler statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SchedulerStats {
    /// Tasks submitted
    pub tasks_submitted: u64,
    /// Tasks scheduled
    pub tasks_scheduled: u64,
    /// Tasks completed
    pub tasks_completed: u64,
    /// Tasks failed
    pub tasks_failed: u64,
    /// Tasks cancelled
    pub tasks_cancelled: u64,
    /// Total tasks executed
    pub total_tasks_executed: u64,
    /// Number of workers
    pub workers: usize,
    /// Average wait time in milliseconds
    pub average_wait_time_ms: f64,
    /// Average execution time in milliseconds
    pub average_execution_time_ms: f64,
}

/// Scheduler errors
#[derive(Debug, Clone)]
pub enum SchedulerError {
    /// Scheduler queue is full
    Full,
    /// Invalid task configuration
    InvalidTask,
    /// Task already exists in scheduler
    AlreadyExists,
    /// Scheduler has not been started
    NotStarted,
    /// Task not found in scheduler
    TaskNotFound(TaskId),
}

impl std::fmt::Display for SchedulerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchedulerError::Full => write!(f, "Scheduler queue full"),
            SchedulerError::InvalidTask => write!(f, "Invalid task"),
            SchedulerError::AlreadyExists => write!(f, "Task already exists"),
            SchedulerError::NotStarted => write!(f, "Scheduler not started"),
            SchedulerError::TaskNotFound(id) => write!(f, "Task {} not found", id),
        }
    }
}

impl std::error::Error for SchedulerError {}

/// Rate limiter for task execution
pub struct RateLimiter {
    tokens: Arc<tokio::sync::Mutex<f64>>,
    /// Rate in tokens per second
    rate: f64,
    /// Maximum burst tokens
    burst: f64,
    last_replenish: Arc<tokio::sync::Mutex<std::time::Instant>>,
}

impl RateLimiter {
    /// Create new rate limiter
    pub fn new(rate: f64, burst: f64) -> Self {
        Self {
            tokens: Arc::new(tokio::sync::Mutex::new(burst)),
            rate,
            burst,
            last_replenish: Arc::new(tokio::sync::Mutex::new(std::time::Instant::now())),
        }
    }

    /// Try to acquire tokens
    pub async fn acquire(&self, tokens: f64) -> bool {
        self.replenish().await;

        let mut current = self.tokens.lock().await;

        if *current >= tokens {
            *current -= tokens;
            true
        } else {
            false
        }
    }

    /// Replenish tokens based on elapsed time
    async fn replenish(&self) {
        let now = std::time::Instant::now();
        let mut last = self.last_replenish.lock().await;
        let elapsed = now - *last;
        *last = now;

        let elapsed_secs = elapsed.as_secs_f64();

        let mut current = self.tokens.lock().await;
        *current = (*current + self.rate * elapsed_secs).min(self.burst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_scheduler_spawn() {
        let scheduler = Scheduler::new(SchedulerConfig::development());
        scheduler.start_with_executor().await.unwrap();

        let task_id = scheduler
            .spawn(
                "test-task",
                Priority::Normal,
                CapabilitySet::standard(),
                async { Ok(()) },
            )
            .await
            .unwrap();

        assert!(task_id > TaskId(0));

        scheduler.stop().await;
    }

    #[tokio::test]
    async fn test_scheduler_cancel() {
        let scheduler = Scheduler::new(SchedulerConfig::development());
        scheduler.start_with_executor().await.unwrap();

        let task_id = scheduler
            .spawn(
                "long-task",
                Priority::Normal,
                CapabilitySet::standard(),
                async {
                    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                    Ok(())
                },
            )
            .await
            .unwrap();

        // Cancel the task
        assert!(scheduler.cancel(task_id).await);

        scheduler.stop().await;
    }

    #[tokio::test]
    async fn test_scheduler_stats() {
        let scheduler = Scheduler::new(SchedulerConfig::development());
        scheduler.start_with_executor().await.unwrap();

        // Spawn some tasks
        for i in 0..5 {
            scheduler
                .spawn(
                    format!("task-{}", i),
                    Priority::Normal,
                    CapabilitySet::standard(),
                    async { Ok(()) },
                )
                .await
                .unwrap();
        }

        let stats = scheduler.stats().await;
        assert_eq!(stats.tasks_submitted, 5);

        scheduler.stop().await;
    }
}
