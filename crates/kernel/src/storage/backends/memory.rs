//! In-Memory Storage Backend
//!
//! Fast, volatile storage using HashMap. Data is lost on restart.

use std::collections::HashMap;

use parking_lot::Mutex;

use crate::storage::{EntryMetadata, StorageBackend, StorageEntry, StorageError};

/// In-memory storage backend
#[derive(Debug)]
pub struct InMemoryStorage {
    data: Mutex<HashMap<String, StorageEntry>>,
}

impl InMemoryStorage {
    /// Create new in-memory storage
    pub fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
        }
    }

    /// Create with pre-allocated capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Mutex::new(HashMap::with_capacity(capacity)),
        }
    }
}

impl Default for InMemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl StorageBackend for InMemoryStorage {
    fn put(
        &self,
        key: &str,
        data: &[u8],
        metadata: EntryMetadata,
    ) -> std::result::Result<(), StorageError> {
        let mut store = self.data.lock();
        store.insert(
            key.to_string(),
            StorageEntry {
                key: key.to_string(),
                data: data.to_vec(),
                metadata,
            },
        );
        Ok(())
    }

    fn get(&self, key: &str) -> std::result::Result<Option<StorageEntry>, StorageError> {
        let store = self.data.lock();
        Ok(store.get(key).cloned())
    }

    fn delete(&self, key: &str) -> std::result::Result<(), StorageError> {
        let mut store = self.data.lock();
        store.remove(key);
        Ok(())
    }

    fn list(&self, prefix: &str) -> std::result::Result<Vec<String>, StorageError> {
        let store = self.data.lock();
        Ok(store
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }

    fn exists(&self, key: &str) -> std::result::Result<bool, StorageError> {
        let store = self.data.lock();
        Ok(store.contains_key(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::test_utils::create_test_metadata;

    #[test]
    fn test_basic_operations() {
        let storage = InMemoryStorage::new();
        let metadata = create_test_metadata();

        // Test put and get
        storage.put("key1", b"value1", metadata.clone()).unwrap();
        let entry = storage.get("key1").unwrap().unwrap();
        assert_eq!(entry.data, b"value1");

        // Test exists
        assert!(storage.exists("key1").unwrap());
        assert!(!storage.exists("nonexistent").unwrap());

        // Test delete
        storage.delete("key1").unwrap();
        assert!(!storage.exists("key1").unwrap());
    }

    #[test]
    fn test_list_with_prefix() {
        let storage = InMemoryStorage::new();
        let metadata = create_test_metadata();

        storage.put("prefix:1", b"v1", metadata.clone()).unwrap();
        storage.put("prefix:2", b"v2", metadata.clone()).unwrap();
        storage.put("other", b"v3", metadata.clone()).unwrap();

        let keys = storage.list("prefix:").unwrap();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"prefix:1".to_string()));
        assert!(keys.contains(&"prefix:2".to_string()));
    }
}
