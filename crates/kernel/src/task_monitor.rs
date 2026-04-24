//! Task Monitor
//!
//! Provides real-time task state change notifications from Kernel to external
//! components (like AgentStateManager).
//!
//! 🔒 P0 FIX: This module implements the Kernel → StateManager event
//! notification mechanism, ensuring state synchronization between kernel tasks
//! and agent states.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};
use tracing::debug;

use crate::scheduler::{TaskId, TaskInfo, TaskStatus};

/// Task state change event
#[derive(Debug, Clone)]
pub struct TaskStateEvent {
    /// Task ID
    pub task_id: TaskId,
    /// Old status (if known)
    pub old_status: Option<TaskStatus>,
    /// New status
    pub new_status: TaskStatus,
    /// Task info snapshot
    pub task_info: TaskInfo,
    /// Timestamp
    pub timestamp: std::time::Instant,
}

/// Task state change handler trait
#[async_trait::async_trait]
pub trait TaskStateHandler: Send + Sync {
    /// Called when a task state changes
    async fn on_task_state_change(&self, event: TaskStateEvent);
}

/// Task monitor for tracking task state changes
///
/// 🔒 P0 FIX: This allows external components to subscribe to kernel task
/// state changes without polling.
pub struct TaskMonitor {
    /// Subscribers for task state changes
    subscribers: RwLock<Vec<Box<dyn TaskStateHandler>>>,
    /// Task status cache (for detecting changes)
    /// Note: Using Arc<RwLock<>> to allow cloning for background cleanup tasks
    status_cache: Arc<RwLock<HashMap<TaskId, TaskStatus>>>,
    /// Event channel for async notifications
    event_tx: mpsc::UnboundedSender<TaskStateEvent>,
    /// Event receiver (for internal processing)
    event_rx: RwLock<mpsc::UnboundedReceiver<TaskStateEvent>>,
}

impl TaskMonitor {
    /// Create a new task monitor
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        Self {
            subscribers: RwLock::new(Vec::new()),
            status_cache: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            event_rx: RwLock::new(event_rx),
        }
    }

    /// Register a state change handler
    pub async fn subscribe<H>(&self, handler: H)
    where
        H: TaskStateHandler + 'static,
    {
        let mut subscribers = self.subscribers.write().await;
        subscribers.push(Box::new(handler));
    }

    /// Report a task state change
    ///
    /// This should be called by the scheduler when a task's state changes.
    pub async fn report_state_change(
        &self,
        task_id: TaskId,
        new_status: TaskStatus,
        task_info: TaskInfo,
    ) {
        // Check if status actually changed
        let mut cache = self.status_cache.write().await;
        let old_status = cache.get(&task_id).cloned();

        // Only report if changed
        if old_status.as_ref() != Some(&new_status) {
            cache.insert(task_id, new_status.clone());
            drop(cache);

            let event = TaskStateEvent {
                task_id,
                old_status,
                new_status,
                task_info,
                timestamp: std::time::Instant::now(),
            };

            // Send to internal channel
            let _ = self.event_tx.send(event.clone());

            // Notify subscribers
            self.notify_subscribers(event).await;
        }
    }

    /// Notify all subscribers of a state change
    async fn notify_subscribers(&self, event: TaskStateEvent) {
        let subscribers = self.subscribers.read().await;
        for handler in subscribers.iter() {
            handler.on_task_state_change(event.clone()).await;
        }
    }

    /// Get the last known status for a task
    pub async fn get_cached_status(&self, task_id: TaskId) -> Option<TaskStatus> {
        let cache = self.status_cache.read().await;
        cache.get(&task_id).cloned()
    }

    /// Clear the status cache for a task
    pub async fn clear_cache(&self, task_id: TaskId) {
        let mut cache = self.status_cache.write().await;
        cache.remove(&task_id);
    }

    /// Start the event processing loop
    ///
    /// This should be called once during kernel initialization.
    pub async fn start_processing(&self) {
        let mut rx = self.event_rx.write().await;

        while let Some(event) = rx.recv().await {
            debug!(
                "Task {} state changed: {:?} -> {:?}",
                event.task_id.as_u64(),
                event.old_status,
                event.new_status
            );

            // Additional processing can be done here
            // e.g., metrics collection, logging, etc.

            // If task is completed, failed, or timed out, clean up cache after a delay
            match event.new_status {
                TaskStatus::Completed
                | TaskStatus::Failed
                | TaskStatus::Cancelled
                | TaskStatus::TimedOut => {
                    let task_id = event.task_id;
                    let cache = self.status_cache.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
                        cache.write().await.remove(&task_id);
                    });
                }
                _ => {}
            }
        }
    }

    /// Create a stream of state change events
    pub async fn subscribe_stream(&self) -> mpsc::UnboundedReceiver<TaskStateEvent> {
        let (tx, rx) = mpsc::unbounded_channel();

        // Create a stream handler
        struct StreamHandler(mpsc::UnboundedSender<TaskStateEvent>);

        #[async_trait::async_trait]
        impl TaskStateHandler for StreamHandler {
            async fn on_task_state_change(&self, event: TaskStateEvent) {
                let _ = self.0.send(event);
            }
        }

        self.subscribe(StreamHandler(tx)).await;
        rx
    }
}

impl Default for TaskMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Bridge handler that forwards kernel task events to AgentStateManager
///
/// 🔒 P0 FIX: This bridges the gap between kernel task lifecycle and
/// agent state management.
pub struct AgentStateBridge {
    /// Mapping from task_id to agent_id
    task_to_agent: Arc<RwLock<HashMap<u64, String>>>,
    /// State manager handle
    state_manager: Arc<dyn AgentStateManagerInterface>,
}

/// Interface for state manager operations (to avoid circular dependencies)
#[async_trait::async_trait]
pub trait AgentStateManagerInterface: Send + Sync {
    /// Transition agent state
    async fn transition_agent(&self, agent_id: &str, new_state: &str, reason: &str);
    /// Get agent ID by kernel task ID
    async fn get_agent_by_task(&self, task_id: u64) -> Option<String>;
}

impl AgentStateBridge {
    /// Create a new bridge
    pub fn new(state_manager: Arc<dyn AgentStateManagerInterface>) -> Self {
        Self {
            task_to_agent: Arc::new(RwLock::new(HashMap::new())),
            state_manager,
        }
    }

    /// Register a task-agent mapping
    pub async fn register_task_agent(&self, task_id: u64, agent_id: impl Into<String>) {
        let mut mapping = self.task_to_agent.write().await;
        mapping.insert(task_id, agent_id.into());
    }

    /// Unregister a task-agent mapping
    pub async fn unregister_task(&self, task_id: u64) {
        let mut mapping = self.task_to_agent.write().await;
        mapping.remove(&task_id);
    }

    /// Map task status to agent state
    fn map_status_to_state(status: TaskStatus) -> (&'static str, String) {
        match status {
            TaskStatus::Pending => ("initializing", "Task is pending".to_string()),
            TaskStatus::Running => ("working", "Task is running".to_string()),
            TaskStatus::Completed => ("idle", "Task completed successfully".to_string()),
            TaskStatus::Failed => ("error", "Task failed".to_string()),
            TaskStatus::Cancelled => ("stopped", "Task was cancelled".to_string()),
            TaskStatus::TimedOut => ("error", "Task timed out".to_string()),
        }
    }
}

#[async_trait::async_trait]
impl TaskStateHandler for AgentStateBridge {
    async fn on_task_state_change(&self, event: TaskStateEvent) {
        let task_id = event.task_id.as_u64();

        // Get agent ID for this task
        let agent_id = {
            let mapping = self.task_to_agent.read().await;
            mapping.get(&task_id).cloned()
        };

        if let Some(agent_id) = agent_id {
            let (state, reason) = Self::map_status_to_state(event.new_status);

            debug!(
                "Syncing agent {} state to '{}' (task {} status change)",
                agent_id, state, task_id
            );

            self.state_manager
                .transition_agent(&agent_id, state, &reason)
                .await;
        } else {
            // Try to get from state manager
            if let Some(agent_id) = self.state_manager.get_agent_by_task(task_id).await {
                let (state, reason) = Self::map_status_to_state(event.new_status);

                self.state_manager
                    .transition_agent(&agent_id, state, &reason)
                    .await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::TaskId;

    #[tokio::test]
    async fn test_task_monitor() {
        let monitor = TaskMonitor::new();

        // Test initial state
        let task_id = TaskId::new(1);
        assert!(monitor.get_cached_status(task_id).await.is_none());

        // Test state change reporting
        let task_info = TaskInfo {
            id: task_id,
            name: "test-task".to_string(),
            state: crate::scheduler::TaskState::Running,
            priority: crate::scheduler::Priority::Normal,
            created_at: std::time::Instant::now(),
            started_at: None,
            completed_at: None,
            capabilities: Default::default(),
            resource_limits: Default::default(),
        };

        monitor
            .report_state_change(task_id, TaskStatus::Running, task_info.clone())
            .await;

        assert_eq!(
            monitor.get_cached_status(task_id).await,
            Some(TaskStatus::Running)
        );
    }
}
