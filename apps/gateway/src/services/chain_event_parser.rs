//! Chain Event Parser
//!
//! Parses events from transaction receipts and logs.
//! Supports AgentIdentity, AgentDAO, and other BeeBotOS contract events.

use alloy_sol_types::{sol, SolEvent};

/// Parsed event from transaction receipt
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ParsedEvent {
    /// Agent identity registered
    IdentityRegistered(IdentityRegisteredEvent),
    /// Agent identity updated
    IdentityUpdated(IdentityUpdatedEvent),
    /// Agent deactivated
    AgentDeactivated(AgentDeactivatedEvent),
    /// Capability granted
    CapabilityGranted(CapabilityGrantedEvent),
    /// Capability revoked
    CapabilityRevoked(CapabilityRevokedEvent),
    /// DAO proposal created
    ProposalCreated(ProposalCreatedEvent),
    /// Vote cast
    VoteCast(VoteCastEvent),
    /// Proposal executed
    ProposalExecuted(ProposalExecutedEvent),
    /// Proposal queued
    ProposalQueued(ProposalQueuedEvent),
    /// Proposal canceled
    ProposalCanceled(ProposalCanceledEvent),
    /// Unknown event (raw log data)
    Unknown {
        address: String,
        topics: Vec<String>,
        data: String,
    },
}

/// Identity registered event
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct IdentityRegisteredEvent {
    pub agent_id: String,
    pub owner: String,
    pub did: String,
    pub block_number: u64,
    pub tx_hash: String,
}

/// Identity updated event
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct IdentityUpdatedEvent {
    pub agent_id: String,
    pub field: String,
    pub block_number: u64,
    pub tx_hash: String,
}

/// Agent deactivated event
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AgentDeactivatedEvent {
    pub agent_id: String,
    pub block_number: u64,
    pub tx_hash: String,
}

/// Capability granted event
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CapabilityGrantedEvent {
    pub agent_id: String,
    pub capability: String,
    pub block_number: u64,
    pub tx_hash: String,
}

/// Capability revoked event
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CapabilityRevokedEvent {
    pub agent_id: String,
    pub capability: String,
    pub block_number: u64,
    pub tx_hash: String,
}

/// Proposal created event
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ProposalCreatedEvent {
    pub proposal_id: u64,
    pub proposer: String,
    pub targets: Vec<String>,
    pub values: Vec<String>,
    pub signatures: Vec<String>,
    pub calldatas: Vec<String>,
    pub start_block: u64,
    pub end_block: u64,
    pub description: String,
    pub block_number: u64,
    pub tx_hash: String,
}

/// Vote cast event
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct VoteCastEvent {
    pub voter: String,
    pub proposal_id: u64,
    pub support: u8,
    pub weight: String,
    pub reason: String,
    pub block_number: u64,
    pub tx_hash: String,
}

/// Proposal executed event
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ProposalExecutedEvent {
    pub proposal_id: u64,
    pub block_number: u64,
    pub tx_hash: String,
}

/// Proposal queued event
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ProposalQueuedEvent {
    pub proposal_id: u64,
    pub eta: u64,
    pub block_number: u64,
    pub tx_hash: String,
}

/// Proposal canceled event
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ProposalCanceledEvent {
    pub proposal_id: u64,
    pub block_number: u64,
    pub tx_hash: String,
}

// Define contract event interfaces
sol! {
    // AgentIdentity events
    event AgentRegistered(bytes32 indexed agentId, address indexed owner, string did);
    event AgentUpdated(bytes32 indexed agentId, string field);
    event AgentDeactivated(bytes32 indexed agentId);
    event CapabilityGranted(bytes32 indexed agentId, bytes32 capability);
    event CapabilityRevoked(bytes32 indexed agentId, bytes32 capability);

    // AgentDAO events
    event ProposalCreated(
        uint256 indexed proposalId,
        address proposer,
        address[] targets,
        uint256[] values,
        string[] signatures,
        bytes[] calldatas,
        uint256 startBlock,
        uint256 endBlock,
        string description
    );
    event VoteCast(
        address indexed voter,
        uint256 indexed proposalId,
        uint8 support,
        uint256 weight,
        string reason
    );
    event ProposalExecuted(uint256 indexed proposalId);
    event ProposalQueued(uint256 indexed proposalId, uint256 eta);
    event ProposalCanceled(uint256 indexed proposalId);
}

/// Event parser configuration
#[derive(Debug, Clone)]
pub struct EventParserConfig {
    /// AgentIdentity contract address
    pub identity_contract: Option<String>,
    /// AgentDAO contract address
    pub dao_contract: Option<String>,
}

/// Event parser for chain events
#[derive(Clone)]
pub struct ChainEventParser {
    config: EventParserConfig,
}

impl ChainEventParser {
    /// Create new event parser
    #[allow(dead_code)]
    pub fn new(config: EventParserConfig) -> Self {
        Self { config }
    }

    /// Parse a single log entry
    pub fn parse_log(
        &self,
        log: &beebotos_chain::compat::LogEntry,
        block_number: u64,
        tx_hash: &str,
    ) -> Option<ParsedEvent> {
        if log.topics.is_empty() {
            return None;
        }

        let event_signature = log.topics[0];
        let address = hex::encode(&log.address);

        // Check if this is from a known contract
        let is_identity_contract = self
            .config
            .identity_contract
            .as_ref()
            .map(|c| c.eq_ignore_ascii_case(&format!("0x{}", address)))
            .unwrap_or(false);

        let is_dao_contract = self
            .config
            .dao_contract
            .as_ref()
            .map(|c| c.eq_ignore_ascii_case(&format!("0x{}", address)))
            .unwrap_or(false);

        // Try to parse based on event signature
        if is_identity_contract {
            self.parse_identity_event(&event_signature, log, block_number, tx_hash)
        } else if is_dao_contract {
            self.parse_dao_event(&event_signature, log, block_number, tx_hash)
        } else {
            // Unknown contract - return raw event
            Some(ParsedEvent::Unknown {
                address: format!("0x{}", address),
                topics: log.topics.iter().map(|t| hex::encode(t)).collect(),
                data: hex::encode(&log.data),
            })
        }
    }

    /// Parse events from a transaction receipt
    pub fn parse_receipt(
        &self,
        receipt: &beebotos_chain::compat::TransactionReceipt,
        tx_hash: &str,
    ) -> Vec<ParsedEvent> {
        receipt
            .logs
            .iter()
            .filter_map(|log| self.parse_log(log, receipt.block_number, tx_hash))
            .collect()
    }

    /// Parse identity contract events
    fn parse_identity_event(
        &self,
        signature: &[u8; 32],
        log: &beebotos_chain::compat::LogEntry,
        block_number: u64,
        tx_hash: &str,
    ) -> Option<ParsedEvent> {
        let agent_registered_sig = AgentRegistered::SIGNATURE_HASH;
        let agent_updated_sig = AgentUpdated::SIGNATURE_HASH;
        let agent_deactivated_sig = AgentDeactivated::SIGNATURE_HASH;
        let capability_granted_sig = CapabilityGranted::SIGNATURE_HASH;
        let capability_revoked_sig = CapabilityRevoked::SIGNATURE_HASH;

        if signature == agent_registered_sig.as_slice() {
            self.parse_agent_registered(log, block_number, tx_hash)
        } else if signature == agent_updated_sig.as_slice() {
            self.parse_agent_updated(log, block_number, tx_hash)
        } else if signature == agent_deactivated_sig.as_slice() {
            self.parse_agent_deactivated(log, block_number, tx_hash)
        } else if signature == capability_granted_sig.as_slice() {
            self.parse_capability_granted(log, block_number, tx_hash)
        } else if signature == capability_revoked_sig.as_slice() {
            self.parse_capability_revoked(log, block_number, tx_hash)
        } else {
            None
        }
    }

    /// Parse DAO contract events
    fn parse_dao_event(
        &self,
        signature: &[u8; 32],
        log: &beebotos_chain::compat::LogEntry,
        block_number: u64,
        tx_hash: &str,
    ) -> Option<ParsedEvent> {
        let proposal_created_sig = ProposalCreated::SIGNATURE_HASH;
        let vote_cast_sig = VoteCast::SIGNATURE_HASH;
        let proposal_executed_sig = ProposalExecuted::SIGNATURE_HASH;
        let proposal_queued_sig = ProposalQueued::SIGNATURE_HASH;
        let proposal_canceled_sig = ProposalCanceled::SIGNATURE_HASH;

        if signature == proposal_created_sig.as_slice() {
            self.parse_proposal_created(log, block_number, tx_hash)
        } else if signature == vote_cast_sig.as_slice() {
            self.parse_vote_cast(log, block_number, tx_hash)
        } else if signature == proposal_executed_sig.as_slice() {
            self.parse_proposal_executed(log, block_number, tx_hash)
        } else if signature == proposal_queued_sig.as_slice() {
            self.parse_proposal_queued(log, block_number, tx_hash)
        } else if signature == proposal_canceled_sig.as_slice() {
            self.parse_proposal_canceled(log, block_number, tx_hash)
        } else {
            None
        }
    }

    fn parse_agent_registered(
        &self,
        log: &beebotos_chain::compat::LogEntry,
        block_number: u64,
        tx_hash: &str,
    ) -> Option<ParsedEvent> {
        if log.topics.len() < 3 {
            return None;
        }

        let agent_id = hex::encode(&log.topics[1]);
        let owner = format!("0x{}", hex::encode(&log.topics[2][12..])); // Last 20 bytes

        // Parse DID from data
        let did = String::from_utf8_lossy(&log.data).to_string();

        Some(ParsedEvent::IdentityRegistered(IdentityRegisteredEvent {
            agent_id,
            owner,
            did,
            block_number,
            tx_hash: tx_hash.to_string(),
        }))
    }

    fn parse_agent_updated(
        &self,
        log: &beebotos_chain::compat::LogEntry,
        block_number: u64,
        tx_hash: &str,
    ) -> Option<ParsedEvent> {
        if log.topics.len() < 2 {
            return None;
        }

        let agent_id = hex::encode(&log.topics[1]);
        let field = String::from_utf8_lossy(&log.data).to_string();

        Some(ParsedEvent::IdentityUpdated(IdentityUpdatedEvent {
            agent_id,
            field,
            block_number,
            tx_hash: tx_hash.to_string(),
        }))
    }

    fn parse_agent_deactivated(
        &self,
        log: &beebotos_chain::compat::LogEntry,
        block_number: u64,
        tx_hash: &str,
    ) -> Option<ParsedEvent> {
        if log.topics.len() < 2 {
            return None;
        }

        let agent_id = hex::encode(&log.topics[1]);

        Some(ParsedEvent::AgentDeactivated(AgentDeactivatedEvent {
            agent_id,
            block_number,
            tx_hash: tx_hash.to_string(),
        }))
    }

    fn parse_capability_granted(
        &self,
        log: &beebotos_chain::compat::LogEntry,
        block_number: u64,
        tx_hash: &str,
    ) -> Option<ParsedEvent> {
        if log.topics.len() < 3 {
            return None;
        }

        let agent_id = hex::encode(&log.topics[1]);
        let capability = hex::encode(&log.topics[2]);

        Some(ParsedEvent::CapabilityGranted(CapabilityGrantedEvent {
            agent_id,
            capability,
            block_number,
            tx_hash: tx_hash.to_string(),
        }))
    }

    fn parse_capability_revoked(
        &self,
        log: &beebotos_chain::compat::LogEntry,
        block_number: u64,
        tx_hash: &str,
    ) -> Option<ParsedEvent> {
        if log.topics.len() < 3 {
            return None;
        }

        let agent_id = hex::encode(&log.topics[1]);
        let capability = hex::encode(&log.topics[2]);

        Some(ParsedEvent::CapabilityRevoked(CapabilityRevokedEvent {
            agent_id,
            capability,
            block_number,
            tx_hash: tx_hash.to_string(),
        }))
    }

    fn parse_proposal_created(
        &self,
        log: &beebotos_chain::compat::LogEntry,
        block_number: u64,
        tx_hash: &str,
    ) -> Option<ParsedEvent> {
        if log.topics.len() < 2 {
            return None;
        }

        let proposal_id = u64::from_be_bytes([
            log.topics[1][24],
            log.topics[1][25],
            log.topics[1][26],
            log.topics[1][27],
            log.topics[1][28],
            log.topics[1][29],
            log.topics[1][30],
            log.topics[1][31],
        ]);

        // For complex events with dynamic data, we'd need full RLP decoding
        // This is a simplified version
        Some(ParsedEvent::ProposalCreated(ProposalCreatedEvent {
            proposal_id,
            proposer: "0x".to_string(), // Would be decoded from data
            targets: vec![],
            values: vec![],
            signatures: vec![],
            calldatas: vec![],
            start_block: 0,
            end_block: 0,
            description: String::new(),
            block_number,
            tx_hash: tx_hash.to_string(),
        }))
    }

    fn parse_vote_cast(
        &self,
        log: &beebotos_chain::compat::LogEntry,
        block_number: u64,
        tx_hash: &str,
    ) -> Option<ParsedEvent> {
        if log.topics.len() < 3 {
            return None;
        }

        let voter = format!("0x{}", hex::encode(&log.topics[1][12..]));
        let proposal_id = u64::from_be_bytes([
            log.topics[2][24],
            log.topics[2][25],
            log.topics[2][26],
            log.topics[2][27],
            log.topics[2][28],
            log.topics[2][29],
            log.topics[2][30],
            log.topics[2][31],
        ]);

        // Support and weight would be decoded from data
        Some(ParsedEvent::VoteCast(VoteCastEvent {
            voter,
            proposal_id,
            support: 0,
            weight: "0".to_string(),
            reason: String::new(),
            block_number,
            tx_hash: tx_hash.to_string(),
        }))
    }

    fn parse_proposal_executed(
        &self,
        log: &beebotos_chain::compat::LogEntry,
        block_number: u64,
        tx_hash: &str,
    ) -> Option<ParsedEvent> {
        if log.topics.len() < 2 {
            return None;
        }

        let proposal_id = u64::from_be_bytes([
            log.topics[1][24],
            log.topics[1][25],
            log.topics[1][26],
            log.topics[1][27],
            log.topics[1][28],
            log.topics[1][29],
            log.topics[1][30],
            log.topics[1][31],
        ]);

        Some(ParsedEvent::ProposalExecuted(ProposalExecutedEvent {
            proposal_id,
            block_number,
            tx_hash: tx_hash.to_string(),
        }))
    }

    fn parse_proposal_queued(
        &self,
        log: &beebotos_chain::compat::LogEntry,
        block_number: u64,
        tx_hash: &str,
    ) -> Option<ParsedEvent> {
        if log.topics.len() < 2 {
            return None;
        }

        let proposal_id = u64::from_be_bytes([
            log.topics[1][24],
            log.topics[1][25],
            log.topics[1][26],
            log.topics[1][27],
            log.topics[1][28],
            log.topics[1][29],
            log.topics[1][30],
            log.topics[1][31],
        ]);

        Some(ParsedEvent::ProposalQueued(ProposalQueuedEvent {
            proposal_id,
            eta: 0, // Would be decoded from data
            block_number,
            tx_hash: tx_hash.to_string(),
        }))
    }

    fn parse_proposal_canceled(
        &self,
        log: &beebotos_chain::compat::LogEntry,
        block_number: u64,
        tx_hash: &str,
    ) -> Option<ParsedEvent> {
        if log.topics.len() < 2 {
            return None;
        }

        let proposal_id = u64::from_be_bytes([
            log.topics[1][24],
            log.topics[1][25],
            log.topics[1][26],
            log.topics[1][27],
            log.topics[1][28],
            log.topics[1][29],
            log.topics[1][30],
            log.topics[1][31],
        ]);

        Some(ParsedEvent::ProposalCanceled(ProposalCanceledEvent {
            proposal_id,
            block_number,
            tx_hash: tx_hash.to_string(),
        }))
    }
}

/// Get event signature hash from event name
#[allow(dead_code)]
pub fn get_event_signature(event_name: &str) -> String {
    use sha3::{Digest, Keccak256};
    let mut hasher = Keccak256::new();
    hasher.update(event_name.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_signatures() {
        // Verify event signatures match expected values
        let agent_registered_sig = get_event_signature("AgentRegistered(bytes32,address,string)");
        assert_eq!(agent_registered_sig.len(), 64);

        let proposal_created_sig = get_event_signature(
            "ProposalCreated(uint256,address,address[],uint256[],string[],bytes[],uint256,uint256,\
             string)",
        );
        assert_eq!(proposal_created_sig.len(), 64);
    }

    #[test]
    fn test_parse_unknown_log() {
        let parser = ChainEventParser::new(EventParserConfig {
            identity_contract: None,
            dao_contract: None,
        });

        let log = beebotos_chain::compat::LogEntry {
            address: beebotos_core::Address::from_slice(&[0u8; 20]),
            topics: vec![[1u8; 32]],
            data: beebotos_core::Bytes::from(vec![2u8; 64]),
        };

        let result = parser.parse_log(&log, 100, "0xabc");
        assert!(matches!(result, Some(ParsedEvent::Unknown { .. })));
    }
}
