//! Cross-Chain Bridge Client
//!
//! Unified implementation that aligns Rust client with Solidity
//! CrossChainBridge contract. Supports both lock/release mechanism (from
//! Solidity) and atomic swaps.

use std::sync::Arc;

use alloy_primitives::Bytes;
use alloy_provider::Provider as AlloyProvider;
use alloy_rpc_types::TransactionReceipt;
use tracing::{debug, error, info, instrument};

use crate::compat::{Address, B256, U256};
use crate::contracts::CrossChainBridge;
use crate::{ChainError, Result};

/// Bridge state (aligned with Solidity BridgeState enum)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BridgeState {
    Pending = 0,
    Locked = 1,
    Confirmed = 2,
    Completed = 3,
    Failed = 4,
}

impl From<u8> for BridgeState {
    fn from(value: u8) -> Self {
        match value {
            0 => BridgeState::Pending,
            1 => BridgeState::Locked,
            2 => BridgeState::Confirmed,
            3 => BridgeState::Completed,
            4 => BridgeState::Failed,
            _ => BridgeState::Failed,
        }
    }
}

/// Bridge request info (aligned with Solidity BridgeRequest struct)
#[derive(Debug, Clone)]
pub struct BridgeRequestInfo {
    pub request_id: B256,
    pub sender: Address,
    pub recipient: Address,
    pub amount: U256,
    pub token: Address,
    pub target_chain: u64,
    pub target_token: B256,
    pub state: BridgeState,
    pub timestamp: u64,
}

impl From<crate::contracts::bindings::CrossChainBridge::BridgeRequest> for BridgeRequestInfo {
    fn from(req: crate::contracts::bindings::CrossChainBridge::BridgeRequest) -> Self {
        Self {
            request_id: req.requestId,
            sender: req.sender,
            recipient: req.recipient,
            amount: req.amount,
            token: req.token,
            target_chain: req.targetChain.to::<u64>(),
            target_token: req.targetToken,
            state: BridgeState::from(req.state),
            timestamp: req.timestamp.to::<u64>(),
        }
    }
}

/// Cross-chain bridge client
pub struct BridgeClient<P: AlloyProvider + Clone> {
    provider: Arc<P>,
    bridge_contract: Address,
    signer: Option<alloy_signer_local::PrivateKeySigner>,
}

impl<P: AlloyProvider + Clone> BridgeClient<P> {
    /// Create new bridge client
    pub fn new(provider: Arc<P>, bridge_contract: Address) -> Self {
        info!(
            target: "chain::bridge",
            bridge_contract = %bridge_contract,
            "Creating bridge client"
        );
        Self {
            provider,
            bridge_contract,
            signer: None,
        }
    }

    /// Create with signer for write operations
    pub fn with_signer(mut self, signer: alloy_signer_local::PrivateKeySigner) -> Self {
        let address = signer.address();
        debug!(
            target: "chain::bridge",
            signer_address = %address,
            "Setting signer"
        );
        self.signer = Some(signer);
        self
    }

    /// Get bridge contract address
    pub fn bridge_address(&self) -> Address {
        self.bridge_contract
    }

    /// Get the underlying provider
    pub fn provider(&self) -> &P {
        &self.provider
    }

    /// Create bridge contract instance
    fn bridge_contract(
        &self,
    ) -> CrossChainBridge::CrossChainBridgeInstance<alloy_transport::BoxTransport, &P> {
        CrossChainBridge::new(self.bridge_contract, &*self.provider)
    }

    /// Initiate bridge out (lock assets)
    ///
    /// This aligns with Solidity's bridgeOut function:
    /// - Locks assets in the bridge contract
    /// - Creates a bridge request
    /// - Emits BridgeInitiated event
    #[instrument(
        skip(self, token, amount, target_chain, target_token, recipient),
        target = "chain::bridge"
    )]
    pub async fn bridge_out(
        &self,
        token: Address,
        amount: U256,
        target_chain: u64,
        target_token: B256,
        recipient: Address,
    ) -> Result<B256> {
        info!(
            target: "chain::bridge",
            token = %token,
            amount = %amount,
            target_chain = target_chain,
            recipient = %recipient,
            "Initiating bridge out"
        );

        // Validate inputs
        if amount == U256::ZERO {
            return Err(ChainError::Validation(
                "Amount must be greater than 0".to_string(),
            ));
        }
        if recipient == Address::ZERO {
            return Err(ChainError::Validation(
                "Invalid recipient address".to_string(),
            ));
        }

        let _signer = self.signer.as_ref().ok_or_else(|| {
            error!(target: "chain::bridge", "No signer configured");
            ChainError::Wallet("No signer configured".to_string())
        })?;

        let contract = self.bridge_contract();

        // Build the bridge out call
        let call = contract.bridgeOut(
            token,
            amount,
            U256::from(target_chain),
            target_token,
            recipient,
        );

        // Determine if this is an ETH transfer (token == address(0))
        let is_eth_transfer = token == Address::ZERO;

        // Send the transaction
        let pending_tx = if is_eth_transfer {
            call.value(amount).send().await
        } else {
            call.send().await
        }
        .map_err(|e| {
            error!(
                target: "chain::bridge",
                error = %e,
                "Failed to send bridge out transaction"
            );
            ChainError::Transaction(format!("Failed to initiate bridge: {}", e))
        })?;

        info!(
            target: "chain::bridge",
            tx_hash = %pending_tx.tx_hash(),
            "Bridge out transaction sent, waiting for confirmation"
        );

        let receipt = pending_tx.get_receipt().await.map_err(|e| {
            error!(
                target: "chain::bridge",
                error = %e,
                "Failed to get bridge out receipt"
            );
            ChainError::Transaction(format!("Bridge out failed: {}", e))
        })?;

        // Extract request ID from receipt
        let request_id = Self::extract_request_id_from_receipt(&receipt).ok_or_else(|| {
            error!(target: "chain::bridge", "Failed to extract request ID from receipt");
            ChainError::Contract("Failed to extract request ID".to_string())
        })?;

        info!(
            target: "chain::bridge",
            request_id = %request_id,
            tx_hash = %receipt.transaction_hash,
            "Bridge out initiated successfully"
        );

        Ok(request_id)
    }

    /// Complete bridge in (release assets)
    ///
    /// This aligns with Solidity's bridgeIn function:
    /// - Verifies cross-chain proof
    /// - Releases assets to recipient
    /// - Emits BridgeCompleted event
    #[instrument(
        skip(self, request_id, recipient, amount, token, proof),
        target = "chain::bridge"
    )]
    pub async fn bridge_in(
        &self,
        request_id: B256,
        recipient: Address,
        amount: U256,
        token: Address,
        proof: Bytes,
    ) -> Result<B256> {
        info!(
            target: "chain::bridge",
            request_id = %request_id,
            recipient = %recipient,
            amount = %amount,
            "Completing bridge in"
        );

        let _signer = self.signer.as_ref().ok_or_else(|| {
            error!(target: "chain::bridge", "No signer configured");
            ChainError::Wallet("No signer configured".to_string())
        })?;

        let contract = self.bridge_contract();
        // proof is a single signature, wrap in Vec for the contract call
        let signatures = vec![proof];
        let call = contract.bridgeIn(request_id, recipient, amount, token, signatures);

        let pending_tx = call.send().await.map_err(|e| {
            error!(
                target: "chain::bridge",
                request_id = %request_id,
                error = %e,
                "Failed to send bridge in transaction"
            );
            ChainError::Transaction(format!("Failed to complete bridge: {}", e))
        })?;

        let receipt = pending_tx.get_receipt().await.map_err(|e| {
            error!(
                target: "chain::bridge",
                request_id = %request_id,
                error = %e,
                "Failed to get bridge in receipt"
            );
            ChainError::Transaction(format!("Bridge in failed: {}", e))
        })?;

        info!(
            target: "chain::bridge",
            request_id = %request_id,
            tx_hash = %receipt.transaction_hash,
            "Bridge in completed successfully"
        );

        Ok(receipt.transaction_hash)
    }

    /// Get bridge request details
    #[instrument(skip(self), target = "chain::bridge")]
    pub async fn get_request(&self, request_id: B256) -> Result<BridgeRequestInfo> {
        debug!(
            target: "chain::bridge",
            request_id = %request_id,
            "Querying bridge request"
        );

        let contract = self.bridge_contract();

        let result = contract.requests(request_id).call().await.map_err(|e| {
            error!(
                target: "chain::bridge",
                request_id = %request_id,
                error = %e,
                "Failed to get bridge request"
            );
            ChainError::Contract(format!("Failed to get bridge request: {}", e))
        })?;

        // Manual conversion from requestsReturn to BridgeRequestInfo
        let request_info = BridgeRequestInfo {
            request_id: result.requestId,
            sender: result.sender,
            recipient: result.recipient,
            amount: result.amount,
            token: result.token,
            target_chain: result.targetChain.to::<u64>(),
            target_token: result.targetToken,
            state: BridgeState::from(result.state),
            timestamp: result.timestamp.to::<u64>(),
        };

        debug!(
            target: "chain::bridge",
            request_id = %request_id,
            state = ?request_info.state,
            "Retrieved bridge request"
        );

        Ok(request_info)
    }

    /// Check if a request is completed
    #[instrument(skip(self), target = "chain::bridge")]
    pub async fn is_completed(&self, request_id: B256) -> Result<bool> {
        let contract = self.bridge_contract();

        let result = contract
            .completedRequests(request_id)
            .call()
            .await
            .map_err(|e| {
                error!(
                    target: "chain::bridge",
                    request_id = %request_id,
                    error = %e,
                    "Failed to check completion status"
                );
                ChainError::Contract(format!("Failed to check completion: {}", e))
            })?;

        Ok(result._0)
    }

    /// Check if a chain is supported
    #[instrument(skip(self), target = "chain::bridge")]
    pub async fn is_chain_supported(&self, chain_id: u64) -> Result<bool> {
        let contract = self.bridge_contract();

        let result = contract
            .supportedChains(U256::from(chain_id))
            .call()
            .await
            .map_err(|e| {
                error!(
                    target: "chain::bridge",
                    chain_id = chain_id,
                    error = %e,
                    "Failed to check chain support"
                );
                ChainError::Contract(format!("Failed to check chain support: {}", e))
            })?;

        Ok(result._0)
    }

    /// Check if a token is supported
    #[instrument(skip(self), target = "chain::bridge")]
    pub async fn is_token_supported(&self, token: Address) -> Result<bool> {
        let contract = self.bridge_contract();

        let result = contract.supportedTokens(token).call().await.map_err(|e| {
            error!(
                target: "chain::bridge",
                token = %token,
                error = %e,
                "Failed to check token support"
            );
            ChainError::Contract(format!("Failed to check token support: {}", e))
        })?;

        Ok(result._0)
    }

    /// Get current fee basis points
    #[instrument(skip(self), target = "chain::bridge")]
    pub async fn get_fee_basis_points(&self) -> Result<U256> {
        let contract = self.bridge_contract();

        let result = contract.feeBasisPoints().call().await.map_err(|e| {
            error!(
                target: "chain::bridge",
                error = %e,
                "Failed to get fee basis points"
            );
            ChainError::Contract(format!("Failed to get fee: {}", e))
        })?;

        Ok(result._0)
    }

    /// Calculate fee for a given amount
    pub async fn calculate_fee(&self, amount: U256) -> Result<U256> {
        let fee_bps = self.get_fee_basis_points().await?;
        let fee = amount * fee_bps / U256::from(10000);
        Ok(fee)
    }

    /// Refund a failed bridge request
    #[instrument(skip(self), target = "chain::bridge")]
    pub async fn refund(&self, request_id: B256) -> Result<B256> {
        info!(
            target: "chain::bridge",
            request_id = %request_id,
            "Requesting refund"
        );

        let _signer = self.signer.as_ref().ok_or_else(|| {
            error!(target: "chain::bridge", "No signer configured");
            ChainError::Wallet("No signer configured".to_string())
        })?;

        let contract = self.bridge_contract();
        let call = contract.refund(request_id);

        let pending_tx = call.send().await.map_err(|e| {
            error!(
                target: "chain::bridge",
                request_id = %request_id,
                error = %e,
                "Failed to send refund transaction"
            );
            ChainError::Transaction(format!("Failed to refund: {}", e))
        })?;

        let receipt = pending_tx.get_receipt().await.map_err(|e| {
            error!(
                target: "chain::bridge",
                request_id = %request_id,
                error = %e,
                "Failed to get refund receipt"
            );
            ChainError::Transaction(format!("Refund failed: {}", e))
        })?;

        info!(
            target: "chain::bridge",
            request_id = %request_id,
            tx_hash = %receipt.transaction_hash,
            "Refund processed successfully"
        );

        Ok(receipt.transaction_hash)
    }

    /// Verify cross-chain proof (pure function)
    ///
    /// NOTE: Currently returns true in Solidity, needs actual implementation
    pub async fn verify_proof(
        &self,
        request_id: B256,
        recipient: Address,
        amount: U256,
        token: Address,
        proof: Bytes,
    ) -> Result<bool> {
        let contract = self.bridge_contract();

        // proof is a single signature, wrap in Vec for the contract call
        let signatures = vec![proof];
        let result = contract
            .verifyCrossChainProof(request_id, recipient, amount, token, signatures)
            .call()
            .await
            .map_err(|e| {
                error!(
                    target: "chain::bridge",
                    request_id = %request_id,
                    error = %e,
                    "Failed to verify proof"
                );
                ChainError::Contract(format!("Proof verification failed: {}", e))
            })?;

        Ok(result._0)
    }

    /// Admin: Add supported chain
    #[instrument(skip(self), target = "chain::bridge")]
    pub async fn add_supported_chain(&self, chain_id: u64) -> Result<()> {
        info!(
            target: "chain::bridge",
            chain_id = chain_id,
            "Adding supported chain"
        );

        let _signer = self
            .signer
            .as_ref()
            .ok_or_else(|| ChainError::Wallet("No signer configured".to_string()))?;

        let contract = self.bridge_contract();
        let call = contract.addSupportedChain(U256::from(chain_id));

        let pending_tx = call.send().await.map_err(|e| {
            error!(
                target: "chain::bridge",
                chain_id = chain_id,
                error = %e,
                "Failed to add supported chain"
            );
            ChainError::Transaction(format!("Failed to add chain: {}", e))
        })?;

        pending_tx.get_receipt().await.map_err(|e| {
            error!(
                target: "chain::bridge",
                chain_id = chain_id,
                error = %e,
                "Failed to get receipt"
            );
            ChainError::Transaction(format!("Add chain failed: {}", e))
        })?;

        info!(target: "chain::bridge", chain_id = chain_id, "Chain added successfully");
        Ok(())
    }

    /// Admin: Add supported token
    #[instrument(skip(self), target = "chain::bridge")]
    pub async fn add_supported_token(&self, token: Address) -> Result<()> {
        info!(
            target: "chain::bridge",
            token = %token,
            "Adding supported token"
        );

        let _signer = self
            .signer
            .as_ref()
            .ok_or_else(|| ChainError::Wallet("No signer configured".to_string()))?;

        let contract = self.bridge_contract();
        let call = contract.addSupportedToken(token);

        let pending_tx = call.send().await.map_err(|e| {
            error!(
                target: "chain::bridge",
                token = %token,
                error = %e,
                "Failed to add supported token"
            );
            ChainError::Transaction(format!("Failed to add token: {}", e))
        })?;

        pending_tx.get_receipt().await.map_err(|e| {
            error!(
                target: "chain::bridge",
                token = %token,
                error = %e,
                "Failed to get receipt"
            );
            ChainError::Transaction(format!("Add token failed: {}", e))
        })?;

        info!(target: "chain::bridge", token = %token, "Token added successfully");
        Ok(())
    }

    /// Extract request ID from transaction receipt
    fn extract_request_id_from_receipt(receipt: &TransactionReceipt) -> Option<B256> {
        // The BridgeInitiated event has the request ID as the first indexed parameter
        for log in receipt.inner.logs() {
            if log.topics().len() >= 2 {
                let request_id_bytes: [u8; 32] = log.topics()[1].into();
                return Some(request_id_bytes.into());
            }
        }
        None
    }
}

/// HTLC (Hash Time Locked Contract) for atomic swaps
///
/// This provides an alternative to the lock/release mechanism,
/// useful for peer-to-peer cross-chain swaps without a trusted bridge.
#[derive(Debug, Clone)]
pub struct HTLC {
    pub hash_lock: B256,
    pub time_lock: u64,
    pub initiator: Address,
    pub participant: Address,
    pub amount: U256,
    pub token: Address,
    pub claimed: bool,
    pub refunded: bool,
}

impl HTLC {
    /// Create new HTLC
    pub fn new(
        secret_hash: B256,
        time_lock: u64,
        initiator: Address,
        participant: Address,
        amount: U256,
        token: Address,
    ) -> Self {
        Self {
            hash_lock: secret_hash,
            time_lock,
            initiator,
            participant,
            amount,
            token,
            claimed: false,
            refunded: false,
        }
    }

    /// Generate hash lock from secret
    pub fn generate_hash_lock(secret: &B256) -> B256 {
        use alloy_primitives::keccak256;
        B256::from(keccak256(secret.as_slice()))
    }

    /// Verify secret matches hash lock
    pub fn verify_secret(&self, secret: &B256) -> bool {
        let hash = Self::generate_hash_lock(secret);
        hash == self.hash_lock
    }

    /// Check if HTLC is expired
    pub fn is_expired(&self, current_timestamp: u64) -> bool {
        current_timestamp > self.time_lock
    }
}

/// AtomicSwap type alias for backward compatibility
///
/// DEPRECATED: Use `HTLC` directly instead
pub type AtomicSwap = HTLC;

/// Atomic swap client for peer-to-peer cross-chain swaps
pub struct AtomicSwapClient;

impl AtomicSwapClient {
    /// Generate a secure random secret
    pub fn generate_secret() -> B256 {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut secret = [0u8; 32];
        rng.fill(&mut secret);
        B256::from(secret)
    }

    /// Initiate a swap (initiator side)
    pub fn initiate_swap(
        secret: &B256,
        time_lock: u64,
        initiator: Address,
        participant: Address,
        amount: U256,
        token: Address,
    ) -> HTLC {
        let hash_lock = HTLC::generate_hash_lock(secret);
        HTLC::new(hash_lock, time_lock, initiator, participant, amount, token)
    }

    /// Participate in a swap (participant side)
    ///
    /// The participant creates their HTLC with a shorter time lock
    pub fn participate_swap(
        hash_lock: B256,
        time_lock: u64,
        participant: Address,
        initiator: Address,
        amount: U256,
        token: Address,
    ) -> HTLC {
        // Participant's time lock should be shorter than initiator's
        HTLC::new(hash_lock, time_lock, participant, initiator, amount, token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_state_from_u8() {
        assert_eq!(BridgeState::from(0), BridgeState::Pending);
        assert_eq!(BridgeState::from(1), BridgeState::Locked);
        assert_eq!(BridgeState::from(2), BridgeState::Confirmed);
        assert_eq!(BridgeState::from(3), BridgeState::Completed);
        assert_eq!(BridgeState::from(4), BridgeState::Failed);
        assert_eq!(BridgeState::from(255), BridgeState::Failed); // Unknown defaults to Failed
    }

    #[test]
    fn test_htlc_secret_verification() {
        let secret = B256::from([1u8; 32]);
        let hash_lock = HTLC::generate_hash_lock(&secret);

        let htlc = HTLC::new(
            hash_lock,
            1000,
            Address::ZERO,
            Address::ZERO,
            U256::ZERO,
            Address::ZERO,
        );

        assert!(htlc.verify_secret(&secret));

        let wrong_secret = B256::from([2u8; 32]);
        assert!(!htlc.verify_secret(&wrong_secret));
    }

    #[test]
    fn test_htlc_expiration() {
        let htlc = HTLC::new(
            B256::ZERO,
            1000,
            Address::ZERO,
            Address::ZERO,
            U256::ZERO,
            Address::ZERO,
        );

        assert!(!htlc.is_expired(999));
        assert!(htlc.is_expired(1001));
    }

    #[test]
    fn test_atomic_swap_flow() {
        let secret = AtomicSwapClient::generate_secret();
        let initiator = Address::from([1u8; 20]);
        let participant = Address::from([2u8; 20]);

        // Initiator creates HTLC
        let htlc_initiator = AtomicSwapClient::initiate_swap(
            &secret,
            1000,
            initiator,
            participant,
            U256::from(1000),
            Address::ZERO,
        );

        assert_eq!(htlc_initiator.initiator, initiator);
        assert_eq!(htlc_initiator.participant, participant);
        assert!(htlc_initiator.verify_secret(&secret));

        // Participant creates HTLC with same hash lock but shorter time lock
        let htlc_participant = AtomicSwapClient::participate_swap(
            htlc_initiator.hash_lock,
            800, // Shorter time lock
            participant,
            initiator,
            U256::from(1000),
            Address::ZERO,
        );

        assert_eq!(htlc_participant.hash_lock, htlc_initiator.hash_lock);
        assert!(htlc_participant.verify_secret(&secret));
    }
}
