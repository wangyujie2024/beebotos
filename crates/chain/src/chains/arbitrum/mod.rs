//! Arbitrum Module
//!
//! Arbitrum is a leading Layer 2 scaling solution for Ethereum using Optimistic
//! Rollups.
//! - Chain ID: 42161 (Mainnet), 421614 (Sepolia Testnet)
//! - Block Time: ~0.25 seconds (sub-second finality)
//! - Consensus: Optimistic Rollup with fraud proofs
//! - Settlement: Ethereum mainnet
//!
//! ## Official Resources
//! - RPC: https://arb1.arbitrum.io/rpc
//! - Explorer: https://arbiscan.io
//! - Native Token: ETH (bridged from Ethereum)

pub mod client;
pub mod types;

// Re-export common types for convenience
pub use crate::chains::common::{
    format_native_token, parse_native_token, ContractCall, ContractDeploy, ContractInstance,
    EventFilter, EvmConfig, EvmError, EvmProvider, Mempool, TransactionBuilder,
    TransactionPriority as ArbitrumPriority,
};

pub const ARBITRUM_MAINNET_CHAIN_ID: u64 = 42161;
pub const ARBITRUM_SEPOLIA_CHAIN_ID: u64 = 421614;
pub const ARBITRUM_NOVA_CHAIN_ID: u64 = 42170;

/// Arbitrum Network Configuration
pub struct ArbitrumConfig {
    pub rpc_url: String,
    pub ws_url: Option<String>,
    pub chain_id: u64,
    pub confirmation_blocks: u64,
    pub gas_limit: u64,
    /// Whether to use EIP-1559 transactions
    pub use_eip1559: bool,
    /// Nitro-specific: ArbOS version
    pub arbos_version: Option<u64>,
}

impl ArbitrumConfig {
    /// Arbitrum One Mainnet configuration
    pub fn mainnet() -> Self {
        Self {
            rpc_url: "https://arb1.arbitrum.io/rpc".to_string(),
            ws_url: Some("wss://arb1.arbitrum.io/ws".to_string()),
            chain_id: ARBITRUM_MAINNET_CHAIN_ID,
            confirmation_blocks: 10, // ~2.5 seconds for finality
            gas_limit: 30_000_000,
            use_eip1559: true,
            arbos_version: Some(11),
        }
    }

    /// Arbitrum Nova configuration (high throughput, lower security)
    pub fn nova() -> Self {
        Self {
            rpc_url: "https://nova.arbitrum.io/rpc".to_string(),
            ws_url: Some("wss://nova.arbitrum.io/ws".to_string()),
            chain_id: ARBITRUM_NOVA_CHAIN_ID,
            confirmation_blocks: 10,
            gas_limit: 30_000_000,
            use_eip1559: true,
            arbos_version: Some(11),
        }
    }

    /// Arbitrum Sepolia Testnet configuration
    pub fn sepolia() -> Self {
        Self {
            rpc_url: "https://sepolia-rollup.arbitrum.io/rpc".to_string(),
            ws_url: Some("wss://sepolia-rollup.arbitrum.io/ws".to_string()),
            chain_id: ARBITRUM_SEPOLIA_CHAIN_ID,
            confirmation_blocks: 10,
            gas_limit: 30_000_000,
            use_eip1559: true,
            arbos_version: Some(11),
        }
    }

    /// Local development configuration
    pub fn devnet() -> Self {
        Self {
            rpc_url: "http://localhost:8547".to_string(),
            ws_url: Some("ws://localhost:8548".to_string()),
            chain_id: 412346,
            confirmation_blocks: 1,
            gas_limit: 30_000_000,
            use_eip1559: false,
            arbos_version: None,
        }
    }

    /// Custom configuration with specific RPC endpoint
    pub fn custom(rpc_url: &str, chain_id: u64) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            ws_url: None,
            chain_id,
            confirmation_blocks: 10,
            gas_limit: 30_000_000,
            use_eip1559: true,
            arbos_version: None,
        }
    }
}

/// Arbitrum Client
pub use client::ArbitrumClient;

/// Arbitrum-specific error type
pub type ArbitrumError = super::common::EvmError;

/// Arbitrum-specific constants
pub mod constants {
    /// Average block time in seconds (sub-second)
    pub const BLOCK_TIME_SECONDS: f64 = 0.25;

    /// Recommended confirmation blocks for safe transactions
    pub const SAFE_CONFIRMATION_BLOCKS: u64 = 10;

    /// Fast confirmation blocks (less safe)
    pub const FAST_CONFIRMATION_BLOCKS: u64 = 5;

    /// Native token symbol (bridged ETH)
    pub const NATIVE_TOKEN: &str = "ETH";

    /// Native token decimals
    pub const NATIVE_TOKEN_DECIMALS: u8 = 18;

    /// Default gas limit for standard transactions
    pub const DEFAULT_GAS_LIMIT: u64 = 21000;

    /// Maximum gas limit per block
    pub const MAX_GAS_LIMIT_PER_BLOCK: u64 = 30_000_000;

    /// L1 (Ethereum) settlement time
    pub const L1_SETTLEMENT_TIME_MINUTES: u64 = 7;

    /// Challenge period for fraud proofs (days)
    pub const CHALLENGE_PERIOD_DAYS: u64 = 7;
}

/// Arbitrum network type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArbitrumNetwork {
    Mainnet,
    Nova,
    Sepolia,
    Devnet,
}

impl ArbitrumNetwork {
    pub fn chain_id(&self) -> u64 {
        match self {
            ArbitrumNetwork::Mainnet => ARBITRUM_MAINNET_CHAIN_ID,
            ArbitrumNetwork::Nova => ARBITRUM_NOVA_CHAIN_ID,
            ArbitrumNetwork::Sepolia => ARBITRUM_SEPOLIA_CHAIN_ID,
            ArbitrumNetwork::Devnet => 412346,
        }
    }

    pub fn config(&self) -> ArbitrumConfig {
        match self {
            ArbitrumNetwork::Mainnet => ArbitrumConfig::mainnet(),
            ArbitrumNetwork::Nova => ArbitrumConfig::nova(),
            ArbitrumNetwork::Sepolia => ArbitrumConfig::sepolia(),
            ArbitrumNetwork::Devnet => ArbitrumConfig::devnet(),
        }
    }

    pub fn explorer_url(&self) -> &'static str {
        match self {
            ArbitrumNetwork::Mainnet => "https://arbiscan.io",
            ArbitrumNetwork::Nova => "https://nova.arbiscan.io",
            ArbitrumNetwork::Sepolia => "https://sepolia.arbiscan.io",
            ArbitrumNetwork::Devnet => "http://localhost:4000",
        }
    }

    pub fn is_mainnet(&self) -> bool {
        matches!(self, ArbitrumNetwork::Mainnet | ArbitrumNetwork::Nova)
    }
}

/// Arbitrum-specific transaction data
#[derive(Debug, Clone)]
pub struct ArbitrumTransactionData {
    /// L1 gas estimate
    pub l1_gas_estimate: u64,
    /// L1 gas price
    pub l1_gas_price: u64,
    /// ArbOS fee
    pub arbos_fee: u64,
}
