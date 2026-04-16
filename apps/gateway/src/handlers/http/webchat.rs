//! WebChat HTTP Handlers
//!
//! REST API for unified chat session and message management.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use gateway::{
    error::GatewayError,
    middleware::{require_any_role, AuthUser},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::AppState;

/// Create session request
#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    #[serde(default = "default_title")]
    pub title: String,
    #[serde(default = "default_channel")]
    pub channel: String,
}

fn default_title() -> String {
    "New Chat".to_string()
}

fn default_channel() -> String {
    "webchat".to_string()
}

/// Update title request
#[derive(Debug, Deserialize)]
pub struct UpdateTitleRequest {
    pub title: String,
}

/// Session response
#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub id: String,
    pub user_id: String,
    pub channel: String,
    pub title: String,
    pub is_pinned: bool,
    pub is_archived: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl From<crate::services::webchat_service::ChatSession> for SessionResponse {
    fn from(s: crate::services::webchat_service::ChatSession) -> Self {
        Self {
            id: s.id,
            user_id: s.user_id,
            channel: s.channel,
            title: s.title,
            is_pinned: s.is_pinned,
            is_archived: s.is_archived,
            created_at: s.created_at.to_rfc3339(),
            updated_at: s.updated_at.to_rfc3339(),
        }
    }
}

/// Message response
#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub metadata: serde_json::Value,
    pub token_usage: Option<serde_json::Value>,
    pub created_at: String,
}

impl From<crate::services::webchat_service::ChatMessage> for MessageResponse {
    fn from(m: crate::services::webchat_service::ChatMessage) -> Self {
        Self {
            id: m.id,
            session_id: m.session_id,
            role: m.role,
            content: m.content,
            metadata: m.metadata,
            token_usage: m.token_usage,
            created_at: m.created_at.to_rfc3339(),
        }
    }
}

/// List all chat sessions for the authenticated user
pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Result<Json<Vec<SessionResponse>>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let sessions = state
        .webchat_service
        .as_ref()
        .ok_or_else(|| GatewayError::internal("Webchat service not initialized"))?
        .list_sessions(&user.user_id)
        .await?;

    Ok(Json(sessions.into_iter().map(SessionResponse::from).collect()))
}

/// Create a new chat session
pub async fn create_session(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<CreateSessionRequest>,
) -> Result<impl IntoResponse, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let session = state
        .webchat_service
        .as_ref()
        .ok_or_else(|| GatewayError::internal("Webchat service not initialized"))?
        .create_session(&user.user_id, &req.channel, &req.title)
        .await?;

    Ok((StatusCode::CREATED, Json(SessionResponse::from(session))))
}

/// Delete a chat session
pub async fn delete_session(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    state
        .webchat_service
        .as_ref()
        .ok_or_else(|| GatewayError::internal("Webchat service not initialized"))?
        .delete_session(&id, &user.user_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Get messages for a session
pub async fn get_messages(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Vec<MessageResponse>>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let messages = state
        .webchat_service
        .as_ref()
        .ok_or_else(|| GatewayError::internal("Webchat service not initialized"))?
        .get_messages(&id, &user.user_id)
        .await?;

    Ok(Json(messages.into_iter().map(MessageResponse::from).collect()))
}

/// Update session title
pub async fn update_title(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateTitleRequest>,
) -> Result<StatusCode, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    state
        .webchat_service
        .as_ref()
        .ok_or_else(|| GatewayError::internal("Webchat service not initialized"))?
        .update_title(&id, &user.user_id, &req.title)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Toggle pin status
pub async fn toggle_pin(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let pinned = state
        .webchat_service
        .as_ref()
        .ok_or_else(|| GatewayError::internal("Webchat service not initialized"))?
        .toggle_pin(&id, &user.user_id)
        .await?;

    Ok(Json(json!({ "is_pinned": pinned })))
}

/// Archive a session
pub async fn archive_session(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    state
        .webchat_service
        .as_ref()
        .ok_or_else(|| GatewayError::internal("Webchat service not initialized"))?
        .archive_session(&id, &user.user_id)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
