//! Multi-chain support for BeeBotOS
//!
//! Provides unified interfaces for interacting with multiple EVM chains:
//! - Ethereum
//! - BSC (Binance Smart Chain)
//! - Polygon
//! - Arbitrum
//! - Optimism
//! - Base
//! - Beechain
//! - Monad

use alloy_primitives::U256;

pub mod arbitrum;
pub mod base;
pub mod beechain;
pub mod bsc;
pub mod common;
pub mod ethereum;
pub mod monad;
pub mod multichain;
pub mod optimism;
pub mod polygon;

// Re-export common components
// Re-export chain clients
pub use arbitrum::ArbitrumClient;
// Re-export individual chain configs
pub use arbitrum::ArbitrumConfig;
pub use base::{BaseClient, BaseConfig};
pub use beechain::BeechainClient;
pub use bsc::BscClient;
// Re-export chain configs from common::client::chain_configs
pub use common::client::chain_configs::{BeechainConfig, BscConfig, EthereumConfig, MonadConfig};
// Chain network types are defined in this module below and automatically exported

// Re-export token utilities
pub use common::token::{
    chain_formatters, format_native_amount, format_token_amount, parse_native_amount,
    parse_token_amount, BeechainPriority, BscPriority, EthereumPriority, TransactionPriority,
    DEFAULT_TOKEN_DECIMALS, WEI_PER_ETH,
};
pub use common::{
    // Batch Operations
    batch::{BatchOperation, BatchRequest, BatchResponse, BatchResultAggregator, TransactionBatch},
    // Events
    events::{
        EventFilter, EventHandler, EventListener, EventManager, EventProcessor, EventRouter,
        EventStream, MultiChainEventManager, Subscription, SubscriptionConfig, SubscriptionId,
        SubscriptionType,
    },
    // Utilities
    format_native_token,
    // Gas Estimation
    gas::{
        EIP1559FeeEstimate, GasEstimate, GasEstimator, GasEstimatorConfig, OperationGasEstimator,
    },
    parse_native_token,
    // State Cache
    state_cache::{ChainStateCache, StateCacheConfig, StateCacheStatistics},
    // Transaction Queue
    tx_queue::{
        QueueStatistics, QueuedTransaction, TransactionQueue, TxBatchProcessor, TxId,
        TxQueueConfig, TxResult,
    },
    // Client
    BaseEvmClient,
    // Generic chain client
    ChainClient,
    ChainClientBuilder,
    ChainConfig,
    ChainFeatures,
    ContractCall,
    ContractDeploy,
    // Contract
    ContractInstance,
    EvmBlock,
    EvmClient,
    EvmClientExt,
    // Core types
    EvmConfig,
    EvmError,
    EvmEvent,
    // Provider
    EvmProvider,
    EvmTransaction,
    // Mempool
    Mempool,
    // Transaction
    TransactionBuilder,
};
pub use ethereum::EthereumClient;
pub use monad::MonadClient;
// Re-export multi-chain abstraction
pub use multichain::{
    ChainConnection, ChainConnectionStatus, ChainFailover, ChainLoadBalancer, CrossChainRouter,
    LoadBalanceStrategy, MultiChainConfig, MultiChainManager, MultiChainStatistics,
};
pub use optimism::{OptimismClient, OptimismConfig};
pub use polygon::{PolygonClient, PolygonConfig};

/// Chain identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChainId {
    Ethereum = 1,
    EthereumSepolia = 11_155_111,
    Bsc = 56,
    BscTestnet = 97,
    Polygon = 137,
    PolygonMumbai = 80_001,
    PolygonAmoy = 80_002,
    Arbitrum = 42_161,
    ArbitrumSepolia = 421_614,
    ArbitrumNova = 42_170,
    Optimism = 10,
    OptimismSepolia = 11_155_420,
    Base = 8_453,
    BaseSepolia = 84_532,
    Beechain = 3188,
    BeechainTestnet = 30188,
    Monad = 1_014_301,
    MonadTestnet = 10_143,
}

impl ChainId {
    /// Get chain ID from u64
    pub fn from_u64(id: u64) -> Option<Self> {
        match id {
            1 => Some(ChainId::Ethereum),
            11_155_111 => Some(ChainId::EthereumSepolia),
            56 => Some(ChainId::Bsc),
            97 => Some(ChainId::BscTestnet),
            137 => Some(ChainId::Polygon),
            80_001 => Some(ChainId::PolygonMumbai),
            80_002 => Some(ChainId::PolygonAmoy),
            42_161 => Some(ChainId::Arbitrum),
            421_614 => Some(ChainId::ArbitrumSepolia),
            42_170 => Some(ChainId::ArbitrumNova),
            10 => Some(ChainId::Optimism),
            11_155_420 => Some(ChainId::OptimismSepolia),
            8_453 => Some(ChainId::Base),
            84_532 => Some(ChainId::BaseSepolia),
            3188 => Some(ChainId::Beechain),
            30188 => Some(ChainId::BeechainTestnet),
            1_014_301 => Some(ChainId::Monad),
            10_143 => Some(ChainId::MonadTestnet),
            _ => None,
        }
    }

    /// Get chain ID as u64
    pub fn as_u64(&self) -> u64 {
        *self as u64
    }
}

/// Chain family categorization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainFamily {
    Ethereum,
    Bsc,
    Polygon,
    Arbitrum,
    Optimism,
    Base,
    Beechain,
    Monad,
}

impl ChainFamily {
    /// Get chain family from ChainNetwork
    pub fn from_network(network: ChainNetwork) -> Self {
        match network {
            ChainNetwork::Ethereum | ChainNetwork::EthereumSepolia => ChainFamily::Ethereum,
            ChainNetwork::Bsc | ChainNetwork::BscTestnet => ChainFamily::Bsc,
            ChainNetwork::Polygon | ChainNetwork::PolygonMumbai => ChainFamily::Polygon,
            ChainNetwork::Arbitrum | ChainNetwork::ArbitrumSepolia => ChainFamily::Arbitrum,
            ChainNetwork::Optimism | ChainNetwork::OptimismSepolia => ChainFamily::Optimism,
            ChainNetwork::Base | ChainNetwork::BaseSepolia => ChainFamily::Base,
            ChainNetwork::Beechain | ChainNetwork::BeechainTestnet => ChainFamily::Beechain,
            ChainNetwork::Monad | ChainNetwork::MonadTestnet => ChainFamily::Monad,
        }
    }

    /// Get chain family from ChainId
    pub fn from_chain_id(chain_id: ChainId) -> Option<Self> {
        match chain_id {
            ChainId::Ethereum | ChainId::EthereumSepolia => Some(ChainFamily::Ethereum),
            ChainId::Bsc | ChainId::BscTestnet => Some(ChainFamily::Bsc),
            ChainId::Polygon | ChainId::PolygonMumbai | ChainId::PolygonAmoy => {
                Some(ChainFamily::Polygon)
            }
            ChainId::Arbitrum | ChainId::ArbitrumSepolia | ChainId::ArbitrumNova => {
                Some(ChainFamily::Arbitrum)
            }
            ChainId::Optimism | ChainId::OptimismSepolia => Some(ChainFamily::Optimism),
            ChainId::Base | ChainId::BaseSepolia => Some(ChainFamily::Base),
            ChainId::Beechain | ChainId::BeechainTestnet => Some(ChainFamily::Beechain),
            ChainId::Monad | ChainId::MonadTestnet => Some(ChainFamily::Monad),
        }
    }
}

/// Chain network enumeration for multi-chain support
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChainNetwork {
    /// Ethereum Mainnet
    Ethereum,
    /// Ethereum Sepolia Testnet
    EthereumSepolia,
    /// Binance Smart Chain
    Bsc,
    /// BSC Testnet
    BscTestnet,
    /// Monad
    Monad,
    /// Monad Testnet
    MonadTestnet,
    /// Polygon
    Polygon,
    /// Polygon Mumbai Testnet
    PolygonMumbai,
    /// Arbitrum
    Arbitrum,
    /// Arbitrum Sepolia
    ArbitrumSepolia,
    /// Optimism
    Optimism,
    /// Optimism Sepolia
    OptimismSepolia,
    /// Base
    Base,
    /// Base Sepolia
    BaseSepolia,
    /// Beechain - High-performance EVM-compatible L1
    /// - TPS: 10,000
    /// - Block time: 0.4s
    /// - Finality: 0.8s (~2 blocks)
    /// - Parallel EVM execution
    /// - Network Series: Ethereum series
    /// - Chain ID: 3188
    /// - RPC: https://rpc.beechain.ai
    /// - Explorer: https://scan.beechain.ai
    /// - Native Token: BKC
    Beechain,
    /// Beechain Testnet
    /// - Network Series: Ethereum series
    /// - Chain ID: 30188
    /// - RPC: https://testnet-rpc.beechain.ai
    /// - Explorer: https://testnet-scan.beechain.ai
    /// - Native Token: BKC
    BeechainTestnet,
}

impl ChainNetwork {
    /// Get the chain ID for this network
    pub fn chain_id(&self) -> u64 {
        match self {
            ChainNetwork::Ethereum => 1,
            ChainNetwork::EthereumSepolia => 11155111,
            ChainNetwork::Bsc => 56,
            ChainNetwork::BscTestnet => 97,
            ChainNetwork::Monad => 1_014_301,
            ChainNetwork::MonadTestnet => 10_143,
            ChainNetwork::Polygon => 137,
            ChainNetwork::PolygonMumbai => 80001,
            ChainNetwork::Arbitrum => 42161,
            ChainNetwork::ArbitrumSepolia => 421614,
            ChainNetwork::Optimism => 10,
            ChainNetwork::OptimismSepolia => 11155420,
            ChainNetwork::Base => 8453,
            ChainNetwork::BaseSepolia => 84532,
            ChainNetwork::Beechain => 3188,
            ChainNetwork::BeechainTestnet => 30188,
        }
    }

    /// Get the network series
    pub fn network_series(&self) -> &'static str {
        match self {
            ChainNetwork::Ethereum
            | ChainNetwork::EthereumSepolia
            | ChainNetwork::Polygon
            | ChainNetwork::PolygonMumbai
            | ChainNetwork::Arbitrum
            | ChainNetwork::ArbitrumSepolia
            | ChainNetwork::Optimism
            | ChainNetwork::OptimismSepolia
            | ChainNetwork::Base
            | ChainNetwork::BaseSepolia => "Ethereum series",
            ChainNetwork::Bsc | ChainNetwork::BscTestnet => "BSC series",
            ChainNetwork::Beechain | ChainNetwork::BeechainTestnet => "Ethereum series",
            ChainNetwork::Monad | ChainNetwork::MonadTestnet => "Monad series",
        }
    }

    /// Get the network name
    pub fn network_name(&self) -> &'static str {
        match self {
            ChainNetwork::Ethereum => "Ethereum Mainnet",
            ChainNetwork::EthereumSepolia => "Ethereum Sepolia",
            ChainNetwork::Bsc => "BSC Mainnet",
            ChainNetwork::BscTestnet => "BSC Testnet",
            ChainNetwork::Monad => "Monad Mainnet",
            ChainNetwork::MonadTestnet => "Monad Testnet",
            ChainNetwork::Polygon => "Polygon Mainnet",
            ChainNetwork::PolygonMumbai => "Polygon Mumbai",
            ChainNetwork::Arbitrum => "Arbitrum One",
            ChainNetwork::ArbitrumSepolia => "Arbitrum Sepolia",
            ChainNetwork::Optimism => "Optimism",
            ChainNetwork::OptimismSepolia => "Optimism Sepolia",
            ChainNetwork::Base => "Base",
            ChainNetwork::BaseSepolia => "Base Sepolia",
            ChainNetwork::Beechain => "Beechain Mainnet",
            ChainNetwork::BeechainTestnet => "Beechain Testnet",
        }
    }

    /// Get the default RPC URL
    pub fn rpc_url(&self) -> &'static str {
        match self {
            ChainNetwork::Ethereum => "https://eth-mainnet.g.alchemy.com/v2/YOUR_API_KEY",
            ChainNetwork::EthereumSepolia => "https://eth-sepolia.g.alchemy.com/v2/YOUR_API_KEY",
            ChainNetwork::Bsc => "https://bsc-dataseed.binance.org",
            ChainNetwork::BscTestnet => "https://data-seed-prebsc-1-s1.binance.org:8545",
            ChainNetwork::Monad => "https://rpc.monad.xyz",
            ChainNetwork::MonadTestnet => "https://testnet-rpc.monad.xyz",
            ChainNetwork::Polygon => "https://polygon-mainnet.g.alchemy.com/v2/YOUR_API_KEY",
            ChainNetwork::PolygonMumbai => "https://polygon-mumbai.g.alchemy.com/v2/YOUR_API_KEY",
            ChainNetwork::Arbitrum => "https://arb-mainnet.g.alchemy.com/v2/YOUR_API_KEY",
            ChainNetwork::ArbitrumSepolia => "https://arb-sepolia.g.alchemy.com/v2/YOUR_API_KEY",
            ChainNetwork::Optimism => "https://opt-mainnet.g.alchemy.com/v2/YOUR_API_KEY",
            ChainNetwork::OptimismSepolia => "https://opt-sepolia.g.alchemy.com/v2/YOUR_API_KEY",
            ChainNetwork::Base => "https://base-mainnet.g.alchemy.com/v2/YOUR_API_KEY",
            ChainNetwork::BaseSepolia => "https://base-sepolia.g.alchemy.com/v2/YOUR_API_KEY",
            ChainNetwork::Beechain => "https://rpc.beechain.ai",
            ChainNetwork::BeechainTestnet => "https://testnet-rpc.beechain.ai",
        }
    }

    /// Get the explorer URL
    pub fn explorer_url(&self) -> &'static str {
        match self {
            ChainNetwork::Ethereum => "https://etherscan.io",
            ChainNetwork::EthereumSepolia => "https://sepolia.etherscan.io",
            ChainNetwork::Bsc => "https://bscscan.com",
            ChainNetwork::BscTestnet => "https://testnet.bscscan.com",
            ChainNetwork::Monad => "https://explorer.monad.xyz",
            ChainNetwork::MonadTestnet => "https://testnet.explorer.monad.xyz",
            ChainNetwork::Polygon => "https://polygonscan.com",
            ChainNetwork::PolygonMumbai => "https://mumbai.polygonscan.com",
            ChainNetwork::Arbitrum => "https://arbiscan.io",
            ChainNetwork::ArbitrumSepolia => "https://sepolia.arbiscan.io",
            ChainNetwork::Optimism => "https://optimistic.etherscan.io",
            ChainNetwork::OptimismSepolia => "https://sepolia-optimism.etherscan.io",
            ChainNetwork::Base => "https://basescan.org",
            ChainNetwork::BaseSepolia => "https://sepolia.basescan.org",
            ChainNetwork::Beechain => "https://scan.beechain.ai",
            ChainNetwork::BeechainTestnet => "https://testnet-scan.beechain.ai",
        }
    }

    /// Get the native token symbol
    pub fn native_token(&self) -> &'static str {
        match self {
            ChainNetwork::Ethereum | ChainNetwork::EthereumSepolia => "ETH",
            ChainNetwork::Bsc | ChainNetwork::BscTestnet => "BNB",
            ChainNetwork::Monad | ChainNetwork::MonadTestnet => "MON",
            ChainNetwork::Polygon | ChainNetwork::PolygonMumbai => "MATIC",
            ChainNetwork::Arbitrum
            | ChainNetwork::ArbitrumSepolia
            | ChainNetwork::Optimism
            | ChainNetwork::OptimismSepolia
            | ChainNetwork::Base
            | ChainNetwork::BaseSepolia => "ETH",
            ChainNetwork::Beechain | ChainNetwork::BeechainTestnet => "BKC",
        }
    }

    /// Check if this is a mainnet network
    pub fn is_mainnet(&self) -> bool {
        matches!(
            self,
            ChainNetwork::Ethereum
                | ChainNetwork::Bsc
                | ChainNetwork::Monad
                | ChainNetwork::Polygon
                | ChainNetwork::Arbitrum
                | ChainNetwork::Optimism
                | ChainNetwork::Base
                | ChainNetwork::Beechain
        )
    }

    /// Check if this is a testnet network
    pub fn is_testnet(&self) -> bool {
        !self.is_mainnet()
    }

    /// Get Beechain-specific network info (if applicable)
    pub fn beechain_info(&self) -> Option<BeechainNetworkInfo> {
        match self {
            ChainNetwork::Beechain => Some(BeechainNetworkInfo {
                tps: 10_000,
                block_time_ms: 400,
                finality_time_ms: 800,
                confirmation_blocks: 2,
                parallel_execution: true,
            }),
            ChainNetwork::BeechainTestnet => Some(BeechainNetworkInfo {
                tps: 10_000,
                block_time_ms: 400,
                finality_time_ms: 800,
                confirmation_blocks: 2,
                parallel_execution: true,
            }),
            _ => None,
        }
    }
}

/// Beechain network-specific information
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeechainNetworkInfo {
    /// Transactions per second capacity
    pub tps: u32,
    /// Block time in milliseconds
    pub block_time_ms: u64,
    /// Finality time in milliseconds
    pub finality_time_ms: u64,
    /// Confirmation blocks needed for finality
    pub confirmation_blocks: u64,
    /// Whether parallel execution is enabled
    pub parallel_execution: bool,
}

// Note: ChainConfig trait is re-exported from common::client

/// Chain health status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainHealth {
    Healthy,
    Degraded,
    Unhealthy,
}

/// Chain metrics
#[derive(Debug, Clone)]
pub struct ChainMetrics {
    pub chain_id: u64,
    pub block_height: u64,
    pub gas_price: U256,
    pub peer_count: usize,
    pub health: ChainHealth,
}

/// Chain registry for managing multiple chains
#[derive(Debug, Clone, Default)]
pub struct ChainRegistry {
    chains: std::collections::HashMap<u64, ChainMetrics>,
}

impl ChainRegistry {
    pub fn new() -> Self {
        Self {
            chains: std::collections::HashMap::new(),
        }
    }

    pub fn register(&mut self, chain_id: u64, metrics: ChainMetrics) {
        self.chains.insert(chain_id, metrics);
    }

    pub fn get(&self, chain_id: u64) -> Option<&ChainMetrics> {
        self.chains.get(&chain_id)
    }

    pub fn all(&self) -> Vec<&ChainMetrics> {
        self.chains.values().collect()
    }
}

// Token utilities and macros are already re-exported from common module above
