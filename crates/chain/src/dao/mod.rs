//! DAO Module

use serde::{Deserialize, Serialize};

use crate::compat::{Address, B256, U256};
use crate::Result;

/// Proposal ID
pub type ProposalId = u64;

/// Vote type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum VoteType {
    For,
    Against,
    Abstain,
}

/// Proposal info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: ProposalId,
    pub proposer: Address,
    pub description: String,
    pub for_votes: U256,
    pub against_votes: U256,
    pub abstain_votes: U256,
    pub executed: bool,
    pub eta: u64,
}

/// DAO interface trait
#[async_trait::async_trait]
pub trait DAOInterface: Send + Sync {
    /// Get proposal count
    async fn proposal_count(&self) -> Result<u64>;

    /// Get proposal by ID
    async fn get_proposal(&self, id: ProposalId) -> Result<Proposal>;

    /// Create new proposal
    async fn propose(
        &self,
        targets: Vec<Address>,
        values: Vec<U256>,
        calldatas: Vec<alloy_primitives::Bytes>,
        description: String,
    ) -> Result<ProposalId>;

    /// Cast vote
    async fn cast_vote(&self, proposal_id: ProposalId, support: VoteType) -> Result<()>;

    /// Execute proposal
    async fn execute(&self, proposal_id: ProposalId) -> Result<B256>;

    /// Get voting power
    async fn get_votes(&self, account: Address) -> Result<U256>;

    /// Delegate votes
    async fn delegate(&self, delegatee: Address) -> Result<()>;
}

// Re-export DAOClient and related types from client module
pub mod client;
pub use client::{DAOClient, ProposalBuilder, ProposalType};

pub mod delegation;
pub mod governance;
pub mod proposal;
pub mod treasury;
pub mod voting;
