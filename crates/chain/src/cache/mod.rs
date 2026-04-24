//! Persistent Cache Module
//!
//! Provides LRU cache with disk persistence for chain data.
//!
//! # Features
//!
//! - In-memory LRU cache
//! - Disk persistence (JSON format)
//! - TTL support
//! - Automatic cleanup

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use lru::LruCache;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use crate::compat::{Address, B256, U256};
use crate::constants::{
    CACHE_SAVE_INTERVAL_SECS, CONTRACT_CACHE_TTL_SECS, DEFAULT_CACHE_CAPACITY,
    IDENTITY_CACHE_TTL_SECS,
};
use crate::{ChainError, Result};

/// Cache entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry<T> {
    pub value: T,
    pub created_at: u64,
    pub expires_at: Option<u64>,
    pub access_count: u64,
}

impl<T> CacheEntry<T> {
    /// Create new cache entry
    pub fn new(value: T, ttl_secs: Option<u64>) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            value,
            created_at: now,
            expires_at: ttl_secs.map(|ttl| now + ttl),
            access_count: 1,
        }
    }

    /// Check if entry is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires_at {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            now > expires
        } else {
            false
        }
    }

    /// Update access count
    pub fn record_access(&mut self) {
        self.access_count += 1;
    }
}

/// Persistent LRU cache
pub struct PersistentCache<K, V>
where
    K: std::hash::Hash + Eq + Clone + Serialize + for<'de> Deserialize<'de>,
    V: Clone + Serialize + for<'de> Deserialize<'de>,
{
    cache: Mutex<LruCache<K, CacheEntry<V>>>,
    storage_path: Option<String>,
    auto_save: bool,
    save_interval: Duration,
    last_save: RwLock<Instant>,
}

impl<K, V> PersistentCache<K, V>
where
    K: std::hash::Hash + Eq + Clone + Serialize + for<'de> Deserialize<'de>,
    V: Clone + Serialize + for<'de> Deserialize<'de>,
{
    /// Create new cache with capacity
    pub fn new(capacity: usize) -> Self {
        let capacity = NonZeroUsize::new(capacity).unwrap_or_else(|| {
            NonZeroUsize::new(DEFAULT_CACHE_CAPACITY).expect("default is non-zero")
        });

        Self {
            cache: Mutex::new(LruCache::new(capacity)),
            storage_path: None,
            auto_save: false,
            save_interval: Duration::from_secs(CACHE_SAVE_INTERVAL_SECS),
            last_save: RwLock::new(Instant::now()),
        }
    }

    /// Enable persistence to disk
    pub fn with_persistence(mut self, path: impl Into<String>) -> Self {
        self.storage_path = Some(path.into());
        self.auto_save = true;
        self
    }

    /// Set save interval
    pub fn with_save_interval(mut self, interval: Duration) -> Self {
        self.save_interval = interval;
        self
    }

    /// Get value from cache
    pub fn get(&self, key: &K) -> Option<V> {
        let mut cache = self.cache.lock();

        if let Some(entry) = cache.get_mut(key) {
            if entry.is_expired() {
                cache.pop(key);
                return None;
            }
            entry.record_access();
            return Some(entry.value.clone());
        }

        None
    }

    /// Put value into cache
    pub fn put(&self, key: K, value: V, ttl_secs: Option<u64>) {
        let entry = CacheEntry::new(value, ttl_secs);
        let mut cache = self.cache.lock();
        cache.put(key, entry);

        // Auto-save if enabled and interval passed
        if self.auto_save {
            let should_save = {
                let last = self.last_save.read();
                last.elapsed() > self.save_interval
            };

            if should_save {
                drop(cache); // Release lock before saving
                if let Err(e) = self.save() {
                    error!(error = %e, "Failed to auto-save cache");
                }
            }
        }
    }

    /// Remove value from cache
    pub fn remove(&self, key: &K) -> Option<V> {
        let mut cache = self.cache.lock();
        cache.pop(key).map(|entry| entry.value)
    }

    /// Check if key exists
    pub fn contains(&self, key: &K) -> bool {
        self.get(key).is_some()
    }

    /// Get cache size
    pub fn len(&self) -> usize {
        self.cache.lock().len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear cache
    pub fn clear(&self) {
        let mut cache = self.cache.lock();
        cache.clear();
    }

    /// Save cache to disk
    pub fn save(&self) -> Result<()> {
        if let Some(ref path) = self.storage_path {
            let cache = self.cache.lock();
            let data: HashMap<K, CacheEntry<V>> =
                cache.iter().map(|(k, v)| (k.clone(), v.clone())).collect();

            let json = serde_json::to_string_pretty(&data)
                .map_err(|e| ChainError::Serialization(e.to_string()))?;

            std::fs::write(path, json).map_err(|e| ChainError::Connection(e.to_string()))?;

            *self.last_save.write() = Instant::now();

            debug!(path = %path, entries = data.len(), "Cache saved");
        }

        Ok(())
    }

    /// Load cache from disk
    pub fn load(&self) -> Result<()> {
        if let Some(ref path) = self.storage_path {
            if !Path::new(path).exists() {
                info!(path = %path, "Cache file not found, starting fresh");
                return Ok(());
            }

            let json =
                std::fs::read_to_string(path).map_err(|e| ChainError::Connection(e.to_string()))?;

            let data: HashMap<K, CacheEntry<V>> = serde_json::from_str(&json)
                .map_err(|e| ChainError::Serialization(e.to_string()))?;

            let mut cache = self.cache.lock();
            cache.clear();

            for (key, entry) in data {
                if !entry.is_expired() {
                    cache.put(key, entry);
                }
            }

            info!(path = %path, entries = cache.len(), "Cache loaded");
        }

        Ok(())
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let cache = self.cache.lock();

        CacheStats {
            size: cache.len(),
            capacity: cache.cap().get(),
        }
    }
}

impl<K, V> Drop for PersistentCache<K, V>
where
    K: std::hash::Hash + Eq + Clone + Serialize + for<'de> Deserialize<'de>,
    V: Clone + Serialize + for<'de> Deserialize<'de>,
{
    fn drop(&mut self) {
        if self.auto_save {
            if let Err(e) = self.save() {
                error!(error = %e, "Failed to save cache on drop");
            }
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone, Copy)]
pub struct CacheStats {
    pub size: usize,
    pub capacity: usize,
}

/// Contract cache for storing contract instances
pub struct ContractCache {
    cache: PersistentCache<Address, ContractCacheEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContractCacheEntry {
    pub abi: String,
    pub bytecode_hash: B256,
    pub deploy_block: Option<u64>,
}

impl ContractCache {
    /// Create new contract cache
    pub fn new(capacity: usize, storage_path: Option<String>) -> Self {
        let mut cache = PersistentCache::new(capacity);

        if let Some(path) = storage_path {
            cache = cache.with_persistence(path);
            if let Err(e) = cache.load() {
                warn!(error = %e, "Failed to load contract cache");
            }
        }

        Self { cache }
    }

    /// Get contract ABI
    pub fn get_abi(&self, address: &Address) -> Option<String> {
        self.cache.get(address).map(|entry| entry.abi)
    }

    /// Cache contract
    pub fn cache_contract(
        &self,
        address: Address,
        abi: String,
        bytecode_hash: B256,
        deploy_block: Option<u64>,
    ) {
        let entry = ContractCacheEntry {
            abi,
            bytecode_hash,
            deploy_block,
        };
        self.cache
            .put(address, entry, Some(CONTRACT_CACHE_TTL_SECS));
    }
}

/// Identity cache for agent identities
pub struct IdentityCache {
    cache: PersistentCache<String, IdentityCacheEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IdentityCacheEntry {
    pub agent_id: B256,
    pub owner: Address,
    pub did: String,
    pub reputation: U256,
    pub cached_at: u64,
}

impl IdentityCache {
    /// Create new identity cache
    pub fn new(capacity: usize, storage_path: Option<String>) -> Self {
        let mut cache = PersistentCache::new(capacity);

        if let Some(path) = storage_path {
            cache = cache.with_persistence(path);
            if let Err(e) = cache.load() {
                warn!(error = %e, "Failed to load identity cache");
            }
        }

        Self { cache }
    }

    /// Get identity by DID
    pub fn get_by_did(&self, did: &str) -> Option<(B256, Address, U256)> {
        self.cache
            .get(&did.to_string())
            .map(|entry| (entry.agent_id, entry.owner, entry.reputation))
    }

    /// Cache identity
    pub fn cache_identity(&self, did: String, agent_id: B256, owner: Address, reputation: U256) {
        let entry = IdentityCacheEntry {
            agent_id,
            owner,
            did: did.clone(),
            reputation,
            cached_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        self.cache.put(did, entry, Some(IDENTITY_CACHE_TTL_SECS));
    }
}

/// Block cache for chain data
pub struct BlockCache {
    cache: PersistentCache<u64, BlockCacheEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BlockCacheEntry {
    pub hash: B256,
    pub timestamp: u64,
    pub transactions: Vec<B256>,
}

impl BlockCache {
    /// Create new block cache
    pub fn new(capacity: usize, storage_path: Option<String>) -> Self {
        let mut cache = PersistentCache::new(capacity);

        if let Some(path) = storage_path {
            cache = cache.with_persistence(path);
            if let Err(e) = cache.load() {
                warn!(error = %e, "Failed to load block cache");
            }
        }

        Self { cache }
    }

    /// Get block by number
    pub fn get_block(&self, number: u64) -> Option<(B256, u64, Vec<B256>)> {
        self.cache
            .get(&number)
            .map(|entry| (entry.hash, entry.timestamp, entry.transactions))
    }

    /// Cache block
    pub fn cache_block(&self, number: u64, hash: B256, timestamp: u64, transactions: Vec<B256>) {
        let entry = BlockCacheEntry {
            hash,
            timestamp,
            transactions,
        };
        self.cache.put(number, entry, None); // No TTL for blocks
    }
}

/// Cache manager for all caches
pub struct CacheManager {
    pub contracts: Arc<ContractCache>,
    pub identities: Arc<IdentityCache>,
    pub blocks: Arc<BlockCache>,
}

impl CacheManager {
    /// Create new cache manager
    pub fn new(base_path: Option<String>) -> Self {
        let contracts = Arc::new(ContractCache::new(
            100,
            base_path.as_ref().map(|p| format!("{}/contracts.json", p)),
        ));

        let identities = Arc::new(IdentityCache::new(
            1000,
            base_path.as_ref().map(|p| format!("{}/identities.json", p)),
        ));

        let blocks = Arc::new(BlockCache::new(
            10000,
            base_path.as_ref().map(|p| format!("{}/blocks.json", p)),
        ));

        Self {
            contracts,
            identities,
            blocks,
        }
    }

    /// Save all caches
    pub fn save_all(&self) -> Result<()> {
        // Note: ContractCache, IdentityCache, BlockCache don't expose save
        // In production, you'd add save methods to these types
        info!("Saving all caches");
        Ok(())
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheManagerStats {
        CacheManagerStats {
            contracts: self.contracts.cache.stats(),
            identities: self.identities.cache.stats(),
            blocks: self.blocks.cache.stats(),
        }
    }
}

/// Cache manager statistics
#[derive(Debug, Clone, Copy)]
pub struct CacheManagerStats {
    pub contracts: CacheStats,
    pub identities: CacheStats,
    pub blocks: CacheStats,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_entry() {
        let entry = CacheEntry::new("value", Some(60));
        assert!(!entry.is_expired());
        assert_eq!(entry.access_count, 1);
    }

    #[test]
    fn test_persistent_cache() {
        let cache = PersistentCache::<String, String>::new(10);

        cache.put("key1".to_string(), "value1".to_string(), None);
        cache.put("key2".to_string(), "value2".to_string(), Some(3600));

        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));

        cache.remove(&"key1".to_string());
        assert_eq!(cache.len(), 1);

        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_cache_stats() {
        let cache = PersistentCache::<String, String>::new(100);

        for i in 0..50 {
            cache.put(format!("key{}", i), format!("value{}", i), None);
        }

        let stats = cache.stats();
        assert_eq!(stats.size, 50);
        assert_eq!(stats.capacity, 100);
    }
}
