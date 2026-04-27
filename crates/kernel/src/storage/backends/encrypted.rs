//! Encrypted Storage Backend
//!
//! Wrapper that adds AES-256-GCM encryption to any storage backend.
//! Data is encrypted before storage and decrypted on retrieval.
//!
//! ## Security Features
//!
//! - AES-256-GCM authenticated encryption
//! - Automatic memory zeroization of encryption keys
//! - Secure key derivation from passwords

use parking_lot::RwLock;
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, ZeroizeOnDrop};

#[cfg(feature = "encryption")]
use crate::storage::{EntryMetadata, StorageBackend, StorageEntry, StorageError};
#[cfg(not(feature = "encryption"))]
use crate::storage::{StorageBackend, StorageError};

/// Secure encryption key wrapper with automatic zeroization
///
/// The key is automatically zeroed when the wrapper is dropped.
/// Includes optional usage tracking for automatic key rotation.
pub struct SecureKey {
    key: [u8; 32],
    version: u32,
    /// Usage tracker for automatic rotation (optional)
    usage_tracker: RwLock<Option<KeyUsageTracker>>,
}

impl std::fmt::Debug for SecureKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecureKey")
            .field("version", &self.version)
            .field("has_tracking", &self.usage_tracker.read().is_some())
            .finish()
    }
}

impl SecureKey {
    /// Create a new secure key
    pub fn new(key: [u8; 32]) -> Self {
        Self {
            key,
            version: 1,
            usage_tracker: RwLock::new(None),
        }
    }

    /// Create a new secure key with version
    pub fn with_version(key: [u8; 32], version: u32) -> Self {
        Self {
            key,
            version,
            usage_tracker: RwLock::new(None),
        }
    }

    /// Create a new secure key with rotation tracking enabled
    pub fn with_rotation_config(key: [u8; 32], config: KeyRotationConfig) -> Self {
        Self {
            key,
            version: 1,
            usage_tracker: RwLock::new(Some(KeyUsageTracker::new(config))),
        }
    }

    /// Get the key bytes (read-only)
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.key
    }

    /// Get the key version
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Set rotation tracking configuration
    pub fn set_rotation_config(&self, config: KeyRotationConfig) {
        *self.usage_tracker.write() = Some(KeyUsageTracker::new(config));
    }

    /// Record an encryption operation (if tracking is enabled)
    pub fn record_encryption(&self) {
        if let Some(ref mut tracker) = *self.usage_tracker.write() {
            tracker.record_encryption();
        }
    }

    /// Record a decryption operation (if tracking is enabled)
    pub fn record_decryption(&self) {
        if let Some(ref mut tracker) = *self.usage_tracker.write() {
            tracker.record_decryption();
        }
    }

    /// Check if rotation is needed
    pub fn rotation_needed(&self) -> bool {
        self.usage_tracker
            .read()
            .as_ref()
            .map(|t| t.rotation_needed())
            .unwrap_or(false)
    }

    /// Get usage statistics
    pub fn usage_stats(&self) -> Option<KeyUsageStats> {
        self.usage_tracker.read().as_ref().map(|t| t.stats())
    }

    /// Get time until next rotation (if applicable)
    pub fn time_until_rotation(&self) -> Option<std::time::Duration> {
        self.usage_tracker
            .read()
            .as_ref()
            .and_then(|t| t.time_until_rotation())
    }
}

impl Zeroize for SecureKey {
    fn zeroize(&mut self) {
        self.key.zeroize();
        self.version = 0;
    }
}

impl Drop for SecureKey {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl ZeroizeOnDrop for SecureKey {}

/// Encrypted storage wrapper
///
/// Uses AES-256-GCM for authenticated encryption. The encryption key
/// is automatically zeroed from memory when the storage is dropped.
pub struct EncryptedStorage<B: StorageBackend> {
    inner: B,
    encryption_key: SecureKey,
}

impl<B: StorageBackend> std::fmt::Debug for EncryptedStorage<B> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncryptedStorage")
            .field("inner", &self.inner)
            .field("key_version", &self.encryption_key.version())
            .finish()
    }
}

/// Encryption header stored with encrypted data (reserved for future use)
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct EncryptionHeader {
    /// Version of encryption format
    version: u8,
    /// Nonce size
    nonce_size: u8,
    /// Tag size
    tag_size: u8,
}

impl Default for EncryptionHeader {
    fn default() -> Self {
        Self {
            version: 1,
            nonce_size: 12, // AES-GCM standard nonce size
            tag_size: 16,   // AES-GCM authentication tag size
        }
    }
}

impl<B: StorageBackend> EncryptedStorage<B> {
    /// Create encrypted storage with raw key
    pub fn with_key(inner: B, key: [u8; 32]) -> Self {
        Self {
            inner,
            encryption_key: SecureKey::new(key),
        }
    }

    /// Create encrypted storage with raw key and rotation tracking
    pub fn with_key_and_tracking(
        inner: B,
        key: [u8; 32],
        rotation_config: KeyRotationConfig,
    ) -> Self {
        Self {
            inner,
            encryption_key: SecureKey::with_rotation_config(key, rotation_config),
        }
    }

    /// Create encrypted storage with password-derived key
    pub fn with_password(inner: B, password: &str, salt: &[u8]) -> Self {
        let key = Self::derive_key(password, salt);
        Self::with_key(inner, key)
    }

    /// Create encrypted storage with password-derived key and rotation tracking
    pub fn with_password_and_tracking(
        inner: B,
        password: &str,
        salt: &[u8],
        rotation_config: KeyRotationConfig,
    ) -> Self {
        let key = Self::derive_key(password, salt);
        Self {
            inner,
            encryption_key: SecureKey::with_rotation_config(key, rotation_config),
        }
    }

    /// Enable key rotation tracking
    pub fn enable_rotation_tracking(&self, config: KeyRotationConfig) {
        self.encryption_key.set_rotation_config(config);
    }

    /// Check if key rotation is needed
    pub fn rotation_needed(&self) -> bool {
        self.encryption_key.rotation_needed()
    }

    /// Get key usage statistics
    pub fn key_usage_stats(&self) -> Option<KeyUsageStats> {
        self.encryption_key.usage_stats()
    }

    /// Derive encryption key from password using PBKDF2-like approach
    fn derive_key(password: &str, salt: &[u8]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(password.as_bytes());
        hasher.update(salt);

        // Simple key stretching
        for _ in 0..10000 {
            let hash = hasher.finalize_reset();
            hasher.update(&hash);
        }

        let result = hasher.finalize();
        let mut key = [0u8; 32];
        key.copy_from_slice(&result);
        key
    }

    /// Get inner backend
    pub fn inner(&self) -> &B {
        &self.inner
    }

    /// Consume and return inner backend
    ///
    /// Note: The encryption key is zeroed when this method is called.
    pub fn into_inner(self) -> B {
        self.inner
    }

    /// Get current key version
    pub fn key_version(&self) -> u32 {
        self.encryption_key.version()
    }

    /// Rotate encryption key (decrypt with old, re-encrypt with new)
    ///
    /// # Security Note
    ///
    /// This implementation currently only swaps the key without re-encrypting
    /// existing data. Use `rotate_key_and_reencrypt` for full re-encryption.
    pub fn rotate_key(self, new_key: [u8; 32]) -> Result<Self, StorageError> {
        Ok(Self {
            inner: self.inner,
            encryption_key: SecureKey::new(new_key),
        })
    }

    /// Rotate encryption key and re-encrypt all data
    ///
    /// This method iterates through all entries in the storage and
    /// re-encrypts them with the new key. The old key is used for
    /// decryption and then securely zeroed.
    ///
    /// # Warning
    ///
    /// This operation can be slow for large datasets. Consider running
    /// it during maintenance windows.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use beebotos_kernel::storage::{InMemoryStorage, EncryptedStorage};
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let inner = InMemoryStorage::new();
    /// # let key1 = [0u8; 32]; // Use a securely generated key in production
    /// # let storage = EncryptedStorage::with_key(inner, key1);
    /// # let key2 = [1u8; 32]; // New key for rotation
    /// let rotated = storage.rotate_key_and_reencrypt(key2)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn rotate_key_and_reencrypt(self, new_key: [u8; 32]) -> Result<Self, StorageError> {
        tracing::info!("Starting key rotation with re-encryption");

        // List all keys in the storage
        let keys = self.inner.list("")?;
        tracing::info!("Re-encrypting {} entries", keys.len());

        let mut reencrypted_count = 0;
        let mut failed_count = 0;

        for key in &keys {
            match self.inner.get(key) {
                Ok(Some(entry)) => {
                    // Decrypt with old key
                    match self.decrypt(&entry.data) {
                        Ok(plaintext) => {
                            // Store decrypted data temporarily
                            let metadata = entry.metadata;

                            // We need to re-encrypt with new key
                            // For now, we'll do this by storing the data back
                            // The new key will be applied after all data is processed

                            // Store the plaintext temporarily (this is safe as we'll re-encrypt)
                            if let Err(e) = self.inner.put(&key, &plaintext, metadata) {
                                tracing::warn!("Failed to update entry {}: {}", key, e);
                                failed_count += 1;
                            } else {
                                reencrypted_count += 1;
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to decrypt entry {}: {}", key, e);
                            failed_count += 1;
                        }
                    }
                }
                Ok(None) => {
                    tracing::debug!("Entry {} not found during rotation", key);
                }
                Err(e) => {
                    tracing::warn!("Failed to read entry {}: {}", key, e);
                    failed_count += 1;
                }
            }
        }

        tracing::info!(
            "Key rotation complete: {} re-encrypted, {} failed",
            reencrypted_count,
            failed_count
        );

        if failed_count > 0 {
            tracing::warn!("Some entries failed to re-encrypt during key rotation");
        }

        // Now swap to the new key
        // The data is currently in plaintext in the backend
        // We need to re-encrypt it with the new key

        // Create new storage with new key
        let new_storage = Self {
            inner: self.inner,
            encryption_key: SecureKey::with_version(new_key, self.encryption_key.version() + 1),
        };

        // Re-encrypt all data with the new key
        let keys = new_storage.inner.list("")?;
        for key in &keys {
            if let Ok(Some(entry)) = new_storage.inner.get(key) {
                // Data is currently plaintext, encrypt with new key
                match new_storage.encrypt(&entry.data) {
                    Ok(ciphertext) => {
                        let metadata = entry.metadata;
                        let _ = new_storage.inner.put(&key, &ciphertext, metadata);
                    }
                    Err(e) => {
                        tracing::error!("Failed to encrypt entry {} with new key: {}", key, e);
                    }
                }
            }
        }

        // Old key is automatically zeroed when self is dropped
        tracing::info!(
            "Key rotation complete. New key version: {}",
            new_storage.key_version()
        );
        Ok(new_storage)
    }
}

#[cfg(feature = "encryption")]
impl<B: StorageBackend> EncryptedStorage<B> {
    /// Encrypt data using AES-256-GCM
    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, StorageError> {
        use aes_gcm::aead::{Aead, KeyInit};
        use aes_gcm::{Aes256Gcm, Nonce};
        use rand::Rng;

        // Record encryption operation
        self.encryption_key.record_encryption();

        let cipher = Aes256Gcm::new_from_slice(self.encryption_key.as_bytes())
            .map_err(|e| StorageError::IoError(format!("Invalid key: {}", e)))?;

        // Generate random nonce
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| StorageError::IoError(format!("Encryption failed: {}", e)))?;

        // Format: [nonce (12 bytes)] [ciphertext + tag]
        let mut result = Vec::with_capacity(12 + ciphertext.len());
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);

        Ok(result)
    }

    /// Decrypt data using AES-256-GCM
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, StorageError> {
        use aes_gcm::aead::{Aead, KeyInit};
        use aes_gcm::{Aes256Gcm, Nonce};

        // Record decryption operation
        self.encryption_key.record_decryption();

        if ciphertext.len() < 12 {
            return Err(StorageError::CorruptedData);
        }

        let cipher = Aes256Gcm::new_from_slice(self.encryption_key.as_bytes())
            .map_err(|e| StorageError::IoError(format!("Invalid key: {}", e)))?;

        // Extract nonce
        let nonce = Nonce::from_slice(&ciphertext[..12]);
        let encrypted_data = &ciphertext[12..];

        // Decrypt
        let plaintext = cipher
            .decrypt(nonce, encrypted_data)
            .map_err(|e| StorageError::IoError(format!("Decryption failed: {}", e)))?;

        Ok(plaintext)
    }
}

#[cfg(feature = "encryption")]
impl<B: StorageBackend> StorageBackend for EncryptedStorage<B> {
    fn put(&self, key: &str, data: &[u8], metadata: EntryMetadata) -> Result<(), StorageError> {
        // Encrypt the data
        let encrypted = self.encrypt(data)?;

        // Add encryption flag to metadata
        let mut metadata = metadata;
        metadata.content_type = format!("encrypted;{}", metadata.content_type);

        // Store encrypted data
        self.inner.put(key, &encrypted, metadata)
    }

    fn get(&self, key: &str) -> Result<Option<StorageEntry>, StorageError> {
        match self.inner.get(key)? {
            Some(entry) => {
                // Decrypt data
                let decrypted = self.decrypt(&entry.data)?;

                // Strip encryption prefix from content type
                let content_type = entry
                    .metadata
                    .content_type
                    .strip_prefix("encrypted;")
                    .unwrap_or(&entry.metadata.content_type)
                    .to_string();

                Ok(Some(StorageEntry {
                    key: entry.key,
                    data: decrypted,
                    metadata: EntryMetadata {
                        content_type,
                        ..entry.metadata
                    },
                }))
            }
            None => Ok(None),
        }
    }

    fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.inner.delete(key)
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
        self.inner.list(prefix)
    }

    fn exists(&self, key: &str) -> Result<bool, StorageError> {
        self.inner.exists(key)
    }
}

// Use macro for stub implementation
crate::define_stub_encrypted_backend!();

/// Generate a random encryption key
pub fn generate_key() -> [u8; 32] {
    use rand::Rng;
    let mut key = [0u8; 32];
    rand::thread_rng().fill(&mut key);
    key
}

/// Generate a random salt
pub fn generate_salt() -> [u8; 16] {
    use rand::Rng;
    let mut salt = [0u8; 16];
    rand::thread_rng().fill(&mut salt);
    salt
}

/// Key rotation configuration
///
/// Defines policies for automatic key rotation.
#[derive(Debug, Clone)]
pub struct KeyRotationConfig {
    /// Enable automatic key rotation
    pub enabled: bool,
    /// Rotate after this many days (0 = disable time-based rotation)
    pub rotate_after_days: u64,
    /// Rotate after this many encryption operations (0 = disable usage-based
    /// rotation)
    pub rotate_after_operations: u64,
    /// Minimum key version to maintain (for backward compatibility)
    pub min_key_version: u32,
}

impl Default for KeyRotationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            rotate_after_days: 90,      // Rotate every 90 days by default
            rotate_after_operations: 0, // Disabled by default
            min_key_version: 1,
        }
    }
}

impl KeyRotationConfig {
    /// Create default rotation config with enabled rotation
    pub fn enabled() -> Self {
        Self {
            enabled: true,
            ..Default::default()
        }
    }

    /// Disable automatic rotation
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }
}

/// Key usage tracker for automatic rotation
///
/// Tracks key usage metrics to determine when rotation is needed.
#[derive(Debug, Clone)]
pub struct KeyUsageTracker {
    /// Key creation timestamp
    pub created_at: std::time::Instant,
    /// Number of encryption operations performed
    pub encryption_count: u64,
    /// Number of decryption operations performed
    pub decryption_count: u64,
    /// Rotation configuration
    pub config: KeyRotationConfig,
}

impl KeyUsageTracker {
    /// Create a new usage tracker
    pub fn new(config: KeyRotationConfig) -> Self {
        Self {
            created_at: std::time::Instant::now(),
            encryption_count: 0,
            decryption_count: 0,
            config,
        }
    }

    /// Record an encryption operation
    pub fn record_encryption(&mut self) {
        self.encryption_count += 1;
    }

    /// Record a decryption operation
    pub fn record_decryption(&mut self) {
        self.decryption_count += 1;
    }

    /// Check if key rotation is needed based on configured policies
    pub fn rotation_needed(&self) -> bool {
        if !self.config.enabled {
            return false;
        }

        // Check time-based rotation
        if self.config.rotate_after_days > 0 {
            let elapsed = self.created_at.elapsed();
            let rotate_after =
                std::time::Duration::from_secs(self.config.rotate_after_days * 86400);
            if elapsed >= rotate_after {
                return true;
            }
        }

        // Check usage-based rotation
        if self.config.rotate_after_operations > 0 {
            let total_ops = self.encryption_count + self.decryption_count;
            if total_ops >= self.config.rotate_after_operations {
                return true;
            }
        }

        false
    }

    /// Get time until next rotation (if time-based rotation is enabled)
    pub fn time_until_rotation(&self) -> Option<std::time::Duration> {
        if !self.config.enabled || self.config.rotate_after_days == 0 {
            return None;
        }

        let rotate_after = std::time::Duration::from_secs(self.config.rotate_after_days * 86400);
        let elapsed = self.created_at.elapsed();

        if elapsed >= rotate_after {
            Some(std::time::Duration::ZERO)
        } else {
            Some(rotate_after - elapsed)
        }
    }

    /// Get usage statistics
    pub fn stats(&self) -> KeyUsageStats {
        KeyUsageStats {
            age: self.created_at.elapsed(),
            encryption_count: self.encryption_count,
            decryption_count: self.decryption_count,
            total_operations: self.encryption_count + self.decryption_count,
        }
    }
}

/// Key usage statistics
#[derive(Debug, Clone)]
pub struct KeyUsageStats {
    /// How long the key has been in use
    pub age: std::time::Duration,
    /// Number of encryption operations
    pub encryption_count: u64,
    /// Number of decryption operations
    pub decryption_count: u64,
    /// Total cryptographic operations
    pub total_operations: u64,
}

impl std::fmt::Display for KeyUsageStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Key age: {:?}, Encryptions: {}, Decryptions: {}, Total: {}",
            self.age, self.encryption_count, self.decryption_count, self.total_operations
        )
    }
}

#[cfg(test)]
#[cfg(feature = "encryption")]
mod tests {
    use super::*;
    use crate::storage::backends::memory::InMemoryStorage;
    use crate::storage::test_utils::create_test_metadata;

    #[test]
    fn test_encrypt_decrypt() {
        let inner = InMemoryStorage::new();
        let key = generate_key();
        let storage = EncryptedStorage::with_key(inner, key);
        let metadata = create_test_metadata();

        // Store encrypted data
        storage.put("secret", b"sensitive data", metadata).unwrap();

        // Retrieve and decrypt
        let entry = storage.get("secret").unwrap().unwrap();
        assert_eq!(entry.data, b"sensitive data");
    }

    #[test]
    fn test_password_derived_key() {
        let inner = InMemoryStorage::new();
        let salt = generate_salt();
        let storage = EncryptedStorage::with_password(inner, "my_password", &salt);
        let metadata = create_test_metadata();

        storage.put("key1", b"value1", metadata.clone()).unwrap();
        let entry = storage.get("key1").unwrap().unwrap();
        assert_eq!(entry.data, b"value1");
    }

    #[test]
    fn test_wrong_key_fails() {
        let inner = InMemoryStorage::new();
        let key1 = generate_key();
        let key2 = generate_key();

        let storage1 = EncryptedStorage::with_key(inner, key1);
        let metadata = create_test_metadata();

        storage1.put("key1", b"value1", metadata).unwrap();

        // Try to decrypt with wrong key
        let storage2 = EncryptedStorage::with_key(storage1.into_inner(), key2);
        assert!(storage2.get("key1").is_err());
    }

    #[test]
    fn test_key_generation() {
        let key1 = generate_key();
        let key2 = generate_key();
        assert_ne!(key1, key2);
    }
}
