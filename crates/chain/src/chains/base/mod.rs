//! Base Module
//!
//! Base is a Layer 2 scaling solution for Ethereum built by Coinbase using
//! Optimism's OP Stack.
//! - Chain ID: 8453 (Mainnet), 84532 (Sepolia Testnet)
//! - Block Time: ~2 seconds
//! - Consensus: Optimistic Rollup
//! - Settlement: Ethereum mainnet
//!
//! ## Official Resources
//! - RPC: https://mainnet.base.org
//! - Explorer: https://basescan.org
//! - Native Token: ETH (bridged from Ethereum)

pub mod client;
pub mod types;

// Re-export common types for convenience
pub use crate::chains::common::{
    format_native_token, parse_native_token, ContractCall, ContractDeploy, ContractInstance,
    EventFilter, EvmConfig, EvmError, EvmProvider, Mempool, TransactionBuilder,
    TransactionPriority as BasePriority,
};

pub const BASE_MAINNET_CHAIN_ID: u64 = 8453;
pub const BASE_SEPOLIA_CHAIN_ID: u64 = 84532;
pub const BASE_GOERLI_CHAIN_ID: u64 = 84531; // Deprecated

/// Base Network Configuration
pub struct BaseConfig {
    pub rpc_url: String,
    pub ws_url: Option<String>,
    pub chain_id: u64,
    pub confirmation_blocks: u64,
    pub gas_limit: u64,
    /// Whether to use EIP-1559 transactions
    pub use_eip1559: bool,
    /// Coinbase-specific: sequencer fee
    pub sequencer_fee: u64,
}

impl BaseConfig {
    /// Base Mainnet configuration
    pub fn mainnet() -> Self {
        Self {
            rpc_url: "https://mainnet.base.org".to_string(),
            ws_url: Some("wss://base.publicnode.com".to_string()),
            chain_id: BASE_MAINNET_CHAIN_ID,
            confirmation_blocks: 5, // ~10 seconds
            gas_limit: 30_000_000,
            use_eip1559: true,
            sequencer_fee: 0,
        }
    }

    /// Base Sepolia Testnet configuration
    pub fn sepolia() -> Self {
        Self {
            rpc_url: "https://sepolia.base.org".to_string(),
            ws_url: Some("wss://base-sepolia.publicnode.com".to_string()),
            chain_id: BASE_SEPOLIA_CHAIN_ID,
            confirmation_blocks: 5,
            gas_limit: 30_000_000,
            use_eip1559: true,
            sequencer_fee: 0,
        }
    }

    /// Local development configuration
    pub fn devnet() -> Self {
        Self {
            rpc_url: "http://localhost:8545".to_string(),
            ws_url: Some("ws://localhost:8546".to_string()),
            chain_id: 8454,
            confirmation_blocks: 1,
            gas_limit: 30_000_000,
            use_eip1559: false,
            sequencer_fee: 0,
        }
    }

    /// Custom configuration with specific RPC endpoint
    pub fn custom(rpc_url: &str, chain_id: u64) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            ws_url: None,
            chain_id,
            confirmation_blocks: 5,
            gas_limit: 30_000_000,
            use_eip1559: true,
            sequencer_fee: 0,
        }
    }
}

/// Base Client
pub use client::BaseClient;

/// Base-specific error type
pub type BaseError = super::common::EvmError;

/// Base-specific constants
pub mod constants {
    /// Average block time in seconds
    pub const BLOCK_TIME_SECONDS: u64 = 2;

    /// Recommended confirmation blocks for safe transactions
    pub const SAFE_CONFIRMATION_BLOCKS: u64 = 5;

    /// Fast confirmation blocks (less safe)
    pub const FAST_CONFIRMATION_BLOCKS: u64 = 3;

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

    /// Sequencer commitment interval
    pub const SEQUENCER_COMMITMENT_SECONDS: u64 = 300; // 5 minutes
}

/// Base network type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseNetwork {
    Mainnet,
    Sepolia,
    Devnet,
}

impl BaseNetwork {
    pub fn chain_id(&self) -> u64 {
        match self {
            BaseNetwork::Mainnet => BASE_MAINNET_CHAIN_ID,
            BaseNetwork::Sepolia => BASE_SEPOLIA_CHAIN_ID,
            BaseNetwork::Devnet => 8454,
        }
    }

    pub fn config(&self) -> BaseConfig {
        match self {
            BaseNetwork::Mainnet => BaseConfig::mainnet(),
            BaseNetwork::Sepolia => BaseConfig::sepolia(),
            BaseNetwork::Devnet => BaseConfig::devnet(),
        }
    }

    pub fn explorer_url(&self) -> &'static str {
        match self {
            BaseNetwork::Mainnet => "https://basescan.org",
            BaseNetwork::Sepolia => "https://sepolia.basescan.org",
            BaseNetwork::Devnet => "http://localhost:4000",
        }
    }

    pub fn is_mainnet(&self) -> bool {
        matches!(self, BaseNetwork::Mainnet)
    }
}

/// Base-specific transaction data (L1 fee info)
#[derive(Debug, Clone)]
pub struct BaseTransactionData {
    /// L1 gas used
    pub l1_gas_used: u64,
    /// L1 gas price
    pub l1_gas_price: u64,
    /// L1 fee
    pub l1_fee: u64,
    /// L1 fee scalar
    pub l1_fee_scalar: f64,
}

/// Base ecosystem partners and integrations
pub mod ecosystem {
    /// Coinbase Wallet integration
    pub const COINBASE_WALLET: &str = "Coinbase Wallet";
    /// Coinbase Pay integration
    pub const COINBASE_PAY: &str = "Coinbase Pay";
    /// Coinbase Cloud RPC
    pub const COINBASE_CLOUD_RPC: &str = "Coinbase Cloud";
    /// Base Bridge
    pub const BASE_BRIDGE: &str = "https://bridge.base.org";
}
