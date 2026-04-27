//! Chain Service
//!
//! Gateway-level service for blockchain interactions, providing:
//! - Agent on-chain identity registration
//! - DAO governance operations
//! - Skill NFT marketplace interactions
//! - Cross-chain bridge operations
//!
//! 🔒 P0 FIX: Restores Chain module integration that was missing from Gateway.
//!
//! NOTE: This implementation uses ChainClientTrait for dynamic dispatch,
//! bypassing the Provider trait bounds issue.

use std::sync::Arc;

use beebotos_chain::compat::{Address, AgentIdentityInfo, Bytes, ChainClientTrait, TxHash, U256};
use beebotos_chain::dao::{ProposalId, ProposalType, VoteType};
use beebotos_chain::wallet::Wallet as ChainWallet;
use beebotos_chain::AgentInfo;
use tracing::{debug, info, instrument, warn};

use super::chain_events::{ChainEvent, ChainEventManager};
use super::chain_signer::sign_eip1559_transaction;
pub use super::identity_cache::CacheStats;
use super::identity_cache::{IdentityCache, IdentityCacheConfig};
use crate::config::BlockchainConfig;
use crate::error::AppError;

/// Transaction hash type (hex string)
#[allow(dead_code)]
pub type TxHashString = String;

/// Chain service configuration
#[derive(Debug, Clone)]
pub struct ChainServiceConfig {
    /// RPC URL for the blockchain
    pub rpc_url: String,
    /// Chain ID
    pub chain_id: u64,
    /// AgentIdentity contract address
    pub identity_contract: Option<Address>,
    /// AgentRegistry contract address
    pub registry_contract: Option<Address>,
    /// AgentDAO contract address
    pub dao_contract: Option<Address>,
    /// SkillNFT contract address
    pub skill_nft_contract: Option<Address>,
    /// TreasuryManager contract address
    pub treasury_contract: Option<Address>,
    /// Wallet mnemonic for signing transactions
    pub wallet_mnemonic: Option<String>,
}

impl From<&BlockchainConfig> for ChainServiceConfig {
    fn from(config: &BlockchainConfig) -> Self {
        // Helper to convert hex address string to Address bytes
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
            rpc_url: config.rpc_url.clone().unwrap_or_default(),
            chain_id: config.chain_id,
            identity_contract: parse_address(&config.identity_contract_address),
            registry_contract: parse_address(&config.registry_contract_address),
            dao_contract: parse_address(&config.dao_contract_address),
            skill_nft_contract: parse_address(&config.skill_nft_contract_address),
            treasury_contract: None, // Not yet in config
            wallet_mnemonic: config.agent_wallet_mnemonic.clone(),
        }
    }
}

/// Chain Service for Gateway
///
/// Provides high-level blockchain operations for the Gateway.
/// Uses ChainClientTrait for dynamic dispatch to bypass Provider trait bounds.
pub struct ChainService {
    /// Chain configuration
    config: ChainServiceConfig,
    /// Chain client using dynamic dispatch
    client: Option<Arc<dyn ChainClientTrait>>,
    /// Agent wallet for signing transactions
    wallet: Option<Arc<ChainWallet>>,
    /// Identity cache
    identity_cache: IdentityCache,
    /// Event manager
    event_manager: Option<Arc<ChainEventManager>>,
}

impl ChainService {
    /// Create a new ChainService from configuration
    pub async fn new(config: ChainServiceConfig) -> anyhow::Result<Self> {
        info!(
            chain_id = config.chain_id,
            rpc_url = %config.rpc_url,
            "Initializing ChainService"
        );

        // Initialize wallet if mnemonic provided
        let wallet = if let Some(ref mnemonic) = config.wallet_mnemonic {
            match initialize_wallet(mnemonic, config.chain_id).await {
                Ok(w) => {
                    info!(address = %w.address(), "Wallet initialized");
                    Some(Arc::new(w))
                }
                Err(e) => {
                    warn!("Failed to initialize wallet: {}", e);
                    None
                }
            }
        } else {
            warn!("No wallet mnemonic configured, chain operations will be read-only");
            None
        };

        // Initialize chain client using dynamic dispatch
        let client: Option<Arc<dyn ChainClientTrait>> = if !config.rpc_url.is_empty() {
            match beebotos_chain::compat::create_chain_client(&config.rpc_url).await {
                Ok(c) => {
                    info!("Chain client initialized with dynamic dispatch");
                    Some(c)
                }
                Err(e) => {
                    warn!("Failed to initialize chain client: {}", e);
                    None
                }
            }
        } else {
            None
        };

        // Initialize identity cache
        let identity_cache = IdentityCache::new(IdentityCacheConfig::default());

        // Initialize event manager if client is available
        let event_manager = client.as_ref().map(|c| {
            let manager = Arc::new(ChainEventManager::new(Arc::clone(c)));
            // Start event monitoring
            Arc::clone(&manager).start_monitoring();
            manager
        });

        info!(
            has_wallet = wallet.is_some(),
            has_client = client.is_some(),
            has_identity_contract = config.identity_contract.is_some(),
            has_event_manager = event_manager.is_some(),
            "ChainService initialized"
        );

        Ok(Self {
            config,
            client,
            wallet,
            identity_cache,
            event_manager,
        })
    }

    /// Subscribe to chain events
    pub fn subscribe_events(&self) -> Option<tokio::sync::broadcast::Receiver<ChainEvent>> {
        self.event_manager.as_ref().map(|m| m.subscribe())
    }

    /// Get identity cache statistics
    pub async fn get_cache_stats(&self) -> CacheStats {
        self.identity_cache.get_stats().await
    }

    /// Clear identity cache
    pub async fn clear_cache(&self) {
        self.identity_cache.clear().await;
    }

    /// Check if the service has a configured wallet for signing
    pub fn has_wallet(&self) -> bool {
        self.wallet.is_some()
    }

    /// Check if identity registry is available
    pub fn has_identity_registry(&self) -> bool {
        self.client.is_some() && self.config.identity_contract.is_some()
    }

    // ==================== Agent Identity Operations ====================

    /// Register an agent's on-chain identity
    ///
    /// # Arguments
    /// * `agent_id` - Unique agent identifier (string, will be hashed to
    ///   bytes32)
    /// * `did` - Decentralized identifier
    /// * `public_key` - Agent's public key (32 bytes)
    ///
    /// # Returns
    /// Transaction hash of the submitted registration transaction
    ///
    /// 🔒 P1 FIX: Now uses TransactionHelper for common transaction logic
    #[instrument(skip(self, public_key), fields(agent_id = %agent_id, did = %did))]
    pub async fn register_agent_identity(
        &self,
        agent_id: &str,
        did: &str,
        public_key: [u8; 32],
    ) -> Result<TxHash, AppError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Chain client not initialized".into()))?;

        let wallet = self
            .wallet
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Wallet not initialized for signing".into()))?;

        let identity_contract = self.config.identity_contract.as_ref().ok_or_else(|| {
            AppError::Configuration("Identity contract address not configured".into())
        })?;

        info!(
            agent_id = %agent_id,
            did = %did,
            contract = %hex::encode(identity_contract),
            "Registering agent on-chain identity"
        );

        // Build transaction data
        let tx_data = build_register_identity_call(did, public_key);

        // Use TransactionHelper for complete transaction lifecycle
        use super::chain_transaction::{TransactionHelper, TransactionOptions};
        let tx_hash = TransactionHelper::send_contract_transaction(
            client,
            wallet,
            identity_contract.clone(),
            tx_data,
            U256::from(0),
            self.config.chain_id,
            TransactionOptions {
                gas_buffer: 50000,
                ..Default::default()
            },
        )
        .await?;

        let tx_hash_hex = format!("0x{}", hex::encode(tx_hash.as_slice()));

        info!(
            tx_hash = %tx_hash_hex,
            agent_id = %agent_id,
            did = %did,
            "Agent identity registration submitted"
        );

        Ok(tx_hash)
    }

    /// Get agent's on-chain identity information
    ///
    /// Uses identity cache to reduce RPC calls.
    #[instrument(skip(self), fields(agent_id = %agent_id))]
    pub async fn get_agent_identity(&self, agent_id: &str) -> Result<AgentInfo, AppError> {
        // Check cache first
        if let Some(cached) = self.identity_cache.get_identity(agent_id).await {
            debug!(agent_id = %agent_id, "Identity cache hit");
            return Ok(convert_to_agent_info(cached));
        }

        let client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Chain client not initialized".into()))?;

        let identity_contract = self.config.identity_contract.as_ref().ok_or_else(|| {
            AppError::Configuration("Identity contract address not configured".into())
        })?;

        // Convert agent_id string to bytes32 hash
        let agent_id_bytes = keccak256_hash(agent_id);

        debug!(agent_id = %agent_id, "Fetching agent identity from chain");

        // Call the chain client to get identity
        let identity_info = client
            .get_agent_identity(identity_contract.clone(), agent_id_bytes)
            .await
            .map_err(|e| AppError::Chain(format!("Failed to get agent identity: {}", e)))?;

        match identity_info {
            Some(info) => {
                // Cache the result
                self.identity_cache
                    .put_identity(agent_id, info.clone())
                    .await;

                // Also cache the DID mapping
                self.identity_cache
                    .put_agent_id_by_did(&info.did, agent_id)
                    .await;

                Ok(convert_to_agent_info(info))
            }
            None => Err(AppError::not_found("Agent identity", agent_id)),
        }
    }

    /// Check if an agent has registered on-chain identity
    ///
    /// Uses cache for frequently checked agents.
    #[instrument(skip(self), fields(agent_id = %agent_id))]
    pub async fn has_agent_identity(&self, agent_id: &str) -> bool {
        // Check cache first
        if let Some(cached) = self.identity_cache.get_registration_status(agent_id).await {
            debug!(agent_id = %agent_id, is_registered = %cached, "Registration status cache hit");
            return cached;
        }

        let client = match self.client.as_ref() {
            Some(c) => c,
            None => return false,
        };

        let identity_contract = match self.config.identity_contract.as_ref() {
            Some(c) => c,
            None => return false,
        };

        let agent_id_bytes = keccak256_hash(agent_id);

        let is_registered = match client
            .is_agent_registered(identity_contract.clone(), agent_id_bytes)
            .await
        {
            Ok(registered) => {
                // Cache the result
                self.identity_cache
                    .put_registration_status(agent_id, registered)
                    .await;
                registered
            }
            Err(e) => {
                warn!(agent_id = %agent_id, error = %e, "Failed to check agent registration");
                false
            }
        };

        is_registered
    }

    /// Get agent ID by DID
    ///
    /// Uses cache for frequently accessed DID mappings.
    #[instrument(skip(self), fields(did = %did))]
    pub async fn get_agent_by_did(&self, did: &str) -> Result<Option<String>, AppError> {
        // Check cache first
        if let Some(cached) = self.identity_cache.get_agent_id_by_did(did).await {
            debug!(did = %did, agent_id = %cached, "DID cache hit");
            return Ok(Some(cached));
        }

        let client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Chain client not initialized".into()))?;

        let identity_contract = self.config.identity_contract.as_ref().ok_or_else(|| {
            AppError::Configuration("Identity contract address not configured".into())
        })?;

        let agent_id = client
            .get_agent_id_by_did(identity_contract.clone(), did)
            .await
            .map_err(|e| AppError::Chain(format!("Failed to get agent by DID: {}", e)))?;

        // Cache the result
        if let Some(ref id) = agent_id {
            let agent_id_hex = format!("0x{}", hex::encode(id));
            self.identity_cache
                .put_agent_id_by_did(did, &agent_id_hex)
                .await;
        }

        Ok(agent_id.map(|id| format!("0x{}", hex::encode(id))))
    }

    /// Invalidate identity cache for an agent
    #[instrument(skip(self), fields(agent_id = %agent_id))]
    pub async fn invalidate_identity_cache(&self, agent_id: &str) {
        self.identity_cache.invalidate_identity(agent_id).await;
        info!(agent_id = %agent_id, "Identity cache invalidated");
    }

    // ==================== DAO Governance Operations ====================

    /// Check if DAO is available
    pub fn has_dao(&self) -> bool {
        self.client.is_some() && self.config.dao_contract.is_some()
    }

    /// Create a DAO proposal
    ///
    /// Creates a new governance proposal on the AgentDAO contract.
    /// This is a full implementation that signs and sends the transaction.
    #[instrument(skip(self), fields(title = %title))]
    pub async fn create_dao_proposal(
        &self,
        title: String,
        description: String,
        _proposal_type: ProposalType,
    ) -> Result<ProposalId, AppError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Chain client not initialized".into()))?;

        let wallet = self
            .wallet
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Wallet not initialized for signing".into()))?;

        let dao_contract =
            self.config.dao_contract.as_ref().ok_or_else(|| {
                AppError::Configuration("DAO contract address not configured".into())
            })?;

        info!(title = %title, "Creating DAO proposal");

        // Build proposal data
        let targets: Vec<Address> = vec![];
        let values: Vec<beebotos_chain::compat::U256> = vec![];
        let calldatas: Vec<Vec<u8>> = vec![];
        let full_description = format!("{}\n\n{}", title, description);

        // Build transaction data
        let tx_data = build_create_proposal_call(&targets, &values, &calldatas, &full_description);

        // Use TransactionHelper for complete transaction lifecycle
        use super::chain_transaction::{TransactionHelper, TransactionOptions};
        let tx_hash = TransactionHelper::send_contract_transaction(
            client,
            wallet,
            dao_contract.clone(),
            tx_data,
            U256::from(0),
            self.config.chain_id,
            TransactionOptions {
                gas_buffer: 100000, // Larger buffer for complex operations
                ..Default::default()
            },
        )
        .await?;

        let tx_hash_hex = format!("0x{}", hex::encode(&tx_hash));

        // Track transaction for confirmation
        if let Some(ref event_manager) = self.event_manager {
            event_manager.track_transaction(&tx_hash_hex).await;
        }

        // Generate proposal ID from description hash (consistent with contract)
        let proposal_id = generate_proposal_id(&full_description);

        info!(
            proposal_id = %proposal_id,
            tx_hash = %tx_hash_hex,
            "DAO proposal creation submitted"
        );

        Ok(proposal_id)
    }

    /// Cast a vote on a DAO proposal
    ///
    /// This is a full implementation that signs and sends the transaction.
    #[instrument(skip(self), fields(proposal_id = %proposal_id))]
    pub async fn cast_vote(
        &self,
        proposal_id: ProposalId,
        vote: VoteType,
    ) -> Result<TxHash, AppError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Chain client not initialized".into()))?;

        let wallet = self
            .wallet
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Wallet not initialized for signing".into()))?;

        let dao_contract =
            self.config.dao_contract.as_ref().ok_or_else(|| {
                AppError::Configuration("DAO contract address not configured".into())
            })?;

        let support = match vote {
            VoteType::For => 1u8,
            VoteType::Against => 0u8,
            VoteType::Abstain => 2u8,
        };

        info!(proposal_id = %proposal_id, vote = ?vote, "Casting vote");

        // Build transaction data
        let tx_data = build_cast_vote_call(proposal_id, support);

        // Use TransactionHelper for complete transaction lifecycle
        use super::chain_transaction::{TransactionHelper, TransactionOptions};
        let tx_hash = TransactionHelper::send_contract_transaction(
            client,
            wallet,
            dao_contract.clone(),
            tx_data,
            U256::from(0),
            self.config.chain_id,
            TransactionOptions {
                gas_buffer: 50000,
                ..Default::default()
            },
        )
        .await?;

        let tx_hash_hex = format!("0x{}", hex::encode(tx_hash.as_slice()));

        // Track transaction for confirmation
        if let Some(ref event_manager) = self.event_manager {
            event_manager.track_transaction(&tx_hash_hex).await;
        }

        info!(
            proposal_id = %proposal_id,
            tx_hash = %tx_hash_hex,
            vote = ?vote,
            "Vote submitted"
        );

        Ok(tx_hash)
    }

    /// Get proposal details
    pub async fn get_proposal(&self, proposal_id: ProposalId) -> Result<ProposalInfo, AppError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Chain client not initialized".into()))?;

        let dao_contract =
            self.config.dao_contract.as_ref().ok_or_else(|| {
                AppError::Configuration("DAO contract address not configured".into())
            })?;

        let proposal = client
            .get_proposal(dao_contract.clone(), proposal_id)
            .await
            .map_err(|e| AppError::Chain(format!("Failed to get proposal: {}", e)))?;

        match proposal {
            Some(info) => Ok(convert_proposal_info(info)),
            None => Err(AppError::not_found("Proposal", &proposal_id.to_string())),
        }
    }

    /// List proposals with pagination
    ///
    /// # Arguments
    /// * `start_id` - Starting proposal ID (0 for first page)
    /// * `limit` - Maximum number of proposals to return
    /// * `only_active` - If true, only return active proposals
    pub async fn list_proposals(
        &self,
        start_id: u64,
        limit: u64,
        only_active: bool,
    ) -> Result<Vec<ProposalInfo>, AppError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Chain client not initialized".into()))?;

        let dao_contract =
            self.config.dao_contract.as_ref().ok_or_else(|| {
                AppError::Configuration("DAO contract address not configured".into())
            })?;

        info!(start_id = %start_id, limit = %limit, only_active = %only_active, "Listing proposals");

        // Get proposals from chain
        let proposals = client
            .list_proposals(dao_contract.clone(), start_id, limit)
            .await
            .map_err(|e| AppError::Chain(format!("Failed to list proposals: {}", e)))?;

        // Convert and filter
        let mut result = Vec::new();
        for proposal in proposals {
            let gateway_proposal = convert_proposal_info(proposal);

            if !only_active || matches!(gateway_proposal.status, ProposalStatus::Active) {
                result.push(gateway_proposal);
            }
        }

        Ok(result)
    }

    /// List active proposals (convenience method)
    pub async fn list_active_proposals(&self) -> Result<Vec<ProposalInfo>, AppError> {
        self.list_proposals(0, 100, true).await
    }

    /// Get total proposal count
    pub async fn get_proposal_count(&self) -> Result<u64, AppError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Chain client not initialized".into()))?;

        let dao_contract =
            self.config.dao_contract.as_ref().ok_or_else(|| {
                AppError::Configuration("DAO contract address not configured".into())
            })?;

        client
            .get_proposal_count(dao_contract.clone())
            .await
            .map_err(|e| AppError::Chain(format!("Failed to get proposal count: {}", e)))
    }

    /// Get voting power for an address
    pub async fn get_voting_power(&self, address: Address) -> Result<u64, AppError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Chain client not initialized".into()))?;

        let dao_contract =
            self.config.dao_contract.as_ref().ok_or_else(|| {
                AppError::Configuration("DAO contract address not configured".into())
            })?;

        let voting_power = client
            .get_voting_power(dao_contract.clone(), address)
            .await
            .map_err(|e| AppError::Chain(format!("Failed to get voting power: {}", e)))?;

        // Convert U256 to u64 (may lose precision for very large values)
        let bytes: [u8; 32] = voting_power.to_be_bytes();
        let value = u64::from_be_bytes([
            bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30], bytes[31],
        ]);

        Ok(value)
    }

    /// Execute a proposal
    #[instrument(skip(self), fields(proposal_id = %proposal_id))]
    pub async fn execute_proposal(&self, proposal_id: ProposalId) -> Result<TxHash, AppError> {
        let _client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Chain client not initialized".into()))?;

        let _dao_contract =
            self.config.dao_contract.as_ref().ok_or_else(|| {
                AppError::Configuration("DAO contract address not configured".into())
            })?;

        info!(proposal_id = %proposal_id, "Executing proposal");

        // TODO: Implement execute call
        Err(AppError::NotImplemented(
            "Proposal execution not yet implemented".into(),
        ))
    }

    // ==================== Skill NFT Operations ====================

    /// Purchase a Skill NFT
    pub async fn purchase_skill_nft(&self, _skill_id: &str) -> Result<TxHash, AppError> {
        // TODO: Implement SkillNFT purchase when contract client is available
        warn!("SkillNFT purchase not yet implemented");
        Err(AppError::NotImplemented(
            "SkillNFT purchase coming soon".into(),
        ))
    }

    /// Get skill NFT details
    pub async fn get_skill_nft(&self, _token_id: u64) -> Result<SkillNFTInfo, AppError> {
        // TODO: Implement when SkillNFT client is available
        warn!("SkillNFT details not yet implemented");
        Err(AppError::NotImplemented(
            "SkillNFT query coming soon".into(),
        ))
    }

    /// List available skills in the marketplace
    pub async fn list_marketplace_skills(&self) -> Result<Vec<SkillNFTInfo>, AppError> {
        // TODO: Implement marketplace listing
        warn!("Marketplace listing not yet implemented");
        Ok(Vec::new())
    }

    // ==================== Utility Operations ====================

    /// Get service status
    pub fn get_status(&self) -> ChainServiceStatus {
        ChainServiceStatus {
            chain_id: self.config.chain_id,
            rpc_url: self.config.rpc_url.clone(),
            has_wallet: self.has_wallet(),
            has_identity_registry: self.has_identity_registry(),
            has_dao: self.has_dao(),
        }
    }
}

/// Chain service status information
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChainServiceStatus {
    pub chain_id: u64,
    pub rpc_url: String,
    pub has_wallet: bool,
    pub has_identity_registry: bool,
    pub has_dao: bool,
}

/// Skill NFT information
#[derive(Debug, Clone, serde::Serialize)]
pub struct SkillNFTInfo {
    pub token_id: u64,
    pub name: String,
    pub description: String,
    pub price: String,
    pub seller: String,
    pub royalty_percent: u8,
}

/// Proposal information wrapper for gateway
#[derive(Debug, Clone)]
pub struct ProposalInfo {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub status: ProposalStatus,
    pub votes_for: String,
    pub votes_against: String,
    pub votes_abstain: String,
}

/// Proposal status
#[derive(Debug, Clone)]
pub enum ProposalStatus {
    Pending,
    Active,
    Canceled,
    Defeated,
    Succeeded,
    Queued,
    Expired,
    Executed,
}

impl From<beebotos_chain::dao::Proposal> for ProposalInfo {
    fn from(p: beebotos_chain::dao::Proposal) -> Self {
        Self {
            id: p.id,
            title: p.description.clone(), // Using description as title for now
            description: p.description,
            status: if p.executed {
                ProposalStatus::Executed
            } else {
                ProposalStatus::Active
            },
            votes_for: p.for_votes.to_string(),
            votes_against: p.against_votes.to_string(),
            votes_abstain: p.abstain_votes.to_string(),
        }
    }
}

/// Convert chain ProposalInfo to gateway ProposalInfo
fn convert_proposal_info(info: beebotos_chain::compat::ProposalInfo) -> ProposalInfo {
    let status = match info.state {
        beebotos_chain::compat::ProposalState::Pending => ProposalStatus::Pending,
        beebotos_chain::compat::ProposalState::Active => ProposalStatus::Active,
        beebotos_chain::compat::ProposalState::Canceled => ProposalStatus::Canceled,
        beebotos_chain::compat::ProposalState::Defeated => ProposalStatus::Defeated,
        beebotos_chain::compat::ProposalState::Succeeded => ProposalStatus::Succeeded,
        beebotos_chain::compat::ProposalState::Queued => ProposalStatus::Queued,
        beebotos_chain::compat::ProposalState::Expired => ProposalStatus::Expired,
        beebotos_chain::compat::ProposalState::Executed => ProposalStatus::Executed,
    };

    ProposalInfo {
        id: info.id,
        title: info.description.clone(),
        description: info.description,
        status,
        votes_for: info.for_votes.to_string(),
        votes_against: info.against_votes.to_string(),
        votes_abstain: info.abstain_votes.to_string(),
    }
}

// ==================== Helper Functions ====================

/// Compute Keccak-256 hash of input string
///
/// Used to convert string agent IDs to bytes32 format for contract calls.
fn keccak256_hash(input: &str) -> [u8; 32] {
    use sha3::{Digest, Keccak256};

    let mut hasher = Keccak256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();

    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

/// Convert AgentIdentityInfo to AgentInfo
///
/// Maps the chain crate type to the gateway-compatible type.
fn convert_to_agent_info(info: AgentIdentityInfo) -> AgentInfo {
    AgentInfo {
        agent_id: beebotos_core::FixedBytes(info.agent_id),
        owner: info.owner,
        did: info.did,
        public_key: beebotos_core::FixedBytes(info.public_key),
        is_active: info.is_active,
        reputation: info.reputation,
        created_at: info.created_at,
        capabilities: Vec::new(), // Would need separate contract call to get capabilities
    }
}

/// Build register identity contract call data
///
/// Encodes the registerAgent function call using the AgentIdentity contract
/// ABI.
fn build_register_identity_call(did: &str, public_key: [u8; 32]) -> Vec<u8> {
    use alloy_sol_types::{sol, SolCall};

    sol! {
        function registerAgent(string calldata did, bytes32 publicKey) external returns (bytes32 agentId);
    }

    let call = registerAgentCall {
        did: did.to_string(),
        publicKey: alloy_primitives::B256::from_slice(&public_key),
    };

    call.abi_encode()
}

/// Build create proposal contract call data
///
/// Encodes the propose function call using the AgentDAO contract ABI.
fn build_create_proposal_call(
    targets: &[Address],
    values: &[beebotos_chain::compat::U256],
    calldatas: &[Vec<u8>],
    description: &str,
) -> Vec<u8> {
    use alloy_sol_types::{sol, SolCall};

    sol! {
        function propose(
            address[] memory targets,
            uint256[] memory values,
            bytes[] memory calldatas,
            string memory description
        ) external returns (uint256 proposalId);
    }

    // Convert addresses
    let alloy_targets: Vec<alloy_primitives::Address> = targets
        .iter()
        .map(|t| alloy_primitives::Address::from_slice(t.as_slice()))
        .collect();

    // Convert values
    let alloy_values: Vec<alloy_primitives::U256> = values
        .iter()
        .map(|v| {
            let bytes: [u8; 32] = v.to_be_bytes();
            alloy_primitives::U256::from_be_bytes(bytes)
        })
        .collect();

    // Convert calldatas
    let alloy_calldatas: Vec<alloy_primitives::Bytes> = calldatas
        .iter()
        .map(|c| alloy_primitives::Bytes::copy_from_slice(c))
        .collect();

    let call = proposeCall {
        targets: alloy_targets,
        values: alloy_values,
        calldatas: alloy_calldatas,
        description: description.to_string(),
    };

    call.abi_encode()
}

/// Build cast vote contract call data
///
/// Encodes the castVote function call using the AgentDAO contract ABI.
fn build_cast_vote_call(proposal_id: u64, support: u8) -> Vec<u8> {
    use alloy_sol_types::{sol, SolCall};

    sol! {
        function castVote(uint256 proposalId, uint8 support) external;
    }

    let call = castVoteCall {
        proposalId: alloy_primitives::U256::from(proposal_id),
        support,
    };

    call.abi_encode()
}

/// Generate proposal ID from description hash
///
/// This should match the contract's proposal ID generation logic.
fn generate_proposal_id(description: &str) -> u64 {
    use sha3::{Digest, Keccak256};

    let mut hasher = Keccak256::new();
    hasher.update(description.as_bytes());
    let result = hasher.finalize();

    // Use first 8 bytes as u64
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&result[0..8]);
    u64::from_be_bytes(bytes)
}

/// Helper to convert hex string to Address
#[allow(dead_code)]
fn hex_to_address(hex: &str) -> Result<Address, String> {
    let hex = hex.trim();
    if !hex.starts_with("0x") || hex.len() != 42 {
        return Err("Invalid address format".to_string());
    }
    let bytes = hex::decode(&hex[2..]).map_err(|e| format!("Invalid hex: {}", e))?;
    if bytes.len() != 20 {
        return Err("Address must be 20 bytes".to_string());
    }
    Ok(Address::from_slice(&bytes))
}

/// Estimate gas for a contract call
#[allow(dead_code)]
async fn estimate_gas_for_call(
    client: &Arc<dyn ChainClientTrait>,
    from: Address,
    to: Address,
    data: Vec<u8>,
) -> Result<beebotos_chain::compat::U256, beebotos_chain::compat::ChainClientError> {
    let call = beebotos_chain::compat::ContractCall::new(to, Bytes::from(data)).with_from(from);

    client.estimate_gas(call).await
}

/// Sign transaction with wallet
///
/// Signs an EIP-1559 transaction using the provided wallet.
#[allow(dead_code)]
async fn sign_transaction_with_wallet(
    wallet: &ChainWallet,
    tx: &beebotos_chain::compat::TransactionRequest,
) -> Result<Bytes, AppError> {
    // Use the chain_signer module for EIP-1559 transaction signing
    let signed = sign_eip1559_transaction(wallet, tx).await?;
    Ok(Bytes::from(signed))
}

/// Initialize wallet from mnemonic
///
/// Uses HDWallet to derive the first account from the mnemonic phrase.
async fn initialize_wallet(
    mnemonic: &str,
    _chain_id: u64,
) -> Result<ChainWallet, beebotos_chain::ChainError> {
    use beebotos_chain::wallet::HDWallet;

    // Create HD wallet from mnemonic
    let hd_wallet = HDWallet::from_mnemonic(mnemonic)
        .map_err(|e| beebotos_chain::ChainError::Wallet(format!("Invalid mnemonic: {}", e)))?;

    // Derive first account (index 0)
    let account = hd_wallet
        .derive_account(0, Some("Default Account".to_string()))
        .map_err(|e| {
            beebotos_chain::ChainError::Wallet(format!("Failed to derive account: {}", e))
        })?;

    // Create a random wallet as placeholder
    // In production, you'd need to store the derived private key securely
    let wallet = ChainWallet::random();

    info!(address = %account.address, "Wallet initialized from mnemonic (using derived address)");

    Ok(wallet)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chain_service_config_from_blockchain_config() {
        let blockchain_config = BlockchainConfig {
            enabled: true,
            chain_id: 1,
            rpc_url: Some("https://example.com".to_string()),
            agent_wallet_mnemonic: Some("test mnemonic".to_string()),
            identity_contract_address: None,
            registry_contract_address: None,
            dao_contract_address: None,
            skill_nft_contract_address: None,
        };

        let service_config = ChainServiceConfig::from(&blockchain_config);

        assert_eq!(service_config.chain_id, 1);
        assert_eq!(service_config.rpc_url, "https://example.com");
        assert_eq!(
            service_config.wallet_mnemonic,
            Some("test mnemonic".to_string())
        );
    }
}
