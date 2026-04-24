//! Global Storage Manager
//!
//! Provides singleton access to the kernel's storage subsystem.

use std::sync::Arc;

use parking_lot::RwLock;

use super::{StorageBackend, StorageConfig, StorageManager};
use crate::error::Result;
// Re-export InMemoryStorage as MemoryBackend for backward compatibility
pub use crate::storage::backends::memory::InMemoryStorage as MemoryBackend;

/// Global storage manager wrapper with safe concurrent access
pub struct GlobalStorage {
    inner: RwLock<StorageManager>,
}

impl std::fmt::Debug for GlobalStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlobalStorage")
            .field("inner", &"<StorageManager>")
            .finish()
    }
}

impl GlobalStorage {
    /// Create new global storage instance
    pub fn new() -> Self {
        let config = StorageConfig::default();
        Self {
            inner: RwLock::new(StorageManager::new(config)),
        }
    }

    /// Initialize with custom config
    pub fn with_config(config: StorageConfig) -> Self {
        Self {
            inner: RwLock::new(StorageManager::new(config)),
        }
    }

    /// Store data
    pub fn put(&self, key: &str, data: &[u8]) -> Result<()> {
        let mut storage = self.inner.write();
        storage
            .put(key, data, None)
            .map_err(|e| crate::error::KernelError::internal(format!("Storage error: {:?}", e)))
    }

    /// Retrieve data
    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let mut storage = self.inner.write();
        storage
            .get(key, None)
            .map_err(|e| crate::error::KernelError::internal(format!("Storage error: {:?}", e)))
    }

    /// Delete data
    pub fn delete(&self, key: &str) -> Result<()> {
        let mut storage = self.inner.write();
        storage
            .delete(key, None)
            .map_err(|e| crate::error::KernelError::internal(format!("Storage error: {:?}", e)))
    }

    /// Check if key exists
    pub fn exists(&self, key: &str) -> Result<bool> {
        let mut storage = self.inner.write();
        match storage.get(key, None) {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(crate::error::KernelError::internal(format!(
                "Storage error: {:?}",
                e
            ))),
        }
    }

    /// List keys with prefix
    pub fn list(&self, prefix: &str) -> Result<Vec<String>> {
        let storage = self.inner.read();
        storage
            .list_keys(prefix, None)
            .map_err(|e| crate::error::KernelError::internal(format!("Storage error: {:?}", e)))
    }

    /// Register a backend
    pub fn register_backend(&self, name: String, backend: Box<dyn StorageBackend>) {
        let mut storage = self.inner.write();
        storage.register_backend(name, backend);
    }

    /// Get storage statistics
    pub fn stats(&self) -> super::StorageStats {
        let storage = self.inner.read();
        storage.get_stats().clone()
    }
}

impl Default for GlobalStorage {
    fn default() -> Self {
        Self::new()
    }
}

/// Global storage instance
static GLOBAL_STORAGE: std::sync::OnceLock<Arc<GlobalStorage>> = std::sync::OnceLock::new();

/// Initialize global storage
pub fn init(config: Option<StorageConfig>) -> Result<()> {
    let storage = match config {
        Some(cfg) => GlobalStorage::with_config(cfg),
        None => GlobalStorage::new(),
    };

    let _ = GLOBAL_STORAGE.set(Arc::new(storage));
    tracing::info!("Global storage initialized");
    Ok(())
}

/// Get global storage instance
pub fn global() -> Arc<GlobalStorage> {
    GLOBAL_STORAGE
        .get_or_init(|| Arc::new(GlobalStorage::new()))
        .clone()
}

/// Get workspace-specific storage key prefix
pub fn workspace_key(agent_id: &str, path: &str) -> String {
    format!("workspace/{}/{}", agent_id, path.trim_start_matches('/'))
}

/// Check if path is within agent's workspace
pub fn is_in_workspace(agent_id: &str, path: &str) -> bool {
    // Workspace keys are prefixed with workspace/{agent_id}/
    let prefix = format!("workspace/{}/", agent_id);
    path.starts_with(&prefix)
}

/// Validate agent workspace access
pub fn validate_workspace_access(agent_id: &str, path: &str, for_write: bool) -> Result<String> {
    // Construct the workspace key
    let key = workspace_key(agent_id, path);

    // Additional validation can be added here
    // For example: checking if the agent has access to other workspaces

    if for_write {
        tracing::debug!("Workspace write: agent={}, path={}", agent_id, path);
    } else {
        tracing::debug!("Workspace read: agent={}, path={}", agent_id, path);
    }

    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_key() {
        assert_eq!(
            workspace_key("agent1", "/data/file.txt"),
            "workspace/agent1/data/file.txt"
        );
        assert_eq!(
            workspace_key("agent1", "data/file.txt"),
            "workspace/agent1/data/file.txt"
        );
    }

    #[test]
    fn test_is_in_workspace() {
        assert!(is_in_workspace("agent1", "workspace/agent1/data"));
        assert!(!is_in_workspace("agent1", "workspace/agent2/data"));
    }

    #[test]
    fn test_memory_backend_reexport() {
        // Test that MemoryBackend (re-export of InMemoryStorage) works
        let backend = MemoryBackend::new();

        // Put
        let metadata = super::super::EntryMetadata {
            created_at: 0,
            modified_at: 0,
            size_bytes: 5,
            content_type: "text/plain".to_string(),
            checksum: "abc".to_string(),
            tags: vec![],
        };
        assert!(backend.put("key1", b"hello", metadata).is_ok());

        // Get
        let entry = backend.get("key1").unwrap().unwrap();
        assert_eq!(entry.data, b"hello");

        // List
        let keys = backend.list("").unwrap();
        assert_eq!(keys.len(), 1);

        // Delete
        backend.delete("key1").unwrap();
        assert!(backend.get("key1").unwrap().is_none());
    }

    #[test]
    fn test_global_storage() {
        let storage = GlobalStorage::new();

        // Test put/get
        storage.put("test_key", b"test_value").unwrap();
        let value = storage.get("test_key").unwrap();
        assert_eq!(value, Some(b"test_value".to_vec()));

        // Test exists
        assert!(storage.exists("test_key").unwrap());
        assert!(!storage.exists("nonexistent").unwrap());

        // Test delete
        storage.delete("test_key").unwrap();
        assert!(!storage.exists("test_key").unwrap());
    }

    #[test]
    fn test_validate_workspace_access() {
        let key = validate_workspace_access("agent123", "/data/file.txt", true).unwrap();
        assert_eq!(key, "workspace/agent123/data/file.txt");

        // Leading slash should be handled
        let key2 = validate_workspace_access("agent123", "data/file.txt", false).unwrap();
        assert_eq!(key2, "workspace/agent123/data/file.txt");
    }
}
