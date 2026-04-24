//! BeeBotOS Chain Layer
//!
//! Blockchain integration and DAO governance client using Alloy.

pub mod bindings;
pub mod bridge;
pub mod cache;
pub mod chains; // Multi-chain support: Ethereum, BSC, Beechain, Monad
pub mod compat;
pub mod config;
pub mod constants; // Centralized constants
pub mod contracts;
pub mod dao;
pub mod defi;
pub mod deployment;
pub mod events;
pub mod health;
pub mod identity;
pub mod message_bus;
pub mod metrics;
pub mod oracle;
pub mod security;
pub mod telemetry;
pub mod wallet;

#[cfg(test)]
pub mod test_utils;

// Re-export mempool from chains::common module (was monad)
// Re-export bridge types
pub use bridge::{
    AtomicSwapClient,
    Bridge,
    BridgeClient,
    BridgeRequestInfo,
    BridgeState,
    BridgeStatus,
    BridgeTx,
    ChainId as BridgeChainId, // Re-export with alias to avoid conflict
    HTLC,
};
// Re-export cache types
pub use cache::{
    BlockCache, CacheEntry, CacheManager, CacheManagerStats, CacheStats, ContractCache,
    IdentityCache, PersistentCache,
};
// Re-export Beechain types
pub use chains::beechain::types::{
    format_bkc, parse_bkc, BeechainBlock, BeechainEvent, BeechainTransaction,
};
pub use chains::beechain::{BeechainClient, BeechainConfig, BeechainError};
// Re-export BSC types
pub use chains::bsc::types::{format_bnb, parse_bnb, BscBlock, BscEvent, BscTransaction};
pub use chains::bsc::{BscClient, BscConfig, BscError};
// Re-export event types from chains::common (was monad)
pub use chains::common::events::SubscriptionType;
pub use chains::common::events::{
    EventFilter, EventHandler, EventListener, EventProcessor, EventStream,
};
pub use chains::common::mempool;
// Re-export Monad-specific types
pub use chains::common::TransactionBuilder;
// Re-export Ethereum types
pub use chains::ethereum::types::{
    format_eth as format_eth_eth, parse_eth as parse_eth_eth, EthereumBlock, EthereumEvent,
    EthereumTransaction,
};
pub use chains::ethereum::{EthereumClient, EthereumConfig, EthereumError};
pub use chains::monad::types::{format_eth, parse_eth, MonadBlock, MonadEvent, MonadTransaction};
pub use chains::monad::{MonadClient, MonadConfig, MonadError};
// Re-export multi-chain utilities
pub use chains::{
    chain_formatters, format_native_amount, format_token_amount, parse_native_amount,
    parse_token_amount, BeechainPriority, BscPriority, EthereumPriority, TransactionPriority,
    DEFAULT_TOKEN_DECIMALS, WEI_PER_ETH,
};
// Re-export common EVM components (new)
pub use chains::{
    // Client
    BaseEvmClient,
    // Batch Operations
    BatchOperation,
    BatchRequest,
    BatchResponse,
    BatchResultAggregator,
    BeechainConfig as GenericBeechainConfig,
    BscConfig as GenericBscConfig,
    // Generic chain client
    ChainClient,
    ChainClientBuilder,
    ChainConfig,
    // State Cache (Balance & Nonce)
    ChainStateCache,
    ContractCall,
    ContractDeploy,
    // Contract
    ContractInstance,
    EIP1559FeeEstimate,
    EthereumConfig as GenericEthereumConfig,
    // Events
    EventFilter as CommonEventFilter,
    EventListener as CommonEventListener,
    // Event Manager
    EventManager,
    EventRouter,
    EventStream as CommonEventStream,
    EvmBlock as CommonEvmBlock,
    EvmClient as EvmClientTrait,
    EvmClientExt,
    // Core types
    EvmConfig as CommonEvmConfig,
    EvmError as CommonEvmError,
    // Provider
    EvmProvider as CommonEvmProvider,
    EvmTransaction as CommonEvmTransaction,
    GasEstimate,
    // Gas Estimation
    GasEstimator,
    GasEstimatorConfig,
    // TransactionPriority is already exported above
    // Mempool
    Mempool as CommonMempool,
    MonadConfig as GenericMonadConfig,
    MultiChainEventManager,
    OperationGasEstimator,
    QueueStatistics,
    QueuedTransaction,
    StateCacheConfig,
    StateCacheStatistics,
    Subscription,
    SubscriptionConfig,
    SubscriptionId,
    TransactionBatch,
    // Transaction
    TransactionBuilder as CommonTransactionBuilder,
    // Transaction Queue
    TransactionQueue,
    TxBatchProcessor,
    TxId,
    TxQueueConfig,
    TxResult,
};
// Re-export multi-chain abstraction (CrossChainRouter from chains::multichain)
pub use chains::{
    ChainConnection, ChainConnectionStatus, ChainFailover, ChainLoadBalancer, CrossChainRouter,
    LoadBalanceStrategy, MultiChainConfig, MultiChainManager, MultiChainStatistics,
};
pub use chains::{ChainFamily, ChainHealth, ChainMetrics, ChainRegistry};
// Re-export provider types
pub use compat::provider::{AlloyClient, Provider};
// Re-export retry utilities
pub use compat::retry::{
    with_retry, with_retry_and_handler, CircuitBreaker, RateLimiter, RetryConfig,
};
// Use Alloy types as primary types
pub use compat::{Address, BlockNumber, Bytes, TxHash, B256, U256};
// Re-export event types from contracts::events
pub use contracts::events::{
    AgentDeactivated, AgentRegistered, AgentUpdated, Approval, AvailabilityChanged, BeeBotOSEvent,
    BeeBotOSEventFilter, BeeBotOSEventListener, BeeBotOSEventStream, BeeBotOSEventType,
    BridgeCompleted, BridgeFailed, BridgeInitiated, BudgetCreated, BudgetReleased,
    CategoryScoreUpdated, DisputeRaised, DisputeResolved, EscrowCreated, EscrowRefunded,
    EscrowReleased, EvidenceSubmitted, Heartbeat, ListingCancelled, ListingCreated,
    ListingFulfilled, MandateCreated, MetadataUpdated, PaymentExecuted, ProposalCreated,
    ProposalExecuted, PurchaseMade, ReputationUpdated, RoyaltyUpdated, SkillMinted, StreamCreated,
    StreamUpdated, Transfer, VoteCast,
};
// Re-export contract types
pub use contracts::{
    multicall::{Call, Call3, MulticallBatch, MulticallResult},
    A2ACommerce,
    // BeeBotOS contract bindings (all from bindings module)
    AgentDAO,
    AgentIdentity,
    AgentIdentityInfo,
    AgentMetadata,
    AgentPayment,
    AgentRegistry,
    BeeToken,
    BridgeRequest,
    CallOptions,
    ContractAbi,
    ContractCacheStats,
    ContractCallBuilder,
    ContractCaller,
    ContractDeployer,
    CrossChainBridge,
    DealEscrow,
    DeploymentResult,
    DisputeResolution,
    DisputeStatus,
    FunctionAbi,
    Multicall3,
    PaymentMandate,
    ReputationSystem,
    Resolution,
    SkillNFT,
    StateMutability,
    Stream,
    TransactionHelper,
    TreasuryManager,
};
// Re-export DAO types
pub use dao::delegation::Delegation;
pub use dao::governance::GovernanceParams;
pub use dao::proposal::{ProposalAction, ProposalInfo, ProposalStatus};
pub use dao::treasury::{Budget, TreasuryAsset};
pub use dao::voting::{Vote, VoteCounter, VotingSnapshot};
pub use dao::{
    DAOClient, DAOInterface, Proposal, ProposalBuilder, ProposalId, ProposalType, VoteType,
};
pub use defi::{LendingProtocol, SwapParams, SwapResult, DEX};
// Re-export deployment types
pub use deployment::{
    ContractDeploymentConfig, ContractInfo, ContractRegistry, Deployer, DeploymentReceipt,
    NetworkConfig, RegistryManager,
};
// Re-export event types
pub use events::{ChainConnectionEvent, ChainEvent};
// Re-export health types
pub use health::{
    ComponentHealth, HealthCheckConfig, HealthCheckResponse, HealthChecker, HealthEndpoint,
    HealthStatus, LivenessResponse, ReadinessResponse,
};
// Re-export identity types
pub use identity::registry::{
    AgentId, AgentInfo, CachedIdentityRegistry, IdentityRegistrationBuilder,
    OnChainIdentityRegistry,
};
pub use identity::resolver::SimpleDIDResolver;
pub use identity::{DIDDocument, DIDResolver, IdentityRegistry, VerificationMethod};
// 🟢 P1 FIX: Message Bus integration
pub use message_bus::{
    init_message_bus, message_bus, ChainMessageBus, ChainTransactionEvent, DaoEvent,
};
// Re-export metrics types
#[cfg(feature = "prometheus")]
pub use metrics::init_prometheus_exporter;
pub use metrics::{
    timed_operation, ContractMetrics, EventMetrics, MetricsCollector, OperationStats,
    OperationType, WalletMetrics,
};
pub use oracle::{AggregatedOracle, PriceData, PriceFeed};
// Re-export security types
pub use security::{
    CallRateLimiter, InputSanitizer, NonceManager, ReentrancyGuard, ReentrancyLock, SecurityAudit,
    SecurityValidator,
};
// Re-export telemetry types
pub use telemetry::{init_telemetry, Instrument, Telemetry, TelemetryConfig, TraceContext};
pub use wallet::{AccountInfo, EncryptedMnemonic, HDWallet, KeyStore, Wallet, WalletConfig};

// Macros are re-exported from chains::common module

/// Chain layer result type
pub type Result<T> = std::result::Result<T, ChainError>;

/// Chain result alias (for backwards compatibility)
pub type ChainResult<T> = Result<T>;

// Error module
pub mod error;
pub use error::ChainError;

// Module version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
