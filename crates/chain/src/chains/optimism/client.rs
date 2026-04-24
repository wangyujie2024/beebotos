//! Optimism Client

use crate::chains::common::{BaseEvmClient, EvmClient, EvmConfig};
use crate::chains::optimism::types::OptimismBlockStats;
use crate::ChainResult;

/// Optimism blockchain client
pub struct OptimismClient {
    base: BaseEvmClient,
    #[allow(dead_code)]
    block_stats: OptimismBlockStats,
}

impl OptimismClient {
    /// Create new Optimism client from RPC URL
    pub async fn new(rpc_url: &str) -> ChainResult<Self> {
        let base =
            BaseEvmClient::new(rpc_url, crate::chains::optimism::OPTIMISM_MAINNET_CHAIN_ID).await?;
        Ok(Self {
            base,
            block_stats: OptimismBlockStats::default(),
        })
    }

    /// Create from base client
    pub fn from_base(base: BaseEvmClient) -> Self {
        Self {
            base,
            block_stats: OptimismBlockStats::default(),
        }
    }

    /// Get recommended confirmation blocks
    pub fn confirmation_blocks(&self) -> u64 {
        crate::chains::optimism::constants::SAFE_CONFIRMATION_BLOCKS
    }

    /// Get explorer URL for transaction
    pub fn get_explorer_url(&self, tx_hash: &str) -> String {
        format!("https://optimistic.etherscan.io/tx/{}", tx_hash)
    }
}

impl std::ops::Deref for OptimismClient {
    type Target = BaseEvmClient;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl std::ops::DerefMut for OptimismClient {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.base
    }
}

#[async_trait::async_trait]
impl EvmClient for OptimismClient {
    fn provider(&self) -> &crate::chains::common::EvmProvider {
        self.base.provider()
    }

    fn config(&self) -> &EvmConfig {
        self.base.config()
    }

    fn confirmation_blocks(&self) -> u64 {
        self.confirmation_blocks()
    }

    async fn wait_for_confirmation(&self, tx_hash: &str, timeout_secs: u64) -> ChainResult<bool> {
        use std::time::Duration;

        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(timeout_secs);

        while start.elapsed() < timeout {
            if let Ok(Some(receipt)) = self.get_transaction_receipt(tx_hash).await {
                if receipt.status() {
                    return Ok(true);
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        Ok(false)
    }
}
