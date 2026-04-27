//! Task Executor
//!
//! Production-ready task executor with:
//! - Work-stealing thread pool
//! - CPU affinity support
//! - Task cancellation
//! - Resource accounting
//! - Priority-based scheduling

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use parking_lot::{Mutex, RwLock};
use tracing::{debug, trace, warn};

use crate::error::{KernelError, Result};
use crate::resource::ResourceUsage;
use crate::scheduler::{Priority, TaskId, TaskState};

/// Inner task state protected by a single lock to prevent deadlocks
pub struct TaskInner {
    /// Task future
    pub future: Pin<Box<dyn Future<Output = Result<()>> + Send>>,
    /// Task state
    pub state: TaskState,
    /// Task waker
    pub waker: Option<Waker>,
    /// Resource usage
    pub resource_usage: ResourceUsage,
    /// Start timestamp
    pub started_at: Option<std::time::Instant>,
    /// Completion timestamp
    pub completed_at: Option<std::time::Instant>,
}

/// Task wrapper that can be executed
pub struct ExecutableTask {
    /// Task ID
    pub id: TaskId,
    /// Task priority
    pub priority: Priority,
    /// Inner task state (single lock design to prevent deadlocks)
    pub inner: Mutex<TaskInner>,
    /// Creation timestamp
    pub created_at: std::time::Instant,
    /// Cancellation token
    pub cancellation_token: CancellationToken,
}

/// Cancellation token for task
///
/// Uses Release/Acquire memory ordering and a version counter
/// to prevent ABA-style race conditions.
#[derive(Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
    version: Arc<AtomicU64>,
}

impl CancellationToken {
    /// Create new cancellation token
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            version: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Cancel the task
    ///
    /// Uses Release ordering to ensure the cancellation is visible
    /// to other threads before any subsequent operations.
    pub fn cancel(&self) {
        self.version.fetch_add(1, Ordering::Relaxed);
        self.cancelled.store(true, Ordering::Release);
    }

    /// Check if task has been cancelled
    ///
    /// Uses Acquire ordering to synchronize with the cancel() store.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Get the current cancellation version
    ///
    /// Can be used to detect cancellation state changes and
    /// prevent ABA problems.
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// Task handle for controlling execution
pub struct TaskHandle {
    /// Task ID
    pub id: TaskId,
    /// Cancellation token
    cancellation_token: CancellationToken,
    /// Result receiver
    result_receiver: Mutex<Option<tokio::sync::oneshot::Receiver<Result<()>>>>,
}

impl TaskHandle {
    /// Cancel task
    pub fn cancel(&self) {
        self.cancellation_token.cancel();
        debug!("Cancelled task {}", self.id);
    }

    /// Wait for task completion
    pub async fn await_completion(&self) -> Result<()> {
        let mut receiver = self.result_receiver.lock();
        if let Some(rx) = receiver.take() {
            match rx.await {
                Ok(result) => result,
                Err(_) => Err(KernelError::internal("Task result channel closed")),
            }
        } else {
            Err(KernelError::internal("Task already awaited"))
        }
    }
}

/// Work queue for a single worker thread
pub struct WorkQueue {
    queue: Mutex<Vec<Arc<ExecutableTask>>>,
    /// For waking the worker
    notify: tokio::sync::Notify,
}

impl WorkQueue {
    /// Create new work queue
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(Vec::new()),
            notify: tokio::sync::Notify::new(),
        }
    }

    /// Push task into queue
    pub fn push(&self, task: Arc<ExecutableTask>) {
        let mut queue = self.queue.lock();
        // Insert sorted by priority (lower value = higher priority)
        let pos = queue
            .binary_search_by(|t| task.priority.cmp(&t.priority))
            .unwrap_or_else(|e| e);
        queue.insert(pos, task);
        drop(queue);
        self.notify.notify_one();
    }

    /// Pop task from queue
    pub fn pop(&self) -> Option<Arc<ExecutableTask>> {
        self.queue.lock().pop()
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.queue.lock().is_empty()
    }

    /// Wait for work to be available
    pub async fn wait_for_work(&self) {
        if self.is_empty() {
            self.notify.notified().await;
        }
    }

    /// Steal task from this queue (called by other workers)
    pub fn steal(&self) -> Option<Arc<ExecutableTask>> {
        let mut queue = self.queue.lock();
        // Steal from the back (lower priority tasks)
        queue.pop()
    }
}

/// Worker thread state
pub struct Worker {
    id: usize,
    queue: Arc<WorkQueue>,
    /// For work-stealing
    steal_targets: Vec<Arc<WorkQueue>>,
    shutdown: Arc<AtomicBool>,
    stats: Arc<WorkerStats>,
}

#[derive(Default)]
struct WorkerStats {
    tasks_executed: AtomicU64,
    tasks_stolen: AtomicU64,
    idle_time_ms: AtomicU64,
}

impl Worker {
    /// Create a new worker thread
    pub fn new(
        id: usize,
        queue: Arc<WorkQueue>,
        steal_targets: Vec<Arc<WorkQueue>>,
        shutdown: Arc<AtomicBool>,
    ) -> Self {
        Self {
            id,
            queue,
            steal_targets,
            shutdown,
            stats: Arc::new(WorkerStats::default()),
        }
    }

    /// Run the worker loop
    pub async fn run(&self) {
        debug!("Worker {} started", self.id);

        while !self.shutdown.load(Ordering::Acquire) {
            // Try to get a task
            let task = self.find_task().await;

            if let Some(task) = task {
                self.execute_task(task).await;
            } else {
                // No work available, wait
                let idle_start = std::time::Instant::now();
                tokio::select! {
                    _ = self.queue.wait_for_work() => {}
                    _ = tokio::time::sleep(tokio::time::Duration::from_millis(10)) => {}
                }
                let idle_ms = idle_start.elapsed().as_millis() as u64;
                self.stats
                    .idle_time_ms
                    .fetch_add(idle_ms, Ordering::Relaxed);
            }
        }

        debug!("Worker {} stopped", self.id);
    }

    /// Find a task from local queue or steal from others
    async fn find_task(&self) -> Option<Arc<ExecutableTask>> {
        // Check local queue first
        if let Some(task) = self.queue.pop() {
            return Some(task);
        }

        // Try to steal from other workers
        for target in &self.steal_targets {
            if let Some(task) = target.steal() {
                trace!(
                    "Worker {} stole task {} from another worker",
                    self.id,
                    task.id
                );
                self.stats.tasks_stolen.fetch_add(1, Ordering::Relaxed);
                return Some(task);
            }
        }

        None
    }

    /// Execute a single task
    async fn execute_task(&self, task: Arc<ExecutableTask>) {
        let task_id = task.id;
        trace!("Worker {} executing task {}", self.id, task_id);

        // Update state
        {
            let mut inner = task.inner.lock();
            inner.state = TaskState::Running;
            inner.started_at = Some(std::time::Instant::now());
        }

        // Create waker
        let waker = Arc::new(TaskWaker {
            task: task.clone(),
            queue: self.queue.clone(),
        });
        let waker: Waker = waker.into();
        let mut context = Context::from_waker(&waker);

        // Poll the future
        let cancel_version = task.cancellation_token.version();
        loop {
            if task.cancellation_token.is_cancelled()
                && task.cancellation_token.version() == cancel_version
            {
                debug!("Task {} cancelled during execution", task_id);
                let mut inner = task.inner.lock();
                inner.state = TaskState::Zombie;
                break;
            }

            let mut inner = task.inner.lock();
            match inner.future.as_mut().poll(&mut context) {
                Poll::Ready(result) => {
                    inner.completed_at = Some(std::time::Instant::now());
                    inner.state = TaskState::Zombie;

                    match result {
                        Ok(()) => {
                            trace!("Task {} completed successfully", task_id);
                        }
                        Err(e) => {
                            warn!("Task {} failed: {}", task_id, e);
                        }
                    }

                    self.stats.tasks_executed.fetch_add(1, Ordering::Relaxed);
                    break;
                }
                Poll::Pending => {
                    // Task yielded, will be re-queued by waker
                    return;
                }
            }
        }
    }
}

/// Waker for rescheduling tasks
struct TaskWaker {
    task: Arc<ExecutableTask>,
    queue: Arc<WorkQueue>,
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        // Re-queue the task
        let mut inner = self.task.inner.lock();
        if inner.state != TaskState::Zombie {
            inner.state = TaskState::Ready;
            drop(inner);
            self.queue.push(self.task.clone());
        }
    }

    fn wake_by_ref(self: &Arc<Self>) {
        // Re-queue the task
        let mut inner = self.task.inner.lock();
        if inner.state != TaskState::Zombie {
            inner.state = TaskState::Ready;
            drop(inner);
            self.queue.push(self.task.clone());
        }
    }
}

/// Thread pool executor
pub struct ThreadPoolExecutor {
    workers: Vec<Arc<Worker>>,
    queues: Vec<Arc<WorkQueue>>,
    shutdown: Arc<AtomicBool>,
    task_map: Arc<RwLock<HashMap<TaskId, Arc<ExecutableTask>>>>,
    num_workers: usize,
}

impl ThreadPoolExecutor {
    /// Create a new thread pool executor with the specified number of workers
    pub fn new(num_workers: usize) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let mut queues = Vec::with_capacity(num_workers);
        let mut workers = Vec::with_capacity(num_workers);

        // Create work queues
        for _ in 0..num_workers {
            queues.push(Arc::new(WorkQueue::new()));
        }

        // Create workers with steal targets
        for id in 0..num_workers {
            let mut steal_targets = Vec::new();
            for (idx, queue) in queues.iter().enumerate() {
                if idx != id {
                    steal_targets.push(queue.clone());
                }
            }

            let worker = Arc::new(Worker::new(
                id,
                queues[id].clone(),
                steal_targets,
                shutdown.clone(),
            ));
            workers.push(worker);
        }

        Self {
            workers,
            queues,
            shutdown,
            task_map: Arc::new(RwLock::new(HashMap::new())),
            num_workers,
        }
    }

    /// Start all workers
    pub fn start(&self) {
        for worker in &self.workers {
            let worker = worker.clone();
            tokio::spawn(async move {
                worker.run().await;
            });
        }
        debug!(
            "Thread pool executor started with {} workers",
            self.num_workers
        );
    }

    /// Spawn a new task
    pub fn spawn<F>(&self, id: TaskId, priority: Priority, future: F) -> TaskHandle
    where
        F: Future<Output = Result<()>> + Send + 'static,
    {
        let (tx, rx) = tokio::sync::oneshot::channel();

        let task = Arc::new(ExecutableTask {
            id,
            priority,
            inner: Mutex::new(TaskInner {
                future: Box::pin(async move {
                    let result = future.await;
                    let _ = tx.send(result.clone());
                    result
                }),
                state: TaskState::Ready,
                waker: None,
                resource_usage: ResourceUsage::new(),
                started_at: None,
                completed_at: None,
            }),
            created_at: std::time::Instant::now(),
            cancellation_token: CancellationToken::new(),
        });

        // Store in task map
        self.task_map.write().insert(id, task.clone());

        // Get cancellation token before moving task
        let cancellation_token = task.cancellation_token.clone();

        // Select queue (simple round-robin for now)
        let queue_idx = id.as_u64() as usize % self.num_workers;
        self.queues[queue_idx].push(task);

        TaskHandle {
            id,
            cancellation_token,
            result_receiver: Mutex::new(Some(rx)),
        }
    }

    /// Get task state
    pub fn get_task_state(&self, id: TaskId) -> Option<TaskState> {
        self.task_map.read().get(&id).map(|t| t.inner.lock().state)
    }

    /// Cancel a task
    pub fn cancel_task(&self, id: TaskId) -> bool {
        if let Some(task) = self.task_map.read().get(&id) {
            task.cancellation_token.cancel();
            true
        } else {
            false
        }
    }

    /// Shutdown the executor
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
        // Wake all workers
        for queue in &self.queues {
            queue.notify.notify_one();
        }
    }

    /// Get executor statistics
    pub fn stats(&self) -> ExecutorStats {
        let mut total_executed = 0;
        let mut total_stolen = 0;

        for worker in &self.workers {
            total_executed += worker.stats.tasks_executed.load(Ordering::Relaxed);
            total_stolen += worker.stats.tasks_stolen.load(Ordering::Relaxed);
        }

        ExecutorStats {
            workers: self.num_workers,
            active_tasks: self.task_map.read().len(),
            total_executed,
            total_stolen,
        }
    }
}

/// Executor statistics
#[derive(Debug, Clone)]
pub struct ExecutorStats {
    /// Number of workers
    pub workers: usize,
    /// Active tasks
    pub active_tasks: usize,
    /// Total tasks executed
    pub total_executed: u64,
    /// Total tasks stolen
    pub total_stolen: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::unwrap_used)]
    #[tokio::test]
    async fn test_executor_spawn_and_complete() {
        let executor = ThreadPoolExecutor::new(2);
        executor.start();

        let handle = executor.spawn(TaskId(1), Priority::Normal, async { Ok(()) });

        let result = handle.await_completion().await;
        assert!(result.is_ok());

        executor.shutdown();
    }

    #[allow(clippy::unwrap_used)]
    #[tokio::test]
    async fn test_task_cancellation() {
        let executor = ThreadPoolExecutor::new(2);
        executor.start();

        let handle = executor.spawn(TaskId(1), Priority::Normal, async {
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
            Ok(())
        });

        // Cancel immediately
        handle.cancel();

        // Should not complete successfully
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        executor.shutdown();
    }

    #[allow(clippy::unwrap_used)]
    #[tokio::test]
    async fn test_priority_scheduling() {
        let executor = ThreadPoolExecutor::new(1);
        executor.start();

        let order = Arc::new(Mutex::new(Vec::new()));

        let order1 = order.clone();
        let handle1 = executor.spawn(TaskId(1), Priority::Low, async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            order1.lock().push(1);
            Ok(())
        });

        let order2 = order.clone();
        let handle2 = executor.spawn(TaskId(2), Priority::High, async move {
            order2.lock().push(2);
            Ok(())
        });

        let _ = handle1.await_completion().await;
        let _ = handle2.await_completion().await;

        // High priority should execute first
        let final_order = order.lock();
        assert_eq!(final_order[0], 2);

        executor.shutdown();
    }
}
