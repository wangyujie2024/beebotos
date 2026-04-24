//! User Settings HTTP Handlers
//!
//! Provides REST APIs for managing per-user preferences (theme, language,
//! notifications, etc.).

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use gateway::error::GatewayError;
use gateway::middleware::{require_any_role, AuthUser};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::AppState;

/// User settings response
#[derive(Debug, Serialize)]
pub struct UserSettingsResponse {
    pub theme: String,
    pub language: String,
    pub notifications_enabled: bool,
    pub auto_update: bool,
    pub api_endpoint: Option<String>,
    pub wallet_address: Option<String>,
}

/// Update user settings request
#[derive(Debug, Deserialize)]
pub struct UpdateUserSettingsRequest {
    pub theme: Option<String>,
    pub language: Option<String>,
    pub notifications_enabled: Option<bool>,
    pub auto_update: Option<bool>,
    pub api_endpoint: Option<String>,
    pub wallet_address: Option<String>,
}

/// Get current user settings
pub async fn get_user_settings(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Result<Json<UserSettingsResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let settings = sqlx::query_as::<_, UserSettingsRow>(
        "SELECT theme, language, notifications_enabled, auto_update, api_endpoint, wallet_address
         FROM user_settings WHERE user_id = ?",
    )
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| GatewayError::internal(format!("Failed to get user settings: {}", e)))?;

    let response = match settings {
        Some(row) => UserSettingsResponse {
            theme: row.theme,
            language: row.language,
            notifications_enabled: row.notifications_enabled != 0,
            auto_update: row.auto_update != 0,
            api_endpoint: row.api_endpoint,
            wallet_address: row.wallet_address,
        },
        None => UserSettingsResponse {
            theme: "dark".to_string(),
            language: "en".to_string(),
            notifications_enabled: true,
            auto_update: true,
            api_endpoint: None,
            wallet_address: None,
        },
    };

    Ok(Json(response))
}

/// Update user settings
pub async fn update_user_settings(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<UpdateUserSettingsRequest>,
) -> Result<impl IntoResponse, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let theme = req.theme.as_deref().unwrap_or("dark");
    let language = req.language.as_deref().unwrap_or("en");
    let notifications_enabled = req.notifications_enabled.unwrap_or(true);
    let auto_update = req.auto_update.unwrap_or(true);

    sqlx::query(
        "INSERT INTO user_settings (user_id, theme, language, notifications_enabled, auto_update, \
         api_endpoint, wallet_address)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(user_id) DO UPDATE SET
             theme = excluded.theme,
             language = excluded.language,
             notifications_enabled = excluded.notifications_enabled,
             auto_update = excluded.auto_update,
             api_endpoint = excluded.api_endpoint,
             wallet_address = excluded.wallet_address,
             updated_at = datetime('now')",
    )
    .bind(&user.user_id)
    .bind(theme)
    .bind(language)
    .bind(if notifications_enabled { 1 } else { 0 })
    .bind(if auto_update { 1 } else { 0 })
    .bind(req.api_endpoint.as_deref())
    .bind(req.wallet_address.as_deref())
    .execute(&state.db)
    .await
    .map_err(|e| GatewayError::internal(format!("Failed to update user settings: {}", e)))?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "message": "Settings updated successfully",
            "theme": theme,
            "language": language,
            "notifications_enabled": notifications_enabled,
            "auto_update": auto_update,
            "api_endpoint": req.api_endpoint,
            "wallet_address": req.wallet_address,
        })),
    ))
}

#[derive(sqlx::FromRow)]
struct UserSettingsRow {
    theme: String,
    language: String,
    notifications_enabled: i64,
    auto_update: i64,
    api_endpoint: Option<String>,
    wallet_address: Option<String>,
}
