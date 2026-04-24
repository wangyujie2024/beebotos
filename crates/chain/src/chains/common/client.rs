//! Generic EVM Client
//!
//! Provides chain-agnostic client functionality and base implementation.

use std::time::Duration;

use alloy_network::TransactionBuilder;
use alloy_rpc_types::TransactionRequest;
use async_trait::async_trait;

use crate::chains::common::contract::{ContractCall, ContractDeploy, ContractInterface};
use crate::chains::common::provider::EvmProvider;
use crate::chains::common::{EvmBlock, EvmConfig, EvmError};
use crate::compat::{Address, U256};
use crate::ChainResult;

/// Generic EVM client trait
#[async_trait]
pub trait EvmClient: Send + Sync {
    /// Get provider reference
    fn provider(&self) -> &EvmProvider;

    /// Get chain configuration
    fn config(&self) -> &EvmConfig;

    /// Get chain ID
    fn chain_id(&self) -> u64 {
        self.config().chain_id
    }

    /// Get recommended confirmation blocks
    fn confirmation_blocks(&self) -> u64 {
        self.config().confirmation_blocks
    }

    /// Get current block number
    async fn get_block_number(&self) -> ChainResult<u64> {
        self.provider()
            .get_block_number()
            .await
            .map_err(|e| crate::ChainError::Provider(e.to_string()))
    }

    /// Get account balance
    async fn get_balance(&self, address: Address) -> ChainResult<U256> {
        self.provider()
            .get_balance(address)
            .await
            .map_err(|e| crate::ChainError::Provider(e.to_string()))
    }

    /// Get transaction count (nonce)
    async fn get_transaction_count(&self, address: Address) -> ChainResult<u64> {
        self.provider()
            .get_transaction_count(address)
            .await
            .map_err(|e| crate::ChainError::Provider(e.to_string()))
    }

    /// Get gas price
    async fn get_gas_price(&self) -> ChainResult<u128> {
        self.provider()
            .get_gas_price()
            .await
            .map_err(|e| crate::ChainError::Provider(e.to_string()))
    }

    /// Get block by number
    async fn get_block(&self, block_number: u64) -> ChainResult<Option<EvmBlock>> {
        let block = self
            .provider()
            .get_block(block_number)
            .await
            .map_err(|e| crate::ChainError::Provider(e.to_string()))?;

        Ok(block.map(|b| EvmBlock {
            number: b.header.number,
            hash: format!("{:?}", b.header.hash),
            parent_hash: format!("{:?}", b.header.parent_hash),
            timestamp: b.header.timestamp,
            gas_limit: b.header.gas_limit,
            gas_used: b.header.gas_used,
            transactions: b
                .transactions
                .hashes()
                .map(|h| format!("{:?}", h))
                .collect(),
            validator: format!("{:?}", b.header.beneficiary),
            base_fee_per_gas: b.header.base_fee_per_gas.map(|f| f as u64),
        }))
    }

    /// Send raw transaction
    async fn send_raw_transaction(&self, tx_bytes: &[u8]) -> ChainResult<String> {
        let pending = self
            .provider()
            .send_raw_transaction(tx_bytes)
            .await
            .map_err(|e| crate::ChainError::Provider(e.to_string()))?;

        Ok(format!("{:?}", pending))
    }

    /// Get transaction receipt
    async fn get_transaction_receipt(
        &self,
        tx_hash: &str,
    ) -> ChainResult<Option<alloy_rpc_types::TransactionReceipt>> {
        let hash: alloy_primitives::B256 = tx_hash
            .parse()
            .map_err(|_| crate::ChainError::Provider("Invalid tx hash".into()))?;

        self.provider()
            .get_transaction_receipt(hash)
            .await
            .map_err(|e| crate::ChainError::Provider(e.to_string()))
    }

    /// Wait for transaction confirmation
    async fn wait_for_confirmation(&self, tx_hash: &str, timeout_secs: u64) -> ChainResult<bool> {
        let hash: alloy_primitives::B256 = tx_hash
            .parse()
            .map_err(|_| crate::ChainError::Provider("Invalid tx hash".into()))?;

        self.provider()
            .wait_for_confirmation(
                hash,
                self.confirmation_blocks(),
                Duration::from_secs(timeout_secs),
            )
            .await
            .map_err(|e| crate::ChainError::Provider(e.to_string()))?;

        Ok(true)
    }

    /// Call a contract (read-only)
    async fn call(&self, call: &ContractCall) -> ChainResult<Vec<u8>> {
        let tx = TransactionRequest::default()
            .with_to(call.to().into())
            .with_input(call.data().to_vec());

        self.provider()
            .call(&tx)
            .await
            .map_err(|e| crate::ChainError::Provider(e.to_string()))
    }

    /// Estimate gas for a transaction
    async fn estimate_gas(&self, tx: &TransactionRequest) -> ChainResult<U256> {
        self.provider()
            .estimate_gas(tx)
            .await
            .map_err(|e| crate::ChainError::Provider(e.to_string()))
    }

    /// Get code at address
    async fn get_code(&self, address: Address) -> ChainResult<Vec<u8>> {
        self.provider()
            .get_code(address)
            .await
            .map_err(|e| crate::ChainError::Provider(e.to_string()))
    }
}

/// EVM client extension methods
#[async_trait]
pub trait EvmClientExt: EvmClient {
    /// Send a simple ETH transfer
    async fn send_transfer(
        &self,
        to: Address,
        value: U256,
        gas_price: u128,
    ) -> ChainResult<String> {
        let _tx = TransactionRequest::default()
            .with_to(to.into())
            .with_value(value)
            .with_gas_price(gas_price);

        // Note: In real implementation, this would sign and send
        // For now, we just return an error indicating it needs implementation
        Err(crate::ChainError::Provider(
            "Transfer requires signer implementation".into(),
        ))
    }

    /// Get current gas price with priority adjustment
    async fn get_gas_price_with_priority(
        &self,
        priority: super::TransactionPriority,
    ) -> ChainResult<u128> {
        let base = self.get_gas_price().await?;
        Ok((base as f64 * priority.multiplier()) as u128)
    }

    /// Check if address is a contract
    async fn is_contract(&self, address: Address) -> ChainResult<bool> {
        let code = self.get_code(address).await?;
        Ok(!code.is_empty())
    }

    /// Get native token balance formatted
    async fn get_balance_formatted(&self, address: Address) -> ChainResult<String> {
        let balance = self.get_balance(address).await?;
        Ok(super::format_native_token(balance, "ETH"))
    }
}

#[async_trait]
impl<T: EvmClient> EvmClientExt for T {}

/// Base EVM client implementation
#[derive(Debug)]
pub struct BaseEvmClient {
    provider: EvmProvider,
    config: EvmConfig,
}

impl BaseEvmClient {
    /// Create new base client
    pub async fn new(rpc_url: &str, chain_id: u64) -> ChainResult<Self> {
        let provider = EvmProvider::from_url(rpc_url, chain_id)
            .await
            .map_err(|e| crate::ChainError::Provider(e.to_string()))?;

        let config = EvmConfig::new(rpc_url, chain_id);

        Ok(Self { provider, config })
    }

    /// Create with custom config
    pub fn with_provider(provider: EvmProvider, config: EvmConfig) -> Self {
        Self { provider, config }
    }

    /// Get provider reference
    pub fn provider(&self) -> &EvmProvider {
        &self.provider
    }

    /// Get mutable provider reference
    pub fn provider_mut(&mut self) -> &mut EvmProvider {
        &mut self.provider
    }

    /// Get config reference
    pub fn config(&self) -> &EvmConfig {
        &self.config
    }

    /// Update configuration
    pub fn set_config(&mut self, config: EvmConfig) {
        self.config = config;
    }
}

#[async_trait]
impl EvmClient for BaseEvmClient {
    fn provider(&self) -> &EvmProvider {
        &self.provider
    }

    fn config(&self) -> &EvmConfig {
        &self.config
    }
}

/// Chain-specific client builder
pub struct EvmClientBuilder {
    rpc_url: String,
    chain_id: u64,
    ws_url: Option<String>,
    confirmation_blocks: u64,
    request_timeout: Duration,
}

impl EvmClientBuilder {
    /// Create new builder
    pub fn new(rpc_url: &str, chain_id: u64) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            chain_id,
            ws_url: None,
            confirmation_blocks: 12,
            request_timeout: Duration::from_secs(30),
        }
    }

    /// Get RPC URL
    pub fn rpc_url(&self) -> &str {
        &self.rpc_url
    }

    /// Set WebSocket URL
    pub fn with_ws(mut self, ws_url: &str) -> Self {
        self.ws_url = Some(ws_url.to_string());
        self
    }

    /// Set confirmation blocks
    pub fn with_confirmation_blocks(mut self, blocks: u64) -> Self {
        self.confirmation_blocks = blocks;
        self
    }

    /// Set request timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Build base client
    pub async fn build(self) -> ChainResult<BaseEvmClient> {
        let mut config = EvmConfig::new(&self.rpc_url, self.chain_id);
        config.confirmation_blocks = self.confirmation_blocks;

        if let Some(ws_url) = self.ws_url {
            config = config.with_ws(&ws_url);
        }

        let provider = EvmProvider::from_url(&self.rpc_url, self.chain_id)
            .await
            .map_err(|e| crate::ChainError::Provider(e.to_string()))?;

        Ok(BaseEvmClient::with_provider(provider, config))
    }
}

#[async_trait]
impl ContractInterface for BaseEvmClient {
    async fn call(&self, call: &ContractCall) -> Result<Vec<u8>, EvmError> {
        EvmClient::call(self, call)
            .await
            .map_err(|e| EvmError::ContractError(e.to_string()))
    }

    async fn send(&self, _call: &ContractCall) -> Result<String, EvmError> {
        // Requires signer - to be implemented
        Err(EvmError::TransactionError("Requires signer".into()))
    }

    async fn deploy(&self, _deploy: &ContractDeploy) -> Result<Address, EvmError> {
        // Requires signer - to be implemented
        Err(EvmError::TransactionError("Requires signer".into()))
    }

    async fn estimate_gas(&self, call: &ContractCall) -> Result<u64, EvmError> {
        let tx = TransactionRequest::default()
            .with_to(call.to().into())
            .with_input(call.data().to_vec());

        let gas = EvmClient::estimate_gas(self, &tx)
            .await
            .map_err(|_e| EvmError::GasEstimationFailed)?;

        Ok(gas.to())
    }
}

/// Generic chain client that can be specialized for different chains
///
/// This reduces code duplication across chain-specific clients by using
/// a generic configuration type that implements `ChainConfig`.
///
/// # Example
/// ```rust,ignore
/// let client = ChainClient::<EthereumConfig>::new("https://rpc.example.com", 1).await?;
/// ```
pub struct ChainClient<C: ChainConfig> {
    base: BaseEvmClient,
    chain_config: C,
}

/// Chain-specific configuration trait
///
/// Implement this trait for each supported chain to provide chain-specific
/// configuration and behavior.
pub trait ChainConfig: Send + Sync + Clone + 'static {
    /// Chain name (e.g., "ethereum", "bsc")
    fn chain_name(&self) -> &str;

    /// Chain ID
    fn chain_id(&self) -> u64;

    /// Recommended confirmation blocks for safe transactions
    fn confirmation_blocks(&self) -> u64;

    /// Whether EIP-1559 is supported
    fn supports_eip1559(&self) -> bool {
        false
    }

    /// Whether fast finality is available
    fn fast_finality(&self) -> bool {
        false
    }

    /// Average block time in seconds
    fn block_time_secs(&self) -> u64 {
        12
    }

    /// Native token symbol
    fn native_token_symbol(&self) -> &str {
        "ETH"
    }

    /// Native token decimals
    fn native_token_decimals(&self) -> u8 {
        18
    }
}

impl<C: ChainConfig> ChainClient<C> {
    /// Create new chain client
    pub async fn new(rpc_url: &str, chain_config: C) -> ChainResult<Self> {
        let base = BaseEvmClient::new(rpc_url, chain_config.chain_id()).await?;
        Ok(Self { base, chain_config })
    }

    /// Create from existing base client
    pub fn from_base(base: BaseEvmClient, chain_config: C) -> Self {
        Self { base, chain_config }
    }

    /// Get chain-specific configuration
    pub fn chain_config(&self) -> &C {
        &self.chain_config
    }

    /// Get base client reference
    pub fn base(&self) -> &BaseEvmClient {
        &self.base
    }

    /// Get recommended confirmation blocks
    pub fn confirmation_blocks(&self) -> u64 {
        self.chain_config.confirmation_blocks()
    }

    /// Get chain name
    pub fn chain_name(&self) -> &str {
        self.chain_config.chain_name()
    }

    /// Get native token symbol
    pub fn native_token_symbol(&self) -> &str {
        self.chain_config.native_token_symbol()
    }
}

impl<C: ChainConfig> std::ops::Deref for ChainClient<C> {
    type Target = BaseEvmClient;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl<C: ChainConfig> std::ops::DerefMut for ChainClient<C> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.base
    }
}

#[async_trait]
impl<C: ChainConfig> EvmClient for ChainClient<C> {
    fn provider(&self) -> &EvmProvider {
        self.base.provider()
    }

    fn config(&self) -> &EvmConfig {
        self.base.config()
    }

    fn confirmation_blocks(&self) -> u64 {
        self.chain_config.confirmation_blocks()
    }
}

/// Generic chain client builder
///
/// Provides a fluent API for building chain clients with custom configuration.
pub struct ChainClientBuilder<C: ChainConfig> {
    base_builder: EvmClientBuilder,
    chain_config: C,
}

impl<C: ChainConfig> ChainClientBuilder<C> {
    /// Create new builder with chain configuration
    pub fn new(rpc_url: &str, chain_config: C) -> Self {
        let base_builder = EvmClientBuilder::new(rpc_url, chain_config.chain_id())
            .with_confirmation_blocks(chain_config.confirmation_blocks());

        Self {
            base_builder,
            chain_config,
        }
    }

    /// Set WebSocket URL
    pub fn with_ws(mut self, ws_url: &str) -> Self {
        self.base_builder = self.base_builder.with_ws(ws_url);
        self
    }

    /// Set confirmation blocks (overrides chain default)
    pub fn with_confirmation_blocks(mut self, blocks: u64) -> Self {
        self.base_builder = self.base_builder.with_confirmation_blocks(blocks);
        self
    }

    /// Set request timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.base_builder = self.base_builder.with_timeout(timeout);
        self
    }

    /// Build the chain client
    pub async fn build(self) -> ChainResult<ChainClient<C>> {
        let base = self.base_builder.build().await?;
        Ok(ChainClient::from_base(base, self.chain_config))
    }
}

/// Predefined chain configurations
pub mod chain_configs {
    use super::ChainConfig;

    /// Ethereum mainnet configuration
    #[derive(Clone, Debug)]
    pub struct EthereumConfig {
        pub use_eip1559: bool,
    }

    impl Default for EthereumConfig {
        fn default() -> Self {
            Self { use_eip1559: true }
        }
    }

    impl ChainConfig for EthereumConfig {
        fn chain_name(&self) -> &str {
            "ethereum"
        }
        fn chain_id(&self) -> u64 {
            1
        }
        fn confirmation_blocks(&self) -> u64 {
            12
        }
        fn supports_eip1559(&self) -> bool {
            self.use_eip1559
        }
        fn block_time_secs(&self) -> u64 {
            12
        }
        fn native_token_symbol(&self) -> &str {
            "ETH"
        }
    }

    /// BSC mainnet configuration
    #[derive(Clone, Debug)]
    pub struct BscConfig {
        pub fast_finality: bool,
    }

    impl Default for BscConfig {
        fn default() -> Self {
            Self {
                fast_finality: true,
            }
        }
    }

    impl ChainConfig for BscConfig {
        fn chain_name(&self) -> &str {
            "bsc"
        }
        fn chain_id(&self) -> u64 {
            56
        }
        fn confirmation_blocks(&self) -> u64 {
            if self.fast_finality {
                5
            } else {
                15
            }
        }
        fn fast_finality(&self) -> bool {
            self.fast_finality
        }
        fn block_time_secs(&self) -> u64 {
            3
        }
        fn native_token_symbol(&self) -> &str {
            "BNB"
        }
    }

    /// Beechain configuration
    #[derive(Clone, Debug)]
    pub struct BeechainConfig;

    impl ChainConfig for BeechainConfig {
        fn chain_name(&self) -> &str {
            "beechain"
        }
        fn chain_id(&self) -> u64 {
            3188
        }
        fn confirmation_blocks(&self) -> u64 {
            2
        }
        fn fast_finality(&self) -> bool {
            true
        }
        fn block_time_secs(&self) -> u64 {
            1
        }
        fn native_token_symbol(&self) -> &str {
            "BKC"
        }
    }

    /// Monad configuration
    #[derive(Clone, Debug)]
    pub struct MonadConfig {
        pub parallel_execution: bool,
    }

    impl Default for MonadConfig {
        fn default() -> Self {
            Self {
                parallel_execution: true,
            }
        }
    }

    impl ChainConfig for MonadConfig {
        fn chain_name(&self) -> &str {
            "monad"
        }
        fn chain_id(&self) -> u64 {
            1_014_301
        }
        fn confirmation_blocks(&self) -> u64 {
            1
        }
        fn fast_finality(&self) -> bool {
            true
        }
        fn block_time_secs(&self) -> u64 {
            1
        }
        fn native_token_symbol(&self) -> &str {
            "MON"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::chain_configs::*;
    use super::*;

    #[test]
    fn test_client_builder() {
        let builder = EvmClientBuilder::new("http://localhost:8545", 1)
            .with_ws("ws://localhost:8546")
            .with_confirmation_blocks(15);

        assert_eq!(builder.chain_id, 1);
        assert_eq!(builder.confirmation_blocks, 15);
    }

    #[test]
    fn test_chain_configs() {
        let eth_config = EthereumConfig::default();
        assert_eq!(eth_config.chain_name(), "ethereum");
        assert_eq!(eth_config.chain_id(), 1);
        assert_eq!(eth_config.confirmation_blocks(), 12);
        assert!(eth_config.supports_eip1559());

        let bsc_config = BscConfig::default();
        assert_eq!(bsc_config.chain_name(), "bsc");
        assert_eq!(bsc_config.chain_id(), 56);
        assert_eq!(bsc_config.confirmation_blocks(), 5); // fast finality
        assert!(bsc_config.fast_finality());

        let beechain_config = BeechainConfig;
        assert_eq!(beechain_config.chain_name(), "beechain");
        assert_eq!(beechain_config.chain_id(), 3188);
        assert_eq!(beechain_config.confirmation_blocks(), 2);

        let monad_config = MonadConfig::default();
        assert_eq!(monad_config.chain_name(), "monad");
        assert_eq!(monad_config.chain_id(), 1_014_301);
        assert_eq!(monad_config.confirmation_blocks(), 1);
    }

    #[test]
    fn test_chain_client_builder() {
        let builder = ChainClientBuilder::new("http://localhost:8545", EthereumConfig::default());

        // Just verify it compiles and has correct configuration
        assert_eq!(builder.chain_config.chain_id(), 1);
    }

    #[tokio::test]
    async fn test_base_client() {
        // This would need a mock provider for proper testing
    }
}
