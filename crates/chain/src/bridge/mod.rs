//! Cross-chain bridge module
//!
//! This module provides unified cross-chain functionality:
//! - BridgeClient: Integration with Solidity CrossChainBridge contract
//!   (lock/release mechanism)
//! - AtomicSwapClient: Peer-to-peer atomic swaps using HTLC
//! - CrossChainRouter: Route selection for different bridge providers

// Note: keccak256 imported from alloy_primitives where needed
use serde::{Deserialize, Serialize};

use crate::compat::{Address, B256, U256};
use crate::{ChainError, Result};

/// Bridge transaction status (legacy, for compatibility)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BridgeStatus {
    Pending,
    InProgress,
    Completed,
    Failed(String),
}

/// Bridge transaction (legacy, for compatibility)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeTx {
    pub tx_hash: B256,
    pub from_chain: ChainId,
    pub to_chain: ChainId,
    pub from_address: Address,
    pub to_address: Address,
    pub token: Address,
    pub amount: U256,
    pub status: BridgeStatus,
    pub timestamp: u64,
}

/// Chain ID wrapper
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ChainId(pub u64);

impl ChainId {
    pub const ETHEREUM: Self = Self(1);
    pub const MONAD: Self = Self(10_143);
    pub const MONAD_TESTNET: Self = Self(10_143);
    pub const BSC: Self = Self(56);
    pub const POLYGON: Self = Self(137);
    pub const ARBITRUM: Self = Self(42_161);
    pub const OPTIMISM: Self = Self(10);
    pub const BASE: Self = Self(8453);
}

/// Cross-chain bridge trait
#[async_trait::async_trait]
pub trait Bridge: Send + Sync {
    /// Initiate bridge transfer
    async fn bridge(
        &self,
        to_chain: ChainId,
        token: Address,
        amount: U256,
        recipient: Address,
    ) -> Result<B256>;

    /// Get bridge status
    async fn get_status(&self, tx_hash: B256) -> Result<BridgeStatus>;

    /// Get supported chains
    fn supported_chains(&self) -> Vec<ChainId>;
}

/// Cross-chain router
pub struct CrossChainRouter {
    bridges: Vec<Box<dyn Bridge>>,
}

impl CrossChainRouter {
    pub fn new() -> Self {
        Self {
            bridges: Vec::new(),
        }
    }

    pub fn register_bridge(&mut self, bridge: Box<dyn Bridge>) {
        self.bridges.push(bridge);
    }

    pub async fn route(
        &self,
        from_chain: ChainId,
        to_chain: ChainId,
        token: Address,
        amount: U256,
    ) -> Result<B256> {
        // Find appropriate bridge
        for bridge in &self.bridges {
            let supported = bridge.supported_chains();
            if supported.contains(&from_chain) && supported.contains(&to_chain) {
                // This is a simplified implementation
                // In practice, you'd need to handle the from_chain properly
                return bridge.bridge(to_chain, token, amount, Address::ZERO).await;
            }
        }

        Err(ChainError::Bridge(format!(
            "No bridge found for chain pair {:?} -> {:?}",
            from_chain, to_chain
        )))
    }
}

impl Default for CrossChainRouter {
    fn default() -> Self {
        Self::new()
    }
}

// Re-export new client types
pub mod client;
pub use client::{
    AtomicSwapClient,

    BridgeClient,
    BridgeRequestInfo,
    BridgeState,
    // Legacy re-exports for backwards compatibility
    BridgeState as BridgeStateUnified,
    HTLC,
};

// Re-export router and atomic swap
pub mod adapters;
pub mod atomic_swap;
pub mod router;

pub use router::{BridgeRoute, RouteFinder};
// Note: HTLC is now defined in client.rs
// pub use atomic_swap::HTLC as HTLCLegacy; // Removed - use client::HTLC
// instead
