//! Chain Transaction Helper
//!
//! 🔒 P1 FIX: Extracted common transaction sending logic from ChainService
//! to reduce code duplication and improve maintainability.

use std::sync::Arc;

use beebotos_chain::compat::{
    Address, Bytes, ChainClientTrait, ContractCall, TransactionRequest, U256,
};
use beebotos_chain::wallet::Wallet as ChainWallet;
use tracing::{debug, info, instrument};

use crate::error::AppError;
use crate::services::chain_signer::sign_eip1559_transaction;

/// Transaction sending options
#[derive(Debug, Clone)]
pub struct TransactionOptions {
    /// Gas limit buffer (added to estimated gas)
    pub gas_buffer: u64,
    /// Max gas price
    pub max_gas_price: Option<U256>,
    /// Priority fee (EIP-1559)
    #[allow(dead_code)]
    pub priority_fee: Option<U256>,
    /// Retry count
    pub retry_count: u32,
    /// Retry delay
    pub retry_delay_ms: u64,
}

impl Default for TransactionOptions {
    fn default() -> Self {
        Self {
            gas_buffer: 50000,
            max_gas_price: None,
            priority_fee: None,
            retry_count: 3,
            retry_delay_ms: 1000,
        }
    }
}

/// Transaction helper for sending contract transactions
pub struct TransactionHelper;

impl TransactionHelper {
    /// Send a contract transaction with full lifecycle management
    ///
    /// 🔒 P1 FIX: This is the extracted common function that handles:
    /// - Gas estimation
    /// - Nonce retrieval
    /// - Gas price fetching
    /// - Transaction signing
    /// - Transaction sending
    /// - Error handling with retry
    #[instrument(
        skip(client, wallet, contract, call_data, options),
        fields(contract = %hex::encode(contract), call_data_len = call_data.len())
    )]
    pub async fn send_contract_transaction(
        client: &Arc<dyn ChainClientTrait>,
        wallet: &ChainWallet,
        contract: Address,
        call_data: Vec<u8>,
        value: U256,
        chain_id: u64,
        options: TransactionOptions,
    ) -> Result<beebotos_chain::compat::TxHash, AppError> {
        let sender_addr = wallet.address();

        info!("Preparing contract transaction");

        // Build contract call for estimation
        let call = ContractCall::new(contract.clone(), Bytes::from(call_data.clone()))
            .with_from(sender_addr);

        // Estimate gas
        let gas_estimate = Self::estimate_gas_with_retry(
            client,
            call,
            options.retry_count,
            options.retry_delay_ms,
        )
        .await?;

        let gas_limit = gas_estimate.to::<u64>() + options.gas_buffer;
        debug!("Gas estimate: {}, gas limit: {}", gas_estimate, gas_limit);

        // Get nonce
        let nonce = Self::get_nonce_with_retry(
            client,
            sender_addr,
            options.retry_count,
            options.retry_delay_ms,
        )
        .await?;
        debug!("Nonce: {}", nonce);

        // Get gas price
        let gas_price =
            Self::get_gas_price_with_retry(client, options.retry_count, options.retry_delay_ms)
                .await?;
        debug!("Gas price: {}", gas_price);

        // Check max gas price
        if let Some(max_price) = options.max_gas_price {
            if gas_price > max_price {
                return Err(AppError::Chain(format!(
                    "Gas price {} exceeds maximum {}",
                    gas_price, max_price
                )));
            }
        }

        // Build transaction
        let tx = TransactionRequest {
            from: sender_addr,
            to: contract,
            data: Bytes::from(call_data),
            value,
            gas_limit,
            gas_price,
            nonce,
            chain_id,
        };

        // Sign transaction using chain_signer
        let signed_tx = sign_eip1559_transaction(wallet, &tx).await?;

        // Send transaction with retry
        let tx_hash = Self::send_raw_transaction_with_retry(
            client,
            signed_tx.into(),
            options.retry_count,
            options.retry_delay_ms,
        )
        .await?;

        let tx_hash_hex = format!("0x{}", hex::encode(tx_hash.as_slice()));
        info!("Transaction sent successfully: {}", tx_hash_hex);

        Ok(tx_hash)
    }

    /// Estimate gas with retry logic
    async fn estimate_gas_with_retry(
        client: &Arc<dyn ChainClientTrait>,
        call: ContractCall,
        retry_count: u32,
        retry_delay_ms: u64,
    ) -> Result<U256, AppError> {
        let mut last_error = None;

        for attempt in 0..retry_count {
            match client.estimate_gas(call.clone()).await {
                Ok(gas) => return Ok(gas),
                Err(e) => {
                    debug!("Gas estimation attempt {} failed: {}", attempt + 1, e);
                    last_error = Some(e);
                    if attempt < retry_count - 1 {
                        tokio::time::sleep(tokio::time::Duration::from_millis(retry_delay_ms))
                            .await;
                    }
                }
            }
        }

        Err(AppError::Chain(format!(
            "Failed to estimate gas after {} attempts: {}",
            retry_count,
            last_error
                .map(|e| e.to_string())
                .unwrap_or_else(|| "Unknown error".to_string())
        )))
    }

    /// Get nonce with retry logic
    async fn get_nonce_with_retry(
        client: &Arc<dyn ChainClientTrait>,
        address: Address,
        retry_count: u32,
        retry_delay_ms: u64,
    ) -> Result<u64, AppError> {
        let mut last_error = None;

        for attempt in 0..retry_count {
            match client.get_transaction_count(address).await {
                Ok(nonce) => return Ok(nonce),
                Err(e) => {
                    debug!("Nonce fetch attempt {} failed: {}", attempt + 1, e);
                    last_error = Some(e);
                    if attempt < retry_count - 1 {
                        tokio::time::sleep(tokio::time::Duration::from_millis(retry_delay_ms))
                            .await;
                    }
                }
            }
        }

        Err(AppError::Chain(format!(
            "Failed to get nonce after {} attempts: {}",
            retry_count,
            last_error
                .map(|e| e.to_string())
                .unwrap_or_else(|| "Unknown error".to_string())
        )))
    }

    /// Get gas price with retry logic
    async fn get_gas_price_with_retry(
        client: &Arc<dyn ChainClientTrait>,
        retry_count: u32,
        retry_delay_ms: u64,
    ) -> Result<U256, AppError> {
        let mut last_error = None;

        for attempt in 0..retry_count {
            match client.get_gas_price().await {
                Ok(price) => return Ok(price),
                Err(e) => {
                    debug!("Gas price fetch attempt {} failed: {}", attempt + 1, e);
                    last_error = Some(e);
                    if attempt < retry_count - 1 {
                        tokio::time::sleep(tokio::time::Duration::from_millis(retry_delay_ms))
                            .await;
                    }
                }
            }
        }

        Err(AppError::Chain(format!(
            "Failed to get gas price after {} attempts: {}",
            retry_count,
            last_error
                .map(|e| e.to_string())
                .unwrap_or_else(|| "Unknown error".to_string())
        )))
    }

    /// Send raw transaction with retry logic
    async fn send_raw_transaction_with_retry(
        client: &Arc<dyn ChainClientTrait>,
        signed_tx: Bytes,
        retry_count: u32,
        retry_delay_ms: u64,
    ) -> Result<beebotos_chain::compat::TxHash, AppError> {
        let mut last_error = None;

        for attempt in 0..retry_count {
            match client.send_raw_transaction(signed_tx.clone()).await {
                Ok(tx_hash) => return Ok(tx_hash),
                Err(e) => {
                    debug!("Transaction send attempt {} failed: {}", attempt + 1, e);
                    last_error = Some(e);
                    if attempt < retry_count - 1 {
                        tokio::time::sleep(tokio::time::Duration::from_millis(retry_delay_ms))
                            .await;
                    }
                }
            }
        }

        Err(AppError::Chain(format!(
            "Failed to send transaction after {} attempts: {}",
            retry_count,
            last_error
                .map(|e| e.to_string())
                .unwrap_or_else(|| "Unknown error".to_string())
        )))
    }
}

/// Extension trait for ChainClientTrait to add convenient methods
#[allow(dead_code)]
#[async_trait::async_trait]
pub trait ChainClientExt {
    /// Send a contract transaction with automatic gas estimation and signing
    async fn send_contract_tx(
        &self,
        wallet: &ChainWallet,
        contract: Address,
        call_data: Vec<u8>,
        value: U256,
        chain_id: u64,
    ) -> Result<beebotos_chain::compat::TxHash, AppError>;
}

#[async_trait::async_trait]
impl ChainClientExt for Arc<dyn ChainClientTrait> {
    async fn send_contract_tx(
        &self,
        wallet: &ChainWallet,
        contract: Address,
        call_data: Vec<u8>,
        value: U256,
        chain_id: u64,
    ) -> Result<beebotos_chain::compat::TxHash, AppError> {
        TransactionHelper::send_contract_transaction(
            self,
            wallet,
            contract,
            call_data,
            value,
            chain_id,
            TransactionOptions::default(),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_options_default() {
        let opts = TransactionOptions::default();
        assert_eq!(opts.gas_buffer, 50000);
        assert_eq!(opts.retry_count, 3);
        assert_eq!(opts.retry_delay_ms, 1000);
    }
}
