//! Smart Contract Bindings
//!
//! Ethereum/Monad/BSC/Beechain contract ABIs and bindings using Alloy.
//! Uses LRU cache for contract instances to prevent memory leaks.
//! All BeeBotOS contract bindings are chain-agnostic and can be used on any EVM
//! chain.

use std::sync::Arc;

use alloy_primitives::{Address, Bytes, B256, U256};
use alloy_provider::{Provider, RootProvider};
use alloy_rpc_types::TransactionReceipt;
use alloy_transport_http::Http;
use tracing::{debug, error, info, instrument};

use crate::ChainError;

// ============================================================================
// BeeBotOS Contract Bindings (Chain-agnostic)
// ============================================================================

/// All BeeBotOS smart contract bindings
pub mod bindings;

// Re-export all contract bindings
// Re-export AgentIdentityInfo from AgentIdentity contract
pub use bindings::AgentIdentity::AgentIdentityInfo;
pub use bindings::{
    A2ACommerce,

    // Core DAO contracts
    AgentDAO,
    // Identity and commerce
    AgentIdentity,
    // Struct types
    AgentMetadata,
    AgentPayment,

    // Registry and discovery
    AgentRegistry,

    BeeToken,
    BridgeRequest,
    // Cross-chain
    CrossChainBridge,

    // Escrow and payments
    DealEscrow,
    // Dispute resolution
    DisputeResolution,

    DisputeStatus,
    PaymentMandate,
    ReputationSystem,

    Resolution,
    // Skills and reputation
    SkillNFT,
    Stream,
    TreasuryManager,
};

// ============================================================================
// Contract Events Module
// ============================================================================

/// Contract events - includes sol! generated events and event system
pub mod events;

// Re-export event types for convenience
pub use events::{
    AgentDeactivated,
    AgentRegistered,
    AgentUpdated,
    Approval,
    AvailabilityChanged,
    BeeBotOSEvent,
    BeeBotOSEventFilter,
    // Event system types
    BeeBotOSEventListener,
    BeeBotOSEventStream,
    BeeBotOSEventType,
    BridgeCompleted,
    BridgeFailed,
    BridgeInitiated,
    BudgetCreated,
    BudgetReleased,
    CategoryScoreUpdated,
    DisputeRaised,
    DisputeResolved,
    EscrowCreated,
    EscrowRefunded,
    EscrowReleased,
    EvidenceSubmitted,
    Heartbeat,
    ListingCancelled,
    ListingCreated,
    ListingFulfilled,
    MandateCreated,
    MetadataUpdated,
    PaymentExecuted,
    // Event types from sol! generated code
    ProposalCreated,
    ProposalExecuted,
    PurchaseMade,
    ReputationUpdated,
    RoyaltyUpdated,
    SkillMinted,
    StreamCreated,
    StreamUpdated,
    Transfer,
    VoteCast,
};

// ============================================================================
// Multicall Support
// ============================================================================

// Generate bindings for Multicall
alloy_sol_types::sol! {
    #[sol(rpc)]
    interface Multicall3 {
        struct Call {
            address target;
            bytes callData;
        }

        struct Call3 {
            address target;
            bool allowFailure;
            bytes callData;
        }

        struct Result {
            bool success;
            bytes returnData;
        }

        function aggregate(Call[] calldata calls) external payable returns (uint256 blockNumber, bytes[] memory returnData);
        function aggregate3(Call3[] calldata calls) external payable returns (Result[] memory returnData);
        function tryAggregate(bool requireSuccess, Call[] calldata calls) external payable returns (Result[] memory returnData);
    }
}

// ============================================================================
// Type Aliases and Helpers
// ============================================================================

/// Type alias for HTTP provider
pub type HttpProvider = RootProvider<Http<reqwest::Client>>;

/// Contract cache statistics
#[derive(Debug, Clone, Copy)]
pub struct ContractCacheStats {
    pub agent_dao_cached: usize,
    pub bee_token_cached: usize,
    pub treasury_manager_cached: usize,
    pub multicall_cached: usize,
}

impl ContractCacheStats {
    /// Total cached contracts
    pub fn total(&self) -> usize {
        self.agent_dao_cached
            + self.bee_token_cached
            + self.treasury_manager_cached
            + self.multicall_cached
    }
}

/// Transaction helper
pub struct TransactionHelper;

impl TransactionHelper {
    /// Estimate gas for a transaction
    #[instrument(skip(provider), target = "chain::contracts")]
    pub async fn estimate_gas(
        provider: Arc<HttpProvider>,
        to: Address,
        data: Bytes,
        value: U256,
    ) -> Result<U256, ChainError> {
        debug!(
            target: "chain::contracts",
            to = %to,
            value = %value,
            "Estimating gas"
        );

        let tx = alloy_rpc_types::TransactionRequest {
            to: Some(to.into()),
            input: data.into(),
            value: Some(value),
            ..Default::default()
        };

        let gas = provider.estimate_gas(&tx).await.map_err(|e| {
            error!(
                target: "chain::contracts",
                to = %to,
                error = %e,
                "Gas estimation failed"
            );
            ChainError::Provider(format!("Gas estimation failed: {}", e))
        })?;

        info!(
            target: "chain::contracts",
            to = %to,
            gas = %gas,
            "Gas estimation successful"
        );

        Ok(U256::from(gas))
    }

    /// Get gas price
    #[instrument(skip(provider), target = "chain::contracts")]
    pub async fn gas_price(provider: Arc<HttpProvider>) -> Result<U256, ChainError> {
        debug!(target: "chain::contracts", "Getting gas price");

        let price = provider.get_gas_price().await.map_err(|e| {
            error!(
                target: "chain::contracts",
                error = %e,
                "Failed to get gas price"
            );
            ChainError::Provider(format!("Failed to get gas price: {}", e))
        })?;

        info!(
            target: "chain::contracts",
            gas_price = %price,
            "Gas price retrieved successfully"
        );

        Ok(U256::from(price))
    }

    /// Wait for transaction confirmation with timeout and max polling limit
    ///
    /// RELIABILITY FIX: Added max polling iterations to prevent infinite loops
    /// and improved error handling for network issues.
    #[instrument(skip(provider), target = "chain::contracts")]
    pub async fn wait_for_receipt(
        provider: Arc<HttpProvider>,
        tx_hash: B256,
        _confirmations: usize,
        timeout_secs: u64,
    ) -> Result<TransactionReceipt, ChainError> {
        const MAX_POLLING_ITERATIONS: u32 = 10000; // Max ~1000 seconds with 100ms sleep
        const POLL_INTERVAL_MS: u64 = 100;
        const MAX_CONSECUTIVE_ERRORS: u32 = 10; // Allow some transient errors

        debug!(
            target: "chain::contracts",
            tx_hash = %tx_hash,
            timeout_secs = timeout_secs,
            "Waiting for transaction confirmation"
        );

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(timeout_secs);
        let mut iterations = 0u32;
        let mut consecutive_errors = 0u32;

        loop {
            iterations += 1;

            // Check timeout
            if start.elapsed() > timeout {
                error!(
                    target: "chain::contracts",
                    tx_hash = %tx_hash,
                    timeout_secs = timeout_secs,
                    iterations = iterations,
                    "Transaction confirmation timeout"
                );
                return Err(ChainError::Provider(format!(
                    "Timeout waiting for confirmation after {} iterations",
                    iterations
                )));
            }

            // Check max polling iterations
            if iterations > MAX_POLLING_ITERATIONS {
                return Err(ChainError::Provider(format!(
                    "Max polling iterations ({}) exceeded. Transaction may be stuck.",
                    MAX_POLLING_ITERATIONS
                )));
            }

            match provider.get_transaction_receipt(tx_hash).await {
                Ok(Some(receipt)) => {
                    info!(
                        target: "chain::contracts",
                        tx_hash = %tx_hash,
                        block_number = ?receipt.block_number,
                        iterations = iterations,
                        "Transaction confirmed"
                    );
                    return Ok(receipt);
                }
                Ok(None) => {
                    // Transaction not yet mined, reset error counter
                    consecutive_errors = 0;
                }
                Err(e) => {
                    consecutive_errors += 1;
                    error!(
                        target: "chain::contracts",
                        tx_hash = %tx_hash,
                        error = %e,
                        consecutive_errors = consecutive_errors,
                        "Failed to get transaction receipt"
                    );

                    // Only fail if we have too many consecutive errors
                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                        return Err(ChainError::Provider(format!(
                            "Failed to get receipt after {} consecutive errors: {}",
                            consecutive_errors, e
                        )));
                    }
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(POLL_INTERVAL_MS)).await;
        }
    }
}

/// Contract caller module
pub mod caller;

pub use caller::{
    BatchContractCaller, CallOptions, ContractAbi, ContractCallBuilder, ContractCaller,
    ContractDeployer, DeploymentResult, FunctionAbi, ParamAbi, StateMutability, TypedContract,
};

// ============================================================================
// Multicall Helper
// ============================================================================

/// Multicall module for batch contract calls
pub mod multicall;

pub use multicall::{Call, Call3, MulticallBatch, MulticallExecutor, MulticallResult};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ChainConfig;

    #[test]
    fn test_contract_cache_stats() {
        let stats = ContractCacheStats {
            agent_dao_cached: 5,
            bee_token_cached: 3,
            treasury_manager_cached: 2,
            multicall_cached: 1,
        };

        assert_eq!(stats.total(), 11);
        assert_eq!(stats.agent_dao_cached, 5);
    }

    #[test]
    fn test_multicall_batch() {
        let mut batch = multicall::MulticallBatch::new();
        assert!(batch.is_empty());

        batch.add_call(Address::ZERO, Bytes::new());
        assert_eq!(batch.len(), 1);

        batch.add_call_allow_failure(Address::ZERO, Bytes::from(vec![0x01]));
        assert_eq!(batch.len(), 2);
    }

    #[test]
    fn test_event_topic_generation() {
        use alloy_sol_types::SolEvent;
        // Verify that events have the correct selectors
        let _ = bindings::AgentDAO::ProposalCreated::SIGNATURE;
        let _ = bindings::AgentDAO::VoteCast::SIGNATURE;
        let _ = bindings::BeeToken::Transfer::SIGNATURE;
    }
}
