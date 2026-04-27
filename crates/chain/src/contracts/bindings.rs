//! BeeBotOS Contract Bindings
//!
//! Complete smart contract bindings for the BeeBotOS platform.
//! These can be deployed on any EVM-compatible chain (Ethereum, BSC, Beechain,
//! Monad, etc.)
//!
//! ## Contract Categories
//!
//! ### Core DAO System
//! - `AgentDAO` - Governance and proposal management
//! - `BeeToken` - Platform utility token
//! - `TreasuryManager` - Budget and fund management
//!
//! ### Identity & Commerce
//! - `AgentIdentity` - Agent registration and identity management
//! - `A2ACommerce` - Agent-to-agent marketplace
//! - `DealEscrow` - Secure transaction escrow
//!
//! ### Payment & Streaming
//! - `AgentPayment` - Payment mandates and streaming payments
//!
//! ### Discovery & Registry
//! - `AgentRegistry` - Agent metadata and discovery service
//!
//! ### Skills & Reputation
//! - `SkillNFT` - Skill tokenization and royalties
//! - `ReputationSystem` - On-chain reputation tracking
//!
//! ### Cross-Chain
//! - `CrossChainBridge` - Bridge operations across chains
//!
//! ### Dispute Resolution
//! - `DisputeResolution` - Escrow dispute arbitration

use alloy_sol_types::sol;

// ============================================================================
// Core DAO System
// ============================================================================

sol! {
    #[sol(rpc)]
    contract AgentDAO {
        function getProposalCount() external view returns (uint256);

        function getProposal(uint256 proposalId) external view returns (
            address proposer,
            string memory description,
            uint256 forVotes,
            uint256 againstVotes,
            uint256 abstainVotes,
            bool executed
        );

        function propose(
            address[] memory targets,
            uint256[] memory values,
            bytes[] memory calldatas,
            string memory description
        ) external returns (uint256 proposalId);

        function castVote(uint256 proposalId, uint8 support) external;

        function execute(uint256 proposalId) external;

        function queue(uint256 proposalId) external;

        function cancel(uint256 proposalId) external;

        function state(uint256 proposalId) external view returns (uint8);

        function getVotes(address account) external view returns (uint256);

        function delegate(address delegatee) external;

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
}

sol! {
    #[sol(rpc)]
    contract BeeToken {
        function balanceOf(address account) external view returns (uint256);
        function transfer(address to, uint256 amount) external returns (bool);
        function allowance(address owner, address spender) external view returns (uint256);
        function approve(address spender, uint256 amount) external returns (bool);
        function decimals() external view returns (uint8);
        function symbol() external view returns (string memory);

        event Transfer(address indexed from, address indexed to, uint256 value);
        event Approval(address indexed owner, address indexed spender, uint256 value);
    }
}

sol! {
    #[sol(rpc)]
    contract TreasuryManager {
        function createBudget(
            address beneficiary,
            uint256 amount,
            address token,
            uint256 startTime,
            uint256 endTime,
            uint8 budgetType
        ) external returns (uint256 budgetId);

        function releaseBudget(uint256 budgetId) external;

        function getBudget(uint256 budgetId) external view returns (
            address beneficiary,
            uint256 amount,
            address token,
            uint256 startTime,
            uint256 endTime,
            uint8 budgetType,
            bool released
        );

        event BudgetCreated(uint256 indexed budgetId);
        event BudgetReleased(uint256 indexed budgetId);
    }
}

// ============================================================================
// Identity & Commerce
// ============================================================================

sol! {
    #[sol(rpc)]
    contract AgentIdentity {
        struct AgentIdentityInfo {
            bytes32 agentId;
            address owner;
            string did;
            bytes32 publicKey;
            bool isActive;
            uint256 reputation;
            uint256 createdAt;
        }

        function registerAgent(string calldata did, bytes32 publicKey) external returns (bytes32 agentId);
        function getAgent(bytes32 agentId) external view returns (AgentIdentityInfo memory);
        function didToAgent(string calldata did) external view returns (bytes32);
        function totalAgents() external view returns (uint256);
        function deactivateAgent(bytes32 agentId) external;
        function updateReputation(bytes32 agentId, uint256 newReputation) external;
        function hasCapability(bytes32 agentId, bytes32 capability) external view returns (bool);
        function getOwnerAgents(address owner) external view returns (bytes32[] memory);
        function grantCapability(bytes32 agentId, bytes32 capability) external;
        function revokeCapability(bytes32 agentId, bytes32 capability) external;

        event AgentRegistered(bytes32 indexed agentId, address indexed owner, string did);
        event AgentUpdated(bytes32 indexed agentId, string field);
        event AgentDeactivated(bytes32 indexed agentId);
        event CapabilityGranted(bytes32 indexed agentId, bytes32 capability);
        event CapabilityRevoked(bytes32 indexed agentId, bytes32 capability);
    }
}

sol! {
    #[sol(rpc)]
    contract A2ACommerce {
        function createListing(address agent, uint256 price, string memory metadata) external returns (uint256 listingId);
        function purchase(uint256 listingId) external payable;
        function fulfill(uint256 listingId) external;
        function cancelListing(uint256 listingId) external;

        event ListingCreated(uint256 indexed listingId, address indexed agent, uint256 price);
        event PurchaseMade(uint256 indexed listingId, address indexed buyer);
        event ListingFulfilled(uint256 indexed listingId);
        event ListingCancelled(uint256 indexed listingId);
    }
}

sol! {
    #[sol(rpc)]
    contract DealEscrow {
        function createEscrow(bytes32 dealId, address buyer, address seller, address token, uint256 amount) external payable returns (bytes32 escrowId);
        function releaseEscrow(bytes32 escrowId, uint256 sellerAmount) external;
        function refundEscrow(bytes32 escrowId) external;
        function getEscrow(bytes32 escrowId) external view returns (
            bytes32 escrowIdOut,
            bytes32 dealId,
            address buyer,
            address seller,
            address token,
            uint256 amount,
            uint256 fee,
            uint256 createdAt,
            uint256 releasedAt,
            bool isReleased,
            bool isRefunded
        );

        event EscrowCreated(bytes32 indexed escrowId, bytes32 indexed dealId, address indexed buyer, address seller, uint256 amount);
        event EscrowReleased(bytes32 indexed escrowId, address indexed seller, uint256 amount);
        event EscrowRefunded(bytes32 indexed escrowId, address indexed buyer, uint256 amount);
    }
}

// ============================================================================
// Payment & Streaming
// ============================================================================

sol! {
    #[sol(rpc)]
    contract AgentPayment {
        struct PaymentMandate {
            bytes32 mandateId;
            address payer;
            address payee;
            address token;
            uint256 maxAmount;
            uint256 usedAmount;
            uint256 validUntil;
            bool isActive;
        }

        struct Stream {
            bytes32 streamId;
            address sender;
            address recipient;
            uint256 totalAmount;
            uint256 releasedAmount;
            uint256 startTime;
            uint256 endTime;
            bool isActive;
        }

        function createMandate(address payee, address token, uint256 maxAmount, uint256 validUntil)
            external returns (bytes32 mandateId);

        function createStream(address recipient, uint256 totalAmount, uint256 duration)
            external payable returns (bytes32 streamId);

        function withdrawFromStream(bytes32 streamId) external returns (uint256);
        function getPendingAmount(bytes32 streamId) external view returns (uint256);

        function mandates(bytes32) external view returns (
            bytes32 mandateId,
            address payer,
            address payee,
            address token,
            uint256 maxAmount,
            uint256 usedAmount,
            uint256 validUntil,
            bool isActive
        );

        function streams(bytes32) external view returns (
            bytes32 streamId,
            address sender,
            address recipient,
            uint256 totalAmount,
            uint256 releasedAmount,
            uint256 startTime,
            uint256 endTime,
            bool isActive
        );

        event MandateCreated(bytes32 indexed mandateId, address indexed payer, uint256 maxAmount);
        event StreamCreated(bytes32 indexed streamId, address indexed sender, uint256 totalAmount);
        event StreamUpdated(bytes32 indexed streamId, uint256 releasedAmount);
        event PaymentExecuted(bytes32 indexed mandateId, bytes32 indexed paymentId, uint256 amount);
    }
}

// ============================================================================
// Discovery & Registry
// ============================================================================

sol! {
    #[sol(rpc)]
    contract AgentRegistry {
        struct AgentMetadata {
            bytes32 agentId;
            string name;
            string description;
            string[] capabilities;
            string endpoint;
            uint256 version;
            bool isAvailable;
            uint256 lastHeartbeat;
        }

        function initialize(address identityAddress) external;
        function registerMetadata(
            bytes32 agentId,
            string calldata name,
            string calldata description,
            string[] calldata capabilities,
            string calldata endpoint
        ) external;
        function heartbeat(bytes32 agentId) external;
        function setAvailability(bytes32 agentId, bool isAvailable) external;
        function findAgentsByCapability(string calldata capability) external view returns (bytes32[] memory);
        function isAgentAvailable(bytes32 agentId) external view returns (bool);

        function metadata(bytes32) external view returns (
            bytes32 agentId,
            string memory name,
            string memory description,
            string memory endpoint,
            uint256 version,
            bool isAvailable,
            uint256 lastHeartbeat
        );

        function capabilityIndex(string calldata, uint256) external view returns (bytes32);
        function availableAgents(uint256) external view returns (bytes32);
        function identityContract() external view returns (address);
        function HEARTBEAT_TIMEOUT() external pure returns (uint256);

        event MetadataUpdated(bytes32 indexed agentId, string name);
        event Heartbeat(bytes32 indexed agentId, uint256 timestamp);
        event AvailabilityChanged(bytes32 indexed agentId, bool isAvailable);
    }
}

// ============================================================================
// Skills & Reputation
// ============================================================================

sol! {
    #[sol(rpc)]
    contract SkillNFT {
        function mintSkill(string calldata name, string calldata version, string calldata metadataURI, bool isTransferable) external returns (uint256 tokenId);
        function getSkill(uint256 tokenId) external view returns (
            uint256 tokenIdOut,
            address creator,
            string memory name,
            string memory version,
            string memory metadataURI,
            bool isTransferable,
            uint256 createdAt
        );
        function setTokenRoyalty(uint256 tokenId, uint96 royaltyBps) external;
        function royaltyInfo(uint256 tokenId, uint256 salePrice) external view returns (address receiver, uint256 royaltyAmount);

        event SkillMinted(uint256 indexed tokenId, address indexed creator, string name, string version, uint96 royaltyBps);
        event RoyaltyUpdated(uint256 indexed tokenId, uint96 newRoyaltyBps);
    }
}

sol! {
    #[sol(rpc)]
    contract ReputationSystem {
        function updateReputation(address account, int256 delta, string calldata reason) external;
        function getReputation(address account) external view returns (uint256);
        function getCategoryScore(address account, bytes32 category) external view returns (uint256);
        function calculateVotingPower(address account) external view returns (uint256);

        event ReputationUpdated(address indexed account, int256 delta, uint256 newScore, string reason);
        event CategoryScoreUpdated(address indexed account, bytes32 indexed category, uint256 score, int256 delta);
    }
}

// ============================================================================
// Cross-Chain
// ============================================================================

sol! {
    #[sol(rpc)]
    contract CrossChainBridge {
        struct BridgeRequest {
            bytes32 requestId;
            address sender;
            address recipient;
            uint256 amount;
            address token;
            uint256 targetChain;
            bytes32 targetToken;
            uint8 state;
            uint256 timestamp;
        }

        function bridgeOut(address token, uint256 amount, uint256 targetChain, bytes32 targetToken, address recipient) external payable returns (bytes32 requestId);
        function bridgeIn(bytes32 requestId, address recipient, uint256 amount, address token, bytes[] calldata signatures) external;
        function verifyCrossChainProof(bytes32 requestId, address recipient, uint256 amount, address token, bytes[] calldata signatures) external view returns (bool);
        function refund(bytes32 requestId) external;
        function addSupportedChain(uint256 chainId) external;
        function removeSupportedChain(uint256 chainId) external;
        function addSupportedToken(address token) external;
        function removeSupportedToken(address token) external;
        function setFee(uint256 newFee) external;
        function withdrawFees(address token) external;

        function requests(bytes32) external view returns (
            bytes32 requestId,
            address sender,
            address recipient,
            uint256 amount,
            address token,
            uint256 targetChain,
            bytes32 targetToken,
            uint8 state,
            uint256 timestamp
        );

        function supportedChains(uint256) external view returns (bool);
        function supportedTokens(address) external view returns (bool);
        function completedRequests(bytes32) external view returns (bool);
        function feeBasisPoints() external view returns (uint256);

        event BridgeInitiated(bytes32 indexed requestId, address indexed sender, uint256 targetChain, uint256 amount, uint256 nonce);
        event BridgeCompleted(bytes32 indexed requestId, address indexed recipient, uint256 amount, uint256 validatorCount);
        event BridgeFailed(bytes32 indexed requestId, string reason);
    }
}

// ============================================================================
// Dispute Resolution
// ============================================================================

sol! {
    #[sol(rpc)]
    contract DisputeResolution {
        enum DisputeStatus {
            Open,
            Evidence,
            Voting,
            Resolved,
            Appealed
        }

        enum Resolution {
            Pending,
            RefundBuyer,
            ReleaseToSeller,
            Split
        }

        function raiseDispute(bytes32 dealId, string calldata reason, bytes32 evidenceHash)
            external payable returns (bytes32);
        function submitEvidence(bytes32 disputeId, bytes32 evidenceHash) external;
        function startVoting(bytes32 disputeId) external;
        function castVote(bytes32 disputeId, bool supportRefund, string calldata justification) external;
        function finalizeVoting(bytes32 disputeId) external;
        function addArbiter(address arbiter) external;
        function removeArbiter(address arbiter) external;

        function disputes(bytes32) external view returns (
            bytes32 disputeId,
            bytes32 dealId,
            address plaintiff,
            address defendant,
            string memory reason,
            uint8 status,
            uint256 createdAt,
            uint256 votingEndsAt,
            uint8 resolution,
            uint256 refundAmount
        );

        event DisputeRaised(bytes32 indexed disputeId, bytes32 indexed dealId, address plaintiff);
        event EvidenceSubmitted(bytes32 indexed disputeId, address submitter, bytes32 evidenceHash);
        event VoteCast(bytes32 indexed disputeId, address arbiter, bool supportRefund);
        event DisputeResolved(bytes32 indexed disputeId, uint8 resolution);
    }
}

// ============================================================================
// Re-exports
// ============================================================================

pub use AgentPayment::{PaymentMandate, Stream};
pub use AgentRegistry::AgentMetadata;
pub use CrossChainBridge::BridgeRequest;
pub use DisputeResolution::{DisputeStatus, Resolution};
