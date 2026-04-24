//! Storage Module
//!
//! Provides persistent storage services for the kernel with support for
//! multiple backends, compression, and encryption.
//!
//! ## Submodules
//!
//! - `kv_store`: Key-value storage operations
//! - `blob_store`: Large binary object storage
//! - `indexing`: Storage indexing and query
//! - `global`: Global storage manager with workspace isolation
//! - `backends`: Storage backend implementations

pub mod backends;
pub mod blob_store;
pub mod global;
pub mod indexing;
pub mod kv_store;

// Re-export common backends for convenience
use std::collections::HashMap;
use std::path::PathBuf;

pub use backends::encrypted::{generate_key, generate_salt, EncryptedStorage};
pub use backends::filesystem::FilesystemStorage;
pub use backends::memory::InMemoryStorage;
#[cfg(feature = "redb")]
pub use backends::redb::{RedbDurability, RedbStats, RedbStorage};
#[cfg(feature = "rocksdb")]
pub use backends::rocksdb::RocksDbStorage;
#[cfg(feature = "sqlite")]
pub use backends::sqlite::{SqliteStats, SqliteStorage};
use serde::{Deserialize, Serialize};

/// Configuration for storage subsystem
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Base path for storage files
    pub base_path: PathBuf,
    /// Maximum storage size in megabytes
    pub max_size_mb: u64,
    /// Whether to enable compression
    pub compression_enabled: bool,
    /// Whether to enable encryption
    pub encryption_enabled: bool,
    /// Cache size in megabytes
    pub cache_size_mb: u64,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            base_path: PathBuf::from("data/storage"),
            max_size_mb: 10240,
            compression_enabled: true,
            encryption_enabled: false,
            cache_size_mb: 512,
        }
    }
}

/// Storage entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageEntry {
    /// Storage key
    pub key: String,
    /// Raw data bytes
    pub data: Vec<u8>,
    /// Entry metadata
    pub metadata: EntryMetadata,
}

/// Metadata for storage entries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryMetadata {
    /// Unix timestamp when entry was created
    pub created_at: u64,
    /// Unix timestamp when entry was last modified
    pub modified_at: u64,
    /// Size of data in bytes
    pub size_bytes: u64,
    /// MIME content type
    pub content_type: String,
    /// Data checksum for integrity verification
    pub checksum: String,
    /// User-defined tags
    pub tags: Vec<String>,
}

/// Backend interface for storage operations
///
/// This trait defines the core operations that all storage backends must
/// implement. It provides default implementations for some methods to reduce
/// boilerplate.
pub trait StorageBackend: Send + Sync + std::fmt::Debug {
    /// Store data with metadata
    fn put(
        &self,
        key: &str,
        data: &[u8],
        metadata: EntryMetadata,
    ) -> std::result::Result<(), StorageError>;

    /// Retrieve data by key
    fn get(&self, key: &str) -> std::result::Result<Option<StorageEntry>, StorageError>;

    /// Delete data by key
    fn delete(&self, key: &str) -> std::result::Result<(), StorageError>;

    /// List keys with given prefix
    fn list(&self, prefix: &str) -> std::result::Result<Vec<String>, StorageError>;

    /// Check if key exists
    ///
    /// Default implementation uses `get`, backends can override for better
    /// performance
    fn exists(&self, key: &str) -> std::result::Result<bool, StorageError> {
        Ok(self.get(key)?.is_some())
    }

    /// Get multiple keys in a single operation
    ///
    /// Default implementation iterates and calls `get` for each key.
    /// Backends can override for more efficient batch operations.
    fn get_batch(
        &self,
        keys: &[&str],
    ) -> std::result::Result<Vec<Option<StorageEntry>>, StorageError> {
        keys.iter().map(|k| self.get(k)).collect()
    }

    /// Store multiple entries in a single operation
    ///
    /// Default implementation iterates and calls `put` for each entry.
    /// Backends can override for atomic batch operations.
    fn put_batch(
        &self,
        entries: &[(&str, &[u8], EntryMetadata)],
    ) -> std::result::Result<(), StorageError> {
        for (key, data, metadata) in entries {
            self.put(key, data, metadata.clone())?;
        }
        Ok(())
    }
}

/// Storage operation errors
#[derive(Debug, Clone)]
pub enum StorageError {
    /// IO operation failed
    IoError(String),
    /// Key not found in storage
    KeyNotFound,
    /// Storage quota exceeded
    StorageFull,
    /// Invalid key format
    InvalidKey,
    /// Data integrity check failed
    CorruptedData,
    /// Access denied
    PermissionDenied,
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageError::IoError(e) => write!(f, "IO error: {}", e),
            StorageError::KeyNotFound => write!(f, "Key not found"),
            StorageError::StorageFull => write!(f, "Storage full"),
            StorageError::InvalidKey => write!(f, "Invalid key"),
            StorageError::CorruptedData => write!(f, "Corrupted data"),
            StorageError::PermissionDenied => write!(f, "Permission denied"),
        }
    }
}

impl std::error::Error for StorageError {}

/// Serialization helpers for storage backends
///
/// These helpers reduce code duplication across backends that use
/// JSON serialization for storage entries.
pub mod serialization {
    use super::{EntryMetadata, StorageEntry, StorageError};

    /// Serialize a storage entry to JSON bytes
    pub fn serialize_entry(
        key: &str,
        data: &[u8],
        metadata: EntryMetadata,
    ) -> Result<Vec<u8>, StorageError> {
        let entry = StorageEntry {
            key: key.to_string(),
            data: data.to_vec(),
            metadata,
        };
        serde_json::to_vec(&entry)
            .map_err(|e| StorageError::IoError(format!("Serialization failed: {}", e)))
    }

    /// Deserialize a storage entry from JSON bytes
    pub fn deserialize_entry(data: &[u8]) -> Result<StorageEntry, StorageError> {
        serde_json::from_slice(data)
            .map_err(|e| StorageError::IoError(format!("Deserialization failed: {}", e)))
    }
}

/// Manages storage backends and operations
pub struct StorageManager {
    /// Storage configuration - retained for future use
    #[allow(dead_code)]
    _config: StorageConfig,
    /// Registered storage backends
    backends: HashMap<String, Box<dyn StorageBackend>>,
    /// Default backend name
    default_backend: String,
    /// Storage statistics
    stats: StorageStats,
}

/// Storage operation statistics
#[derive(Debug, Clone, Default)]
pub struct StorageStats {
    /// Total number of keys stored
    pub total_keys: u64,
    /// Total size of all data in bytes
    pub total_size_bytes: u64,
    /// Number of get operations performed
    pub get_operations: u64,
    /// Number of put operations performed
    pub put_operations: u64,
    /// Number of delete operations performed
    pub delete_operations: u64,
    /// Cache hit count
    pub hit_count: u64,
    /// Cache miss count
    pub miss_count: u64,
}

impl StorageManager {
    /// Create new storage manager with configuration
    pub fn new(config: StorageConfig) -> Self {
        let mut backends: HashMap<String, Box<dyn StorageBackend>> = HashMap::new();
        // Register default in-memory backend
        backends.insert(
            "default".to_string(),
            Box::new(crate::storage::backends::memory::InMemoryStorage::new()),
        );

        Self {
            _config: config,
            backends,
            default_backend: "default".to_string(),
            stats: StorageStats::default(),
        }
    }

    /// Register a storage backend
    pub fn register_backend(&mut self, name: String, backend: Box<dyn StorageBackend>) {
        self.backends.insert(name, backend);
    }

    /// Set the default backend name
    pub fn set_default_backend(&mut self, name: &str) {
        self.default_backend = name.to_string();
    }

    fn get_backend(
        &self,
        name: Option<&str>,
    ) -> std::result::Result<&dyn StorageBackend, StorageError> {
        let name = name.unwrap_or(&self.default_backend);
        self.backends
            .get(name)
            .map(|b| b.as_ref())
            .ok_or(StorageError::KeyNotFound)
    }

    /// Store data with metadata
    pub fn put(
        &mut self,
        key: &str,
        data: &[u8],
        backend: Option<&str>,
    ) -> std::result::Result<(), StorageError> {
        let backend = self.get_backend(backend)?;

        let metadata = EntryMetadata {
            created_at: chrono::Utc::now().timestamp() as u64,
            modified_at: chrono::Utc::now().timestamp() as u64,
            size_bytes: data.len() as u64,
            content_type: "application/octet-stream".to_string(),
            checksum: self.calculate_checksum(data),
            tags: vec![],
        };

        backend.put(key, data, metadata)?;
        self.stats.put_operations += 1;
        self.stats.total_keys += 1;
        self.stats.total_size_bytes += data.len() as u64;

        Ok(())
    }

    /// Retrieve data by key
    pub fn get(
        &mut self,
        key: &str,
        backend: Option<&str>,
    ) -> std::result::Result<Option<Vec<u8>>, StorageError> {
        let backend = self.get_backend(backend)?;

        match backend.get(key)? {
            Some(entry) => {
                self.stats.get_operations += 1;
                self.stats.hit_count += 1;
                Ok(Some(entry.data))
            }
            None => {
                self.stats.get_operations += 1;
                self.stats.miss_count += 1;
                Ok(None)
            }
        }
    }

    /// Delete a key from storage
    pub fn delete(
        &mut self,
        key: &str,
        backend: Option<&str>,
    ) -> std::result::Result<(), StorageError> {
        let backend = self.get_backend(backend)?;

        if let Some(entry) = backend.get(key)? {
            backend.delete(key)?;
            self.stats.delete_operations += 1;
            self.stats.total_keys = self.stats.total_keys.saturating_sub(1);
            self.stats.total_size_bytes = self
                .stats
                .total_size_bytes
                .saturating_sub(entry.metadata.size_bytes);
        }

        Ok(())
    }

    /// List keys with optional prefix filter
    pub fn list_keys(
        &self,
        prefix: &str,
        backend: Option<&str>,
    ) -> std::result::Result<Vec<String>, StorageError> {
        let backend = self.get_backend(backend)?;
        backend.list(prefix)
    }

    fn calculate_checksum(&self, data: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    /// Get storage statistics
    pub fn get_stats(&self) -> &StorageStats {
        &self.stats
    }

    /// Calculate cache hit rate
    pub fn get_cache_hit_rate(&self) -> f64 {
        let total = self.stats.hit_count + self.stats.miss_count;
        if total == 0 {
            0.0
        } else {
            self.stats.hit_count as f64 / total as f64
        }
    }
}

use crate::error::Result;

/// Initialize storage subsystem
pub fn init() -> Result<()> {
    tracing::info!("Initializing storage subsystem");
    // Storage initialization happens lazily when first used
    Ok(())
}

/// Test utilities for storage backends
///
/// These utilities are only available in test builds and provide
/// common helper functions to reduce test code duplication.
#[cfg(test)]
pub mod test_utils {
    use super::EntryMetadata;

    /// Create a standard test metadata entry
    ///
    /// This helper is used across all backend test modules to ensure
    /// consistent test data.
    pub fn create_test_metadata() -> EntryMetadata {
        EntryMetadata {
            created_at: 0,
            modified_at: 0,
            size_bytes: 0,
            content_type: "application/octet-stream".to_string(),
            checksum: "".to_string(),
            tags: vec![],
        }
    }

    /// Create test metadata with custom content type
    pub fn create_test_metadata_with_content_type(content_type: &str) -> EntryMetadata {
        EntryMetadata {
            created_at: 0,
            modified_at: 0,
            size_bytes: 0,
            content_type: content_type.to_string(),
            checksum: "".to_string(),
            tags: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_utils::create_test_metadata;
    use super::*;

    #[test]
    fn test_storage_config_default() {
        let config = StorageConfig::default();
        assert_eq!(config.max_size_mb, 10240);
        assert!(config.compression_enabled);
        assert!(!config.encryption_enabled);
    }

    #[test]
    fn test_storage_manager_new() {
        let config = StorageConfig::default();
        let manager = StorageManager::new(config);
        assert_eq!(manager.get_stats().total_keys, 0);
    }

    #[test]
    fn test_test_utils_create_metadata() {
        let metadata = create_test_metadata();
        assert_eq!(metadata.created_at, 0);
        assert_eq!(metadata.content_type, "application/octet-stream");
        assert!(metadata.tags.is_empty());
    }
}
