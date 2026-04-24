//! DeFi Module

use serde::{Deserialize, Serialize};

use crate::compat::{Address, U256};

/// DEX trait
#[async_trait::async_trait]
pub trait DEX: Send + Sync {
    /// Get token price
    async fn get_price(&self, token_in: Address, token_out: Address) -> anyhow::Result<U256>;

    /// Swap tokens
    async fn swap(&self, params: SwapParams) -> anyhow::Result<SwapResult>;

    /// Get liquidity for pair
    async fn get_liquidity(
        &self,
        token_a: Address,
        token_b: Address,
    ) -> anyhow::Result<(U256, U256)>;
}

/// Swap parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapParams {
    pub token_in: Address,
    pub token_out: Address,
    pub amount_in: U256,
    pub min_amount_out: U256,
    pub recipient: Address,
    pub deadline: u64,
}

/// Swap result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapResult {
    pub amount_in: U256,
    pub amount_out: U256,
    pub price_impact: f64,
    pub tx_hash: Option<crate::compat::B256>,
}

/// Lending protocol trait
#[async_trait::async_trait]
pub trait LendingProtocol: Send + Sync {
    /// Get collateral factor for asset
    async fn get_collateral_factor(&self, asset: Address) -> anyhow::Result<U256>;

    /// Supply collateral
    async fn supply(&self, asset: Address, amount: U256) -> anyhow::Result<()>;

    /// Borrow asset
    async fn borrow(&self, asset: Address, amount: U256) -> anyhow::Result<()>;

    /// Repay borrow
    async fn repay(&self, asset: Address, amount: U256) -> anyhow::Result<()>;

    /// Get account liquidity
    async fn get_account_liquidity(&self, account: Address) -> anyhow::Result<(U256, U256)>;
}

pub mod dex;
pub mod lending;
