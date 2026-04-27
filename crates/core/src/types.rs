//! Core type definitions for BeeBotOS
//!
//! 🟡 P1 FIX: Unified primitive types across all modules.
//! This module re-exports common primitive types to ensure consistency.

use std::fmt;

// =============================================================================
// 🟡 P1 FIX: Unified Primitive Types
// =============================================================================
// These types are re-exported from alloy-primitives to ensure consistency
// across chain, agents, and gateway modules.
/// Ethereum address type (20 bytes)
///
/// Used for:
/// - Wallet addresses
/// - Contract addresses
/// - Token holders
pub use alloy_primitives::Address;
/// Variable-length byte array
///
/// Used for:
/// - Transaction data
/// - Contract bytecode
/// - ABI-encoded data
pub use alloy_primitives::Bytes;
/// Fixed-size byte array helper
pub use alloy_primitives::FixedBytes;
/// 256-bit fixed-size byte array
///
/// Used for:
/// - Transaction hashes
/// - Block hashes
/// - Storage slots
pub use alloy_primitives::B256;
/// 256-bit unsigned integer type
///
/// Used for:
/// - Token amounts
/// - Gas prices
/// - Block numbers
/// - Timestamps
pub use alloy_primitives::U256;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Transaction hash type alias
pub type TxHash = B256;

/// Block number type alias
pub type BlockNumber = u64;

/// Gas limit type alias
pub type Gas = u64;

/// Wei amount type alias (U256 for large values)
pub type Wei = U256;

// =============================================================================
// Domain-Specific Types
// =============================================================================

/// Unique identifier for Agents
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub Uuid);

impl AgentId {
    /// Create a new random AgentId
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.simple())
    }
}

/// Unique identifier for Sessions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub Uuid);

impl SessionId {
    /// Create a new random SessionId
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Agent types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentType {
    /// Human-controlled agent
    Human,
    /// Autonomous AI agent
    Autonomous,
    /// Hybrid human-AI agent
    Hybrid,
}

/// Agent status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStatus {
    /// Initial state
    None,
    /// Creating
    Spawning,
    /// Idle waiting
    Idle,
    /// Active processing
    Active,
    /// Running task
    Running,
    /// Paused
    Paused,
    /// Error state
    Error,
    /// Cleanup pending
    Zombie,
}

/// Capability levels (10-layer stack)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[repr(u8)]
pub enum CapabilityLevel {
    /// Local computation only
    #[default]
    L0 = 0,
    /// File read access
    L1 = 1,
    /// File write access
    L2 = 2,
    /// Network outbound
    L3 = 3,
    /// Network inbound
    L4 = 4,
    /// Spawn limited agents (max 5)
    L5 = 5,
    /// Spawn unlimited agents
    L6 = 6,
    /// Blockchain read
    L7 = 7,
    /// Blockchain write (low value < 0.1 ETH)
    L8 = 8,
    /// Blockchain write (high value >= 0.1 ETH)
    L9 = 9,
    /// System admin
    L10 = 10,
}

/// Timestamp wrapper
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timestamp(pub DateTime<Utc>);

impl Timestamp {
    /// Current timestamp
    pub fn now() -> Self {
        Self(Utc::now())
    }
}

impl Default for Timestamp {
    fn default() -> Self {
        Self::now()
    }
}

/// Memory types (5-layer model)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryType {
    /// Short-term memory (7±2 items)
    ShortTerm,
    /// Episodic memory (time-space encoded)
    Episodic,
    /// Semantic memory (concept network)
    Semantic,
    /// Procedural memory (action sequences)
    Procedural,
    /// Working memory (current focus)
    Working,
}

/// Proposal types for DAO
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposalType {
    /// Parameter change
    ParameterChange,
    /// Treasury spend
    TreasurySpend,
    /// Member management
    MemberManagement,
    /// Contract upgrade
    ContractUpgrade,
    /// Strategy execution
    StrategyExecution,
    /// Emergency action
    EmergencyAction,
}

/// Vote types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoteType {
    /// Against the proposal
    Against = 0,
    /// For the proposal
    For = 1,
    /// Abstain from voting
    Abstain = 2,
}

/// Task priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum Priority {
    /// Low priority
    Low = 0,
    /// Normal priority
    #[default]
    Normal = 1,
    /// High priority
    High = 2,
    /// Critical priority
    Critical = 3,
}

/// Blockchain chain identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChainId {
    /// Monad mainnet
    Monad,
    /// Monad testnet
    MonadTestnet,
    /// Ethereum
    Ethereum,
    /// Ethereum Sepolia
    Sepolia,
    /// Solana
    Solana,
    /// Cosmos
    Cosmos,
    /// Polkadot
    Polkadot,
}

/// Cross-chain message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossChainMessage {
    /// Source chain
    pub from_chain: ChainId,
    /// Destination chain
    pub to_chain: ChainId,
    /// Sender address
    pub sender: String,
    /// Recipient address
    pub recipient: String,
    /// Message payload
    pub payload: Vec<u8>,
    /// Timestamp
    pub timestamp: Timestamp,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_id_generation() {
        let id1 = AgentId::new();
        let id2 = AgentId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_capability_level_ordering() {
        assert!(CapabilityLevel::L10 > CapabilityLevel::L5);
        assert!(CapabilityLevel::L0 < CapabilityLevel::L1);
    }

    #[test]
    fn test_vote_type_serialization() {
        let vote = VoteType::For;
        let json = serde_json::to_string(&vote).unwrap();
        assert_eq!(json, "\"For\"");
    }
}
