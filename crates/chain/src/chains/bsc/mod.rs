//! BSC (Binance Smart Chain) Module
//!
//! BSC is an EVM-compatible blockchain with high throughput and low fees.
//! - Chain ID: 56 (Mainnet), 97 (Testnet)
//! - Block Time: ~3 seconds
//! - Consensus: Proof of Staked Authority (PoSA)

pub mod client;
pub mod types;

// Re-export common types for convenience
pub use crate::chains::common::{
    format_native_token, parse_native_token, ContractCall, ContractDeploy, ContractInstance,
    EventFilter, EvmConfig, EvmError, EvmProvider, Mempool, TransactionBuilder,
    TransactionPriority as BscPriority,
};

pub const BSC_MAINNET_CHAIN_ID: u64 = 56;
pub const BSC_TESTNET_CHAIN_ID: u64 = 97;

/// BSC Network Configuration
pub struct BscConfig {
    pub rpc_url: String,
    pub ws_url: Option<String>,
    pub chain_id: u64,
    pub confirmation_blocks: u64,
    pub gas_limit: u64,
    /// BSC specific: whether to use fast finality
    pub fast_finality: bool,
}

impl BscConfig {
    /// BSC Mainnet configuration
    pub fn mainnet() -> Self {
        Self {
            rpc_url: "https://bsc-dataseed.binance.org".to_string(),
            ws_url: Some("wss://bsc-ws-node.nariox.org:443".to_string()),
            chain_id: BSC_MAINNET_CHAIN_ID,
            confirmation_blocks: 15, // BSC recommends 15 blocks for finality
            gas_limit: 30_000_000,
            fast_finality: true,
        }
    }

    /// BSC Testnet configuration
    pub fn testnet() -> Self {
        Self {
            rpc_url: "https://data-seed-prebsc-1-s1.binance.org:8545".to_string(),
            ws_url: Some("wss://bsc-testnet.nariox.org:443".to_string()),
            chain_id: BSC_TESTNET_CHAIN_ID,
            confirmation_blocks: 15,
            gas_limit: 30_000_000,
            fast_finality: true,
        }
    }

    /// Local development configuration
    pub fn devnet() -> Self {
        Self {
            rpc_url: "http://localhost:8545".to_string(),
            ws_url: Some("ws://localhost:8546".to_string()),
            chain_id: 1337,
            confirmation_blocks: 1,
            gas_limit: 30_000_000,
            fast_finality: false,
        }
    }

    /// Custom configuration with specific RPC endpoint
    pub fn custom(rpc_url: &str, chain_id: u64) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            ws_url: None,
            chain_id,
            confirmation_blocks: 15,
            gas_limit: 30_000_000,
            fast_finality: true,
        }
    }
}

/// BSC Client
pub use client::BscClient;

/// BSC-specific error type
///
/// This is now a type alias to `EvmError` for consistency across chains.
/// Use `EvmError` directly for new code.
pub type BscError = super::common::EvmError;

/// BSC-specific constants
pub mod constants {
    /// Average block time in seconds
    pub const BLOCK_TIME_SECONDS: u64 = 3;

    /// Recommended confirmation blocks for safe transactions
    pub const SAFE_CONFIRMATION_BLOCKS: u64 = 15;

    /// Fast confirmation blocks (less safe)
    pub const FAST_CONFIRMATION_BLOCKS: u64 = 5;

    /// Native token symbol
    pub const NATIVE_TOKEN: &str = "BNB";

    /// Native token decimals
    pub const NATIVE_TOKEN_DECIMALS: u8 = 18;
}
