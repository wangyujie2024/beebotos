//! HTTP Handlers
//!
//! REST API handlers for Gateway.

pub mod admin_config;
pub mod agent_logs;
pub mod agents;
pub mod agents_v2;
pub mod auth;
pub mod browser;
pub mod chain;
pub mod chain_v2;
pub mod channels;
pub mod llm_admin;
pub mod llm_config;
pub mod llm_metrics;
pub mod skills;
pub mod state_machine;
pub mod task_monitor;
pub mod treasury;
pub mod user_channels;
pub mod user_settings;
pub mod webchat;
pub mod webhooks;

use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

/// Health check handler
#[allow(dead_code)]
pub async fn health() -> impl IntoResponse {
    Json(json!({
        "status": "healthy",
        "service": "beebotos-gateway",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// API info handler
#[allow(dead_code)]
pub async fn api_info() -> impl IntoResponse {
    Json(json!({
        "name": "BeeBotOS Gateway API",
        "version": "v2.0.0",
        "endpoints": [
            "/health",
            "/api/v1/agents",
        ]
    }))
}
