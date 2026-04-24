//! Health Check Module
//!
//! Production-ready health checking with:
//! - Liveness probe (is the service running)
//! - Readiness probe (is the service ready to accept traffic)
//! - Dependency health checks
//! - Health aggregation

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::config::HealthConfig;

/// Overall health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HealthStatus {
    /// Service is healthy
    Up,
    /// Service is unhealthy
    Down,
    /// Service is starting up
    Starting,
    /// Service is shutting down
    Stopping,
}

impl HealthStatus {
    /// Check if status is up
    pub fn is_up(&self) -> bool {
        matches!(self, HealthStatus::Up)
    }

    /// Get HTTP status code
    pub fn http_status(&self) -> StatusCode {
        match self {
            HealthStatus::Up => StatusCode::OK,
            HealthStatus::Down => StatusCode::SERVICE_UNAVAILABLE,
            HealthStatus::Starting => StatusCode::SERVICE_UNAVAILABLE,
            HealthStatus::Stopping => StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}

/// Individual component health
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentHealth {
    /// Component name
    pub name: String,
    /// Health status
    pub status: HealthStatus,
    /// Response time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_time_ms: Option<u64>,
    /// Error message if unhealthy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Last check timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_check: Option<String>,
}

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    /// Overall status
    pub status: HealthStatus,
    /// Service name
    pub service: String,
    /// Service version
    pub version: String,
    /// Timestamp
    pub timestamp: String,
    /// Uptime in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime_seconds: Option<u64>,
    /// Individual component health
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<ComponentHealth>>,
}

/// Health check trait
#[async_trait::async_trait]
pub trait HealthCheck: Send + Sync {
    /// Check name
    fn name(&self) -> &str;

    /// Perform health check
    async fn check(&self) -> ComponentHealth;

    /// Check interval
    fn interval(&self) -> Duration {
        Duration::from_secs(30)
    }
}

/// Health check registry
#[derive(Clone)]
pub struct HealthRegistry {
    checks: Arc<RwLock<Vec<Arc<dyn HealthCheck>>>>,
    results: Arc<RwLock<HashMap<String, ComponentHealth>>>,
    start_time: Instant,
    config: HealthConfig,
}

impl std::fmt::Debug for HealthRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HealthRegistry")
            .field("start_time", &self.start_time)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl HealthRegistry {
    /// Create new health registry
    pub fn new(config: HealthConfig) -> Self {
        Self {
            checks: Arc::new(RwLock::new(Vec::new())),
            results: Arc::new(RwLock::new(HashMap::new())),
            start_time: Instant::now(),
            config,
        }
    }

    /// Register a health check
    pub async fn register(&self, check: Arc<dyn HealthCheck>) {
        let mut checks = self.checks.write().await;
        checks.push(check);
    }

    /// Start background health checks
    ///
    /// 🟡 MEDIUM FIX: Uses configured interval instead of hardcoded value
    pub fn start(&self) {
        let checks = self.checks.clone();
        let results = self.results.clone();
        // 🟡 MEDIUM FIX: Use configured interval from HealthConfig
        let interval_seconds = self.config.check_interval_seconds;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(interval_seconds));

            loop {
                interval.tick().await;

                let checks_list = checks.read().await.clone();

                for check in checks_list {
                    // Respect individual check intervals
                    let check_interval = check.interval().as_secs();
                    // Only run check if enough time has passed (simplified - would need per-check
                    // tracking in production)
                    if check_interval <= interval_seconds {
                        let health = check.check().await;
                        let mut results = results.write().await;
                        results.insert(check.name().to_string(), health);
                    }
                }
            }
        });
    }

    /// Get overall health
    pub async fn health(&self) -> HealthResponse {
        let results = self.results.read().await;
        let components: Vec<_> = results.values().cloned().collect();

        // Overall status is Down if any critical component is down
        let status = if components
            .iter()
            .any(|c| matches!(c.status, HealthStatus::Down))
        {
            HealthStatus::Down
        } else {
            HealthStatus::Up
        };

        let uptime = Instant::now().duration_since(self.start_time).as_secs();

        HealthResponse {
            status,
            service: "beebotos-gateway".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            uptime_seconds: Some(uptime),
            components: Some(components),
        }
    }

    /// Get liveness status (is the service running)
    pub async fn liveness(&self) -> HealthResponse {
        HealthResponse {
            status: HealthStatus::Up,
            service: "beebotos-gateway".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            uptime_seconds: Some(Instant::now().duration_since(self.start_time).as_secs()),
            components: None,
        }
    }

    /// Get readiness status (is the service ready)
    pub async fn readiness(&self) -> HealthResponse {
        self.health().await
    }
}

/// Simple ping health check
#[derive(Debug)]
pub struct PingHealthCheck {
    name: String,
    target: String,
}

impl PingHealthCheck {
    /// Create new ping health check
    pub fn new(name: impl Into<String>, target: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            target: target.into(),
        }
    }
}

#[async_trait::async_trait]
impl HealthCheck for PingHealthCheck {
    fn name(&self) -> &str {
        &self.name
    }

    /// 🟠 HIGH SECURITY FIX: Explicitly closes TCP connection to prevent
    /// resource leaks
    async fn check(&self) -> ComponentHealth {
        let start = Instant::now();

        // Simple TCP connect check with explicit connection cleanup
        match tokio::net::TcpStream::connect(&self.target).await {
            Ok(stream) => {
                // 🟠 HIGH SECURITY FIX: Explicitly drop the connection to prevent resource leak
                drop(stream);

                let elapsed = start.elapsed();
                ComponentHealth {
                    name: self.name.clone(),
                    status: HealthStatus::Up,
                    response_time_ms: Some(elapsed.as_millis() as u64),
                    error: None,
                    last_check: Some(chrono::Utc::now().to_rfc3339()),
                }
            }
            Err(e) => ComponentHealth {
                name: self.name.clone(),
                status: HealthStatus::Down,
                response_time_ms: None,
                error: Some(e.to_string()),
                last_check: Some(chrono::Utc::now().to_rfc3339()),
            },
        }
    }
}

/// HTTP health check
///
/// 🟡 MEDIUM PERFORMANCE FIX: Reuses HTTP client for connection pooling
#[derive(Debug)]
pub struct HttpHealthCheck {
    name: String,
    url: String,
    expected_status: u16,
    /// 🟡 MEDIUM PERFORMANCE FIX: Reusable client with connection pooling
    client: reqwest::Client,
}

impl HttpHealthCheck {
    /// Create new HTTP health check
    ///
    /// 🟡 MEDIUM PERFORMANCE FIX: Creates reusable client with connection pool
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        let name = name.into();
        let url = url.into();

        // 🟡 MEDIUM PERFORMANCE FIX: Create reusable client with connection pool
        // This avoids creating a new connection pool for each health check
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(10)
            .build()
            .expect("Failed to create HTTP client");

        Self {
            name,
            url,
            expected_status: 200,
            client,
        }
    }

    /// Set expected HTTP status code
    pub fn with_expected_status(mut self, status: u16) -> Self {
        self.expected_status = status;
        self
    }

    /// Set request timeout (creates new client with updated timeout)
    ///
    /// 🟡 MEDIUM PERFORMANCE FIX: Recreates client only when timeout changes
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.client = reqwest::Client::builder()
            .timeout(timeout)
            .pool_max_idle_per_host(10)
            .build()
            .expect("Failed to create HTTP client");
        self
    }
}

#[async_trait::async_trait]
impl HealthCheck for HttpHealthCheck {
    fn name(&self) -> &str {
        &self.name
    }

    /// 🟡 MEDIUM PERFORMANCE FIX: Uses reusable client for connection pooling
    async fn check(&self) -> ComponentHealth {
        let start = Instant::now();

        // 🟡 MEDIUM PERFORMANCE FIX: Reuse existing client (with connection pool)
        // Old code created a new client (and new connection pool) for each check
        match self.client.get(&self.url).send().await {
            Ok(response) => {
                let status = response.status();
                let elapsed = start.elapsed();

                if status.as_u16() == self.expected_status {
                    ComponentHealth {
                        name: self.name.clone(),
                        status: HealthStatus::Up,
                        response_time_ms: Some(elapsed.as_millis() as u64),
                        error: None,
                        last_check: Some(chrono::Utc::now().to_rfc3339()),
                    }
                } else {
                    ComponentHealth {
                        name: self.name.clone(),
                        status: HealthStatus::Down,
                        response_time_ms: Some(elapsed.as_millis() as u64),
                        error: Some(format!("Unexpected status: {}", status)),
                        last_check: Some(chrono::Utc::now().to_rfc3339()),
                    }
                }
            }
            Err(e) => ComponentHealth {
                name: self.name.clone(),
                status: HealthStatus::Down,
                response_time_ms: None,
                error: Some(e.to_string()),
                last_check: Some(chrono::Utc::now().to_rfc3339()),
            },
        }
    }
}

/// Database health check placeholder
#[derive(Debug)]
#[allow(dead_code)]
pub struct DatabaseHealthCheck {
    name: String,
    connection_string: String,
}

impl DatabaseHealthCheck {
    /// Create new database health check
    pub fn new(name: impl Into<String>, connection_string: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            connection_string: connection_string.into(),
        }
    }
}

#[async_trait::async_trait]
impl HealthCheck for DatabaseHealthCheck {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check(&self) -> ComponentHealth {
        // Placeholder - would actually check DB connection
        ComponentHealth {
            name: self.name.clone(),
            status: HealthStatus::Up,
            response_time_ms: Some(0),
            error: None,
            last_check: Some(chrono::Utc::now().to_rfc3339()),
        }
    }
}

/// Health check HTTP handlers
pub mod handlers {
    use axum::extract::State;

    use super::*;

    /// Liveness probe handler
    pub async fn liveness(State(registry): State<Arc<HealthRegistry>>) -> impl IntoResponse {
        let health = registry.liveness().await;
        let status = health.status.http_status();
        (status, Json(health))
    }

    /// Readiness probe handler
    pub async fn readiness(State(registry): State<Arc<HealthRegistry>>) -> impl IntoResponse {
        let health = registry.readiness().await;
        let status = health.status.http_status();
        (status, Json(health))
    }

    /// Full health check handler
    pub async fn health(State(registry): State<Arc<HealthRegistry>>) -> impl IntoResponse {
        let health = registry.health().await;
        let status = health.status.http_status();
        (status, Json(health))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status() {
        assert!(HealthStatus::Up.is_up());
        assert!(!HealthStatus::Down.is_up());
        assert!(!HealthStatus::Starting.is_up());
        assert!(!HealthStatus::Stopping.is_up());

        assert_eq!(HealthStatus::Up.http_status(), StatusCode::OK);
        assert_eq!(
            HealthStatus::Down.http_status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[tokio::test]
    async fn test_health_registry() {
        let config = HealthConfig::default();
        let registry = Arc::new(HealthRegistry::new(config));

        // Add a mock health check
        registry
            .register(Arc::new(PingHealthCheck::new(
                "localhost",
                "127.0.0.1:12345",
            )))
            .await;

        // Get health
        let health = registry.health().await;
        assert!(matches!(
            health.status,
            HealthStatus::Up | HealthStatus::Down
        ));
        assert_eq!(health.service, "beebotos-gateway");
    }

    #[test]
    fn test_component_health() {
        let component = ComponentHealth {
            name: "test".to_string(),
            status: HealthStatus::Up,
            response_time_ms: Some(100),
            error: None,
            last_check: Some(chrono::Utc::now().to_rfc3339()),
        };

        let json = serde_json::to_string(&component).unwrap();
        assert!(json.contains("\"name\":\"test\""));
        assert!(json.contains("\"status\":\"UP\""));
    }
}
