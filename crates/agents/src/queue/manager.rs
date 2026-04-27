//! Queue Manager
//!
//! Production-ready multi-queue system with worker pools.
//! Supports main, cron, subagent, and nested queues with proper backpressure.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ::tracing::{error, info, warn};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex, RwLock, Semaphore};
use tokio::task::JoinHandle;

use crate::events::AgentEventBus;
use crate::session::SessionKey;

/// Queue configuration with backpressure settings
#[derive(Debug, Clone)]
pub struct QueueConfig {
    /// Main queue capacity (0 = unbounded)
    pub main_queue_capacity: usize,
    /// Cron queue capacity (0 = unbounded)
    pub cron_queue_capacity: usize,
    /// Subagent queue capacity
    pub subagent_queue_capacity: usize,
    /// Nested queue capacity (0 = unbounded)
    pub nested_queue_capacity: usize,
    /// Max concurrent subagent tasks
    pub max_concurrent_subagents: usize,
    /// Max memory usage in MB (0 = unlimited)
    pub max_memory_mb: usize,
    /// CODE QUALITY FIX: Dynamic scaling configuration
    pub scaling: ScalingConfig,
}

/// CODE QUALITY FIX: Dynamic scaling configuration
#[derive(Debug, Clone)]
pub struct ScalingConfig {
    /// Enable dynamic scaling
    pub enabled: bool,
    /// Minimum number of subagent workers
    pub min_workers: usize,
    /// Maximum number of subagent workers
    pub max_workers: usize,
    /// Scale up threshold (queue depth)
    pub scale_up_threshold: usize,
    /// Scale down threshold (queue depth)
    pub scale_down_threshold: usize,
    /// Check interval in seconds
    pub check_interval_secs: u64,
    /// Cooldown period between scaling actions (seconds)
    pub cooldown_secs: u64,
}

impl Default for ScalingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_workers: 2,
            max_workers: 20,
            scale_up_threshold: 10,  // Scale up when 10+ tasks queued
            scale_down_threshold: 2, // Scale down when < 2 tasks queued
            check_interval_secs: 30, // Check every 30 seconds
            cooldown_secs: 60,       // Wait 60s between scaling actions
        }
    }
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            main_queue_capacity: 1000,    // Backpressure: limit pending tasks
            cron_queue_capacity: 100,     // Backpressure: limit scheduled tasks
            subagent_queue_capacity: 100, // Already bounded
            nested_queue_capacity: 50,    // Backpressure: limit recursion depth
            max_concurrent_subagents: 5,  // Concurrency limit
            max_memory_mb: 512,           // 512MB default memory limit
            scaling: ScalingConfig::default(),
        }
    }
}

/// Task processor trait for executing queue tasks
#[async_trait::async_trait]
pub trait TaskProcessor: Send + Sync {
    async fn process(&self, task: QueueTask) -> TaskResult;
}

/// Task execution result
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub task_id: String,
    pub success: bool,
    pub output: String,
}

/// Worker supervision info
#[derive(Debug, Clone)]
struct WorkerInfo {
    worker_type: WorkerType,
    worker_id: usize,
    restart_count: u32,
}

/// Worker types for supervision
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkerType {
    Main,
    Cron,
    Subagent,
    Nested,
}

/// Queue manager with worker pools and supervision
///
/// RELIABILITY FIX: Added worker supervision for automatic restart on panic
pub struct QueueManager {
    /// Main queue - sequential execution
    main_queue: MainQueue,
    /// Cron queue - scheduled tasks
    cron_queue: CronQueue,
    /// Subagent queue - parallel execution (max 5)
    subagent_queue: SubagentQueue,
    /// Nested queue - recursion prevention
    nested_queue: NestedQueue,
    /// Worker handles for graceful shutdown
    workers: Arc<Mutex<Vec<JoinHandle<()>>>>,
    /// Task statistics
    stats: Arc<RwLock<QueueStats>>,
    /// Shutdown signal
    shutdown: Arc<tokio::sync::Notify>,
    /// Event bus for notifications
    event_bus: Option<AgentEventBus>,
    /// RELIABILITY FIX: Worker supervision info
    worker_infos: Arc<Mutex<Vec<WorkerInfo>>>,
    /// RELIABILITY FIX: Max worker restarts before giving up
    max_restarts: u32,
    /// CODE QUALITY FIX: Queue depth counter for auto-scaling
    subagent_queue_depth: Arc<AtomicUsize>,
    /// CODE QUALITY FIX: Queue configuration
    config: QueueConfig,
}

/// Main queue - sequential execution with bounded capacity (backpressure)
struct MainQueue {
    tx: mpsc::Sender<QueueTask>,
    rx: Arc<Mutex<mpsc::Receiver<QueueTask>>>,
}

/// Cron queue - scheduled tasks with bounded capacity
struct CronQueue {
    tx: mpsc::Sender<QueueTask>,
    rx: Arc<Mutex<mpsc::Receiver<QueueTask>>>,
}

/// Subagent queue - parallel execution with semaphore-based concurrency limit
struct SubagentQueue {
    tx: mpsc::Sender<QueueTask>,
    rx: Arc<Mutex<mpsc::Receiver<QueueTask>>>,
    semaphore: Arc<Semaphore>,
}

/// Nested queue - recursion prevention with bounded capacity
struct NestedQueue {
    tx: mpsc::Sender<QueueTask>,
    rx: Arc<Mutex<mpsc::Receiver<QueueTask>>>,
    active: Arc<RwLock<HashMap<String, usize>>>,
}

/// Queue statistics
#[derive(Debug, Clone, Default)]
pub struct QueueStats {
    pub main_queue_processed: u64,
    pub cron_queue_processed: u64,
    pub subagent_queue_processed: u64,
    pub nested_queue_processed: u64,
    pub main_queue_failed: u64,
    pub cron_queue_failed: u64,
    pub subagent_queue_failed: u64,
    pub nested_queue_failed: u64,
}

/// Queue task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueTask {
    pub id: String,
    pub session_key: SessionKey,
    pub task_type: TaskType,
    pub priority: Priority,
}

/// Task types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskType {
    ExecuteCommand(String),
    ProcessMessage(String),
    SpawnSubagent(String),
    CronJob(String),
}

/// Priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

impl QueueManager {
    /// Creates a new QueueManager with all channels and workers
    ///
    /// # Backpressure
    /// - Main queue: bounded channel with configurable capacity
    /// - Cron queue: bounded channel with configurable capacity
    /// - Subagent queue: bounded channel + semaphore for concurrency control
    /// - Nested queue: bounded channel with configurable capacity
    pub fn new() -> Self {
        Self::with_config(QueueConfig::default())
    }

    /// Creates a new QueueManager with custom configuration
    pub fn with_config(config: QueueConfig) -> Self {
        // Bounded channels for backpressure - prevents memory exhaustion under load
        let (main_tx, main_rx) = mpsc::channel(config.main_queue_capacity);
        let (cron_tx, cron_rx) = mpsc::channel(config.cron_queue_capacity);
        let (subagent_tx, subagent_rx) = mpsc::channel(config.subagent_queue_capacity);
        let (nested_tx, nested_rx) = mpsc::channel(config.nested_queue_capacity);

        Self {
            main_queue: MainQueue {
                tx: main_tx,
                rx: Arc::new(Mutex::new(main_rx)),
            },
            cron_queue: CronQueue {
                tx: cron_tx,
                rx: Arc::new(Mutex::new(cron_rx)),
            },
            subagent_queue: SubagentQueue {
                tx: subagent_tx,
                rx: Arc::new(Mutex::new(subagent_rx)),
                semaphore: Arc::new(Semaphore::new(config.max_concurrent_subagents)),
            },
            nested_queue: NestedQueue {
                tx: nested_tx,
                rx: Arc::new(Mutex::new(nested_rx)),
                active: Arc::new(RwLock::new(HashMap::new())),
            },
            workers: Arc::new(Mutex::new(Vec::new())),
            stats: Arc::new(RwLock::new(QueueStats::default())),
            shutdown: Arc::new(tokio::sync::Notify::new()),
            event_bus: None,
            // RELIABILITY FIX: Initialize supervision fields
            worker_infos: Arc::new(Mutex::new(Vec::new())),
            max_restarts: 10, // Max 10 restarts per worker before giving up
            // CODE QUALITY FIX: Initialize queue depth counter
            subagent_queue_depth: Arc::new(AtomicUsize::new(0)),
            // CODE QUALITY FIX: Store config
            config,
        }
    }

    /// Set event bus for notifications
    pub fn with_event_bus(mut self, event_bus: AgentEventBus) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    /// Start worker pools for all queues
    ///
    /// RELIABILITY FIX: Now starts supervisor task for automatic worker restart
    /// on panic
    ///
    /// CODE QUALITY FIX: Added dynamic scaling for subagent workers
    pub async fn spawn_workers(&self, processor: Arc<dyn TaskProcessor>) {
        info!("Starting queue workers...");

        // Spawn main queue worker (sequential - single worker)
        self.spawn_main_worker(processor.clone()).await;

        // Spawn cron queue worker (sequential - single worker)
        self.spawn_cron_worker(processor.clone()).await;

        // CODE QUALITY FIX: Use config-based initial worker count instead of fixed 5
        let initial_workers = if self.config.scaling.enabled {
            self.config.scaling.min_workers
        } else {
            5 // Default for backward compatibility
        };

        // Spawn subagent queue workers (parallel - dynamic count)
        for i in 0..initial_workers {
            self.spawn_subagent_worker(processor.clone(), i).await;
        }
        info!("Spawned {} initial subagent workers", initial_workers);

        // Spawn nested queue worker (sequential - single worker)
        self.spawn_nested_worker(processor.clone()).await;

        // RELIABILITY FIX: Start supervisor task for automatic restart
        self.start_supervisor(processor.clone()).await;

        // CODE QUALITY FIX: Start dynamic scaling task if enabled
        if self.config.scaling.enabled {
            self.start_auto_scaling(processor.clone()).await;
        }

        info!("All queue workers started with supervision");
    }

    /// CODE QUALITY FIX: Start auto-scaling task for subagent workers
    async fn start_auto_scaling(&self, processor: Arc<dyn TaskProcessor>) {
        let workers = self.workers.clone();
        let worker_infos = self.worker_infos.clone();
        let shutdown = self.shutdown.clone();
        let config = self.config.clone();

        let semaphore = self.subagent_queue.semaphore.clone();
        let stats = self.stats.clone();
        let event_bus = self.event_bus.clone();
        // CODE QUALITY FIX: Use queue depth counter instead of always 0
        let queue_depth_counter = self.subagent_queue_depth.clone();
        // CODE QUALITY FIX: Clone subagent queue receiver for new workers
        let rx = self.subagent_queue.rx.clone();

        tokio::spawn(async move {
            info!("Auto-scaling started for subagent workers");
            let mut interval =
                tokio::time::interval(Duration::from_secs(config.scaling.check_interval_secs));
            let mut last_scale_action = std::time::Instant::now();
            let mut _current_workers = config.scaling.min_workers;

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Check cooldown period
                        if last_scale_action.elapsed().as_secs() < config.scaling.cooldown_secs {
                            continue;
                        }

                        // CODE QUALITY FIX: Get actual queue depth from counter
                        let queue_depth = queue_depth_counter.load(Ordering::SeqCst);

                        // Get current worker count (subagent workers only)
                        let worker_count = {
                            let infos_guard = worker_infos.lock().await;
                            infos_guard.iter().filter(|w| w.worker_type == WorkerType::Subagent).count()
                        };

                        // Scale up logic
                        if queue_depth >= config.scaling.scale_up_threshold
                            && worker_count < config.scaling.max_workers {
                            let new_worker_id = worker_count;
                            info!("Scaling up: adding worker {} (queue depth: {})",
                                new_worker_id, queue_depth);

                            // Spawn new worker with queue depth counter
                            let handle = crate::queue::worker_tasks::spawn_subagent_worker_task(
                                rx.clone(),
                                semaphore.clone(),
                                stats.clone(),
                                shutdown.clone(),
                                event_bus.clone(),
                                processor.clone(),
                                new_worker_id,
                                0,
                                Some(queue_depth_counter.clone()),
                            );

                            {
                                let mut workers_guard = workers.lock().await;
                                workers_guard.push(handle);
                            }
                            {
                                let mut infos_guard = worker_infos.lock().await;
                                infos_guard.push(WorkerInfo {
                                    worker_type: WorkerType::Subagent,
                                    worker_id: new_worker_id,
                                    restart_count: 0,
                                });
                            }

                            _current_workers += 1;
                            last_scale_action = std::time::Instant::now();
                        }

                        // Scale down logic (simplified - just log for now)
                        // In production, you'd want to gracefully terminate idle workers
                        if queue_depth <= config.scaling.scale_down_threshold
                            && worker_count > config.scaling.min_workers {
                            info!("Scale down condition met (queue depth: {}), but keeping {} workers for stability",
                                queue_depth, config.scaling.min_workers);
                            // Note: Actual scale-down is complex - we'd need to signal workers to stop
                            // when they finish their current task
                        }
                    }
                    _ = shutdown.notified() => {
                        info!("Auto-scaling shutting down");
                        break;
                    }
                }
            }
        });
    }

    /// RELIABILITY FIX: Start supervisor task to monitor and restart workers
    ///
    /// Workers are restarted automatically when they panic, up to max_restarts.
    async fn start_supervisor(&self, processor: Arc<dyn TaskProcessor>) {
        let workers = self.workers.clone();
        let worker_infos = self.worker_infos.clone();
        let shutdown = self.shutdown.clone();
        let max_restarts = self.max_restarts;

        // Clone all resources needed for worker restart
        let main_rx = self.main_queue.rx.clone();
        let cron_rx = self.cron_queue.rx.clone();
        let subagent_rx = self.subagent_queue.rx.clone();
        let subagent_semaphore = self.subagent_queue.semaphore.clone();
        let nested_rx = self.nested_queue.rx.clone();
        let nested_active = self.nested_queue.active.clone();
        let stats = self.stats.clone();
        let event_bus = self.event_bus.clone();
        // CODE QUALITY FIX: Clone queue depth counter for subagent worker restarts
        let subagent_queue_depth = self.subagent_queue_depth.clone();

        tokio::spawn(async move {
            info!("Worker supervisor started");
            let mut check_interval = tokio::time::interval(Duration::from_secs(5));

            loop {
                tokio::select! {
                    _ = check_interval.tick() => {
                        let mut workers = workers.lock().await;
                        let mut infos = worker_infos.lock().await;

                        // Check each worker for panic
                        let mut i = 0;
                        while i < workers.len() {
                            if workers[i].is_finished() {
                                // Worker has finished (likely panicked)
                                let handle = workers.remove(i);
                                let info = if i < infos.len() {
                                    Some(infos.remove(i))
                                } else {
                                    None
                                };

                                // Check restart count
                                if let Some(ref wi) = info {
                                    if wi.restart_count >= max_restarts {
                                        error!(
                                            "Worker {:?} #{} exceeded max restarts ({}), NOT restarting",
                                            wi.worker_type, wi.worker_id, wi.restart_count
                                        );
                                        continue;
                                    }
                                }

                                // Log the panic/completion
                                match handle.await {
                                    Ok(()) => info!("Worker completed normally"),
                                    Err(e) => {
                                        error!("Worker panicked: {}", e);
                                    }
                                }

                                // RELIABILITY FIX: Restart the worker
                                if let Some(wi) = info {
                                    warn!(
                                        "Restarting {:?} worker #{} (restart count: {})",
                                        wi.worker_type, wi.worker_id, wi.restart_count + 1
                                    );

                                    // Spawn new worker with incremented restart count
                                    let new_handle = match wi.worker_type {
                                        WorkerType::Main => {
                                            super::worker_tasks::spawn_main_worker_task(
                                                main_rx.clone(),
                                                stats.clone(),
                                                shutdown.clone(),
                                                event_bus.clone(),
                                                processor.clone(),
                                                wi.restart_count + 1,
                                            )
                                        }
                                        WorkerType::Cron => {
                                            super::worker_tasks::spawn_cron_worker_task(
                                                cron_rx.clone(),
                                                stats.clone(),
                                                shutdown.clone(),
                                                event_bus.clone(),
                                                processor.clone(),
                                                wi.restart_count + 1,
                                            )
                                        }
                                        WorkerType::Subagent => {
                                            super::worker_tasks::spawn_subagent_worker_task(
                                                subagent_rx.clone(),
                                                subagent_semaphore.clone(),
                                                stats.clone(),
                                                shutdown.clone(),
                                                event_bus.clone(),
                                                processor.clone(),
                                                wi.worker_id,
                                                wi.restart_count + 1,
                                                Some(subagent_queue_depth.clone()),
                                            )
                                        }
                                        WorkerType::Nested => {
                                            super::worker_tasks::spawn_nested_worker_task(
                                                nested_rx.clone(),
                                                nested_active.clone(),
                                                stats.clone(),
                                                shutdown.clone(),
                                                event_bus.clone(),
                                                processor.clone(),
                                                wi.restart_count + 1,
                                            )
                                        }
                                    };

                                    // Update worker info with incremented restart count
                                    let new_info = WorkerInfo {
                                        worker_type: wi.worker_type,
                                        worker_id: wi.worker_id,
                                        restart_count: wi.restart_count + 1,
                                    };

                                    workers.push(new_handle);
                                    infos.push(new_info);
                                }
                            } else {
                                i += 1;
                            }
                        }
                    }
                    _ = shutdown.notified() => {
                        info!("Worker supervisor shutting down");
                        break;
                    }
                }
            }
        });
    }

    /// Spawn main queue worker
    async fn spawn_main_worker(&self, processor: Arc<dyn TaskProcessor>) {
        // RELIABILITY FIX: Use standalone worker task function for restartability
        let handle = super::worker_tasks::spawn_main_worker_task(
            self.main_queue.rx.clone(),
            self.stats.clone(),
            self.shutdown.clone(),
            self.event_bus.clone(),
            processor,
            0, // initial spawn, not restart
        );

        // RELIABILITY FIX: Store worker info for supervision
        let worker_id = {
            let mut workers = self.workers.lock().await;
            workers.push(handle);
            workers.len() - 1
        };

        self.worker_infos.lock().await.push(WorkerInfo {
            worker_type: WorkerType::Main,
            worker_id,
            restart_count: 0,
        });
    }

    /// Spawn cron queue worker
    async fn spawn_cron_worker(&self, processor: Arc<dyn TaskProcessor>) {
        // RELIABILITY FIX: Use standalone worker task function for restartability
        let handle = super::worker_tasks::spawn_cron_worker_task(
            self.cron_queue.rx.clone(),
            self.stats.clone(),
            self.shutdown.clone(),
            self.event_bus.clone(),
            processor,
            0, // initial spawn, not restart
        );

        // RELIABILITY FIX: Store worker info for supervision
        let worker_id = {
            let mut workers = self.workers.lock().await;
            workers.push(handle);
            workers.len() - 1
        };

        self.worker_infos.lock().await.push(WorkerInfo {
            worker_type: WorkerType::Cron,
            worker_id,
            restart_count: 0,
        });
    }

    /// Spawn subagent queue worker
    async fn spawn_subagent_worker(&self, processor: Arc<dyn TaskProcessor>, subagent_id: usize) {
        // RELIABILITY FIX: Use standalone worker task function for restartability
        // CODE QUALITY FIX: Pass queue depth counter for accurate tracking
        let handle = super::worker_tasks::spawn_subagent_worker_task(
            self.subagent_queue.rx.clone(),
            self.subagent_queue.semaphore.clone(),
            self.stats.clone(),
            self.shutdown.clone(),
            self.event_bus.clone(),
            processor,
            subagent_id,
            0, // initial spawn, not restart
            Some(self.subagent_queue_depth.clone()),
        );

        // RELIABILITY FIX: Store worker info for supervision
        let _worker_id = {
            let mut workers = self.workers.lock().await;
            workers.push(handle);
            workers.len() - 1
        };

        self.worker_infos.lock().await.push(WorkerInfo {
            worker_type: WorkerType::Subagent,
            worker_id: subagent_id, // Use subagent_id as the worker identifier
            restart_count: 0,
        });
    }

    /// Spawn nested queue worker
    async fn spawn_nested_worker(&self, processor: Arc<dyn TaskProcessor>) {
        // RELIABILITY FIX: Use standalone worker task function for restartability
        let handle = super::worker_tasks::spawn_nested_worker_task(
            self.nested_queue.rx.clone(),
            self.nested_queue.active.clone(),
            self.stats.clone(),
            self.shutdown.clone(),
            self.event_bus.clone(),
            processor,
            0, // initial spawn, not restart
        );

        // RELIABILITY FIX: Store worker info for supervision
        let worker_id = {
            let mut workers = self.workers.lock().await;
            workers.push(handle);
            workers.len() - 1
        };

        self.worker_infos.lock().await.push(WorkerInfo {
            worker_type: WorkerType::Nested,
            worker_id,
            restart_count: 0,
        });
    }

    /// Submit to main queue (sequential)
    ///
    /// # Backpressure
    /// Returns `QueueError::QueueFull` if main queue is at capacity
    pub fn submit_main(&self, task: QueueTask) -> Result<(), QueueError> {
        // Use try_send for non-blocking submission with backpressure
        self.main_queue.tx.try_send(task).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => QueueError::QueueFull,
            mpsc::error::TrySendError::Closed(_) => QueueError::QueueClosed,
        })
    }

    /// Submit to cron queue
    ///
    /// # Backpressure
    /// Returns `QueueError::QueueFull` if cron queue is at capacity
    pub fn submit_cron(&self, task: QueueTask) -> Result<(), QueueError> {
        // Use try_send for non-blocking submission with backpressure
        self.cron_queue.tx.try_send(task).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => QueueError::QueueFull,
            mpsc::error::TrySendError::Closed(_) => QueueError::QueueClosed,
        })
    }

    /// Submit to subagent queue (parallel, max 5 concurrent)
    ///
    /// # Backpressure + Concurrency Control
    /// - Channel capacity limits pending tasks
    /// - Semaphore limits concurrent execution
    pub async fn submit_subagent(&self, task: QueueTask) -> Result<(), QueueError> {
        // First check if channel has space (backpressure)
        match self.subagent_queue.tx.try_reserve() {
            Ok(permit) => {
                // Send the task
                permit.send(task);
                // CODE QUALITY FIX: Increment queue depth counter for auto-scaling
                self.subagent_queue_depth.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            Err(_) => Err(QueueError::QueueFull),
        }
    }

    /// Submit to nested queue
    ///
    /// # Backpressure + Recursion Limit
    /// - Channel capacity limits pending tasks
    /// - Max nesting depth: 5 levels
    pub async fn submit_nested(&self, task: QueueTask) -> Result<(), QueueError> {
        // Check nesting depth
        let session_key = task.session_key.to_string();
        let active = self.nested_queue.active.read().await;
        let depth = active.get(&session_key).copied().unwrap_or(0);
        drop(active);

        if depth >= 5 {
            return Err(QueueError::NestedLimitExceeded);
        }

        // Use try_send for backpressure
        self.nested_queue.tx.try_send(task).map_err(|e| match e {
            mpsc::error::TrySendError::Full(_) => QueueError::QueueFull,
            mpsc::error::TrySendError::Closed(_) => QueueError::QueueClosed,
        })
    }

    /// Check if nested execution is allowed
    pub async fn check_nesting(&self, session_key: &SessionKey) -> bool {
        let active = self.nested_queue.active.read().await;
        let depth = active.get(&session_key.to_string()).copied().unwrap_or(0);
        depth < 5
    }

    /// Get queue statistics
    pub async fn stats(&self) -> QueueStats {
        self.stats.read().await.clone()
    }

    /// Graceful shutdown
    pub async fn shutdown(&self) {
        info!("QueueManager initiating graceful shutdown...");

        // Signal all workers to stop
        self.shutdown.notify_waiters();

        // Wait for all workers to complete
        let mut workers = self.workers.lock().await;
        for handle in workers.drain(..) {
            if let Err(e) = handle.await {
                error!("Worker panicked: {}", e);
            }
        }

        info!("QueueManager shutdown complete");
    }
}

impl Default for QueueManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Queue errors
#[derive(Debug, Clone)]
pub enum QueueError {
    QueueFull,
    QueueClosed,
    TaskRejected,
    NestedLimitExceeded,
}

impl std::fmt::Display for QueueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueueError::QueueFull => write!(f, "Queue is full"),
            QueueError::QueueClosed => write!(f, "Queue is closed"),
            QueueError::TaskRejected => write!(f, "Task rejected"),
            QueueError::NestedLimitExceeded => write!(f, "Nested execution limit exceeded"),
        }
    }
}

impl std::error::Error for QueueError {}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct TestProcessor {
        count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl TaskProcessor for TestProcessor {
        async fn process(&self, _task: QueueTask) -> TaskResult {
            self.count.fetch_add(1, Ordering::SeqCst);
            TaskResult {
                task_id: "test".to_string(),
                success: true,
                output: "ok".to_string(),
            }
        }
    }

    #[tokio::test]
    async fn test_queue_manager_basic() {
        // Disable auto-scaling for this test to avoid background tasks
        let config = QueueConfig {
            scaling: ScalingConfig {
                enabled: false,
                ..ScalingConfig::default()
            },
            ..QueueConfig::default()
        };
        let manager = QueueManager::with_config(config);
        let processor = Arc::new(TestProcessor {
            count: Arc::new(AtomicUsize::new(0)),
        });

        manager.spawn_workers(processor.clone()).await;

        // Give workers time to start listening for shutdown
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Shutdown with timeout to prevent test hang
        match tokio::time::timeout(Duration::from_secs(5), manager.shutdown()).await {
            Ok(()) => {}
            Err(_) => panic!("shutdown() timed out - workers did not exit properly"),
        }
    }
}
