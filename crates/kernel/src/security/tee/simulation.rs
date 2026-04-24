//! Simulation TEE Provider
//!
//! This module provides a software-based TEE simulation for development
//! and testing purposes. It mimics TEE behavior without actual hardware
//! security guarantees.
//!
//! ## Warning
//!
//! **This provider is for testing only!** It does not provide any
//! actual security guarantees. All cryptographic operations are
//! simulated and can be easily bypassed.
//!
//! ## Use Cases
//!
//! - Development without TEE hardware
//! - Unit testing
//! - CI/CD pipelines
//! - Educational purposes

use std::sync::atomic::{AtomicBool, Ordering};

use zeroize::{Zeroize, ZeroizeOnDrop};

use super::provider::utils;
use super::{
    AttestationVerification, EnclaveConfig, TeeCapabilities, TeeError, TeeMeasurement, TeeProvider,
    TeeProviderType,
};

/// Secure key storage for TEE keys
///
/// Keys are automatically zeroed when the storage is dropped.
#[derive(Debug, Clone)]
struct SecureKeyStorage {
    signing_key: [u8; 32],
    seal_key: [u8; 32],
}

impl Zeroize for SecureKeyStorage {
    fn zeroize(&mut self) {
        self.signing_key.zeroize();
        self.seal_key.zeroize();
    }
}

impl Drop for SecureKeyStorage {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl ZeroizeOnDrop for SecureKeyStorage {}

/// Software simulation TEE Provider
///
/// **WARNING: This is for testing only!** No security guarantees.
pub struct SimulationProvider {
    initialized: AtomicBool,
    config: EnclaveConfig,
    measurement: TeeMeasurement,
    keys: SecureKeyStorage,
}

impl SimulationProvider {
    /// Create a new simulation provider
    pub fn new(config: &EnclaveConfig) -> Result<Self, TeeError> {
        // Generate deterministic keys based on config
        let signing_key = derive_key(b"SIM_SIGNING_KEY", config);
        let seal_key = derive_key(b"SIM_SEAL_KEY", config);

        tracing::warn!("Creating SIMULATION TEE provider - NOT FOR PRODUCTION USE!");

        Ok(Self {
            initialized: AtomicBool::new(false),
            config: config.clone(),
            measurement: TeeMeasurement::default(),
            keys: SecureKeyStorage {
                signing_key,
                seal_key,
            },
        })
    }

    /// Sign data with the simulation key
    fn sign(&self, data: &[u8]) -> Vec<u8> {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(&self.keys.signing_key);
        hasher.update(data);
        hasher.finalize().to_vec()
    }

    /// Verify signature
    fn verify(&self, data: &[u8], signature: &[u8]) -> bool {
        let expected = self.sign(data);
        expected == signature
    }

    /// Generate a simulated attestation quote
    fn generate_simulated_quote(&self, user_data: Option<&[u8]>) -> Vec<u8> {
        use sha2::{Digest, Sha256};

        // Quote structure:
        // [MAGIC (4 bytes)] [VERSION (4 bytes)] [TIMESTAMP (8 bytes)]
        // [MEASUREMENT (32 bytes)] [USERDATA_HASH (32 bytes)] [SIGNATURE (32 bytes)]

        let mut quote = Vec::with_capacity(112);

        // Magic: "SIM\0"
        quote.extend_from_slice(b"SIM\0");

        // Version
        quote.extend_from_slice(&1u32.to_le_bytes());

        // Timestamp
        let timestamp = current_timestamp();
        quote.extend_from_slice(&timestamp.to_le_bytes());

        // Measurement
        quote.extend_from_slice(&self.measurement.hash);

        // User data hash
        if let Some(data) = user_data {
            let hash = Sha256::digest(data);
            quote.extend_from_slice(&hash);
        } else {
            quote.extend_from_slice(&[0u8; 32]);
        }

        // Signature (over everything except signature itself)
        let signature = self.sign(&quote);
        quote.extend_from_slice(&signature);

        quote
    }

    /// Parse and verify a simulated quote
    fn parse_simulated_quote(&self, quote: &[u8]) -> Result<QuoteData, TeeError> {
        if quote.len() < 112 {
            return Err(TeeError::InvalidData("Quote too short".to_string()));
        }

        // Check magic
        if &quote[0..4] != b"SIM\0" {
            return Err(TeeError::InvalidData("Invalid magic".to_string()));
        }

        // Parse version
        let version = u32::from_le_bytes([quote[4], quote[5], quote[6], quote[7]]);
        if version != 1 {
            return Err(TeeError::InvalidData(format!(
                "Unsupported version: {}",
                version
            )));
        }

        // Parse timestamp
        let timestamp = u64::from_le_bytes([
            quote[8], quote[9], quote[10], quote[11], quote[12], quote[13], quote[14], quote[15],
        ]);

        // Extract measurement
        let mut measurement = [0u8; 32];
        measurement.copy_from_slice(&quote[16..48]);

        // Extract user data hash
        let mut user_data_hash = [0u8; 32];
        user_data_hash.copy_from_slice(&quote[48..80]);

        // Extract and verify signature
        let signature = &quote[80..112];
        let data_to_verify = &quote[0..80];

        if !self.verify(data_to_verify, signature) {
            return Err(TeeError::VerificationFailed(
                "Invalid signature".to_string(),
            ));
        }

        Ok(QuoteData {
            version,
            timestamp,
            measurement: TeeMeasurement::new(measurement),
            user_data_hash,
        })
    }
}

/// Parsed quote data
struct QuoteData {
    #[allow(dead_code)]
    version: u32,
    timestamp: u64,
    measurement: TeeMeasurement,
    #[allow(dead_code)]
    user_data_hash: [u8; 32],
}

/// Derive a deterministic key from seed and config
fn derive_key(seed: &[u8], config: &EnclaveConfig) -> [u8; 32] {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(seed);
    hasher.update(&[config.debug_mode as u8]);

    if let Some(mem) = config.memory_size {
        hasher.update(&mem.to_le_bytes());
    }

    if let Some(threads) = config.thread_count {
        hasher.update(&threads.to_le_bytes());
    }

    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

/// Get current timestamp
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl TeeProvider for SimulationProvider {
    fn provider_type(&self) -> TeeProviderType {
        TeeProviderType::Simulation
    }

    fn initialize(&self) -> Result<(), TeeError> {
        if self.initialized.load(Ordering::SeqCst) {
            return Ok(());
        }

        tracing::info!("Initializing SIMULATION TEE provider");
        tracing::warn!("╔════════════════════════════════════════════════════════════╗");
        tracing::warn!("║  WARNING: Using SIMULATION TEE - NO SECURITY GUARANTEES    ║");
        tracing::warn!("║  This is for development/testing only!                     ║");
        tracing::warn!("╚════════════════════════════════════════════════════════════╝");

        // Simulate measurement computation
        let measurement_data = format!(
            "simulation_enclave_{}_{}",
            self.config.debug_mode,
            self.config.memory_size.unwrap_or(0)
        );
        let measurement = utils::compute_measurement(measurement_data.as_bytes());

        tracing::info!("Simulation measurement: {}", measurement.to_hex());

        self.initialized.store(true, Ordering::SeqCst);
        tracing::info!("Simulation TEE provider initialized");

        Ok(())
    }

    fn shutdown(&self) -> Result<(), TeeError> {
        if !self.initialized.load(Ordering::SeqCst) {
            return Ok(());
        }

        tracing::info!("Shutting down SIMULATION TEE provider");
        self.initialized.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn capabilities(&self) -> TeeCapabilities {
        TeeCapabilities {
            remote_attestation: false, // Simulation doesn't support remote attestation
            local_attestation: true,
            sealing: true,
            secure_execution: false,             // No actual secure execution
            max_memory_size: 1024 * 1024 * 1024, // 1 GB
            max_threads: 8,
            platform_version: 1,
        }
    }

    fn get_measurement(&self) -> Result<TeeMeasurement, TeeError> {
        if !self.initialized.load(Ordering::SeqCst) {
            return Err(TeeError::NotInitialized);
        }

        Ok(self.measurement)
    }

    fn generate_quote(&self, user_data: Option<&[u8]>) -> Result<Vec<u8>, TeeError> {
        if !self.initialized.load(Ordering::SeqCst) {
            return Err(TeeError::NotInitialized);
        }

        tracing::debug!("Generating SIMULATION attestation quote");
        Ok(self.generate_simulated_quote(user_data))
    }

    fn verify_quote(&self, quote: &[u8]) -> Result<AttestationVerification, TeeError> {
        match self.parse_simulated_quote(quote) {
            Ok(quote_data) => {
                let now = current_timestamp();
                let timestamp_valid =
                    now >= quote_data.timestamp && now - quote_data.timestamp < 3600; // 1 hour validity

                let measurement_matches = quote_data.measurement.hash == self.measurement.hash;

                Ok(AttestationVerification {
                    valid: timestamp_valid && measurement_matches,
                    measurement_matches,
                    timestamp_valid,
                    details: format!(
                        "Simulation quote verified (measurement_match={}, timestamp_valid={})",
                        measurement_matches, timestamp_valid
                    ),
                })
            }
            Err(e) => Ok(AttestationVerification {
                valid: false,
                measurement_matches: false,
                timestamp_valid: false,
                details: format!("Verification failed: {}", e),
            }),
        }
    }

    fn seal_data(&self, data: &[u8]) -> Result<Vec<u8>, TeeError> {
        if !self.initialized.load(Ordering::SeqCst) {
            return Err(TeeError::NotInitialized);
        }

        tracing::debug!("Sealing {} bytes (SIMULATION)", data.len());

        // Generate unique key ID
        let key_id = utils::generate_key_id();

        // Derive sealing key from seal_key and key_id
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&self.keys.seal_key);
        hasher.update(&key_id);
        let sealing_key = hasher.finalize();

        // "Encrypt" data (XOR is NOT secure - this is simulation!)
        let encrypted = utils::xor_obfuscate(data, &sealing_key);

        // Format: [SIM_MAGIC (4 bytes)] [KEY_ID (32 bytes)] [ENCRYPTED_DATA]
        let mut sealed = Vec::with_capacity(4 + 32 + encrypted.len());
        sealed.extend_from_slice(b"SIMS"); // SIMulation Seal
        sealed.extend_from_slice(&key_id);
        sealed.extend_from_slice(&encrypted);

        tracing::debug!("Data sealed (SIMULATION)");
        Ok(sealed)
    }

    fn unseal_data(&self, sealed: &[u8]) -> Result<Vec<u8>, TeeError> {
        if !self.initialized.load(Ordering::SeqCst) {
            return Err(TeeError::NotInitialized);
        }

        if sealed.len() < 36 {
            return Err(TeeError::InvalidData("Invalid sealed data".to_string()));
        }

        // Check magic
        if &sealed[0..4] != b"SIMS" {
            return Err(TeeError::InvalidData("Invalid magic".to_string()));
        }

        tracing::debug!("Unsealing {} bytes (SIMULATION)", sealed.len());

        // Extract key ID
        let key_id = &sealed[4..36];
        let encrypted = &sealed[36..];

        // Derive the same sealing key
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&self.keys.seal_key);
        hasher.update(key_id);
        let sealing_key = hasher.finalize();

        // Decrypt
        let decrypted = utils::xor_obfuscate(encrypted, &sealing_key);

        tracing::debug!("Data unsealed (SIMULATION)");
        Ok(decrypted)
    }

    unsafe fn execute(&self, code: &[u8], input: &[u8]) -> Result<Vec<u8>, TeeError> {
        if !self.initialized.load(Ordering::SeqCst) {
            return Err(TeeError::NotInitialized);
        }

        tracing::debug!("Executing {} bytes (SIMULATION)", code.len());
        tracing::warn!("SIMULATION: Code executed without actual isolation!");

        // Just return a simulated result
        let mut result = Vec::new();
        result.extend_from_slice(b"sim_result:");
        result.extend_from_slice(input);
        result.extend_from_slice(format!(":code_len={}", code.len()).as_bytes());

        Ok(result)
    }

    fn get_platform_data(&self) -> Result<Vec<u8>, TeeError> {
        let data = serde_json::json!({
            "platform": "SIMULATION",
            "warning": "NO SECURITY GUARANTEES - FOR TESTING ONLY",
            "version": "1.0",
            "debug_mode": self.config.debug_mode,
            "measurement": hex::encode(self.measurement.hash),
        });

        serde_json::to_vec(&data).map_err(|e| TeeError::PlatformError(e.to_string()))
    }
}

#[allow(dead_code)]
/// Check if simulation is available (always true)
pub fn is_available() -> bool {
    true
}

#[allow(dead_code)]
/// Get simulation info
pub fn get_info() -> SimulationInfo {
    SimulationInfo {
        version: "1.0".to_string(),
        warning: "NO SECURITY GUARANTEES".to_string(),
        is_simulation: true,
    }
}

/// Simulation information (reserved for future use)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SimulationInfo {
    pub version: String,
    pub warning: String,
    pub is_simulation: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simulation_provider_creation() {
        let config = EnclaveConfig::default();
        let provider = SimulationProvider::new(&config);
        assert!(provider.is_ok());
    }

    #[test]
    fn test_simulation_capabilities() {
        let config = EnclaveConfig::default();
        let provider = SimulationProvider::new(&config).unwrap();

        let caps = provider.capabilities();
        assert!(!caps.remote_attestation);
        assert!(caps.local_attestation);
        assert!(caps.sealing);
        assert!(!caps.secure_execution); // Important: simulation has no secure
                                         // execution
    }

    #[test]
    fn test_simulation_initialization() {
        let config = EnclaveConfig::default();
        let provider = SimulationProvider::new(&config).unwrap();

        assert!(!provider.initialized.load(Ordering::SeqCst));
        provider.initialize().unwrap();
        assert!(provider.initialized.load(Ordering::SeqCst));
    }

    #[test]
    fn test_simulation_quote_generation_and_verification() {
        let config = EnclaveConfig::default();
        let provider = SimulationProvider::new(&config).unwrap();
        provider.initialize().unwrap();

        // Generate quote
        let user_data = b"test data";
        let quote = provider.generate_quote(Some(user_data)).unwrap();
        assert!(quote.len() >= 112);

        // Verify quote
        let verification = provider.verify_quote(&quote).unwrap();
        assert!(verification.valid);
        assert!(verification.measurement_matches);
    }

    #[test]
    fn test_simulation_sealing() {
        let config = EnclaveConfig::default();
        let provider = SimulationProvider::new(&config).unwrap();
        provider.initialize().unwrap();

        // Seal data
        let data = b"secret data";
        let sealed = provider.seal_data(data).unwrap();

        // Check format
        assert_eq!(&sealed[0..4], b"SIMS");

        // Unseal and verify
        let unsealed = provider.unseal_data(&sealed).unwrap();
        assert_eq!(data.to_vec(), unsealed);
    }

    #[test]
    fn test_simulation_sealing_invalid_magic() {
        let config = EnclaveConfig::default();
        let provider = SimulationProvider::new(&config).unwrap();
        provider.initialize().unwrap();

        // Create invalid sealed data
        let mut invalid = b"INVALID".to_vec();
        invalid.extend_from_slice(&[0u8; 100]);

        let result = provider.unseal_data(&invalid);
        assert!(result.is_err());
    }

    #[test]
    fn test_simulation_quote_tampering() {
        let config = EnclaveConfig::default();
        let provider = SimulationProvider::new(&config).unwrap();
        provider.initialize().unwrap();

        // Generate quote
        let quote = provider.generate_quote(None).unwrap();

        // Tamper with quote
        let mut tampered = quote.clone();
        tampered[50] ^= 0xFF; // Flip bits in measurement area

        // Verification should fail
        let result = provider.verify_quote(&tampered);
        assert!(result.is_err() || !result.unwrap().valid);
    }

    #[test]
    fn test_simulation_info() {
        let info = get_info();
        assert!(info.is_simulation);
        assert!(!info.warning.is_empty());
    }

    #[test]
    fn test_simulation_is_always_available() {
        assert!(is_available());
    }
}
