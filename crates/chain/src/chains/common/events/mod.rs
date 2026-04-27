//! Generic Event System for EVM Chains
//!
//! Provides chain-agnostic event filtering, streaming, and processing.

pub mod filter;
pub mod listener;
pub mod manager;
pub mod processor;
pub mod stream;

use std::fmt::Debug;

pub use filter::EventFilter;
pub use listener::{EventListener, MultiListener, SubscriptionType};
pub use manager::{
    EventManager, EventRouter, MultiChainEventManager, Subscription, SubscriptionConfig,
    SubscriptionId,
};
// Note: SubscriptionConfig is defined in manager.rs, not here
pub use processor::{EventHandler, EventProcessor};
use serde::{Deserialize, Serialize};
pub use stream::{BatchedEventStream, EventStream, EventStreamConfig, FilteredEventStream};

use crate::compat::{Address, B256};

/// Generic event for EVM chains
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmEvent {
    /// Contract address that emitted the event
    pub address: Address,
    /// Event topics (indexed parameters)
    pub topics: Vec<B256>,
    /// Event data (non-indexed parameters)
    pub data: Vec<u8>,
    /// Block number where event was emitted
    pub block_number: u64,
    /// Block hash
    pub block_hash: B256,
    /// Transaction hash
    pub transaction_hash: B256,
    /// Log index within block
    pub log_index: u64,
    /// Transaction index within block
    pub transaction_index: u64,
    /// Whether this log was removed (reorg)
    pub removed: bool,
}

impl EvmEvent {
    /// Get topic at index (topic0 is event signature)
    pub fn topic(&self, index: usize) -> Option<&B256> {
        self.topics.get(index)
    }

    /// Get event signature (topic0)
    pub fn signature(&self) -> Option<&B256> {
        self.topics.first()
    }

    /// Check if event matches a specific signature
    pub fn matches_signature(&self, signature: &B256) -> bool {
        self.signature() == Some(signature)
    }

    /// Decode event data using SolEvent type
    pub fn decode<D: alloy_sol_types::SolEvent>(&self) -> Result<D, alloy_sol_types::Error> {
        let log =
            alloy_primitives::Log::new(self.address, self.topics.clone(), self.data.clone().into())
                .ok_or_else(|| alloy_sol_types::Error::Other("Invalid log".into()))?;

        D::decode_raw_log(log.topics(), log.data.data.as_ref(), true)
    }
}

impl From<alloy_rpc_types::Log> for EvmEvent {
    fn from(log: alloy_rpc_types::Log) -> Self {
        Self {
            address: log.address(),
            topics: log.topics().to_vec(),
            data: log.data().data.to_vec(),
            block_number: log.block_number.unwrap_or_default(),
            block_hash: log.block_hash.unwrap_or_default(),
            transaction_hash: log.transaction_hash.unwrap_or_default(),
            log_index: log.log_index.unwrap_or_default(),
            transaction_index: log.transaction_index.unwrap_or_default(),
            removed: log.removed,
        }
    }
}

/// Event callback trait
#[async_trait::async_trait]
pub trait EventCallback: Send + Sync {
    async fn on_event(&self, event: EvmEvent);
    async fn on_error(&self, error: crate::ChainError);
}

/// Event filter options
#[derive(Debug, Clone)]
pub struct FilterOptions {
    /// From block (inclusive)
    pub from_block: Option<u64>,
    /// To block (inclusive)
    pub to_block: Option<u64>,
    /// Contract addresses to filter
    pub addresses: Vec<Address>,
    /// Event signatures to filter (topic0)
    pub event_signatures: Vec<B256>,
    /// Additional topic filters
    pub topics: Vec<Option<Vec<B256>>>,
    /// Maximum number of blocks to query at once
    pub max_block_range: Option<u64>,
}

impl Default for FilterOptions {
    fn default() -> Self {
        Self {
            from_block: None,
            to_block: None,
            addresses: Vec::new(),
            event_signatures: Vec::new(),
            topics: Vec::new(),
            max_block_range: Some(2000),
        }
    }
}

/// Event metrics
#[derive(Debug, Clone, Default)]
pub struct EventMetrics {
    /// Total events received
    pub events_received: u64,
    /// Total events processed
    pub events_processed: u64,
    /// Total errors
    pub errors: u64,
    /// Average processing time (ms)
    pub avg_processing_time_ms: f64,
    /// Last event timestamp
    pub last_event_at: Option<std::time::SystemTime>,
}

/// Event processor configuration
#[derive(Debug, Clone)]
pub struct ProcessorConfig {
    /// Number of worker threads
    pub worker_count: usize,
    /// Channel buffer size
    pub channel_buffer: usize,
    /// Maximum events per second
    pub rate_limit: Option<u64>,
    /// Enable metrics collection
    pub enable_metrics: bool,
}

impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            worker_count: 4,
            channel_buffer: 1000,
            rate_limit: None,
            enable_metrics: true,
        }
    }
}

/// Utility functions
pub mod utils {
    use super::*;

    /// Compute event signature hash
    pub fn event_signature(event_name: &str) -> B256 {
        alloy_primitives::keccak256(event_name.as_bytes())
    }

    /// Create topic filter for event
    pub fn topic_filter(sig: &str) -> B256 {
        event_signature(sig)
    }

    /// Decode indexed parameter (topic)
    pub fn decode_indexed<T: alloy_sol_types::SolType>(
        topic: &B256,
    ) -> Result<T::RustType, alloy_sol_types::Error> {
        T::abi_decode(topic.as_ref(), true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_signature() {
        let sig = utils::event_signature("Transfer(address,address,uint256)");
        assert_eq!(sig.len(), 32);
    }
}
