//! Event system for BeeBotOS

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};

use crate::types::{AgentId, AgentStatus, Timestamp};

/// Event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    /// Agent lifecycle event
    AgentLifecycle {
        /// Agent ID
        agent_id: AgentId,
        /// Old status
        from: AgentStatus,
        /// New status
        to: AgentStatus,
        /// Timestamp
        timestamp: Timestamp,
    },

    /// Agent spawned sub-agent
    AgentSpawned {
        /// Parent agent ID
        parent_id: AgentId,
        /// Child agent ID
        child_id: AgentId,
        /// Timestamp
        timestamp: Timestamp,
    },

    /// Memory consolidated
    MemoryConsolidated {
        /// Agent ID
        agent_id: AgentId,
        /// Memory ID
        memory_id: String,
        /// Memory type
        memory_type: String,
        /// Timestamp
        timestamp: Timestamp,
    },

    /// Blockchain transaction
    BlockchainTx {
        /// Transaction hash
        tx_hash: String,
        /// Chain ID
        chain_id: String,
        /// Status
        status: TxStatus,
        /// Timestamp
        timestamp: Timestamp,
    },

    /// DAO proposal created
    DaoProposalCreated {
        /// Proposal ID
        proposal_id: u64,
        /// Proposer
        proposer: String,
        /// Proposal type
        proposal_type: String,
        /// Timestamp
        timestamp: Timestamp,
    },

    /// DAO vote cast
    DaoVoteCast {
        /// Proposal ID
        proposal_id: u64,
        /// Voter
        voter: String,
        /// Vote type
        vote: i8,
        /// Voting power
        weight: u64,
        /// Timestamp
        timestamp: Timestamp,
    },

    /// Skill executed
    SkillExecuted {
        /// Agent ID
        agent_id: AgentId,
        /// Skill name
        skill_name: String,
        /// Execution time (ms)
        execution_time_ms: u64,
        /// Success
        success: bool,
        /// Timestamp
        timestamp: Timestamp,
    },

    /// System metric
    Metric {
        /// Metric name
        name: String,
        /// Metric value
        value: f64,
        /// Labels
        labels: HashMap<String, String>,
        /// Timestamp
        timestamp: Timestamp,
    },

    /// Task started
    TaskStarted {
        /// Task ID
        task_id: String,
        /// Agent ID
        agent_id: Option<AgentId>,
        /// Timestamp
        timestamp: Timestamp,
    },

    /// Task completed
    TaskCompleted {
        /// Task ID
        task_id: String,
        /// Agent ID
        agent_id: Option<AgentId>,
        /// Success
        success: bool,
        /// Timestamp
        timestamp: Timestamp,
    },
}

/// Transaction status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TxStatus {
    /// Pending confirmation
    Pending,
    /// Confirmed
    Confirmed,
    /// Failed
    Failed,
}

/// Event bus for broadcasting events
#[derive(Debug)]
pub struct EventBus {
    /// Subscribers
    subscribers: Arc<RwLock<HashMap<String, mpsc::UnboundedSender<Event>>>>,
}

impl EventBus {
    /// Create a new event bus
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Subscribe to events
    pub async fn subscribe(&self, name: &str) -> mpsc::UnboundedReceiver<Event> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.subscribers.write().await.insert(name.to_string(), tx);
        rx
    }

    /// Unsubscribe from events
    pub async fn unsubscribe(&self, name: &str) {
        self.subscribers.write().await.remove(name);
    }

    /// Emit an event to all subscribers
    pub async fn emit(&self, event: Event) {
        let subscribers = self.subscribers.read().await;
        for (name, tx) in subscribers.iter() {
            if let Err(e) = tx.send(event.clone()) {
                tracing::warn!("Failed to send event to {}: {}", name, e);
            }
        }
    }

    /// Emit an event filtered by type
    pub async fn emit_filtered<F>(&self, event: Event, filter: F)
    where
        F: Fn(&Event) -> bool,
    {
        if filter(&event) {
            self.emit(event).await;
        }
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AgentStatus;

    #[tokio::test]
    async fn test_event_bus() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe("test").await;

        let event = Event::AgentLifecycle {
            agent_id: AgentId::new(),
            from: AgentStatus::Idle,
            to: AgentStatus::Running,
            timestamp: Timestamp::now(),
        };

        bus.emit(event.clone()).await;

        let received = rx.try_recv();
        assert!(received.is_ok());
    }
}
