//! LLM Provider Failover Module
//!
//! ARCHITECTURE FIX: Implements provider failover with automatic chain-based
//! fallback. When the primary provider fails, requests are automatically routed
//! to secondary providers.

use std::sync::Arc;

use tokio::time::{timeout, Duration};
use tracing::{info, warn};

use super::traits::{LLMProvider, ProviderCapabilities};
use super::types::{LLMError, LLMRequest, LLMResponse, LLMResult};

/// Provider with health status
#[derive(Clone)]
struct ProviderEntry {
    provider: Arc<dyn LLMProvider>,
    name: String,
    healthy: bool,
    consecutive_failures: u32,
}

impl std::fmt::Debug for ProviderEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderEntry")
            .field("name", &self.name)
            .field("healthy", &self.healthy)
            .field("consecutive_failures", &self.consecutive_failures)
            .finish_non_exhaustive()
    }
}

/// Failover provider configuration
#[derive(Debug, Clone)]
pub struct FailoverConfig {
    /// Timeout for each provider attempt
    pub attempt_timeout_secs: u64,
    /// Max consecutive failures before marking unhealthy
    pub max_failures: u32,
    /// Retry interval for unhealthy providers (seconds)
    pub health_check_interval_secs: u64,
    /// Enable circuit breaker pattern
    pub enable_circuit_breaker: bool,
}

impl Default for FailoverConfig {
    fn default() -> Self {
        Self {
            attempt_timeout_secs: 30,
            max_failures: 3,
            health_check_interval_secs: 60,
            enable_circuit_breaker: true,
        }
    }
}

/// Multi-provider LLM client with automatic failover
///
/// ARCHITECTURE FIX: Chains multiple providers and automatically fails over
/// when the primary provider is unavailable or returns errors.
pub struct FailoverProvider {
    providers: tokio::sync::RwLock<Vec<ProviderEntry>>,
    config: FailoverConfig,
}

impl FailoverProvider {
    /// Create new failover provider with primary and fallback providers
    pub fn new(primary: Arc<dyn LLMProvider>, fallback: Vec<Arc<dyn LLMProvider>>) -> Self {
        let mut providers = vec![ProviderEntry {
            provider: primary,
            name: "primary".to_string(),
            healthy: true,
            consecutive_failures: 0,
        }];

        for (i, provider) in fallback.into_iter().enumerate() {
            providers.push(ProviderEntry {
                provider,
                name: format!("fallback-{}", i + 1),
                healthy: true,
                consecutive_failures: 0,
            });
        }

        Self {
            providers: tokio::sync::RwLock::new(providers),
            config: FailoverConfig::default(),
        }
    }

    /// Create with custom config
    pub fn with_config(mut self, config: FailoverConfig) -> Self {
        self.config = config;
        self
    }

    /// Add a provider to the chain
    pub async fn add_provider(&self, name: impl Into<String>, provider: Arc<dyn LLMProvider>) {
        let mut providers = self.providers.write().await;
        providers.push(ProviderEntry {
            provider,
            name: name.into(),
            healthy: true,
            consecutive_failures: 0,
        });
    }

    /// Get current provider status
    pub async fn get_provider_status(&self) -> Vec<(String, bool, u32)> {
        let providers = self.providers.read().await;
        providers
            .iter()
            .map(|p| (p.name.clone(), p.healthy, p.consecutive_failures))
            .collect()
    }

    /// Mark provider as healthy/unhealthy
    async fn update_provider_health(&self, index: usize, success: bool) {
        let mut providers = self.providers.write().await;
        if let Some(entry) = providers.get_mut(index) {
            if success {
                entry.consecutive_failures = 0;
                if !entry.healthy {
                    entry.healthy = true;
                    info!("Provider {} is now healthy", entry.name);
                }
            } else {
                entry.consecutive_failures += 1;
                if entry.consecutive_failures >= self.config.max_failures {
                    entry.healthy = false;
                    warn!(
                        "Provider {} marked unhealthy after {} consecutive failures",
                        entry.name, entry.consecutive_failures
                    );
                }
            }
        }
    }

    /// Try to complete request with failover
    async fn try_complete(&self, request: LLMRequest) -> LLMResult<LLMResponse> {
        let provider_count = self.providers.read().await.len();

        for index in 0..provider_count {
            // Get provider info
            let (provider_name, provider, _is_healthy) = {
                let providers = self.providers.read().await;
                if index >= providers.len() {
                    break;
                }
                let entry = &providers[index];
                // Skip unhealthy providers if circuit breaker is enabled
                if self.config.enable_circuit_breaker && !entry.healthy {
                    continue;
                }
                (entry.name.clone(), entry.provider.clone(), entry.healthy)
            };

            // Try with timeout
            let attempt_timeout = Duration::from_secs(self.config.attempt_timeout_secs);
            info!("[LLM-TRACE] Trying provider {} with timeout {}s", provider_name, self.config.attempt_timeout_secs);
            let provider_start = std::time::Instant::now();
            let result = timeout(attempt_timeout, provider.complete(request.clone())).await;
            let provider_elapsed = provider_start.elapsed();

            match result {
                Ok(Ok(response)) => {
                    info!("[LLM-TRACE] Provider {} succeeded in {:?}", provider_name, provider_elapsed);
                    // Success - mark provider healthy
                    self.update_provider_health(index, true).await;
                    info!("Request succeeded with provider {}", provider_name);
                    return Ok(response);
                }
                Ok(Err(e)) => {
                    // Provider error - mark failure and continue to next
                    warn!("[LLM-TRACE] Provider {} failed after {:?}: {}", provider_name, provider_elapsed, e);
                    self.update_provider_health(index, false).await;
                }
                Err(_) => {
                    // Timeout - mark failure and continue
                    warn!("[LLM-TRACE] Provider {} timed out after {:?} (limit: {}s)", provider_name, provider_elapsed, self.config.attempt_timeout_secs);
                    self.update_provider_health(index, false).await;
                }
            }

            if index < provider_count - 1 {
                info!("Failing over to next provider");
            }
        }

        Err(LLMError::Provider(
            "All providers failed or are unavailable".to_string(),
        ))
    }
}

#[async_trait::async_trait]
impl LLMProvider for FailoverProvider {
    async fn complete(&self, request: LLMRequest) -> LLMResult<LLMResponse> {
        self.try_complete(request).await
    }

    async fn complete_stream(
        &self,
        request: LLMRequest,
    ) -> LLMResult<tokio::sync::mpsc::Receiver<super::types::StreamChunk>> {
        // QUALITY FIX: Streaming failover - try providers until one succeeds
        // Note: This is a simplified implementation that tries providers sequentially.
        // Full streaming failover with mid-stream switching is more complex and
        // would require a streaming aggregation layer.

        let provider_count = self.providers.read().await.len();

        for index in 0..provider_count {
            // Get provider info
            let (provider, provider_name) = {
                let providers = self.providers.read().await;
                if index >= providers.len() {
                    break;
                }
                let entry = &providers[index];
                // Skip unhealthy providers if circuit breaker is enabled
                if self.config.enable_circuit_breaker && !entry.healthy {
                    continue;
                }
                (entry.provider.clone(), entry.name.clone())
            };

            // Try streaming with this provider
            match provider.complete_stream(request.clone()).await {
                Ok(receiver) => {
                    // Success - mark provider healthy
                    self.update_provider_health(index, true).await;
                    info!(
                        "Streaming request succeeded with provider {}",
                        provider_name
                    );
                    return Ok(receiver);
                }
                Err(e) => {
                    // Provider failed - mark failure and continue to next
                    warn!("Provider {} failed for streaming: {}", provider_name, e);
                    self.update_provider_health(index, false).await;
                }
            }

            if index < provider_count - 1 {
                info!("Failing over streaming to next provider");
            }
        }

        Err(LLMError::Provider(
            "All providers failed for streaming request".to_string(),
        ))
    }

    fn capabilities(&self) -> ProviderCapabilities {
        // Return capabilities of the first healthy provider
        // This is a simplified approach - in production, you'd aggregate capabilities
        let providers = self.providers.try_read();
        if let Ok(providers) = providers {
            for entry in providers.iter() {
                if entry.healthy {
                    return entry.provider.capabilities();
                }
            }
        }
        ProviderCapabilities::default()
    }

    async fn health_check(&self) -> LLMResult<()> {
        let providers = self.providers.read().await;
        let mut any_healthy = false;

        for entry in providers.iter() {
            if entry.provider.health_check().await.is_ok() {
                any_healthy = true;
                break;
            }
        }

        if any_healthy {
            Ok(())
        } else {
            Err(LLMError::Provider(
                "No healthy providers available".to_string(),
            ))
        }
    }

    fn name(&self) -> &str {
        "failover"
    }

    async fn list_models(&self) -> LLMResult<Vec<super::traits::ModelInfo>> {
        // Aggregate models from all providers
        let providers = self.providers.read().await;
        let mut all_models = Vec::new();

        for entry in providers.iter() {
            if let Ok(models) = entry.provider.list_models().await {
                all_models.extend(models);
            }
        }

        Ok(all_models)
    }
}

/// Builder for FailoverProvider
pub struct FailoverProviderBuilder {
    primary: Option<Arc<dyn LLMProvider>>,
    fallbacks: Vec<Arc<dyn LLMProvider>>,
    config: FailoverConfig,
}

impl FailoverProviderBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            primary: None,
            fallbacks: Vec::new(),
            config: FailoverConfig::default(),
        }
    }

    /// Set primary provider
    pub fn primary(mut self, provider: Arc<dyn LLMProvider>) -> Self {
        self.primary = Some(provider);
        self
    }

    /// Add fallback provider
    pub fn fallback(mut self, provider: Arc<dyn LLMProvider>) -> Self {
        self.fallbacks.push(provider);
        self
    }

    /// Set timeout per attempt
    pub fn timeout_secs(mut self, secs: u64) -> Self {
        self.config.attempt_timeout_secs = secs;
        self
    }

    /// Set max failures before circuit breaker
    pub fn max_failures(mut self, max: u32) -> Self {
        self.config.max_failures = max;
        self
    }

    /// Build the provider
    pub fn build(self) -> LLMResult<FailoverProvider> {
        let primary = self
            .primary
            .ok_or_else(|| LLMError::InvalidRequest("Primary provider required".to_string()))?;

        Ok(FailoverProvider::new(primary, self.fallbacks).with_config(self.config))
    }
}

impl Default for FailoverProviderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests would require mock providers
    // For now, we just verify the structure compiles

    #[test]
    fn test_failover_config_default() {
        let config = FailoverConfig::default();
        assert_eq!(config.attempt_timeout_secs, 30);
        assert_eq!(config.max_failures, 3);
        assert!(config.enable_circuit_breaker);
    }
}
