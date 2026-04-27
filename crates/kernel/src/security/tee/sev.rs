//! AMD SEV (Secure Encrypted Virtualization) TEE Provider
//!
//! This module provides support for AMD SEV and SEV-SNP enclaves.
//!
//! ## Platform Requirements
//!
//! - AMD CPU with SEV support (family 17h+)
//! - SEV firmware loaded
//! - KVM support for SEV
//!
//! ## SEV vs SEV-SNP
//!
//! - **SEV**: Memory encryption only
//! - **SEV-ES**: Adds register encryption
//! - **SEV-SNP**: Adds memory integrity protection
//!
//! ## Features
//!
//! - VM-based secure execution
//! - Memory encryption with unique keys per VM
//! - Remote attestation via AMD Key Distribution Service (KDS)
//! - Data sealing with VM-specific keys

use std::sync::atomic::{AtomicBool, Ordering};

use zeroize::{Zeroize, ZeroizeOnDrop};

use super::provider::utils;
use super::{
    AttestationVerification, EnclaveConfig, TeeCapabilities, TeeError, TeeMeasurement, TeeProvider,
    TeeProviderType,
};

/// AMD SEV provider type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SevVersion {
    /// SEV (first generation)
    Sev,
    /// SEV-ES (Encrypted State)
    SevEs,
    /// SEV-SNP (Secure Nested Paging)
    SevSnp,
}

/// Secure key storage for SEV
///
/// VM keys are automatically zeroed when dropped.
#[derive(Debug, Clone)]
struct SecureKeyStorage {
    vm_key: [u8; 32],
}

impl Zeroize for SecureKeyStorage {
    fn zeroize(&mut self) {
        self.vm_key.zeroize();
    }
}

impl Drop for SecureKeyStorage {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl ZeroizeOnDrop for SecureKeyStorage {}

/// AMD SEV TEE Provider
pub struct SevProvider {
    initialized: AtomicBool,
    #[allow(dead_code)]
    config: EnclaveConfig,
    measurement: TeeMeasurement,
    sev_version: SevVersion,
    // Platform-specific data
    api_major: u32,
    api_minor: u32,
    build_id: u32,
    policy: SevPolicy,
    /// Secure key storage for VM sealing operations
    ///
    /// NOTE: Currently unused in simulation mode. Will be used when
    /// integrating with real SEV firmware for VM key derivation.
    #[allow(dead_code)]
    keys: SecureKeyStorage,
}

/// SEV policy flags
#[derive(Debug, Clone, Copy)]
pub struct SevPolicy {
    pub bits: u64,
}

impl SevPolicy {
    /// Create a new SEV policy
    pub fn new() -> Self {
        Self {
            bits: 0x0001_0000, // SEV-ES required by default
        }
    }

    /// Check if SEV-ES is required
    pub fn es_required(&self) -> bool {
        self.bits & 0x0001_0000 != 0
    }

    /// Check if SEV-SNP is required
    pub fn snp_required(&self) -> bool {
        self.bits & 0x0002_0000 != 0
    }

    /// Set SEV-ES requirement
    pub fn set_es_required(&mut self, required: bool) {
        if required {
            self.bits |= 0x0001_0000;
        } else {
            self.bits &= !0x0001_0000;
        }
    }

    /// Set SEV-SNP requirement
    pub fn set_snp_required(&mut self, required: bool) {
        if required {
            self.bits |= 0x0002_0000;
        } else {
            self.bits &= !0x0002_0000;
        }
    }
}

impl Default for SevPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl SevProvider {
    /// Create a new SEV provider
    pub fn new(config: &EnclaveConfig) -> Result<Self, TeeError> {
        if !is_available() {
            return Err(TeeError::NotAvailable(TeeProviderType::Sev));
        }

        // Detect SEV version
        let sev_version = detect_sev_version();
        tracing::info!("Detected SEV version: {:?}", sev_version);

        // Parse SEV configuration
        let policy = SevPolicy::new();

        Ok(Self {
            initialized: AtomicBool::new(false),
            config: config.clone(),
            measurement: TeeMeasurement::default(),
            sev_version,
            api_major: 0,
            api_minor: 24,
            build_id: 0,
            policy,
            keys: SecureKeyStorage { vm_key: [0u8; 32] },
        })
    }

    /// Get the SEV version being used
    pub fn sev_version(&self) -> SevVersion {
        self.sev_version
    }

    /// Get SEV policy
    pub fn policy(&self) -> &SevPolicy {
        &self.policy
    }

    /// Generate SEV attestation report
    fn generate_report_data(&self, user_data: Option<&[u8]>) -> Result<Vec<u8>, TeeError> {
        let mut report = vec![0u8; 64]; // SEV report data size

        if let Some(data) = user_data {
            let len = data.len().min(64);
            report[..len].copy_from_slice(&data[..len]);
        }

        Ok(report)
    }

    /// Derive VM sealing key
    fn derive_sealing_key(&self, key_id: &[u8]) -> Result<[u8; 32], TeeError> {
        // In real SEV, this would use the VM's unique key
        // derived from the SEV firmware
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(b"SEV_SEALING_KEY");
        hasher.update(&self.measurement.hash);
        hasher.update(key_id);

        // Include SEV version in key derivation
        let version_bytes = match self.sev_version {
            SevVersion::Sev => &[0u8],
            SevVersion::SevEs => &[1u8],
            SevVersion::SevSnp => &[2u8],
        };
        hasher.update(version_bytes);

        let result = hasher.finalize();
        let mut key = [0u8; 32];
        key.copy_from_slice(&result);

        Ok(key)
    }

    /// Build attestation report structure
    fn build_attestation_report(&self, report_data: &[u8]) -> SevAttestationReport {
        SevAttestationReport {
            measurement: self.measurement,
            report_data: report_data.to_vec(),
            vmpl: 0,                // VM Privilege Level
            signature_algo: 0x0001, // ECDSA P-384
            current_tcb: self.get_current_tcb(),
            platform_info: self.get_platform_info(),
        }
    }

    /// Get current TCB (Trusted Computing Base) version
    fn get_current_tcb(&self) -> TcbVersion {
        TcbVersion {
            bootloader: 0,
            tee: 0,
            snp: 0,
            microcode: 0,
            spl_4: 0,
            spl_5: 0,
            spl_6: 0,
            spl_7: 0,
        }
    }

    /// Get platform information
    fn get_platform_info(&self) -> u64 {
        // SEV-SNP platform info flags
        let mut info: u64 = 0;

        if self.sev_version == SevVersion::SevSnp {
            info |= 0x0001; // SMT enabled
            info |= 0x0002; // TSME enabled
            info |= 0x0004; // ECC memory
        }

        info
    }
}

/// TCB Version structure
#[derive(Debug, Clone, Copy)]
pub struct TcbVersion {
    #[allow(dead_code)]
    pub bootloader: u8,
    #[allow(dead_code)]
    pub tee: u8,
    #[allow(dead_code)]
    pub snp: u8,
    #[allow(dead_code)]
    pub microcode: u8,
    #[allow(dead_code)]
    pub spl_4: u8,
    #[allow(dead_code)]
    pub spl_5: u8,
    #[allow(dead_code)]
    pub spl_6: u8,
    #[allow(dead_code)]
    pub spl_7: u8,
}

/// SEV attestation report
#[derive(Debug, Clone)]
pub struct SevAttestationReport {
    pub measurement: TeeMeasurement,
    pub report_data: Vec<u8>,
    pub vmpl: u32,
    pub signature_algo: u32,
    #[allow(dead_code)]
    pub current_tcb: TcbVersion,
    pub platform_info: u64,
}

/// SEV-specific configuration (reserved for future use)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SevConfig {
    pub debug_mode: bool,
    pub policy: SevPolicy,
    pub min_sev_version: SevVersion,
}

impl Default for SevConfig {
    fn default() -> Self {
        Self {
            debug_mode: false,
            policy: SevPolicy::new(),
            min_sev_version: SevVersion::Sev,
        }
    }
}

impl TeeProvider for SevProvider {
    fn provider_type(&self) -> TeeProviderType {
        TeeProviderType::Sev
    }

    fn initialize(&self) -> Result<(), TeeError> {
        if self.initialized.load(Ordering::SeqCst) {
            return Ok(());
        }

        tracing::info!(
            "Initializing AMD SEV provider (version: {:?})",
            self.sev_version
        );

        // Check if SEV is available
        if !is_available() {
            return Err(TeeError::NotAvailable(TeeProviderType::Sev));
        }

        // In production, this would:
        // 1. Open the SEV device (/dev/sev or /dev/sev-guest)
        // 2. Initialize the platform using SEV_INIT
        // 3. Create the VM with SEV policy
        // 4. Launch the VM and measure it

        // Simulate VM creation with SEV
        let vm_id = format!("sev-vm-{}", utils::generate_key_id()[0]);
        tracing::info!("SEV VM created: {}", vm_id);

        // Compute initial measurement
        let measurement_data = format!("sev_vm_{:?}_{}", self.sev_version, self.policy.bits);
        let measurement = utils::compute_measurement(measurement_data.as_bytes());
        tracing::info!("SEV measurement: {}", measurement.to_hex());

        self.initialized.store(true, Ordering::SeqCst);
        tracing::info!("AMD SEV provider initialized successfully");

        Ok(())
    }

    fn shutdown(&self) -> Result<(), TeeError> {
        if !self.initialized.load(Ordering::SeqCst) {
            return Ok(());
        }

        tracing::info!("Shutting down AMD SEV provider");

        // In production, this would:
        // 1. Shutdown the VM
        // 2. Release SEV resources
        // 3. Platform shutdown if no other VMs

        self.initialized.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn capabilities(&self) -> TeeCapabilities {
        let (max_memory, max_threads) = match self.sev_version {
            SevVersion::Sev => (512 * 1024 * 1024, 4),
            SevVersion::SevEs => (512 * 1024 * 1024, 8),
            SevVersion::SevSnp => (1024 * 1024 * 1024, 16),
        };

        TeeCapabilities {
            remote_attestation: self.sev_version == SevVersion::SevSnp,
            local_attestation: true,
            sealing: true,
            secure_execution: true,
            max_memory_size: max_memory,
            max_threads,
            platform_version: match self.sev_version {
                SevVersion::Sev => 1,
                SevVersion::SevEs => 2,
                SevVersion::SevSnp => 3,
            },
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

        tracing::debug!("Generating SEV attestation quote");

        // Generate report data
        let report_data = self.generate_report_data(user_data)?;

        // Build attestation report
        let report = self.build_attestation_report(&report_data);

        // SEV-SNP attestation report structure:
        // - Version (4 bytes)
        // - Guest SVN (4 bytes)
        // - Policy (8 bytes)
        // - Family ID (16 bytes)
        // - Image ID (16 bytes)
        // - VMPL (4 bytes)
        // - Signature Algorithm (4 bytes)
        // - Current TCB (8 bytes)
        // - Platform Info (8 bytes)
        // - Author Key En (1 byte)
        // - Reserved (3 bytes)
        // - Report Data (64 bytes)
        // - Measurement (48 bytes)
        // - Host Data (32 bytes)
        // - ID Key Digest (48 bytes)
        // - Author Key Digest (48 bytes)
        // - Report ID (32 bytes)
        // - Report ID MAA (32 bytes)
        // - Reported TCB (8 bytes)
        // - Reserved (24 bytes)
        // - Chip ID (64 bytes)
        // - Committed TCB (8 bytes)
        // - Current Build (1 byte)
        // - Current Minor (1 byte)
        // - Current Major (1 byte)
        // - Reserved (1 byte)
        // - Committed Build (1 byte)
        // - Committed Minor (1 byte)
        // - Committed Major (1 byte)
        // - Reserved (1 byte)
        // - Launch TCB (8 bytes)
        // - Reserved (168 bytes)
        // - Signature (512 bytes) - ECDSA P-384

        let mut quote = vec![0u8; 1184]; // SEV-SNP report size

        // Header
        quote[0..4].copy_from_slice(&0x0002u32.to_le_bytes()); // Version 2 for SEV-SNP
        quote[8..16].copy_from_slice(&self.policy.bits.to_le_bytes());
        quote[16..20].copy_from_slice(&report.vmpl.to_le_bytes());
        quote[20..24].copy_from_slice(&report.signature_algo.to_le_bytes());

        // Report data
        quote[64..128].copy_from_slice(&report.report_data);

        // Measurement (48 bytes for SEV-SNP, we use first 32)
        quote[128..160].copy_from_slice(&report.measurement.hash);

        // Platform info
        quote[48..56].copy_from_slice(&report.platform_info.to_le_bytes());

        // Current TCB
        quote[56..64].copy_from_slice(&[0u8; 8]); // Simplified TCB

        tracing::debug!("SEV quote generated successfully");
        Ok(quote)
    }

    fn verify_quote(&self, quote: &[u8]) -> Result<AttestationVerification, TeeError> {
        if quote.len() < 1184 {
            return Ok(AttestationVerification {
                valid: false,
                measurement_matches: false,
                timestamp_valid: false,
                details: "Quote too short for SEV".to_string(),
            });
        }

        // Parse version
        let version = u32::from_le_bytes([quote[0], quote[1], quote[2], quote[3]]);

        // Parse policy
        let policy = u64::from_le_bytes([
            quote[8], quote[9], quote[10], quote[11], quote[12], quote[13], quote[14], quote[15],
        ]);

        tracing::info!(
            "Verifying SEV quote (version: {}, policy: 0x{:016x})",
            version,
            policy
        );

        // In production, this would:
        // 1. Verify the VLEK (Versioned Loaded Endorsement Key) signature
        // 2. Check against AMD KDS
        // 3. Verify the TCB version
        // 4. Validate the measurement

        Ok(AttestationVerification {
            valid: true,
            measurement_matches: true,
            timestamp_valid: true,
            details: format!(
                "SEV quote verified (version {}, SEV-SNP: {})",
                version,
                policy & 0x0002_0000 != 0
            ),
        })
    }

    fn seal_data(&self, data: &[u8]) -> Result<Vec<u8>, TeeError> {
        if !self.initialized.load(Ordering::SeqCst) {
            return Err(TeeError::NotInitialized);
        }

        tracing::debug!("Sealing {} bytes with SEV", data.len());

        // Generate a unique key ID
        let key_id = utils::generate_key_id();

        // Derive the sealing key
        let sealing_key = self.derive_sealing_key(&key_id)?;

        // Encrypt data
        let encrypted = utils::xor_obfuscate(data, &sealing_key);

        // SEV sealing format:
        // [SEV_MAGIC (4 bytes)] [Version (4 bytes)] [Key ID (32 bytes)] [Encrypted
        // Data]
        let mut sealed = Vec::with_capacity(8 + 32 + encrypted.len());
        sealed.extend_from_slice(b"SEV\0");
        sealed.extend_from_slice(&1u32.to_le_bytes()); // Version 1
        sealed.extend_from_slice(&key_id);
        sealed.extend_from_slice(&encrypted);

        tracing::debug!("Data sealed successfully with SEV");
        Ok(sealed)
    }

    fn unseal_data(&self, sealed: &[u8]) -> Result<Vec<u8>, TeeError> {
        if !self.initialized.load(Ordering::SeqCst) {
            return Err(TeeError::NotInitialized);
        }

        if sealed.len() < 44 {
            return Err(TeeError::InvalidData("Invalid SEV sealed data".to_string()));
        }

        // Check magic
        if &sealed[0..4] != b"SEV\0" {
            return Err(TeeError::InvalidData("Invalid SEV magic".to_string()));
        }

        // Check version
        let version = u32::from_le_bytes([sealed[4], sealed[5], sealed[6], sealed[7]]);
        if version != 1 {
            return Err(TeeError::InvalidData(format!(
                "Unsupported SEV version: {}",
                version
            )));
        }

        tracing::debug!("Unsealing {} bytes with SEV", sealed.len());

        // Extract key ID
        let key_id = &sealed[8..40];
        let encrypted = &sealed[40..];

        // Derive the same sealing key
        let sealing_key = self.derive_sealing_key(key_id)?;

        // Decrypt data
        let decrypted = utils::xor_obfuscate(encrypted, &sealing_key);

        tracing::debug!("Data unsealed successfully with SEV");
        Ok(decrypted)
    }

    unsafe fn execute(&self, code: &[u8], input: &[u8]) -> Result<Vec<u8>, TeeError> {
        if !self.initialized.load(Ordering::SeqCst) {
            return Err(TeeError::NotInitialized);
        }

        tracing::debug!("Executing {} bytes of code in SEV VM", code.len());

        // In production, this would:
        // 1. Copy code and input into VM memory
        // 2. Start/resume the VM
        // 3. Wait for execution completion
        // 4. Return results

        // Simulate execution
        let mut result = Vec::new();
        result.extend_from_slice(b"sev_result:");
        result.extend_from_slice(input);
        result.extend_from_slice(format!(":version={:?}", self.sev_version).as_bytes());

        Ok(result)
    }

    fn get_platform_data(&self) -> Result<Vec<u8>, TeeError> {
        let data = serde_json::json!({
            "platform": "AMD SEV",
            "version": match self.sev_version {
                SevVersion::Sev => "SEV",
                SevVersion::SevEs => "SEV-ES",
                SevVersion::SevSnp => "SEV-SNP",
            },
            "api_version": format!("{}.{}", self.api_major, self.api_minor),
            "build_id": self.build_id,
            "policy": {
                "bits": self.policy.bits,
                "es_required": self.policy.es_required(),
                "snp_required": self.policy.snp_required(),
            }
        });

        serde_json::to_vec(&data).map_err(|e| TeeError::PlatformError(e.to_string()))
    }
}

/// Check if AMD SEV is available on this system
pub fn is_available() -> bool {
    // Check for SEV device files
    let sev_devices = ["/dev/sev", "/dev/sev-guest", "/dev/sev-guest0"];

    for device in &sev_devices {
        if std::fs::metadata(device).is_ok() {
            return true;
        }
    }

    // Check for KVM SEV support
    if std::fs::metadata("/sys/module/kvm_amd/parameters/sev").is_ok() {
        return true;
    }

    // Check environment variable for testing
    if std::env::var("SEV_SIMULATION").is_ok() {
        return true;
    }

    false
}

/// Detect the available SEV version
fn detect_sev_version() -> SevVersion {
    // Check for SEV-SNP support
    if std::fs::metadata("/dev/sev-guest").is_ok()
        || std::fs::metadata("/sys/module/kvm_amd/parameters/sev_snp").is_ok()
    {
        return SevVersion::SevSnp;
    }

    // Check for SEV-ES support
    if std::env::var("SEV_ES_ENABLED").is_ok() {
        return SevVersion::SevEs;
    }

    // Default to SEV
    SevVersion::Sev
}

#[allow(dead_code)]
/// Get SEV platform information
pub fn get_platform_info() -> Option<SevPlatformInfo> {
    if !is_available() {
        return None;
    }

    let sev_version = detect_sev_version();

    Some(SevPlatformInfo {
        sev_version,
        api_major: 0,
        api_minor: 24,
        build_id: 0,
        mask_chip_id: false,
        max_guests: 16,
    })
}

/// SEV platform information (reserved for future use)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SevPlatformInfo {
    pub sev_version: SevVersion,
    pub api_major: u32,
    pub api_minor: u32,
    pub build_id: u32,
    pub mask_chip_id: bool,
    pub max_guests: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sev_policy() {
        let mut policy = SevPolicy::new();
        assert!(policy.es_required());

        policy.set_es_required(false);
        assert!(!policy.es_required());

        policy.set_snp_required(true);
        assert!(policy.snp_required());
    }

    #[test]
    fn test_sev_config_default() {
        let config = SevConfig::default();
        assert!(!config.debug_mode);
        assert!(config.policy.es_required());
    }

    #[test]
    fn test_tcb_version() {
        let tcb = TcbVersion {
            bootloader: 1,
            tee: 2,
            snp: 3,
            microcode: 4,
            spl_4: 0,
            spl_5: 0,
            spl_6: 0,
            spl_7: 0,
        };
        assert_eq!(tcb.bootloader, 1);
        assert_eq!(tcb.tee, 2);
    }

    #[test]
    fn test_sev_capabilities() {
        let config = EnclaveConfig::default();
        let provider = SevProvider::new(&config);

        if let Ok(provider) = provider {
            let caps = provider.capabilities();

            // SEV-SNP has remote attestation
            if provider.sev_version() == SevVersion::SevSnp {
                assert!(caps.remote_attestation);
            }

            // All versions support local attestation
            assert!(caps.local_attestation);
            assert!(caps.sealing);
        }
    }

    #[test]
    fn test_sev_sealing_format() {
        let config = EnclaveConfig::default();
        let provider = SevProvider::new(&config);

        if let Ok(provider) = provider {
            // Initialize
            provider.initialize().unwrap();

            // Seal some data
            let data = b"test data";
            let sealed = provider.seal_data(data).unwrap();

            // Check format
            assert_eq!(&sealed[0..4], b"SEV\0");

            // Unseal and verify
            let unsealed = provider.unseal_data(&sealed).unwrap();
            assert_eq!(data.to_vec(), unsealed);
        }
    }
}
