//! Task Queue Implementations

use std::collections::{BTreeMap, VecDeque};

use tokio::sync::Mutex;

use super::{Priority, Task, TaskId};

/// Scheduling algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
pub enum SchedulingAlgorithm {
    /// First Come First Served
    Fcfs,
    /// Round Robin
    RoundRobin,
    /// Priority-based
    Priority,
    /// Completely Fair Scheduler (Linux-style)
    Cfs,
    /// Earliest Deadline First
    Edf,
}

/// Task queue trait
#[async_trait::async_trait]
pub trait TaskQueueTrait: Send + Sync {
    /// Add task to queue
    async fn enqueue(&self, task: Task);
    /// Remove and return highest priority task
    async fn dequeue(&self) -> Option<Task>;
    /// Peek at next task without removing
    async fn peek(&self) -> Option<Task>;
    /// Remove specific task
    async fn remove(&self, task_id: TaskId) -> Option<Task>;
    /// Get specific task
    async fn get(&self, task_id: TaskId) -> Option<Task>;
    /// Queue length
    async fn len(&self) -> usize;
    /// Check if empty
    async fn is_empty(&self) -> bool;
}

/// Multi-level feedback queue
pub struct TaskQueue {
    /// Scheduling algorithm
    algorithm: SchedulingAlgorithm,
    /// Priority queues (0 = highest priority)
    queues: Vec<Mutex<VecDeque<Task>>>,
    /// CFS red-black tree simulation (using BTreeMap)
    cfs_queue: Mutex<BTreeMap<u64, VecDeque<Task>>>,
    /// EDF deadline queue
    edf_queue: Mutex<BTreeMap<u64, VecDeque<Task>>>,
    /// Current time slice counter for CFS
    vtime: Mutex<u64>,
}

impl TaskQueue {
    /// Create new task queue
    pub fn new(algorithm: SchedulingAlgorithm) -> Self {
        let queues = (0..=10).map(|_| Mutex::new(VecDeque::new())).collect();

        Self {
            algorithm,
            queues,
            cfs_queue: Mutex::new(BTreeMap::new()),
            edf_queue: Mutex::new(BTreeMap::new()),
            vtime: Mutex::new(0),
        }
    }

    /// Enqueue task
    pub async fn enqueue(&self, task: Task) {
        match self.algorithm {
            SchedulingAlgorithm::Fcfs | SchedulingAlgorithm::RoundRobin => {
                // Single queue
                self.queues[5].lock().await.push_back(task);
            }
            SchedulingAlgorithm::Priority => {
                // Map priority (0-4) to queue index (0-4)
                // 0 = RealTime (highest), 4 = Idle (lowest)
                let prio = task.priority.level();
                let queue_idx = prio.clamp(0, 4) as usize;
                self.queues[queue_idx].lock().await.push_back(task);
            }
            SchedulingAlgorithm::Cfs => {
                // Calculate virtual runtime
                let weight = self.prio_to_weight(task.priority);
                let vtime = *self.vtime.lock().await;
                let key = vtime * 1024 / weight;

                let mut queue = self.cfs_queue.lock().await;
                queue.entry(key).or_default().push_back(task);
            }
            SchedulingAlgorithm::Edf => {
                if let Some(deadline) = task.deadline {
                    let key = deadline
                        .duration_since(std::time::Instant::now())
                        .as_nanos() as u64;
                    let mut queue = self.edf_queue.lock().await;
                    queue.entry(key).or_default().push_back(task);
                } else {
                    // No deadline - put at end
                    let mut queue = self.edf_queue.lock().await;
                    let max_key = queue.keys().next_back().copied().unwrap_or(0);
                    queue.entry(max_key + 1).or_default().push_back(task);
                }
            }
        }
    }

    /// Dequeue task
    pub async fn dequeue(&self) -> Option<Task> {
        match self.algorithm {
            SchedulingAlgorithm::Fcfs | SchedulingAlgorithm::RoundRobin => {
                self.queues[5].lock().await.pop_front()
            }
            SchedulingAlgorithm::Priority => {
                // Find first non-empty queue
                for queue in &self.queues {
                    if let Some(task) = queue.lock().await.pop_front() {
                        return Some(task);
                    }
                }
                None
            }
            SchedulingAlgorithm::Cfs => {
                let mut queue = self.cfs_queue.lock().await;
                if let Some((key, tasks)) = queue.iter_mut().next() {
                    let key = *key;
                    if let Some(task) = tasks.pop_front() {
                        if tasks.is_empty() {
                            queue.remove(&key);
                        }
                        // Update vtime
                        drop(queue);
                        let weight = self.prio_to_weight(task.priority);
                        *self.vtime.lock().await += 1000000 / weight;
                        return Some(task);
                    }
                }
                None
            }
            SchedulingAlgorithm::Edf => {
                let mut queue = self.edf_queue.lock().await;
                if let Some((key, tasks)) = queue.iter_mut().next() {
                    let key = *key;
                    if let Some(task) = tasks.pop_front() {
                        if tasks.is_empty() {
                            queue.remove(&key);
                        }
                        return Some(task);
                    }
                }
                None
            }
        }
    }

    /// Peek at next task
    pub async fn peek(&self) -> Option<Task> {
        match self.algorithm {
            SchedulingAlgorithm::Fcfs | SchedulingAlgorithm::RoundRobin => {
                self.queues[5].lock().await.front().cloned()
            }
            SchedulingAlgorithm::Priority => {
                for queue in &self.queues {
                    if let Some(task) = queue.lock().await.front() {
                        return Some(task.clone());
                    }
                }
                None
            }
            SchedulingAlgorithm::Cfs => {
                let queue = self.cfs_queue.lock().await;
                queue.values().next().and_then(|q| q.front().cloned())
            }
            SchedulingAlgorithm::Edf => {
                let queue = self.edf_queue.lock().await;
                queue.values().next().and_then(|q| q.front().cloned())
            }
        }
    }

    /// Remove specific task
    pub async fn remove(&self, task_id: TaskId) -> Option<Task> {
        match self.algorithm {
            SchedulingAlgorithm::Fcfs | SchedulingAlgorithm::RoundRobin => {
                let mut queue = self.queues[5].lock().await;
                if let Some(pos) = queue.iter().position(|t| t.id == task_id) {
                    return queue.remove(pos);
                }
                None
            }
            SchedulingAlgorithm::Priority => {
                for queue in &self.queues {
                    let mut queue = queue.lock().await;
                    if let Some(pos) = queue.iter().position(|t| t.id == task_id) {
                        return queue.remove(pos);
                    }
                }
                None
            }
            SchedulingAlgorithm::Cfs => {
                let mut queue = self.cfs_queue.lock().await;
                for (key, tasks) in queue.iter_mut() {
                    if let Some(pos) = tasks.iter().position(|t| t.id == task_id) {
                        let key = *key;
                        let task = tasks.remove(pos);
                        if tasks.is_empty() {
                            queue.remove(&key);
                        }
                        return task;
                    }
                }
                None
            }
            SchedulingAlgorithm::Edf => {
                let mut queue = self.edf_queue.lock().await;
                for (key, tasks) in queue.iter_mut() {
                    if let Some(pos) = tasks.iter().position(|t| t.id == task_id) {
                        let key = *key;
                        let task = tasks.remove(pos);
                        if tasks.is_empty() {
                            queue.remove(&key);
                        }
                        return task;
                    }
                }
                None
            }
        }
    }

    /// Get specific task
    pub async fn get(&self, task_id: TaskId) -> Option<Task> {
        match self.algorithm {
            SchedulingAlgorithm::Fcfs | SchedulingAlgorithm::RoundRobin => self.queues[5]
                .lock()
                .await
                .iter()
                .find(|t| t.id == task_id)
                .cloned(),
            SchedulingAlgorithm::Priority => {
                for queue in &self.queues {
                    if let Some(task) = queue.lock().await.iter().find(|t| t.id == task_id) {
                        return Some(task.clone());
                    }
                }
                None
            }
            SchedulingAlgorithm::Cfs => {
                let queue = self.cfs_queue.lock().await;
                for tasks in queue.values() {
                    if let Some(task) = tasks.iter().find(|t| t.id == task_id) {
                        return Some(task.clone());
                    }
                }
                None
            }
            SchedulingAlgorithm::Edf => {
                let queue = self.edf_queue.lock().await;
                for tasks in queue.values() {
                    if let Some(task) = tasks.iter().find(|t| t.id == task_id) {
                        return Some(task.clone());
                    }
                }
                None
            }
        }
    }

    /// Get queue length
    pub async fn len(&self) -> usize {
        match self.algorithm {
            SchedulingAlgorithm::Fcfs | SchedulingAlgorithm::RoundRobin => {
                self.queues[5].lock().await.len()
            }
            SchedulingAlgorithm::Priority => {
                let mut total = 0;
                for queue in &self.queues {
                    total += queue.lock().await.len();
                }
                total
            }
            SchedulingAlgorithm::Cfs => self.cfs_queue.lock().await.values().map(|q| q.len()).sum(),
            SchedulingAlgorithm::Edf => self.edf_queue.lock().await.values().map(|q| q.len()).sum(),
        }
    }

    /// Check if empty
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Convert priority to CFS weight
    fn prio_to_weight(&self, priority: Priority) -> u64 {
        // Map -20..20 to weight 88761..15
        let prio = priority.level().clamp(-20, 20);
        let weights = [
            88761, 71755, 56483, 46273, 36291, 29154, 23254, 18705, 14949, 11916, 9548, 7620, 6100,
            4904, 3906, 3121, 2501, 1991, 1586, 1277, 1024, 820, 655, 526, 423, 335, 272, 215, 172,
            137, 110, 87, 70, 56, 45, 36, 29, 23, 18, 15,
        ];
        weights[(prio + 20) as usize]
    }
}

/// Work-stealing queue for multi-core
pub struct WorkStealingQueue {
    /// Local queue
    local: Mutex<VecDeque<Task>>,
    /// Stealable queue
    stealable: crossbeam::deque::Worker<Task>,
}

impl WorkStealingQueue {
    /// Create new work-stealing queue
    pub fn new() -> (Self, crossbeam::deque::Stealer<Task>) {
        let worker = crossbeam::deque::Worker::new_fifo();
        let stealer = worker.stealer();

        let queue = Self {
            local: Mutex::new(VecDeque::new()),
            stealable: worker,
        };

        (queue, stealer)
    }

    /// Push task to local queue
    pub async fn push_local(&self, task: Task) {
        self.local.lock().await.push_back(task);
    }

    /// Push task to stealable queue
    pub fn push_stealable(&self, task: Task) {
        self.stealable.push(task);
    }

    /// Pop from local queue
    pub async fn pop_local(&self) -> Option<Task> {
        self.local.lock().await.pop_front()
    }

    /// Pop from stealable queue
    pub fn pop_stealable(&self) -> Option<Task> {
        self.stealable.pop()
    }

    /// Steal from this queue
    pub fn steal(&self, stealer: &crossbeam::deque::Stealer<Task>) -> Option<Task> {
        match stealer.steal() {
            crossbeam::deque::Steal::Success(task) => Some(task),
            _ => None,
        }
    }
}

/// Rate-limited queue
#[allow(dead_code)]
pub struct RateLimitedQueue {
    #[allow(dead_code)]
    inner: TaskQueue,
    /// Max tasks per second
    #[allow(dead_code)]
    rate_limit: u32,
    /// Tokens available
    #[allow(dead_code)]
    tokens: Mutex<u32>,
    /// Last refill time
    #[allow(dead_code)]
    last_refill: Mutex<std::time::Instant>,
}

impl RateLimitedQueue {
    /// Create new rate-limited queue
    pub fn new(algorithm: SchedulingAlgorithm, rate_limit: u32) -> Self {
        Self {
            inner: TaskQueue::new(algorithm),
            rate_limit,
            tokens: Mutex::new(rate_limit),
            last_refill: Mutex::new(std::time::Instant::now()),
        }
    }

    /// Refill tokens
    #[allow(dead_code)]
    async fn refill(&self) {
        let now = std::time::Instant::now();
        let mut last = self.last_refill.lock().await;
        let elapsed = now.duration_since(*last);
        let to_add = (elapsed.as_secs() as u32 * self.rate_limit).min(self.rate_limit);

        let mut tokens = self.tokens.lock().await;
        *tokens = (*tokens + to_add).min(self.rate_limit);
        *last = now;
    }

    /// Try to consume token
    #[allow(dead_code)]
    async fn try_consume(&self) -> bool {
        self.refill().await;
        let mut tokens = self.tokens.lock().await;
        if *tokens > 0 {
            *tokens -= 1;
            true
        } else {
            false
        }
    }
}
