//! DAO Proposal Module

use serde::{Deserialize, Serialize};

use crate::compat::{Address, Bytes, U256};

/// Proposal status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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

/// Proposal info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalInfo {
    pub id: u64,
    pub proposer: Address,
    pub targets: Vec<Address>,
    pub values: Vec<U256>,
    pub calldatas: Vec<Bytes>,
    pub signatures: Vec<String>,
    pub description: String,
    pub start_block: u64,
    pub end_block: u64,
    pub status: ProposalStatus,
    pub for_votes: U256,
    pub against_votes: U256,
    pub abstain_votes: U256,
}

/// Proposal action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalAction {
    pub target: Address,
    pub value: U256,
    pub data: Bytes,
    pub signature: Option<String>,
}

/// Proposal builder
pub struct ProposalBuilder {
    targets: Vec<Address>,
    values: Vec<U256>,
    calldatas: Vec<Bytes>,
    signatures: Vec<String>,
    description: String,
}

impl ProposalBuilder {
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            targets: Vec::new(),
            values: Vec::new(),
            calldatas: Vec::new(),
            signatures: Vec::new(),
            description: description.into(),
        }
    }

    pub fn add_action(
        mut self,
        target: Address,
        value: U256,
        data: Bytes,
        signature: Option<String>,
    ) -> Self {
        self.targets.push(target);
        self.values.push(value);
        self.calldatas.push(data);
        self.signatures.push(signature.unwrap_or_default());
        self
    }

    pub fn build(self) -> (Vec<Address>, Vec<U256>, Vec<Bytes>, String) {
        (self.targets, self.values, self.calldatas, self.description)
    }
}
