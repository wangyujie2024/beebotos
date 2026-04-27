//! Wallet Service
//!
//! Manages blockchain wallets and basic chain interactions.
//! Separated from ChainService for better modularity.

use std::sync::Arc;

use beebotos_chain::compat::{Address, Bytes, ChainClientTrait, TxHash, U256};
use beebotos_chain::wallet::Wallet as ChainWallet;
use tracing::{info, warn};

use crate::config::BlockchainConfig;
use crate::error::AppError;

/// Wallet service configuration
#[derive(Debug, Clone)]
pub struct WalletServiceConfig {
    /// RPC URL for the blockchain
    pub rpc_url: String,
    /// Chain ID
    pub chain_id: u64,
    /// Wallet mnemonic for signing transactions
    pub wallet_mnemonic: Option<String>,
}

impl From<&BlockchainConfig> for WalletServiceConfig {
    fn from(config: &BlockchainConfig) -> Self {
        Self {
            rpc_url: config.rpc_url.clone().unwrap_or_default(),
            chain_id: config.chain_id,
            wallet_mnemonic: config.agent_wallet_mnemonic.clone(),
        }
    }
}

/// Wallet information
#[derive(Debug, Clone)]
pub struct WalletInfo {
    /// Wallet address
    pub address: Address,
    /// Chain ID
    pub chain_id: u64,
    /// Balance (in wei)
    pub balance: U256,
    /// Nonce
    pub nonce: u64,
}

/// Transaction request
#[derive(Debug, Clone)]
pub struct TransactionRequest {
    /// To address
    pub to: Address,
    /// Transaction data
    pub data: Bytes,
    /// Value to send
    pub value: U256,
    /// Gas limit (optional)
    pub gas_limit: Option<u64>,
}

/// Transaction result
#[derive(Debug, Clone)]
pub struct TransactionResult {
    /// Transaction hash
    pub tx_hash: TxHash,
    /// Gas used (if available)
    pub gas_used: Option<u64>,
    /// Block number (if confirmed)
    pub block_number: Option<u64>,
    /// Status (true = success)
    pub success: bool,
}

/// Wallet Service
pub struct WalletService {
    /// Configuration
    config: WalletServiceConfig,
    /// Chain client
    client: Option<Arc<dyn ChainClientTrait>>,
    /// Wallet for signing
    wallet: Option<Arc<ChainWallet>>,
}

impl WalletService {
    /// Create new wallet service
    pub async fn new(config: WalletServiceConfig) -> anyhow::Result<Self> {
        info!(chain_id = config.chain_id, "Initializing WalletService");

        // Initialize wallet if mnemonic provided
        let wallet = if let Some(ref mnemonic) = config.wallet_mnemonic {
            match initialize_wallet(mnemonic, config.chain_id).await {
                Ok(w) => {
                    info!(address = %w.address(), "Wallet initialized");
                    Some(Arc::new(w))
                }
                Err(e) => {
                    warn!("Failed to initialize wallet: {}", e);
                    None
                }
            }
        } else {
            warn!("No wallet mnemonic configured, wallet operations will be unavailable");
            None
        };

        // Initialize chain client
        let client = if !config.rpc_url.is_empty() {
            match beebotos_chain::compat::create_chain_client(&config.rpc_url).await {
                Ok(c) => {
                    info!("Chain client initialized");
                    Some(c)
                }
                Err(e) => {
                    warn!("Failed to initialize chain client: {}", e);
                    None
                }
            }
        } else {
            None
        };

        info!(
            has_wallet = wallet.is_some(),
            has_client = client.is_some(),
            "WalletService initialized"
        );

        Ok(Self {
            config,
            client,
            wallet,
        })
    }

    /// Check if wallet is available
    pub fn has_wallet(&self) -> bool {
        self.wallet.is_some()
    }

    /// Check if chain client is available
    pub fn has_client(&self) -> bool {
        self.client.is_some()
    }

    /// Get wallet address
    pub fn get_address(&self) -> Option<Address> {
        self.wallet.as_ref().map(|w| w.address())
    }

    /// Get wallet info including balance
    pub async fn get_wallet_info(&self) -> Result<WalletInfo, AppError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Chain client not initialized".into()))?;

        let wallet = self
            .wallet
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Wallet not initialized".into()))?;

        let address = wallet.address();

        // Get balance and nonce from chain
        let (balance, nonce) = tokio::try_join!(
            client.get_balance(address),
            client.get_transaction_count(address)
        )
        .map_err(|e| AppError::Chain(format!("Failed to fetch wallet info: {}", e)))?;

        Ok(WalletInfo {
            address,
            chain_id: self.config.chain_id,
            balance,
            nonce,
        })
    }

    /// Get balance for any address
    pub async fn get_balance(&self, address: Address) -> Result<U256, AppError> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Chain client not initialized".into()))?;

        client
            .get_balance(address)
            .await
            .map_err(|e| AppError::Chain(format!("Failed to get balance: {}", e)))
    }

    /// Send transaction
    pub async fn send_transaction(&self, _request: TransactionRequest) -> Result<TxHash, AppError> {
        let _client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Chain client not initialized".into()))?;

        let _wallet = self
            .wallet
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Wallet not initialized for signing".into()))?;

        // TODO: Implement proper transaction sending
        // For now, return a placeholder tx hash
        Ok(TxHash::default())
    }

    /// Send contract transaction
    pub async fn send_contract_transaction(
        &self,
        contract: Address,
        data: Bytes,
        value: U256,
    ) -> Result<TxHash, AppError> {
        self.send_transaction(TransactionRequest {
            to: contract,
            data,
            value,
            gas_limit: None,
        })
        .await
    }

    /// Transfer native tokens
    pub async fn transfer(&self, to: Address, amount: U256) -> Result<TxHash, AppError> {
        self.send_transaction(TransactionRequest {
            to,
            data: Bytes::new(),
            value: amount,
            gas_limit: None,
        })
        .await
    }

    /// Estimate gas for transaction
    pub async fn estimate_gas(&self, _request: &TransactionRequest) -> Result<u64, AppError> {
        // TODO: Implement gas estimation
        Ok(21000) // Default gas for simple transfer
    }

    /// Wait for transaction receipt
    pub async fn wait_for_receipt(
        &self,
        tx_hash: TxHash,
        _timeout_secs: u64,
    ) -> Result<TransactionResult, AppError> {
        // TODO: Implement proper receipt waiting
        // For now, return a placeholder result
        Ok(TransactionResult {
            tx_hash,
            gas_used: None,
            block_number: None,
            success: true,
        })
    }
}

/// Initialize wallet from mnemonic
async fn initialize_wallet(_mnemonic: &str, _chain_id: u64) -> anyhow::Result<ChainWallet> {
    // TODO: Implement proper wallet initialization
    // For now, create a random wallet
    Ok(ChainWallet::random())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wallet_service_config_from_blockchain_config() {
        let blockchain_config = BlockchainConfig {
            enabled: true,
            rpc_url: Some("https://rpc.example.com".to_string()),
            chain_id: 1,
            agent_wallet_mnemonic: Some("test mnemonic".to_string()),
            ..Default::default()
        };

        let wallet_config = WalletServiceConfig::from(&blockchain_config);

        assert_eq!(wallet_config.rpc_url, "https://rpc.example.com");
        assert_eq!(wallet_config.chain_id, 1);
        assert_eq!(
            wallet_config.wallet_mnemonic,
            Some("test mnemonic".to_string())
        );
    }
}
