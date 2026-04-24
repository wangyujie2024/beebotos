//! Ethereum-specific types

use alloy_consensus::Transaction as _;
use alloy_primitives::U256;
use alloy_rpc_types::{Block, Log, Transaction, TransactionReceipt};
use serde::{Deserialize, Serialize};

use crate::chains::common::token::{chain_formatters, parse_native_amount};

/// Ethereum Block representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthereumBlock {
    pub number: u64,
    pub hash: String,
    pub parent_hash: String,
    pub timestamp: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub transactions: Vec<String>,
    pub validator: String,
    /// Base fee per gas (EIP-1559)
    pub base_fee_per_gas: Option<u64>,
    /// Withdrawals root (post-Shanghai)
    pub withdrawals_root: Option<String>,
    /// Blob gas used (post-Dencun)
    pub blob_gas_used: Option<u64>,
    /// Excess blob gas (post-Dencun)
    pub excess_blob_gas: Option<u64>,
}

impl From<Block> for EthereumBlock {
    fn from(block: Block) -> Self {
        Self {
            number: block.header.number,
            hash: format!("{:?}", block.header.hash),
            parent_hash: format!("{:?}", block.header.parent_hash),
            timestamp: block.header.timestamp,
            gas_limit: block.header.gas_limit,
            gas_used: block.header.gas_used,
            transactions: block
                .transactions
                .hashes()
                .map(|h| format!("{:?}", h))
                .collect(),
            validator: format!("{:?}", block.header.beneficiary),
            base_fee_per_gas: block.header.base_fee_per_gas.map(|f| f as u64),
            withdrawals_root: block.header.withdrawals_root.map(|h| format!("{:?}", h)),
            blob_gas_used: block.header.blob_gas_used.map(|g| g as u64),
            excess_blob_gas: block.header.excess_blob_gas.map(|g| g as u64),
        }
    }
}

/// Ethereum Transaction representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthereumTransaction {
    pub hash: String,
    pub from: String,
    pub to: Option<String>,
    pub value: String,
    pub gas_price: Option<String>,
    pub max_fee_per_gas: Option<String>,
    pub max_priority_fee_per_gas: Option<String>,
    pub gas_limit: u64,
    pub nonce: u64,
    pub data: String,
    pub status: Option<bool>,
    pub block_number: Option<u64>,
    pub block_hash: Option<String>,
    pub gas_used: Option<u64>,
    pub effective_gas_price: Option<String>,
    /// Transaction type (0=legacy, 1=EIP-2930, 2=EIP-1559, 3=EIP-4844)
    pub tx_type: u8,
    /// Access list (EIP-2930)
    pub access_list: Option<Vec<AccessListEntry>>,
    /// Blob versioned hashes (EIP-4844)
    pub blob_versioned_hashes: Option<Vec<String>>,
}

impl From<Transaction> for EthereumTransaction {
    fn from(tx: Transaction) -> Self {
        Self {
            hash: format!("{:?}", tx.inner.tx_hash()),
            from: format!("{:?}", tx.from),
            to: tx.to().map(|a| format!("{:?}", a)),
            value: tx.value().to_string(),
            gas_price: tx.gas_price().map(|p| p.to_string()),
            max_fee_per_gas: None, // Would need to extract from inner transaction
            max_priority_fee_per_gas: None,
            gas_limit: tx.gas_limit(),
            nonce: tx.nonce(),
            data: format!("0x{}", hex::encode(tx.input().as_ref())),
            status: None,
            block_number: tx.block_number,
            block_hash: tx.block_hash.map(|h| format!("{:?}", h)),
            gas_used: None,
            effective_gas_price: None,
            tx_type: tx.inner.tx_type() as u8,
            access_list: None,
            blob_versioned_hashes: None,
        }
    }
}

impl From<TransactionReceipt> for EthereumTransaction {
    fn from(receipt: TransactionReceipt) -> Self {
        Self {
            hash: format!("{:?}", receipt.transaction_hash),
            from: format!("{:?}", receipt.from),
            to: receipt.to.map(|a| format!("{:?}", a)),
            value: "0".to_string(),
            gas_price: None,
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
            gas_limit: 0,
            nonce: 0,
            data: String::new(),
            status: Some(receipt.status()),
            block_number: receipt.block_number,
            block_hash: receipt.block_hash.map(|h| format!("{:?}", h)),
            gas_used: Some(receipt.gas_used as u64),
            effective_gas_price: Some(receipt.effective_gas_price.to_string()),
            tx_type: receipt.transaction_type() as u8,
            access_list: None,
            blob_versioned_hashes: None,
        }
    }
}

/// Access list entry for EIP-2930
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessListEntry {
    pub address: String,
    pub storage_keys: Vec<String>,
}

/// Ethereum Event representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthereumEvent {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub block_number: u64,
    pub transaction_hash: String,
    pub log_index: u64,
    /// Whether this event is from a finalized block
    pub finalized: bool,
    /// Removed flag (for reorg detection)
    pub removed: bool,
}

impl From<Log> for EthereumEvent {
    fn from(log: Log) -> Self {
        Self {
            address: format!("{:?}", log.address()),
            topics: log.topics().iter().map(|t| format!("{:?}", t)).collect(),
            data: format!("0x{}", hex::encode(log.data().data.clone().as_ref())),
            block_number: log.block_number.unwrap_or_default(),
            transaction_hash: format!("{:?}", log.transaction_hash.unwrap_or_default()),
            log_index: log.log_index.unwrap_or_default(),
            finalized: false,
            removed: log.removed,
        }
    }
}

/// ERC-20 Token information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Erc20Token {
    pub address: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: String,
}

/// Validator information (Beacon Chain)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeaconValidator {
    pub validator_index: u64,
    pub pubkey: String,
    pub withdrawal_credentials: String,
    pub effective_balance: u64, // in Gwei
    pub slashed: bool,
    pub activation_epoch: u64,
    pub exit_epoch: u64,
    pub status: ValidatorStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidatorStatus {
    PendingInitialized,
    PendingQueued,
    ActiveOngoing,
    ActiveExiting,
    ActiveSlashed,
    ExitedUnslashed,
    ExitedSlashed,
    WithdrawalPossible,
    WithdrawalDone,
}

/// Format ETH amount (18 decimals)
///
/// DEPRECATED: Use `chains::common::token::chain_formatters::format_eth`
/// instead
pub fn format_eth(wei: U256) -> String {
    chain_formatters::format_eth(wei)
}

/// Parse ETH string to wei
///
/// DEPRECATED: Use `chains::common::token::parse_native_amount` instead
pub fn parse_eth(eth: &str) -> Option<U256> {
    parse_native_amount(eth)
}

/// Transaction priority strategies (EIP-1559)
///
/// Re-export from common module for backward compatibility
pub use crate::chains::common::token::EthereumPriority;

/// Block tag for Ethereum RPC calls
#[derive(Debug, Clone, Copy)]
pub enum BlockTag {
    Latest,
    Safe,
    Finalized,
    Pending,
    Earliest,
    Number(u64),
}

impl BlockTag {
    pub fn as_string(&self) -> String {
        match self {
            BlockTag::Latest => "latest".to_string(),
            BlockTag::Safe => "safe".to_string(),
            BlockTag::Finalized => "finalized".to_string(),
            BlockTag::Pending => "pending".to_string(),
            BlockTag::Earliest => "earliest".to_string(),
            BlockTag::Number(n) => format!("0x{:x}", n),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_eth() {
        let wei = U256::from(1_500_000_000_000_000_000u64); // 1.5 ETH
        let eth = format_eth(wei);
        assert!(eth.starts_with("1.5"));
    }

    #[test]
    fn test_parse_eth() {
        let eth = "1.5";
        let wei = parse_eth(eth).unwrap();
        let expected = U256::from(1_500_000_000_000_000_000u64);
        assert_eq!(wei, expected);
    }

    #[test]
    fn test_block_tag() {
        assert_eq!(BlockTag::Latest.as_string(), "latest");
        assert_eq!(BlockTag::Number(100).as_string(), "0x64");
    }

    #[test]
    fn test_ethereum_priority() {
        assert_eq!(EthereumPriority::Urgent.priority_fee_gwei(), 5);
        assert_eq!(EthereumPriority::Slow.priority_fee_gwei(), 0);
    }
}
