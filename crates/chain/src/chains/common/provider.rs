//! Generic EVM Provider
//!
//! Provides chain-agnostic provider functionality for EVM chains.

use std::sync::Arc;
use std::time::Duration;

use alloy_provider::Provider;
use alloy_rpc_types::TransactionRequest;
use tokio::sync::RwLock;

use crate::compat::{Address, U256};
use crate::contracts::HttpProvider;

/// Generic EVM provider wrapper
#[derive(Debug, Clone)]
pub struct EvmProvider {
    inner: Arc<HttpProvider>,
    chain_id: u64,
    request_timeout: Duration,
}

impl EvmProvider {
    /// Create new provider
    pub fn new(provider: HttpProvider, chain_id: u64) -> Self {
        Self {
            inner: Arc::new(provider),
            chain_id,
            request_timeout: Duration::from_secs(30),
        }
    }

    /// Create from RPC URL
    pub async fn from_url(rpc_url: &str, chain_id: u64) -> crate::Result<Self> {
        use alloy_rpc_client::RpcClient;
        use alloy_transport_http::Http;
        use reqwest::Client;

        let _client = Client::new();
        let http = Http::new(
            rpc_url
                .parse()
                .map_err(|e| crate::ChainError::Provider(format!("Invalid URL: {}", e)))?,
        );
        let client = RpcClient::new(http, true);
        let provider = HttpProvider::new(client);
        Ok(Self::new(provider, chain_id))
    }

    /// Get inner provider reference
    pub fn inner(&self) -> &HttpProvider {
        &self.inner
    }

    /// Get chain ID
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Set request timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Estimate gas for a transaction
    pub async fn estimate_gas(&self, tx: &TransactionRequest) -> crate::Result<U256> {
        let gas =
            self.inner.estimate_gas(tx).await.map_err(|e| {
                crate::ChainError::Provider(format!("Gas estimation failed: {}", e))
            })?;
        Ok(U256::from(gas))
    }

    /// Get account balance
    pub async fn get_balance(&self, address: Address) -> crate::Result<U256> {
        let balance =
            self.inner.get_balance(address).await.map_err(|e| {
                crate::ChainError::Provider(format!("Failed to get balance: {}", e))
            })?;
        Ok(balance)
    }

    /// Get current block number
    pub async fn get_block_number(&self) -> crate::Result<u64> {
        let block = self.inner.get_block_number().await.map_err(|e| {
            crate::ChainError::Provider(format!("Failed to get block number: {}", e))
        })?;
        Ok(block)
    }

    /// Get transaction count (nonce)
    pub async fn get_transaction_count(&self, address: Address) -> crate::Result<u64> {
        let count = self
            .inner
            .get_transaction_count(address)
            .await
            .map_err(|e| {
                crate::ChainError::Provider(format!("Failed to get transaction count: {}", e))
            })?;
        Ok(count)
    }

    /// Get gas price
    pub async fn get_gas_price(&self) -> crate::Result<u128> {
        let price =
            self.inner.get_gas_price().await.map_err(|e| {
                crate::ChainError::Provider(format!("Failed to get gas price: {}", e))
            })?;
        Ok(price)
    }

    /// Get transaction receipt
    pub async fn get_transaction_receipt(
        &self,
        tx_hash: alloy_primitives::B256,
    ) -> crate::Result<Option<alloy_rpc_types::TransactionReceipt>> {
        let receipt = self
            .inner
            .get_transaction_receipt(tx_hash)
            .await
            .map_err(|e| crate::ChainError::Provider(format!("Failed to get receipt: {}", e)))?;
        Ok(receipt)
    }

    /// Call a contract (read-only)
    pub async fn call(&self, call: &TransactionRequest) -> crate::Result<Vec<u8>> {
        let result = self
            .inner
            .call(call)
            .await
            .map_err(|e| crate::ChainError::Provider(format!("Call failed: {}", e)))?;
        Ok(result.to_vec())
    }

    /// Get code at address
    pub async fn get_code(&self, address: Address) -> crate::Result<Vec<u8>> {
        let code = self
            .inner
            .get_code_at(address)
            .await
            .map_err(|e| crate::ChainError::Provider(format!("Failed to get code: {}", e)))?;
        Ok(code.to_vec())
    }

    /// Get fee history (EIP-1559)
    pub async fn get_fee_history(
        &self,
        block_count: u64,
        newest_block: alloy_rpc_types::BlockNumberOrTag,
        reward_percentiles: &[f64],
    ) -> crate::Result<alloy_rpc_types::FeeHistory> {
        let history = self
            .inner
            .get_fee_history(block_count, newest_block, reward_percentiles)
            .await
            .map_err(|e| {
                crate::ChainError::Provider(format!("Failed to get fee history: {}", e))
            })?;
        Ok(history)
    }
}

/// Provider pool for load balancing and failover
pub struct ProviderPool {
    providers: Vec<EvmProvider>,
    current: RwLock<usize>,
}

impl ProviderPool {
    /// Create new provider pool
    pub fn new(providers: Vec<EvmProvider>) -> Self {
        Self {
            providers,
            current: RwLock::new(0),
        }
    }

    /// Get next provider (round-robin)
    pub async fn next(&self) -> Option<EvmProvider> {
        if self.providers.is_empty() {
            return None;
        }
        let mut current = self.current.write().await;
        let provider = self.providers.get(*current).cloned()?;
        *current = (*current + 1) % self.providers.len();
        Some(provider)
    }
}

impl EvmProvider {
    /// Wait for transaction confirmation
    pub async fn wait_for_confirmation(
        &self,
        tx_hash: alloy_primitives::B256,
        _confirmation_blocks: u64,
        timeout: Duration,
    ) -> crate::Result<alloy_rpc_types::TransactionReceipt> {
        let start = std::time::Instant::now();

        loop {
            if start.elapsed() > timeout {
                return Err(crate::ChainError::Timeout(
                    "Transaction confirmation timeout".to_string(),
                ));
            }

            if let Some(receipt) = self
                .inner
                .get_transaction_receipt(tx_hash)
                .await
                .map_err(|e| crate::ChainError::Provider(e.to_string()))?
            {
                return Ok(receipt);
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    /// Get block by number
    pub async fn get_block(
        &self,
        block_number: u64,
    ) -> crate::Result<Option<alloy_rpc_types::Block>> {
        use alloy_provider::Provider;
        use alloy_rpc_types::BlockTransactionsKind;
        let block = self
            .inner
            .get_block(block_number.into(), BlockTransactionsKind::default())
            .await
            .map_err(|e| crate::ChainError::Provider(e.to_string()))?;
        Ok(block)
    }

    /// Get logs (events) matching a filter
    pub async fn get_logs(
        &self,
        filter: &alloy_rpc_types::Filter,
    ) -> crate::Result<Vec<alloy_rpc_types::Log>> {
        use alloy_provider::Provider;
        let logs = self
            .inner
            .get_logs(filter)
            .await
            .map_err(|e| crate::ChainError::Provider(format!("Failed to get logs: {}", e)))?;
        Ok(logs)
    }

    /// Send raw transaction
    pub async fn send_raw_transaction(
        &self,
        tx_bytes: &[u8],
    ) -> crate::Result<alloy_primitives::B256> {
        use alloy_provider::Provider;
        let pending = self
            .inner
            .send_raw_transaction(tx_bytes)
            .await
            .map_err(|e| crate::ChainError::Provider(e.to_string()))?;
        // Get the transaction hash from the pending transaction
        Ok(*pending.tx_hash())
    }
}
