//! Chain module event types for SystemEventBus integration

use beebotos_core::event_bus::SystemEvent;
use chrono::{DateTime, Utc};

/// Blockchain transaction events
#[derive(Debug, Clone)]
pub enum ChainEvent {
    /// Transaction submitted to mempool
    TransactionSubmitted {
        tx_hash: String,
        from: String,
        to: String,
        value: String,
    },
    /// Transaction confirmed on chain
    TransactionConfirmed {
        tx_hash: String,
        block_number: u64,
        gas_used: u64,
        confirmations: u64,
    },
    /// Transaction failed
    TransactionFailed { tx_hash: String, error: String },
    /// Identity registered
    IdentityRegistered {
        agent_id: String,
        did: String,
        owner: String,
        tx_hash: String,
    },
    /// DAO proposal created
    ProposalCreated {
        proposal_id: u64,
        proposer: String,
        title: String,
        tx_hash: String,
    },
    /// Vote cast on proposal
    VoteCast {
        proposal_id: u64,
        voter: String,
        support: bool,
        voting_power: String,
        tx_hash: String,
    },
    /// Proposal executed
    ProposalExecuted {
        proposal_id: u64,
        executor: String,
        tx_hash: String,
    },
    /// Skill NFT minted
    SkillMinted {
        token_id: u64,
        owner: String,
        skill_name: String,
        tx_hash: String,
    },
}

impl SystemEvent for ChainEvent {
    fn event_type(&self) -> &'static str {
        "chain"
    }

    fn timestamp(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Chain connection events
#[derive(Debug, Clone)]
pub enum ChainConnectionEvent {
    /// Connected to chain
    Connected { chain_id: u64, rpc_url: String },
    /// Disconnected from chain
    Disconnected { chain_id: u64, reason: String },
    /// Connection error
    Error { chain_id: u64, error: String },
    /// Block received
    NewBlock {
        chain_id: u64,
        block_number: u64,
        block_hash: String,
    },
}

impl SystemEvent for ChainConnectionEvent {
    fn event_type(&self) -> &'static str {
        "chain.connection"
    }

    fn timestamp(&self) -> DateTime<Utc> {
        Utc::now()
    }
}
