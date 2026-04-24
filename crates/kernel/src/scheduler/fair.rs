//! CFS (Completely Fair Scheduler)
//!
//! Fair scheduling algorithm for agent tasks.

use std::collections::BTreeMap;
use std::time::Duration;

use super::{Task, TaskId};

/// CFS Scheduler
pub struct CFSScheduler {
    /// Tasks by vruntime
    tasks: BTreeMap<u64, Task>,
    /// Current task
    current: Option<TaskId>,
    /// Time slice for each task
    time_slice: Duration,
    /// Minimum granularity
    min_granularity: Duration,
    /// Target latency
    target_latency: Duration,
}

/// Virtual runtime calculation
#[derive(Debug, Clone)]
pub struct VRuntime {
    /// Virtual runtime value
    pub value: u64,
    /// Priority weight
    pub weight: u32,
}

/// Priority weights (nice -20 to 19)
pub const PRIORITY_WEIGHTS: [u32; 40] = [
    88761, 71755, 56483, 46273, 36291, 29154, 23254, 18705, 14949, 11916, 9548, 7620, 6100, 4904,
    3906, 3121, 2501, 1991, 1586, 1277, 1024, 820, 655, 526, 423, 335, 272, 215, 172, 137, 110, 87,
    70, 56, 45, 36, 29, 23, 18, 15,
];

impl CFSScheduler {
    /// Create a new CFS scheduler
    pub fn new() -> Self {
        Self {
            tasks: BTreeMap::new(),
            current: None,
            time_slice: Duration::from_millis(100),
            min_granularity: Duration::from_millis(1),
            target_latency: Duration::from_millis(20),
        }
    }

    /// Add task to scheduler
    pub fn enqueue(&mut self, task: Task) {
        let vruntime = self.calculate_vruntime(&task);
        self.tasks.insert(vruntime, task);
    }

    /// Pick next task to run (lowest vruntime)
    pub fn pick_next(&mut self) -> Option<Task> {
        if let Some((_vruntime, task)) = self.tasks.pop_first() {
            self.current = Some(task.id);
            Some(task)
        } else {
            None
        }
    }

    /// Update task vruntime after execution
    pub fn update_vruntime(&mut self, task: &mut Task, elapsed: Duration) {
        let weight = self.get_weight(task.priority);
        let delta_vruntime = (elapsed.as_nanos() as u64 * 1024) / weight as u64;
        task.vruntime += delta_vruntime;
    }

    /// Calculate ideal time slice for a task
    pub fn time_slice(&self, num_tasks: usize) -> Duration {
        if num_tasks == 0 {
            return self.time_slice;
        }

        let slice = self.target_latency / num_tasks as u32;
        slice.max(self.min_granularity)
    }

    /// Calculate vruntime for new task
    fn calculate_vruntime(&self, _task: &Task) -> u64 {
        // If first task, start at 0
        // Otherwise, start at min vruntime to avoid starvation
        self.tasks.keys().next().copied().unwrap_or(0)
    }

    /// Get weight for priority
    fn get_weight(&self, priority: super::Priority) -> u32 {
        let nice = match priority {
            super::Priority::RealTime => -20,
            super::Priority::High => -10,
            super::Priority::Normal => 0,
            super::Priority::Low => 10,
            super::Priority::Idle => 15,
        };

        let idx = (nice + 20) as usize;
        PRIORITY_WEIGHTS[idx.min(39)]
    }

    /// Get number of tasks
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Check if scheduler has no tasks
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }
}

impl Default for CFSScheduler {
    fn default() -> Self {
        Self::new()
    }
}

/// Scheduler statistics
#[derive(Debug, Clone)]
pub struct SchedulerStats {
    /// Number of tasks currently queued
    pub tasks_queued: usize,
    /// Currently executing task ID if any
    pub current_task: Option<TaskId>,
    /// Minimum virtual runtime in queue
    pub min_vruntime: u64,
    /// Maximum virtual runtime in queue
    pub max_vruntime: u64,
}

impl CFSScheduler {
    /// Get scheduler statistics
    pub fn stats(&self) -> SchedulerStats {
        SchedulerStats {
            tasks_queued: self.tasks.len(),
            current_task: self.current,
            min_vruntime: self.tasks.keys().next().copied().unwrap_or(0),
            max_vruntime: self.tasks.keys().last().copied().unwrap_or(0),
        }
    }
}
