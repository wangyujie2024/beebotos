//! Health Check Utilities
//!
//! Re-exports gateway-lib health types and provides business-specific
//! health check utilities (database, kernel, chain).

use std::sync::Arc;
use std::time::Duration;

// Re-export health types from gateway-lib (may be used by other modules)
#[allow(unused_imports)]
pub use gateway::health::{ComponentHealth, HealthRegistry, HealthResponse, HealthStatus};

use crate::AppState;

/// Component status for health checks
#[derive(serde::Serialize)]
pub struct ComponentStatus {
    pub status: String,
    pub message: Option<String>,
    pub latency_ms: Option<u64>,
}

impl ComponentStatus {
    /// Create OK status
    #[allow(dead_code)]
    pub fn ok() -> Self {
        Self {
            status: "ok".to_string(),
            message: None,
            latency_ms: None,
        }
    }

    /// Create error status
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            status: "error".to_string(),
            message: Some(message.into()),
            latency_ms: None,
        }
    }

    /// Create status with latency
    pub fn with_latency(latency_ms: u64) -> Self {
        Self {
            status: "ok".to_string(),
            message: None,
            latency_ms: Some(latency_ms),
        }
    }
}

/// Check database connectivity
pub async fn check_database(db: &sqlx::SqlitePool) -> ComponentStatus {
    let start = std::time::Instant::now();

    match sqlx::query("SELECT 1").fetch_one(db).await {
        Ok(_) => {
            let latency = start.elapsed().as_millis() as u64;
            ComponentStatus::with_latency(latency)
        }
        Err(e) => ComponentStatus::error(format!("Database error: {}", e)),
    }
}

/// Check kernel service
pub async fn check_kernel(kernel: &beebotos_kernel::Kernel) -> ComponentStatus {
    let start = std::time::Instant::now();

    let stats = kernel.stats().await;
    let latency = start.elapsed().as_millis() as u64;

    if stats.running {
        ComponentStatus {
            status: "ok".to_string(),
            message: Some(format!(
                "tasks_submitted={}, tasks_completed={}",
                stats.scheduler.tasks_submitted, stats.scheduler.tasks_completed
            )),
            latency_ms: Some(latency),
        }
    } else {
        ComponentStatus::error("Kernel is not running".to_string())
    }
}

/// Check chain service
pub async fn check_chain(config: &crate::config::AppConfig) -> ComponentStatus {
    let start = std::time::Instant::now();

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => return ComponentStatus::error(format!("Failed to create HTTP client: {}", e)),
    };

    let services = match &config.services {
        Some(s) => s,
        None => return ComponentStatus::error("Services configuration not available".to_string()),
    };

    match client
        .get(format!("{}/health", services.chain_url))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                let latency = start.elapsed().as_millis() as u64;
                ComponentStatus::with_latency(latency)
            } else {
                ComponentStatus::error(format!("Chain returned status: {}", response.status()))
            }
        }
        Err(e) => ComponentStatus::error(format!("Chain unreachable: {}", e)),
    }
}

/// Run basic system health check (backward compatible)
#[allow(dead_code)]
pub async fn check_system(state: &Arc<AppState>) -> SystemHealth {
    use chrono::Utc;

    let database = check_database(&state.db).await;
    let kernel = check_kernel(&state.kernel).await;
    let chain = check_chain(&state.config).await;

    let is_healthy = database.status == "ok" && kernel.status == "ok" && chain.status == "ok";

    SystemHealth {
        database,
        kernel,
        chain,
        redis: None,
        llm_service: None,
        webhook_handler: None,
        overall: if is_healthy {
            "healthy".to_string()
        } else {
            "unhealthy".to_string()
        },
        timestamp: Utc::now().to_rfc3339(),
    }
}

/// System health summary
#[derive(serde::Serialize)]
pub struct SystemHealth {
    pub database: ComponentStatus,
    pub kernel: ComponentStatus,
    pub chain: ComponentStatus,
    // OBS-003: Additional health checks
    pub redis: Option<ComponentStatus>,
    pub llm_service: Option<ComponentStatus>,
    pub webhook_handler: Option<ComponentStatus>,
    pub overall: String,
    pub timestamp: String,
}

impl SystemHealth {
    /// Check if all components are healthy
    pub fn is_healthy(&self) -> bool {
        self.database.status == "ok"
            && self.kernel.status == "ok"
            && self.chain.status == "ok"
            && self
                .redis
                .as_ref()
                .map(|r| r.status == "ok")
                .unwrap_or(true)
            && self
                .llm_service
                .as_ref()
                .map(|s| s.status == "ok")
                .unwrap_or(true)
            && self
                .webhook_handler
                .as_ref()
                .map(|w| w.status == "ok")
                .unwrap_or(true)
    }

    /// Get overall status
    pub fn overall_status(&self) -> &'static str {
        if self.is_healthy() {
            "healthy"
        } else {
            "unhealthy"
        }
    }
}

/// Check LLM service health
pub async fn check_llm_service(config: &crate::config::AppConfig) -> ComponentStatus {
    let start = std::time::Instant::now();

    // Check if default provider is configured
    if config.models.default_provider.is_empty() {
        return ComponentStatus::error("No default LLM provider configured".to_string());
    }

    // Check if API key is available (simplified check)
    let api_key_env = format!("{}_API_KEY", config.models.default_provider.to_uppercase());
    if std::env::var(&api_key_env).is_err() && std::env::var("OPENAI_API_KEY").is_err() {
        return ComponentStatus::error(format!(
            "LLM API key not found (checked {} and OPENAI_API_KEY)",
            api_key_env
        ));
    }

    let latency = start.elapsed().as_millis() as u64;
    ComponentStatus {
        status: "ok".to_string(),
        message: Some(format!("Provider: {}", config.models.default_provider)),
        latency_ms: Some(latency),
    }
}

/// Check webhook handler status
pub async fn check_webhook_handler(
    webhook_state: &Arc<tokio::sync::RwLock<crate::handlers::http::webhooks::WebhookHandlerState>>,
) -> ComponentStatus {
    let start = std::time::Instant::now();

    let state = webhook_state.read().await;
    let paths = state.manager.list_registered_paths().await;

    let latency = start.elapsed().as_millis() as u64;
    ComponentStatus {
        status: "ok".to_string(),
        message: Some(format!("{} handlers registered", paths.len())),
        latency_ms: Some(latency),
    }
}

/// Enhanced system health check with all components
pub async fn check_system_full(state: &Arc<crate::AppState>) -> SystemHealth {
    use chrono::Utc;

    let database = check_database(&state.db).await;
    let kernel = check_kernel(&state.kernel).await;
    let chain = check_chain(&state.config).await;

    // Optional checks
    let llm_service = Some(check_llm_service(&state.config).await);
    let webhook_handler = Some(check_webhook_handler(&state.webhook_state).await);

    let health = SystemHealth {
        database,
        kernel,
        chain,
        redis: None, // Will be populated if Redis is configured
        llm_service,
        webhook_handler,
        overall: "unknown".to_string(),
        timestamp: Utc::now().to_rfc3339(),
    };

    let overall = health.overall_status().to_string();
    SystemHealth { overall, ..health }
}
