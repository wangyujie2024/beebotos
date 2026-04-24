//! Agent-specific event types for SystemEventBus

use beebotos_core::event_bus::SystemEvent;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Agent lifecycle events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentLifecycleEvent {
    /// Agent created
    Created {
        agent_id: String,
        owner_id: String,
        capabilities: Vec<String>,
    },
    /// Agent initialized
    Initialized { agent_id: String, duration_ms: u64 },
    /// Agent started
    Started {
        agent_id: String,
        kernel_task_id: Option<u64>,
    },
    /// Task assigned to agent
    TaskAssigned {
        agent_id: String,
        task_id: String,
        task_type: String,
    },
    /// Task completed
    TaskCompleted {
        agent_id: String,
        task_id: String,
        success: bool,
        duration_ms: u64,
    },
    /// Agent paused
    Paused { agent_id: String, reason: String },
    /// Agent resumed
    Resumed { agent_id: String },
    /// Agent stopped
    Stopped { agent_id: String, reason: String },
    /// Agent error
    Error {
        agent_id: String,
        error: String,
        fatal: bool,
    },
}

impl SystemEvent for AgentLifecycleEvent {
    fn event_type(&self) -> &'static str {
        "agent.lifecycle"
    }

    fn timestamp(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Agent state change event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStateEvent {
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// Agent ID
    pub agent_id: String,
    /// Old state
    pub old_state: String,
    /// New state
    pub new_state: String,
    /// Transition reason
    pub reason: Option<String>,
    /// Additional metadata
    pub metadata: std::collections::HashMap<String, String>,
}

impl AgentStateEvent {
    /// Create a new state change event
    pub fn new(
        agent_id: impl Into<String>,
        old_state: impl Into<String>,
        new_state: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            agent_id: agent_id.into(),
            old_state: old_state.into(),
            new_state: new_state.into(),
            reason: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Set reason
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }
}

impl SystemEvent for AgentStateEvent {
    fn event_type(&self) -> &'static str {
        "agent.state"
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}

/// Agent task execution event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskEvent {
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// Agent ID
    pub agent_id: String,
    /// Task ID
    pub task_id: String,
    /// Event type
    pub event_type: TaskEventType,
}

/// Task event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskEventType {
    Started,
    Progress { percent: u8, message: String },
    Completed { result: String },
    Failed { error: String },
    Cancelled,
}

impl SystemEvent for AgentTaskEvent {
    fn event_type(&self) -> &'static str {
        "agent.task"
    }

    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}
