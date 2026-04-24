//! Event Filter Builder

use alloy_rpc_types::{Filter, FilterBlockOption};
use serde::{Deserialize, Serialize};

use crate::compat::{Address, B256};

/// Event filter builder
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EventFilter {
    from_block: Option<u64>,
    to_block: Option<u64>,
    addresses: Vec<Address>,
    topics: Vec<Option<Vec<B256>>>,
}

impl EventFilter {
    /// Create new filter
    pub fn new() -> Self {
        Self::default()
    }

    /// Set from block (inclusive)
    pub fn from_block(mut self, block: u64) -> Self {
        self.from_block = Some(block);
        self
    }

    /// Set to block (inclusive)
    pub fn to_block(mut self, block: u64) -> Self {
        self.to_block = Some(block);
        self
    }

    /// Set block range
    pub fn block_range(mut self, from: u64, to: u64) -> Self {
        self.from_block = Some(from);
        self.to_block = Some(to);
        self
    }

    /// Add contract address filter
    pub fn address(mut self, address: Address) -> Self {
        self.addresses.push(address);
        self
    }

    /// Add multiple addresses
    pub fn addresses(mut self, addresses: Vec<Address>) -> Self {
        self.addresses.extend(addresses);
        self
    }

    /// Add topic0 filter (event signature)
    pub fn event_signature(mut self, signature: B256) -> Self {
        self.topics.push(Some(vec![signature]));
        self
    }

    /// Add topic filter
    pub fn topic(mut self, topic: B256) -> Self {
        self.topics.push(Some(vec![topic]));
        self
    }

    /// Add topic at specific position
    pub fn topic_at(mut self, index: usize, topic: B256) -> Self {
        // Ensure topics vector has enough capacity
        while self.topics.len() <= index {
            self.topics.push(None);
        }
        self.topics[index] = Some(vec![topic]);
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

        // Set topics
        for (i, topic) in self.topics.iter().enumerate().take(4) {
            if let Some(t) = topic {
                filter.topics[i] = alloy_rpc_types::FilterSet::from(t.clone());
            }
        }

        filter
    }

    /// Check if filter is empty
    pub fn is_empty(&self) -> bool {
        self.from_block.is_none()
            && self.to_block.is_none()
            && self.addresses.is_empty()
            && self.topics.is_empty()
    }

    /// Get from block
    pub fn get_from_block(&self) -> Option<u64> {
        self.from_block
    }

    /// Get to block
    pub fn get_to_block(&self) -> Option<u64> {
        self.to_block
    }

    /// Get addresses
    pub fn get_addresses(&self) -> &[Address] {
        &self.addresses
    }
}

/// Filter builder for common patterns
pub struct CommonFilters;

impl CommonFilters {
    /// Filter for specific event from specific contract
    pub fn event_from_contract(address: Address, event_signature: B256) -> EventFilter {
        EventFilter::new()
            .address(address)
            .event_signature(event_signature)
    }

    /// Filter for recent blocks
    pub fn recent_blocks(blocks: u64, current_block: u64) -> EventFilter {
        EventFilter::new()
            .from_block(current_block.saturating_sub(blocks))
            .to_block(current_block)
    }

    /// Filter for transfer events (ERC20/ERC721)
    pub fn transfers(token_address: Address) -> EventFilter {
        // Transfer(address,address,uint256)
        let signature = alloy_primitives::keccak256(b"Transfer(address,address,uint256)");
        EventFilter::new()
            .address(token_address)
            .event_signature(signature.into())
    }

    /// Filter for approval events (ERC20/ERC721)
    pub fn approvals(token_address: Address) -> EventFilter {
        // Approval(address,address,uint256)
        let signature = alloy_primitives::keccak256(b"Approval(address,address,uint256)");
        EventFilter::new()
            .address(token_address)
            .event_signature(signature.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_builder() {
        let addr = Address::ZERO;
        let sig = B256::ZERO;

        let filter = EventFilter::new()
            .from_block(100)
            .to_block(200)
            .address(addr)
            .event_signature(sig);

        assert_eq!(filter.get_from_block(), Some(100));
        assert_eq!(filter.get_to_block(), Some(200));
        assert_eq!(filter.get_addresses(), &[addr]);
    }

    #[test]
    fn test_common_filters() {
        let addr = Address::ZERO;
        let filter = CommonFilters::transfers(addr);

        assert_eq!(filter.get_addresses(), &[addr]);
        assert!(!filter.is_empty());
    }
}
