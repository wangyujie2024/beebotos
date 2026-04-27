//! Multi-Chain Configuration Module
//!
//! Provides support for multiple blockchain networks (Ethereum, BSC, Monad,
//! etc.) with per-chain contract address configuration.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Supported blockchain networks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
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
    Beechain,
    /// Beechain Testnet
    BeechainTestnet,
    /// Custom network
    Custom(u64),
}

#[allow(dead_code)]
impl ChainNetwork {
    /// Get chain ID
    pub fn chain_id(&self) -> u64 {
        match self {
            ChainNetwork::Ethereum => 1,
            ChainNetwork::EthereumSepolia => 11155111,
            ChainNetwork::Bsc => 56,
            ChainNetwork::BscTestnet => 97,
            ChainNetwork::Monad => 1, // TODO: Update when Monad mainnet launches
            ChainNetwork::MonadTestnet => 10143,
            ChainNetwork::Polygon => 137,
            ChainNetwork::PolygonMumbai => 80001,
            ChainNetwork::Arbitrum => 42161,
            ChainNetwork::ArbitrumSepolia => 421614,
            ChainNetwork::Optimism => 10,
            ChainNetwork::OptimismSepolia => 11155420,
            ChainNetwork::Base => 8453,
            ChainNetwork::BaseSepolia => 84532,
            ChainNetwork::Beechain => 3188,
            ChainNetwork::BeechainTestnet => 3189, // 假设的测试网链ID
            ChainNetwork::Custom(id) => *id,
        }
    }

    /// Get default RPC URL
    #[allow(dead_code)]
    pub fn default_rpc_url(&self) -> Option<&'static str> {
        match self {
            ChainNetwork::Ethereum => Some("https://ethereum-rpc.publicnode.com"),
            ChainNetwork::EthereumSepolia => Some("https://ethereum-sepolia-rpc.publicnode.com"),
            ChainNetwork::Bsc => Some("https://bsc-rpc.publicnode.com"),
            ChainNetwork::BscTestnet => Some("https://bsc-testnet-rpc.publicnode.com"),
            ChainNetwork::Monad => None, // Not available yet
            ChainNetwork::MonadTestnet => Some("https://testnet-rpc.monad.xyz"),
            ChainNetwork::Polygon => Some("https://polygon-rpc.com"),
            ChainNetwork::PolygonMumbai => Some("https://rpc-mumbai.maticvigil.com"),
            ChainNetwork::Arbitrum => Some("https://arbitrum-one.publicnode.com"),
            ChainNetwork::ArbitrumSepolia => Some("https://arbitrum-sepolia.publicnode.com"),
            ChainNetwork::Optimism => Some("https://optimism-rpc.publicnode.com"),
            ChainNetwork::OptimismSepolia => Some("https://optimism-sepolia-rpc.publicnode.com"),
            ChainNetwork::Base => Some("https://base-rpc.publicnode.com"),
            ChainNetwork::BaseSepolia => Some("https://base-sepolia-rpc.publicnode.com"),
            ChainNetwork::Beechain => Some("https://rpc.beechain.ai"),
            ChainNetwork::BeechainTestnet => Some("https://testnet-rpc.beechain.ai"),
            ChainNetwork::Custom(_) => None,
        }
    }

    /// Get block explorer URL
    #[allow(dead_code)]
    pub fn explorer_url(&self) -> Option<&'static str> {
        match self {
            ChainNetwork::Ethereum => Some("https://etherscan.io"),
            ChainNetwork::EthereumSepolia => Some("https://sepolia.etherscan.io"),
            ChainNetwork::Bsc => Some("https://bscscan.com"),
            ChainNetwork::BscTestnet => Some("https://testnet.bscscan.com"),
            ChainNetwork::Monad => None,
            ChainNetwork::MonadTestnet => Some("https://testnet.monadexplorer.com"),
            ChainNetwork::Polygon => Some("https://polygonscan.com"),
            ChainNetwork::PolygonMumbai => Some("https://mumbai.polygonscan.com"),
            ChainNetwork::Arbitrum => Some("https://arbiscan.io"),
            ChainNetwork::ArbitrumSepolia => Some("https://sepolia.arbiscan.io"),
            ChainNetwork::Optimism => Some("https://optimistic.etherscan.io"),
            ChainNetwork::OptimismSepolia => Some("https://sepolia-optimistic.etherscan.io"),
            ChainNetwork::Base => Some("https://basescan.org"),
            ChainNetwork::BaseSepolia => Some("https://sepolia.basescan.org"),
            ChainNetwork::Beechain => Some("https://scan.beechain.ai"),
            ChainNetwork::BeechainTestnet => Some("https://testnet-scan.beechain.ai"),
            ChainNetwork::Custom(_) => None,
        }
    }

    /// Check if this is a testnet
    #[allow(dead_code)]
    pub fn is_testnet(&self) -> bool {
        matches!(
            self,
            ChainNetwork::EthereumSepolia
                | ChainNetwork::BscTestnet
                | ChainNetwork::MonadTestnet
                | ChainNetwork::PolygonMumbai
                | ChainNetwork::ArbitrumSepolia
                | ChainNetwork::OptimismSepolia
                | ChainNetwork::BaseSepolia
                | ChainNetwork::BeechainTestnet
        )
    }

    /// Get network name
    #[allow(dead_code)]
    pub fn name(&self) -> &'static str {
        match self {
            ChainNetwork::Ethereum => "Ethereum Mainnet",
            ChainNetwork::EthereumSepolia => "Ethereum Sepolia",
            ChainNetwork::Bsc => "BSC Mainnet",
            ChainNetwork::BscTestnet => "BSC Testnet",
            ChainNetwork::Monad => "Monad",
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
            ChainNetwork::Custom(_) => "Custom Network",
        }
    }
}

impl std::str::FromStr for ChainNetwork {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ethereum" | "eth" | "mainnet" => Ok(ChainNetwork::Ethereum),
            "ethereum_sepolia" | "sepolia" => Ok(ChainNetwork::EthereumSepolia),
            "bsc" | "binance" => Ok(ChainNetwork::Bsc),
            "bsc_testnet" => Ok(ChainNetwork::BscTestnet),
            "monad" => Ok(ChainNetwork::Monad),
            "monad_testnet" => Ok(ChainNetwork::MonadTestnet),
            "polygon" | "matic" => Ok(ChainNetwork::Polygon),
            "polygon_mumbai" | "mumbai" => Ok(ChainNetwork::PolygonMumbai),
            "arbitrum" => Ok(ChainNetwork::Arbitrum),
            "arbitrum_sepolia" => Ok(ChainNetwork::ArbitrumSepolia),
            "optimism" | "op" => Ok(ChainNetwork::Optimism),
            "optimism_sepolia" => Ok(ChainNetwork::OptimismSepolia),
            "base" => Ok(ChainNetwork::Base),
            "base_sepolia" => Ok(ChainNetwork::BaseSepolia),
            "beechain" => Ok(ChainNetwork::Beechain),
            "beechain_testnet" | "beechain_test" => Ok(ChainNetwork::BeechainTestnet),
            _ => {
                // Try to parse as custom chain ID
                if let Ok(chain_id) = s.parse::<u64>() {
                    Ok(ChainNetwork::Custom(chain_id))
                } else {
                    Err(format!("Unknown chain network: {}", s))
                }
            }
        }
    }
}

/// Contract addresses for a specific chain
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ChainContractAddresses {
    /// AgentIdentity contract address
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    /// AgentRegistry contract address
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,
    /// AgentDAO contract address
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dao: Option<String>,
    /// SkillNFT contract address
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_nft: Option<String>,
    /// TreasuryManager contract address
    #[serde(skip_serializing_if = "Option::is_none")]
    pub treasury: Option<String>,
    /// Token contract address
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// A2ACommerce contract address
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commerce: Option<String>,
}

/// Configuration for a specific chain
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ChainConfig {
    /// Network type
    pub network: ChainNetwork,
    /// Custom RPC URL (optional, uses default if not set)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpc_url: Option<String>,
    /// Contract addresses
    #[serde(flatten)]
    pub contracts: ChainContractAddresses,
    /// Confirmation blocks required
    pub confirmation_blocks: u64,
    /// Gas price multiplier (e.g., 1.1 for 10% premium)
    pub gas_price_multiplier: f64,
    /// Enabled
    pub enabled: bool,
}

#[allow(dead_code)]
impl ChainConfig {
    /// Create new chain config with defaults
    pub fn new(network: ChainNetwork) -> Self {
        Self {
            network,
            rpc_url: None,
            contracts: ChainContractAddresses::default(),
            confirmation_blocks: 12,
            gas_price_multiplier: 1.1,
            enabled: true,
        }
    }

    /// Get effective RPC URL
    #[allow(dead_code)]
    pub fn effective_rpc_url(&self) -> Option<String> {
        self.rpc_url
            .clone()
            .or_else(|| self.network.default_rpc_url().map(String::from))
    }

    /// Get transaction explorer URL for a hash
    #[allow(dead_code)]
    pub fn tx_explorer_url(&self, tx_hash: &str) -> Option<String> {
        self.network
            .explorer_url()
            .map(|base| format!("{}/tx/{}", base, tx_hash))
    }

    /// Get address explorer URL
    #[allow(dead_code)]
    pub fn address_explorer_url(&self, address: &str) -> Option<String> {
        self.network
            .explorer_url()
            .map(|base| format!("{}/address/{}", base, address))
    }
}

/// Multi-chain configuration manager
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct MultiChainConfig {
    /// Chain configurations by network
    pub chains: HashMap<ChainNetwork, ChainConfig>,
    /// Default chain
    pub default_chain: Option<ChainNetwork>,
}

#[allow(dead_code)]
impl MultiChainConfig {
    /// Create empty multi-chain config
    pub fn new() -> Self {
        Self {
            chains: HashMap::new(),
            default_chain: None,
        }
    }

    /// Add a chain configuration
    pub fn add_chain(&mut self, config: ChainConfig) {
        let network = config.network;
        self.chains.insert(network, config);

        // Set as default if it's the first enabled chain
        if self.default_chain.is_none() {
            self.default_chain = Some(network);
        }
    }

    /// Get chain configuration
    pub fn get_chain(&self, network: &ChainNetwork) -> Option<&ChainConfig> {
        self.chains.get(network)
    }

    /// Get mutable chain configuration
    pub fn get_chain_mut(&mut self, network: &ChainNetwork) -> Option<&mut ChainConfig> {
        self.chains.get_mut(network)
    }

    /// Get default chain configuration
    pub fn get_default(&self) -> Option<&ChainConfig> {
        self.default_chain.as_ref().and_then(|n| self.chains.get(n))
    }

    /// Set default chain
    pub fn set_default(&mut self, network: ChainNetwork) {
        if self.chains.contains_key(&network) {
            self.default_chain = Some(network);
        }
    }

    /// Get all enabled chains
    pub fn enabled_chains(&self) -> Vec<&ChainConfig> {
        self.chains.values().filter(|c| c.enabled).collect()
    }

    /// Get chain by chain ID
    pub fn get_by_chain_id(&self, chain_id: u64) -> Option<&ChainConfig> {
        self.chains
            .values()
            .find(|c| c.network.chain_id() == chain_id)
    }

    /// Remove a chain
    pub fn remove_chain(&mut self, network: &ChainNetwork) {
        self.chains.remove(network);

        // Reset default if needed
        if self.default_chain == Some(*network) {
            self.default_chain = self.chains.keys().next().copied();
        }
    }

    /// Validate all configurations
    pub fn validate(&self) -> Vec<(ChainNetwork, String)> {
        let mut errors = Vec::new();

        for (network, config) in &self.chains {
            // Check RPC URL
            if config.effective_rpc_url().is_none() {
                errors.push((*network, "No RPC URL configured".to_string()));
            }

            // Check if at least one contract is configured
            let has_contract = config.contracts.identity.is_some()
                || config.contracts.dao.is_some()
                || config.contracts.registry.is_some();

            if !has_contract {
                errors.push((*network, "No contracts configured".to_string()));
            }
        }

        errors
    }
}

/// Predefined configurations for common networks
#[allow(dead_code)]
pub mod presets {
    use super::*;

    /// Ethereum mainnet preset
    pub fn ethereum() -> ChainConfig {
        ChainConfig::new(ChainNetwork::Ethereum)
    }

    /// Ethereum Sepolia preset
    #[allow(dead_code)]
    pub fn ethereum_sepolia() -> ChainConfig {
        ChainConfig::new(ChainNetwork::EthereumSepolia)
    }

    /// BSC mainnet preset
    #[allow(dead_code)]
    pub fn bsc() -> ChainConfig {
        ChainConfig::new(ChainNetwork::Bsc)
    }

    /// Monad testnet preset
    #[allow(dead_code)]
    pub fn monad_testnet() -> ChainConfig {
        let mut config = ChainConfig::new(ChainNetwork::MonadTestnet);
        config.confirmation_blocks = 1; // Faster finality on testnet
        config.gas_price_multiplier = 1.0; // No premium needed
        config
    }

    /// Beechain mainnet preset
    ///
    /// High-performance EVM-compatible L1 with parallel execution:
    /// - TPS: 10,000
    /// - Block time: 0.4s
    /// - Finality: ~0.8s (2 blocks)
    /// - Native token: BKC
    #[allow(dead_code)]
    pub fn beechain() -> ChainConfig {
        let mut config = ChainConfig::new(ChainNetwork::Beechain);
        // Fast finality: 0.8s / 0.4s block time = 2 blocks
        config.confirmation_blocks = 2;
        // Low gas price multiplier due to high throughput
        config.gas_price_multiplier = 1.05;
        config
    }

    /// Beechain testnet preset
    #[allow(dead_code)]
    pub fn beechain_testnet() -> ChainConfig {
        let mut config = ChainConfig::new(ChainNetwork::BeechainTestnet);
        config.confirmation_blocks = 2;
        config.gas_price_multiplier = 1.0;
        config
    }

    /// Development preset with Anvil/Hardhat defaults
    #[allow(dead_code)]
    pub fn localhost() -> ChainConfig {
        let mut config = ChainConfig::new(ChainNetwork::Custom(31337));
        config.rpc_url = Some("http://localhost:8545".to_string());
        config.confirmation_blocks = 1;
        config.gas_price_multiplier = 1.0;
        config
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_chain_network_chain_id() {
        assert_eq!(ChainNetwork::Ethereum.chain_id(), 1);
        assert_eq!(ChainNetwork::EthereumSepolia.chain_id(), 11155111);
        assert_eq!(ChainNetwork::MonadTestnet.chain_id(), 10143);
        assert_eq!(ChainNetwork::Beechain.chain_id(), 3188);
        assert_eq!(ChainNetwork::BeechainTestnet.chain_id(), 3189);
        assert_eq!(ChainNetwork::Custom(999).chain_id(), 999);
    }

    #[test]
    fn test_chain_network_from_str() {
        assert_eq!(
            ChainNetwork::from_str("ethereum").unwrap(),
            ChainNetwork::Ethereum
        );
        assert_eq!(
            ChainNetwork::from_str("monad_testnet").unwrap(),
            ChainNetwork::MonadTestnet
        );
        assert_eq!(
            ChainNetwork::from_str("beechain").unwrap(),
            ChainNetwork::Beechain
        );
        assert_eq!(
            ChainNetwork::from_str("beechain_testnet").unwrap(),
            ChainNetwork::BeechainTestnet
        );
        assert_eq!(
            ChainNetwork::from_str("999").unwrap(),
            ChainNetwork::Custom(999)
        );
    }

    #[test]
    fn test_multi_chain_config() {
        let mut config = MultiChainConfig::new();

        let eth = presets::ethereum();
        config.add_chain(eth);

        let monad = presets::monad_testnet();
        config.add_chain(monad);

        let beechain = presets::beechain();
        config.add_chain(beechain);

        assert_eq!(config.chains.len(), 3);
        assert!(config.get_default().is_some());

        let enabled = config.enabled_chains();
        assert_eq!(enabled.len(), 3);
    }

    #[test]
    fn test_chain_config_explorer_urls() {
        let config = presets::ethereum();

        assert!(config.tx_explorer_url("0xabc").is_some());
        assert!(config.address_explorer_url("0x123").is_some());

        let beechain_config = presets::beechain();
        assert!(beechain_config.tx_explorer_url("0xabc").is_some());
        assert!(beechain_config.address_explorer_url("0x123").is_some());
    }

    #[test]
    fn test_is_testnet() {
        assert!(!ChainNetwork::Ethereum.is_testnet());
        assert!(ChainNetwork::EthereumSepolia.is_testnet());
        assert!(ChainNetwork::MonadTestnet.is_testnet());
        assert!(!ChainNetwork::Beechain.is_testnet());
        assert!(ChainNetwork::BeechainTestnet.is_testnet());
    }

    #[test]
    fn test_beechain_config() {
        let config = presets::beechain();

        assert_eq!(config.network.chain_id(), 3188);
        assert_eq!(config.confirmation_blocks, 2); // Fast finality: 0.8s / 0.4s = 2 blocks
        assert_eq!(config.gas_price_multiplier, 1.05); // Low premium due to high TPS

        assert_eq!(
            config.effective_rpc_url(),
            Some("https://rpc.beechain.ai".to_string())
        );

        assert_eq!(
            config.tx_explorer_url("0xabc123"),
            Some("https://scan.beechain.ai/tx/0xabc123".to_string())
        );
    }
}
