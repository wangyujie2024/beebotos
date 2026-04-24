//! User Channel HTTP Handlers
//!
//! P2 FIX: Provides REST APIs for managing user-channel bindings
//! (create, list, get, delete, connect, disconnect).

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use gateway::middleware::{require_any_role, AuthUser};
use serde::Deserialize;
use serde_json::json;
use tracing::{info, warn};

use crate::error::GatewayError;
use crate::AppState;

/// Create a new user channel
#[derive(Debug, Deserialize)]
pub struct CreateUserChannelRequest {
    pub platform: String,
    pub instance_name: String,
    pub platform_user_id: Option<String>,
    /// Platform-specific configuration JSON
    pub config: serde_json::Value,
}

pub async fn create_user_channel(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<CreateUserChannelRequest>,
) -> Result<impl IntoResponse, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let user_ch_svc = state
        .user_channel_service
        .as_ref()
        .ok_or_else(|| GatewayError::internal("User channel service not initialized"))?;

    let platform = parse_platform(&req.platform)
        .ok_or_else(|| GatewayError::bad_request(format!("Invalid platform: {}", req.platform)))?;

    // Deserialize config into UserChannelConfig
    let config: beebotos_agents::communication::UserChannelConfig =
        serde_json::from_value(req.config.clone())
            .map_err(|e| GatewayError::bad_request(format!("Invalid channel config: {}", e)))?;

    let binding = user_ch_svc
        .create_user_channel(
            &user.user_id,
            platform,
            &req.instance_name,
            req.platform_user_id.clone(),
            &config,
        )
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to create user channel: {}", e)))?;

    info!(
        "Created user channel {} for user {} on {:?}",
        binding.id, user.user_id, platform
    );

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": binding.id,
            "user_id": binding.user_id,
            "platform": req.platform,
            "instance_name": binding.instance_name,
            "platform_user_id": binding.platform_user_id,
            "status": format!("{:?}", binding.status),
        })),
    ))
}

/// List user channels for the current user
pub async fn list_user_channels(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Result<impl IntoResponse, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let user_ch_svc = state
        .user_channel_service
        .as_ref()
        .ok_or_else(|| GatewayError::internal("User channel service not initialized"))?;

    let channels = user_ch_svc
        .list_by_user(&user.user_id)
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to list user channels: {}", e)))?;

    let channels: Vec<serde_json::Value> = channels
        .into_iter()
        .map(|c| {
            json!({
                "id": c.id,
                "user_id": c.user_id,
                "platform": format!("{:?}", c.platform),
                "instance_name": c.instance_name,
                "platform_user_id": c.platform_user_id,
                "status": format!("{:?}", c.status),
                "webhook_path": c.webhook_path,
            })
        })
        .collect();

    Ok((StatusCode::OK, Json(json!({ "channels": channels }))))
}

/// Get a single user channel
pub async fn get_user_channel(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let user_ch_svc = state
        .user_channel_service
        .as_ref()
        .ok_or_else(|| GatewayError::internal("User channel service not initialized"))?;

    let channel = user_ch_svc
        .get(&id)
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to get user channel: {}", e)))?
        .ok_or_else(|| GatewayError::not_found("User channel", &id))?;

    // Authorization: users can only access their own channels unless admin
    if !user.is_admin() && channel.user_id != user.user_id {
        return Err(GatewayError::forbidden(
            "You don't have permission to access this user channel",
        ));
    }

    Ok((
        StatusCode::OK,
        Json(json!({
            "id": channel.id,
            "user_id": channel.user_id,
            "platform": format!("{:?}", channel.platform),
            "instance_name": channel.instance_name,
            "platform_user_id": channel.platform_user_id,
            "status": format!("{:?}", channel.status),
            "webhook_path": channel.webhook_path,
        })),
    ))
}

/// Delete a user channel
pub async fn delete_user_channel(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let user_ch_svc = state
        .user_channel_service
        .as_ref()
        .ok_or_else(|| GatewayError::internal("User channel service not initialized"))?;

    // Verify ownership before deletion
    let channel = user_ch_svc
        .get(&id)
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to get user channel: {}", e)))?
        .ok_or_else(|| GatewayError::not_found("User channel", &id))?;

    if !user.is_admin() && channel.user_id != user.user_id {
        return Err(GatewayError::forbidden(
            "You don't have permission to delete this user channel",
        ));
    }

    user_ch_svc
        .delete_user_channel(&id)
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to delete user channel: {}", e)))?;

    info!("Deleted user channel {}", id);

    Ok((
        StatusCode::OK,
        Json(json!({ "message": "User channel deleted successfully", "id": id })),
    ))
}

/// Connect a user channel
pub async fn connect_user_channel(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let user_ch_svc = state
        .user_channel_service
        .as_ref()
        .ok_or_else(|| GatewayError::internal("User channel service not initialized"))?;

    // Verify ownership
    let channel = user_ch_svc
        .get(&id)
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to get user channel: {}", e)))?
        .ok_or_else(|| GatewayError::not_found("User channel", &id))?;

    if !user.is_admin() && channel.user_id != user.user_id {
        return Err(GatewayError::forbidden(
            "You don't have permission to connect this user channel",
        ));
    }

    user_ch_svc
        .connect_user_channel(&id)
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to connect user channel: {}", e)))?;

    Ok((
        StatusCode::OK,
        Json(json!({ "message": "User channel connected successfully", "id": id })),
    ))
}

/// Disconnect a user channel
pub async fn disconnect_user_channel(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let user_ch_svc = state
        .user_channel_service
        .as_ref()
        .ok_or_else(|| GatewayError::internal("User channel service not initialized"))?;

    // Verify ownership
    let channel = user_ch_svc
        .get(&id)
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to get user channel: {}", e)))?
        .ok_or_else(|| GatewayError::not_found("User channel", &id))?;

    if !user.is_admin() && channel.user_id != user.user_id {
        return Err(GatewayError::forbidden(
            "You don't have permission to disconnect this user channel",
        ));
    }

    user_ch_svc
        .disconnect_user_channel(&id)
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to disconnect user channel: {}", e)))?;

    Ok((
        StatusCode::OK,
        Json(json!({ "message": "User channel disconnected successfully", "id": id })),
    ))
}

/// Parse platform string into PlatformType
fn parse_platform(platform: &str) -> Option<beebotos_agents::communication::PlatformType> {
    use beebotos_agents::communication::PlatformType;
    match platform.to_lowercase().as_str() {
        "slack" => Some(PlatformType::Slack),
        "telegram" => Some(PlatformType::Telegram),
        "discord" => Some(PlatformType::Discord),
        "whatsapp" => Some(PlatformType::WhatsApp),
        "signal" => Some(PlatformType::Signal),
        "imessage" => Some(PlatformType::IMessage),
        "wechat" => Some(PlatformType::WeChat),
        "teams" => Some(PlatformType::Teams),
        "twitter" => Some(PlatformType::Twitter),
        "lark" | "feishu" => Some(PlatformType::Lark),
        "dingtalk" => Some(PlatformType::DingTalk),
        "matrix" => Some(PlatformType::Matrix),
        "googlechat" => Some(PlatformType::GoogleChat),
        "line" => Some(PlatformType::Line),
        "qq" => Some(PlatformType::QQ),
        "irc" => Some(PlatformType::IRC),
        "webchat" => Some(PlatformType::WebChat),
        _ => None,
    }
}
