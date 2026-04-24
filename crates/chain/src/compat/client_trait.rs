//! Chain client trait for dynamic dispatch
//!
//! This module defines object-safe traits that allow ChainService
//! to store different client implementations without generic parameters.

use async_trait::async_trait;

use crate::compat::{Address, BlockNumber, Bytes, TxHash, U256};

/// Agent identity information (simplified for trait object safety)
#[derive(Debug, Clone)]
pub struct AgentIdentityInfo {
    pub agent_id: [u8; 32],
    pub owner: Address,
    pub did: String,
    pub public_key: [u8; 32],
    pub is_active: bool,
    pub reputation: U256,
    pub created_at: U256,
}

/// Chain client abstraction trait (object-safe)
#[async_trait]
pub trait ChainClientTrait: Send + Sync {
    /// Get chain ID
    async fn get_chain_id(&self) -> Result<u64, ChainClientError>;

    /// Get current block number
    async fn get_block_number(&self) -> Result<BlockNumber, ChainClientError>;

    /// Get account balance
    async fn get_balance(&self, address: Address) -> Result<U256, ChainClientError>;

    /// Get transaction receipt
    async fn get_transaction_receipt(
        &self,
        tx_hash: TxHash,
    ) -> Result<Option<TransactionReceipt>, ChainClientError>;

    /// Call contract (read-only)
    async fn call(&self, call: ContractCall) -> Result<Bytes, ChainClientError>;

    /// Send raw transaction
    async fn send_raw_transaction(&self, signed_tx: Bytes) -> Result<TxHash, ChainClientError>;

    /// Estimate gas
    async fn estimate_gas(&self, call: ContractCall) -> Result<U256, ChainClientError>;

    /// Get gas price
    async fn get_gas_price(&self) -> Result<U256, ChainClientError>;

    /// Check client health
    async fn health_check(&self) -> Result<HealthStatus, ChainClientError>;

    /// Send a transaction (for use with external signing)
    ///
    /// # Arguments
    /// * `tx` - Transaction request
    ///
    /// # Returns
    /// Transaction hash
    async fn send_transaction(&self, tx: TransactionRequest) -> Result<TxHash, ChainClientError>;

    /// Get transaction count (nonce) for address
    async fn get_transaction_count(&self, address: Address) -> Result<u64, ChainClientError>;

    // ==================== Identity Registry Operations ====================

    /// Register agent identity on-chain
    ///
    /// # Arguments
    /// * `identity_contract` - The AgentIdentity contract address
    /// * `agent_id` - Unique agent identifier (bytes32)
    /// * `did` - Decentralized identifier string
    /// * `public_key` - Agent's public key (32 bytes)
    /// * `sender` - Transaction sender address
    ///
    /// # Returns
    /// Transaction hash of the registration transaction
    async fn register_agent_identity(
        &self,
        identity_contract: Address,
        agent_id: [u8; 32],
        did: &str,
        public_key: [u8; 32],
        sender: Address,
    ) -> Result<TxHash, ChainClientError>;

    /// Get agent identity information
    ///
    /// # Arguments
    /// * `identity_contract` - The AgentIdentity contract address
    /// * `agent_id` - Unique agent identifier (bytes32)
    ///
    /// # Returns
    /// Agent identity information if registered
    async fn get_agent_identity(
        &self,
        identity_contract: Address,
        agent_id: [u8; 32],
    ) -> Result<Option<AgentIdentityInfo>, ChainClientError>;

    /// Check if agent identity is registered
    ///
    /// # Arguments
    /// * `identity_contract` - The AgentIdentity contract address
    /// * `agent_id` - Unique agent identifier (bytes32)
    async fn is_agent_registered(
        &self,
        identity_contract: Address,
        agent_id: [u8; 32],
    ) -> Result<bool, ChainClientError>;

    /// Get agent ID by DID
    ///
    /// # Arguments
    /// * `identity_contract` - The AgentIdentity contract address
    /// * `did` - Decentralized identifier string
    async fn get_agent_id_by_did(
        &self,
        identity_contract: Address,
        did: &str,
    ) -> Result<Option<[u8; 32]>, ChainClientError>;

    // ==================== DAO Governance Operations ====================

    /// Create a DAO proposal
    ///
    /// # Arguments
    /// * `dao_contract` - The AgentDAO contract address
    /// * `targets` - Target addresses for proposal actions
    /// * `values` - ETH values for proposal actions
    /// * `calldatas` - Encoded function calls
    /// * `description` - Proposal description
    async fn create_dao_proposal(
        &self,
        dao_contract: Address,
        targets: Vec<Address>,
        values: Vec<U256>,
        calldatas: Vec<Bytes>,
        description: &str,
    ) -> Result<u64, ChainClientError>;

    /// Cast a vote on a proposal
    ///
    /// # Arguments
    /// * `dao_contract` - The AgentDAO contract address
    /// * `proposal_id` - Proposal ID
    /// * `support` - Vote type (0=against, 1=for, 2=abstain)
    async fn cast_vote(
        &self,
        dao_contract: Address,
        proposal_id: u64,
        support: u8,
    ) -> Result<(), ChainClientError>;

    /// Get proposal information
    ///
    /// # Arguments
    /// * `dao_contract` - The AgentDAO contract address
    /// * `proposal_id` - Proposal ID
    async fn get_proposal(
        &self,
        dao_contract: Address,
        proposal_id: u64,
    ) -> Result<Option<ProposalInfo>, ChainClientError>;

    /// Get voting power for an address
    ///
    /// # Arguments
    /// * `dao_contract` - The AgentDAO contract address
    /// * `account` - Account address
    async fn get_voting_power(
        &self,
        dao_contract: Address,
        account: Address,
    ) -> Result<U256, ChainClientError>;

    /// Get proposal count
    ///
    /// # Arguments
    /// * `dao_contract` - The AgentDAO contract address
    async fn get_proposal_count(&self, dao_contract: Address) -> Result<u64, ChainClientError>;

    /// List proposals with pagination
    ///
    /// # Arguments
    /// * `dao_contract` - The AgentDAO contract address
    /// * `start_id` - Starting proposal ID
    /// * `limit` - Maximum number of proposals to return
    async fn list_proposals(
        &self,
        dao_contract: Address,
        start_id: u64,
        limit: u64,
    ) -> Result<Vec<ProposalInfo>, ChainClientError>;
}

/// DAO Proposal information
#[derive(Debug, Clone)]
pub struct ProposalInfo {
    pub id: u64,
    pub proposer: Address,
    pub description: String,
    pub for_votes: U256,
    pub against_votes: U256,
    pub abstain_votes: U256,
    pub executed: bool,
    pub state: ProposalState,
}

/// Proposal state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalState {
    Pending,
    Active,
    Canceled,
    Defeated,
    Succeeded,
    Queued,
    Expired,
    Executed,
}

/// Transaction receipt
#[derive(Debug, Clone)]
pub struct TransactionReceipt {
    pub transaction_hash: TxHash,
    pub block_number: BlockNumber,
    pub gas_used: u64,
    pub status: bool,
    pub logs: Vec<LogEntry>,
}

/// Log entry from transaction receipt
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub address: Address,
    pub topics: Vec<[u8; 32]>,
    pub data: Bytes,
}

/// Contract call parameters
#[derive(Debug, Clone)]
pub struct ContractCall {
    pub to: Address,
    pub data: Bytes,
    pub value: Option<U256>,
    pub from: Option<Address>,
}

/// Transaction request for sending transactions
#[derive(Debug, Clone)]
pub struct TransactionRequest {
    pub from: Address,
    pub to: Address,
    pub data: Bytes,
    pub value: U256,
    pub gas_limit: u64,
    pub gas_price: U256,
    pub nonce: u64,
    pub chain_id: u64,
}

impl ContractCall {
    /// Create a new contract call
    pub fn new(to: Address, data: Bytes) -> Self {
        Self {
            to,
            data,
            value: None,
            from: None,
        }
    }

    /// Set value
    pub fn with_value(mut self, value: U256) -> Self {
        self.value = Some(value);
        self
    }

    /// Set from address
    pub fn with_from(mut self, from: Address) -> Self {
        self.from = Some(from);
        self
    }
}

/// Health status
#[derive(Debug, Clone)]
pub struct HealthStatus {
    pub healthy: bool,
    pub latency_ms: u64,
    pub last_block: BlockNumber,
    pub sync_status: SyncStatus,
}

/// Sync status
#[derive(Debug, Clone)]
pub enum SyncStatus {
    Synced,
    Syncing {
        current: BlockNumber,
        target: BlockNumber,
    },
    NotConnected,
}

/// Chain client error types
#[derive(Debug, thiserror::Error)]
pub enum ChainClientError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("RPC error: {code} - {message}")]
    Rpc { code: i32, message: String },
    #[error("Timeout")]
    Timeout,
    #[error("Not connected")]
    NotConnected,
    #[error("Invalid address: {0}")]
    InvalidAddress(String),
    #[error("Other: {0}")]
    Other(String),
}
