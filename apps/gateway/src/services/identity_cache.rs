//! Identity Cache Module
//!
//! Provides caching for frequently accessed identity information.
//! Reduces RPC calls and improves response times.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use beebotos_chain::compat::AgentIdentityInfo;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Cache entry with expiration
#[derive(Debug, Clone)]
struct CacheEntry<T> {
    data: T,
    expires_at: Instant,
}

impl<T> CacheEntry<T> {
    fn new(data: T, ttl: Duration) -> Self {
        Self {
            data,
            expires_at: Instant::now() + ttl,
        }
    }

    fn is_expired(&self) -> bool {
        Instant::now() > self.expires_at
    }
}

/// Identity cache configuration
#[derive(Debug, Clone)]
pub struct IdentityCacheConfig {
    /// Default TTL for cache entries
    pub default_ttl: Duration,
    /// TTL for active identities (shorter, as they may change)
    pub active_identity_ttl: Duration,
    /// Maximum cache size
    pub max_size: usize,
    /// Cleanup interval
    pub cleanup_interval: Duration,
}

impl Default for IdentityCacheConfig {
    fn default() -> Self {
        Self {
            default_ttl: Duration::from_secs(300),        // 5 minutes
            active_identity_ttl: Duration::from_secs(60), // 1 minute
            max_size: 10000,
            cleanup_interval: Duration::from_secs(60), // 1 minute
        }
    }
}

/// Identity information cache
pub struct IdentityCache {
    config: IdentityCacheConfig,
    /// Agent ID -> Identity info cache
    identity_by_id: Arc<RwLock<HashMap<String, CacheEntry<AgentIdentityInfo>>>>,
    /// DID -> Agent ID cache
    agent_id_by_did: Arc<RwLock<HashMap<String, CacheEntry<String>>>>,
    /// Registration status cache
    registration_status: Arc<RwLock<HashMap<String, CacheEntry<bool>>>>,
    /// Cache hits/misses statistics
    stats: Arc<RwLock<CacheStats>>,
}

/// Cache statistics
#[derive(Debug, Default)]
pub struct CacheStats {
    pub identity_hits: u64,
    pub identity_misses: u64,
    pub did_hits: u64,
    pub did_misses: u64,
    pub status_hits: u64,
    pub status_misses: u64,
    pub evictions: u64,
}

impl IdentityCache {
    /// Create new identity cache
    pub fn new(config: IdentityCacheConfig) -> Self {
        let cache = Self {
            config,
            identity_by_id: Arc::new(RwLock::new(HashMap::new())),
            agent_id_by_did: Arc::new(RwLock::new(HashMap::new())),
            registration_status: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(CacheStats::default())),
        };

        // Start cleanup task
        cache.start_cleanup_task();

        cache
    }

    /// Create with default configuration
    #[allow(dead_code)]
    pub fn default() -> Self {
        Self::new(IdentityCacheConfig::default())
    }

    /// Get identity by agent ID
    pub async fn get_identity(&self, agent_id: &str) -> Option<AgentIdentityInfo> {
        let cache = self.identity_by_id.read().await;

        if let Some(entry) = cache.get(agent_id) {
            if !entry.is_expired() {
                // Cache hit
                let data = entry.data.clone();
                drop(cache);

                let mut stats = self.stats.write().await;
                stats.identity_hits += 1;
                drop(stats);

                debug!(agent_id = %agent_id, "Identity cache hit");
                return Some(data);
            }
        }

        // Cache miss
        drop(cache);
        let mut stats = self.stats.write().await;
        stats.identity_misses += 1;
        drop(stats);

        None
    }

    /// Put identity into cache
    pub async fn put_identity(&self, agent_id: &str, identity: AgentIdentityInfo) {
        let ttl = if identity.is_active {
            self.config.active_identity_ttl
        } else {
            self.config.default_ttl
        };

        let mut cache = self.identity_by_id.write().await;

        // Check cache size limit
        if cache.len() >= self.config.max_size && !cache.contains_key(agent_id) {
            // Evict oldest entry (simple FIFO for now)
            if let Some(first_key) = cache.keys().next().cloned() {
                cache.remove(&first_key);
                let mut stats = self.stats.write().await;
                stats.evictions += 1;
            }
        }

        cache.insert(agent_id.to_string(), CacheEntry::new(identity, ttl));
        debug!(agent_id = %agent_id, "Identity cached");
    }

    /// Invalidate identity cache entry
    pub async fn invalidate_identity(&self, agent_id: &str) {
        let mut cache = self.identity_by_id.write().await;
        cache.remove(agent_id);
        debug!(agent_id = %agent_id, "Identity cache invalidated");
    }

    /// Get agent ID by DID
    pub async fn get_agent_id_by_did(&self, did: &str) -> Option<String> {
        let cache = self.agent_id_by_did.read().await;

        if let Some(entry) = cache.get(did) {
            if !entry.is_expired() {
                let data = entry.data.clone();
                drop(cache);

                let mut stats = self.stats.write().await;
                stats.did_hits += 1;
                drop(stats);

                debug!(did = %did, "DID cache hit");
                return Some(data);
            }
        }

        drop(cache);
        let mut stats = self.stats.write().await;
        stats.did_misses += 1;
        drop(stats);

        None
    }

    /// Put DID mapping into cache
    pub async fn put_agent_id_by_did(&self, did: &str, agent_id: &str) {
        let mut cache = self.agent_id_by_did.write().await;

        cache.insert(
            did.to_string(),
            CacheEntry::new(agent_id.to_string(), self.config.default_ttl),
        );

        debug!(did = %did, agent_id = %agent_id, "DID mapping cached");
    }

    /// Get registration status
    pub async fn get_registration_status(&self, agent_id: &str) -> Option<bool> {
        let cache = self.registration_status.read().await;

        if let Some(entry) = cache.get(agent_id) {
            if !entry.is_expired() {
                let data = entry.data;
                drop(cache);

                let mut stats = self.stats.write().await;
                stats.status_hits += 1;
                drop(stats);

                debug!(agent_id = %agent_id, "Registration status cache hit");
                return Some(data);
            }
        }

        drop(cache);
        let mut stats = self.stats.write().await;
        stats.status_misses += 1;
        drop(stats);

        None
    }

    /// Put registration status into cache
    pub async fn put_registration_status(&self, agent_id: &str, is_registered: bool) {
        let mut cache = self.registration_status.write().await;

        cache.insert(
            agent_id.to_string(),
            CacheEntry::new(is_registered, self.config.active_identity_ttl),
        );

        debug!(agent_id = %agent_id, is_registered = %is_registered, "Registration status cached");
    }

    /// Get cache statistics
    pub async fn get_stats(&self) -> CacheStats {
        let stats = self.stats.read().await;
        CacheStats {
            identity_hits: stats.identity_hits,
            identity_misses: stats.identity_misses,
            did_hits: stats.did_hits,
            did_misses: stats.did_misses,
            status_hits: stats.status_hits,
            status_misses: stats.status_misses,
            evictions: stats.evictions,
        }
    }

    /// Get cache hit rate
    #[allow(dead_code)]
    pub async fn get_hit_rate(&self) -> f64 {
        let stats = self.stats.read().await;
        let total_identity = stats.identity_hits + stats.identity_misses;
        let total_did = stats.did_hits + stats.did_misses;
        let total_status = stats.status_hits + stats.status_misses;

        let total_hits = stats.identity_hits + stats.did_hits + stats.status_hits;
        let total_requests = total_identity + total_did + total_status;

        if total_requests == 0 {
            0.0
        } else {
            (total_hits as f64 / total_requests as f64) * 100.0
        }
    }

    /// Clear all caches
    pub async fn clear(&self) {
        let mut identity_cache = self.identity_by_id.write().await;
        let mut did_cache = self.agent_id_by_did.write().await;
        let mut status_cache = self.registration_status.write().await;
        let mut stats = self.stats.write().await;

        identity_cache.clear();
        did_cache.clear();
        status_cache.clear();
        *stats = CacheStats::default();

        info!("Identity cache cleared");
    }

    /// Start background cleanup task
    fn start_cleanup_task(&self) {
        let identity_cache = Arc::clone(&self.identity_by_id);
        let did_cache = Arc::clone(&self.agent_id_by_did);
        let status_cache = Arc::clone(&self.registration_status);
        let interval = self.config.cleanup_interval;

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;

                // Clean expired entries
                let mut removed = 0usize;

                {
                    let mut cache = identity_cache.write().await;
                    let before = cache.len();
                    cache.retain(|_, entry| !entry.is_expired());
                    removed += before - cache.len();
                }

                {
                    let mut cache = did_cache.write().await;
                    let before = cache.len();
                    cache.retain(|_, entry| !entry.is_expired());
                    removed += before - cache.len();
                }

                {
                    let mut cache = status_cache.write().await;
                    let before = cache.len();
                    cache.retain(|_, entry| !entry.is_expired());
                    removed += before - cache.len();
                }

                if removed > 0 {
                    debug!(removed = %removed, "Cleaned up expired cache entries");
                }
            }
        });
    }
}

/// Cached identity service wrapper
#[allow(dead_code)]
pub struct CachedIdentityService<S> {
    #[allow(dead_code)]
    inner: S,
    #[allow(dead_code)]
    cache: IdentityCache,
}

#[allow(dead_code)]
impl<S> CachedIdentityService<S> {
    /// Create new cached service
    pub fn new(inner: S, cache: IdentityCache) -> Self {
        Self { inner, cache }
    }

    /// Get underlying service
    #[allow(dead_code)]
    pub fn inner(&self) -> &S {
        &self.inner
    }

    /// Get cache reference
    #[allow(dead_code)]
    pub fn cache(&self) -> &IdentityCache {
        &self.cache
    }
}

#[cfg(test)]
mod tests {
    use beebotos_chain::compat::U256;

    use super::*;

    fn create_test_identity() -> AgentIdentityInfo {
        AgentIdentityInfo {
            agent_id: [1u8; 32],
            owner: beebotos_core::Address::from_slice(&[1u8; 20]),
            did: "did:beebotos:test".to_string(),
            public_key: [2u8; 32],
            is_active: true,
            reputation: U256::from(100),
            created_at: U256::from(1234567890u64),
        }
    }

    #[tokio::test]
    async fn test_cache_put_and_get() {
        let cache = IdentityCache::default();
        let identity = create_test_identity();

        cache.put_identity("agent-1", identity.clone()).await;

        let cached = cache.get_identity("agent-1").await;
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().did, identity.did);
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let cache = IdentityCache::default();

        let cached = cache.get_identity("non-existent").await;
        assert!(cached.is_none());

        let stats = cache.get_stats().await;
        assert_eq!(stats.identity_misses, 1);
    }

    #[tokio::test]
    async fn test_cache_invalidation() {
        let cache = IdentityCache::default();
        let identity = create_test_identity();

        cache.put_identity("agent-1", identity).await;
        cache.invalidate_identity("agent-1").await;

        let cached = cache.get_identity("agent-1").await;
        assert!(cached.is_none());
    }

    #[tokio::test]
    async fn test_cache_stats() {
        let cache = IdentityCache::default();
        let identity = create_test_identity();

        // Cache miss
        let _ = cache.get_identity("agent-1").await;

        // Cache put
        cache.put_identity("agent-1", identity).await;

        // Cache hit
        let _ = cache.get_identity("agent-1").await;

        let stats = cache.get_stats().await;
        assert_eq!(stats.identity_hits, 1);
        assert_eq!(stats.identity_misses, 1);
    }
}
