//! Common Chain Components
//!
//! This module provides shared functionality for all EVM-compatible chains:
//! - Contract interactions
//! - Transaction building
//! - Mempool management
//! - Provider wrappers
//! - Event handling
//! - Client abstractions

pub mod batch;
pub mod client;
pub mod contract;
pub mod events;
pub mod gas;
pub mod macros;
pub mod mempool;
pub mod provider;
pub mod state_cache;
pub mod token;
pub mod transaction;
pub mod tx_queue;

// Macros are exported at crate root due to #[macro_export]
// Use crate::define_block_from etc. to use them

// Re-export commonly used types
pub use batch::{
    BatchOperation, BatchRequest, BatchResponse, BatchResultAggregator, TransactionBatch,
};
// Re-export chain configs from client module
pub use client::chain_configs::{BeechainConfig, BscConfig, EthereumConfig, MonadConfig};
pub use client::{
    BaseEvmClient, ChainClient, ChainClientBuilder, ChainConfig, EvmClient, EvmClientExt,
};
pub use contract::{ContractCall, ContractDeploy, ContractInstance};
pub use events::{
    BatchedEventStream, EventFilter, EventListener, EventManager, EventRouter, EventStream,
    FilteredEventStream, MultiChainEventManager, Subscription, SubscriptionConfig, SubscriptionId,
    SubscriptionType,
};
pub use gas::{
    EIP1559FeeEstimate, GasEstimate, GasEstimator, GasEstimatorConfig, OperationGasEstimator,
};
pub use mempool::Mempool;
pub use provider::EvmProvider;
pub use state_cache::{ChainStateCache, StateCacheConfig, StateCacheStatistics};
pub use token::{
    chain_formatters, format_native_amount, format_token_amount, parse_native_amount,
    parse_token_amount, BeechainPriority, BscPriority, EthereumPriority, TransactionPriority,
    DEFAULT_TOKEN_DECIMALS,
};
pub use transaction::TransactionBuilder;
pub use tx_queue::{
    QueueStatistics, QueuedTransaction, QueuedTxBuilder, TransactionQueue, TxBatchBuilder,
    TxBatchProcessor, TxId, TxQueueConfig, TxResult,
};

use crate::compat::U256;
use crate::constants::{
    DEFAULT_CONFIRMATION_BLOCKS, DEFAULT_GAS_LIMIT, DEFAULT_MAX_FEE_PER_GAS,
    DEFAULT_MAX_PRIORITY_FEE_PER_GAS, WEI_PER_ETH,
};

/// Generic configuration for EVM chains
#[derive(Debug, Clone)]
pub struct EvmConfig {
    pub rpc_url: String,
    pub ws_url: Option<String>,
    pub chain_id: u64,
    pub confirmation_blocks: u64,
    pub gas_limit: u64,
    pub max_fee_per_gas: U256,
    pub max_priority_fee_per_gas: U256,
}

impl EvmConfig {
    pub fn new(rpc_url: &str, chain_id: u64) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            ws_url: None,
            chain_id,
            confirmation_blocks: DEFAULT_CONFIRMATION_BLOCKS,
            gas_limit: DEFAULT_GAS_LIMIT,
            max_fee_per_gas: U256::from(DEFAULT_MAX_FEE_PER_GAS),
            max_priority_fee_per_gas: U256::from(DEFAULT_MAX_PRIORITY_FEE_PER_GAS),
        }
    }

    pub fn with_ws(mut self, ws_url: &str) -> Self {
        self.ws_url = Some(ws_url.to_string());
        self
    }

    pub fn with_confirmation_blocks(mut self, blocks: u64) -> Self {
        self.confirmation_blocks = blocks;
        self
    }
}

/// Common error type for EVM chains
#[derive(Debug, Clone)]
pub enum EvmError {
    ProviderError(String),
    ContractError(String),
    TransactionError(String),
    NetworkError(String),
    InvalidAddress,
    InsufficientFunds,
    GasEstimationFailed,
    NonceError(String),
    ReplacementTransactionUnderpriced,
}

impl std::fmt::Display for EvmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvmError::ProviderError(e) => write!(f, "Provider error: {}", e),
            EvmError::ContractError(e) => write!(f, "Contract error: {}", e),
            EvmError::TransactionError(e) => write!(f, "Transaction error: {}", e),
            EvmError::NetworkError(e) => write!(f, "Network error: {}", e),
            EvmError::InvalidAddress => write!(f, "Invalid address"),
            EvmError::InsufficientFunds => write!(f, "Insufficient funds"),
            EvmError::GasEstimationFailed => write!(f, "Gas estimation failed"),
            EvmError::NonceError(e) => write!(f, "Nonce error: {}", e),
            EvmError::ReplacementTransactionUnderpriced => {
                write!(f, "Replacement transaction underpriced")
            }
        }
    }
}

impl std::error::Error for EvmError {}

impl From<crate::ChainError> for EvmError {
    fn from(e: crate::ChainError) -> Self {
        EvmError::ProviderError(e.to_string())
    }
}

/// Generic block representation for EVM chains
#[derive(Debug, Clone)]
pub struct EvmBlock {
    pub number: u64,
    pub hash: String,
    pub parent_hash: String,
    pub timestamp: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub transactions: Vec<String>,
    pub validator: String,
    pub base_fee_per_gas: Option<u64>,
}

/// Generic transaction representation
#[derive(Debug, Clone)]
pub struct EvmTransaction {
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
    pub tx_type: u8,
}

/// Generic event representation
#[derive(Debug, Clone)]
pub struct EvmEvent {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub block_number: u64,
    pub transaction_hash: String,
    pub log_index: u64,
    pub removed: bool,
}

/// Format native token amount (18 decimals)
pub fn format_native_token(wei: U256, symbol: &str) -> String {
    let divisor = U256::from(WEI_PER_ETH);
    let whole = wei / divisor;
    let remainder = wei % divisor;
    format!("{}.{:018} {}", whole, remainder, symbol)
}

/// Parse native token string to wei
pub fn parse_native_token(amount: &str) -> Option<U256> {
    let parts: Vec<&str> = amount.split('.').collect();
    if parts.is_empty() || parts.len() > 2 {
        return None;
    }

    let whole: u64 = parts[0].parse().ok()?;
    let whole = U256::from(whole);
    let mut frac = U256::ZERO;

    if parts.len() == 2 {
        let frac_str = format!("{:0<18}", parts[1]);
        let frac_str = &frac_str[..18.min(frac_str.len())];
        let frac_val: u64 = frac_str.parse().ok()?;
        frac = U256::from(frac_val);
    }

    let multiplier = U256::from(WEI_PER_ETH);
    Some(whole * multiplier + frac)
}

/// Common chain constants
pub mod constants {
    // use alloy_primitives::U256;

    /// Standard ERC20 transfer gas limit
    pub const ERC20_TRANSFER_GAS: u64 = 65_000;

    /// Standard ERC20 approve gas limit
    pub const ERC20_APPROVE_GAS: u64 = 55_000;

    /// ETH transfer gas limit
    pub const ETH_TRANSFER_GAS: u64 = 21_000;

    /// Contract deployment base gas
    pub const CONTRACT_DEPLOY_BASE_GAS: u64 = 100_000;

    /// Default gas price in wei (5 gwei)
    pub const DEFAULT_GAS_PRICE: u128 = 5_000_000_000;

    /// Max gas price in wei (500 gwei)
    pub const MAX_GAS_PRICE: u128 = 500_000_000_000;

    /// EIP-1559 base fee max change denominator
    pub const BASE_FEE_MAX_CHANGE_DENOMINATOR: u64 = 8;

    /// EIP-1559 elasticity multiplier
    pub const ELASTICITY_MULTIPLIER: u64 = 2;

    /// Zero address
    pub const ZERO_ADDRESS: &str = "0x0000000000000000000000000000000000000000";
}

/// Chain feature flags
#[derive(Debug, Clone, Copy, Default)]
pub struct ChainFeatures {
    /// Supports EIP-1559
    pub eip1559: bool,
    /// Supports EIP-2930 (access lists)
    pub eip2930: bool,
    /// Supports EIP-4844 (blob transactions)
    pub eip4844: bool,
    /// Supports parallel execution
    pub parallel_execution: bool,
    /// Has fast finality
    pub fast_finality: bool,
}
