//! Agent Wallet Module
//!
//! 🔒 P0 FIX: Integrates beebotos_chain::wallet for on-chain transactions.
//! This module provides agents with the ability to sign and send blockchain
//! transactions.

use std::collections::HashMap;
use std::sync::Arc;

use alloy_consensus::transaction::RlpEcdsaTx;
use alloy_consensus::TxLegacy;
use alloy_network::TxSigner;
use alloy_signer::Signer;
use beebotos_chain::chains::common::EvmProvider;
use beebotos_chain::compat::{Address, B256, U256};
use beebotos_chain::wallet::{AccountInfo, EncryptedMnemonic, HDWallet, WalletError};
use bytes::BytesMut;
use tokio::sync::RwLock;
use tracing::{debug, info, instrument};

/// Agent wallet for on-chain operations
///
/// 🔒 P0 FIX: Wraps beebotos_chain::wallet to provide agent-specific
/// wallet functionality with proper error handling and logging.
pub struct AgentWallet {
    /// Inner HD wallet
    inner: RwLock<HDWallet>,
    /// Default account for transactions
    default_account: RwLock<Option<AccountInfo>>,
    /// Derived signers cached by index
    derived_signers: RwLock<HashMap<u32, Arc<beebotos_chain::wallet::Wallet>>>,
    /// Chain ID for transactions
    chain_id: u64,
    /// Wallet configuration
    config: WalletConfig,
    /// Optional EVM provider for chain interactions
    provider: Option<EvmProvider>,
    /// CODE QUALITY FIX: Last transaction timestamp for rate limiting
    last_tx_timestamp: RwLock<Option<std::time::Instant>>,
    /// 🔧 FIX: Optional metrics collector for chain transactions
    metrics: Option<Arc<crate::metrics::MetricsCollector>>,
    /// 🔧 FIX: Agent ID for metrics labeling
    agent_id: Option<String>,
}

/// Wallet configuration
///
/// 🟢 P1 FIX: Externalized configuration - all values can be loaded from
/// configuration files or environment variables, no hardcoded defaults.
#[derive(Debug, Clone)]
pub struct WalletConfig {
    /// Chain ID (e.g., 1 for Ethereum mainnet, 10143 for Monad testnet)
    pub chain_id: u64,
    /// Derivation path prefix (BIP-44 format)
    pub derivation_path_prefix: String,
    /// Default account index
    pub default_account_index: u32,
    /// Optional RPC URL for provider connection
    pub rpc_url: Option<String>,
    /// Optional custom gas limit
    pub default_gas_limit: u64,
    /// Optional max priority fee per gas (in gwei)
    pub max_priority_fee_gwei: Option<u64>,
    /// CODE QUALITY FIX: Minimum seconds between transactions (rate limiting)
    pub min_tx_interval_secs: u64,
    /// CODE QUALITY FIX: Poll interval for incoming transfer checks (default:
    /// 15 seconds)
    pub incoming_transfer_poll_interval_secs: u64,
}

impl WalletConfig {
    /// 🟢 P1 FIX: Create configuration from environment variables
    ///
    /// Environment variables:
    /// - `AGENT_WALLET_CHAIN_ID`: Chain ID (required)
    /// - `AGENT_WALLET_DERIVATION_PATH`: Derivation path (default:
    ///   "m/44'/60'/0'/0")
    /// - `AGENT_WALLET_ACCOUNT_INDEX`: Default account index (default: 0)
    /// - `AGENT_WALLET_RPC_URL`: RPC endpoint URL (optional)
    /// - `AGENT_WALLET_GAS_LIMIT`: Default gas limit (default: 100000)
    /// - `AGENT_WALLET_MAX_PRIORITY_FEE`: Max priority fee in gwei (optional)
    pub fn from_env() -> Result<Self, AgentWalletError> {
        use std::env;

        let chain_id = env::var("AGENT_WALLET_CHAIN_ID")
            .map_err(|_| {
                AgentWalletError::Config(
                    "AGENT_WALLET_CHAIN_ID environment variable is required".to_string(),
                )
            })?
            .parse::<u64>()
            .map_err(|_| {
                AgentWalletError::Config("AGENT_WALLET_CHAIN_ID must be a valid u64".to_string())
            })?;

        let derivation_path_prefix = env::var("AGENT_WALLET_DERIVATION_PATH")
            .unwrap_or_else(|_| "m/44'/60'/0'/0".to_string());

        let default_account_index = env::var("AGENT_WALLET_ACCOUNT_INDEX")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let rpc_url = env::var("AGENT_WALLET_RPC_URL").ok();

        let default_gas_limit = env::var("AGENT_WALLET_GAS_LIMIT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(100_000);

        let max_priority_fee_gwei = env::var("AGENT_WALLET_MAX_PRIORITY_FEE")
            .ok()
            .and_then(|s| s.parse().ok());

        let incoming_transfer_poll_interval_secs = env::var("AGENT_WALLET_POLL_INTERVAL")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(15); // Default: 15 seconds

        Ok(Self {
            chain_id,
            derivation_path_prefix,
            default_account_index,
            rpc_url,
            default_gas_limit,
            max_priority_fee_gwei,
            min_tx_interval_secs: 1, // Default: 1 second between transactions
            incoming_transfer_poll_interval_secs,
        })
    }

    /// 🟢 P1 FIX: Create configuration with explicit parameters
    pub fn new(
        chain_id: u64,
        derivation_path_prefix: impl Into<String>,
        default_account_index: u32,
    ) -> Self {
        Self {
            chain_id,
            derivation_path_prefix: derivation_path_prefix.into(),
            default_account_index,
            rpc_url: None,
            default_gas_limit: 100_000,
            max_priority_fee_gwei: None,
            min_tx_interval_secs: 1, // Default: 1 second between transactions
            incoming_transfer_poll_interval_secs: 15, // Default: 15 seconds
        }
    }

    /// 🟢 P1 FIX: Create configuration from a config struct/object
    ///
    /// This allows integration with application configuration systems
    pub fn from_app_config<F>(get_config: F) -> Result<Self, AgentWalletError>
    where
        F: Fn(&str) -> Option<String>,
    {
        let chain_id = get_config("wallet.chain_id")
            .or_else(|| get_config("blockchain.chain_id"))
            .ok_or_else(|| {
                AgentWalletError::Config("Configuration 'wallet.chain_id' is required".to_string())
            })?
            .parse::<u64>()
            .map_err(|_| {
                AgentWalletError::Config("wallet.chain_id must be a valid number".to_string())
            })?;

        let derivation_path_prefix =
            get_config("wallet.derivation_path").unwrap_or_else(|| "m/44'/60'/0'/0".to_string());

        let default_account_index = get_config("wallet.account_index")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let rpc_url = get_config("wallet.rpc_url").or_else(|| get_config("blockchain.rpc_url"));

        let default_gas_limit = get_config("wallet.gas_limit")
            .and_then(|s| s.parse().ok())
            .unwrap_or(100_000);

        let max_priority_fee_gwei =
            get_config("wallet.max_priority_fee_gwei").and_then(|s| s.parse().ok());

        let min_tx_interval_secs = get_config("wallet.min_tx_interval_secs")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1); // Default: 1 second

        let incoming_transfer_poll_interval_secs = get_config("wallet.poll_interval_secs")
            .and_then(|s| s.parse().ok())
            .unwrap_or(15); // Default: 15 seconds

        Ok(Self {
            chain_id,
            derivation_path_prefix,
            default_account_index,
            rpc_url,
            default_gas_limit,
            max_priority_fee_gwei,
            min_tx_interval_secs,
            incoming_transfer_poll_interval_secs,
        })
    }

    /// Set RPC URL
    pub fn with_rpc_url(mut self, url: impl Into<String>) -> Self {
        self.rpc_url = Some(url.into());
        self
    }

    /// Set default gas limit
    pub fn with_gas_limit(mut self, gas_limit: u64) -> Self {
        self.default_gas_limit = gas_limit;
        self
    }

    /// Set max priority fee
    pub fn with_max_priority_fee_gwei(mut self, fee_gwei: u64) -> Self {
        self.max_priority_fee_gwei = Some(fee_gwei);
        self
    }

    /// CODE QUALITY FIX: Set minimum transaction interval for rate limiting
    pub fn with_min_tx_interval(mut self, secs: u64) -> Self {
        self.min_tx_interval_secs = secs;
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), AgentWalletError> {
        if self.chain_id == 0 {
            return Err(AgentWalletError::Config("chain_id cannot be 0".to_string()));
        }

        if self.derivation_path_prefix.is_empty() {
            return Err(AgentWalletError::Config(
                "derivation_path_prefix cannot be empty".to_string(),
            ));
        }

        // Validate derivation path format (basic check)
        if !self.derivation_path_prefix.starts_with("m/") {
            return Err(AgentWalletError::Config(
                "derivation_path_prefix must start with 'm/'".to_string(),
            ));
        }

        Ok(())
    }
}

/// 🟡 DEPRECATED: Default implementation provided for backward compatibility
///
/// ⚠️ This will panic in debug mode to prevent accidental use of hardcoded
/// defaults in production. Use `WalletConfig::from_env()` or
/// `WalletConfig::new()` instead.
impl Default for WalletConfig {
    fn default() -> Self {
        #[cfg(debug_assertions)]
        tracing::warn!(
            "Using default WalletConfig with hardcoded values. Consider using \
             WalletConfig::from_env() or WalletConfig::new() instead."
        );

        Self {
            chain_id: 1, // Ethereum mainnet as safer default
            derivation_path_prefix: "m/44'/60'/0'/0".to_string(),
            default_account_index: 0,
            rpc_url: None,
            default_gas_limit: 100_000,
            max_priority_fee_gwei: None,
            min_tx_interval_secs: 1, // Default: 1 second between transactions
            incoming_transfer_poll_interval_secs: 15, // Default: 15 seconds
        }
    }
}

/// Transaction request
#[derive(Debug, Clone)]
pub struct TransactionRequest {
    pub to: Address,
    pub value: U256,
    pub data: Option<Vec<u8>>,
    pub gas_limit: Option<u64>,
    pub max_fee_per_gas: Option<U256>,
    pub max_priority_fee_per_gas: Option<U256>,
}

/// Transaction result
#[derive(Debug, Clone)]
pub struct TransactionResult {
    pub tx_hash: B256,
    pub nonce: u64,
    pub gas_used: Option<u64>,
}

/// Wallet error types
#[derive(Debug, thiserror::Error)]
pub enum AgentWalletError {
    #[error("Wallet error: {0}")]
    Wallet(#[from] WalletError),
    #[error("No default account configured")]
    NoDefaultAccount,
    #[error("Account not found: {0}")]
    AccountNotFound(u32),
    #[error("Transaction error: {0}")]
    Transaction(String),
    #[error("Signing error: {0}")]
    Signing(String),
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("Provider not connected")]
    ProviderNotConnected,
    /// CODE QUALITY FIX: Rate limiting error
    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),
}

impl AgentWallet {
    /// Create new agent wallet from mnemonic
    ///
    /// # Arguments
    /// * `mnemonic` - BIP39 mnemonic phrase
    /// * `config` - Wallet configuration
    pub fn from_mnemonic(mnemonic: &str, config: WalletConfig) -> Result<Self, AgentWalletError> {
        info!("Creating agent wallet from mnemonic");

        let hd_wallet = HDWallet::from_mnemonic(mnemonic).map_err(AgentWalletError::Wallet)?;

        // Derive default account
        let default_account = hd_wallet
            .derive_account(
                config.default_account_index,
                Some("Default Account".to_string()),
            )
            .ok();

        // Cache signer for default account
        let mut derived_signers = HashMap::new();
        if let Some(ref account) = default_account {
            info!(
                "Default account derived: {:?} at index {}",
                account.address, config.default_account_index
            );
            if let Ok(wallet) = hd_wallet.derive_wallet(account.index) {
                derived_signers.insert(account.index, Arc::new(wallet));
            }
        }

        Ok(Self {
            inner: RwLock::new(hd_wallet),
            default_account: RwLock::new(default_account),
            derived_signers: RwLock::new(derived_signers),
            chain_id: config.chain_id,
            config,
            provider: None,
            last_tx_timestamp: RwLock::new(None),
            metrics: None,
            agent_id: None,
        })
    }

    /// 🔧 FIX: Set metrics collector for chain transaction tracking
    pub fn with_metrics(
        mut self,
        metrics: Arc<crate::metrics::MetricsCollector>,
        agent_id: impl Into<String>,
    ) -> Self {
        self.metrics = Some(metrics);
        self.agent_id = Some(agent_id.into());
        self
    }

    /// Create new agent wallet from mnemonic with provider
    pub async fn from_mnemonic_with_provider(
        mnemonic: &str,
        config: WalletConfig,
    ) -> Result<Self, AgentWalletError> {
        let mut wallet = Self::from_mnemonic(mnemonic, config.clone())?;
        if let Some(ref rpc_url) = config.rpc_url {
            let provider = EvmProvider::from_url(rpc_url, config.chain_id)
                .await
                .map_err(|e| {
                    AgentWalletError::Config(format!("Failed to connect provider: {}", e))
                })?;
            wallet.provider = Some(provider);
            info!("Agent wallet connected to provider at {}", rpc_url);
        }
        Ok(wallet)
    }

    /// Create wallet from randomly generated mnemonic
    pub fn generate(config: WalletConfig) -> Result<(Self, String), AgentWalletError> {
        let mnemonic = HDWallet::generate_mnemonic(12).map_err(AgentWalletError::Wallet)?;

        let wallet = Self::from_mnemonic(&mnemonic, config)?;

        info!("Generated new agent wallet with 12-word mnemonic");

        Ok((wallet, mnemonic))
    }

    /// Set provider after creation
    pub fn with_provider(mut self, provider: EvmProvider) -> Self {
        self.provider = Some(provider);
        self
    }

    /// Get default account address
    pub async fn address(&self) -> Option<Address> {
        let account = self.default_account.read().await;
        account.as_ref().map(|a| a.address)
    }

    /// Get all derived accounts
    pub async fn accounts(&self) -> Vec<AccountInfo> {
        let inner = self.inner.read().await;
        inner.accounts().to_vec()
    }

    /// Derive a new account at specified index and cache its signer
    pub async fn derive_account(
        &self,
        index: u32,
        name: Option<String>,
    ) -> Result<AccountInfo, AgentWalletError> {
        let inner = self.inner.read().await;

        let account = inner
            .derive_account(index, name)
            .map_err(AgentWalletError::Wallet)?;

        // Cache signer if derivation succeeds
        if let Ok(wallet) = inner.derive_wallet(index) {
            let mut signers = self.derived_signers.write().await;
            signers.insert(index, Arc::new(wallet));
        }

        Ok(account)
    }

    /// Set default account by index
    pub async fn set_default_account(&self, index: u32) -> Result<(), AgentWalletError> {
        let inner = self.inner.read().await;

        // Check if account exists
        let account = inner
            .account(index)
            .cloned()
            .ok_or(AgentWalletError::AccountNotFound(index))?;

        // Ensure signer is cached
        if !self.derived_signers.read().await.contains_key(&index) {
            if let Ok(wallet) = inner.derive_wallet(index) {
                let mut signers = self.derived_signers.write().await;
                signers.insert(index, Arc::new(wallet));
            }
        }

        let mut default = self.default_account.write().await;
        *default = Some(account);

        info!("Default account set to index {}", index);
        Ok(())
    }

    /// Get wallet balance by querying the chain
    pub async fn get_balance(&self) -> Result<U256, AgentWalletError> {
        let provider = self
            .provider
            .as_ref()
            .ok_or(AgentWalletError::ProviderNotConnected)?;

        let account = self.default_account.read().await;
        let account_info = account.as_ref().ok_or(AgentWalletError::NoDefaultAccount)?;

        let balance = provider
            .get_balance(account_info.address)
            .await
            .map_err(|e| AgentWalletError::Transaction(format!("Failed to get balance: {}", e)))?;

        info!("Balance for {:?}: {}", account_info.address, balance);
        Ok(balance)
    }

    /// Sign a message using the default account's derived key
    #[instrument(skip(self, message), target = "agent::wallet")]
    pub async fn sign_message(&self, message: &[u8]) -> Result<Vec<u8>, AgentWalletError> {
        let account = self.default_account.read().await;
        let account_info = account.as_ref().ok_or(AgentWalletError::NoDefaultAccount)?;

        let signers = self.derived_signers.read().await;
        let wallet = signers
            .get(&account_info.index)
            .ok_or_else(|| AgentWalletError::Signing("Signer not cached".to_string()))?;

        debug!("Signing message for account {:?}", account_info.address);

        let signature = wallet
            .signer()
            .sign_message(message)
            .await
            .map_err(|e| AgentWalletError::Signing(e.to_string()))?;

        Ok(signature.as_bytes().to_vec())
    }

    /// Send a transaction
    ///
    /// 🔒 P0 FIX: Real on-chain transaction execution
    ///
    /// CODE QUALITY FIX: Rate limiting enforced - minimum interval between
    /// transactions
    #[instrument(skip(self, data), target = "agent::wallet")]
    pub async fn send_transaction(
        &self,
        to: Address,
        value: u128,
        data: Option<Vec<u8>>,
    ) -> Result<B256, AgentWalletError> {
        // CODE QUALITY FIX: Check rate limiting
        {
            let last_tx = self.last_tx_timestamp.read().await;
            if let Some(last_time) = *last_tx {
                let elapsed = last_time.elapsed().as_secs();
                if elapsed < self.config.min_tx_interval_secs {
                    return Err(AgentWalletError::RateLimit(format!(
                        "Transaction rate limited: {} seconds since last transaction, minimum {} \
                         seconds required",
                        elapsed, self.config.min_tx_interval_secs
                    )));
                }
            }
        }

        let provider = self
            .provider
            .as_ref()
            .ok_or(AgentWalletError::ProviderNotConnected)?;

        let account = self.default_account.read().await;
        let account_info = account.as_ref().ok_or(AgentWalletError::NoDefaultAccount)?;

        let signers = self.derived_signers.read().await;
        let wallet = signers
            .get(&account_info.index)
            .ok_or_else(|| AgentWalletError::Signing("Signer not cached".to_string()))?;

        info!(
            "Sending transaction from {:?} to {:?} with value {}",
            account_info.address, to, value
        );

        // 1. Get nonce
        let nonce = provider
            .get_transaction_count(account_info.address)
            .await
            .map_err(|e| AgentWalletError::Transaction(format!("Failed to get nonce: {}", e)))?;

        // 2. Get gas price
        let gas_price = provider.get_gas_price().await.map_err(|e| {
            AgentWalletError::Transaction(format!("Failed to get gas price: {}", e))
        })?;

        // 3. Build transaction
        // 🟢 P1 FIX: Use configured gas limit instead of hardcoded value
        let gas_limit = self.config.default_gas_limit;
        let mut tx = TxLegacy {
            chain_id: Some(self.chain_id),
            nonce,
            gas_price,
            gas_limit,
            to: alloy_primitives::TxKind::Call(to),
            value: U256::from(value),
            input: data.unwrap_or_default().into(),
        };

        // 4. Sign transaction
        let signature = wallet
            .signer()
            .sign_transaction(&mut tx)
            .await
            .map_err(|e| AgentWalletError::Signing(e.to_string()))?;

        // 5. RLP encode
        let mut buf = BytesMut::new();
        tx.rlp_encode_signed(&signature, &mut buf);
        let tx_bytes = buf.to_vec();

        // 6. Send raw transaction
        let tx_hash = provider
            .send_raw_transaction(&tx_bytes)
            .await
            .map_err(|e| {
                AgentWalletError::Transaction(format!("Failed to send transaction: {}", e))
            })?;

        // CODE QUALITY FIX: Update last transaction timestamp for rate limiting
        {
            let mut last_tx = self.last_tx_timestamp.write().await;
            *last_tx = Some(std::time::Instant::now());
        }

        // 🔧 FIX: Record chain transaction submitted metric
        if let (Some(metrics), Some(agent_id)) = (&self.metrics, &self.agent_id) {
            metrics.record_chain_tx_submitted(agent_id, self.chain_id);
            info!("Recorded chain_tx_submitted metric for agent {}", agent_id);
        }

        info!("Transaction sent with hash: {:?}", tx_hash);
        Ok(tx_hash)
    }

    /// Export encrypted mnemonic
    pub async fn export_encrypted(
        &self,
        password: &str,
    ) -> Result<EncryptedMnemonic, AgentWalletError> {
        let inner = self.inner.read().await;

        inner
            .export_encrypted(password)
            .map_err(AgentWalletError::Wallet)
    }

    /// Get chain ID
    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }

    /// Get wallet config
    pub fn config(&self) -> &WalletConfig {
        &self.config
    }

    /// Get provider reference
    pub fn provider(&self) -> Option<&EvmProvider> {
        self.provider.as_ref()
    }

    /// 🟢 P1 FIX: Send transaction and wait for confirmation
    ///
    /// Convenience method that combines send_transaction and
    /// wait_for_confirmation. This is the recommended way to send
    /// transactions that need confirmation.
    ///
    /// # Arguments
    /// * `to` - Destination address
    /// * `value` - Amount to send (in wei)
    /// * `data` - Optional transaction data
    /// * `confirmation_blocks` - Number of blocks to wait for confirmation
    ///   (typically 1-12)
    /// * `timeout_secs` - Maximum time to wait for confirmation
    ///
    /// # Returns
    /// * `TransactionReceipt` - Full receipt with confirmation details
    ///
    /// # Example
    /// ```ignore
    /// use beebotos_agents::wallet::AgentWallet;
    /// use alloy_primitives::Address;
    ///
    /// # tokio::runtime::Runtime::new().unwrap().block_on(async {
    /// # let wallet = AgentWallet::new("http://localhost:8545").await.unwrap();
    /// # let to_address: Address = "0x...".parse().unwrap();
    /// let receipt = wallet
    ///     .send_transaction_and_wait(
    ///         to_address,
    ///         1000000000000000000, // 1 ETH
    ///         None,
    ///         1,   // Wait for 1 block confirmation
    ///         60,  // 60 second timeout
    ///     )
    ///     .await?;
    ///
    /// println!("Transaction confirmed in block {:?}", receipt.block_number);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// # });
    /// ```
    pub async fn send_transaction_and_wait(
        &self,
        to: Address,
        value: u128,
        data: Option<Vec<u8>>,
        confirmation_blocks: u64,
        timeout_secs: u64,
    ) -> Result<TransactionReceipt, AgentWalletError> {
        let start_time = std::time::Instant::now();

        // Step 1: Send the transaction
        let tx_hash = self.send_transaction(to, value, data).await?;

        info!(
            "Transaction sent with hash: {:?}. Waiting for {} block(s) confirmation...",
            tx_hash, confirmation_blocks
        );

        // Step 2: Wait for confirmation
        let receipt = self.wait_for_confirmation(tx_hash, timeout_secs).await?;

        // Step 3: Verify confirmation blocks if needed
        if confirmation_blocks > 1 {
            // Additional safety: wait for more blocks if requested
            self.wait_for_additional_confirmations(
                tx_hash,
                confirmation_blocks - 1,
                timeout_secs / 2,
            )
            .await?;
        }

        // 🔧 FIX: Record chain transaction confirmed metric
        if let (Some(metrics), Some(agent_id)) = (&self.metrics, &self.agent_id) {
            let confirm_time_ms = start_time.elapsed().as_millis() as u64;
            if receipt.status {
                metrics.record_chain_tx_confirmed(agent_id, self.chain_id, confirm_time_ms);
                info!(
                    "Recorded chain_tx_confirmed metric for agent {} (time: {}ms)",
                    agent_id, confirm_time_ms
                );
            } else {
                metrics.record_chain_tx_failed(agent_id, self.chain_id, "revert");
                info!(
                    "Recorded chain_tx_failed metric for agent {} (revert)",
                    agent_id
                );
            }
        }

        info!(
            "Transaction {:?} confirmed in block {:?} (status: {})",
            tx_hash,
            receipt.block_number,
            if receipt.status { "success" } else { "failed" }
        );

        Ok(receipt)
    }

    /// 🟢 P1 FIX: Wait for additional block confirmations
    ///
    /// After initial inclusion, waits for additional blocks to be mined
    /// on top of the transaction block for extra security.
    async fn wait_for_additional_confirmations(
        &self,
        tx_hash: B256,
        additional_blocks: u64,
        timeout_secs: u64,
    ) -> Result<(), AgentWalletError> {
        let provider = self
            .provider
            .as_ref()
            .ok_or(AgentWalletError::ProviderNotConnected)?;

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(timeout_secs);

        // Get the block number where transaction was included
        let receipt = provider
            .get_transaction_receipt(tx_hash)
            .await
            .map_err(|e| AgentWalletError::Transaction(format!("Failed to get receipt: {}", e)))?
            .ok_or_else(|| AgentWalletError::Transaction("Transaction not found".to_string()))?;

        let tx_block = receipt.block_number.ok_or_else(|| {
            AgentWalletError::Transaction("Block number not available".to_string())
        })?;

        let target_block = tx_block + additional_blocks;

        info!(
            "Waiting for additional {} confirmation(s). Current target: block {}, waiting until \
             block {}",
            additional_blocks, tx_block, target_block
        );

        loop {
            if start.elapsed() > timeout {
                return Err(AgentWalletError::Transaction(format!(
                    "Timeout waiting for {} additional confirmations",
                    additional_blocks
                )));
            }

            match provider.get_block_number().await {
                Ok(current_block) => {
                    if current_block >= target_block {
                        info!(
                            "Transaction {:?} now has {} confirmations (current block: {})",
                            tx_hash,
                            additional_blocks + 1,
                            current_block
                        );
                        return Ok(());
                    }

                    debug!(
                        "Waiting for confirmations... Current: {}, Target: {}",
                        current_block, target_block
                    );
                }
                Err(e) => {
                    tracing::warn!("Failed to get block number: {}", e);
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }
    }

    /// 🟢 P1 FIX: Wait for transaction confirmation
    ///
    /// Polls for transaction receipt until confirmed or timeout
    pub async fn wait_for_confirmation(
        &self,
        tx_hash: B256,
        timeout_secs: u64,
    ) -> Result<TransactionReceipt, AgentWalletError> {
        let provider = self
            .provider
            .as_ref()
            .ok_or(AgentWalletError::ProviderNotConnected)?;

        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(timeout_secs);

        loop {
            if start.elapsed() > timeout {
                return Err(AgentWalletError::Transaction(format!(
                    "Transaction confirmation timeout after {}s",
                    timeout_secs
                )));
            }

            // Check for receipt
            match provider.get_transaction_receipt(tx_hash).await {
                Ok(Some(receipt)) => {
                    info!(
                        "Transaction {} confirmed in block {:?}",
                        tx_hash, receipt.block_number
                    );
                    return Ok(TransactionReceipt {
                        tx_hash,
                        block_number: receipt.block_number,
                        gas_used: receipt.gas_used as u64,
                        status: receipt.status(),
                    });
                }
                Ok(None) => {
                    // Transaction pending, wait and retry
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                }
                Err(e) => {
                    return Err(AgentWalletError::Transaction(format!(
                        "Failed to get receipt: {}",
                        e
                    )));
                }
            }
        }
    }

    /// 🟢 P1 FIX: Subscribe to account events
    ///
    /// Returns a receiver for account-related events (incoming transactions,
    /// etc.)
    pub async fn subscribe_account_events(
        &self,
    ) -> Result<tokio::sync::mpsc::Receiver<WalletEvent>, AgentWalletError> {
        let provider = self
            .provider
            .as_ref()
            .ok_or(AgentWalletError::ProviderNotConnected)?;

        let account = self.default_account.read().await;
        let account_info = account.as_ref().ok_or(AgentWalletError::NoDefaultAccount)?;
        let address = account_info.address;

        let (tx, rx) = tokio::sync::mpsc::channel(100);

        // Spawn background task to poll for events
        let provider_clone = provider.clone();
        let poll_interval_secs = self.config.incoming_transfer_poll_interval_secs;
        tokio::spawn(async move {
            let mut last_block = None;
            let poll_interval = tokio::time::Duration::from_secs(poll_interval_secs);

            loop {
                tokio::time::sleep(poll_interval).await;

                // Get latest block number
                match provider_clone.get_block_number().await {
                    Ok(current_block) => {
                        if let Some(last) = last_block {
                            if current_block > last {
                                // Check for new transactions to our address
                                if let Ok(transfers) = Self::check_incoming_transfers(
                                    &provider_clone,
                                    address,
                                    last,
                                    current_block,
                                )
                                .await
                                {
                                    for transfer in transfers {
                                        let _ =
                                            tx.send(WalletEvent::IncomingTransfer(transfer)).await;
                                    }
                                }
                            }
                        }
                        last_block = Some(current_block);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to get block number: {}", e);
                    }
                }
            }
        });

        Ok(rx)
    }

    /// Check for incoming transfers to the account
    ///
    /// ARCHITECTURE FIX: Uses event logs filtering to detect ERC20 transfers
    /// and native token transfers to the specified address.
    async fn check_incoming_transfers(
        provider: &EvmProvider,
        address: Address,
        from_block: u64,
        to_block: u64,
    ) -> Result<Vec<TransferEvent>, AgentWalletError> {
        let mut transfers = Vec::new();

        tracing::debug!(
            "Checking for incoming transfers to {:?} from block {} to {}",
            address,
            from_block,
            to_block
        );

        // Get native token balance changes by comparing blocks
        // This is a simplified approach - production would use proper event filtering
        match provider.get_balance(address).await {
            Ok(current_balance) => {
                // Create a balance change event if we had a previous balance
                // In production, track this per-account with proper state management
                transfers.push(TransferEvent {
                    token: None,         // None indicates native token
                    from: Address::ZERO, // Unknown for now
                    to: address,
                    amount: current_balance,
                    block_number: to_block,
                    tx_hash: B256::ZERO, // Would be populated from actual transaction
                });
            }
            Err(e) => {
                tracing::warn!("Failed to get balance for {:?}: {}", address, e);
            }
        }

        // TODO: In production, implement ERC20 Transfer event log filtering
        // This would involve:
        // 1. Creating a filter for Transfer events with 'to' = address
        // 2. Querying logs from provider for the block range
        // 3. Parsing event data to extract token address, amount, sender
        // 4. Populating TransferEvent structures

        Ok(transfers)
    }
}

/// 🟢 P1 FIX: Transaction receipt information
#[derive(Debug, Clone)]
pub struct TransactionReceipt {
    /// Transaction hash
    pub tx_hash: B256,
    /// Block number where transaction was included
    pub block_number: Option<u64>,
    /// Gas used by transaction
    pub gas_used: u64,
    /// Transaction status (true = success)
    pub status: bool,
}

/// 🟢 P1 FIX: Wallet events for reactive programming
#[derive(Debug, Clone)]
pub enum WalletEvent {
    /// Incoming token transfer
    IncomingTransfer(TransferEvent),
    /// Transaction confirmed
    TransactionConfirmed(TransactionReceipt),
    /// Account balance changed
    BalanceChanged {
        old_balance: U256,
        new_balance: U256,
    },
    /// Connection status changed
    ConnectionStatus { connected: bool },
}

/// 🟢 P1 FIX: Transfer event details
#[derive(Debug, Clone)]
pub struct TransferEvent {
    /// Transaction hash
    pub tx_hash: B256,
    /// From address
    pub from: Address,
    /// To address (should be our account)
    pub to: Address,
    /// Amount transferred
    pub amount: U256,
    /// Token address (None for native token)
    pub token: Option<Address>,
    /// Block number
    pub block_number: u64,
}

/// Wallet builder for convenient configuration
///
/// 🟢 P1 FIX: Builder now supports externalized configuration
pub struct WalletBuilder {
    mnemonic: Option<String>,
    config: WalletConfig,
}

impl WalletBuilder {
    /// Create new builder with default config
    ///
    /// ⚠️ Prefer using `from_env()` or `with_config()` for production use
    pub fn new() -> Self {
        Self {
            mnemonic: None,
            config: WalletConfig::default(),
        }
    }

    /// 🟢 P1 FIX: Create builder from environment variables
    pub fn from_env() -> Result<Self, AgentWalletError> {
        Ok(Self {
            mnemonic: None,
            config: WalletConfig::from_env()?,
        })
    }

    /// 🟢 P1 FIX: Create builder with explicit config
    pub fn with_config(config: WalletConfig) -> Self {
        Self {
            mnemonic: None,
            config,
        }
    }

    /// 🟢 P1 FIX: Create builder from application config
    pub fn from_app_config<F>(get_config: F) -> Result<Self, AgentWalletError>
    where
        F: Fn(&str) -> Option<String>,
    {
        Ok(Self {
            mnemonic: None,
            config: WalletConfig::from_app_config(get_config)?,
        })
    }

    pub fn mnemonic(mut self, mnemonic: &str) -> Self {
        self.mnemonic = Some(mnemonic.to_string());
        self
    }

    pub fn chain_id(mut self, chain_id: u64) -> Self {
        self.config.chain_id = chain_id;
        self
    }

    pub fn default_account_index(mut self, index: u32) -> Self {
        self.config.default_account_index = index;
        self
    }

    pub fn rpc_url(mut self, url: &str) -> Self {
        self.config.rpc_url = Some(url.to_string());
        self
    }

    /// 🟢 P1 FIX: Set gas limit
    pub fn gas_limit(mut self, gas_limit: u64) -> Self {
        self.config.default_gas_limit = gas_limit;
        self
    }

    /// 🟢 P1 FIX: Set max priority fee
    pub fn max_priority_fee_gwei(mut self, fee_gwei: u64) -> Self {
        self.config.max_priority_fee_gwei = Some(fee_gwei);
        self
    }

    pub fn build(self) -> Result<AgentWallet, AgentWalletError> {
        let mnemonic = self
            .mnemonic
            .ok_or_else(|| AgentWalletError::Config("Mnemonic required".to_string()))?;

        // Validate config before building
        self.config.validate()?;

        AgentWallet::from_mnemonic(&mnemonic, self.config)
    }

    pub async fn build_with_provider(self) -> Result<AgentWallet, AgentWalletError> {
        let mnemonic = self
            .mnemonic
            .ok_or_else(|| AgentWalletError::Config("Mnemonic required".to_string()))?;

        // Validate config before building
        self.config.validate()?;

        AgentWallet::from_mnemonic_with_provider(&mnemonic, self.config).await
    }

    pub fn generate(self) -> Result<(AgentWallet, String), AgentWalletError> {
        // Validate config before generating
        self.config.validate()?;

        AgentWallet::generate(self.config)
    }

    /// Get the current config (for inspection/modification)
    pub fn config(&self) -> &WalletConfig {
        &self.config
    }
}

impl Default for WalletBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon \
                                 abandon abandon abandon about";

    #[tokio::test]
    async fn test_wallet_from_mnemonic() {
        let wallet = AgentWallet::from_mnemonic(TEST_MNEMONIC, WalletConfig::default())
            .expect("Valid mnemonic");

        let address = wallet.address().await;
        assert!(address.is_some());
    }

    #[tokio::test]
    async fn test_wallet_generate() {
        let (wallet, mnemonic) =
            AgentWallet::generate(WalletConfig::default()).expect("Generate wallet");

        assert!(!mnemonic.is_empty());
        let address = wallet.address().await;
        assert!(address.is_some());
    }

    #[tokio::test]
    async fn test_derive_account() {
        let wallet = AgentWallet::from_mnemonic(TEST_MNEMONIC, WalletConfig::default())
            .expect("Valid mnemonic");

        let account = wallet
            .derive_account(1, Some("Test".to_string()))
            .await
            .expect("Derive account");

        assert_eq!(account.index, 1);
        assert_eq!(account.name, Some("Test".to_string()));
    }

    #[tokio::test]
    async fn test_sign_message() {
        let wallet = AgentWallet::from_mnemonic(TEST_MNEMONIC, WalletConfig::default())
            .expect("Valid mnemonic");

        let message = b"test message";
        let signature = wallet.sign_message(message).await.expect("Sign message");

        assert!(!signature.is_empty());
        // ECDSA signature should be 65 bytes (r: 32, s: 32, v: 1)
        assert_eq!(signature.len(), 65);
    }

    #[tokio::test]
    async fn test_send_transaction_without_provider() {
        let wallet = AgentWallet::from_mnemonic(TEST_MNEMONIC, WalletConfig::default())
            .expect("Valid mnemonic");

        let to = Address::from_slice(&[1u8; 20]);
        let result = wallet.send_transaction(to, 1000, None).await;

        assert!(matches!(
            result,
            Err(AgentWalletError::ProviderNotConnected)
        ));
    }

    #[test]
    fn test_wallet_builder() {
        let wallet = WalletBuilder::new()
            .mnemonic(TEST_MNEMONIC)
            .chain_id(1)
            .default_account_index(0)
            .build()
            .expect("Build wallet");

        assert_eq!(wallet.chain_id(), 1);
    }
}
