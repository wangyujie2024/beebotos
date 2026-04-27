//! Chain State Cache Module
//!
//! Provides caching for frequently accessed on-chain data like balances and
//! nonces. Reduces RPC calls and improves performance for wallet operations.

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use lru::LruCache;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, trace};

use crate::chains::common::EvmError;
use crate::compat::{Address, U256};
use crate::constants::DEFAULT_CACHE_TTL_SECS;

/// Chain state cache for balances and nonces
#[derive(Clone)]
pub struct ChainStateCache {
    /// Balance cache
    balance_cache: Arc<RwLock<LruCache<Address, CachedBalance>>>,
    /// Nonce cache
    nonce_cache: Arc<RwLock<LruCache<Address, CachedNonce>>>,
    /// Code cache (for contract detection)
    code_cache: Arc<RwLock<LruCache<Address, CachedCode>>>,
    /// Storage cache
    storage_cache: Arc<RwLock<LruCache<(Address, U256), CachedStorage>>>,
    /// Default TTL for cache entries
    default_ttl: Duration,
    /// Balance TTL (usually longer)
    balance_ttl: Duration,
    /// Nonce TTL (shorter for pending transactions)
    nonce_ttl: Duration,
}

/// Cached balance entry
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedBalance {
    balance: U256,
    block_number: u64,
    cached_at: u64,
    ttl_secs: u64,
}

impl CachedBalance {
    fn new(balance: U256, block_number: u64, ttl_secs: u64) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            balance,
            block_number,
            cached_at: now,
            ttl_secs,
        }
    }

    fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now >= self.cached_at + self.ttl_secs
    }
}

/// Cached nonce entry
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedNonce {
    nonce: u64,
    block_number: u64,
    cached_at: u64,
    ttl_secs: u64,
    /// Whether this is the pending nonce
    is_pending: bool,
}

impl CachedNonce {
    fn new(nonce: u64, block_number: u64, ttl_secs: u64, is_pending: bool) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            nonce,
            block_number,
            cached_at: now,
            ttl_secs,
            is_pending,
        }
    }

    fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now >= self.cached_at + self.ttl_secs
    }
}

/// Cached code entry
#[derive(Debug, Clone)]
struct CachedCode {
    #[allow(dead_code)]
    code_hash: [u8; 32],
    code_size: usize,
    is_contract: bool,
    cached_at: Instant,
    ttl: Duration,
}

impl CachedCode {
    fn new(code: &[u8], ttl: Duration) -> Self {
        // use alloy_primitives::Keccak256;
        let code_hash = if code.is_empty() {
            [0u8; 32]
        } else {
            alloy_primitives::keccak256(code).into()
        };

        Self {
            code_hash,
            code_size: code.len(),
            is_contract: !code.is_empty(),
            cached_at: Instant::now(),
            ttl,
        }
    }

    fn is_expired(&self) -> bool {
        self.cached_at.elapsed() > self.ttl
    }
}

/// Cached storage entry
#[derive(Debug, Clone)]
struct CachedStorage {
    value: U256,
    block_number: u64,
    cached_at: Instant,
    ttl: Duration,
}

impl CachedStorage {
    fn new(value: U256, block_number: u64, ttl: Duration) -> Self {
        Self {
            value,
            block_number,
            cached_at: Instant::now(),
            ttl,
        }
    }

    fn is_expired(&self) -> bool {
        self.cached_at.elapsed() > self.ttl
    }
}

/// Cache configuration
#[derive(Debug, Clone)]
pub struct StateCacheConfig {
    /// Balance cache capacity
    pub balance_cache_capacity: usize,
    /// Nonce cache capacity
    pub nonce_cache_capacity: usize,
    /// Code cache capacity
    pub code_cache_capacity: usize,
    /// Storage cache capacity
    pub storage_cache_capacity: usize,
    /// Default TTL for cache entries
    pub default_ttl: Duration,
    /// Balance TTL
    pub balance_ttl: Duration,
    /// Nonce TTL (shorter for pending)
    pub nonce_ttl: Duration,
    /// Pending nonce TTL (very short)
    pub pending_nonce_ttl: Duration,
    /// Code TTL (longer, as contracts rarely change)
    pub code_ttl: Duration,
}

impl Default for StateCacheConfig {
    fn default() -> Self {
        Self {
            balance_cache_capacity: 1000,
            nonce_cache_capacity: 1000,
            code_cache_capacity: 500,
            storage_cache_capacity: 500,
            default_ttl: Duration::from_secs(DEFAULT_CACHE_TTL_SECS),
            balance_ttl: Duration::from_secs(60), // 1 minute
            nonce_ttl: Duration::from_secs(12),   // 1 block
            pending_nonce_ttl: Duration::from_secs(6), // ~0.5 block
            code_ttl: Duration::from_secs(3600),  // 1 hour
        }
    }
}

impl ChainStateCache {
    /// Create new state cache with default config
    pub fn new() -> Self {
        Self::with_config(StateCacheConfig::default())
    }

    /// Create with custom config
    pub fn with_config(config: StateCacheConfig) -> Self {
        let balance_cache = LruCache::new(
            NonZeroUsize::new(config.balance_cache_capacity)
                .unwrap_or(NonZeroUsize::new(1000).unwrap()),
        );
        let nonce_cache = LruCache::new(
            NonZeroUsize::new(config.nonce_cache_capacity)
                .unwrap_or(NonZeroUsize::new(1000).unwrap()),
        );
        let code_cache = LruCache::new(
            NonZeroUsize::new(config.code_cache_capacity)
                .unwrap_or(NonZeroUsize::new(500).unwrap()),
        );
        let storage_cache = LruCache::new(
            NonZeroUsize::new(config.storage_cache_capacity)
                .unwrap_or(NonZeroUsize::new(500).unwrap()),
        );

        Self {
            balance_cache: Arc::new(RwLock::new(balance_cache)),
            nonce_cache: Arc::new(RwLock::new(nonce_cache)),
            code_cache: Arc::new(RwLock::new(code_cache)),
            storage_cache: Arc::new(RwLock::new(storage_cache)),
            default_ttl: config.default_ttl,
            balance_ttl: config.balance_ttl,
            nonce_ttl: config.nonce_ttl,
        }
    }

    // ============================================================================
    // Balance Cache
    // ============================================================================

    /// Get cached balance
    pub fn get_balance(&self, address: &Address) -> Option<(U256, u64)> {
        let mut cache = self.balance_cache.write();

        if let Some(entry) = cache.get_mut(address) {
            if !entry.is_expired() {
                trace!(
                    target: "chain::state_cache",
                    address = %address,
                    balance = %entry.balance,
                    "Cache hit for balance"
                );
                return Some((entry.balance, entry.block_number));
            }
            // Expired, remove it
            cache.pop(address);
        }

        None
    }

    /// Cache balance
    pub fn cache_balance(&self, address: Address, balance: U256, block_number: u64) {
        let entry = CachedBalance::new(balance, block_number, self.balance_ttl.as_secs());

        self.balance_cache.write().put(address, entry);

        debug!(
            target: "chain::state_cache",
            address = %address,
            balance = %balance,
            block_number = block_number,
            "Balance cached"
        );
    }

    /// Invalidate balance cache for address
    pub fn invalidate_balance(&self, address: &Address) {
        self.balance_cache.write().pop(address);
        debug!(target: "chain::state_cache", address = %address, "Balance cache invalidated");
    }

    /// Invalidate all balances
    pub fn invalidate_all_balances(&self) {
        self.balance_cache.write().clear();
        debug!(target: "chain::state_cache", "All balance caches invalidated");
    }

    // ============================================================================
    // Nonce Cache
    // ============================================================================

    /// Get cached nonce
    pub fn get_nonce(&self, address: &Address) -> Option<(u64, u64, bool)> {
        let mut cache = self.nonce_cache.write();

        if let Some(entry) = cache.get_mut(address) {
            if !entry.is_expired() {
                trace!(
                    target: "chain::state_cache",
                    address = %address,
                    nonce = entry.nonce,
                    is_pending = entry.is_pending,
                    "Cache hit for nonce"
                );
                return Some((entry.nonce, entry.block_number, entry.is_pending));
            }
            // Expired, remove it
            cache.pop(address);
        }

        None
    }

    /// Cache nonce
    pub fn cache_nonce(&self, address: Address, nonce: u64, block_number: u64, is_pending: bool) {
        let ttl = if is_pending {
            self.nonce_ttl.as_secs() / 2 // Shorter TTL for pending nonces
        } else {
            self.nonce_ttl.as_secs()
        };

        let entry = CachedNonce::new(nonce, block_number, ttl, is_pending);

        self.nonce_cache.write().put(address, entry);

        debug!(
            target: "chain::state_cache",
            address = %address,
            nonce = nonce,
            is_pending = is_pending,
            "Nonce cached"
        );
    }

    /// Increment cached nonce for address (used after sending a transaction)
    pub fn increment_nonce(&self, address: &Address) -> Option<u64> {
        let mut cache = self.nonce_cache.write();

        if let Some(entry) = cache.get_mut(address) {
            entry.nonce += 1;
            entry.cached_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            entry.is_pending = true;

            debug!(
                target: "chain::state_cache",
                address = %address,
                new_nonce = entry.nonce,
                "Nonce incremented"
            );

            return Some(entry.nonce);
        }

        None
    }

    /// Invalidate nonce cache for address
    pub fn invalidate_nonce(&self, address: &Address) {
        self.nonce_cache.write().pop(address);
        debug!(target: "chain::state_cache", address = %address, "Nonce cache invalidated");
    }

    /// Invalidate all nonces
    pub fn invalidate_all_nonces(&self) {
        self.nonce_cache.write().clear();
        debug!(target: "chain::state_cache", "All nonce caches invalidated");
    }

    // ============================================================================
    // Code Cache
    // ============================================================================

    /// Get cached code info
    pub fn get_code_info(&self, address: &Address) -> Option<(bool, usize)> {
        let mut cache = self.code_cache.write();

        if let Some(entry) = cache.get_mut(address) {
            if !entry.is_expired() {
                return Some((entry.is_contract, entry.code_size));
            }
            cache.pop(address);
        }

        None
    }

    /// Cache code info
    pub fn cache_code(&self, address: Address, code: &[u8]) {
        let entry = CachedCode::new(code, Duration::from_secs(3600));

        self.code_cache.write().put(address, entry);

        trace!(
            target: "chain::state_cache",
            address = %address,
            code_size = code.len(),
            is_contract = !code.is_empty(),
            "Code cached"
        );
    }

    // ============================================================================
    // Storage Cache
    // ============================================================================

    /// Get cached storage value
    pub fn get_storage(&self, address: &Address, slot: &U256) -> Option<(U256, u64)> {
        let mut cache = self.storage_cache.write();
        let key = (*address, *slot);

        if let Some(entry) = cache.get_mut(&key) {
            if !entry.is_expired() {
                return Some((entry.value, entry.block_number));
            }
            cache.pop(&key);
        }

        None
    }

    /// Cache storage value
    pub fn cache_storage(&self, address: Address, slot: U256, value: U256, block_number: u64) {
        let entry = CachedStorage::new(value, block_number, self.default_ttl);

        self.storage_cache.write().put((address, slot), entry);

        trace!(
            target: "chain::state_cache",
            address = %address,
            slot = %slot,
            value = %value,
            "Storage cached"
        );
    }

    // ============================================================================
    // General Operations
    // ============================================================================

    /// Invalidate all caches for an address
    pub fn invalidate_address(&self, address: &Address) {
        self.invalidate_balance(address);
        self.invalidate_nonce(address);
        self.code_cache.write().pop(address);

        // Clear storage for this address
        let slots_to_remove: Vec<(Address, U256)> = {
            let cache = self.storage_cache.read();
            cache
                .iter()
                .filter(|((addr, _), _)| addr == address)
                .map(|((addr, slot), _)| (*addr, *slot))
                .collect()
        };

        let mut cache = self.storage_cache.write();
        for key in slots_to_remove {
            cache.pop(&key);
        }

        debug!(target: "chain::state_cache", address = %address, "All caches invalidated for address");
    }

    /// Clear all caches
    pub fn clear_all(&self) {
        self.balance_cache.write().clear();
        self.nonce_cache.write().clear();
        self.code_cache.write().clear();
        self.storage_cache.write().clear();

        debug!(target: "chain::state_cache", "All caches cleared");
    }

    /// Get cache statistics
    pub fn get_statistics(&self) -> StateCacheStatistics {
        StateCacheStatistics {
            balance_cache_size: self.balance_cache.read().len(),
            balance_cache_capacity: self.balance_cache.read().cap().get(),
            nonce_cache_size: self.nonce_cache.read().len(),
            nonce_cache_capacity: self.nonce_cache.read().cap().get(),
            code_cache_size: self.code_cache.read().len(),
            code_cache_capacity: self.code_cache.read().cap().get(),
            storage_cache_size: self.storage_cache.read().len(),
            storage_cache_capacity: self.storage_cache.read().cap().get(),
        }
    }
}

impl Default for ChainStateCache {
    fn default() -> Self {
        Self::new()
    }
}

/// State cache statistics
#[derive(Debug, Clone, Copy)]
pub struct StateCacheStatistics {
    pub balance_cache_size: usize,
    pub balance_cache_capacity: usize,
    pub nonce_cache_size: usize,
    pub nonce_cache_capacity: usize,
    pub code_cache_size: usize,
    pub code_cache_capacity: usize,
    pub storage_cache_size: usize,
    pub storage_cache_capacity: usize,
}

impl StateCacheStatistics {
    /// Total cached items
    pub fn total_cached(&self) -> usize {
        self.balance_cache_size
            + self.nonce_cache_size
            + self.code_cache_size
            + self.storage_cache_size
    }

    /// Total capacity
    pub fn total_capacity(&self) -> usize {
        self.balance_cache_capacity
            + self.nonce_cache_capacity
            + self.code_cache_capacity
            + self.storage_cache_capacity
    }

    /// Overall utilization ratio
    pub fn utilization_ratio(&self) -> f64 {
        let total = self.total_capacity();
        if total == 0 {
            return 0.0;
        }
        self.total_cached() as f64 / total as f64
    }
}

/// Cached state provider trait
///
/// This trait can be implemented by providers to integrate with the cache
#[allow(async_fn_in_trait)]
pub trait CachedStateProvider {
    /// Get balance with caching
    async fn get_cached_balance(
        &self,
        cache: &ChainStateCache,
        address: Address,
    ) -> Result<U256, EvmError>;

    /// Get nonce with caching
    async fn get_cached_nonce(
        &self,
        cache: &ChainStateCache,
        address: Address,
    ) -> Result<u64, EvmError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_balance_cache() {
        let cache = ChainStateCache::new();
        let address = Address::ZERO;
        let balance = U256::from(1000);
        let block_number = 100;

        // Initially not cached
        assert!(cache.get_balance(&address).is_none());

        // Cache balance
        cache.cache_balance(address, balance, block_number);

        // Should be retrievable
        let (cached_balance, cached_block) = cache.get_balance(&address).unwrap();
        assert_eq!(cached_balance, balance);
        assert_eq!(cached_block, block_number);

        // Invalidate and check
        cache.invalidate_balance(&address);
        assert!(cache.get_balance(&address).is_none());
    }

    #[test]
    fn test_nonce_cache() {
        let cache = ChainStateCache::new();
        let address = Address::ZERO;
        let nonce = 5;
        let block_number = 100;

        // Cache nonce
        cache.cache_nonce(address, nonce, block_number, false);

        // Should be retrievable
        let (cached_nonce, _, _) = cache.get_nonce(&address).unwrap();
        assert_eq!(cached_nonce, nonce);

        // Increment nonce
        let new_nonce = cache.increment_nonce(&address).unwrap();
        assert_eq!(new_nonce, nonce + 1);

        let (cached_nonce, _, is_pending) = cache.get_nonce(&address).unwrap();
        assert_eq!(cached_nonce, nonce + 1);
        assert!(is_pending);
    }

    #[test]
    fn test_code_cache() {
        let cache = ChainStateCache::new();
        let address = Address::ZERO;
        let code = vec![0x60, 0x80, 0x60, 0x40]; // Simple bytecode

        // Cache code
        cache.cache_code(address, &code);

        // Should be retrievable
        let (is_contract, code_size) = cache.get_code_info(&address).unwrap();
        assert!(is_contract);
        assert_eq!(code_size, code.len());
    }

    #[test]
    fn test_cache_statistics() {
        let cache = ChainStateCache::new();

        // Add some entries
        cache.cache_balance(Address::ZERO, U256::from(1000), 100);
        cache.cache_nonce(Address::ZERO, 5, 100, false);

        let stats = cache.get_statistics();
        assert_eq!(stats.balance_cache_size, 1);
        assert_eq!(stats.nonce_cache_size, 1);
        assert!(stats.utilization_ratio() > 0.0);
    }

    #[test]
    fn test_cached_balance_expiration() {
        let mut config = StateCacheConfig::default();
        // Use 2 seconds TTL to ensure expiration works correctly
        config.balance_ttl = Duration::from_secs(2);

        let cache = ChainStateCache::with_config(config);
        let address = Address::ZERO;

        cache.cache_balance(address, U256::from(1000), 100);

        // Should be available immediately
        assert!(cache.get_balance(&address).is_some());

        // Wait for expiration (sleep 2.5 seconds to ensure we cross 2 second
        // boundaries)
        std::thread::sleep(Duration::from_millis(2500));

        // Should be expired
        assert!(cache.get_balance(&address).is_none());
    }
}
