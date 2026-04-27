//! Common test utilities
//!
//! Shared test setup and helper functions.

use std::sync::Once;

static INIT: Once = Once::new();

/// Initialize test environment
pub async fn setup_test_env() {
    INIT.call_once(|| {
        // Initialize tracing for tests
        let _ = tracing_subscriber::fmt()
            .with_env_filter("debug")
            .try_init();
    });
}

/// Test database configuration
#[allow(dead_code)]
pub fn test_database_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://./data/beebotos_test.db".to_string())
}

/// Wait for a condition with timeout
#[allow(dead_code)]
pub async fn wait_for<F, Fut>(condition: F, timeout_ms: u64)
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_millis(timeout_ms);

    while start.elapsed() < timeout {
        if condition().await {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    panic!("Condition not met within {}ms", timeout_ms);
}

/// Generate a unique test ID
#[allow(dead_code)]
pub fn unique_test_id() -> String {
    format!("test-{}", uuid::Uuid::new_v4())
}
