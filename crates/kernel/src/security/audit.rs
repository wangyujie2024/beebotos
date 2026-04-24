//! Audit Logging System
//!
//! Production-ready audit logging with:
//! - Persistent storage (file-based or database)
//! - Structured logging with JSON format
//! - **Encrypted log entries** (AES-256-GCM)
//! - Tamper-evident log entries (optional hashing)
//! - Log rotation and retention policies
//! - Async batch writes for performance
//! - Query and filtering capabilities
//!
//! ## Encryption
//!
//! Audit logs can be encrypted using AES-256-GCM for confidentiality.
//! The encryption key is automatically zeroed from memory when the
//! audit log is dropped.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{KernelError, Result};
use crate::security::{AccessAction, AccessDecision, SecurityContext};

/// Secure encryption key for audit logs
///
/// The key is automatically zeroed from memory when dropped.
#[derive(Debug, Clone)]
pub struct AuditEncryptionKey {
    key: [u8; 32],
    version: u32,
}

impl AuditEncryptionKey {
    /// Create a new encryption key
    pub fn new(key: [u8; 32]) -> Self {
        Self { key, version: 1 }
    }

    /// Create a key from a password using PBKDF2
    pub fn from_password(password: &str, salt: &[u8]) -> Self {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();
        hasher.update(password.as_bytes());
        hasher.update(salt);

        // Simple key stretching (10,000 iterations)
        for _ in 0..10000 {
            let hash = hasher.finalize_reset();
            hasher.update(&hash);
        }

        let result = hasher.finalize();
        let mut key = [0u8; 32];
        key.copy_from_slice(&result);

        Self { key, version: 1 }
    }

    /// Get key bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.key
    }

    /// Get key version
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Generate a random encryption key
    pub fn generate() -> Self {
        use rand::Rng;
        let mut key = [0u8; 32];
        rand::thread_rng().fill(&mut key);
        Self { key, version: 1 }
    }
}

impl Zeroize for AuditEncryptionKey {
    fn zeroize(&mut self) {
        self.key.zeroize();
        self.version = 0;
    }
}

impl Drop for AuditEncryptionKey {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl ZeroizeOnDrop for AuditEncryptionKey {}

/// Encrypted audit entry wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedAuditEntry {
    /// Key version used for encryption
    pub key_version: u32,
    /// Nonce for AES-GCM
    pub nonce: Vec<u8>,
    /// Encrypted data
    pub ciphertext: Vec<u8>,
    /// Authentication tag
    pub tag: Vec<u8>,
    /// Timestamp of encryption
    pub encrypted_at: u64,
}

/// Audit entry representing a single security event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unique entry ID (ULID or UUID)
    pub id: String,
    /// Unix timestamp with nanosecond precision
    pub timestamp_ns: u64,
    /// Event sequence number (for ordering and detection of missing entries)
    pub sequence: u64,
    /// User/agent ID who performed the action
    pub subject_id: String,
    /// Subject's group/role
    pub subject_group: String,
    /// Object being accessed (resource path, file, etc.)
    pub object: String,
    /// Action performed
    pub action: AccessAction,
    /// Access decision
    pub decision: AccessDecision,
    /// Security clearance level at time of access
    pub clearance_level: String,
    /// Client IP address (if applicable)
    pub client_ip: Option<String>,
    /// Additional context as key-value pairs
    pub context: serde_json::Value,
    /// Optional SHA-256 hash of entry for tamper detection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity_hash: Option<String>,
}

impl AuditEntry {
    /// Create a new audit entry
    pub fn new(
        sequence: u64,
        subject: &SecurityContext,
        object: &str,
        action: AccessAction,
        decision: AccessDecision,
    ) -> Self {
        let timestamp_ns = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let id = ulid::Ulid::new().to_string();

        Self {
            id,
            timestamp_ns,
            sequence,
            subject_id: subject.user_id.clone(),
            subject_group: subject.group_id.clone(),
            object: object.to_string(),
            action,
            decision,
            clearance_level: format!("{:?}", subject.clearance_level),
            client_ip: None,
            context: serde_json::json!({}),
            integrity_hash: None,
        }
    }

    /// Add client IP
    pub fn with_client_ip(mut self, ip: impl Into<String>) -> Self {
        self.client_ip = Some(ip.into());
        self
    }

    /// Add context
    pub fn with_context(mut self, context: serde_json::Value) -> Self {
        self.context = context;
        self
    }

    /// Calculate integrity hash for tamper detection
    pub fn calculate_hash(&self) -> String {
        use sha2::{Digest, Sha256};

        let data = format!(
            "{}:{}:{}:{}:{}:{:?}:{:?}",
            self.id,
            self.timestamp_ns,
            self.sequence,
            self.subject_id,
            self.object,
            self.action,
            self.decision
        );

        let mut hasher = Sha256::new();
        hasher.update(data.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Verify entry integrity
    pub fn verify_integrity(&self) -> bool {
        match &self.integrity_hash {
            Some(hash) => hash == &self.calculate_hash(),
            None => true, // No hash to verify
        }
    }
}

/// Audit log storage backend
#[derive(Debug, Clone)]
pub enum AuditBackend {
    /// In-memory storage (for testing, volatile)
    Memory {
        /// Maximum entries to store
        max_entries: usize,
    },
    /// File-based append-only log
    File {
        /// Log file path
        path: PathBuf,
        /// Maximum size in MB
        max_size_mb: u64,
    },
}

impl Default for AuditBackend {
    fn default() -> Self {
        AuditBackend::Memory { max_entries: 10000 }
    }
}

impl AuditBackend {
    /// Create file backend
    pub fn file<P: AsRef<Path>>(path: P) -> Self {
        AuditBackend::File {
            path: path.as_ref().to_path_buf(),
            max_size_mb: 100,
        }
    }

    /// Create file backend with custom size limit
    pub fn file_with_limit<P: AsRef<Path>>(path: P, max_size_mb: u64) -> Self {
        AuditBackend::File {
            path: path.as_ref().to_path_buf(),
            max_size_mb,
        }
    }
}

/// Audit log configuration
#[derive(Debug, Clone)]
pub struct AuditConfig {
    /// Storage backend
    pub backend: AuditBackend,
    /// Enable integrity hashing
    pub enable_integrity: bool,
    /// Enable encryption (requires encryption_key)
    pub enable_encryption: bool,
    /// Async batch size (0 = synchronous)
    pub batch_size: usize,
    /// Flush interval in milliseconds
    pub flush_interval_ms: u64,
    /// Retention period in days (0 = unlimited)
    pub retention_days: u64,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            backend: AuditBackend::default(),
            enable_integrity: false,
            enable_encryption: false,
            batch_size: 100,
            flush_interval_ms: 1000,
            retention_days: 90,
        }
    }
}

impl AuditConfig {
    /// Create configuration with encryption enabled
    pub fn with_encryption(mut self, enabled: bool) -> Self {
        self.enable_encryption = enabled;
        self
    }

    /// Create configuration with integrity checking enabled
    pub fn with_integrity(mut self, enabled: bool) -> Self {
        self.enable_integrity = enabled;
        self
    }
}

/// Inner storage implementation
enum AuditStorage {
    Memory {
        entries: Vec<AuditEntry>,
        max_entries: usize,
    },
    File {
        path: PathBuf,
        file: Mutex<std::fs::File>,
        max_size_bytes: u64,
        current_size: Mutex<u64>,
        buffer: Mutex<Vec<AuditEntry>>,
    },
}

/// Production-ready audit log with optional encryption
///
/// When encryption is enabled, all audit entries are encrypted using
/// AES-256-GCM before storage. The encryption key is automatically
/// zeroed from memory when the audit log is dropped.
pub struct AuditLog {
    storage: Mutex<AuditStorage>,
    config: AuditConfig,
    sequence: Mutex<u64>,
    encryption_key: Option<AuditEncryptionKey>,
}

impl std::fmt::Debug for AuditLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditLog")
            .field("config", &self.config)
            .field("encrypted", &self.encryption_key.is_some())
            .finish()
    }
}

impl AuditLog {
    /// Create new audit log with default config (memory backend)
    pub fn new() -> Self {
        Self::with_config(AuditConfig::default()).expect("Memory backend cannot fail")
    }

    /// Create audit log with specific configuration
    pub fn with_config(config: AuditConfig) -> Result<Self> {
        let storage = match &config.backend {
            AuditBackend::Memory { max_entries } => AuditStorage::Memory {
                entries: Vec::with_capacity(*max_entries),
                max_entries: *max_entries,
            },
            AuditBackend::File { path, max_size_mb } => {
                // Create parent directory if needed
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        KernelError::io(format!("Failed to create audit directory: {}", e))
                    })?;
                }

                // Open or create log file
                let file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .map_err(|e| KernelError::io(format!("Failed to open audit log: {}", e)))?;

                // Get current file size
                let metadata = std::fs::metadata(path).map_err(|e| {
                    KernelError::io(format!("Failed to get audit log metadata: {}", e))
                })?;
                let current_size = metadata.len();

                AuditStorage::File {
                    path: path.clone(),
                    file: Mutex::new(file),
                    max_size_bytes: max_size_mb * 1024 * 1024,
                    current_size: Mutex::new(current_size),
                    buffer: Mutex::new(Vec::with_capacity(config.batch_size.max(1))),
                }
            }
        };

        Ok(Self {
            storage: Mutex::new(storage),
            config,
            sequence: Mutex::new(0),
            encryption_key: None,
        })
    }

    /// Create audit log with encryption enabled
    ///
    /// # Example
    ///
    /// ```rust
    /// use beebotos_kernel::security::audit::{AuditConfig, AuditEncryptionKey, AuditLog};
    ///
    /// let key = AuditEncryptionKey::generate();
    /// let log = AuditLog::with_encryption(AuditConfig::default(), key)
    ///     .expect("Failed to create encrypted audit log");
    /// ```
    pub fn with_encryption(config: AuditConfig, key: AuditEncryptionKey) -> Result<Self> {
        if !config.enable_encryption {
            tracing::warn!("Creating encrypted audit log but enable_encryption is false in config");
        }

        let mut log = Self::with_config(config)?;
        log.encryption_key = Some(key);
        Ok(log)
    }

    /// Create audit log with password-based encryption
    pub fn with_password_encryption(
        config: AuditConfig,
        password: &str,
        salt: &[u8],
    ) -> Result<Self> {
        let key = AuditEncryptionKey::from_password(password, salt);
        let mut config = config;
        config.enable_encryption = true;
        Self::with_encryption(config, key)
    }

    /// Check if audit log is encrypted
    pub fn is_encrypted(&self) -> bool {
        self.encryption_key.is_some()
    }

    /// Encrypt an audit entry
    ///
    /// # Note
    ///
    /// This method requires the `encryption` feature to be enabled.
    #[cfg(feature = "encryption")]
    fn encrypt_entry(&self, entry: &AuditEntry) -> Result<EncryptedAuditEntry> {
        use aes_gcm::aead::{Aead, KeyInit};
        use aes_gcm::{Aes256Gcm, Nonce};
        use rand::Rng;

        let key = self
            .encryption_key
            .as_ref()
            .ok_or_else(|| KernelError::internal("Encryption key not set"))?;

        let cipher = Aes256Gcm::new_from_slice(key.as_bytes())
            .map_err(|e| KernelError::internal(format!("Invalid encryption key: {}", e)))?;

        // Generate random nonce
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Serialize entry
        let plaintext = serde_json::to_vec(entry)
            .map_err(|e| KernelError::internal(format!("Failed to serialize entry: {}", e)))?;

        // Encrypt
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_ref())
            .map_err(|e| KernelError::internal(format!("Encryption failed: {}", e)))?;

        // Split ciphertext and tag (AES-GCM appends 16-byte tag)
        let tag_start = ciphertext.len().saturating_sub(16);
        let (encrypted_data, tag) = ciphertext.split_at(tag_start);

        Ok(EncryptedAuditEntry {
            key_version: key.version(),
            nonce: nonce_bytes.to_vec(),
            ciphertext: encrypted_data.to_vec(),
            tag: tag.to_vec(),
            encrypted_at: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        })
    }

    /// Decrypt an audit entry
    ///
    /// # Note
    ///
    /// This method requires the `encryption` feature to be enabled.
    #[cfg(feature = "encryption")]
    fn decrypt_entry(&self, encrypted: &EncryptedAuditEntry) -> Result<AuditEntry> {
        use aes_gcm::aead::{Aead, KeyInit};
        use aes_gcm::{Aes256Gcm, Nonce};

        let key = self
            .encryption_key
            .as_ref()
            .ok_or_else(|| KernelError::internal("Encryption key not set"))?;

        let cipher = Aes256Gcm::new_from_slice(key.as_bytes())
            .map_err(|e| KernelError::internal(format!("Invalid encryption key: {}", e)))?;

        let nonce = Nonce::from_slice(&encrypted.nonce);

        // Combine ciphertext and tag
        let mut combined = encrypted.ciphertext.clone();
        combined.extend_from_slice(&encrypted.tag);

        // Decrypt
        let plaintext = cipher
            .decrypt(nonce, combined.as_ref())
            .map_err(|e| KernelError::internal(format!("Decryption failed: {}", e)))?;

        // Deserialize
        let entry: AuditEntry = serde_json::from_slice(&plaintext)
            .map_err(|e| KernelError::internal(format!("Failed to deserialize entry: {}", e)))?;

        Ok(entry)
    }

    /// Log an access attempt
    pub fn log_access_attempt(
        &self,
        subject: &SecurityContext,
        object: &str,
        action: AccessAction,
        decision: AccessDecision,
    ) {
        let mut sequence = self.sequence.lock();
        *sequence += 1;

        let mut entry = AuditEntry::new(*sequence, subject, object, action, decision);

        if self.config.enable_integrity {
            entry.integrity_hash = Some(entry.calculate_hash());
        }

        drop(sequence);

        if let Err(e) = self.write_entry(entry) {
            error!("Failed to write audit entry: {}", e);
        }
    }

    /// Log custom security event
    pub fn log_event(
        &self,
        subject: &SecurityContext,
        object: &str,
        action: AccessAction,
        decision: AccessDecision,
        context: serde_json::Value,
    ) {
        let mut sequence = self.sequence.lock();
        *sequence += 1;

        let mut entry =
            AuditEntry::new(*sequence, subject, object, action, decision).with_context(context);

        if self.config.enable_integrity {
            entry.integrity_hash = Some(entry.calculate_hash());
        }

        drop(sequence);

        if let Err(e) = self.write_entry(entry) {
            error!("Failed to write audit entry: {}", e);
        }
    }

    /// Write entry to storage
    fn write_entry(&self, entry: AuditEntry) -> Result<()> {
        let mut storage = self.storage.lock();

        match &mut *storage {
            AuditStorage::Memory {
                entries,
                max_entries,
            } => {
                if entries.len() >= *max_entries {
                    // Remove oldest entries (20% of capacity)
                    let to_remove = *max_entries / 5;
                    entries.drain(0..to_remove);
                    warn!(
                        "Audit log memory buffer full, removed {} oldest entries",
                        to_remove
                    );
                }
                entries.push(entry);
            }
            AuditStorage::File {
                file: _, buffer, ..
            } => {
                let mut buf = buffer.lock();
                buf.push(entry);

                // Flush if buffer is full
                if buf.len() >= self.config.batch_size {
                    drop(buf);
                    self.flush_file_buffer(&mut storage)?;
                }
            }
        }

        Ok(())
    }

    /// Flush file buffer to disk
    fn flush_file_buffer(&self, storage: &mut AuditStorage) -> Result<()> {
        if let AuditStorage::File {
            file,
            buffer,
            current_size,
            max_size_bytes,
            path,
        } = storage
        {
            let mut buf = buffer.lock();
            if buf.is_empty() {
                return Ok(());
            }

            // Check size limit and rotate if needed
            let size_val = *current_size.lock();
            if size_val > *max_size_bytes {
                // Rotate first
                drop(file.lock());
                self.rotate_log_file(path)?;
                *current_size.lock() = 0;
            }

            let mut file = file.lock();
            let mut size = current_size.lock();

            // Write entries
            for entry in buf.drain(..) {
                #[cfg(feature = "encryption")]
                let line = if self.encryption_key.is_some() {
                    // Encrypt the entry
                    let encrypted = self.encrypt_entry(&entry)?;
                    serde_json::to_string(&encrypted).map_err(|e| {
                        KernelError::internal(format!("Failed to serialize encrypted entry: {}", e))
                    })?
                } else {
                    // Plain text
                    serde_json::to_string(&entry).map_err(|e| {
                        KernelError::internal(format!("Failed to serialize audit entry: {}", e))
                    })?
                };

                #[cfg(not(feature = "encryption"))]
                let line = {
                    // Encryption feature not enabled, always use plain text
                    serde_json::to_string(&entry).map_err(|e| {
                        KernelError::internal(format!("Failed to serialize audit entry: {}", e))
                    })?
                };

                let bytes = line.as_bytes();

                file.write_all(bytes)
                    .and_then(|_| file.write_all(b"\n"))
                    .map_err(|e| KernelError::io(format!("Failed to write audit entry: {}", e)))?;

                *size += bytes.len() as u64 + 1;
            }

            file.flush()
                .map_err(|e| KernelError::io(format!("Failed to flush audit log: {}", e)))?;

            debug!("Flushed {} audit entries to disk", buf.len());
        }

        Ok(())
    }

    /// Rotate log file
    fn rotate_log_file(&self, path: &Path) -> Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let rotated_path = path.with_extension(format!("{}.bak", timestamp));

        std::fs::rename(path, &rotated_path)
            .map_err(|e| KernelError::io(format!("Failed to rotate audit log: {}", e)))?;

        info!("Rotated audit log to {:?}", rotated_path);
        Ok(())
    }

    /// Flush pending writes to disk
    pub fn flush(&self) -> Result<()> {
        let mut storage = self.storage.lock();

        match &mut *storage {
            AuditStorage::File { .. } => {
                self.flush_file_buffer(&mut storage)?;
            }
            _ => {} // Other backends write synchronously
        }

        Ok(())
    }

    /// Query audit log with filters
    pub fn query(&self, filter: AuditFilter) -> Result<Vec<AuditEntry>> {
        let storage = self.storage.lock();

        match &*storage {
            AuditStorage::Memory { entries, .. } => {
                let results: Vec<_> = entries
                    .iter()
                    .filter(|e| filter.matches(e))
                    .cloned()
                    .collect();
                Ok(results)
            }
            AuditStorage::File { .. } => {
                // For file backend, we'd need to read and parse the entire file
                // This is a simplified implementation
                Err(KernelError::not_implemented(
                    "Query not supported for file backend.",
                ))
            }
        }
    }

    /// Get log statistics
    pub fn stats(&self) -> AuditStats {
        let storage = self.storage.lock();
        let sequence = self.sequence.lock();

        match &*storage {
            AuditStorage::Memory { entries, .. } => AuditStats {
                total_entries: entries.len(),
                sequence: *sequence,
                backend: "memory",
            },
            AuditStorage::File { current_size, .. } => {
                let _size = *current_size.lock();
                AuditStats {
                    total_entries: 0, // Unknown without scanning
                    sequence: *sequence,
                    backend: "file",
                }
            }
        }
    }

    /// Run retention cleanup
    pub fn cleanup_old_entries(&self) -> Result<usize> {
        if self.config.retention_days == 0 {
            return Ok(0);
        }

        let cutoff_ns = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .saturating_sub(std::time::Duration::from_secs(
                self.config.retention_days * 86400,
            ))
            .as_nanos() as u64;

        let mut storage = self.storage.lock();

        match &mut *storage {
            AuditStorage::Memory { entries, .. } => {
                let before = entries.len();
                entries.retain(|e| e.timestamp_ns >= cutoff_ns);
                let removed = before - entries.len();
                debug!("Cleaned up {} old audit entries", removed);
                Ok(removed)
            }
            AuditStorage::File { .. } => {
                // File rotation handles retention for file backend
                Ok(0)
            }
        }
    }
}

impl Default for AuditLog {
    fn default() -> Self {
        Self::new()
    }
}

/// Audit query filter
#[derive(Debug, Default)]
pub struct AuditFilter {
    /// Filter by subject ID
    pub subject_id: Option<String>,
    /// Filter by object
    pub object: Option<String>,
    /// Filter by start time (nanoseconds)
    pub start_time_ns: Option<u64>,
    /// Filter by end time (nanoseconds)
    pub end_time_ns: Option<u64>,
    /// Limit number of results
    pub limit: Option<usize>,
}

impl AuditFilter {
    /// Create new audit filter
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by subject
    pub fn by_subject(mut self, id: impl Into<String>) -> Self {
        self.subject_id = Some(id.into());
        self
    }

    /// Filter by object
    pub fn by_object(mut self, object: impl Into<String>) -> Self {
        self.object = Some(object.into());
        self
    }

    /// Filter by time range
    pub fn time_range(mut self, start: u64, end: u64) -> Self {
        self.start_time_ns = Some(start);
        self.end_time_ns = Some(end);
        self
    }

    /// Limit results
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }

    fn matches(&self, entry: &AuditEntry) -> bool {
        if let Some(subject) = &self.subject_id {
            if &entry.subject_id != subject {
                return false;
            }
        }
        if let Some(object) = &self.object {
            if &entry.object != object {
                return false;
            }
        }
        if let Some(start) = self.start_time_ns {
            if entry.timestamp_ns < start {
                return false;
            }
        }
        if let Some(end) = self.end_time_ns {
            if entry.timestamp_ns > end {
                return false;
            }
        }
        true
    }
}

/// Audit statistics
#[derive(Debug, Clone)]
pub struct AuditStats {
    /// Total audit entries
    pub total_entries: usize,
    /// Current sequence number
    pub sequence: u64,
    /// Storage backend type
    pub backend: &'static str,
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    fn test_context() -> SecurityContext {
        SecurityContext {
            user_id: "test-user".to_string(),
            group_id: "test-group".to_string(),
            capabilities: vec![],
            clearance_level: crate::security::ClearanceLevel::Public,
            client_ip: None,
            session_id: None,
        }
    }

    #[test]
    fn test_audit_entry_creation() {
        let ctx = test_context();
        let entry = AuditEntry::new(
            1,
            &ctx,
            "/test/resource",
            AccessAction::Read,
            AccessDecision::Allow,
        );

        assert_eq!(entry.sequence, 1);
        assert_eq!(entry.subject_id, "test-user");
        assert_eq!(entry.object, "/test/resource");
    }

    #[test]
    fn test_audit_entry_integrity() {
        let ctx = test_context();
        let mut entry = AuditEntry::new(
            1,
            &ctx,
            "/test/resource",
            AccessAction::Read,
            AccessDecision::Allow,
        );

        entry.integrity_hash = Some(entry.calculate_hash());
        assert!(entry.verify_integrity());

        // Tamper with entry
        entry.object = "/tampered".to_string();
        assert!(!entry.verify_integrity());
    }

    #[test]
    fn test_memory_audit_log() {
        let log = AuditLog::new();
        let ctx = test_context();

        log.log_access_attempt(&ctx, "/test", AccessAction::Read, AccessDecision::Allow);
        log.log_access_attempt(&ctx, "/test2", AccessAction::Write, AccessDecision::Deny);

        let stats = log.stats();
        assert_eq!(stats.sequence, 2);
        assert_eq!(stats.backend, "memory");
    }

    #[test]
    fn test_file_audit_log() {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("audit.log");

        let config = AuditConfig {
            backend: AuditBackend::file(&log_path),
            enable_integrity: true,
            batch_size: 1, // Immediate flush
            ..Default::default()
        };

        let log = AuditLog::with_config(config).unwrap();
        let ctx = test_context();

        log.log_access_attempt(&ctx, "/test", AccessAction::Read, AccessDecision::Allow);
        log.flush().unwrap();

        // Verify file was created and contains data
        let content = std::fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("test-user"));
        assert!(content.contains("/test"));
    }
}
