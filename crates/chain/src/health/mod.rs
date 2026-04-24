//! Health Check Module
//!
//! Provides health check endpoints and system status monitoring for the chain
//! module.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use alloy_provider::Provider as AlloyProvider;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{error, info, instrument, warn};

use crate::compat::Address;
use crate::constants::{HEALTH_CHECK_RPC_TIMEOUT, MAX_BLOCK_AGE_SECS, MAX_SYNC_LAG_BLOCKS};

/// Health status levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Degraded => write!(f, "degraded"),
            HealthStatus::Unhealthy => write!(f, "unhealthy"),
        }
    }
}

/// Component health information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub name: String,
    pub status: HealthStatus,
    pub latency_ms: u64,
    pub last_check: u64,
    pub message: Option<String>,
}

/// Overall health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResponse {
    pub status: HealthStatus,
    pub timestamp: u64,
    pub version: String,
    pub components: Vec<ComponentHealth>,
    pub uptime_seconds: u64,
}

/// Health checker for chain components
pub struct HealthChecker {
    start_time: Instant,
    components: Arc<RwLock<HashMap<String, ComponentHealth>>>,
    config: HealthCheckConfig,
}

/// Health check configuration
#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    pub rpc_timeout: Duration,
    pub max_block_age_secs: u64,
    pub max_sync_lag: u64,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            rpc_timeout: HEALTH_CHECK_RPC_TIMEOUT,
            max_block_age_secs: MAX_BLOCK_AGE_SECS,
            max_sync_lag: MAX_SYNC_LAG_BLOCKS,
        }
    }
}

impl HealthChecker {
    /// Create new health checker
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            components: Arc::new(RwLock::new(HashMap::new())),
            config: HealthCheckConfig::default(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: HealthCheckConfig) -> Self {
        Self {
            start_time: Instant::now(),
            components: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Get uptime in seconds
    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Check RPC provider health
    #[instrument(skip(self, provider), target = "chain::health")]
    pub async fn check_rpc_health<P: AlloyProvider>(&self, provider: Arc<P>) -> ComponentHealth {
        let start = Instant::now();
        let name = "rpc_provider".to_string();

        match tokio::time::timeout(self.config.rpc_timeout, provider.get_block_number()).await {
            Ok(Ok(_block_number)) => {
                let latency = start.elapsed().as_millis() as u64;
                ComponentHealth {
                    name,
                    status: HealthStatus::Healthy,
                    latency_ms: latency,
                    last_check: current_timestamp(),
                    message: Some("RPC responsive".to_string()),
                }
            }
            Ok(Err(e)) => {
                let latency = start.elapsed().as_millis() as u64;
                error!(target: "chain::health", error = %e, "RPC provider error");
                ComponentHealth {
                    name,
                    status: HealthStatus::Unhealthy,
                    latency_ms: latency,
                    last_check: current_timestamp(),
                    message: Some(format!("RPC error: {}", e)),
                }
            }
            Err(_) => {
                error!(target: "chain::health", "RPC timeout");
                ComponentHealth {
                    name,
                    status: HealthStatus::Unhealthy,
                    latency_ms: self.config.rpc_timeout.as_millis() as u64,
                    last_check: current_timestamp(),
                    message: Some("RPC timeout".to_string()),
                }
            }
        }
    }

    /// Check contract health
    #[instrument(skip(self, provider), target = "chain::health")]
    pub async fn check_contract_health<P: AlloyProvider>(
        &self,
        provider: Arc<P>,
        contract_address: Address,
        contract_name: &str,
    ) -> ComponentHealth {
        let start = Instant::now();
        let name = format!("contract_{}", contract_name);

        // Check if contract has code (is deployed)
        match tokio::time::timeout(
            self.config.rpc_timeout,
            provider.get_code_at(contract_address),
        )
        .await
        {
            Ok(Ok(code)) => {
                let latency = start.elapsed().as_millis() as u64;
                if code.len() > 2 {
                    ComponentHealth {
                        name,
                        status: HealthStatus::Healthy,
                        latency_ms: latency,
                        last_check: current_timestamp(),
                        message: Some(format!("{} contract deployed", contract_name)),
                    }
                } else {
                    warn!(target: "chain::health", %contract_address, "{} contract not deployed", contract_name);
                    ComponentHealth {
                        name,
                        status: HealthStatus::Unhealthy,
                        latency_ms: latency,
                        last_check: current_timestamp(),
                        message: Some(format!("{} contract not deployed", contract_name)),
                    }
                }
            }
            Ok(Err(e)) => {
                let latency = start.elapsed().as_millis() as u64;
                error!(target: "chain::health", error = %e, "{} contract check failed", contract_name);
                ComponentHealth {
                    name,
                    status: HealthStatus::Unhealthy,
                    latency_ms: latency,
                    last_check: current_timestamp(),
                    message: Some(format!("Contract check failed: {}", e)),
                }
            }
            Err(_) => {
                error!(target: "chain::health", "{} contract check timeout", contract_name);
                ComponentHealth {
                    name,
                    status: HealthStatus::Unhealthy,
                    latency_ms: self.config.rpc_timeout.as_millis() as u64,
                    last_check: current_timestamp(),
                    message: Some("Contract check timeout".to_string()),
                }
            }
        }
    }

    /// Check sync status
    #[instrument(skip(self, provider), target = "chain::health")]
    pub async fn check_sync_status<P: AlloyProvider>(&self, provider: Arc<P>) -> ComponentHealth {
        let start = Instant::now();
        let name = "sync_status".to_string();

        match tokio::time::timeout(self.config.rpc_timeout, provider.get_block_number()).await {
            Ok(Ok(block_number)) => {
                let latency = start.elapsed().as_millis() as u64;

                // Get block timestamp to check age
                match tokio::time::timeout(
                    self.config.rpc_timeout,
                    provider.get_block_by_number(
                        alloy_rpc_types::BlockNumberOrTag::Latest,
                        alloy_rpc_types::BlockTransactionsKind::Hashes,
                    ),
                )
                .await
                {
                    Ok(Ok(Some(block))) => {
                        let now = current_timestamp();
                        let block_time = block.header.timestamp;
                        let age_secs = now.saturating_sub(block_time);

                        if age_secs > self.config.max_block_age_secs {
                            warn!(
                                target: "chain::health",
                                age_secs,
                                "Node appears out of sync"
                            );
                            ComponentHealth {
                                name,
                                status: HealthStatus::Degraded,
                                latency_ms: latency,
                                last_check: current_timestamp(),
                                message: Some(format!(
                                    "Block age: {}s (max: {}s)",
                                    age_secs, self.config.max_block_age_secs
                                )),
                            }
                        } else {
                            ComponentHealth {
                                name,
                                status: HealthStatus::Healthy,
                                latency_ms: latency,
                                last_check: current_timestamp(),
                                message: Some(format!(
                                    "Synced at block {} (age: {}s)",
                                    block_number, age_secs
                                )),
                            }
                        }
                    }
                    _ => ComponentHealth {
                        name,
                        status: HealthStatus::Degraded,
                        latency_ms: latency,
                        last_check: current_timestamp(),
                        message: Some(format!("Synced at block {}", block_number)),
                    },
                }
            }
            Ok(Err(e)) => {
                let latency = start.elapsed().as_millis() as u64;
                error!(target: "chain::health", error = %e, "Sync check failed");
                ComponentHealth {
                    name,
                    status: HealthStatus::Unhealthy,
                    latency_ms: latency,
                    last_check: current_timestamp(),
                    message: Some(format!("Sync check failed: {}", e)),
                }
            }
            Err(_) => {
                error!(target: "chain::health", "Sync check timeout");
                ComponentHealth {
                    name,
                    status: HealthStatus::Unhealthy,
                    latency_ms: self.config.rpc_timeout.as_millis() as u64,
                    last_check: current_timestamp(),
                    message: Some("Sync check timeout".to_string()),
                }
            }
        }
    }

    /// Perform full health check
    #[instrument(skip(self, provider), target = "chain::health")]
    pub async fn check_health<P: AlloyProvider>(
        &self,
        provider: Arc<P>,
        contracts: Vec<(Address, String)>,
    ) -> HealthCheckResponse {
        let mut components = Vec::new();

        // Check RPC
        components.push(self.check_rpc_health(provider.clone()).await);

        // Check sync status
        components.push(self.check_sync_status(provider.clone()).await);

        // Check contracts
        for (address, name) in contracts {
            components.push(
                self.check_contract_health(provider.clone(), address, &name)
                    .await,
            );
        }

        // Determine overall status
        let status = if components
            .iter()
            .any(|c| c.status == HealthStatus::Unhealthy)
        {
            HealthStatus::Unhealthy
        } else if components
            .iter()
            .any(|c| c.status == HealthStatus::Degraded)
        {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };

        // Store component statuses
        {
            let mut stored = self.components.write().await;
            for component in &components {
                stored.insert(component.name.clone(), component.clone());
            }
        }

        info!(
            target: "chain::health",
            status = %status,
            component_count = components.len(),
            "Health check completed"
        );

        HealthCheckResponse {
            status,
            timestamp: current_timestamp(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            components,
            uptime_seconds: self.uptime_seconds(),
        }
    }

    /// Get last health status for a component
    pub async fn get_component_status(&self, name: &str) -> Option<ComponentHealth> {
        let components = self.components.read().await;
        components.get(name).cloned()
    }

    /// Get all component statuses
    pub async fn get_all_statuses(&self) -> Vec<ComponentHealth> {
        let components = self.components.read().await;
        components.values().cloned().collect()
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple health check endpoint handler
pub struct HealthEndpoint {
    checker: HealthChecker,
}

impl HealthEndpoint {
    /// Create new health endpoint
    pub fn new(checker: HealthChecker) -> Self {
        Self { checker }
    }

    /// Handle liveness probe
    pub fn liveness(&self) -> LivenessResponse {
        LivenessResponse {
            alive: true,
            timestamp: current_timestamp(),
        }
    }

    /// Handle readiness probe
    pub async fn readiness(&self) -> ReadinessResponse {
        let components = self.checker.get_all_statuses().await;
        let ready = components
            .iter()
            .all(|c| matches!(c.status, HealthStatus::Healthy | HealthStatus::Degraded));

        ReadinessResponse {
            ready,
            timestamp: current_timestamp(),
            components,
        }
    }
}

/// Liveness probe response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivenessResponse {
    pub alive: bool,
    pub timestamp: u64,
}

/// Readiness probe response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessResponse {
    pub ready: bool,
    pub timestamp: u64,
    pub components: Vec<ComponentHealth>,
}

/// Get current Unix timestamp
fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_display() {
        assert_eq!(HealthStatus::Healthy.to_string(), "healthy");
        assert_eq!(HealthStatus::Degraded.to_string(), "degraded");
        assert_eq!(HealthStatus::Unhealthy.to_string(), "unhealthy");
    }

    #[test]
    fn test_health_checker_uptime() {
        let checker = HealthChecker::new();
        assert!(checker.uptime_seconds() < 2);
    }

    #[test]
    fn test_liveness_response() {
        let checker = HealthChecker::new();
        let endpoint = HealthEndpoint::new(checker);
        let resp = endpoint.liveness();
        assert!(resp.alive);
        assert!(resp.timestamp > 0);
    }

    #[tokio::test]
    async fn test_readiness_empty_components() {
        let checker = HealthChecker::new();
        let endpoint = HealthEndpoint::new(checker);
        let resp = endpoint.readiness().await;
        // Empty components should be considered ready
        assert!(resp.ready);
    }
}
