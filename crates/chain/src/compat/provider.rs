//! Alloy Provider Wrapper
//!
//! Provides a unified interface for blockchain interaction using Alloy

use std::sync::Arc;

use alloy_primitives::{Address, B256, U256};
use alloy_provider::{Provider as AlloyProviderTrait, ReqwestProvider};
use alloy_rpc_types::{BlockId, BlockNumberOrTag, BlockTransactionsKind, TransactionRequest};
// Re-export async_trait for trait implementations
pub use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::compat::retry::{with_retry, RetryConfig};
use crate::ChainError;

/// Alloy-based blockchain client with retry support
#[derive(Clone)]
pub struct AlloyClient {
    provider: ReqwestProvider,
    chain_id: u64,
    retry_config: Arc<RwLock<RetryConfig>>,
}

impl AlloyClient {
    /// Create a new client from RPC URL with default retry config
    pub async fn new(rpc_url: &str) -> Result<Self, ChainError> {
        let provider = ReqwestProvider::new_http(
            reqwest::Url::parse(rpc_url).map_err(|e| ChainError::UrlParse(e.to_string()))?,
        );

        // Get chain ID with retry
        let chain_id = with_retry(&RetryConfig::default(), || async {
            provider
                .get_chain_id()
                .await
                .map_err(|e| ChainError::AlloyProvider(e.to_string()))
        })
        .await?;

        Ok(Self {
            provider,
            chain_id,
            retry_config: Arc::new(RwLock::new(RetryConfig::default())),
        })
    }

    /// Create a new client with known chain ID (skips network call)
    pub fn new_with_chain_id(rpc_url: &str, chain_id: u64) -> Result<Self, ChainError> {
        let provider = ReqwestProvider::new_http(
            reqwest::Url::parse(rpc_url).map_err(|e| ChainError::UrlParse(e.to_string()))?,
        );

        Ok(Self {
            provider,
            chain_id,
            retry_config: Arc::new(RwLock::new(RetryConfig::default())),
        })
    }

    /// Create with custom retry configuration
    pub fn with_retry_config(mut self, config: RetryConfig) -> Self {
        self.retry_config = Arc::new(RwLock::new(config));
        self
    }

    /// Update retry configuration
    pub async fn set_retry_config(&self, config: RetryConfig) {
        let mut guard = self.retry_config.write().await;
        *guard = config;
    }

    /// Get current retry configuration
    pub async fn retry_config(&self) -> RetryConfig {
        self.retry_config.read().await.clone()
    }

    /// Get the underlying provider
    pub fn provider(&self) -> &ReqwestProvider {
        &self.provider
    }

    /// Get chain ID
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Get latest block number with retry
    pub async fn get_block_number(&self) -> Result<u64, ChainError> {
        let config = self.retry_config.read().await.clone();
        with_retry(&config, || async {
            self.provider
                .get_block_number()
                .await
                .map_err(|e| ChainError::AlloyProvider(e.to_string()))
        })
        .await
    }

    /// Get balance for an address with retry
    pub async fn get_balance(&self, address: Address) -> Result<U256, ChainError> {
        let config = self.retry_config.read().await.clone();
        with_retry(&config, || async {
            self.provider
                .get_balance(address)
                .await
                .map_err(|e| ChainError::AlloyProvider(e.to_string()))
        })
        .await
    }

    /// Get gas price with retry
    pub async fn get_gas_price(&self) -> Result<u128, ChainError> {
        let config = self.retry_config.read().await.clone();
        with_retry(&config, || async {
            self.provider
                .get_gas_price()
                .await
                .map_err(|e| ChainError::AlloyProvider(e.to_string()))
        })
        .await
    }

    /// Estimate gas for a transaction with retry
    pub async fn estimate_gas(&self, tx: &TransactionRequest) -> Result<u64, ChainError> {
        let config = self.retry_config.read().await.clone();
        with_retry(&config, || async {
            self.provider
                .estimate_gas(tx)
                .await
                .map_err(|e| ChainError::AlloyProvider(e.to_string()))
        })
        .await
    }

    /// Send a transaction (no retry - transactions should not be blindly
    /// retried)
    pub async fn send_transaction(&self, tx: TransactionRequest) -> Result<B256, ChainError> {
        let pending = self
            .provider
            .send_transaction(tx)
            .await
            .map_err(|e| ChainError::AlloyProvider(e.to_string()))?;
        Ok(*pending.tx_hash())
    }

    /// Get transaction receipt with retry
    pub async fn get_transaction_receipt(
        &self,
        tx_hash: B256,
    ) -> Result<Option<alloy_rpc_types::TransactionReceipt>, ChainError> {
        let config = self.retry_config.read().await.clone();
        with_retry(&config, || async {
            self.provider
                .get_transaction_receipt(tx_hash)
                .await
                .map_err(|e| ChainError::AlloyProvider(e.to_string()))
        })
        .await
    }

    /// Get transaction by hash with retry
    pub async fn get_transaction(
        &self,
        tx_hash: B256,
    ) -> Result<Option<alloy_rpc_types::Transaction>, ChainError> {
        let config = self.retry_config.read().await.clone();
        with_retry(&config, || async {
            self.provider
                .get_transaction_by_hash(tx_hash)
                .await
                .map_err(|e| ChainError::AlloyProvider(e.to_string()))
        })
        .await
    }

    /// Get block by number with retry
    pub async fn get_block(
        &self,
        block_number: BlockNumberOrTag,
    ) -> Result<Option<alloy_rpc_types::Block>, ChainError> {
        let block_id = BlockId::Number(block_number);
        let config = self.retry_config.read().await.clone();
        with_retry(&config, || async {
            self.provider
                .get_block(block_id, BlockTransactionsKind::Hashes)
                .await
                .map_err(|e| ChainError::AlloyProvider(e.to_string()))
        })
        .await
    }

    /// Wait for transaction confirmation with timeout, max polling limit and
    /// retry
    ///
    /// RELIABILITY FIX: Added max polling iterations proportional to timeout to
    /// prevent infinite loops in case of network issues or node
    /// synchronization problems.
    pub async fn wait_for_confirmation(
        &self,
        tx_hash: B256,
        confirmations: u64,
        timeout_secs: u64,
    ) -> Result<alloy_rpc_types::TransactionReceipt, ChainError> {
        const POLL_INTERVAL_MS: u64 = 500;
        const MIN_POLLING_ITERATIONS: u32 = 10; // Minimum iterations before giving up

        // RELIABILITY FIX: Calculate max iterations based on timeout (with buffer)
        // This ensures the iteration limit is always meaningful relative to timeout
        let max_polling_iterations = ((timeout_secs * 1000 / POLL_INTERVAL_MS) as u32)
            .max(MIN_POLLING_ITERATIONS)
            .saturating_add(10); // Add 10 iteration buffer

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(timeout_secs);
        let mut iterations = 0u32;

        loop {
            iterations += 1;

            // Check timeout
            if start.elapsed() > timeout {
                return Err(ChainError::Provider(format!(
                    "Timeout waiting for {} confirmations after {:?}",
                    confirmations,
                    start.elapsed()
                )));
            }

            // RELIABILITY FIX: Check max polling iterations (calculated from timeout)
            if iterations > max_polling_iterations {
                return Err(ChainError::Provider(format!(
                    "Max polling iterations ({}) exceeded while waiting for {} confirmations. \
                     Transaction may be stuck or network may be congested.",
                    max_polling_iterations, confirmations
                )));
            }

            // Try to get receipt
            match self.get_transaction_receipt(tx_hash).await {
                Ok(Some(receipt)) => {
                    if let Some(block_number) = receipt.block_number {
                        match self.get_block_number().await {
                            Ok(current_block) => {
                                if current_block >= block_number + confirmations {
                                    tracing::info!(
                                        "Transaction confirmed after {} iterations ({}s)",
                                        iterations,
                                        start.elapsed().as_secs()
                                    );
                                    return Ok(receipt);
                                }
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to get current block number (iteration {}): {}",
                                    iterations,
                                    e
                                );
                            }
                        }
                    }
                }
                Ok(None) => {
                    // Transaction not yet mined, continue polling
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to get transaction receipt (iteration {}): {}",
                        iterations,
                        e
                    );
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(POLL_INTERVAL_MS)).await;
        }
    }
}

/// Trait for provider operations
#[async_trait]
pub trait Provider: Send + Sync {
    async fn get_block_number(&self) -> Result<u64, ChainError>;
    async fn get_balance(&self, address: Address) -> Result<U256, ChainError>;
    async fn get_gas_price(&self) -> Result<u128, ChainError>;
    async fn estimate_gas(&self, tx: &TransactionRequest) -> Result<u64, ChainError>;
    async fn send_transaction(&self, tx: TransactionRequest) -> Result<B256, ChainError>;
    async fn get_transaction_receipt(
        &self,
        tx_hash: B256,
    ) -> Result<Option<alloy_rpc_types::TransactionReceipt>, ChainError>;
}

#[async_trait]
impl Provider for AlloyClient {
    async fn get_block_number(&self) -> Result<u64, ChainError> {
        self.get_block_number().await
    }

    async fn get_balance(&self, address: Address) -> Result<U256, ChainError> {
        self.get_balance(address).await
    }

    async fn get_gas_price(&self) -> Result<u128, ChainError> {
        self.get_gas_price().await
    }

    async fn estimate_gas(&self, tx: &TransactionRequest) -> Result<u64, ChainError> {
        self.estimate_gas(tx).await
    }

    async fn send_transaction(&self, tx: TransactionRequest) -> Result<B256, ChainError> {
        self.send_transaction(tx).await
    }

    async fn get_transaction_receipt(
        &self,
        tx_hash: B256,
    ) -> Result<Option<alloy_rpc_types::TransactionReceipt>, ChainError> {
        self.get_transaction_receipt(tx_hash).await
    }
}
