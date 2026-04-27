//! WebChat HTTP Handlers
//!
//! REST API for unified chat session and message management.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use gateway::error::GatewayError;
use gateway::middleware::{require_any_role, AuthUser};
use serde::{Deserialize, Serialize};
use serde_json::json;
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

    Ok(Json(
        sessions.into_iter().map(SessionResponse::from).collect(),
    ))
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

    Ok(Json(
        messages.into_iter().map(MessageResponse::from).collect(),
    ))
}

/// Update session title
pub async fn update_title(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateTitleRequest>,
) -> Result<Json<SessionResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let session = state
        .webchat_service
        .as_ref()
        .ok_or_else(|| GatewayError::internal("Webchat service not initialized"))?
        .update_title(&id, &user.user_id, &req.title)
        .await?;

    Ok(Json(SessionResponse::from(session)))
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
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    state
        .webchat_service
        .as_ref()
        .ok_or_else(|| GatewayError::internal("Webchat service not initialized"))?
        .archive_session(&id, &user.user_id)
        .await?;

    Ok(Json(
        json!({ "success": true, "message": "Session archived" }),
    ))
}

/// Clear all messages in a session
pub async fn clear_messages(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let result = sqlx::query("DELETE FROM chat_messages WHERE session_id = ?1")
        .bind(&id)
        .execute(&state.db)
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to clear messages: {}", e)))?;

    Ok(Json(json!({
        "success": true,
        "message": "Messages cleared",
        "session_id": id,
        "deleted_count": result.rows_affected(),
    })))
}

/// Get usage statistics for the authenticated user
pub async fn get_usage(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let total_sessions: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM chat_sessions WHERE user_id = ?1")
            .bind(&user.user_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);

    let total_messages: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM chat_messages m JOIN chat_sessions s ON m.session_id = s.id WHERE \
         s.user_id = ?1",
    )
    .bind(&user.user_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    // Upsert today's usage record
    let _ = sqlx::query(
        "INSERT INTO webchat_usage_daily (user_id, date, session_count, message_count)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(user_id, date) DO UPDATE SET
         session_count = excluded.session_count,
         message_count = excluded.message_count",
    )
    .bind(&user.user_id)
    .bind(&today)
    .bind(total_sessions)
    .bind(total_messages)
    .execute(&state.db)
    .await;

    let daily: Vec<serde_json::Value> = sqlx::query_as::<_, UsageRow>(
        "SELECT date, session_count, message_count, input_tokens, output_tokens
         FROM webchat_usage_daily WHERE user_id = ?1 ORDER BY date DESC LIMIT 30",
    )
    .bind(&user.user_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|r| {
        json!({
            "date": r.date,
            "session_count": r.session_count,
            "message_count": r.message_count,
            "input_tokens": r.input_tokens,
            "output_tokens": r.output_tokens,
        })
    })
    .collect();

    Ok(Json(json!({
        "total_sessions": total_sessions,
        "total_messages": total_messages,
        "daily": daily,
    })))
}

#[derive(sqlx::FromRow)]
struct UsageRow {
    date: String,
    session_count: i64,
    message_count: i64,
    input_tokens: i64,
    output_tokens: i64,
}

/// Side question request
#[derive(Debug, Deserialize)]
pub struct SideQuestionRequest {
    pub session_id: String,
    pub question: String,
}

/// Create a side question (stub — stores question for later answering)
pub async fn create_side_question(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<SideQuestionRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let id = uuid::Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO webchat_side_questions (id, session_id, user_id, question, status)
         VALUES (?1, ?2, ?3, ?4, 'pending')",
    )
    .bind(&id)
    .bind(&req.session_id)
    .bind(&user.user_id)
    .bind(&req.question)
    .execute(&state.db)
    .await
    .map_err(|e| GatewayError::internal(format!("Failed to create side question: {}", e)))?;

    Ok(Json(json!({
        "id": id,
        "question": req.question,
        "status": "pending",
        "message": "Side question created",
    })))
}

/// Export a session as JSON
pub async fn export_session(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let session = state
        .webchat_service
        .as_ref()
        .ok_or_else(|| GatewayError::internal("Webchat service not initialized"))?
        .get_session(&id, &user.user_id)
        .await?;

    let messages = state
        .webchat_service
        .as_ref()
        .unwrap()
        .get_messages(&id, &user.user_id)
        .await?;

    Ok(Json(json!({
        "session": {
            "id": session.id,
            "title": session.title,
            "channel": session.channel,
            "is_pinned": session.is_pinned,
            "is_archived": session.is_archived,
            "created_at": session.created_at.to_rfc3339(),
            "updated_at": session.updated_at.to_rfc3339(),
        },
        "messages": messages.into_iter().map(|m| json!({
            "id": m.id,
            "role": m.role,
            "content": m.content,
            "metadata": m.metadata,
            "created_at": m.created_at.to_rfc3339(),
        })).collect::<Vec<_>>(),
        "export_version": "1.0",
    })))
}

/// Import session request
#[derive(Debug, Deserialize)]
pub struct ImportSessionRequest {
    pub data: serde_json::Value,
}

/// Import a session from JSON
pub async fn import_session(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<ImportSessionRequest>,
) -> Result<impl IntoResponse, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let session_data = req
        .data
        .get("session")
        .ok_or_else(|| GatewayError::bad_request("Missing 'session' field in import data"))?;

    let title = session_data
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Imported Chat");

    let channel = session_data
        .get("channel")
        .and_then(|v| v.as_str())
        .unwrap_or("webchat");

    let session = state
        .webchat_service
        .as_ref()
        .ok_or_else(|| GatewayError::internal("Webchat service not initialized"))?
        .create_session(&user.user_id, channel, title)
        .await?;

    Ok((StatusCode::CREATED, Json(SessionResponse::from(session))))
}

/// Send a streaming message (stub — returns a stream ID)
pub async fn send_message_streaming(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let stream_id = format!("stream_{}", uuid::Uuid::new_v4());

    info!("Streaming message request for session {} (stub)", id);

    Ok(Json(json!({
        "stream_id": stream_id,
        "status": "started",
        "message": "Streaming endpoint is a stub. Use regular message send for now.",
        "session_id": id,
    })))
}
