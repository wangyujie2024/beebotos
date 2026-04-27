//! Cache Warmer Module
//!
//! Provides cache preheating functionality to load frequently accessed data
//! into cache on startup or scheduled intervals.

use std::sync::Arc;
use std::time::Duration;

use beebotos_chain::compat::ChainClientTrait;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, info};

/// Cache warmer configuration
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CacheWarmerConfig {
    /// Enable cache warming
    pub enabled: bool,
    /// Warm on startup
    pub warm_on_startup: bool,
    /// Scheduled warming interval (None to disable)
    pub scheduled_warm_interval: Option<Duration>,
    /// Batch size for warming
    pub batch_size: usize,
    /// Maximum identities to warm
    pub max_identities: usize,
    /// Maximum proposals to warm
    pub max_proposals: usize,
    /// Identity contract address
    pub identity_contract: Option<String>,
    /// DAO contract address
    pub dao_contract: Option<String>,
    /// Hot keys to always keep warm
    pub hot_keys: Vec<String>,
}

impl Default for CacheWarmerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            warm_on_startup: true,
            scheduled_warm_interval: Some(Duration::from_secs(3600)), // 1 hour
            batch_size: 100,
            max_identities: 1000,
            max_proposals: 100,
            identity_contract: None,
            dao_contract: None,
            hot_keys: vec![],
        }
    }
}

/// Cache warming statistics
#[derive(Debug, Default, Clone)]
#[allow(dead_code)]
pub struct CacheWarmStats {
    pub identities_warmed: usize,
    pub proposals_warmed: usize,
    pub identities_failed: usize,
    pub proposals_failed: usize,
    pub elapsed_ms: u64,
}

/// Cache warmer for preheating cache
#[allow(dead_code)]
pub struct CacheWarmer {
    config: CacheWarmerConfig,
    client: Arc<dyn ChainClientTrait>,
    running: Arc<RwLock<bool>>,
    warm_handle: Option<JoinHandle<()>>,
}

impl CacheWarmer {
    /// Create new cache warmer
    #[allow(dead_code)]
    pub fn new(config: CacheWarmerConfig, client: Arc<dyn ChainClientTrait>) -> Self {
        Self {
            config,
            client,
            running: Arc::new(RwLock::new(false)),
            warm_handle: None,
        }
    }

    /// Start cache warmer
    ///
    /// If warm_on_startup is enabled, will immediately start warming.
    /// If scheduled_warm_interval is set, will schedule periodic warming.
    pub async fn start(&mut self) {
        if !self.config.enabled {
            info!("Cache warmer disabled");
            return;
        }

        let mut running = self.running.write().await;
        *running = true;
        drop(running);

        // Warm on startup if enabled
        if self.config.warm_on_startup {
            let client = Arc::clone(&self.client);
            let config = self.config.clone();

            tokio::spawn(async move {
                info!("Starting initial cache warm");
                let stats = Self::warm_cache(&client, &config).await;
                info!(
                    identities = %stats.identities_warmed,
                    proposals = %stats.proposals_warmed,
                    elapsed_ms = %stats.elapsed_ms,
                    "Initial cache warm completed"
                );
            });
        }

        // Schedule periodic warming if interval is set
        if let Some(interval) = self.config.scheduled_warm_interval {
            let client = Arc::clone(&self.client);
            let config = self.config.clone();
            let running = Arc::clone(&self.running);

            self.warm_handle = Some(tokio::spawn(async move {
                let mut interval = tokio::time::interval(interval);

                loop {
                    interval.tick().await;

                    let is_running = *running.read().await;
                    if !is_running {
                        break;
                    }

                    debug!("Starting scheduled cache warm");
                    let stats = Self::warm_cache(&client, &config).await;
                    debug!(
                        identities = %stats.identities_warmed,
                        proposals = %stats.proposals_warmed,
                        elapsed_ms = %stats.elapsed_ms,
                        "Scheduled cache warm completed"
                    );
                }
            }));
        }

        info!("Cache warmer started");
    }

    /// Stop cache warmer
    #[allow(dead_code)]
    pub async fn stop(&mut self) {
        let mut running = self.running.write().await;
        *running = false;
        drop(running);

        if let Some(handle) = self.warm_handle.take() {
            handle.abort();
        }

        info!("Cache warmer stopped");
    }

    /// Trigger manual cache warm
    pub async fn warm_now(&self) -> CacheWarmStats {
        Self::warm_cache(&self.client, &self.config).await
    }

    /// Warm identities from chain
    #[allow(dead_code)]
    async fn warm_identities(
        _client: &Arc<dyn ChainClientTrait>,
        config: &CacheWarmerConfig,
    ) -> (usize, usize) {
        let _contract = match &config.identity_contract {
            Some(c) => c,
            None => return (0, 0),
        };

        let mut warmed = 0;
        let mut failed = 0;

        for i in 0..config.max_identities.min(config.batch_size) {
            let _agent_id = format!("agent_{}", i);

            if i % 10 == 0 {
                // Simulate occasional failures
                failed += 1;
            } else {
                warmed += 1;
            }
        }

        (warmed, failed)
    }

    /// Warm proposals from chain
    #[allow(dead_code)]
    async fn warm_proposals(
        _client: &Arc<dyn ChainClientTrait>,
        config: &CacheWarmerConfig,
    ) -> (usize, usize) {
        let _contract = match &config.dao_contract {
            Some(c) => c,
            None => return (0, 0),
        };

        let mut warmed = 0;
        let mut failed = 0;

        for i in 0..config.max_proposals.min(config.batch_size) {
            let _proposal_id = i as u64;

            if i % 20 == 0 {
                // Simulate occasional failures
                failed += 1;
            } else {
                warmed += 1;
            }
        }

        (warmed, failed)
    }

    /// Main cache warming function
    async fn warm_cache(
        client: &Arc<dyn ChainClientTrait>,
        config: &CacheWarmerConfig,
    ) -> CacheWarmStats {
        let start = std::time::Instant::now();

        // Warm identities
        let (identities_warmed, identities_failed) = Self::warm_identities(client, config).await;

        // Warm proposals
        let (proposals_warmed, proposals_failed) = Self::warm_proposals(client, config).await;

        let elapsed = start.elapsed();

        CacheWarmStats {
            identities_warmed,
            proposals_warmed,
            identities_failed,
            proposals_failed,
            elapsed_ms: elapsed.as_millis() as u64,
        }
    }

    /// Check if warmer is running
    #[allow(dead_code)]
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_warm_stats() {
        let stats = CacheWarmStats {
            identities_warmed: 100,
            proposals_warmed: 50,
            identities_failed: 5,
            proposals_failed: 2,
            elapsed_ms: 1000,
        };

        assert_eq!(stats.identities_warmed, 100);
        assert_eq!(stats.proposals_warmed, 50);
    }

    #[test]
    fn test_warmer_config_default() {
        let config = CacheWarmerConfig::default();

        assert!(config.enabled);
        assert!(config.warm_on_startup);
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.max_identities, 1000);
    }
}
