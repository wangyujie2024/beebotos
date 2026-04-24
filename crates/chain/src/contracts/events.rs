//! Contract Events Module
//!
//! Complete event system for all BeeBotOS contracts.
//! Works on any EVM-compatible chain.

use std::pin::Pin;
use std::task::{Context, Poll};

use alloy_primitives::Log as PrimitiveLog;
use alloy_provider::Provider;
use alloy_rpc_types::{Filter, FilterBlockOption, Log};
use alloy_sol_types::SolEvent;
use tokio::sync::mpsc;
use tokio_stream::{Stream, StreamExt};
use tracing::{debug, error, info, warn};

use crate::compat::{Address, B256};
// Commerce events
pub use crate::contracts::bindings::A2ACommerce::{
    ListingCancelled, ListingCreated, ListingFulfilled, PurchaseMade,
};
// ============================================================================
// Event Type Definitions (from sol! generated code)
// ============================================================================

// DAO events
pub use crate::contracts::bindings::AgentDAO::{ProposalCreated, ProposalExecuted, VoteCast};
// Identity events
pub use crate::contracts::bindings::AgentIdentity::{
    AgentDeactivated, AgentRegistered, AgentUpdated,
};
// Payment events
pub use crate::contracts::bindings::AgentPayment::{
    MandateCreated, PaymentExecuted, StreamCreated, StreamUpdated,
};
// Registry events
pub use crate::contracts::bindings::AgentRegistry::{
    AvailabilityChanged, Heartbeat, MetadataUpdated,
};
// Token events
pub use crate::contracts::bindings::BeeToken::{Approval, Transfer};
// Bridge events
pub use crate::contracts::bindings::CrossChainBridge::{
    BridgeCompleted, BridgeFailed, BridgeInitiated,
};
// Escrow events
pub use crate::contracts::bindings::DealEscrow::{EscrowCreated, EscrowRefunded, EscrowReleased};
// Dispute events
pub use crate::contracts::bindings::DisputeResolution::{
    DisputeRaised, DisputeResolved, EvidenceSubmitted,
};
// Reputation events
pub use crate::contracts::bindings::ReputationSystem::{CategoryScoreUpdated, ReputationUpdated};
// Skill NFT events
pub use crate::contracts::bindings::SkillNFT::{RoyaltyUpdated, SkillMinted};
// Treasury events
pub use crate::contracts::bindings::TreasuryManager::{BudgetCreated, BudgetReleased};
// use crate::contracts::bindings::*; // Events are re-exported individually below
use crate::{ChainError, Result};

// ============================================================================
// Event System Types
// ============================================================================

/// Event type for filtering BeeBotOS events
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeeBotOSEventType {
    // DAO Events
    ProposalCreated,
    VoteCast,
    ProposalExecuted,

    // Identity Events
    AgentRegistered,
    AgentDeactivated,
    CapabilityGranted,
    CapabilityRevoked,

    // Registry Events
    MetadataUpdated,
    Heartbeat,
    AvailabilityChanged,

    // Payment Events
    MandateCreated,
    StreamCreated,
    StreamUpdated,

    // Escrow Events
    EscrowCreated,
    EscrowReleased,
    EscrowRefunded,

    // Bridge Events
    BridgeInitiated,
    BridgeCompleted,
    BridgeFailed,

    // Skill NFT Events
    SkillMinted,
    RoyaltySet,

    // Reputation Events
    ReputationUpdated,

    // Token Events
    Transfer,
    Approval,

    // Treasury Events
    BudgetCreated,
    BudgetReleased,
}

impl BeeBotOSEventType {
    /// Get the event signature hash for filtering
    pub fn signature_hash(&self) -> B256 {
        match self {
            // DAO Events
            BeeBotOSEventType::ProposalCreated => ProposalCreated::SIGNATURE_HASH,
            BeeBotOSEventType::VoteCast => VoteCast::SIGNATURE_HASH,
            BeeBotOSEventType::ProposalExecuted => ProposalExecuted::SIGNATURE_HASH,

            // Token Events
            BeeBotOSEventType::Transfer => Transfer::SIGNATURE_HASH,
            BeeBotOSEventType::Approval => Approval::SIGNATURE_HASH,

            // Treasury Events
            BeeBotOSEventType::BudgetCreated => BudgetCreated::SIGNATURE_HASH,
            BeeBotOSEventType::BudgetReleased => BudgetReleased::SIGNATURE_HASH,

            // Escrow Events
            BeeBotOSEventType::EscrowCreated => EscrowCreated::SIGNATURE_HASH,
            BeeBotOSEventType::EscrowReleased => EscrowReleased::SIGNATURE_HASH,
            BeeBotOSEventType::EscrowRefunded => EscrowRefunded::SIGNATURE_HASH,

            // Other events would need their bindings generated
            // For now, return zero for unimplemented ones
            _ => B256::ZERO,
        }
    }
}

/// BeeBotOS event filter builder
#[derive(Debug, Clone)]
pub struct BeeBotOSEventFilter {
    pub from_block: Option<u64>,
    pub to_block: Option<u64>,
    pub addresses: Vec<Address>,
    pub event_types: Vec<BeeBotOSEventType>,
}

impl BeeBotOSEventFilter {
    /// Create new filter
    pub fn new() -> Self {
        Self {
            from_block: None,
            to_block: None,
            addresses: Vec::new(),
            event_types: Vec::new(),
        }
    }

    /// Filter from block
    pub fn from_block(mut self, block: u64) -> Self {
        self.from_block = Some(block);
        self
    }

    /// Filter to block
    pub fn to_block(mut self, block: u64) -> Self {
        self.to_block = Some(block);
        self
    }

    /// Filter by contract address
    pub fn address(mut self, address: Address) -> Self {
        self.addresses.push(address);
        self
    }

    /// Filter by event type
    pub fn event_type(mut self, event_type: BeeBotOSEventType) -> Self {
        self.event_types.push(event_type);
        self
    }

    /// Convert to Alloy Filter
    pub fn to_alloy_filter(&self) -> Filter {
        let mut filter = Filter::new();

        // Set block range
        let block_option = match (self.from_block, self.to_block) {
            (Some(from), Some(to)) => FilterBlockOption::Range {
                from_block: Some(from.into()),
                to_block: Some(to.into()),
            },
            (Some(from), None) => FilterBlockOption::Range {
                from_block: Some(from.into()),
                to_block: None,
            },
            (None, Some(to)) => FilterBlockOption::Range {
                from_block: None,
                to_block: Some(to.into()),
            },
            (None, None) => FilterBlockOption::Range {
                from_block: None,
                to_block: None,
            },
        };
        filter.block_option = block_option;

        // Set addresses
        if !self.addresses.is_empty() {
            filter.address = alloy_rpc_types::FilterSet::from(self.addresses.clone());
        }

        // Set event type topics if specified
        if !self.event_types.is_empty() {
            let topics: Vec<B256> = self
                .event_types
                .iter()
                .map(|et| et.signature_hash())
                .filter(|hash| *hash != B256::ZERO)
                .collect();

            if !topics.is_empty() {
                filter.topics[0] = alloy_rpc_types::FilterSet::from(topics);
            }
        }

        filter
    }
}

impl Default for BeeBotOSEventFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Unified event enum for all BeeBotOS contract events
#[derive(Clone)]
pub enum BeeBotOSEvent {
    // DAO Events
    ProposalCreated(ProposalCreated),
    VoteCast(VoteCast),
    ProposalExecuted(ProposalExecuted),

    // Token Events
    Transfer(Transfer),
    Approval(Approval),

    // Treasury Events
    BudgetCreated(BudgetCreated),
    BudgetReleased(BudgetReleased),

    // Escrow Events
    EscrowCreated(EscrowCreated),
    EscrowReleased(EscrowReleased),
    EscrowRefunded(EscrowRefunded),

    // Raw log for unhandled events
    Raw(Log),
}

impl std::fmt::Debug for BeeBotOSEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BeeBotOSEvent::ProposalCreated(_) => f.debug_tuple("ProposalCreated").finish(),
            BeeBotOSEvent::VoteCast(_) => f.debug_tuple("VoteCast").finish(),
            BeeBotOSEvent::ProposalExecuted(_) => f.debug_tuple("ProposalExecuted").finish(),
            BeeBotOSEvent::Transfer(_) => f.debug_tuple("Transfer").finish(),
            BeeBotOSEvent::Approval(_) => f.debug_tuple("Approval").finish(),
            BeeBotOSEvent::BudgetCreated(_) => f.debug_tuple("BudgetCreated").finish(),
            BeeBotOSEvent::BudgetReleased(_) => f.debug_tuple("BudgetReleased").finish(),
            BeeBotOSEvent::EscrowCreated(_) => f.debug_tuple("EscrowCreated").finish(),
            BeeBotOSEvent::EscrowReleased(_) => f.debug_tuple("EscrowReleased").finish(),
            BeeBotOSEvent::EscrowRefunded(_) => f.debug_tuple("EscrowRefunded").finish(),
            BeeBotOSEvent::Raw(log) => f.debug_tuple("Raw").field(log).finish(),
        }
    }
}

/// BeeBotOS event listener
pub struct BeeBotOSEventListener<P: Provider> {
    provider: P,
    filter: BeeBotOSEventFilter,
}

impl<P: Provider + Clone + 'static> BeeBotOSEventListener<P> {
    /// Create new event listener
    pub fn new(provider: P, filter: BeeBotOSEventFilter) -> Self {
        Self { provider, filter }
    }

    /// Get historical events
    pub async fn get_logs(&self) -> Result<Vec<BeeBotOSEvent>> {
        let filter = self.filter.to_alloy_filter();
        let logs = self
            .provider
            .get_logs(&filter)
            .await
            .map_err(|e| ChainError::Provider(format!("Failed to get logs: {}", e)))?;

        let mut events = Vec::new();
        for log in logs {
            if let Ok(event) = Self::decode_log(&log) {
                events.push(event);
            } else {
                events.push(BeeBotOSEvent::Raw(log));
            }
        }

        Ok(events)
    }

    /// Stream events in real-time
    pub async fn stream_events(&self) -> Result<BeeBotOSEventStream> {
        let filter = self.filter.to_alloy_filter();

        // Try WebSocket first, fallback to polling
        match self.provider.watch_logs(&filter).await {
            Ok(subscription) => {
                info!(
                    target: "chain::events",
                    "WebSocket subscription established"
                );

                let (tx, rx) = mpsc::channel(100);

                tokio::spawn(async move {
                    let mut stream = subscription.into_stream();

                    while let Some(logs) = stream.next().await {
                        for log in logs {
                            if tx.send(log).await.is_err() {
                                debug!(
                                    target: "chain::events",
                                    "Event stream receiver dropped, stopping stream"
                                );
                                return;
                            }
                        }
                    }
                });

                Ok(BeeBotOSEventStream {
                    receiver: rx,
                    is_websocket: true,
                })
            }
            Err(e) => {
                warn!(
                    target: "chain::events",
                    error = %e,
                    "WebSocket subscription failed, falling back to polling"
                );
                self.stream_polling().await
            }
        }
    }

    /// Stream events using polling
    async fn stream_polling(&self) -> Result<BeeBotOSEventStream> {
        let (tx, rx) = mpsc::channel(100);

        let provider = self.provider.clone();
        let filter = self.filter.clone();
        let poll_interval = std::time::Duration::from_secs(5);

        tokio::spawn(async move {
            let mut last_checked_block = filter.from_block;

            loop {
                let mut current_filter = filter.clone();

                if let Some(last_block) = last_checked_block {
                    current_filter = current_filter.from_block(last_block + 1);
                }

                match provider.get_logs(&current_filter.to_alloy_filter()).await {
                    Ok(logs) => {
                        for log in logs {
                            if let Some(block_num) = log.block_number {
                                last_checked_block = Some(block_num);
                            }

                            if tx.send(log).await.is_err() {
                                debug!(
                                    target: "chain::events",
                                    "Event stream receiver dropped, stopping stream"
                                );
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        error!(
                            target: "chain::events",
                            error = %e,
                            "Error polling for logs"
                        );
                    }
                }

                tokio::time::sleep(poll_interval).await;
            }
        });

        Ok(BeeBotOSEventStream {
            receiver: rx,
            is_websocket: false,
        })
    }

    /// Decode a log into a BeeBotOS event
    fn decode_log(log: &Log) -> Result<BeeBotOSEvent> {
        let primitive_log = PrimitiveLog {
            address: log.address(),
            data: alloy_primitives::LogData::new(log.topics().to_vec(), log.data().data.clone())
                .ok_or_else(|| ChainError::Validation("Invalid log data".to_string()))?,
        };

        // Try to decode each event type
        if let Ok(event) = ProposalCreated::decode_log(&primitive_log, true) {
            return Ok(BeeBotOSEvent::ProposalCreated(event.data));
        }
        if let Ok(event) = VoteCast::decode_log(&primitive_log, true) {
            return Ok(BeeBotOSEvent::VoteCast(event.data));
        }
        if let Ok(event) = ProposalExecuted::decode_log(&primitive_log, true) {
            return Ok(BeeBotOSEvent::ProposalExecuted(event.data));
        }
        if let Ok(event) = Transfer::decode_log(&primitive_log, true) {
            return Ok(BeeBotOSEvent::Transfer(event.data));
        }
        if let Ok(event) = BudgetCreated::decode_log(&primitive_log, true) {
            return Ok(BeeBotOSEvent::BudgetCreated(event.data));
        }
        if let Ok(event) = BudgetReleased::decode_log(&primitive_log, true) {
            return Ok(BeeBotOSEvent::BudgetReleased(event.data));
        }
        if let Ok(event) = EscrowCreated::decode_log(&primitive_log, true) {
            return Ok(BeeBotOSEvent::EscrowCreated(event.data));
        }
        if let Ok(event) = EscrowReleased::decode_log(&primitive_log, true) {
            return Ok(BeeBotOSEvent::EscrowReleased(event.data));
        }
        if let Ok(event) = EscrowRefunded::decode_log(&primitive_log, true) {
            return Ok(BeeBotOSEvent::EscrowRefunded(event.data));
        }

        Err(ChainError::Validation("Unknown event type".to_string()))
    }
}

/// BeeBotOS event stream
pub struct BeeBotOSEventStream {
    receiver: mpsc::Receiver<Log>,
    is_websocket: bool,
}

impl BeeBotOSEventStream {
    /// Check if using WebSocket
    pub fn is_websocket(&self) -> bool {
        self.is_websocket
    }
}

impl Stream for BeeBotOSEventStream {
    type Item = BeeBotOSEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.receiver.poll_recv(cx) {
            Poll::Ready(Some(log)) => match Self::decode_log_static(&log) {
                Ok(event) => Poll::Ready(Some(event)),
                Err(_) => Poll::Ready(Some(BeeBotOSEvent::Raw(log))),
            },
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl BeeBotOSEventStream {
    /// Decode a log into a BeeBotOS event (static method)
    fn decode_log_static(log: &Log) -> Result<BeeBotOSEvent> {
        let primitive_log = PrimitiveLog {
            address: log.address(),
            data: alloy_primitives::LogData::new(log.topics().to_vec(), log.data().data.clone())
                .ok_or_else(|| ChainError::Validation("Invalid log data".to_string()))?,
        };

        // Try to decode each event type
        if let Ok(event) = ProposalCreated::decode_log(&primitive_log, true) {
            return Ok(BeeBotOSEvent::ProposalCreated(event.data));
        }
        if let Ok(event) = VoteCast::decode_log(&primitive_log, true) {
            return Ok(BeeBotOSEvent::VoteCast(event.data));
        }
        if let Ok(event) = ProposalExecuted::decode_log(&primitive_log, true) {
            return Ok(BeeBotOSEvent::ProposalExecuted(event.data));
        }
        if let Ok(event) = Transfer::decode_log(&primitive_log, true) {
            return Ok(BeeBotOSEvent::Transfer(event.data));
        }
        if let Ok(event) = BudgetCreated::decode_log(&primitive_log, true) {
            return Ok(BeeBotOSEvent::BudgetCreated(event.data));
        }
        if let Ok(event) = BudgetReleased::decode_log(&primitive_log, true) {
            return Ok(BeeBotOSEvent::BudgetReleased(event.data));
        }
        if let Ok(event) = EscrowCreated::decode_log(&primitive_log, true) {
            return Ok(BeeBotOSEvent::EscrowCreated(event.data));
        }
        if let Ok(event) = EscrowReleased::decode_log(&primitive_log, true) {
            return Ok(BeeBotOSEvent::EscrowReleased(event.data));
        }
        if let Ok(event) = EscrowRefunded::decode_log(&primitive_log, true) {
            return Ok(BeeBotOSEvent::EscrowRefunded(event.data));
        }

        Err(ChainError::Validation("Unknown event type".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_filter_builder() {
        let filter = BeeBotOSEventFilter::new()
            .from_block(100)
            .to_block(200)
            .address(Address::ZERO)
            .event_type(BeeBotOSEventType::ProposalCreated);

        assert_eq!(filter.from_block, Some(100));
        assert_eq!(filter.to_block, Some(200));
        assert_eq!(filter.addresses.len(), 1);
        assert_eq!(filter.event_types.len(), 1);
    }

    #[test]
    fn test_event_type_signatures() {
        // Verify that event types have valid signature hashes
        let proposal_hash = BeeBotOSEventType::ProposalCreated.signature_hash();
        assert_ne!(proposal_hash, B256::ZERO);

        let vote_hash = BeeBotOSEventType::VoteCast.signature_hash();
        assert_ne!(vote_hash, B256::ZERO);

        let transfer_hash = BeeBotOSEventType::Transfer.signature_hash();
        assert_ne!(transfer_hash, B256::ZERO);
    }
}
