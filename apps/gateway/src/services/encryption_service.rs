//! Encryption Service for API Key storage
//!
//! Uses AES-256-GCM from beebotos_crypto crate.
//! Master key is read from BEE__SECURITY__MASTER_KEY environment variable.

use beebotos_crypto::encryption::aes::AES256GCMScheme;
use beebotos_crypto::encryption::{EncryptedData, EncryptionScheme};
use std::sync::Arc;

/// Service for encrypting and decrypting sensitive data
pub struct EncryptionService {
    scheme: Arc<AES256GCMScheme>,
}

impl EncryptionService {
    /// Create a new encryption service from environment
    pub fn from_env() -> Result<Self, String> {
        let master_key = std::env::var("BEE__SECURITY__MASTER_KEY")
            .map_err(|_| "BEE__SECURITY__MASTER_KEY environment variable not set".to_string())?;

        let key_bytes = Self::derive_key(&master_key);
        let scheme = AES256GCMScheme::new(&key_bytes)
            .map_err(|e| format!("Failed to initialize AES-256-GCM: {:?}", e))?;

        Ok(Self {
            scheme: Arc::new(scheme),
        })
    }

    /// Derive a 32-byte key from master key string using SHA-256
    fn derive_key(master_key: &str) -> Vec<u8> {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(master_key.as_bytes());
        hasher.finalize().to_vec()
    }

    /// Encrypt plaintext, return base64-encoded string
    pub fn encrypt(&self, plaintext: &str) -> Result<String, String> {
        let encrypted = self
            .scheme
            .encrypt(plaintext.as_bytes(), None)
            .map_err(|e| format!("Encryption failed: {:?}", e))?;

        let json = serde_json::to_vec(&encrypted)
            .map_err(|e| format!("Serialization failed: {}", e))?;
        Ok(base64::encode(json))
    }

    /// Decrypt base64-encoded ciphertext
    pub fn decrypt(&self, ciphertext: &str) -> Result<String, String> {
        let json = base64::decode(ciphertext)
            .map_err(|e| format!("Base64 decode failed: {}", e))?;
        let encrypted: EncryptedData = serde_json::from_slice(&json)
            .map_err(|e| format!("Deserialization failed: {}", e))?;

        let plaintext = self
            .scheme
            .decrypt(&encrypted, None)
            .map_err(|e| format!("Decryption failed: {:?}", e))?;

        String::from_utf8(plaintext).map_err(|e| format!("Invalid UTF-8: {}", e))
    }
}
