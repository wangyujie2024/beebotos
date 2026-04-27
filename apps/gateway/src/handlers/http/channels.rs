//! Channel HTTP Handlers
//!
//! Handles channel management and WeChat QR code login.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::Json;
use beebotos_agents::communication::channel::{ChannelEvent, PersonalWeChatChannel};
use beebotos_agents::communication::{Message, MessageType, PlatformType};
use gateway::middleware::AuthUser;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::error::GatewayError;
use crate::AppState;

/// WeChat QR code response
#[derive(Debug, Serialize)]
pub struct WeChatQrResponse {
    /// QR code string (for generating QR image)
    pub qrcode: String,
    /// QR code image URL or base64 content
    pub qrcode_img_content: Option<String>,
    /// Expiration time in seconds
    pub expires_in: u64,
}

/// QR code status response
#[derive(Debug, Serialize)]
pub struct QrStatusResponse {
    /// Status: pending, scanned, confirmed, expired
    pub status: String,
    /// Bot token (only present when confirmed)
    pub bot_token: Option<String>,
    /// Base URL for bot API
    pub base_url: Option<String>,
    /// Status message
    pub message: Option<String>,
}

/// Get WeChat QR code for login
pub async fn get_wechat_qr(
    State(state): State<Arc<AppState>>,
) -> Result<Json<WeChatQrResponse>, GatewayError> {
    info!("Getting WeChat QR code for login");

    // Get personal_wechat channel from registry
    let registry = state
        .channel_registry
        .as_ref()
        .ok_or_else(|| GatewayError::internal("Channel registry not initialized"))?
        .clone();

    // Try to get the personal_wechat channel specifically.
    // In the legacy registry, both "wechat" (enterprise) and "personal_wechat"
    // share PlatformType::WeChat, so we look up by exact channel type first.
    let channel = if let Some(ch) = registry.get_channel("personal_wechat").await {
        ch
    } else {
        warn!("personal_wechat not found in legacy_map, falling back to PlatformType::WeChat");
        registry
            .get_channel_by_platform(PlatformType::WeChat)
            .await
            .ok_or_else(|| GatewayError::internal("Personal WeChat channel not initialized"))?
    };

    let qr_resp = {
        let guard = channel.read().await;
        let pwc = guard
            .as_any()
            .downcast_ref::<PersonalWeChatChannel>()
            .ok_or_else(|| {
                GatewayError::internal(
                    "Channel is not PersonalWeChatChannel. Ensure 'personal_wechat' is enabled in \
                     config and not overwritten by 'wechat' (enterprise).",
                )
            })?;
        pwc.get_qr_code().await
    }
    .map_err(|e| GatewayError::internal(format!("Failed to get QR code: {}", e)))?;

    info!("Successfully generated WeChat QR code");

    Ok(Json(WeChatQrResponse {
        qrcode: qr_resp.qrcode,
        qrcode_img_content: qr_resp.qrcode_img_content,
        expires_in: 300, // QR code expires in 5 minutes
    }))
}

/// Check WeChat QR code scan status
#[derive(Debug, Deserialize)]
pub struct CheckQrRequest {
    pub qr_code: String,
}

pub async fn check_wechat_qr(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CheckQrRequest>,
) -> Result<Json<QrStatusResponse>, GatewayError> {
    info!("Checking WeChat QR code status");

    // Create iLink client directly to check QR status
    let client = beebotos_agents::communication::channel::ILinkClient::new(None);

    let qr_status = match client.get_qrcode_status(&req.qr_code).await {
        Ok(status) => status,
        Err(e) => {
            error!("Failed to check QR status: {}", e);
            return Err(GatewayError::internal(format!(
                "Failed to check QR status: {}",
                e
            )));
        }
    };

    let status = if qr_status.status == "confirmed" {
        if let (Some(token), Some(base_url)) =
            (qr_status.bot_token.clone(), qr_status.base_url.clone())
        {
            info!("WeChat QR scan confirmed, completing login on channel...");

            let registry = state
                .channel_registry
                .as_ref()
                .ok_or_else(|| GatewayError::internal("Channel registry not initialized"))?
                .clone();

            let channel = registry
                .get_channel_by_platform(PlatformType::WeChat)
                .await
                .ok_or_else(|| GatewayError::internal("Personal WeChat channel not initialized"))?;

            let event_bus = state
                .channel_event_bus
                .as_ref()
                .ok_or_else(|| GatewayError::internal("Channel event bus not initialized"))?
                .clone();

            let login_result = {
                let guard = channel.read().await;
                let pwc = guard
                    .as_any()
                    .downcast_ref::<PersonalWeChatChannel>()
                    .ok_or_else(|| {
                        GatewayError::internal("Channel is not PersonalWeChatChannel")
                    })?;
                pwc.complete_login(token, base_url, event_bus).await
            };

            match login_result {
                Ok(_) => {
                    info!("✅ Personal WeChat login completed and listener started");
                }
                Err(e) => {
                    error!("❌ Failed to complete login: {}", e);
                    return Err(GatewayError::internal(format!(
                        "Failed to complete login: {}",
                        e
                    )));
                }
            }
        }

        QrStatusResponse {
            status: "confirmed".to_string(),
            bot_token: qr_status.bot_token,
            base_url: qr_status.base_url,
            message: Some("Login successful".to_string()),
        }
    } else if qr_status.status == "scanned" {
        QrStatusResponse {
            status: "scanned".to_string(),
            bot_token: None,
            base_url: None,
            message: Some("QR code scanned, waiting for confirmation".to_string()),
        }
    } else if qr_status.status == "expired" {
        QrStatusResponse {
            status: "expired".to_string(),
            bot_token: None,
            base_url: None,
            message: Some("QR code expired".to_string()),
        }
    } else {
        QrStatusResponse {
            status: "pending".to_string(),
            bot_token: None,
            base_url: None,
            message: Some("Waiting for scan".to_string()),
        }
    };

    Ok(Json(status))
}

/// List all channels
pub async fn list_channels(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ChannelInfo>>, GatewayError> {
    info!("Listing channels");

    let mut channels = Vec::new();

    // 🟢 P1 FIX: Query ChannelRegistry for actual registered channels
    if let Some(ref registry) = state.channel_registry {
        let registered = registry.list_channels().await;
        for info in registered {
            let platform_str = info.platform.to_string();
            channels.push(ChannelInfo {
                id: info.channel_type.clone(),
                name: platform_str.clone(),
                description: format!("{} channel", platform_str),
                icon: platform_icon(&platform_str).to_string(),
                enabled: info.enabled,
                status: if info.is_connected {
                    "connected".to_string()
                } else {
                    "disconnected".to_string()
                },
                config: None,
                last_error: None,
                created_at: None,
                updated_at: None,
            });
        }
    }

    // Fallback: if no channels found via registry, return default list
    if channels.is_empty() {
        channels = vec![
            ChannelInfo {
                id: "wechat".to_string(),
                name: "微信".to_string(),
                description: "WeChat".to_string(),
                icon: "💬".to_string(),
                enabled: true,
                status: "connected".to_string(),
                config: None,
                last_error: None,
                created_at: None,
                updated_at: None,
            },
            ChannelInfo {
                id: "webchat".to_string(),
                name: "WebChat".to_string(),
                description: "Web Admin Chat".to_string(),
                icon: "🌐".to_string(),
                enabled: true,
                status: "connected".to_string(),
                config: None,
                last_error: None,
                created_at: None,
                updated_at: None,
            },
            ChannelInfo {
                id: "dingtalk".to_string(),
                name: "钉钉".to_string(),
                description: "DingTalk".to_string(),
                icon: "💼".to_string(),
                enabled: false,
                status: "disabled".to_string(),
                config: None,
                last_error: None,
                created_at: None,
                updated_at: None,
            },
            ChannelInfo {
                id: "feishu".to_string(),
                name: "飞书".to_string(),
                description: "Lark".to_string(),
                icon: "🚀".to_string(),
                enabled: false,
                status: "disabled".to_string(),
                config: None,
                last_error: None,
                created_at: None,
                updated_at: None,
            },
        ];
    }

    Ok(Json(channels))
}

/// Return an emoji icon for a platform name.
fn platform_icon(platform: &str) -> &'static str {
    match platform.to_lowercase().as_str() {
        "wechat" => "💬",
        "webchat" => "🌐",
        "dingtalk" => "💼",
        "lark" | "feishu" => "🚀",
        "slack" => "💻",
        "telegram" => "✈️",
        "discord" => "🎮",
        "whatsapp" => "📱",
        "teams" => "🏢",
        "twitter" => "🐦",
        _ => "📡",
    }
}

/// Channel information
#[derive(Debug, Serialize)]
pub struct ChannelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub enabled: bool,
    pub status: String,
    pub config: Option<serde_json::Value>,
    pub last_error: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// Send a message to the WebChat channel
#[derive(Debug, Deserialize)]
pub struct SendWebChatMessageRequest {
    pub user_id: String,
    pub content: String,
    #[serde(default)]
    pub session_id: Option<String>,
}

pub async fn send_webchat_message(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<SendWebChatMessageRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    info!("Received WebChat message from user: {}", user.user_id);

    let event_bus = state
        .channel_event_bus
        .as_ref()
        .ok_or_else(|| GatewayError::internal("Channel event bus not initialized"))?
        .clone();

    let session_id = req.session_id.unwrap_or_else(|| "default".to_string());
    let thread_id = Uuid::new_v4();

    let mut metadata = std::collections::HashMap::new();
    metadata.insert("sender_id".to_string(), user.user_id.clone());
    metadata.insert("session_id".to_string(), session_id.clone());
    metadata.insert(
        "message_id".to_string(),
        format!(
            "webchat_{}_{}",
            user.user_id,
            chrono::Utc::now().timestamp_millis()
        ),
    );

    let message = Message {
        id: Uuid::new_v4(),
        thread_id,
        platform: PlatformType::WebChat,
        message_type: MessageType::Text,
        content: req.content,
        metadata,
        timestamp: chrono::Utc::now(),
    };

    let event = ChannelEvent::MessageReceived {
        platform: PlatformType::WebChat,
        channel_id: session_id,
        message,
    };

    event_bus
        .send(event)
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to send channel event: {}", e)))?;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Message sent to WebChat channel"
    })))
}

/// Get channel by ID
pub async fn get_channel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ChannelInfo>, GatewayError> {
    info!("Getting channel: {}", id);

    // 🟢 P1 FIX: Query ChannelRegistry for actual channel
    if let Some(ref registry) = state.channel_registry {
        if let Some(channel) = registry.get_channel(&id).await {
            let guard = channel.read().await;
            let platform = guard.platform();
            let platform_str = platform.to_string();
            let is_connected = guard.is_connected();

            return Ok(Json(ChannelInfo {
                id: id.clone(),
                name: platform_str.clone(),
                description: format!("{} channel", platform_str),
                icon: platform_icon(&platform_str).to_string(),
                enabled: true,
                status: if is_connected {
                    "connected".to_string()
                } else {
                    "disconnected".to_string()
                },
                config: None,
                last_error: None,
                created_at: None,
                updated_at: None,
            }));
        }
    }

    // Fallback to hardcoded list for well-known channels
    let channel = match id.as_str() {
        "wechat" => ChannelInfo {
            id: "wechat".to_string(),
            name: "微信".to_string(),
            description: "WeChat".to_string(),
            icon: "💬".to_string(),
            enabled: true,
            status: "connected".to_string(),
            config: None,
            last_error: None,
            created_at: None,
            updated_at: None,
        },
        "webchat" => ChannelInfo {
            id: "webchat".to_string(),
            name: "WebChat".to_string(),
            description: "Web Admin Chat".to_string(),
            icon: "🌐".to_string(),
            enabled: true,
            status: "connected".to_string(),
            config: None,
            last_error: None,
            created_at: None,
            updated_at: None,
        },
        _ => {
            return Err(GatewayError::not_found("Channel", &id));
        }
    };

    Ok(Json(channel))
}

/// Update channel configuration
#[derive(Debug, Deserialize)]
pub struct UpdateChannelRequest {
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub bot_token: Option<String>,
    #[serde(default)]
    pub auto_reconnect: Option<bool>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

pub async fn update_channel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateChannelRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    info!("Updating channel: {}", id);

    // Persist any config changes via ChannelRegistry
    if let Some(ref registry) = state.channel_registry {
        if let Some(channel) = registry.get_channel(&id).await {
            let mut guard = channel.write().await;
            if let Some(enabled) = req.enabled {
                if enabled {
                    guard.connect().await.map_err(|e| {
                        GatewayError::internal(format!("Failed to connect channel: {}", e))
                    })?;
                } else {
                    guard.disconnect().await.map_err(|e| {
                        GatewayError::internal(format!("Failed to disconnect channel: {}", e))
                    })?;
                }
            }
            // Note: actual config fields (base_url, bot_token, auto_reconnect)
            // are channel-type-specific. Full persistence would
            // require a per-channel config table follow-up
            // enhancement.
        }
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Channel updated",
        "channel_id": id,
    })))
}

/// Enable or disable a channel
pub async fn set_channel_enabled(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let enabled = req
        .get("enabled")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| GatewayError::bad_request("Missing 'enabled' field"))?;

    info!("Setting channel {} enabled = {}", id, enabled);

    if let Some(ref registry) = state.channel_registry {
        if let Some(channel) = registry.get_channel(&id).await {
            let mut guard = channel.write().await;
            if enabled {
                guard.connect().await.map_err(|e| {
                    GatewayError::internal(format!("Failed to connect channel: {}", e))
                })?;
            } else {
                guard.disconnect().await.map_err(|e| {
                    GatewayError::internal(format!("Failed to disconnect channel: {}", e))
                })?;
            }
        }
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "message": if enabled { "Channel enabled" } else { "Channel disabled" },
        "channel_id": id,
        "enabled": enabled,
    })))
}

/// Test channel connection
pub async fn test_channel_connection(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    info!("Testing channel connection: {}", id);

    let connected = if let Some(ref registry) = state.channel_registry {
        if let Some(channel) = registry.get_channel(&id).await {
            let guard = channel.read().await;
            guard.is_connected()
        } else {
            false
        }
    } else {
        false
    };

    Ok(Json(serde_json::json!({
        "success": connected,
        "message": if connected { "Channel connection OK" } else { "Channel not connected" },
        "channel_id": id,
    })))
}
