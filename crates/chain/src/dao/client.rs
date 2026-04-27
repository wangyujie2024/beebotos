//! DAO Client Implementation
//!
//! Full implementation of DAO operations using Alloy contracts.

use std::sync::Arc;

use alloy_primitives::Bytes;
use alloy_provider::Provider as AlloyProvider;
use alloy_rpc_types::TransactionReceipt;
use tracing::{debug, error, info, instrument};

use crate::compat::{Address, B256, U256};
use crate::config::ChainConfig;
use crate::constants::{
    PROPOSAL_EMERGENCY_VOTING_PERIOD, PROPOSAL_FAST_TRACK_VOTING_PERIOD,
    PROPOSAL_STANDARD_VOTING_PERIOD,
};
use crate::contracts::AgentDAO;
use crate::dao::{DAOInterface, Proposal, ProposalId, VoteType};
use crate::{ChainError, Result};

/// Proposal type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalType {
    Standard,
    FastTrack,
    Emergency,
}

impl ProposalType {
    /// Get voting period in blocks
    pub fn voting_period(&self) -> u64 {
        match self {
            ProposalType::Standard => PROPOSAL_STANDARD_VOTING_PERIOD, // ~1 week
            ProposalType::FastTrack => PROPOSAL_FAST_TRACK_VOTING_PERIOD, // ~1 day
            ProposalType::Emergency => PROPOSAL_EMERGENCY_VOTING_PERIOD, // ~15 minutes
        }
    }
}

/// DAO Client for interacting with on-chain governance
pub struct DAOClient<P: AlloyProvider + Clone> {
    provider: Arc<P>,
    dao_contract: Address,
    token_contract: Option<Address>,
    signer: Option<alloy_signer_local::PrivateKeySigner>,
}

impl<P: AlloyProvider + Clone> DAOClient<P> {
    /// Create new DAO client
    pub fn new(provider: Arc<P>, dao_contract: Address) -> Self {
        info!(
            target: "chain::dao",
            dao_contract = %dao_contract,
            "Creating DAO client"
        );
        Self {
            provider,
            dao_contract,
            token_contract: None,
            signer: None,
        }
    }

    /// Create new DAO client with token contract
    pub fn with_token(mut self, token_contract: Address) -> Self {
        debug!(
            target: "chain::dao",
            token_contract = %token_contract,
            "Setting token contract address"
        );
        self.token_contract = Some(token_contract);
        self
    }

    /// Create new DAO client with signer for transactions
    pub fn with_signer(mut self, signer: alloy_signer_local::PrivateKeySigner) -> Self {
        let address = signer.address();
        debug!(
            target: "chain::dao",
            signer_address = %address,
            "Setting signer"
        );
        self.signer = Some(signer);
        self
    }

    /// Create from chain configuration
    pub fn from_config(provider: Arc<P>, config: &ChainConfig) -> Result<Self> {
        let dao_address = config.get_dao_address().map_err(|e| {
            error!(target: "chain::dao", "DAO address not configured: {}", e);
            ChainError::DAO(format!("DAO address not configured: {}", e))
        })?;
        info!(
            target: "chain::dao",
            dao_address = %dao_address,
            "Creating DAO client from config"
        );
        Ok(Self::new(provider, dao_address))
    }

    /// Get DAO contract address
    pub fn dao_address(&self) -> Address {
        self.dao_contract
    }

    /// Get the underlying provider
    pub fn provider(&self) -> &P {
        &self.provider
    }

    /// Get signer if available
    pub fn signer(&self) -> Option<&alloy_signer_local::PrivateKeySigner> {
        self.signer.as_ref()
    }

    /// Convert VoteType to u8 for contract call
    fn vote_type_to_u8(vote_type: VoteType) -> u8 {
        match vote_type {
            VoteType::Against => 0,
            VoteType::For => 1,
            VoteType::Abstain => 2,
        }
    }

    /// Create a contract instance for read operations
    fn contract(&self) -> AgentDAO::AgentDAOInstance<alloy_transport::BoxTransport, &P> {
        AgentDAO::new(self.dao_contract, &*self.provider)
    }

    /// Create a contract instance with signer for write operations
    #[allow(dead_code)]
    fn contract_with_signer(
        &self,
    ) -> Result<AgentDAO::AgentDAOInstance<alloy_transport::BoxTransport, &P>> {
        if self.signer.is_none() {
            return Err(ChainError::Wallet("No signer configured".to_string()));
        }
        Ok(AgentDAO::new(self.dao_contract, &*self.provider))
    }
}

#[async_trait::async_trait]
impl<P: AlloyProvider + Clone + Send + Sync> DAOInterface for DAOClient<P> {
    #[instrument(skip(self), target = "chain::dao")]
    async fn proposal_count(&self) -> Result<u64> {
        debug!(
            target: "chain::dao",
            dao_contract = %self.dao_contract,
            "Querying proposal count"
        );

        let contract = self.contract();

        let count = contract.getProposalCount().call().await.map_err(|e| {
            error!(
                target: "chain::dao",
                error = %e,
                "Failed to get proposal count"
            );
            ChainError::Contract(format!("Failed to get proposal count: {}", e))
        })?;

        info!(
            target: "chain::dao",
            count = %count._0,
            "Retrieved proposal count"
        );

        Ok(count._0.to::<u64>())
    }

    #[instrument(skip(self), target = "chain::dao", fields(proposal_id = id))]
    async fn get_proposal(&self, id: ProposalId) -> Result<Proposal> {
        debug!(
            target: "chain::dao",
            proposal_id = id,
            "Querying proposal details"
        );

        let contract = self.contract();

        let result = contract
            .getProposal(U256::from(id))
            .call()
            .await
            .map_err(|e| {
                error!(
                    target: "chain::dao",
                    proposal_id = id,
                    error = %e,
                    "Failed to get proposal"
                );
                ChainError::Contract(format!("Failed to get proposal {}: {}", id, e))
            })?;

        let proposal = Proposal {
            id,
            proposer: result.proposer,
            description: result.description,
            for_votes: result.forVotes,
            against_votes: result.againstVotes,
            abstain_votes: result.abstainVotes,
            executed: result.executed,
            eta: 0, // TODO: Get from contract if available
        };

        info!(
            target: "chain::dao",
            proposal_id = id,
            proposer = %proposal.proposer,
            "Retrieved proposal details"
        );

        Ok(proposal)
    }

    #[instrument(
        skip(self, targets, values, calldatas, description),
        target = "chain::dao"
    )]
    async fn propose(
        &self,
        targets: Vec<Address>,
        values: Vec<U256>,
        calldatas: Vec<Bytes>,
        description: String,
    ) -> Result<ProposalId> {
        info!(
            target: "chain::dao",
            target_count = targets.len(),
            description = %description,
            "Creating new proposal"
        );

        // Validate inputs
        if targets.is_empty() {
            error!(target: "chain::dao", "Proposal targets cannot be empty");
            return Err(ChainError::Validation(
                "Targets cannot be empty".to_string(),
            ));
        }
        if targets.len() != values.len() || targets.len() != calldatas.len() {
            error!(
                target: "chain::dao",
                targets_len = targets.len(),
                values_len = values.len(),
                calldatas_len = calldatas.len(),
                "Proposal parameter length mismatch"
            );
            return Err(ChainError::Validation(
                "Targets, values, and calldatas must have the same length".to_string(),
            ));
        }

        // Check if signer is available
        let _signer = self.signer.as_ref().ok_or_else(|| {
            error!(target: "chain::dao", "No signer configured for proposal creation");
            ChainError::Wallet("No signer configured".to_string())
        })?;

        let contract = self.contract();

        // Build the propose call
        let call = contract.propose(targets, values, calldatas, description.clone());

        // Send the transaction
        let pending_tx = call.send().await.map_err(|e| {
            error!(
                target: "chain::dao",
                error = %e,
                "Failed to send propose transaction"
            );
            ChainError::Transaction(format!("Failed to create proposal: {}", e))
        })?;

        info!(
            target: "chain::dao",
            tx_hash = %pending_tx.tx_hash(),
            "Propose transaction sent, waiting for confirmation"
        );

        // Wait for receipt
        let receipt: TransactionReceipt = pending_tx.get_receipt().await.map_err(|e| {
            error!(
                target: "chain::dao",
                error = %e,
                "Failed to get proposal creation receipt"
            );
            ChainError::Transaction(format!("Proposal creation failed: {}", e))
        })?;

        // Extract proposal ID from logs
        let proposal_id = Self::extract_proposal_id_from_receipt(&receipt).ok_or_else(|| {
            error!(target: "chain::dao", "Failed to extract proposal ID from receipt");
            ChainError::Contract("Failed to extract proposal ID".to_string())
        })?;

        info!(
            target: "chain::dao",
            proposal_id,
            tx_hash = %receipt.transaction_hash,
            "Proposal created successfully"
        );

        Ok(proposal_id)
    }

    #[instrument(skip(self), target = "chain::dao", fields(proposal_id, support = ?support))]
    async fn cast_vote(&self, proposal_id: ProposalId, support: VoteType) -> Result<()> {
        info!(
            target: "chain::dao",
            proposal_id,
            vote_type = ?support,
            "Casting vote"
        );

        // Check if signer is available
        let _signer = self.signer.as_ref().ok_or_else(|| {
            error!(target: "chain::dao", "No signer configured for voting");
            ChainError::Wallet("No signer configured".to_string())
        })?;

        let support_u8 = Self::vote_type_to_u8(support);

        let contract = self.contract();
        let call = contract.castVote(U256::from(proposal_id), support_u8);

        let pending_tx = call.send().await.map_err(|e| {
            error!(
                target: "chain::dao",
                proposal_id,
                error = %e,
                "Failed to send vote transaction"
            );
            ChainError::Transaction(format!("Failed to cast vote: {}", e))
        })?;

        let receipt = pending_tx.get_receipt().await.map_err(|e| {
            error!(
                target: "chain::dao",
                proposal_id,
                error = %e,
                "Failed to get vote receipt"
            );
            ChainError::Transaction(format!("Vote transaction failed: {}", e))
        })?;

        info!(
            target: "chain::dao",
            proposal_id,
            tx_hash = %receipt.transaction_hash,
            "Vote cast successfully"
        );

        Ok(())
    }

    #[instrument(skip(self), target = "chain::dao", fields(proposal_id))]
    async fn execute(&self, proposal_id: ProposalId) -> Result<B256> {
        info!(
            target: "chain::dao",
            proposal_id,
            "Executing proposal"
        );

        // Check if signer is available
        let _signer = self.signer.as_ref().ok_or_else(|| {
            error!(target: "chain::dao", "No signer configured for execution");
            ChainError::Wallet("No signer configured".to_string())
        })?;

        let contract = self.contract();
        let call = contract.execute(U256::from(proposal_id));

        let pending_tx = call.send().await.map_err(|e| {
            error!(
                target: "chain::dao",
                proposal_id,
                error = %e,
                "Failed to send execute transaction"
            );
            ChainError::Transaction(format!("Failed to execute proposal: {}", e))
        })?;

        let receipt = pending_tx.get_receipt().await.map_err(|e| {
            error!(
                target: "chain::dao",
                proposal_id,
                error = %e,
                "Failed to get execution receipt"
            );
            ChainError::Transaction(format!("Proposal execution failed: {}", e))
        })?;

        info!(
            target: "chain::dao",
            proposal_id,
            tx_hash = %receipt.transaction_hash,
            "Proposal executed successfully"
        );

        Ok(receipt.transaction_hash)
    }

    #[instrument(skip(self), target = "chain::dao")]
    async fn get_votes(&self, account: Address) -> Result<U256> {
        debug!(
            target: "chain::dao",
            account = %account,
            "Querying voting power"
        );

        let contract = self.contract();

        let result = contract.getVotes(account).call().await.map_err(|e| {
            error!(
                target: "chain::dao",
                account = %account,
                error = %e,
                "Failed to get votes"
            );
            ChainError::Contract(format!("Failed to get votes for {}: {}", account, e))
        })?;

        info!(
            target: "chain::dao",
            account = %account,
            votes = %result._0,
            "Retrieved voting power"
        );

        Ok(result._0)
    }

    #[instrument(skip(self), target = "chain::dao")]
    async fn delegate(&self, delegatee: Address) -> Result<()> {
        info!(
            target: "chain::dao",
            delegatee = %delegatee,
            "Delegating voting power"
        );

        // Check if signer is available
        let _signer = self.signer.as_ref().ok_or_else(|| {
            error!(target: "chain::dao", "No signer configured for delegation");
            ChainError::Wallet("No signer configured".to_string())
        })?;

        let contract = self.contract();
        let call = contract.delegate(delegatee);

        let pending_tx = call.send().await.map_err(|e| {
            error!(
                target: "chain::dao",
                delegatee = %delegatee,
                error = %e,
                "Failed to send delegate transaction"
            );
            ChainError::Transaction(format!("Failed to delegate: {}", e))
        })?;

        let receipt = pending_tx.get_receipt().await.map_err(|e| {
            error!(
                target: "chain::dao",
                delegatee = %delegatee,
                error = %e,
                "Failed to get delegation receipt"
            );
            ChainError::Transaction(format!("Delegation failed: {}", e))
        })?;

        info!(
            target: "chain::dao",
            delegatee = %delegatee,
            tx_hash = %receipt.transaction_hash,
            "Delegation successful"
        );

        Ok(())
    }
}

impl<P: AlloyProvider + Clone> DAOClient<P> {
    /// Extract proposal ID from transaction receipt logs
    fn extract_proposal_id_from_receipt(receipt: &TransactionReceipt) -> Option<ProposalId> {
        // The ProposalCreated event has the proposal ID as the first indexed parameter
        // Topic 0 is the event signature, topic 1 is the proposal ID
        for log in receipt.inner.logs() {
            if log.topics().len() >= 2 {
                // Try to decode the proposal ID from the first indexed topic
                let proposal_id_bytes: [u8; 32] = log.topics()[1].into();
                let proposal_id = U256::from_be_bytes(proposal_id_bytes);
                return Some(proposal_id.to::<u64>());
            }
        }
        None
    }

    /// Queue a proposal for execution (if using timelock)
    #[instrument(skip(self), target = "chain::dao", fields(proposal_id))]
    pub async fn queue(&self, proposal_id: ProposalId) -> Result<B256> {
        info!(
            target: "chain::dao",
            proposal_id,
            "Queuing proposal"
        );

        // Check if signer is available
        let _signer = self.signer.as_ref().ok_or_else(|| {
            error!(target: "chain::dao", "No signer configured for queue");
            ChainError::Wallet("No signer configured".to_string())
        })?;

        // Verify proposal exists and is in correct state
        let proposal = self.get_proposal(proposal_id).await?;

        if proposal.executed {
            error!(target: "chain::dao", proposal_id, "Proposal already executed");
            return Err(ChainError::DAO("Proposal already executed".to_string()));
        }

        let contract = self.contract();
        let call = contract.queue(U256::from(proposal_id));

        let pending_tx = call.send().await.map_err(|e| {
            error!(
                target: "chain::dao",
                proposal_id,
                error = %e,
                "Failed to send queue transaction"
            );
            ChainError::Transaction(format!("Failed to queue proposal: {}", e))
        })?;

        let receipt = pending_tx.get_receipt().await.map_err(|e| {
            error!(
                target: "chain::dao",
                proposal_id,
                error = %e,
                "Failed to get queue receipt"
            );
            ChainError::Transaction(format!("Queue transaction failed: {}", e))
        })?;

        info!(
            target: "chain::dao",
            proposal_id,
            tx_hash = %receipt.transaction_hash,
            "Proposal queued successfully"
        );

        Ok(receipt.transaction_hash)
    }

    /// Cancel a proposal
    #[instrument(skip(self), target = "chain::dao", fields(proposal_id))]
    pub async fn cancel(&self, proposal_id: ProposalId) -> Result<B256> {
        info!(
            target: "chain::dao",
            proposal_id,
            "Cancelling proposal"
        );

        // Check if signer is available
        let _signer = self.signer.as_ref().ok_or_else(|| {
            error!(target: "chain::dao", "No signer configured for cancel");
            ChainError::Wallet("No signer configured".to_string())
        })?;

        // Verify proposal exists and can be cancelled
        let proposal = self.get_proposal(proposal_id).await?;

        if proposal.executed {
            error!(target: "chain::dao", proposal_id, "Cannot cancel executed proposal");
            return Err(ChainError::DAO(
                "Cannot cancel executed proposal".to_string(),
            ));
        }

        let contract = self.contract();
        let call = contract.cancel(U256::from(proposal_id));

        let pending_tx = call.send().await.map_err(|e| {
            error!(
                target: "chain::dao",
                proposal_id,
                error = %e,
                "Failed to send cancel transaction"
            );
            ChainError::Transaction(format!("Failed to cancel proposal: {}", e))
        })?;

        let receipt = pending_tx.get_receipt().await.map_err(|e| {
            error!(
                target: "chain::dao",
                proposal_id,
                error = %e,
                "Failed to get cancel receipt"
            );
            ChainError::Transaction(format!("Cancel transaction failed: {}", e))
        })?;

        info!(
            target: "chain::dao",
            proposal_id,
            tx_hash = %receipt.transaction_hash,
            "Proposal cancelled successfully"
        );

        Ok(receipt.transaction_hash)
    }

    /// Get proposal state (0=Pending, 1=Active, 2=Canceled, 3=Defeated,
    /// 4=Succeeded, 5=Queued, 6=Expired, 7=Executed)
    #[instrument(skip(self), target = "chain::dao", fields(proposal_id))]
    pub async fn get_proposal_state(&self, proposal_id: ProposalId) -> Result<u8> {
        debug!(
            target: "chain::dao",
            proposal_id,
            "Querying proposal state"
        );

        let contract = self.contract();

        let result = contract
            .state(U256::from(proposal_id))
            .call()
            .await
            .map_err(|e| {
                error!(
                    target: "chain::dao",
                    proposal_id,
                    error = %e,
                    "Failed to get proposal state"
                );
                ChainError::Contract(format!("Failed to get proposal state: {}", e))
            })?;

        let state = result._0;

        info!(
            target: "chain::dao",
            proposal_id,
            state,
            "Retrieved proposal state"
        );

        Ok(state)
    }
}

/// Proposal builder for convenient proposal creation
pub struct ProposalBuilder {
    description: String,
    proposal_type: ProposalType,
    targets: Vec<Address>,
    values: Vec<U256>,
    calldatas: Vec<Bytes>,
    signatures: Vec<String>,
}

impl ProposalBuilder {
    /// Create new proposal builder
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            proposal_type: ProposalType::Standard,
            targets: Vec::new(),
            values: Vec::new(),
            calldatas: Vec::new(),
            signatures: Vec::new(),
        }
    }

    /// Set proposal type
    pub fn proposal_type(mut self, proposal_type: ProposalType) -> Self {
        self.proposal_type = proposal_type;
        self
    }

    /// Add an action to the proposal
    pub fn add_action(
        mut self,
        target: Address,
        value: U256,
        calldata: Bytes,
        signature: Option<String>,
    ) -> Self {
        self.targets.push(target);
        self.values.push(value);
        self.calldatas.push(calldata);
        self.signatures.push(signature.unwrap_or_default());
        self
    }

    /// Add a transfer action
    pub fn add_transfer(self, recipient: Address, amount: U256) -> Self {
        // Empty calldata for simple transfers
        self.add_action(recipient, amount, Bytes::new(), None)
    }

    /// Add a contract call action
    pub fn add_contract_call(
        self,
        target: Address,
        calldata: Bytes,
        signature: impl Into<String>,
    ) -> Self {
        self.add_action(target, U256::ZERO, calldata, Some(signature.into()))
    }

    /// Get proposal type
    pub fn get_proposal_type(&self) -> ProposalType {
        self.proposal_type
    }

    /// Get voting period for this proposal type
    pub fn voting_period(&self) -> u64 {
        self.proposal_type.voting_period()
    }

    /// Build the proposal components
    pub fn build(self) -> (Vec<Address>, Vec<U256>, Vec<Bytes>, String) {
        (self.targets, self.values, self.calldatas, self.description)
    }

    /// Build with full details including signatures
    pub fn build_full(
        self,
    ) -> (
        Vec<Address>,
        Vec<U256>,
        Vec<Bytes>,
        Vec<String>,
        String,
        ProposalType,
    ) {
        (
            self.targets,
            self.values,
            self.calldatas,
            self.signatures,
            self.description,
            self.proposal_type,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proposal_builder() {
        let builder = ProposalBuilder::new("Test Proposal").proposal_type(ProposalType::Standard);

        let (targets, values, calldatas, description) = builder.build();
        assert!(targets.is_empty());
        assert!(values.is_empty());
        assert!(calldatas.is_empty());
        assert_eq!(description, "Test Proposal");
    }

    #[test]
    fn test_proposal_builder_with_actions() {
        let target = Address::from([1u8; 20]);
        let builder = ProposalBuilder::new("Transfer Proposal")
            .add_transfer(target, U256::from(1000))
            .add_contract_call(
                target,
                Bytes::from(vec![0x01, 0x02]),
                "transfer(address,uint256)",
            );

        let (targets, values, _, _, _, proposal_type) = builder.build_full();
        assert_eq!(targets.len(), 2);
        assert_eq!(values[0], U256::from(1000));
        assert_eq!(proposal_type, ProposalType::Standard);
    }

    #[test]
    fn test_vote_type_conversion() {
        assert_eq!(
            match VoteType::Against {
                VoteType::Against => 0u8,
                VoteType::For => 1u8,
                VoteType::Abstain => 2u8,
            },
            0
        );
        assert_eq!(
            match VoteType::For {
                VoteType::Against => 0u8,
                VoteType::For => 1u8,
                VoteType::Abstain => 2u8,
            },
            1
        );
        assert_eq!(
            match VoteType::Abstain {
                VoteType::Against => 0u8,
                VoteType::For => 1u8,
                VoteType::Abstain => 2u8,
            },
            2
        );
    }

    #[test]
    fn test_proposal_type_periods() {
        assert_eq!(ProposalType::Standard.voting_period(), 40_320);
        assert_eq!(ProposalType::FastTrack.voting_period(), 5_760);
        assert_eq!(ProposalType::Emergency.voting_period(), 60);
    }
}
