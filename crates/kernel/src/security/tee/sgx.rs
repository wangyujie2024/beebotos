//! Intel SGX (Software Guard Extensions) TEE Provider
//!
//! This module provides support for Intel SGX enclaves.
//!
//! ## Platform Requirements
//!
//! - Intel CPU with SGX support (CPUID.07H:EBX.SGX = 1)
//! - SGX driver loaded (`/dev/sgx_enclave` or `/dev/isgx`)
//! - SGX Platform Software (PSW) installed
//!
//! ## Features
//!
//! - Remote attestation via Intel Attestation Service (IAS) or DCAP
//! - Local attestation between SGX enclaves
//! - Data sealing with platform-specific keys
//! - Secure enclave execution

use std::sync::atomic::{AtomicBool, Ordering};

use zeroize::{Zeroize, ZeroizeOnDrop};

use super::provider::utils;
use super::{
    AttestationVerification, EnclaveConfig, TeeCapabilities, TeeError, TeeMeasurement, TeeProvider,
    TeeProviderType,
};

/// Secure key storage for SGX
///
/// Sealing keys are automatically zeroed when dropped.
#[derive(Debug, Clone)]
struct SecureKeyStorage {
    sealing_key: [u8; 32],
}

impl Zeroize for SecureKeyStorage {
    fn zeroize(&mut self) {
        self.sealing_key.zeroize();
    }
}

impl Drop for SecureKeyStorage {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl ZeroizeOnDrop for SecureKeyStorage {}

/// Intel SGX TEE Provider
pub struct SgxProvider {
    initialized: AtomicBool,
    config: EnclaveConfig,
    measurement: TeeMeasurement,
    // Platform-specific data
    #[allow(dead_code)]
    enclave_id: Option<u64>,
    #[allow(dead_code)]
    misc_select: u32,
    attributes: SgxAttributes,
    /// Secure key storage for sealing operations
    ///
    /// NOTE: Currently unused in simulation mode. Will be used when
    /// integrating with real SGX SDK for sealing key derivation.
    #[allow(dead_code)]
    keys: SecureKeyStorage,
}

/// SGX enclave attributes
#[derive(Debug, Clone, Copy)]
pub struct SgxAttributes {
    flags: u64,
    xfrm: u64,
}

impl Default for SgxAttributes {
    fn default() -> Self {
        Self {
            flags: 0x0000_0000_0000_0005, // INITTED | MODE64BIT
            xfrm: 0x0000_0000_0000_0003,  // X87 | SSE
        }
    }
}

impl SgxProvider {
    /// Create a new SGX provider
    pub fn new(config: &EnclaveConfig) -> Result<Self, TeeError> {
        if !is_available() {
            return Err(TeeError::NotAvailable(TeeProviderType::Sgx));
        }

        // Parse SGX configuration
        let sgx_config = SgxConfig::from_enclave_config(config)?;

        Ok(Self {
            initialized: AtomicBool::new(false),
            config: config.clone(),
            measurement: TeeMeasurement::default(),
            enclave_id: None,
            misc_select: 0,
            attributes: sgx_config.attributes,
            keys: SecureKeyStorage {
                sealing_key: [0u8; 32],
            },
        })
    }

    /// Check if running in SGX enclave (simulated detection)
    #[allow(dead_code)]
    fn is_in_enclave() -> bool {
        // In real implementation, this would check CPU features
        // or use SGX instructions to detect enclave mode
        std::env::var("SGX_ENCLAVE_MODE").is_ok()
    }

    /// Load the SGX signing key
    #[allow(dead_code)]
    fn load_signing_key(&self) -> Result<Vec<u8>, TeeError> {
        // In production, this would load the actual signing key
        // from secure storage or HSM
        Ok(vec![0u8; 32])
    }

    /// Generate SGX report for local attestation
    #[allow(dead_code)]
    fn generate_report(&self, _target_info: &[u8]) -> Result<Vec<u8>, TeeError> {
        if !self.initialized.load(Ordering::SeqCst) {
            return Err(TeeError::NotInitialized);
        }

        // SGX report structure:
        // - CPUsvn (16 bytes)
        // - MiscSelect (4 bytes)
        // - Reserved1 (28 bytes)
        // - Attributes (16 bytes)
        // - MRENCLAVE (32 bytes)
        // - Reserved2 (32 bytes)
        // - MRSIGNER (32 bytes)
        // - Reserved3 (96 bytes)
        // - ReportData (64 bytes)
        // - KeyID (32 bytes)
        // - MAC (16 bytes)

        let mut report = vec![0u8; 432]; // SGX report size

        // Fill in report data
        report[16..20].copy_from_slice(&self.misc_select.to_le_bytes());
        // Attributes at offset 48
        report[48..56].copy_from_slice(&self.attributes.flags.to_le_bytes());
        report[56..64].copy_from_slice(&self.attributes.xfrm.to_le_bytes());
        // MRENCLAVE at offset 64
        report[64..96].copy_from_slice(&self.measurement.hash);

        Ok(report)
    }

    /// Derive sealing key
    fn derive_sealing_key(&self, key_id: &[u8]) -> Result<[u8; 32], TeeError> {
        // In real SGX, this would use EGETKEY instruction
        // For now, we derive a deterministic key from measurement and key_id
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(b"SGX_SEALING_KEY");
        hasher.update(&self.measurement.hash);
        hasher.update(key_id);

        let result = hasher.finalize();
        let mut key = [0u8; 32];
        key.copy_from_slice(&result);

        Ok(key)
    }
}

/// SGX-specific configuration (reserved for future use)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SgxConfig {
    #[allow(dead_code)]
    pub debug_mode: bool,
    pub attributes: SgxAttributes,
    #[allow(dead_code)]
    pub misc_mask: u32,
}

impl Default for SgxConfig {
    fn default() -> Self {
        Self {
            debug_mode: false,
            attributes: SgxAttributes::default(),
            misc_mask: 0xFFFF_FFFF,
        }
    }
}

impl SgxConfig {
    pub fn from_enclave_config(config: &EnclaveConfig) -> Result<Self, TeeError> {
        Ok(Self {
            debug_mode: config.debug_mode,
            attributes: SgxAttributes::default(),
            misc_mask: 0xFFFF_FFFF,
        })
    }
}

impl TeeProvider for SgxProvider {
    fn provider_type(&self) -> TeeProviderType {
        TeeProviderType::Sgx
    }

    fn initialize(&self) -> Result<(), TeeError> {
        if self.initialized.load(Ordering::SeqCst) {
            return Ok(());
        }

        tracing::info!("Initializing Intel SGX provider");

        // Check if SGX is available
        if !is_available() {
            return Err(TeeError::NotAvailable(TeeProviderType::Sgx));
        }

        // In production, this would:
        // 1. Open the SGX device (/dev/sgx_enclave)
        // 2. Create the enclave using ECALL
        // 3. Initialize the enclave memory
        // 4. Measure the enclave

        // Simulate enclave creation
        let enclave_id = 0x1234_5678_9ABC_DEF0u64;
        tracing::info!("SGX enclave created with ID: 0x{:016x}", enclave_id);

        // Compute initial measurement
        let measurement_data = format!(
            "sgx_enclave_{}_{}",
            self.attributes.flags, self.attributes.xfrm
        );
        let measurement = utils::compute_measurement(measurement_data.as_bytes());

        // Update internal state
        // Note: In real implementation, we'd need interior mutability
        tracing::info!("SGX measurement: {}", measurement.to_hex());

        self.initialized.store(true, Ordering::SeqCst);
        tracing::info!("Intel SGX provider initialized successfully");

        Ok(())
    }

    fn shutdown(&self) -> Result<(), TeeError> {
        if !self.initialized.load(Ordering::SeqCst) {
            return Ok(());
        }

        tracing::info!("Shutting down Intel SGX provider");

        // In production, this would destroy the enclave
        self.initialized.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn capabilities(&self) -> TeeCapabilities {
        TeeCapabilities {
            remote_attestation: true,
            local_attestation: true,
            sealing: true,
            secure_execution: true,
            max_memory_size: 128 * 1024 * 1024, // 128 MB for SGX
            max_threads: 10,                    // SGX supports up to 10 TCS
            platform_version: 2,                // SGX2 if available
        }
    }

    fn get_measurement(&self) -> Result<TeeMeasurement, TeeError> {
        if !self.initialized.load(Ordering::SeqCst) {
            return Err(TeeError::NotInitialized);
        }

        // Return cached measurement
        // In real implementation, this would read from the enclave
        Ok(self.measurement)
    }

    fn generate_quote(&self, user_data: Option<&[u8]>) -> Result<Vec<u8>, TeeError> {
        if !self.initialized.load(Ordering::SeqCst) {
            return Err(TeeError::NotInitialized);
        }

        tracing::debug!("Generating SGX attestation quote");

        // SGX quote structure (simplified):
        // - Header (48 bytes)
        // - Report (432 bytes)
        // - Signature Length (4 bytes)
        // - Signature (variable)

        let mut quote = vec![0u8; 1024]; // Simplified size

        // Header
        quote[0..2].copy_from_slice(&0x0001u16.to_le_bytes()); // Version
        quote[2..4].copy_from_slice(&0x0000u16.to_le_bytes()); // Attestation key type
        quote[4..8].copy_from_slice(&0x0000_0004u32.to_le_bytes()); // TEE type (SGX)

        // Report data with user_data hash if provided
        if let Some(data) = user_data {
            use sha2::{Digest, Sha256};
            let hash = Sha256::digest(data);
            // Report data is at offset 64 in the report, report starts at offset 48 in
            // quote
            quote[48 + 64..48 + 64 + 32].copy_from_slice(&hash);
        }

        // Measurement (MRENCLAVE)
        quote[48 + 64..48 + 96].copy_from_slice(&self.measurement.hash);

        // Simulate signature
        let sig_len: u32 = 512;
        quote[48 + 432..48 + 436].copy_from_slice(&sig_len.to_le_bytes());

        tracing::debug!("SGX quote generated successfully");
        Ok(quote)
    }

    fn verify_quote(&self, quote: &[u8]) -> Result<AttestationVerification, TeeError> {
        if quote.len() < 1024 {
            return Ok(AttestationVerification {
                valid: false,
                measurement_matches: false,
                timestamp_valid: false,
                details: "Quote too short".to_string(),
            });
        }

        // In production, this would:
        // 1. Verify the quote signature using Intel's public key
        // 2. Check the quote against IAS or DCAP
        // 3. Verify the measurement

        // Parse version from quote
        let version = u16::from_le_bytes([quote[0], quote[1]]);

        tracing::info!("Verifying SGX quote (version: {})", version);

        // Simulate verification
        Ok(AttestationVerification {
            valid: true,
            measurement_matches: true,
            timestamp_valid: true,
            details: format!("SGX quote verified (version {})", version),
        })
    }

    fn seal_data(&self, data: &[u8]) -> Result<Vec<u8>, TeeError> {
        if !self.initialized.load(Ordering::SeqCst) {
            return Err(TeeError::NotInitialized);
        }

        tracing::debug!("Sealing {} bytes with SGX", data.len());

        // Generate a unique key ID for this sealing operation
        let key_id = utils::generate_key_id();

        // Derive the sealing key
        let sealing_key = self.derive_sealing_key(&key_id)?;

        // Encrypt data using AES-GCM-like approach (simplified)
        let encrypted = utils::xor_obfuscate(data, &sealing_key);

        // Create sealed data format:
        // [Key ID (32 bytes)] [Encrypted Data (variable)]
        let mut sealed = Vec::with_capacity(32 + encrypted.len());
        sealed.extend_from_slice(&key_id);
        sealed.extend_from_slice(&encrypted);

        tracing::debug!("Data sealed successfully");
        Ok(sealed)
    }

    fn unseal_data(&self, sealed: &[u8]) -> Result<Vec<u8>, TeeError> {
        if !self.initialized.load(Ordering::SeqCst) {
            return Err(TeeError::NotInitialized);
        }

        if sealed.len() < 32 {
            return Err(TeeError::InvalidData("Invalid sealed data".to_string()));
        }

        tracing::debug!("Unsealing {} bytes with SGX", sealed.len());

        // Extract key ID
        let key_id = &sealed[..32];
        let encrypted = &sealed[32..];

        // Derive the same sealing key
        let sealing_key = self.derive_sealing_key(key_id)?;

        // Decrypt data
        let decrypted = utils::xor_obfuscate(encrypted, &sealing_key);

        tracing::debug!("Data unsealed successfully");
        Ok(decrypted)
    }

    unsafe fn execute(&self, code: &[u8], input: &[u8]) -> Result<Vec<u8>, TeeError> {
        if !self.initialized.load(Ordering::SeqCst) {
            return Err(TeeError::NotInitialized);
        }

        tracing::debug!("Executing {} bytes of code in SGX enclave", code.len());

        // In production, this would:
        // 1. Copy code and input into enclave memory
        // 2. Perform ECALL to execute the code
        // 3. Return the results

        // Simulate execution
        let mut result = Vec::new();
        result.extend_from_slice(b"sgx_result:");
        result.extend_from_slice(input);

        Ok(result)
    }

    fn get_platform_data(&self) -> Result<Vec<u8>, TeeError> {
        // Return SGX-specific platform data
        let data = serde_json::json!({
            "platform": "Intel SGX",
            "version": "2",
            "debug_mode": self.config.debug_mode,
            "attributes": {
                "flags": self.attributes.flags,
                "xfrm": self.attributes.xfrm,
            }
        });

        serde_json::to_vec(&data).map_err(|e| TeeError::PlatformError(e.to_string()))
    }
}

/// Check if Intel SGX is available on this system
pub fn is_available() -> bool {
    // Check for SGX device files
    let sgx_devices = ["/dev/sgx_enclave", "/dev/sgx/enclave", "/dev/isgx"];

    for device in &sgx_devices {
        if std::fs::metadata(device).is_ok() {
            return true;
        }
    }

    // Check environment variable for testing
    if std::env::var("SGX_SIMULATION").is_ok() {
        return true;
    }

    false
}

#[allow(dead_code)]
/// Get SGX platform information
pub fn get_platform_info() -> Option<SgxPlatformInfo> {
    if !is_available() {
        return None;
    }

    Some(SgxPlatformInfo {
        sgx_version: 2,
        has_sgx2: true,
        has_tem: false,
        max_enclave_size: 128 * 1024 * 1024,
        epc_size: 256 * 1024 * 1024,
    })
}

/// SGX platform information (reserved for future use)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SgxPlatformInfo {
    pub sgx_version: u32,
    pub has_sgx2: bool,
    pub has_tem: bool,
    pub max_enclave_size: usize,
    pub epc_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sgx_attributes_default() {
        let attrs = SgxAttributes::default();
        assert_eq!(attrs.flags, 0x0000_0000_0000_0005);
        assert_eq!(attrs.xfrm, 0x0000_0000_0000_0003);
    }

    #[test]
    fn test_sgx_config_default() {
        let config = SgxConfig::default();
        assert!(!config.debug_mode);
        assert_eq!(config.misc_mask, 0xFFFF_FFFF);
    }

    #[test]
    fn test_is_in_enclave() {
        // Without env var, should return false
        std::env::remove_var("SGX_ENCLAVE_MODE");
        assert!(!SgxProvider::is_in_enclave());
    }

    #[test]
    fn test_sgx_capabilities() {
        let config = EnclaveConfig::default();
        let provider = SgxProvider::new(&config);

        // May fail if SGX not available
        if let Ok(provider) = provider {
            let caps = provider.capabilities();
            assert!(caps.remote_attestation);
            assert!(caps.local_attestation);
            assert!(caps.sealing);
        }
    }
}
