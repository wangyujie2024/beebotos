//! Priority Scheduler
//!
//! Priority-based scheduling algorithm.

use std::collections::BinaryHeap;
use std::time::Duration;

use super::{Task, TaskId, TaskState};

/// Priority scheduler trait
pub trait PrioritySchedulerTrait {
    /// Schedule next task
    fn schedule(&mut self, tasks: &mut [Task]) -> Option<TaskId>;
    /// Add process to scheduler
    fn add_task(&mut self, task: Task);
    /// Remove task
    fn remove_task(&mut self, task_id: TaskId);
    /// Time tick
    fn tick(&mut self);
}

#[derive(Debug, Clone)]
struct PriorityItem {
    task_id: TaskId,
    priority: i32,
    timestamp: u64,
}

impl PartialEq for PriorityItem {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.timestamp == other.timestamp
    }
}

impl Eq for PriorityItem {}

impl PartialOrd for PriorityItem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PriorityItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .priority
            .cmp(&self.priority)
            .then_with(|| other.timestamp.cmp(&self.timestamp))
    }
}

/// Priority scheduler
pub struct PriorityScheduler {
    ready_queue: BinaryHeap<PriorityItem>,
    aging_threshold: Duration,
    boost_interval: Duration,
    last_boost: std::time::Instant,
    counter: u64,
}

impl PriorityScheduler {
    /// Create new scheduler
    pub fn new() -> Self {
        Self {
            ready_queue: BinaryHeap::new(),
            aging_threshold: Duration::from_secs(5),
            boost_interval: Duration::from_secs(30),
            last_boost: std::time::Instant::now(),
            counter: 0,
        }
    }

    /// With aging threshold
    pub fn with_aging_threshold(mut self, threshold: Duration) -> Self {
        self.aging_threshold = threshold;
        self
    }

    #[allow(dead_code)]
    fn calculate_dynamic_priority(&self, task: &Task) -> i32 {
        let base_priority = match task.priority {
            super::Priority::RealTime => 100,
            super::Priority::High => 80,
            super::Priority::Normal => 50,
            super::Priority::Low => 20,
            super::Priority::Idle => 0,
        };

        let elapsed = task.elapsed().as_millis() as u64;
        let aging_bonus = (elapsed / self.aging_threshold.as_millis() as u64) as i32;
        base_priority + aging_bonus.min(20)
    }

    fn boost_priorities(&mut self, tasks: &mut [Task]) {
        for task in tasks.iter_mut() {
            if task.state == TaskState::Ready {
                task.priority = match task.priority {
                    super::Priority::Idle => super::Priority::Low,
                    super::Priority::Low => super::Priority::Normal,
                    super::Priority::Normal => super::Priority::High,
                    p => p,
                };
            }
        }
    }
}

impl PrioritySchedulerTrait for PriorityScheduler {
    fn schedule(&mut self, tasks: &mut [Task]) -> Option<TaskId> {
        if self.last_boost.elapsed() > self.boost_interval {
            self.boost_priorities(tasks);
            self.last_boost = std::time::Instant::now();
        }

        while let Some(item) = self.ready_queue.pop() {
            if let Some(task) = tasks.iter().find(|t| t.id == item.task_id) {
                if task.state == TaskState::Ready {
                    return Some(item.task_id);
                }
            }
        }
        None
    }

    fn add_task(&mut self, task: Task) {
        self.counter += 1;
        let dynamic_priority = match task.priority {
            super::Priority::RealTime => 100,
            super::Priority::High => 80,
            super::Priority::Normal => 50,
            super::Priority::Low => 20,
            super::Priority::Idle => 0,
        };

        self.ready_queue.push(PriorityItem {
            task_id: task.id,
            priority: dynamic_priority,
            timestamp: self.counter,
        });
    }

    fn remove_task(&mut self, task_id: TaskId) {
        let mut temp = BinaryHeap::new();
        while let Some(item) = self.ready_queue.pop() {
            if item.task_id != task_id {
                temp.push(item);
            }
        }
        self.ready_queue = temp;
    }

    fn tick(&mut self) {}
}

impl Default for PriorityScheduler {
    fn default() -> Self {
        Self::new()
    }
}

/// EDF Scheduler trait
pub trait EDFSchedulerTrait {
    /// Schedule the next task to run
    fn schedule(&mut self, tasks: &mut [Task]) -> Option<TaskId>;
    /// Add a task to the scheduler
    fn add_task(&mut self, task: Task);
    /// Remove a task from the scheduler
    fn remove_task(&mut self, task_id: TaskId);
}

/// EDF Scheduler placeholder
pub struct EDFScheduler;

impl EDFScheduler {
    /// Create a new EDF scheduler
    pub fn new() -> Self {
        Self
    }

    /// Check if scheduler can accept more tasks
    pub fn is_schedulable(&self) -> bool {
        true // Placeholder
    }
}

impl EDFSchedulerTrait for EDFScheduler {
    fn schedule(&mut self, tasks: &mut [Task]) -> Option<TaskId> {
        tasks
            .iter()
            .filter(|t| t.state == TaskState::Ready)
            .min_by_key(|t| t.vruntime)
            .map(|t| t.id)
    }

    fn add_task(&mut self, _task: Task) {}

    fn remove_task(&mut self, _task_id: TaskId) {}
}

impl Default for EDFScheduler {
    fn default() -> Self {
        Self::new()
    }
}
