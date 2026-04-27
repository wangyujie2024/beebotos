//! Chain Events Module
//!
//! Provides event listening and subscription for blockchain events.
//! Used to track transaction confirmations and contract events.

use std::collections::HashMap;
use std::sync::Arc;

use beebotos_chain::compat::ChainClientTrait;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, instrument, warn};

use super::chain_event_parser::{ChainEventParser, ParsedEvent};

/// Event subscription manager
#[allow(dead_code)]
pub struct ChainEventManager {
    /// Chain client
    client: Arc<dyn ChainClientTrait>,
    /// Active subscriptions
    subscriptions: Arc<RwLock<HashMap<String, EventSubscription>>>,
    /// Event broadcaster
    event_tx: broadcast::Sender<ChainEvent>,
    /// Event parser for parsing transaction receipts
    event_parser: Option<ChainEventParser>,
}

/// Chain event types
#[derive(Debug, Clone)]
pub enum ChainEvent {
    /// Identity registration event
    IdentityRegistered {
        tx_hash: String,
        agent_id: String,
        did: String,
        owner: String,
        block_number: u64,
        confirmed: bool,
    },
    /// DAO proposal created event
    ProposalCreated {
        tx_hash: String,
        proposal_id: u64,
        proposer: String,
        description: String,
        block_number: u64,
    },
    /// Vote cast event
    VoteCast {
        tx_hash: String,
        proposal_id: u64,
        voter: String,
        support: u8,
        weight: String,
        block_number: u64,
    },
    /// Transaction confirmation event
    TransactionConfirmed {
        tx_hash: String,
        block_number: u64,
        gas_used: u64,
        success: bool,
    },
    /// Transaction pending (submitted but not yet confirmed)
    TransactionPending { tx_hash: String, timestamp: u64 },
}

/// Event subscription
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct EventSubscription {
    pub id: String,
    pub event_type: EventType,
    pub filters: EventFilters,
}

/// Event types for subscription
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum EventType {
    IdentityRegistered,
    ProposalCreated,
    VoteCast,
    TransactionConfirmed,
    All,
}

/// Event filters
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct EventFilters {
    pub agent_id: Option<String>,
    pub did: Option<String>,
    pub proposal_id: Option<u64>,
    pub from_block: Option<u64>,
    pub to_block: Option<u64>,
}

/// Transaction confirmation tracker
#[allow(dead_code)]
pub struct ConfirmationTracker {
    /// Required confirmation blocks
    required_confirmations: u64,
    /// Tracked transactions
    pending_txs: Arc<RwLock<HashMap<String, PendingTransaction>>>,
    /// Chain client
    client: Arc<dyn ChainClientTrait>,
}

/// Pending transaction info
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PendingTransaction {
    pub tx_hash: String,
    pub submitted_at: std::time::Instant,
    pub block_number: Option<u64>,
    pub confirmations: u64,
    pub status: PendingTxStatus,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum PendingTxStatus {
    Submitted,
    Mined,
    Confirmed,
    Failed(String),
}

impl ChainEventManager {
    /// Create new event manager
    pub fn new(client: Arc<dyn ChainClientTrait>) -> Self {
        let (event_tx, _) = broadcast::channel(1000);

        Self {
            client,
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            event_parser: None,
        }
    }

    /// Create with event parser
    #[allow(dead_code)]
    pub fn with_parser(client: Arc<dyn ChainClientTrait>, parser: ChainEventParser) -> Self {
        let (event_tx, _) = broadcast::channel(1000);

        Self {
            client,
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            event_parser: Some(parser),
        }
    }

    /// Set event parser
    #[allow(dead_code)]
    pub fn set_event_parser(&mut self, parser: ChainEventParser) {
        self.event_parser = Some(parser);
    }

    /// Subscribe to events
    pub fn subscribe(&self) -> broadcast::Receiver<ChainEvent> {
        self.event_tx.subscribe()
    }

    /// Add event subscription
    #[allow(dead_code)]
    pub async fn add_subscription(&self, subscription: EventSubscription) {
        let subscription_id = subscription.id.clone();
        let mut subs = self.subscriptions.write().await;
        subs.insert(subscription_id.clone(), subscription);
        debug!(subscription_id = %subscription_id, "Added event subscription");
    }

    /// Remove event subscription
    #[allow(dead_code)]
    pub async fn remove_subscription(&self, subscription_id: &str) {
        let mut subs = self.subscriptions.write().await;
        subs.remove(subscription_id);
        debug!(subscription_id = %subscription_id, "Removed event subscription");
    }

    /// Publish event to all subscribers
    pub fn publish_event(&self, event: ChainEvent) {
        let _ = self.event_tx.send(event);
    }

    /// Start event monitoring loop
    pub fn start_monitoring(self: Arc<Self>) {
        tokio::spawn(async move {
            info!("Starting chain event monitoring loop");

            loop {
                // Poll for new events
                if let Err(e) = self.poll_events().await {
                    error!(error = %e, "Error polling events");
                }

                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        });
    }

    /// Poll for new events
    async fn poll_events(&self) -> anyhow::Result<()> {
        // Get current block number
        let current_block = self
            .client
            .get_block_number()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get block number: {}", e))?;

        debug!(current_block = %current_block, "Polling for events");

        // TODO: Query events from chain using filters
        // This would query the contract event logs

        Ok(())
    }

    /// Track a transaction for confirmation and parse events
    #[instrument(skip(self), fields(tx_hash = %tx_hash))]
    pub async fn track_transaction(&self, tx_hash: &str) {
        info!("Starting transaction tracking");

        // Publish pending event
        self.publish_event(ChainEvent::TransactionPending {
            tx_hash: tx_hash.to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        });

        // Start confirmation tracking with event parsing
        let tx_hash_bytes: [u8; 32] = match hex::decode(tx_hash.trim_start_matches("0x")) {
            Ok(bytes) if bytes.len() == 32 => {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&bytes);
                arr
            }
            _ => {
                error!("Invalid transaction hash length");
                return;
            }
        };

        let has_parser = self.event_parser.is_some();
        let event_parser = self.event_parser.clone();

        let client = Arc::clone(&self.client);
        let event_tx = self.event_tx.clone();
        let tx_hash = tx_hash.to_string();

        tokio::spawn(async move {
            track_confirmation_with_parsing(
                client,
                event_tx,
                tx_hash,
                tx_hash_bytes,
                has_parser.then(|| event_parser).flatten(),
            )
            .await;
        });
    }
}

/// Track transaction confirmation with event parsing
async fn track_confirmation_with_parsing(
    client: Arc<dyn ChainClientTrait>,
    event_tx: broadcast::Sender<ChainEvent>,
    tx_hash: String,
    tx_hash_bytes: [u8; 32],
    event_parser: Option<ChainEventParser>,
) {
    let mut checked_blocks = 0u64;
    let max_wait_blocks = 100u64; // Stop after ~20 minutes (100 * 12s blocks)

    loop {
        match client
            .get_transaction_receipt(beebotos_chain::compat::TxHash::from_slice(&tx_hash_bytes))
            .await
        {
            Ok(Some(receipt)) => {
                // Parse events from receipt if parser is available
                if let Some(ref parser) = event_parser {
                    let events = parser.parse_receipt(&receipt, &tx_hash);
                    for event in events {
                        convert_and_emit_event(&event_tx, event, &tx_hash, receipt.block_number);
                    }
                }

                let confirmed = ChainEvent::TransactionConfirmed {
                    tx_hash: tx_hash.clone(),
                    block_number: receipt.block_number,
                    gas_used: receipt.gas_used,
                    success: receipt.status,
                };

                let _ = event_tx.send(confirmed);

                info!(
                    tx_hash = %tx_hash,
                    block_number = %receipt.block_number,
                    success = %receipt.status,
                    "Transaction confirmed"
                );

                return;
            }
            Ok(None) => {
                // Transaction not yet mined
                checked_blocks += 1;

                if checked_blocks >= max_wait_blocks {
                    warn!(tx_hash = %tx_hash, "Transaction confirmation timeout");
                    return;
                }

                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
            Err(e) => {
                error!(tx_hash = %tx_hash, error = %e, "Error checking transaction receipt");
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        }
    }
}

/// Convert ParsedEvent to ChainEvent and emit
fn convert_and_emit_event(
    event_tx: &broadcast::Sender<ChainEvent>,
    event: ParsedEvent,
    tx_hash: &str,
    block_number: u64,
) {
    let chain_event = match event {
        ParsedEvent::IdentityRegistered(e) => Some(ChainEvent::IdentityRegistered {
            tx_hash: tx_hash.to_string(),
            agent_id: e.agent_id,
            did: e.did,
            owner: e.owner,
            block_number,
            confirmed: true,
        }),
        ParsedEvent::ProposalCreated(e) => Some(ChainEvent::ProposalCreated {
            tx_hash: tx_hash.to_string(),
            proposal_id: e.proposal_id,
            proposer: e.proposer,
            description: e.description,
            block_number,
        }),
        ParsedEvent::VoteCast(e) => Some(ChainEvent::VoteCast {
            tx_hash: tx_hash.to_string(),
            proposal_id: e.proposal_id,
            voter: e.voter,
            support: e.support,
            weight: e.weight,
            block_number,
        }),
        _ => None,
    };

    if let Some(e) = chain_event {
        let _ = event_tx.send(e);
    }
}

#[allow(dead_code)]
impl ConfirmationTracker {
    /// Create new confirmation tracker
    pub fn new(client: Arc<dyn ChainClientTrait>, required_confirmations: u64) -> Self {
        Self {
            required_confirmations,
            pending_txs: Arc::new(RwLock::new(HashMap::new())),
            client,
        }
    }

    /// Add transaction to track
    #[allow(dead_code)]
    pub async fn track(&self, tx_hash: String) {
        let pending = PendingTransaction {
            tx_hash: tx_hash.clone(),
            submitted_at: std::time::Instant::now(),
            block_number: None,
            confirmations: 0,
            status: PendingTxStatus::Submitted,
        };

        let mut txs = self.pending_txs.write().await;
        txs.insert(tx_hash, pending);
    }

    /// Get pending transaction status
    #[allow(dead_code)]
    pub async fn get_status(&self, tx_hash: &str) -> Option<PendingTransaction> {
        let txs = self.pending_txs.read().await;
        txs.get(tx_hash).cloned()
    }

    /// Start confirmation monitoring
    #[allow(dead_code)]
    pub fn start_monitoring(self: Arc<Self>) {
        tokio::spawn(async move {
            info!("Starting confirmation tracker");

            loop {
                self.check_confirmations().await;
                tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
            }
        });
    }

    /// Check confirmations for all pending transactions
    #[allow(dead_code)]
    async fn check_confirmations(&self) {
        let current_block = match self.client.get_block_number().await {
            Ok(block) => block,
            Err(e) => {
                error!(error = %e, "Failed to get current block");
                return;
            }
        };

        let mut txs = self.pending_txs.write().await;
        let mut to_remove = Vec::new();

        for (tx_hash, pending) in txs.iter_mut() {
            if pending.status == PendingTxStatus::Confirmed {
                to_remove.push(tx_hash.clone());
                continue;
            }

            // Check if transaction is mined
            if let Some(mined_block) = pending.block_number {
                let confirmations = current_block - mined_block;
                pending.confirmations = confirmations;

                if confirmations >= self.required_confirmations {
                    pending.status = PendingTxStatus::Confirmed;
                    info!(
                        tx_hash = %tx_hash,
                        confirmations = %confirmations,
                        "Transaction fully confirmed"
                    );
                    to_remove.push(tx_hash.clone());
                }
            }
        }

        // Remove confirmed transactions
        for tx_hash in to_remove {
            txs.remove(&tx_hash);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_subscription() {
        let subscription = EventSubscription {
            id: "test-1".to_string(),
            event_type: EventType::IdentityRegistered,
            filters: EventFilters::default(),
        };

        assert_eq!(subscription.event_type, EventType::IdentityRegistered);
    }

    #[test]
    fn test_pending_transaction() {
        let pending = PendingTransaction {
            tx_hash: "0x123".to_string(),
            submitted_at: std::time::Instant::now(),
            block_number: Some(100),
            confirmations: 5,
            status: PendingTxStatus::Mined,
        };

        assert_eq!(pending.confirmations, 5);
        assert_eq!(pending.status, PendingTxStatus::Mined);
    }
}
