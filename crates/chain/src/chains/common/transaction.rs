//! Generic Transaction Builder for EVM Chains
//!
//! Provides chain-agnostic transaction construction.

use std::collections::HashMap;

use alloy_rpc_types::TransactionRequest;

use crate::chains::common::TransactionPriority;
use crate::compat::{Address, U256};

/// Transaction builder with fluent API
#[derive(Debug, Clone)]
pub struct TransactionBuilder {
    tx: TransactionRequest,
}

impl TransactionBuilder {
    /// Create new transaction builder
    pub fn new() -> Self {
        Self {
            tx: TransactionRequest::default(),
        }
    }

    /// Set transaction recipient
    pub fn to(mut self, addr: Address) -> Self {
        self.tx.to = Some(addr.into());
        self
    }

    /// Set transaction value
    pub fn value(mut self, value: U256) -> Self {
        self.tx.value = Some(value);
        self
    }

    /// Set transaction data/input
    pub fn data(mut self, data: Vec<u8>) -> Self {
        self.tx.input = data.into();
        self
    }

    /// Set gas limit
    pub fn gas_limit(mut self, gas: u64) -> Self {
        self.tx.gas = Some(gas);
        self
    }

    /// Set gas price (legacy transactions)
    pub fn gas_price(mut self, price: u128) -> Self {
        self.tx.gas_price = Some(price);
        self
    }

    /// Set max fee per gas (EIP-1559)
    pub fn max_fee_per_gas(mut self, max_fee: u128) -> Self {
        self.tx.max_fee_per_gas = Some(max_fee);
        self
    }

    /// Set max priority fee per gas (EIP-1559)
    pub fn max_priority_fee_per_gas(mut self, priority_fee: u128) -> Self {
        self.tx.max_priority_fee_per_gas = Some(priority_fee);
        self
    }

    /// Set nonce
    pub fn nonce(mut self, nonce: u64) -> Self {
        self.tx.nonce = Some(nonce);
        self
    }

    /// Set chain ID
    pub fn chain_id(mut self, chain_id: u64) -> Self {
        self.tx.chain_id = Some(chain_id);
        self
    }

    /// Set transaction type (0=legacy, 1=EIP-2930, 2=EIP-1559, 3=EIP-4844)
    pub fn tx_type(mut self, tx_type: u8) -> Self {
        self.tx.transaction_type = Some(tx_type);
        self
    }

    /// Set access list (EIP-2930)
    pub fn access_list(mut self, access_list: alloy_rpc_types::AccessList) -> Self {
        self.tx.access_list = Some(access_list);
        self
    }

    /// Set blob versioned hashes (EIP-4844)
    pub fn blob_versioned_hashes(mut self, hashes: Vec<[u8; 32]>) -> Self {
        self.tx.blob_versioned_hashes = Some(hashes.into_iter().map(Into::into).collect());
        self
    }

    /// Build the transaction request
    pub fn build(self) -> TransactionRequest {
        self.tx
    }

    /// Build with priority level (auto-adjusts gas prices)
    pub fn build_with_priority(
        self,
        priority: TransactionPriority,
        base_gas_price: u128,
    ) -> TransactionRequest {
        let mut tx = self.tx;

        // Apply priority multiplier to gas price
        let multiplier = priority.multiplier();
        let adjusted_gas_price = (base_gas_price as f64 * multiplier) as u128;

        tx.gas_price = Some(adjusted_gas_price);

        // For EIP-1559 transactions
        if tx.max_fee_per_gas.is_some() {
            tx.max_fee_per_gas = Some(adjusted_gas_price);
            tx.max_priority_fee_per_gas =
                Some((priority.priority_fee_gwei() as u128) * 1_000_000_000);
        }

        tx
    }
}

impl Default for TransactionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Access list item for EIP-2930
#[derive(Debug, Clone)]
pub struct AccessListItem {
    pub address: Address,
    pub storage_keys: Vec<[u8; 32]>,
}

impl From<AccessListItem> for alloy_rpc_types::AccessListItem {
    fn from(item: AccessListItem) -> Self {
        Self {
            address: item.address,
            storage_keys: item.storage_keys.into_iter().map(Into::into).collect(),
        }
    }
}

/// Transaction monitor for tracking pending transactions
pub struct TransactionMonitor {
    tracked: HashMap<String, TransactionStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionStatus {
    Pending,
    Mined,
    Confirmed { confirmations: u64 },
    Failed { error: String },
    Dropped,
}

impl TransactionMonitor {
    pub fn new() -> Self {
        Self {
            tracked: HashMap::new(),
        }
    }

    /// Start tracking a transaction
    pub fn track(&mut self, tx_hash: &str) {
        self.tracked
            .insert(tx_hash.to_string(), TransactionStatus::Pending);
    }

    /// Update transaction status
    pub fn update(&mut self, tx_hash: &str, status: TransactionStatus) {
        self.tracked.insert(tx_hash.to_string(), status);
    }

    /// Get transaction status
    pub fn status(&self, tx_hash: &str) -> Option<TransactionStatus> {
        self.tracked.get(tx_hash).cloned()
    }

    /// Remove from tracking
    pub fn untrack(&mut self, tx_hash: &str) {
        self.tracked.remove(tx_hash);
    }

    /// Get all pending transactions
    pub fn pending(&self) -> Vec<String> {
        self.tracked
            .iter()
            .filter(|(_, status)| matches!(status, TransactionStatus::Pending))
            .map(|(hash, _)| hash.clone())
            .collect()
    }
}

impl Default for TransactionMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Transaction batch builder for multicall
pub struct BatchTransactionBuilder {
    txs: Vec<TransactionRequest>,
}

impl BatchTransactionBuilder {
    pub fn new() -> Self {
        Self { txs: Vec::new() }
    }

    pub fn add(mut self, tx: TransactionRequest) -> Self {
        self.txs.push(tx);
        self
    }

    pub fn build(self) -> Vec<TransactionRequest> {
        self.txs
    }

    pub fn len(&self) -> usize {
        self.txs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.txs.is_empty()
    }
}

impl Default for BatchTransactionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper functions for transaction processing
pub mod utils {
    use super::*;

    /// Estimate intrinsic gas cost for a transaction
    pub fn estimate_intrinsic_gas(data: &[u8], is_contract_creation: bool) -> u64 {
        let base_cost = if is_contract_creation { 53_000 } else { 21_000 };
        let data_cost: u64 = data.iter().map(|b| if *b == 0 { 4 } else { 16 }).sum();
        base_cost + data_cost
    }

    /// Check if transaction is a contract creation
    pub fn is_contract_creation(tx: &TransactionRequest) -> bool {
        tx.to.is_none()
    }

    /// Calculate effective gas price (works for both legacy and EIP-1559)
    pub fn effective_gas_price(tx: &TransactionRequest, base_fee: u128) -> u128 {
        if let Some(max_fee) = tx.max_fee_per_gas {
            let priority_fee = tx.max_priority_fee_per_gas.unwrap_or(0);
            // min(max_fee, base_fee + priority_fee)
            max_fee.min(base_fee.saturating_add(priority_fee))
        } else if let Some(gas_price) = tx.gas_price {
            gas_price
        } else {
            base_fee
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_builder() {
        let to = Address::ZERO;
        let value = U256::from(1000);
        let data = vec![0x12, 0x34];

        let tx = TransactionBuilder::new()
            .to(to)
            .value(value)
            .data(data.clone())
            .gas_limit(100_000)
            .nonce(5)
            .chain_id(1)
            .build();

        assert_eq!(tx.to, Some(to.into()));
        assert_eq!(tx.value, Some(value));
        assert_eq!(tx.input.input.as_ref().unwrap().as_ref(), data.as_slice());
        assert_eq!(tx.gas, Some(100_000));
        assert_eq!(tx.nonce, Some(5));
        assert_eq!(tx.chain_id, Some(1));
    }

    #[test]
    fn test_transaction_priority() {
        let tx = TransactionBuilder::new()
            .to(Address::ZERO)
            .gas_price(10_000_000_000) // 10 gwei
            .build_with_priority(TransactionPriority::High, 10_000_000_000);

        // High priority should multiply gas price by 1.2
        assert!(tx.gas_price.unwrap() >= 12_000_000_000);
    }

    #[test]
    fn test_estimate_intrinsic_gas() {
        let data = vec![0x00, 0x01, 0x02];
        let gas = utils::estimate_intrinsic_gas(&data, false);
        // 21000 + 4 + 16 + 16 = 21036
        assert_eq!(gas, 21_036);
    }
}
