//! Chain Transaction Signer
//!
//! Provides transaction signing using the chain wallet with alloy-signer.
//! Supports EIP-1559 (London) transactions.

use alloy_consensus::{SignableTransaction, TxEip1559};
use alloy_primitives::{Address, TxKind, U256};
use alloy_signer::Signer;
use tracing::{debug, info};

use crate::error::AppError;

/// EIP-1559 Transaction request for signing
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SignableEip1559Tx {
    pub chain_id: u64,
    pub nonce: u64,
    pub max_priority_fee_per_gas: U256,
    pub max_fee_per_gas: U256,
    pub gas_limit: u64,
    pub to: Address,
    pub value: U256,
    pub data: Vec<u8>,
    pub access_list: Vec<(Address, Vec<alloy_primitives::B256>)>,
}

#[allow(dead_code)]
impl SignableEip1559Tx {
    /// Create from gateway TransactionRequest
    pub fn from_request(
        req: &beebotos_chain::compat::TransactionRequest,
        max_priority_fee: U256,
    ) -> Self {
        let to_addr = alloy_primitives::Address::from_slice(req.to.as_slice());

        Self {
            chain_id: req.chain_id,
            nonce: req.nonce,
            max_priority_fee_per_gas: max_priority_fee,
            max_fee_per_gas: {
                let bytes: [u8; 32] = req.gas_price.to_be_bytes();
                U256::from_be_bytes(bytes)
            },
            gas_limit: req.gas_limit,
            to: to_addr,
            value: {
                let bytes: [u8; 32] = req.value.to_be_bytes();
                U256::from_be_bytes(bytes)
            },
            data: req.data.to_vec(),
            access_list: Vec::new(),
        }
    }
}

/// Sign a transaction using the wallet
///
/// # Arguments
/// * `wallet` - Chain wallet for signing
/// * `tx_request` - Transaction request
/// * `chain_id` - Chain ID
///
/// # Returns
/// Signed raw transaction bytes ready for broadcast
pub async fn sign_eip1559_transaction(
    wallet: &beebotos_chain::wallet::Wallet,
    tx_request: &beebotos_chain::compat::TransactionRequest,
) -> Result<Vec<u8>, AppError> {
    debug!("Signing EIP-1559 transaction");

    let signer = wallet.signer();

    // Convert U256 gas price to u128
    let gas_price_u128: u128 = tx_request.gas_price.to();

    // Build the EIP-1559 transaction
    let tx = TxEip1559 {
        chain_id: tx_request.chain_id,
        nonce: tx_request.nonce,
        max_priority_fee_per_gas: gas_price_u128 / 10,
        max_fee_per_gas: gas_price_u128,
        gas_limit: tx_request.gas_limit,
        to: TxKind::Call(alloy_primitives::Address::from_slice(
            tx_request.to.as_slice(),
        )),
        value: tx_request.value,
        input: alloy_primitives::Bytes::copy_from_slice(tx_request.data.as_ref()),
        access_list: alloy_eips::eip2930::AccessList::default(),
    };

    // Get the signature hash (transaction hash to sign)
    let signature_hash = tx.signature_hash();
    debug!(hash = %signature_hash, "Transaction signature hash");

    // Sign the hash
    let signature = signer
        .sign_message(signature_hash.as_slice())
        .await
        .map_err(|e| AppError::Chain(format!("Failed to sign transaction: {}", e)))?;

    info!(address = %wallet.address(), "Transaction signed successfully");

    // Build the signed transaction
    let signed_tx = tx.into_signed(signature);

    // Encode the signed transaction using RLP encoding
    let mut encoded = Vec::new();
    signed_tx.rlp_encode(&mut encoded);

    debug!(encoded_len = %encoded.len(), "Transaction encoded");

    Ok(encoded)
}

/// Legacy transaction signing (for chains that don't support EIP-1559)
#[allow(dead_code)]
pub async fn sign_legacy_transaction(
    wallet: &beebotos_chain::wallet::Wallet,
    tx_request: &beebotos_chain::compat::TransactionRequest,
) -> Result<Vec<u8>, AppError> {
    debug!("Signing legacy transaction");

    use alloy_consensus::TxLegacy;

    let signer = wallet.signer();

    // Convert U256 gas price to u128
    let gas_price_u128: u128 = tx_request.gas_price.to();

    let tx = TxLegacy {
        chain_id: Some(tx_request.chain_id),
        nonce: tx_request.nonce,
        gas_price: gas_price_u128,
        gas_limit: tx_request.gas_limit,
        to: TxKind::Call(alloy_primitives::Address::from_slice(
            tx_request.to.as_slice(),
        )),
        value: tx_request.value,
        input: alloy_primitives::Bytes::copy_from_slice(tx_request.data.as_ref()),
    };

    let signature_hash = tx.signature_hash();
    let signature = signer
        .sign_message(signature_hash.as_slice())
        .await
        .map_err(|e| AppError::Chain(format!("Failed to sign transaction: {}", e)))?;

    let signed_tx = tx.into_signed(signature);

    let mut encoded = Vec::new();
    signed_tx.rlp_encode(&mut encoded);

    Ok(encoded)
}

/// Sign a message using the wallet
#[allow(dead_code)]
pub async fn sign_message(
    wallet: &beebotos_chain::wallet::Wallet,
    message: &[u8],
) -> Result<Vec<u8>, AppError> {
    use alloy_primitives::B256;

    let signature = wallet
        .sign_message(message)
        .await
        .map_err(|e| AppError::Chain(format!("Failed to sign message: {}", e)))?;

    // Encode signature as bytes (r, s, v)
    // r and s are encoded as big-endian (32 bytes each) for Ethereum compatibility
    // v is encoded as 27 or 28 (standard Ethereum recovery ID)
    let mut encoded = Vec::with_capacity(65);
    // Convert U256 to B256 (big-endian) for proper encoding
    let r_bytes: B256 = signature.r().into();
    let s_bytes: B256 = signature.s().into();
    encoded.extend_from_slice(r_bytes.as_slice());
    encoded.extend_from_slice(s_bytes.as_slice());
    // Convert boolean v to standard Ethereum recovery ID (27 or 28)
    encoded.push(if signature.v() { 28 } else { 27 });

    Ok(encoded)
}

/// Verify a signature
#[allow(dead_code)]
pub fn verify_signature(address: &str, message: &[u8], signature: &[u8]) -> Result<bool, AppError> {
    use alloy_primitives::{eip191_hash_message, B256};
    use alloy_signer::Signature;

    if signature.len() != 65 {
        return Err(AppError::chain("Invalid signature length"));
    }

    let r = B256::from_slice(&signature[0..32]);
    let s = B256::from_slice(&signature[32..64]);
    let v = signature[64];

    let sig = Signature::from_scalars_and_parity(r, s, v > 27);

    let hash = eip191_hash_message(message);

    let recovered = sig
        .recover_address_from_prehash(&hash)
        .map_err(|e| AppError::chain(format!("Failed to recover address: {}", e)))?;

    let expected_addr = alloy_primitives::Address::parse_checksummed(address, None)
        .map_err(|e| AppError::chain(format!("Invalid address: {}", e)))?;

    Ok(recovered == expected_addr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sign_and_verify_message() {
        let wallet = beebotos_chain::wallet::Wallet::random();
        let message = b"Hello, BeeBotOS!";

        let signature = sign_message(&wallet, message).await.unwrap();
        assert_eq!(signature.len(), 65);

        let address = wallet.address();
        let verified = verify_signature(&address.to_string(), message, &signature).unwrap();
        assert!(verified);
    }

    #[test]
    fn test_verify_invalid_signature() {
        let message = b"test";
        let signature = vec![0u8; 65];

        let result = verify_signature(
            "0x0000000000000000000000000000000000000000",
            message,
            &signature,
        );

        // Should fail to recover or not match
        assert!(result.is_err() || !result.unwrap());
    }
}
