//! Chain Module Message Bus Integration
//!
//! Provides integration between Chain module and the unified Message Bus.

use std::sync::Arc;

use beebotos_message_bus::{Message, MessageBus, Result as BusResult};

/// Blockchain transaction events
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ChainTransactionEvent {
    /// Transaction submitted
    Submitted {
        tx_hash: String,
        from: String,
        to: String,
        value: String,
        timestamp: u64,
    },
    /// Transaction confirmed
    Confirmed {
        tx_hash: String,
        block_number: u64,
        gas_used: u64,
        status: bool,
        timestamp: u64,
    },
    /// Transaction failed
    Failed {
        tx_hash: String,
        error: String,
        timestamp: u64,
    },
}

/// DAO events
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum DaoEvent {
    /// Proposal created
    ProposalCreated {
        proposal_id: u64,
        proposer: String,
        description: String,
        timestamp: u64,
    },
    /// Vote cast
    VoteCast {
        proposal_id: u64,
        voter: String,
        support: bool,
        voting_power: u64,
        timestamp: u64,
    },
    /// Proposal executed
    ProposalExecuted { proposal_id: u64, timestamp: u64 },
}

/// Chain Message Bus adapter
pub struct ChainMessageBus<B: MessageBus> {
    bus: Arc<B>,
}

impl<B: MessageBus> ChainMessageBus<B> {
    /// Create a new Chain Message Bus adapter
    pub fn new(bus: Arc<B>) -> Self {
        Self { bus }
    }

    /// Publish transaction event
    pub async fn publish_transaction(&self, event: ChainTransactionEvent) -> BusResult<()> {
        let topic = format!(
            "chain/tx/{}",
            match &event {
                ChainTransactionEvent::Submitted { tx_hash, .. } => tx_hash.clone(),
                ChainTransactionEvent::Confirmed { tx_hash, .. } => tx_hash.clone(),
                ChainTransactionEvent::Failed { tx_hash, .. } => tx_hash.clone(),
            }
        );

        let payload = serde_json::to_vec(&event).unwrap_or_default();
        let message = Message::new(&topic, payload);
        self.bus.publish(&topic, message).await
    }

    /// Publish DAO event
    pub async fn publish_dao_event(&self, event: DaoEvent) -> BusResult<()> {
        let topic = "chain/dao/events".to_string();
        let payload = serde_json::to_vec(&event).unwrap_or_default();
        let message = Message::new(&topic, payload);
        self.bus.publish(&topic, message).await
    }

    /// Subscribe to transaction events
    pub async fn subscribe_transactions(
        &self,
    ) -> BusResult<(
        beebotos_message_bus::SubscriptionId,
        beebotos_message_bus::MessageStream,
    )> {
        self.bus.subscribe("chain/tx/+").await
    }

    /// Subscribe to DAO events
    pub async fn subscribe_dao_events(
        &self,
    ) -> BusResult<(
        beebotos_message_bus::SubscriptionId,
        beebotos_message_bus::MessageStream,
    )> {
        self.bus.subscribe("chain/dao/+").await
    }

    /// Publish transaction submitted
    pub async fn transaction_submitted(
        &self,
        tx_hash: String,
        from: String,
        to: String,
        value: String,
    ) -> BusResult<()> {
        self.publish_transaction(ChainTransactionEvent::Submitted {
            tx_hash,
            from,
            to,
            value,
            timestamp: chrono::Utc::now().timestamp() as u64,
        })
        .await
    }

    /// Publish transaction confirmed
    pub async fn transaction_confirmed(
        &self,
        tx_hash: String,
        block_number: u64,
        gas_used: u64,
        status: bool,
    ) -> BusResult<()> {
        self.publish_transaction(ChainTransactionEvent::Confirmed {
            tx_hash,
            block_number,
            gas_used,
            status,
            timestamp: chrono::Utc::now().timestamp() as u64,
        })
        .await
    }

    /// Publish proposal created
    pub async fn proposal_created(
        &self,
        proposal_id: u64,
        proposer: String,
        description: String,
    ) -> BusResult<()> {
        self.publish_dao_event(DaoEvent::ProposalCreated {
            proposal_id,
            proposer,
            description,
            timestamp: chrono::Utc::now().timestamp() as u64,
        })
        .await
    }

    /// Publish vote cast
    pub async fn vote_cast(
        &self,
        proposal_id: u64,
        voter: String,
        support: bool,
        voting_power: u64,
    ) -> BusResult<()> {
        self.publish_dao_event(DaoEvent::VoteCast {
            proposal_id,
            voter,
            support,
            voting_power,
            timestamp: chrono::Utc::now().timestamp() as u64,
        })
        .await
    }
}

impl<B: MessageBus> Clone for ChainMessageBus<B> {
    fn clone(&self) -> Self {
        Self {
            bus: Arc::clone(&self.bus),
        }
    }
}

use std::sync::OnceLock;

/// Global Message Bus handle
///
/// SECURITY FIX: Replaced `static mut` with `OnceLock` for thread-safe
/// initialization
static CHAIN_MESSAGE_BUS: OnceLock<Arc<dyn MessageBus>> = OnceLock::new();

/// Initialize Chain Message Bus
///
/// This function is thread-safe and can only be called once.
pub fn init_message_bus<B: MessageBus + 'static>(bus: Arc<B>) -> Result<(), &'static str> {
    CHAIN_MESSAGE_BUS
        .set(bus)
        .map_err(|_| "Chain message bus already initialized")
}

/// Get global Message Bus
pub fn message_bus() -> Option<Arc<dyn MessageBus>> {
    CHAIN_MESSAGE_BUS.get().cloned()
}

#[cfg(test)]
mod tests {
    use beebotos_message_bus::{DefaultMessageBus, JsonCodec, MemoryTransport};

    use super::*;

    #[tokio::test]
    async fn test_chain_message_bus() {
        let bus = Arc::new(DefaultMessageBus::new(
            MemoryTransport::new(),
            Box::new(JsonCodec::new()),
            None,
        ));
        let chain_bus = ChainMessageBus::new(bus);

        assert!(chain_bus
            .transaction_submitted(
                "0x123".to_string(),
                "0xabc".to_string(),
                "0xdef".to_string(),
                "1000".to_string()
            )
            .await
            .is_ok());
    }
}
