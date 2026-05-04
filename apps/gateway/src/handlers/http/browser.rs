//! Browser Automation HTTP Handlers
//!
//! Provides REST APIs for browser profile management and basic automation.
//! Full CDP integration is a future enhancement; this module provides
//! the API surface and SQLite-backed profile storage.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use gateway::{
    error::GatewayError,
    middleware::AuthUser,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::AppState;

// ==================== Data Models ====================

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct BrowserProfile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: String,
    #[serde(default = "default_cdp_port")]
    pub cdp_port: i64,
    #[serde(default)]
    pub headless: i64,
    pub proxy: Option<String>,
    pub user_agent: Option<String>,
    #[serde(default = "default_viewport_width")]
    pub viewport_width: i64,
    #[serde(default = "default_viewport_height")]
    pub viewport_height: i64,
}

fn default_cdp_port() -> i64 { 9222 }
fn default_viewport_width() -> i64 { 1920 }
fn default_viewport_height() -> i64 { 1080 }

#[derive(Debug, Deserialize)]
pub struct CreateProfileRequest {
    pub name: String,
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default = "default_cdp_port")]
    pub cdp_port: i64,
    #[serde(default = "default_headless")]
    pub headless: i64,
    pub proxy: Option<String>,
    pub user_agent: Option<String>,
    #[serde(default = "default_viewport_width")]
    pub viewport_width: i64,
    #[serde(default = "default_viewport_height")]
    pub viewport_height: i64,
}

fn default_color() -> String { "#3b82f6".to_string() }
fn default_headless() -> i64 { 1 }

#[derive(Debug, Serialize)]
pub struct BrowserInstance {
    pub id: String,
    pub profile_id: String,
    pub status: String,
    pub current_url: String,
}

#[derive(Debug, Deserialize)]
pub struct ConnectRequest {
    pub profile_id: String,
}

#[derive(Debug, Deserialize)]
pub struct DisconnectRequest {
    pub instance_id: String,
}

#[derive(Debug, Deserialize)]
pub struct NavigateRequest {
    pub instance_id: String,
    pub url: String,
}

#[derive(Debug, Serialize)]
pub struct NavigationResponse {
    pub success: bool,
    pub url: String,
    pub title: String,
}

#[derive(Debug, Deserialize)]
pub struct EvaluateRequest {
    pub instance_id: String,
    pub script: String,
}

#[derive(Debug, Serialize)]
pub struct EvaluateResponse {
    pub result: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exception: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct ScreenshotRequest {
    pub instance_id: String,
    #[serde(default)]
    pub full_page: bool,
}

#[derive(Debug, Serialize)]
pub struct ScreenshotResponse {
    pub data: String,
    pub format: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Serialize)]
pub struct BrowserStatus {
    pub enabled: bool,
    pub profiles_count: i64,
    pub active_instances: i64,
}

// Sandbox models
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct BrowserSandbox {
    pub id: String,
    pub name: String,
    pub profile_id: Option<String>,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateSandboxRequest {
    pub name: String,
    pub profile_id: String,
}

#[derive(Debug, Deserialize)]
pub struct BatchExecuteRequest {
    pub instance_id: String,
    pub operations: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct BatchExecuteResponse {
    pub success: bool,
    pub results: Vec<serde_json::Value>,
    pub completed_count: usize,
}

#[derive(Debug, Serialize)]
pub struct SandboxStats {
    pub sandbox_id: String,
    pub name: String,
    pub total_executions: i64,
    pub active_instances: i64,
    pub uptime_seconds: i64,
}

// ==================== Handlers ====================

/// Get browser automation service status
pub async fn get_status(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
) -> Result<Json<BrowserStatus>, GatewayError> {
    let profiles_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM browser_profiles")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    let active_instances: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM browser_instances WHERE status = 'connected'"
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    Ok(Json(BrowserStatus {
        enabled: true,
        profiles_count,
        active_instances,
    }))
}

/// List all browser profiles
pub async fn list_profiles(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
) -> Result<Json<Vec<BrowserProfile>>, GatewayError> {
    let rows: Vec<BrowserProfile> = sqlx::query_as(
        "SELECT id, name, color, cdp_port, headless, proxy, user_agent, viewport_width, viewport_height
         FROM browser_profiles ORDER BY created_at DESC"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| GatewayError::internal(format!("Failed to list profiles: {}", e)))?;

    Ok(Json(rows))
}

/// Create a browser profile
pub async fn create_profile(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
    Json(req): Json<CreateProfileRequest>,
) -> Result<impl IntoResponse, GatewayError> {
    let id = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO browser_profiles (id, name, color, cdp_port, headless, proxy, user_agent, viewport_width, viewport_height)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
    )
    .bind(&id)
    .bind(&req.name)
    .bind(&req.color)
    .bind(req.cdp_port)
    .bind(req.headless)
    .bind(&req.proxy)
    .bind(&req.user_agent)
    .bind(req.viewport_width)
    .bind(req.viewport_height)
    .execute(&state.db)
    .await
    .map_err(|e| GatewayError::internal(format!("Failed to create profile: {}", e)))?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": id,
            "name": req.name,
            "message": "Profile created"
        })),
    ))
}

/// Delete a browser profile
pub async fn delete_profile(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, GatewayError> {
    let result = sqlx::query("DELETE FROM browser_profiles WHERE id = ?1")
        .bind(&id)
        .execute(&state.db)
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to delete profile: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(GatewayError::not_found("BrowserProfile", &id));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Connect to a browser profile (creates an instance record)
pub async fn connect(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
    Json(req): Json<ConnectRequest>,
) -> Result<Json<BrowserInstance>, GatewayError> {
    // Verify profile exists
    let exists: bool = sqlx::query_scalar("SELECT COUNT(*) > 0 FROM browser_profiles WHERE id = ?1")
        .bind(&req.profile_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(false);

    if !exists {
        return Err(GatewayError::not_found("BrowserProfile", &req.profile_id));
    }

    let instance_id = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO browser_instances (id, profile_id, status, current_url, connected_at)
         VALUES (?1, ?2, 'connected', 'about:blank', datetime('now'))"
    )
    .bind(&instance_id)
    .bind(&req.profile_id)
    .execute(&state.db)
    .await
    .map_err(|e| GatewayError::internal(format!("Failed to create instance: {}", e)))?;

    Ok(Json(BrowserInstance {
        id: instance_id,
        profile_id: req.profile_id,
        status: "connected".to_string(),
        current_url: "about:blank".to_string(),
    }))
}

/// Disconnect a browser instance
pub async fn disconnect(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
    Json(req): Json<DisconnectRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    sqlx::query(
        "UPDATE browser_instances SET status = 'disconnected', disconnected_at = datetime('now') WHERE id = ?1"
    )
    .bind(&req.instance_id)
    .execute(&state.db)
    .await
    .map_err(|e| GatewayError::internal(format!("Failed to disconnect: {}", e)))?;

    Ok(Json(json!({ "success": true, "message": "Browser disconnected" })))
}

/// Navigate to a URL (stub — records intent, returns simulated result)
pub async fn navigate(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
    Json(req): Json<NavigateRequest>,
) -> Result<Json<NavigationResponse>, GatewayError> {
    sqlx::query("UPDATE browser_instances SET current_url = ?1 WHERE id = ?2")
        .bind(&req.url)
        .bind(&req.instance_id)
        .execute(&state.db)
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to navigate: {}", e)))?;

    Ok(Json(NavigationResponse {
        success: true,
        url: req.url,
        title: "BeeBotOS Browser".to_string(),
    }))
}

/// Evaluate JavaScript (stub — returns simulated result)
pub async fn evaluate(
    State(_state): State<Arc<AppState>>,
    _user: AuthUser,
    Json(req): Json<EvaluateRequest>,
) -> Result<Json<EvaluateResponse>, GatewayError> {
    // Stub: in a full implementation this would send CDP Runtime.evaluate
    Ok(Json(EvaluateResponse {
        result: serde_json::Value::String(format!("Evaluated: {} (stub result)", req.script.chars().take(40).collect::<String>())),
        exception: None,
    }))
}

/// Capture screenshot (stub)
pub async fn screenshot(
    State(_state): State<Arc<AppState>>,
    _user: AuthUser,
    Json(_req): Json<ScreenshotRequest>,
) -> Result<Json<ScreenshotResponse>, GatewayError> {
    // Stub: in a full implementation this would send CDP Page.captureScreenshot
    Ok(Json(ScreenshotResponse {
        data: String::new(),
        format: "Png".to_string(),
        width: 0,
        height: 0,
    }))
}

/// Execute a batch of browser operations (stub)
pub async fn execute_batch(
    State(_state): State<Arc<AppState>>,
    _user: AuthUser,
    Json(req): Json<BatchExecuteRequest>,
) -> Result<Json<BatchExecuteResponse>, GatewayError> {
    let results: Vec<serde_json::Value> = req
        .operations
        .iter()
        .enumerate()
        .map(|(i, _op)| {
            json!({
                "index": i,
                "success": true,
                "message": "Batch operation stub executed",
            })
        })
        .collect();

    let completed_count = results.len();

    Ok(Json(BatchExecuteResponse {
        success: true,
        results,
        completed_count,
    }))
}

/// List all browser sandboxes
pub async fn list_sandboxes(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
) -> Result<Json<Vec<BrowserSandbox>>, GatewayError> {
    let rows: Vec<BrowserSandbox> = sqlx::query_as(
        "SELECT id, name, profile_id, status, created_at FROM browser_sandboxes ORDER BY created_at DESC"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| GatewayError::internal(format!("Failed to list sandboxes: {}", e)))?;

    Ok(Json(rows))
}

/// Create a browser sandbox
pub async fn create_sandbox(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
    Json(req): Json<CreateSandboxRequest>,
) -> Result<impl IntoResponse, GatewayError> {
    let id = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO browser_sandboxes (id, name, profile_id, status) VALUES (?1, ?2, ?3, 'active')"
    )
    .bind(&id)
    .bind(&req.name)
    .bind(&req.profile_id)
    .execute(&state.db)
    .await
    .map_err(|e| GatewayError::internal(format!("Failed to create sandbox: {}", e)))?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": id,
            "name": req.name,
            "profile_id": req.profile_id,
            "status": "active",
            "message": "Sandbox created"
        })),
    ))
}

/// Delete a browser sandbox
pub async fn delete_sandbox(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let result = sqlx::query("DELETE FROM browser_sandboxes WHERE id = ?1")
        .bind(&id)
        .execute(&state.db)
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to delete sandbox: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(GatewayError::not_found("BrowserSandbox", &id));
    }

    Ok(Json(json!({ "success": true, "message": "Sandbox deleted" })))
}

/// Get sandbox statistics (stub)
pub async fn get_sandbox_stats(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<SandboxStats>, GatewayError> {
    let sandbox: Option<BrowserSandbox> = sqlx::query_as(
        "SELECT id, name, profile_id, status, created_at FROM browser_sandboxes WHERE id = ?1"
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| GatewayError::internal(format!("Failed to get sandbox: {}", e)))?;

    if sandbox.is_none() {
        return Err(GatewayError::not_found("BrowserSandbox", &id));
    }

    let instance_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM browser_instances WHERE profile_id IN (SELECT profile_id FROM browser_sandboxes WHERE id = ?1)"
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    Ok(Json(SandboxStats {
        sandbox_id: id.clone(),
        name: sandbox.unwrap().name,
        total_executions: 0,
        active_instances: instance_count,
        uptime_seconds: 0,
    }))
}
