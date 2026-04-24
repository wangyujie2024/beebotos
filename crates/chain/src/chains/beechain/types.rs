//! Beechain-specific types

use alloy_consensus::Transaction as _;
use alloy_primitives::U256;
use alloy_rpc_types::{Block, Log, Transaction, TransactionReceipt};
use serde::{Deserialize, Serialize};

use crate::chains::common::token::{chain_formatters, parse_native_amount, TransactionPriority};

/// Beechain Block representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeechainBlock {
    pub number: u64,
    pub hash: String,
    pub parent_hash: String,
    pub timestamp: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub transactions: Vec<String>,
    pub validator: String,
    /// Beechain-specific: parallel execution group
    pub parallel_group: Option<u32>,
    /// Beechain-specific: number of parallel transactions in this block
    pub parallel_tx_count: u32,
    /// Beechain-specific: total sequential gas used (if sequential fallback)
    pub sequential_gas_used: Option<u64>,
}

impl From<Block> for BeechainBlock {
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
            parallel_group: None,
            parallel_tx_count: block.transactions.len() as u32,
            sequential_gas_used: None,
        }
    }
}

/// Beechain Transaction representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeechainTransaction {
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
    /// Beechain-specific: parallel execution group assigned
    pub parallel_group: Option<u32>,
    /// Beechain-specific: execution time in microseconds
    pub execution_time_us: Option<u64>,
    /// Beechain-specific: contention score (0-100)
    pub contention_score: Option<u8>,
}

impl From<Transaction> for BeechainTransaction {
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
            parallel_group: None,
            execution_time_us: None,
            contention_score: None,
        }
    }
}

impl From<TransactionReceipt> for BeechainTransaction {
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
            parallel_group: None,
            execution_time_us: None,
            contention_score: None,
        }
    }
}

/// Beechain Event representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeechainEvent {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub block_number: u64,
    pub transaction_hash: String,
    pub log_index: u64,
    /// Whether this event is from a finalized block
    pub finalized: bool,
    /// Beechain-specific: time to finality (seconds)
    pub time_to_finality: Option<f64>,
}

impl From<Log> for BeechainEvent {
    fn from(log: Log) -> Self {
        Self {
            address: format!("{:?}", log.address()),
            topics: log.topics().iter().map(|t| format!("{:?}", t)).collect(),
            data: format!("0x{}", hex::encode(log.data().data.clone().as_ref())),
            block_number: log.block_number.unwrap_or_default(),
            transaction_hash: format!("{:?}", log.transaction_hash.unwrap_or_default()),
            log_index: log.log_index.unwrap_or_default(),
            finalized: false,
            time_to_finality: None,
        }
    }
}

/// BKC (Beechain Coin) Token information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BkcToken {
    pub address: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: String,
}

/// Validator information for Beechain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeechainValidator {
    pub address: String,
    pub voting_power: u64,
    pub status: ValidatorStatus,
    /// Performance score (0-100)
    pub performance_score: u8,
    /// Commission rate (basis points)
    pub commission_rate: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidatorStatus {
    Active,
    Inactive,
    Jailed,
    Unbonding,
}

/// Format BKC amount (18 decimals)
///
/// DEPRECATED: Use `chains::common::token::chain_formatters::format_bkc`
/// instead
pub fn format_bkc(wei: U256) -> String {
    chain_formatters::format_bkc(wei)
}

/// Parse BKC string to wei
///
/// DEPRECATED: Use `chains::common::token::parse_native_amount` instead
pub fn parse_bkc(bkc: &str) -> Option<U256> {
    parse_native_amount(bkc)
}

/// Transaction priority for Beechain
///
/// Re-export from common module for backward compatibility
pub type BeechainPriority = TransactionPriority;

/// Extension trait for Beechain-specific priority functionality
pub trait BeechainPriorityExt {
    /// Whether to prefer parallel execution
    fn prefer_parallel(&self) -> bool;
}

impl BeechainPriorityExt for TransactionPriority {
    fn prefer_parallel(&self) -> bool {
        matches!(
            self,
            TransactionPriority::High | TransactionPriority::Urgent
        )
    }
}

/// Parallel execution group assignment
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParallelGroup {
    /// Can execute in any parallel group
    Flexible,
    /// Must execute in specific group
    Fixed(u32),
    /// Cannot execute in parallel (requires sequential execution)
    Sequential,
}

/// Execution result with parallel metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    pub gas_used: u64,
    pub execution_time_us: u64,
    pub parallel_group: Option<u32>,
    pub contention_score: u8,
}

/// Block time statistics for monitoring
#[derive(Debug, Clone, Default)]
pub struct BlockTimeStats {
    pub last_block_time_ms: f64,
    pub avg_block_time_ms: f64,
    pub min_block_time_ms: f64,
    pub max_block_time_ms: f64,
    pub deviation_count: u64,
}

impl BlockTimeStats {
    pub fn update(&mut self, block_time_ms: f64) {
        self.last_block_time_ms = block_time_ms;

        if self.avg_block_time_ms == 0.0 {
            self.avg_block_time_ms = block_time_ms;
            self.min_block_time_ms = block_time_ms;
            self.max_block_time_ms = block_time_ms;
        } else {
            // Exponential moving average
            self.avg_block_time_ms = self.avg_block_time_ms * 0.9 + block_time_ms * 0.1;
            self.min_block_time_ms = self.min_block_time_ms.min(block_time_ms);
            self.max_block_time_ms = self.max_block_time_ms.max(block_time_ms);
        }

        // Check for deviation (>20% from target 400ms)
        if (block_time_ms - 400.0).abs() > 80.0 {
            self.deviation_count += 1;
        }
    }

    pub fn is_healthy(&self) -> bool {
        // Less than 5% deviation in last 100 blocks
        self.deviation_count < 5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bkc() {
        let wei = U256::from(1_500_000_000_000_000_000u64); // 1.5 BKC
        let bkc = format_bkc(wei);
        assert!(bkc.starts_with("1.5"));
    }

    #[test]
    fn test_parse_bkc() {
        let bkc = "1.5";
        let wei = parse_bkc(bkc).unwrap();
        let expected = U256::from(1_500_000_000_000_000_000u64);
        assert_eq!(wei, expected);
    }

    #[test]
    fn test_priority_multiplier() {
        assert_eq!(BeechainPriority::Low.multiplier(), 0.9);
        assert_eq!(BeechainPriority::Urgent.multiplier(), 1.5);
    }

    #[test]
    fn test_block_time_stats() {
        let mut stats = BlockTimeStats::default();
        stats.update(400.0);
        stats.update(410.0);
        stats.update(390.0);

        assert!(stats.avg_block_time_ms > 0.0);
        // is_healthy returns true when deviation_count < 5
        // With only 3 samples within normal range, it should be healthy
        assert!(stats.is_healthy());
    }
}
