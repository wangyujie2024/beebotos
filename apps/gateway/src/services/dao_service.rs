//! DAO Service
//!
//! Manages DAO governance operations.
//! Separated from ChainService for better modularity.

use std::sync::Arc;

use beebotos_chain::compat::{Address, ChainClientTrait, ProposalInfo, TxHash, U256};
use beebotos_chain::dao::proposal::ProposalAction;
use beebotos_chain::dao::{ProposalId, ProposalType, VoteType};
use tracing::{info, instrument};

use super::wallet_service::WalletService;
use crate::config::BlockchainConfig;
use crate::error::AppError;

/// DAO service configuration
#[derive(Debug, Clone)]
pub struct DaoServiceConfig {
    /// Chain ID
    pub chain_id: u64,
    /// DAO contract address
    pub dao_contract: Option<Address>,
    /// Treasury contract address
    pub treasury_contract: Option<Address>,
}

impl From<&BlockchainConfig> for DaoServiceConfig {
    fn from(config: &BlockchainConfig) -> Self {
        let parse_address = |addr: &Option<String>| -> Option<Address> {
            addr.as_ref().and_then(|a| {
                let a = a.trim();
                if a.len() >= 42 && a.starts_with("0x") {
                    hex::decode(&a[2..])
                        .ok()
                        .filter(|b| b.len() == 20)
                        .map(|b| Address::from_slice(&b))
                } else {
                    None
                }
            })
        };

        Self {
            chain_id: config.chain_id,
            dao_contract: parse_address(&config.dao_contract_address),
            treasury_contract: None, // Not yet in config
        }
    }
}

/// Proposal creation request
#[derive(Debug, Clone)]
pub struct CreateProposalRequest {
    /// Proposal title
    pub title: String,
    /// Proposal description
    pub description: String,
    /// Proposal type
    pub proposal_type: ProposalType,
    /// Proposal action
    pub action: ProposalAction,
    /// Voting period in seconds
    pub voting_period_secs: u64,
}

/// Vote request
#[derive(Debug, Clone)]
pub struct CastVoteRequest {
    /// Proposal ID
    pub proposal_id: ProposalId,
    /// Vote type
    pub vote_type: VoteType,
    /// Voting power (for weighted voting)
    pub voting_power: Option<u64>,
}

/// DAO Service
pub struct DaoService {
    /// Configuration
    config: DaoServiceConfig,
    /// Chain client
    client: Option<Arc<dyn ChainClientTrait>>,
    /// Wallet service for transactions
    wallet_service: Arc<WalletService>,
}

impl DaoService {
    /// Create new DAO service
    pub async fn new(
        config: DaoServiceConfig,
        wallet_service: Arc<WalletService>,
    ) -> anyhow::Result<Self> {
        info!(
            chain_id = config.chain_id,
            has_dao_contract = config.dao_contract.is_some(),
            "Initializing DaoService"
        );

        // Initialize chain client
        let client = if wallet_service.has_client() {
            // Try to get client from wallet service
            // Note: In real implementation, we might want to share the client
            // For now, we'll create a new one or require it to be passed
            None
        } else {
            None
        };

        Ok(Self {
            config,
            client,
            wallet_service,
        })
    }

    /// Create DAO service with explicit client
    pub fn with_client(
        config: DaoServiceConfig,
        wallet_service: Arc<WalletService>,
        client: Option<Arc<dyn ChainClientTrait>>,
    ) -> Self {
        Self {
            config,
            client,
            wallet_service,
        }
    }

    /// Check if DAO contract is configured
    pub fn has_dao_contract(&self) -> bool {
        self.client.is_some() && self.config.dao_contract.is_some()
    }

    /// Create a new DAO proposal
    #[instrument(skip(self, request), fields(title = %request.title))]
    pub async fn create_proposal(
        &self,
        request: CreateProposalRequest,
    ) -> Result<ProposalId, AppError> {
        let _client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Chain client not initialized".into()))?;

        let dao_contract = self
            .config
            .dao_contract
            .as_ref()
            .ok_or_else(|| AppError::Configuration("DAO contract not configured".into()))?;

        if !self.wallet_service.has_wallet() {
            return Err(AppError::Configuration(
                "Wallet not initialized for signing".into(),
            ));
        }

        info!(
            title = %request.title,
            proposal_type = ?request.proposal_type,
            "Creating DAO proposal"
        );

        // Build transaction data for create proposal
        let tx_data = build_create_proposal_call(&request);

        // Send transaction via wallet service
        let tx_hash = self
            .wallet_service
            .send_contract_transaction(
                dao_contract.clone(),
                tx_data,
                beebotos_chain::compat::U256::from(0),
            )
            .await?;

        // Wait for receipt to get proposal ID from event
        let receipt = self.wallet_service.wait_for_receipt(tx_hash, 120).await?;

        if !receipt.success {
            return Err(AppError::Chain("Proposal creation failed".into()));
        }

        // Extract proposal ID from event (simplified)
        // In real implementation, parse event logs
        // ProposalId is u64, generate from tx_hash bytes
        let proposal_id = ProposalId::from_le_bytes([
            receipt.tx_hash[0],
            receipt.tx_hash[1],
            receipt.tx_hash[2],
            receipt.tx_hash[3],
            receipt.tx_hash[4],
            receipt.tx_hash[5],
            receipt.tx_hash[6],
            receipt.tx_hash[7],
        ]);

        info!(proposal_id = %proposal_id, "Proposal created");

        Ok(proposal_id)
    }

    /// Cast a vote on a proposal
    #[instrument(skip(self, request), fields(proposal_id = %request.proposal_id))]
    pub async fn cast_vote(&self, request: CastVoteRequest) -> Result<TxHash, AppError> {
        let _client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Chain client not initialized".into()))?;

        let dao_contract = self
            .config
            .dao_contract
            .as_ref()
            .ok_or_else(|| AppError::Configuration("DAO contract not configured".into()))?;

        if !self.wallet_service.has_wallet() {
            return Err(AppError::Configuration(
                "Wallet not initialized for signing".into(),
            ));
        }

        info!(
            proposal_id = %request.proposal_id,
            vote_type = ?request.vote_type,
            "Casting vote"
        );

        // Build transaction data
        let tx_data = build_cast_vote_call(&request);

        // Send transaction
        let tx_hash = self
            .wallet_service
            .send_contract_transaction(
                dao_contract.clone(),
                tx_data,
                beebotos_chain::compat::U256::from(0),
            )
            .await?;

        info!(
            tx_hash = %hex::encode(tx_hash.as_slice()),
            "Vote submitted"
        );

        Ok(tx_hash)
    }

    /// Get proposal information
    pub async fn get_proposal(&self, proposal_id: ProposalId) -> Result<ProposalInfo, AppError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Chain client not initialized".into()))?;

        let dao_contract = self
            .config
            .dao_contract
            .as_ref()
            .ok_or_else(|| AppError::Configuration("DAO contract not configured".into()))?;

        // Call contract to get proposal info
        let proposal = client
            .get_proposal(dao_contract.clone(), proposal_id)
            .await
            .map_err(|e| AppError::Chain(format!("Failed to get proposal: {}", e)))?;

        match proposal {
            Some(info) => Ok(info),
            None => Err(AppError::NotFound(format!("proposal: {}", proposal_id))),
        }
    }

    /// List all proposals
    pub async fn list_proposals(
        &self,
        _include_executed: bool,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ProposalInfo>, AppError> {
        let _client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Chain client not initialized".into()))?;

        let _dao_contract = self
            .config
            .dao_contract
            .as_ref()
            .ok_or_else(|| AppError::Configuration("DAO contract not configured".into()))?;

        // Get proposal count
        let count = self.get_proposal_count().await?;

        // Fetch proposals
        let mut proposals = Vec::new();
        let start = offset as u64;
        let end = std::cmp::min(start + limit as u64, count);

        for i in start..end {
            if let Ok(proposal) = self.get_proposal(i).await {
                // Include all proposals for now, filtering can be added later
                proposals.push(proposal);
            }
        }

        Ok(proposals)
    }

    /// List active (non-executed) proposals
    pub async fn list_active_proposals(&self) -> Result<Vec<ProposalInfo>, AppError> {
        self.list_proposals(false, 100, 0).await
    }

    /// Get total proposal count
    pub async fn get_proposal_count(&self) -> Result<u64, AppError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Chain client not initialized".into()))?;

        let dao_contract = self
            .config
            .dao_contract
            .as_ref()
            .ok_or_else(|| AppError::Configuration("DAO contract not configured".into()))?;

        client
            .get_proposal_count(dao_contract.clone())
            .await
            .map_err(|e| AppError::Chain(format!("Failed to get proposal count: {}", e)))
    }

    /// Execute a passed proposal
    pub async fn execute_proposal(&self, proposal_id: ProposalId) -> Result<TxHash, AppError> {
        let dao_contract = self
            .config
            .dao_contract
            .as_ref()
            .ok_or_else(|| AppError::Configuration("DAO contract not configured".into()))?;

        if !self.wallet_service.has_wallet() {
            return Err(AppError::Configuration(
                "Wallet not initialized for signing".into(),
            ));
        }

        info!(
            proposal_id = %proposal_id,
            "Executing proposal"
        );

        // Build transaction data
        let tx_data = build_execute_proposal_call(proposal_id);

        // Send transaction
        let tx_hash = self
            .wallet_service
            .send_contract_transaction(
                dao_contract.clone(),
                tx_data,
                beebotos_chain::compat::U256::from(0),
            )
            .await?;

        info!(
            tx_hash = %hex::encode(tx_hash.as_slice()),
            "Proposal execution submitted"
        );

        Ok(tx_hash)
    }

    /// Get voting power for an address
    pub async fn get_voting_power(&self, address: Address) -> Result<U256, AppError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Chain client not initialized".into()))?;

        let dao_contract = self
            .config
            .dao_contract
            .as_ref()
            .ok_or_else(|| AppError::Configuration("DAO contract not configured".into()))?;

        client
            .get_voting_power(dao_contract.clone(), address)
            .await
            .map_err(|e| AppError::Chain(format!("Failed to get voting power: {}", e)))
    }

    /// Check if address has voted on proposal
    pub async fn has_voted(
        &self,
        _proposal_id: ProposalId,
        _voter: Address,
    ) -> Result<bool, AppError> {
        // TODO: Implement has_voted check
        // This would require querying the DAO contract state
        Ok(false)
    }
}

// Helper functions to build transaction data

fn build_create_proposal_call(_request: &CreateProposalRequest) -> beebotos_chain::compat::Bytes {
    // In real implementation, this would ABI encode the function call
    // For now, return placeholder
    beebotos_chain::compat::Bytes::from(vec![0u8; 32])
}

fn build_cast_vote_call(request: &CastVoteRequest) -> beebotos_chain::compat::Bytes {
    // In real implementation, this would ABI encode the function call
    let mut data = Vec::new();
    data.extend_from_slice(&request.proposal_id.to_le_bytes());
    data.push(match request.vote_type {
        VoteType::For => 1,
        VoteType::Against => 0,
        VoteType::Abstain => 2,
    });
    beebotos_chain::compat::Bytes::from(data)
}

fn build_execute_proposal_call(proposal_id: ProposalId) -> beebotos_chain::compat::Bytes {
    beebotos_chain::compat::Bytes::from(proposal_id.to_le_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dao_service_config() {
        let blockchain_config = BlockchainConfig {
            enabled: true,
            chain_id: 1,
            dao_contract_address: Some("0x1234567890123456789012345678901234567890".to_string()),
            ..Default::default()
        };

        let dao_config = DaoServiceConfig::from(&blockchain_config);

        assert_eq!(dao_config.chain_id, 1);
        assert!(dao_config.dao_contract.is_some());
    }
}
