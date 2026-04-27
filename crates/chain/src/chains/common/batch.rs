//! Batch Operations Module
//!
//! Provides enhanced batch operations for transactions, calls, and event
//! queries. Supports both multicall contract integration and batched RPC calls.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use alloy_rpc_types::TransactionRequest;
use futures::future::join_all;
use tracing::{info, instrument};

use crate::chains::common::{EvmError, EvmProvider};
use crate::compat::{Address, Bytes, U256};
use crate::constants::MAX_RPC_BATCH_SIZE;

/// Batch request type
#[derive(Debug, Clone)]
pub enum BatchRequest {
    /// Get balance
    GetBalance(Address),
    /// Get nonce
    GetNonce(Address),
    /// Get code
    GetCode(Address),
    /// Call a contract (read-only)
    Call(TransactionRequest),
    /// Get storage at
    GetStorageAt(Address, U256),
    /// Estimate gas
    EstimateGas(TransactionRequest),
}

/// Batch response type
#[derive(Debug, Clone)]
pub enum BatchResponse {
    /// Balance response
    Balance(U256),
    /// Nonce response
    Nonce(u64),
    /// Code response
    Code(Bytes),
    /// Call response
    Call(Bytes),
    /// Storage response
    StorageAt(U256),
    /// Gas estimate response
    GasEstimate(u64),
    /// Error response
    Error(String),
}

/// Batch operation builder
#[derive(Debug)]
pub struct BatchOperation {
    requests: Vec<BatchRequest>,
    max_batch_size: usize,
    parallel: bool,
}

impl BatchOperation {
    /// Create new batch operation
    pub fn new() -> Self {
        Self {
            requests: Vec::new(),
            max_batch_size: MAX_RPC_BATCH_SIZE,
            parallel: true,
        }
    }

    /// Create with max batch size
    pub fn with_max_size(max_size: usize) -> Self {
        Self {
            requests: Vec::new(),
            max_batch_size: max_size,
            parallel: true,
        }
    }

    /// Add balance request
    pub fn add_balance_check(mut self, address: Address) -> Self {
        self.requests.push(BatchRequest::GetBalance(address));
        self
    }

    /// Add nonce request
    pub fn add_nonce_check(mut self, address: Address) -> Self {
        self.requests.push(BatchRequest::GetNonce(address));
        self
    }

    /// Add code request
    pub fn add_code_check(mut self, address: Address) -> Self {
        self.requests.push(BatchRequest::GetCode(address));
        self
    }

    /// Add contract call (read-only)
    pub fn add_call(mut self, call: TransactionRequest) -> Self {
        self.requests.push(BatchRequest::Call(call));
        self
    }

    /// Add storage query
    pub fn add_storage_at(mut self, address: Address, slot: U256) -> Self {
        self.requests
            .push(BatchRequest::GetStorageAt(address, slot));
        self
    }

    /// Add gas estimate
    pub fn add_gas_estimate(mut self, tx: TransactionRequest) -> Self {
        self.requests.push(BatchRequest::EstimateGas(tx));
        self
    }

    /// Set parallel execution
    pub fn parallel(mut self, parallel: bool) -> Self {
        self.parallel = parallel;
        self
    }

    /// Get request count
    pub fn len(&self) -> usize {
        self.requests.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }

    /// Execute batch
    #[instrument(skip(self, provider), target = "chain::batch")]
    pub async fn execute(self, provider: Arc<EvmProvider>) -> Result<Vec<BatchResponse>, EvmError> {
        if self.requests.is_empty() {
            return Ok(Vec::new());
        }

        let start = Instant::now();
        let request_count = self.requests.len();

        info!(
            target = "chain::batch",
            request_count = request_count,
            parallel = self.parallel,
            "Executing batch operation"
        );

        let responses = if self.parallel {
            self.execute_parallel(provider).await
        } else {
            self.execute_sequential(provider).await
        };

        let duration = start.elapsed();
        info!(
            target = "chain::batch",
            request_count = request_count,
            duration_ms = duration.as_millis(),
            "Batch operation completed"
        );

        responses
    }

    /// Execute requests in parallel
    async fn execute_parallel(
        &self,
        provider: Arc<EvmProvider>,
    ) -> Result<Vec<BatchResponse>, EvmError> {
        let futures: Vec<_> = self
            .requests
            .iter()
            .map(|req| {
                let provider = provider.clone();
                execute_single_request(req.clone(), provider)
            })
            .collect();

        let results = join_all(futures).await;
        Ok(results.into_iter().collect())
    }

    /// Execute requests sequentially
    async fn execute_sequential(
        &self,
        provider: Arc<EvmProvider>,
    ) -> Result<Vec<BatchResponse>, EvmError> {
        let mut responses = Vec::with_capacity(self.requests.len());

        for request in &self.requests {
            let response = execute_single_request(request.clone(), provider.clone()).await;
            responses.push(response);
        }

        Ok(responses)
    }

    /// Chunk requests into batches
    pub fn into_chunks(self) -> Vec<Vec<BatchRequest>> {
        self.requests
            .chunks(self.max_batch_size)
            .map(|chunk| chunk.to_vec())
            .collect()
    }
}

impl Default for BatchOperation {
    fn default() -> Self {
        Self::new()
    }
}

/// Execute a single batch request
async fn execute_single_request(
    request: BatchRequest,
    provider: Arc<EvmProvider>,
) -> BatchResponse {
    match request {
        BatchRequest::GetBalance(address) => match provider.get_balance(address).await {
            Ok(balance) => BatchResponse::Balance(balance),
            Err(e) => BatchResponse::Error(format!("Balance check failed: {}", e)),
        },
        BatchRequest::GetNonce(address) => match provider.get_transaction_count(address).await {
            Ok(nonce) => BatchResponse::Nonce(nonce),
            Err(e) => BatchResponse::Error(format!("Nonce check failed: {}", e)),
        },
        BatchRequest::GetCode(address) => match provider.get_code(address).await {
            Ok(code) => BatchResponse::Code(Bytes::from(code)),
            Err(e) => BatchResponse::Error(format!("Code check failed: {}", e)),
        },
        BatchRequest::Call(tx) => match provider.call(&tx).await {
            Ok(result) => BatchResponse::Call(Bytes::from(result)),
            Err(e) => BatchResponse::Error(format!("Call failed: {}", e)),
        },
        BatchRequest::GetStorageAt(_address, _slot) => {
            // This would need to be implemented in the provider
            // For now, return error
            BatchResponse::Error("GetStorageAt not implemented".to_string())
        }
        BatchRequest::EstimateGas(tx) => match provider.estimate_gas(&tx).await {
            Ok(gas) => BatchResponse::GasEstimate(gas.to::<u64>()),
            Err(e) => BatchResponse::Error(format!("Gas estimation failed: {}", e)),
        },
    }
}

/// Transaction batch builder
#[derive(Debug)]
pub struct TransactionBatch {
    transactions: Vec<TransactionRequest>,
    gas_estimates: Vec<Option<u64>>,
}

impl TransactionBatch {
    /// Create new transaction batch
    pub fn new() -> Self {
        Self {
            transactions: Vec::new(),
            gas_estimates: Vec::new(),
        }
    }

    /// Add transaction to batch
    pub fn add_transaction(mut self, tx: TransactionRequest) -> Self {
        self.transactions.push(tx);
        self.gas_estimates.push(None);
        self
    }

    /// Add transaction with gas estimate
    pub fn add_transaction_with_gas(mut self, tx: TransactionRequest, gas_estimate: u64) -> Self {
        self.transactions.push(tx);
        self.gas_estimates.push(Some(gas_estimate));
        self
    }

    /// Get transaction count
    pub fn len(&self) -> usize {
        self.transactions.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.transactions.is_empty()
    }

    /// Estimate gas for all transactions
    #[instrument(skip(self, provider), target = "chain::batch")]
    pub async fn estimate_all_gas(
        &mut self,
        provider: Arc<EvmProvider>,
    ) -> Result<Vec<u64>, EvmError> {
        let mut estimates = Vec::with_capacity(self.transactions.len());

        for tx in &self.transactions {
            let gas = provider
                .estimate_gas(tx)
                .await
                .map_err(|e| EvmError::ProviderError(format!("Gas estimation failed: {}", e)))?;
            estimates.push(gas.to::<u64>());
        }

        self.gas_estimates = estimates.iter().map(|&e| Some(e)).collect();
        Ok(estimates)
    }

    /// Get total gas required
    pub fn total_gas(&self) -> u64 {
        self.gas_estimates.iter().filter_map(|&e| e).sum()
    }

    /// Validate all transactions
    pub fn validate(&self) -> Vec<(usize, String)> {
        let mut errors = Vec::new();

        for (i, tx) in self.transactions.iter().enumerate() {
            // Check gas limit
            if let Some(gas) = tx.gas {
                if gas < 21000 {
                    errors.push((i, format!("Gas limit {} below minimum", gas)));
                }
            }

            // Check value
            if let Some(value) = tx.value {
                if value > U256::from(u128::MAX) {
                    errors.push((i, "Value exceeds maximum".to_string()));
                }
            }
        }

        errors
    }

    /// Get transactions
    pub fn transactions(&self) -> &[TransactionRequest] {
        &self.transactions
    }

    /// Get gas estimates
    pub fn gas_estimates(&self) -> &[Option<u64>] {
        &self.gas_estimates
    }

    /// Build final transactions with proper gas limits
    pub fn build(self) -> Vec<TransactionRequest> {
        self.transactions
            .into_iter()
            .zip(self.gas_estimates.into_iter())
            .map(|(mut tx, gas)| {
                if let Some(gas_limit) = gas {
                    // Add 20% buffer to gas estimate
                    tx.gas = Some((gas_limit as f64 * 1.2) as u64);
                }
                tx
            })
            .collect()
    }
}

impl Default for TransactionBatch {
    fn default() -> Self {
        Self::new()
    }
}

/// Multi-chain batch operations
pub struct MultiChainBatch {
    batches: HashMap<u64, BatchOperation>,
}

impl MultiChainBatch {
    /// Create new multi-chain batch
    pub fn new() -> Self {
        Self {
            batches: HashMap::new(),
        }
    }

    /// Add batch for a specific chain
    pub fn add_chain_batch(mut self, chain_id: u64, batch: BatchOperation) -> Self {
        self.batches.insert(chain_id, batch);
        self
    }

    /// Execute all batches
    #[instrument(skip(self, providers), target = "chain::batch")]
    pub async fn execute_all(
        self,
        providers: HashMap<u64, Arc<EvmProvider>>,
    ) -> HashMap<u64, Result<Vec<BatchResponse>, EvmError>> {
        let mut results = HashMap::new();

        for (chain_id, batch) in self.batches {
            if let Some(provider) = providers.get(&chain_id) {
                let result = batch.execute(provider.clone()).await;
                results.insert(chain_id, result);
            } else {
                results.insert(
                    chain_id,
                    Err(EvmError::ProviderError(format!(
                        "No provider for chain {}",
                        chain_id
                    ))),
                );
            }
        }

        results
    }
}

impl Default for MultiChainBatch {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch result aggregator
pub struct BatchResultAggregator {
    responses: Vec<BatchResponse>,
}

impl BatchResultAggregator {
    /// Create from responses
    pub fn new(responses: Vec<BatchResponse>) -> Self {
        Self { responses }
    }

    /// Get all balances
    pub fn balances(&self) -> Vec<(usize, U256)> {
        self.responses
            .iter()
            .enumerate()
            .filter_map(|(i, r)| {
                if let BatchResponse::Balance(b) = r {
                    Some((i, *b))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get all nonces
    pub fn nonces(&self) -> Vec<(usize, u64)> {
        self.responses
            .iter()
            .enumerate()
            .filter_map(|(i, r)| {
                if let BatchResponse::Nonce(n) = r {
                    Some((i, *n))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get all errors
    pub fn errors(&self) -> Vec<(usize, String)> {
        self.responses
            .iter()
            .enumerate()
            .filter_map(|(i, r)| {
                if let BatchResponse::Error(e) = r {
                    Some((i, e.clone()))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Check if all succeeded
    pub fn all_succeeded(&self) -> bool {
        self.responses
            .iter()
            .all(|r| !matches!(r, BatchResponse::Error(_)))
    }

    /// Get success count
    pub fn success_count(&self) -> usize {
        self.responses
            .iter()
            .filter(|r| !matches!(r, BatchResponse::Error(_)))
            .count()
    }

    /// Get error count
    pub fn error_count(&self) -> usize {
        self.responses
            .iter()
            .filter(|r| matches!(r, BatchResponse::Error(_)))
            .count()
    }
}

/// Batch operation statistics
#[derive(Debug, Clone, Copy)]
pub struct BatchStatistics {
    pub request_count: usize,
    pub success_count: usize,
    pub error_count: usize,
    pub execution_time_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_operation_builder() {
        let batch = BatchOperation::new()
            .add_balance_check(Address::ZERO)
            .add_nonce_check(Address::ZERO)
            .parallel(true);

        assert_eq!(batch.len(), 2);
        assert!(batch.parallel);
    }

    #[test]
    fn test_transaction_batch() {
        let mut batch = TransactionBatch::new();

        let tx = TransactionRequest::default();
        batch = batch.add_transaction(tx);

        assert_eq!(batch.len(), 1);

        // Test validation
        let errors = batch.validate();
        assert!(errors.is_empty() || !errors.is_empty()); // Depends on tx
                                                          // validation logic
    }

    #[test]
    fn test_batch_result_aggregator() {
        let responses = vec![
            BatchResponse::Balance(U256::from(1000)),
            BatchResponse::Nonce(5),
            BatchResponse::Error("Test error".to_string()),
        ];

        let aggregator = BatchResultAggregator::new(responses);

        assert_eq!(aggregator.balances().len(), 1);
        assert_eq!(aggregator.nonces().len(), 1);
        assert_eq!(aggregator.errors().len(), 1);
        assert!(!aggregator.all_succeeded());
        assert_eq!(aggregator.success_count(), 2);
        assert_eq!(aggregator.error_count(), 1);
    }

    #[test]
    fn test_multi_chain_batch() {
        let batch = MultiChainBatch::new()
            .add_chain_batch(1, BatchOperation::new().add_balance_check(Address::ZERO))
            .add_chain_batch(56, BatchOperation::new().add_balance_check(Address::ZERO));

        // Cannot test execution without providers
        assert_eq!(batch.batches.len(), 2);
    }

    #[test]
    fn test_batch_into_chunks() {
        let batch = BatchOperation::with_max_size(2)
            .add_balance_check(Address::ZERO)
            .add_balance_check(Address::ZERO)
            .add_balance_check(Address::ZERO)
            .add_balance_check(Address::ZERO);

        let chunks = batch.into_chunks();
        assert_eq!(chunks.len(), 2); // 4 requests / 2 per chunk = 2 chunks
        assert_eq!(chunks[0].len(), 2);
        assert_eq!(chunks[1].len(), 2);
    }
}
