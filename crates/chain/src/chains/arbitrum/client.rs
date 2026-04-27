//! Arbitrum Client

use crate::chains::arbitrum::types::ArbitrumBlockStats;
use crate::chains::common::{BaseEvmClient, EvmClient, EvmConfig};
use crate::ChainResult;

/// Arbitrum blockchain client
pub struct ArbitrumClient {
    base: BaseEvmClient,
    #[allow(dead_code)]
    block_stats: ArbitrumBlockStats,
}

impl ArbitrumClient {
    /// Create new Arbitrum client from RPC URL
    pub async fn new(rpc_url: &str) -> ChainResult<Self> {
        let base =
            BaseEvmClient::new(rpc_url, crate::chains::arbitrum::ARBITRUM_MAINNET_CHAIN_ID).await?;
        Ok(Self {
            base,
            block_stats: ArbitrumBlockStats::default(),
        })
    }

    /// Create from base client
    pub fn from_base(base: BaseEvmClient) -> Self {
        Self {
            base,
            block_stats: ArbitrumBlockStats::default(),
        }
    }

    /// Get recommended confirmation blocks
    pub fn confirmation_blocks(&self) -> u64 {
        crate::chains::arbitrum::constants::SAFE_CONFIRMATION_BLOCKS
    }

    /// Get L1 settlement time
    pub fn l1_settlement_time_minutes(&self) -> u64 {
        crate::chains::arbitrum::constants::L1_SETTLEMENT_TIME_MINUTES
    }

    /// Get explorer URL for transaction
    pub fn get_explorer_url(&self, tx_hash: &str) -> String {
        format!("https://arbiscan.io/tx/{}", tx_hash)
    }
}

impl std::ops::Deref for ArbitrumClient {
    type Target = BaseEvmClient;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl std::ops::DerefMut for ArbitrumClient {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.base
    }
}

#[async_trait::async_trait]
impl EvmClient for ArbitrumClient {
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
            tokio::time::sleep(Duration::from_millis(250)).await;
        }

        Ok(false)
    }
}
