//! Secure storage for sensitive configuration
//!
//! Uses platform-native keyring/keychain when available,
//! falls back to file-based AES-256-GCM encryption.

#![allow(dead_code)]

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Key names for secure storage
pub const KEY_API_KEY: &str = "beebotos_api_key";
pub const KEY_PRIVATE_KEY: &str = "beebotos_private_key";

/// Secure storage backend
pub struct SecureStorage {
    /// Master key for file-based encryption (derived from machine-specific
    /// data)
    master_key: Vec<u8>,
}

impl SecureStorage {
    /// Create new secure storage instance
    pub fn new() -> Result<Self> {
        let master_key = Self::derive_master_key()?;
        Ok(Self { master_key })
    }

    /// Derive a master key from machine-specific information
    /// This ensures secrets can only be decrypted on the same machine
    fn derive_master_key() -> Result<Vec<u8>> {
        use hkdf::Hkdf;
        use sha2::Sha256;

        // Collect machine-specific data
        let machine_id = Self::get_machine_id()?;
        let username = whoami::username();

        // Create a salt from machine-specific data
        let salt = format!("{}-{}", machine_id, username);

        // Derive a 256-bit key using HKDF-SHA256
        let hkdf = Hkdf::<Sha256>::new(Some(salt.as_bytes()), machine_id.as_bytes());
        let mut key = [0u8; 32];
        hkdf.expand(b"beebotos-secure-storage-v1", &mut key)
            .map_err(|e| anyhow::anyhow!("Key derivation failed: {:?}", e))?;

        Ok(key.to_vec())
    }

    /// Get a machine-specific identifier
    fn get_machine_id() -> Result<String> {
        // Try to get a stable machine identifier
        #[cfg(target_os = "linux")]
        {
            // Use machine-id on Linux
            if let Ok(id) = std::fs::read_to_string("/etc/machine-id") {
                return Ok(id.trim().to_string());
            }
            if let Ok(id) = std::fs::read_to_string("/var/lib/dbus/machine-id") {
                return Ok(id.trim().to_string());
            }
        }

        #[cfg(target_os = "macos")]
        {
            // Use IOPlatformUUID on macOS
            if let Ok(output) = std::process::Command::new("ioreg")
                .args(["-rd1", "-c", "IOPlatformExpertDevice"])
                .output()
            {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if line.contains("IOPlatformUUID") {
                        if let Some(uuid) = line.split('"').nth(3) {
                            return Ok(uuid.to_string());
                        }
                    }
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            // Use MachineGuid on Windows
            use std::process::Command;
            if let Ok(output) = Command::new("wmic")
                .args(["csproduct", "get", "uuid", "/value"])
                .output()
            {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if line.starts_with("UUID=") {
                        return Ok(line[5..].trim().to_string());
                    }
                }
            }
        }

        // Fallback: use home directory path hash
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(home.to_string_lossy().as_bytes());
        let result = hasher.finalize();
        Ok(hex::encode(&result[..16]))
    }

    /// Store a secret value
    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        // Try to use keyring first
        #[cfg(feature = "keyring")]
        {
            match keyring::Entry::new("beebotos", key) {
                Ok(entry) => {
                    if entry.set_password(value).is_ok() {
                        return Ok(());
                    }
                }
                Err(e) => {
                    log::warn!("Failed to create keyring entry: {}", e);
                }
            }
        }

        // Fallback to file-based storage with AES-256-GCM encryption
        self.set_file_based(key, value)
    }

    /// Retrieve a secret value
    pub fn get(&self, key: &str) -> Result<Option<String>> {
        // Try keyring first
        #[cfg(feature = "keyring")]
        {
            match keyring::Entry::new("beebotos", key) {
                Ok(entry) => match entry.get_password() {
                    Ok(value) => return Ok(Some(value)),
                    Err(keyring::Error::NoEntry) => return Ok(None),
                    Err(e) => {
                        log::warn!("Failed to retrieve from keyring: {}", e);
                    }
                },
                Err(e) => {
                    log::warn!("Failed to create keyring entry: {}", e);
                }
            }
        }

        // Fallback to file-based storage
        self.get_file_based(key)
    }

    /// Delete a secret
    pub fn delete(&self, key: &str) -> Result<()> {
        #[cfg(feature = "keyring")]
        {
            match keyring::Entry::new("beebotos", key) {
                Ok(entry) => {
                    if entry.delete_password().is_ok() {
                        return Ok(());
                    }
                }
                Err(e) => {
                    log::warn!("Failed to delete from keyring: {}", e);
                }
            }
        }

        self.delete_file_based(key)
    }

    /// File-based storage with AES-256-GCM encryption
    fn set_file_based(&self, key: &str, value: &str) -> Result<()> {
        let storage_path = self.storage_path()?;
        std::fs::create_dir_all(storage_path.parent().unwrap())?;

        let mut storage = self.load_storage()?;
        let encrypted = self.encrypt(value)?;
        storage.secrets.insert(key.to_string(), encrypted);

        let content = serde_json::to_string(&storage)?;
        std::fs::write(&storage_path, content)?;

        // Set restrictive permissions (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&storage_path, permissions)?;
        }

        Ok(())
    }

    fn get_file_based(&self, key: &str) -> Result<Option<String>> {
        let storage = self.load_storage()?;

        match storage.secrets.get(key) {
            Some(encrypted) => {
                let decrypted = self.decrypt(encrypted)?;
                Ok(Some(decrypted))
            }
            None => Ok(None),
        }
    }

    fn delete_file_based(&self, key: &str) -> Result<()> {
        let storage_path = self.storage_path()?;
        let mut storage = self.load_storage()?;
        storage.secrets.remove(key);

        let content = serde_json::to_string(&storage)?;
        std::fs::write(&storage_path, content)?;
        Ok(())
    }

    /// Encrypt data using AES-256-GCM
    fn encrypt(&self, plaintext: &str) -> Result<EncryptedData> {
        use aes_gcm::aead::{Aead, KeyInit};
        use aes_gcm::{Aes256Gcm, Nonce};
        use rand::RngCore;

        let cipher = Aes256Gcm::new_from_slice(&self.master_key)
            .map_err(|e| anyhow::anyhow!("Failed to create cipher: {:?}", e))?;

        // Generate a random 96-bit nonce
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt the plaintext
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| anyhow::anyhow!("Encryption failed: {:?}", e))?;

        Ok(EncryptedData {
            nonce: b64::encode(&nonce_bytes),
            ciphertext: b64::encode(&ciphertext),
            version: 1,
        })
    }

    /// Decrypt data using AES-256-GCM
    fn decrypt(&self, encrypted: &EncryptedData) -> Result<String> {
        use aes_gcm::aead::{Aead, KeyInit};
        use aes_gcm::{Aes256Gcm, Nonce};

        if encrypted.version != 1 {
            return Err(anyhow::anyhow!(
                "Unsupported encryption version: {}",
                encrypted.version
            ));
        }

        let cipher = Aes256Gcm::new_from_slice(&self.master_key)
            .map_err(|e| anyhow::anyhow!("Failed to create cipher: {:?}", e))?;

        let nonce_bytes = b64::decode(&encrypted.nonce)?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = b64::decode(&encrypted.ciphertext)?;

        let plaintext = cipher.decrypt(nonce, ciphertext.as_ref()).map_err(|e| {
            anyhow::anyhow!(
                "Decryption failed (wrong machine or tampered data): {:?}",
                e
            )
        })?;

        String::from_utf8(plaintext)
            .map_err(|e| anyhow::anyhow!("Invalid UTF-8 in decrypted data: {}", e))
    }

    fn storage_path(&self) -> Result<PathBuf> {
        let data_dir = dirs::data_dir().context("Could not find data directory")?;
        Ok(data_dir.join("beebotos").join("secrets-v2.json"))
    }

    fn load_storage(&self) -> Result<StorageData> {
        let path = self.storage_path()?;
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(StorageData::default())
        }
    }

    /// Migrate from old XOR-encrypted storage
    pub fn migrate_from_v1(&self) -> Result<usize> {
        let old_path = self.old_storage_path()?;
        if !old_path.exists() {
            return Ok(0);
        }

        // Load old storage
        let content = std::fs::read_to_string(&old_path)?;
        let old_storage: OldStorageData = serde_json::from_str(&content)?;

        let mut migrated = 0;
        for (key, encrypted) in old_storage.secrets {
            // Decrypt using old XOR method
            if let Ok(decrypted) = self.decrypt_xor(&encrypted, &key) {
                self.set(&key, &decrypted)?;
                migrated += 1;
            }
        }

        // Rename old file as backup
        let backup_path = old_path.with_extension("json.backup");
        std::fs::rename(&old_path, &backup_path)?;

        Ok(migrated)
    }

    fn old_storage_path(&self) -> Result<PathBuf> {
        let data_dir = dirs::data_dir().context("Could not find data directory")?;
        Ok(data_dir.join("beebotos").join("secrets.json"))
    }

    fn decrypt_xor(&self, encrypted: &str, key: &str) -> Result<String> {
        let key_bytes = key.as_bytes();
        let encrypted_bytes = b64::decode(encrypted)?;
        let decrypted: Vec<u8> = encrypted_bytes
            .iter()
            .enumerate()
            .map(|(i, b)| b ^ key_bytes[i % key_bytes.len()])
            .collect();
        String::from_utf8(decrypted).map_err(|e| anyhow::anyhow!("Invalid UTF-8: {}", e))
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct StorageData {
    secrets: std::collections::HashMap<String, EncryptedData>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct EncryptedData {
    nonce: String,
    ciphertext: String,
    version: u8,
}

#[derive(Debug, Serialize, Deserialize)]
struct OldStorageData {
    secrets: std::collections::HashMap<String, String>,
}

// Simple base64 encoding/decoding
mod b64 {
    pub fn encode(data: &[u8]) -> String {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(data)
    }

    pub fn decode(s: &str) -> anyhow::Result<Vec<u8>> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(s)
            .map_err(|e| anyhow::anyhow!("Base64 decode error: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_roundtrip() {
        let storage = SecureStorage::new().unwrap();
        let data = "secret-password-123!@#";

        let encrypted = storage.encrypt(data).unwrap();
        let decrypted = storage.decrypt(&encrypted).unwrap();

        assert_eq!(data, decrypted);
    }

    #[test]
    fn test_different_data_produces_different_ciphertexts() {
        let storage = SecureStorage::new().unwrap();
        let data = "test-data";

        let encrypted1 = storage.encrypt(data).unwrap();
        let encrypted2 = storage.encrypt(data).unwrap();

        // Nonce should be different
        assert_ne!(encrypted1.nonce, encrypted2.nonce);
        assert_ne!(encrypted1.ciphertext, encrypted2.ciphertext);
    }

    #[test]
    fn test_tampered_data_fails() {
        let storage = SecureStorage::new().unwrap();
        let data = "sensitive-data";

        let mut encrypted = storage.encrypt(data).unwrap();
        // Tamper with the ciphertext
        encrypted.ciphertext = b64::encode(b"tampered-data-padding-here");

        // Decryption should fail
        assert!(storage.decrypt(&encrypted).is_err());
    }

    #[test]
    fn test_secure_storage_set_get_delete() {
        let storage = SecureStorage::new().unwrap();

        // Test set and get
        storage.set("test_key", "test_value").unwrap();
        let value = storage.get("test_key").unwrap();
        assert_eq!(value, Some("test_value".to_string()));

        // Test delete
        storage.delete("test_key").unwrap();
        let value = storage.get("test_key").unwrap();
        assert_eq!(value, None);
    }
}
