use alloy_consensus::Transaction as _;
use alloy_primitives::U256;
use alloy_rpc_types::{Block, Log, Transaction, TransactionReceipt};
use serde::{Deserialize, Serialize};

use crate::chains::common::token::{chain_formatters, parse_native_amount};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonadBlock {
    pub number: u64,
    pub hash: String,
    pub parent_hash: String,
    pub timestamp: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub transactions: Vec<String>,
    pub validator: String,
}

impl From<Block> for MonadBlock {
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
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonadTransaction {
    pub hash: String,
    pub from: String,
    pub to: Option<String>,
    pub value: String,
    pub gas_price: Option<String>,
    pub gas_limit: u64,
    pub nonce: u64,
    pub data: String,
    pub status: Option<bool>,
    pub block_number: Option<u64>,
    pub block_hash: Option<String>,
    pub gas_used: Option<u64>,
    pub effective_gas_price: Option<String>,
}

impl From<Transaction> for MonadTransaction {
    fn from(tx: Transaction) -> Self {
        Self {
            hash: format!("{:?}", tx.inner.tx_hash()),
            from: format!("{:?}", tx.from),
            to: tx.to().map(|a| format!("{:?}", a)),
            value: tx.value().to_string(),
            gas_price: tx.gas_price().map(|p| p.to_string()),
            gas_limit: tx.gas_limit(),
            nonce: tx.nonce(),
            data: format!("0x{}", hex::encode(tx.input().as_ref())),
            status: None,
            block_number: tx.block_number,
            block_hash: tx.block_hash.map(|h| format!("{:?}", h)),
            gas_used: None,
            effective_gas_price: None,
        }
    }
}

impl From<TransactionReceipt> for MonadTransaction {
    fn from(receipt: TransactionReceipt) -> Self {
        Self {
            hash: format!("{:?}", receipt.transaction_hash),
            from: format!("{:?}", receipt.from),
            to: receipt.to.map(|a| format!("{:?}", a)),
            value: "0".to_string(),
            gas_price: None,
            gas_limit: 0,
            nonce: 0,
            data: String::new(),
            status: Some(receipt.status()),
            block_number: receipt.block_number,
            block_hash: receipt.block_hash.map(|h| format!("{:?}", h)),
            gas_used: Some(receipt.gas_used as u64),
            effective_gas_price: Some(receipt.effective_gas_price.to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonadEvent {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub block_number: u64,
    pub transaction_hash: String,
    pub log_index: u64,
}

impl From<Log> for MonadEvent {
    fn from(log: Log) -> Self {
        Self {
            address: format!("{:?}", log.address()),
            topics: log.topics().iter().map(|t| format!("{:?}", t)).collect(),
            data: format!("0x{}", hex::encode(log.data().data.clone().as_ref())),
            block_number: log.block_number.unwrap_or_default(),
            transaction_hash: format!("{:?}", log.transaction_hash.unwrap_or_default()),
            log_index: log.log_index.unwrap_or_default(),
        }
    }
}

/// Format MONAD amount (18 decimals)
///
/// DEPRECATED: Use `chains::common::token::chain_formatters::format_monad`
/// instead
pub fn format_eth(wei: U256) -> String {
    chain_formatters::format_monad(wei)
}

/// Parse MONAD string to U256
///
/// DEPRECATED: Use `chains::common::token::parse_native_amount` instead
pub fn parse_eth(eth: &str) -> Option<U256> {
    parse_native_amount(eth)
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
}
