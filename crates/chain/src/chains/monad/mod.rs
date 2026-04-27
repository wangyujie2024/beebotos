//! Monad Module
//!
//! Monad-specific implementations. Generic EVM components have been moved to
//! `chains::common`. BeeBotOS contract bindings are in `contracts::bindings`
//! and `contracts::events`.

pub mod client;
pub mod types;

// Re-export BeeBotOS contract bindings from contracts module (chain-agnostic)
// Re-export comprehensive event types (BeeBotOS specific)
pub use events_comprehensive::{
    BeeBotOSEvent, BeeBotOSEventFilter, BeeBotOSEventListener, BeeBotOSEventStream,
    BeeBotOSEventType,
};

// Re-export common EVM components for convenience
pub use crate::chains::common::{
    format_native_token, parse_native_token, ContractCall, ContractDeploy, ContractInstance,
    EventFilter, EvmConfig, EvmProvider, Mempool, TransactionBuilder,
};
pub use crate::contracts::{bindings as contracts, events as events_comprehensive};

pub const MONAD_CHAIN_ID: u64 = 1_014_301;
pub const MONAD_TESTNET_CHAIN_ID: u64 = 10_143;

/// Monad configuration
pub struct MonadConfig {
    pub rpc_url: String,
    pub ws_url: Option<String>,
    pub chain_id: u64,
    pub confirmation_blocks: u64,
    pub gas_limit: u64,
}

impl MonadConfig {
    pub fn mainnet() -> Self {
        Self {
            rpc_url: "https://rpc.monad.xyz".to_string(),
            ws_url: Some("wss://ws.monad.xyz".to_string()),
            chain_id: MONAD_CHAIN_ID,
            confirmation_blocks: 1,
            gas_limit: 30_000_000,
        }
    }

    pub fn testnet() -> Self {
        Self {
            rpc_url: "https://rpc.testnet.monad.xyz".to_string(),
            ws_url: Some("wss://ws.testnet.monad.xyz".to_string()),
            chain_id: MONAD_TESTNET_CHAIN_ID,
            confirmation_blocks: 1,
            gas_limit: 30_000_000,
        }
    }

    pub fn devnet() -> Self {
        Self {
            rpc_url: "http://localhost:8545".to_string(),
            ws_url: Some("ws://localhost:8546".to_string()),
            chain_id: 1337,
            confirmation_blocks: 0,
            gas_limit: 30_000_000,
        }
    }
}

/// Monad client
pub use client::MonadClient;

/// Monad-specific error type
///
/// This is now a type alias to `EvmError` for consistency across chains.
/// Use `EvmError` directly for new code.
pub type MonadError = super::common::EvmError;
