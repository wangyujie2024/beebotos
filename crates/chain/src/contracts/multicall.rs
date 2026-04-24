//! Multicall Module
//!
//! Provides batch contract call functionality using the Multicall3 contract.

use std::sync::Arc;

use alloy_primitives::U256;
use alloy_provider::Provider as AlloyProvider;
use tracing::{debug, error, instrument};

// Re-export the Multicall3 types from contracts module
pub use super::Multicall3;
use crate::compat::{Address, Bytes};
use crate::{ChainError, Result};

/// A single call in a multicall batch
#[derive(Debug, Clone)]
pub struct Call {
    pub target: Address,
    pub data: Bytes,
}

/// A single call with failure allowed
#[derive(Debug, Clone)]
pub struct Call3 {
    pub target: Address,
    pub allow_failure: bool,
    pub data: Bytes,
}

/// Result of a multicall
#[derive(Debug, Clone)]
pub struct MulticallResult {
    pub success: bool,
    pub return_data: Bytes,
}

/// Multicall executor that interacts with on-chain Multicall3 contract
pub struct MulticallExecutor<P: AlloyProvider> {
    provider: Arc<P>,
    multicall_address: Address,
}

impl<P: AlloyProvider + Clone> MulticallExecutor<P> {
    /// Create new multicall executor
    pub fn new(provider: Arc<P>, multicall_address: Address) -> Self {
        Self {
            provider,
            multicall_address,
        }
    }

    /// Execute aggregate3 call (allows individual calls to fail)
    #[instrument(skip(self, calls), target = "chain::multicall")]
    pub async fn aggregate3(&self, calls: Vec<Call3>) -> Result<Vec<MulticallResult>> {
        if calls.is_empty() {
            return Ok(Vec::new());
        }

        debug!(
            target: "chain::multicall",
            call_count = calls.len(),
            "Executing aggregate3 multicall"
        );

        // Convert calls to Multicall3 Call3 format
        let calls_3: Vec<Multicall3::Call3> = calls
            .into_iter()
            .map(|call| Multicall3::Call3 {
                target: call.target,
                allowFailure: call.allow_failure,
                callData: call.data,
            })
            .collect();

        // Create the contract instance
        let multicall = Multicall3::new(self.multicall_address, (*self.provider).clone());

        // Build and send the transaction
        let call = multicall.aggregate3(calls_3);

        match call.call().await {
            Ok(results) => {
                let multicall_results: Vec<MulticallResult> = results
                    .returnData
                    .into_iter()
                    .map(|result| MulticallResult {
                        success: result.success,
                        return_data: result.returnData,
                    })
                    .collect();

                debug!(
                    target: "chain::multicall",
                    result_count = multicall_results.len(),
                    "Multicall executed successfully"
                );

                Ok(multicall_results)
            }
            Err(e) => {
                error!(
                    target: "chain::multicall",
                    error = %e,
                    "Multicall execution failed"
                );
                Err(ChainError::Contract(format!("Multicall failed: {}", e)))
            }
        }
    }

    /// Execute aggregate call (all calls must succeed)
    #[instrument(skip(self, calls), target = "chain::multicall")]
    pub async fn aggregate(&self, calls: Vec<Call>) -> Result<(U256, Vec<Bytes>)> {
        if calls.is_empty() {
            return Ok((U256::ZERO, Vec::new()));
        }

        debug!(
            target: "chain::multicall",
            call_count = calls.len(),
            "Executing aggregate multicall"
        );

        // Convert calls to Multicall3 Call format
        let calls_inner: Vec<Multicall3::Call> = calls
            .into_iter()
            .map(|call| Multicall3::Call {
                target: call.target,
                callData: call.data,
            })
            .collect();

        // Create the contract instance
        let multicall = Multicall3::new(self.multicall_address, (*self.provider).clone());

        // Build and send the call
        let call = multicall.aggregate(calls_inner);

        match call.call().await {
            Ok(result) => {
                debug!(
                    target: "chain::multicall",
                    block_number = %result.blockNumber,
                    result_count = result.returnData.len(),
                    "Aggregate multicall executed successfully"
                );

                Ok((result.blockNumber, result.returnData))
            }
            Err(e) => {
                error!(
                    target: "chain::multicall",
                    error = %e,
                    "Aggregate multicall execution failed"
                );
                Err(ChainError::Contract(format!(
                    "Aggregate multicall failed: {}",
                    e
                )))
            }
        }
    }

    /// Execute tryAggregate call
    #[instrument(skip(self, calls), target = "chain::multicall")]
    pub async fn try_aggregate(
        &self,
        require_success: bool,
        calls: Vec<Call>,
    ) -> Result<Vec<MulticallResult>> {
        if calls.is_empty() {
            return Ok(Vec::new());
        }

        debug!(
            target: "chain::multicall",
            call_count = calls.len(),
            require_success,
            "Executing tryAggregate multicall"
        );

        // Convert calls to Multicall3 Call format
        let calls_inner: Vec<Multicall3::Call> = calls
            .into_iter()
            .map(|call| Multicall3::Call {
                target: call.target,
                callData: call.data,
            })
            .collect();

        // Create the contract instance
        let multicall = Multicall3::new(self.multicall_address, (*self.provider).clone());

        // Build and send the call
        let call = multicall.tryAggregate(require_success, calls_inner);

        match call.call().await {
            Ok(results) => {
                let multicall_results: Vec<MulticallResult> = results
                    .returnData
                    .into_iter()
                    .map(|result| MulticallResult {
                        success: result.success,
                        return_data: result.returnData,
                    })
                    .collect();

                debug!(
                    target: "chain::multicall",
                    result_count = multicall_results.len(),
                    "TryAggregate multicall executed successfully"
                );

                Ok(multicall_results)
            }
            Err(e) => {
                error!(
                    target: "chain::multicall",
                    error = %e,
                    "TryAggregate multicall execution failed"
                );
                Err(ChainError::Contract(format!(
                    "TryAggregate multicall failed: {}",
                    e
                )))
            }
        }
    }
}

/// Multicall batch builder with execution capabilities
pub struct MulticallBatch {
    calls: Vec<Call3>,
}

impl MulticallBatch {
    /// Create new empty batch
    pub fn new() -> Self {
        Self { calls: Vec::new() }
    }

    /// Add a call to the batch
    pub fn add_call(&mut self, target: Address, data: Bytes) -> &mut Self {
        self.calls.push(Call3 {
            target,
            allow_failure: false,
            data,
        });
        self
    }

    /// Add a call that can fail
    pub fn add_call_allow_failure(&mut self, target: Address, data: Bytes) -> &mut Self {
        self.calls.push(Call3 {
            target,
            allow_failure: true,
            data,
        });
        self
    }

    /// Get number of calls in batch
    pub fn len(&self) -> usize {
        self.calls.len()
    }

    /// Check if batch is empty
    pub fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }

    /// Execute the batch using the provided executor
    pub async fn execute<P: AlloyProvider + Clone>(
        &self,
        executor: &MulticallExecutor<P>,
    ) -> Result<Vec<MulticallResult>> {
        executor.aggregate3(self.calls.clone()).await
    }

    /// Execute and require all calls to succeed
    pub async fn execute_all_or_nothing<P: AlloyProvider + Clone>(
        &self,
        executor: &MulticallExecutor<P>,
    ) -> Result<Vec<Bytes>> {
        let results = self.execute(executor).await?;

        let mut return_data = Vec::with_capacity(results.len());
        for (i, result) in results.iter().enumerate() {
            if !result.success {
                error!(
                    target: "chain::multicall",
                    index = i,
                    "Call {} in batch failed", i
                );
                return Err(ChainError::Contract(format!("Call {} in batch failed", i)));
            }
            return_data.push(result.return_data.clone());
        }

        Ok(return_data)
    }
}

impl Default for MulticallBatch {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multicall_batch_builder() {
        let mut batch = MulticallBatch::new();
        assert!(batch.is_empty());

        batch.add_call(Address::ZERO, Bytes::new());
        assert_eq!(batch.len(), 1);

        batch.add_call_allow_failure(Address::ZERO, Bytes::from(vec![0x01, 0x02]));
        assert_eq!(batch.len(), 2);
    }

    #[test]
    fn test_multicall_result() {
        let result = MulticallResult {
            success: true,
            return_data: Bytes::from(vec![0x01, 0x02]),
        };
        assert!(result.success);
        assert_eq!(result.return_data.len(), 2);
    }
}
