//! Wallet Module
//!
//! HD wallet and secure key management using BIP39/BIP32 standards.
//! Migrated to use Alloy primitives.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use alloy_signer::{Signature, Signer};
use alloy_signer_local::PrivateKeySigner;
use coins_bip32::path::DerivationPath;
use coins_bip32::prelude::XPriv;
use coins_bip39::{English, Mnemonic as CoinsMnemonic};
use pbkdf2::pbkdf2_hmac;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tracing::{debug, info};
use zeroize::Zeroize;

use crate::compat::{Address, B256};
use crate::constants::{
    AES_GCM_NONCE_SIZE, AES_GCM_SALT_SIZE, ETHEREUM_DERIVATION_PREFIX, PBKDF2_ITERATIONS,
};

/// Wallet using Alloy signer
pub struct Wallet {
    inner: PrivateKeySigner,
    #[allow(dead_code)]
    derivation_path: String,
}

/// HD Wallet with secure mnemonic handling
///
/// Implements BIP39 for mnemonic generation and BIP32 for hierarchical
/// deterministic key derivation. Sensitive data is automatically zeroized
/// when dropped.
pub struct HDWallet {
    /// BIP39 mnemonic phrase (zeroized on drop)
    mnemonic: Mnemonic,
    /// BIP32 master seed (automatically zeroized)
    seed: [u8; 64],
    /// Derived accounts
    accounts: Vec<AccountInfo>,
}

impl Zeroize for HDWallet {
    fn zeroize(&mut self) {
        self.mnemonic.zeroize();
        self.seed = [0u8; 64];
    }
}

impl Drop for HDWallet {
    fn drop(&mut self) {
        self.zeroize();
    }
}

/// BIP39 mnemonic wrapper with secure handling
///
/// SECURITY FIX: Uses secrecy::SecretString to prevent accidental exposure
/// and automatic zeroization on drop.
#[derive(Clone)]
pub struct Mnemonic {
    /// SECURITY FIX: SecretString prevents the mnemonic from being logged or
    /// displayed
    phrase: secrecy::SecretString,
}

impl Mnemonic {
    /// Create from phrase string
    pub fn from_phrase(phrase: &str) -> Result<Self, WalletError> {
        // Validate the mnemonic using coins-bip39
        let _ = CoinsMnemonic::<English>::new_from_phrase(phrase)
            .map_err(|e| WalletError::InvalidMnemonic(format!("Invalid mnemonic: {}", e)))?;

        Ok(Self {
            phrase: secrecy::SecretString::new(phrase.to_string().into()),
        })
    }

    /// Generate new random mnemonic
    pub fn generate(word_count: usize) -> Result<Self, WalletError> {
        // Validate word count
        if ![12, 15, 18, 21, 24].contains(&word_count) {
            return Err(WalletError::InvalidMnemonic(format!(
                "Invalid word count: {}. Use 12, 15, 18, 21, or 24",
                word_count
            )));
        }

        // new_with_count expects word count (12, 15, 18, 21, 24)
        let mnemonic =
            CoinsMnemonic::<English>::new_with_count(&mut rand::thread_rng(), word_count).map_err(
                |e| WalletError::InvalidMnemonic(format!("Failed to generate mnemonic: {}", e)),
            )?;

        Ok(Self {
            phrase: secrecy::SecretString::new(mnemonic.to_phrase().into()),
        })
    }

    /// Get phrase string
    ///
    /// SECURITY: Returns &str for temporary access. The SecretString prevents
    /// the phrase from being accidentally logged or displayed.
    pub fn phrase(&self) -> &str {
        self.phrase.expose_secret()
    }

    /// Get the secret string (for internal use)
    #[allow(dead_code)]
    pub(crate) fn secret_phrase(&self) -> &secrecy::SecretString {
        &self.phrase
    }
}

impl Zeroize for Mnemonic {
    fn zeroize(&mut self) {
        // SecretString handles zeroization internally
        // This is a no-op but kept for compatibility
    }
}

impl Drop for Mnemonic {
    fn drop(&mut self) {
        // SecretString automatically zeroizes on drop
        // This ensures the mnemonic is cleared from memory
    }
}

/// Account info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub address: Address,
    pub derivation_path: String,
    pub index: u32,
    pub name: Option<String>,
}

/// Wallet configuration
#[derive(Debug, Clone)]
pub struct WalletConfig {
    pub chain_id: u64,
    pub derivation_path_prefix: String,
}

impl Default for WalletConfig {
    fn default() -> Self {
        Self {
            chain_id: 10143, // Monad testnet
            derivation_path_prefix: ETHEREUM_DERIVATION_PREFIX.to_string(),
        }
    }
}

impl Wallet {
    /// Create from private key bytes
    pub fn from_key(key: &[u8]) -> Result<Self, WalletError> {
        let inner = PrivateKeySigner::from_slice(key).map_err(|_| WalletError::InvalidKey)?;
        Ok(Self {
            inner,
            derivation_path: String::new(),
        })
    }

    /// Create random wallet
    pub fn random() -> Self {
        let inner = PrivateKeySigner::random();
        Self {
            inner,
            derivation_path: String::new(),
        }
    }

    /// Get address
    pub fn address(&self) -> Address {
        self.inner.address()
    }

    /// Get underlying signer
    pub fn signer(&self) -> &PrivateKeySigner {
        &self.inner
    }

    /// Sign message
    pub async fn sign_message(&self, message: &[u8]) -> Result<Signature, WalletError> {
        self.inner
            .sign_message(message)
            .await
            .map_err(|e| WalletError::SigningError(e.to_string()))
    }

    /// Sign transaction hash
    pub async fn sign_hash(&self, hash: &B256) -> Result<Signature, WalletError> {
        // Convert B256 reference to the expected type
        self.inner
            .sign_message(hash.as_slice())
            .await
            .map_err(|e| WalletError::SigningError(e.to_string()))
    }
}

impl HDWallet {
    /// Create from mnemonic phrase
    pub fn from_mnemonic(mnemonic_phrase: &str) -> Result<Self, WalletError> {
        info!("Creating HD wallet from mnemonic");

        // Parse and validate mnemonic
        let mnemonic = Mnemonic::from_phrase(mnemonic_phrase)?;

        // Generate BIP32 seed from mnemonic (no passphrase) using coins-bip39
        let coins_mnemonic = CoinsMnemonic::<English>::new_from_phrase(mnemonic_phrase)
            .map_err(|e| WalletError::InvalidMnemonic(e.to_string()))?;

        // Generate seed from mnemonic (64 bytes)
        let seed = coins_mnemonic.to_seed(Some(""))?;

        debug!("HD wallet created successfully");

        Ok(Self {
            mnemonic,
            seed,
            accounts: vec![],
        })
    }

    /// Generate new random mnemonic with specified word count
    /// Supported word counts: 12, 15, 18, 21, 24
    pub fn generate_mnemonic(word_count: usize) -> Result<String, WalletError> {
        info!("Generating new mnemonic with {} words", word_count);

        // Validate word count
        if ![12, 15, 18, 21, 24].contains(&word_count) {
            return Err(WalletError::InvalidMnemonic(format!(
                "Invalid word count: {}. Use 12, 15, 18, 21, or 24",
                word_count
            )));
        }

        // Mnemonic::generate takes word count directly
        let mnemonic = Mnemonic::generate(word_count)?;
        let phrase = mnemonic.phrase().to_string();

        Ok(phrase)
    }

    /// Generate standard 12-word mnemonic (convenience method)
    pub fn generate_mnemonic_12() -> Result<String, WalletError> {
        Self::generate_mnemonic(12)
    }

    /// Generate high-security 24-word mnemonic (convenience method)
    pub fn generate_mnemonic_24() -> Result<String, WalletError> {
        Self::generate_mnemonic(24)
    }

    /// Derive account at specified index
    pub fn derive_account(
        &self,
        index: u32,
        name: Option<String>,
    ) -> Result<AccountInfo, WalletError> {
        let path = format!("{}/{}", ETHEREUM_DERIVATION_PREFIX, index);
        debug!("Deriving account at path: {}", path);

        // Parse derivation path using coins-bip32
        let derivation_path: DerivationPath = path
            .parse()
            .map_err(|e| WalletError::InvalidDerivationPath(format!("{:?}", e)))?;

        // Derive private key from seed using coins-bip32
        let master = XPriv::root_from_seed(&self.seed, None)
            .map_err(|e| WalletError::DerivationError(e.to_string()))?;
        let derived_key = master
            .derive_path(&derivation_path)
            .map_err(|e| WalletError::DerivationError(e.to_string()))?;

        // Create signer from derived key
        // Get the private key bytes from the derived XPriv
        // XPriv implements AsRef<coins_bip32::ecdsa::SigningKey>
        let signing_key: &coins_bip32::ecdsa::SigningKey = derived_key.as_ref();
        let private_key_bytes = signing_key.to_bytes();
        let signer = PrivateKeySigner::from_slice(&private_key_bytes)
            .map_err(|_| WalletError::InvalidKey)?;

        let address: Address = signer.address().into();

        let account = AccountInfo {
            address,
            derivation_path: path,
            index,
            name,
        };

        info!("Derived account {} at address {:?}", index, address);
        Ok(account)
    }

    /// Derive a Wallet (signer) at specified index
    pub fn derive_wallet(&self, index: u32) -> Result<Wallet, WalletError> {
        let path = format!("{}/{}", ETHEREUM_DERIVATION_PREFIX, index);

        let derivation_path: DerivationPath = path
            .parse()
            .map_err(|e| WalletError::InvalidDerivationPath(format!("{:?}", e)))?;

        let master = XPriv::root_from_seed(&self.seed, None)
            .map_err(|e| WalletError::DerivationError(e.to_string()))?;
        let derived_key = master
            .derive_path(&derivation_path)
            .map_err(|e| WalletError::DerivationError(e.to_string()))?;

        let signing_key: &coins_bip32::ecdsa::SigningKey = derived_key.as_ref();
        let private_key_bytes = signing_key.to_bytes();

        Wallet::from_key(&private_key_bytes)
    }

    /// Derive account with custom path
    pub fn derive_account_with_path(
        &self,
        path: &str,
        name: Option<String>,
    ) -> Result<AccountInfo, WalletError> {
        debug!("Deriving account at custom path: {}", path);

        let derivation_path: DerivationPath = path
            .parse()
            .map_err(|e| WalletError::InvalidDerivationPath(format!("{:?}", e)))?;

        // Derive private key from seed using coins-bip32
        let master = XPriv::root_from_seed(&self.seed, None)
            .map_err(|e| WalletError::DerivationError(e.to_string()))?;
        let derived_key = master
            .derive_path(&derivation_path)
            .map_err(|e| WalletError::DerivationError(e.to_string()))?;

        // Get the private key bytes from the derived XPriv
        // XPriv implements AsRef<coins_bip32::ecdsa::SigningKey>
        let signing_key: &coins_bip32::ecdsa::SigningKey = derived_key.as_ref();
        let private_key_bytes = signing_key.to_bytes();
        let signer = PrivateKeySigner::from_slice(&private_key_bytes)
            .map_err(|_| WalletError::InvalidKey)?;

        let address: Address = signer.address().into();

        // Extract index from path if possible
        let index = path
            .split('/')
            .last()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);

        let account = AccountInfo {
            address,
            derivation_path: path.to_string(),
            index,
            name,
        };

        Ok(account)
    }

    /// Derive multiple accounts at once
    pub fn derive_accounts(
        &mut self,
        start_index: u32,
        count: u32,
        name_prefix: Option<&str>,
    ) -> Result<Vec<AccountInfo>, WalletError> {
        let mut accounts = Vec::with_capacity(count as usize);

        for i in 0..count {
            let index = start_index + i;
            let name = name_prefix.map(|prefix| format!("{}{}", prefix, index));
            let account = self.derive_account(index, name)?;
            accounts.push(account);
        }

        self.accounts.extend(accounts.clone());
        Ok(accounts)
    }

    /// Get account at index (must be derived first)
    pub fn account(&self, index: u32) -> Option<&AccountInfo> {
        self.accounts.iter().find(|a| a.index == index)
    }

    /// Get account by address
    pub fn account_by_address(&self, address: &Address) -> Option<&AccountInfo> {
        self.accounts.iter().find(|a| &a.address == address)
    }

    /// List all derived accounts
    pub fn accounts(&self) -> &[AccountInfo] {
        &self.accounts
    }

    /// Get the mnemonic phrase
    pub fn mnemonic_phrase(&self) -> &str {
        self.mnemonic.phrase()
    }

    /// Export mnemonic as encrypted string using AES-256-GCM
    ///
    /// Uses PBKDF2 with SHA-256 for key derivation and AES-256-GCM for
    /// authenticated encryption. This provides strong confidentiality and
    /// integrity protection for the mnemonic.
    pub fn export_encrypted(&self, password: &str) -> Result<EncryptedMnemonic, WalletError> {
        // Generate random salt
        let salt: [u8; AES_GCM_SALT_SIZE] = rand::random();

        // Generate random nonce for AES-GCM
        let nonce_bytes: [u8; AES_GCM_NONCE_SIZE] = rand::random();

        // Derive 256-bit key using PBKDF2
        let mut key_bytes = [0u8; 32];
        pbkdf2_hmac::<Sha256>(
            password.as_bytes(),
            &salt,
            PBKDF2_ITERATIONS,
            &mut key_bytes,
        );

        // Create AES-256-GCM cipher
        let cipher = Aes256Gcm::new_from_slice(&key_bytes)
            .map_err(|e| WalletError::EncryptionError(format!("Failed to create cipher: {}", e)))?;

        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt the mnemonic
        let mnemonic_bytes = self.mnemonic.phrase().as_bytes();
        let ciphertext = cipher
            .encrypt(nonce, mnemonic_bytes)
            .map_err(|e| WalletError::EncryptionError(format!("Encryption failed: {}", e)))?;

        // Zeroize sensitive key material
        key_bytes.zeroize();

        Ok(EncryptedMnemonic {
            ciphertext,
            salt: salt.to_vec(),
            iv: nonce_bytes.to_vec(),
            version: 1, // Version for future compatibility
        })
    }

    /// Decrypt and restore mnemonic using AES-256-GCM
    ///
    /// Verifies the password and decrypts the mnemonic. Returns an error if the
    /// password is incorrect or if the ciphertext has been tampered with.
    pub fn decrypt_mnemonic(
        encrypted: &EncryptedMnemonic,
        password: &str,
    ) -> Result<String, WalletError> {
        // Check version compatibility
        if encrypted.version != 1 {
            return Err(WalletError::EncryptionError(format!(
                "Unsupported encryption version: {}",
                encrypted.version
            )));
        }

        // Validate salt and IV lengths
        if encrypted.salt.len() != AES_GCM_SALT_SIZE {
            return Err(WalletError::EncryptionError(
                "Invalid salt length".to_string(),
            ));
        }
        if encrypted.iv.len() != AES_GCM_NONCE_SIZE {
            return Err(WalletError::EncryptionError(
                "Invalid nonce length".to_string(),
            ));
        }

        // Derive key using PBKDF2
        let mut key_bytes = [0u8; 32];
        pbkdf2_hmac::<Sha256>(
            password.as_bytes(),
            &encrypted.salt,
            PBKDF2_ITERATIONS,
            &mut key_bytes,
        );

        // Create cipher
        let cipher = Aes256Gcm::new_from_slice(&key_bytes)
            .map_err(|e| WalletError::EncryptionError(format!("Failed to create cipher: {}", e)))?;

        let nonce = Nonce::from_slice(&encrypted.iv);

        // Decrypt
        let decrypted = cipher
            .decrypt(nonce, encrypted.ciphertext.as_ref())
            .map_err(|_| WalletError::InvalidPassword)?;

        // Zeroize sensitive key material
        key_bytes.zeroize();

        String::from_utf8(decrypted).map_err(|_| {
            WalletError::EncryptionError("Invalid UTF-8 in decrypted data".to_string())
        })
    }
}

/// Encrypted mnemonic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedMnemonic {
    pub ciphertext: Vec<u8>,
    pub salt: Vec<u8>,
    pub iv: Vec<u8>,
    /// Encryption version for backward compatibility
    #[serde(default = "default_version")]
    pub version: u32,
}

fn default_version() -> u32 {
    1
}

/// Key storage
pub struct KeyStore {
    path: std::path::PathBuf,
}

impl KeyStore {
    /// Open key store
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, WalletError> {
        let path = path.as_ref().to_path_buf();
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    /// Store encrypted key
    pub fn store(&self, address: &Address, encrypted: &EncryptedKey) -> Result<(), WalletError> {
        let path = self.path.join(format!("{:?}.json", address));
        let data = serde_json::to_vec(encrypted)?;
        std::fs::write(&path, data)?;
        debug!("Stored key for address {:?}", address);
        Ok(())
    }

    /// Load encrypted key
    pub fn load(&self, address: &Address) -> Result<Option<EncryptedKey>, WalletError> {
        let path = self.path.join(format!("{:?}.json", address));
        if !path.exists() {
            return Ok(None);
        }
        let data = std::fs::read(&path)?;
        let encrypted: EncryptedKey = serde_json::from_slice(&data)?;
        Ok(Some(encrypted))
    }

    /// List stored addresses
    pub fn list(&self) -> Result<Vec<Address>, WalletError> {
        let mut addresses = vec![];
        for entry in std::fs::read_dir(&self.path)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(addr_str) = name.strip_suffix(".json") {
                if let Ok(addr) = addr_str.parse::<Address>() {
                    addresses.push(addr);
                }
            }
        }
        Ok(addresses)
    }

    /// Delete stored key
    pub fn delete(&self, address: &Address) -> Result<(), WalletError> {
        let path = self.path.join(format!("{:?}.json", address));
        if path.exists() {
            std::fs::remove_file(&path)?;
            info!("Deleted key for address {:?}", address);
        }
        Ok(())
    }
}

/// Encrypted key (Web3 Secret Storage Definition)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedKey {
    pub version: u32,
    pub id: String,
    pub address: String,
    pub crypto: CryptoData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoData {
    pub ciphertext: String,
    pub cipherparams: CipherParams,
    pub cipher: String,
    pub kdf: String,
    pub kdfparams: KdfParams,
    pub mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CipherParams {
    pub iv: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KdfParams {
    pub dklen: u32,
    pub salt: String,
    pub n: u32,
    pub r: u32,
    pub p: u32,
}

/// Wallet errors
#[derive(Debug)]
pub enum WalletError {
    InvalidKey,
    InvalidMnemonic(String),
    InvalidPassword,
    InvalidDerivationPath(String),
    DerivationError(String),
    StorageError(std::io::Error),
    SerializationError(serde_json::Error),
    SigningError(String),
    EncryptionError(String),
    NotFound,
}

impl std::fmt::Display for WalletError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WalletError::InvalidKey => write!(f, "Invalid key"),
            WalletError::InvalidMnemonic(msg) => write!(f, "Invalid mnemonic: {}", msg),
            WalletError::InvalidPassword => write!(f, "Invalid password"),
            WalletError::InvalidDerivationPath(msg) => {
                write!(f, "Invalid derivation path: {}", msg)
            }
            WalletError::DerivationError(msg) => write!(f, "Key derivation error: {}", msg),
            WalletError::StorageError(e) => write!(f, "Storage error: {}", e),
            WalletError::SerializationError(e) => write!(f, "Serialization error: {}", e),
            WalletError::SigningError(e) => write!(f, "Signing error: {}", e),
            WalletError::EncryptionError(e) => write!(f, "Encryption error: {}", e),
            WalletError::NotFound => write!(f, "Wallet not found"),
        }
    }
}

impl std::error::Error for WalletError {}

impl From<std::io::Error> for WalletError {
    fn from(e: std::io::Error) -> Self {
        WalletError::StorageError(e)
    }
}

impl From<serde_json::Error> for WalletError {
    fn from(e: serde_json::Error) -> Self {
        WalletError::SerializationError(e)
    }
}

impl From<alloy_signer::Error> for WalletError {
    fn from(e: alloy_signer::Error) -> Self {
        WalletError::SigningError(e.to_string())
    }
}

impl From<coins_bip39::MnemonicError> for WalletError {
    fn from(e: coins_bip39::MnemonicError) -> Self {
        WalletError::InvalidMnemonic(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mnemonic_generation() {
        // Test 12-word mnemonic
        let mnemonic = HDWallet::generate_mnemonic(12).unwrap();
        let words: Vec<&str> = mnemonic.split_whitespace().collect();
        assert_eq!(words.len(), 12);

        // Test 24-word mnemonic
        let mnemonic = HDWallet::generate_mnemonic(24).unwrap();
        let words: Vec<&str> = mnemonic.split_whitespace().collect();
        assert_eq!(words.len(), 24);
    }

    #[test]
    fn test_wallet_from_mnemonic() {
        let test_mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon \
                             abandon abandon abandon about";
        let wallet = HDWallet::from_mnemonic(test_mnemonic).unwrap();

        // Derive first account
        let account = wallet
            .derive_account(0, Some("Test Account".to_string()))
            .unwrap();

        // Address should be valid (not zero)
        assert!(!account.address.is_zero());
        assert_eq!(account.index, 0);
        assert_eq!(account.name, Some("Test Account".to_string()));
    }

    #[test]
    fn test_derivation_path() {
        let test_mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon \
                             abandon abandon abandon about";
        let wallet = HDWallet::from_mnemonic(test_mnemonic).unwrap();

        // Derive multiple accounts
        let account0 = wallet.derive_account(0, None).unwrap();
        let account1 = wallet.derive_account(1, None).unwrap();

        // Different indices should produce different addresses
        assert_ne!(account0.address, account1.address);

        // Check derivation paths
        assert!(account0.derivation_path.contains("m/44'/60'/0'/0/0"));
        assert!(account1.derivation_path.contains("m/44'/60'/0'/0/1"));
    }

    #[test]
    fn test_custom_derivation_path() {
        let test_mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon \
                             abandon abandon abandon about";
        let wallet = HDWallet::from_mnemonic(test_mnemonic).unwrap();

        // Custom path
        let account = wallet
            .derive_account_with_path("m/44'/60'/1'/0/0", None)
            .unwrap();
        assert!(!account.address.is_zero());
    }

    #[test]
    fn test_mnemonic_encryption() {
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon \
                        abandon abandon about";
        let wallet = HDWallet::from_mnemonic(mnemonic).unwrap();

        // Encrypt
        let encrypted = wallet.export_encrypted("my_password").unwrap();
        assert!(!encrypted.ciphertext.is_empty());
        assert_eq!(encrypted.salt.len(), 16);
        assert_eq!(encrypted.iv.len(), 12);
        assert_eq!(encrypted.version, 1);

        // Decrypt
        let decrypted = HDWallet::decrypt_mnemonic(&encrypted, "my_password").unwrap();
        assert_eq!(decrypted, mnemonic);

        // Wrong password should fail
        let result = HDWallet::decrypt_mnemonic(&encrypted, "wrong_password");
        assert!(matches!(result, Err(WalletError::InvalidPassword)));

        // Tampered ciphertext should fail
        let mut tampered = encrypted.clone();
        tampered.ciphertext[0] ^= 0xFF;
        let result = HDWallet::decrypt_mnemonic(&tampered, "my_password");
        assert!(matches!(result, Err(WalletError::InvalidPassword)));
    }

    #[test]
    fn test_keystore() {
        let temp_dir = std::env::temp_dir().join("test_keystore_alloy");

        // Cleanup before test
        let _ = std::fs::remove_dir_all(&temp_dir);

        // Open keystore after cleanup (creates directory)
        let keystore = KeyStore::open(&temp_dir).unwrap();

        let address = Address::from_slice(&[1u8; 20]);
        let encrypted = EncryptedKey {
            version: 3,
            id: "test-id".to_string(),
            address: format!("{:?}", address),
            crypto: CryptoData {
                ciphertext: "test".to_string(),
                cipherparams: CipherParams {
                    iv: "test".to_string(),
                },
                cipher: "aes-128-ctr".to_string(),
                kdf: "scrypt".to_string(),
                kdfparams: KdfParams {
                    dklen: 32,
                    salt: "test".to_string(),
                    n: 262144,
                    r: 8,
                    p: 1,
                },
                mac: "test".to_string(),
            },
        };

        // Store
        keystore.store(&address, &encrypted).unwrap();

        // Load
        let loaded = keystore.load(&address).unwrap();
        assert!(loaded.is_some());

        // List
        let addresses = keystore.list().unwrap();
        assert!(addresses.contains(&address));

        // Delete
        keystore.delete(&address).unwrap();
        let loaded = keystore.load(&address).unwrap();
        assert!(loaded.is_none());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
