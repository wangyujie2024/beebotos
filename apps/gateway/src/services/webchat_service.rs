//! WebChat Service
//!
//! Unified chat persistence and session management for all channels
//! (webchat, personal_wechat, lark, dingtalk, qq, feishu, etc.).

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::SqlitePool;
use tracing::{error, info, warn};

use crate::error::AppError;

/// Chat session model
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatSession {
    pub id: String,
    pub user_id: String,
    pub channel: String,
    pub title: String,
    pub is_pinned: bool,
    pub is_archived: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// SQLite-compatible row for ChatSession
#[derive(sqlx::FromRow)]
struct ChatSessionRow {
    id: String,
    user_id: String,
    channel: String,
    title: String,
    is_pinned: i32,
    is_archived: i32,
    created_at: String,
    updated_at: String,
}

impl TryFrom<ChatSessionRow> for ChatSession {
    type Error = String;

    fn try_from(row: ChatSessionRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            user_id: row.user_id,
            channel: row.channel,
            title: row.title,
            is_pinned: row.is_pinned != 0,
            is_archived: row.is_archived != 0,
            created_at: row
                .created_at
                .parse()
                .map_err(|e| format!("Invalid datetime: {}", e))?,
            updated_at: row
                .updated_at
                .parse()
                .map_err(|e| format!("Invalid datetime: {}", e))?,
        })
    }
}

/// Chat message model
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub metadata: Value,
    pub token_usage: Option<Value>,
    pub created_at: DateTime<Utc>,
}

/// SQLite-compatible row for ChatMessage
#[derive(sqlx::FromRow)]
struct ChatMessageRow {
    id: String,
    session_id: String,
    role: String,
    content: String,
    metadata: String,
    token_usage: Option<String>,
    created_at: String,
}

impl TryFrom<ChatMessageRow> for ChatMessage {
    type Error = String;

    fn try_from(row: ChatMessageRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            session_id: row.session_id,
            role: row.role,
            content: row.content,
            metadata: serde_json::from_str(&row.metadata)
                .map_err(|e| format!("Invalid metadata JSON: {}", e))?,
            token_usage: row
                .token_usage
                .map(|s| serde_json::from_str(&s))
                .transpose()
                .map_err(|e| format!("Invalid token_usage JSON: {}", e))?,
            created_at: row
                .created_at
                .parse()
                .map_err(|e| format!("Invalid datetime: {}", e))?,
        })
    }
}

/// WebChat service for unified chat management
#[derive(Debug, Clone)]
pub struct WebchatService {
    db: SqlitePool,
}

impl WebchatService {
    /// Create a new WebchatService
    pub fn new(db: SqlitePool) -> Self {
        Self { db }
    }

    /// Create a new chat session
    pub async fn create_session(
        &self,
        user_id: &str,
        channel: &str,
        title: &str,
    ) -> Result<ChatSession, AppError> {
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO chat_sessions (user_id, channel, title, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?4)
            "#,
        )
        .bind(user_id)
        .bind(channel)
        .bind(title)
        .bind(&now)
        .execute(&self.db)
        .await
        .map_err(|e| {
            error!("Failed to create chat session: {}", e);
            AppError::database(e)
        })?;

        let row: ChatSessionRow =
            sqlx::query_as("SELECT * FROM chat_sessions WHERE user_id = ?1 AND created_at = ?2")
                .bind(user_id)
                .bind(&now)
                .fetch_one(&self.db)
                .await
                .map_err(|e| AppError::database(e))?;

        let session: ChatSession = row
            .try_into()
            .map_err(|e: String| AppError::Internal(format!("Failed to parse session: {}", e)))?;

        info!("Created chat session {} for user {}", session.id, user_id);
        Ok(session)
    }

    /// Get a single session by ID, verifying ownership
    pub async fn get_session(
        &self,
        session_id: &str,
        user_id: &str,
    ) -> Result<ChatSession, AppError> {
        let row: ChatSessionRow =
            sqlx::query_as("SELECT * FROM chat_sessions WHERE id = ?1 AND user_id = ?2")
                .bind(session_id)
                .bind(user_id)
                .fetch_one(&self.db)
                .await
                .map_err(|e| match e {
                    sqlx::Error::RowNotFound => AppError::not_found("Session", session_id),
                    _ => AppError::database(e),
                })?;

        row.try_into()
            .map_err(|e: String| AppError::Internal(format!("Failed to parse session: {}", e)))
    }

    /// List sessions for a user, ordered by updated_at desc
    pub async fn list_sessions(&self, user_id: &str) -> Result<Vec<ChatSession>, AppError> {
        let rows: Vec<ChatSessionRow> = sqlx::query_as(
            r#"
            SELECT * FROM chat_sessions
            WHERE user_id = ?1 AND is_archived = 0
            ORDER BY is_pinned DESC, updated_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::database(e))?;

        let sessions: Result<Vec<_>, _> = rows.into_iter().map(|r| r.try_into()).collect();

        sessions.map_err(|e: String| AppError::Internal(format!("Failed to parse sessions: {}", e)))
    }

    /// Get messages for a session, verifying ownership
    pub async fn get_messages(
        &self,
        session_id: &str,
        user_id: &str,
    ) -> Result<Vec<ChatMessage>, AppError> {
        // Verify ownership
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM chat_sessions WHERE id = ?1 AND user_id = ?2")
                .bind(session_id)
                .bind(user_id)
                .fetch_one(&self.db)
                .await
                .map_err(|e| AppError::database(e))?;

        if count == 0 {
            return Err(AppError::not_found("Session", session_id));
        }

        let rows: Vec<ChatMessageRow> = sqlx::query_as(
            r#"
            SELECT * FROM chat_messages
            WHERE session_id = ?1
            ORDER BY created_at ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::database(e))?;

        let messages: Result<Vec<_>, _> = rows.into_iter().map(|r| r.try_into()).collect();

        messages.map_err(|e: String| AppError::Internal(format!("Failed to parse messages: {}", e)))
    }

    /// Save a message to a session
    pub async fn save_message(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
        metadata: Option<Value>,
        token_usage: Option<Value>,
    ) -> Result<String, AppError> {
        let id = uuid::Uuid::new_v4().to_string();
        let metadata_json = metadata
            .map(|v| v.to_string())
            .unwrap_or_else(|| "{}".to_string());
        let token_usage_json = token_usage.map(|v| v.to_string());
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO chat_messages (id, session_id, role, content, metadata, token_usage, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
        )
        .bind(&id)
        .bind(session_id)
        .bind(role)
        .bind(content)
        .bind(&metadata_json)
        .bind(token_usage_json.as_deref())
        .bind(&now)
        .execute(&self.db)
        .await
        .map_err(|e| {
            error!("Failed to save chat message: {}", e);
            AppError::database(e)
        })?;

        Ok(id)
    }

    /// Update session title
    pub async fn update_title(
        &self,
        session_id: &str,
        user_id: &str,
        title: &str,
    ) -> Result<ChatSession, AppError> {
        let result =
            sqlx::query("UPDATE chat_sessions SET title = ?1 WHERE id = ?2 AND user_id = ?3")
                .bind(title)
                .bind(session_id)
                .bind(user_id)
                .execute(&self.db)
                .await
                .map_err(|e| AppError::database(e))?;

        if result.rows_affected() == 0 {
            return Err(AppError::not_found("Session", session_id));
        }

        let row: ChatSessionRow = sqlx::query_as(
            "SELECT id, user_id, channel, title, is_pinned, is_archived, created_at, updated_at \
             FROM chat_sessions WHERE id = ?1 AND user_id = ?2",
        )
        .bind(session_id)
        .bind(user_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::database(e))?;

        row.try_into().map_err(|e: String| AppError::Internal(e))
    }

    /// Toggle pin status, returning new pin state
    pub async fn toggle_pin(&self, session_id: &str, user_id: &str) -> Result<bool, AppError> {
        let current: Option<i32> = sqlx::query_scalar(
            "SELECT is_pinned FROM chat_sessions WHERE id = ?1 AND user_id = ?2",
        )
        .bind(session_id)
        .bind(user_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::database(e))?;

        let current = current.ok_or_else(|| AppError::not_found("Session", session_id))?;
        let new_pinned = if current == 0 { 1 } else { 0 };

        sqlx::query("UPDATE chat_sessions SET is_pinned = ?1 WHERE id = ?2 AND user_id = ?3")
            .bind(new_pinned)
            .bind(session_id)
            .bind(user_id)
            .execute(&self.db)
            .await
            .map_err(|e| AppError::database(e))?;

        Ok(new_pinned != 0)
    }

    /// Archive a session
    pub async fn archive_session(&self, session_id: &str, user_id: &str) -> Result<(), AppError> {
        let result =
            sqlx::query("UPDATE chat_sessions SET is_archived = 1 WHERE id = ?1 AND user_id = ?2")
                .bind(session_id)
                .bind(user_id)
                .execute(&self.db)
                .await
                .map_err(|e| AppError::database(e))?;

        if result.rows_affected() == 0 {
            return Err(AppError::not_found("Session", session_id));
        }

        Ok(())
    }

    /// Delete a session (cascades to messages via FK)
    pub async fn delete_session(&self, session_id: &str, user_id: &str) -> Result<(), AppError> {
        let result = sqlx::query("DELETE FROM chat_sessions WHERE id = ?1 AND user_id = ?2")
            .bind(session_id)
            .bind(user_id)
            .execute(&self.db)
            .await
            .map_err(|e| AppError::database(e))?;

        if result.rows_affected() == 0 {
            return Err(AppError::not_found("Session", session_id));
        }

        info!("Deleted chat session {}", session_id);
        Ok(())
    }

    /// Validate that a session exists and belongs to the given user
    pub async fn validate_session(
        &self,
        session_id: &str,
        user_id: &str,
    ) -> Result<bool, AppError> {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM chat_sessions WHERE id = ?1 AND user_id = ?2")
                .bind(session_id)
                .bind(user_id)
                .fetch_one(&self.db)
                .await
                .map_err(|e| AppError::database(e))?;
        Ok(count > 0)
    }

    /// Get or create a session for external channels (personal_wechat, lark,
    /// etc.)
    pub async fn get_or_create_channel_session(
        &self,
        user_id: &str,
        channel: &str,
        _sender_id: &str,
    ) -> Result<String, AppError> {
        // Look for existing session for this user + channel
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT id FROM chat_sessions WHERE user_id = ?1 AND channel = ?2 LIMIT 1",
        )
        .bind(user_id)
        .bind(channel)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::database(e))?;

        if let Some(id) = existing {
            return Ok(id);
        }

        // Create new session
        let title = format!("{} Chat", channel);
        let session = self.create_session(user_id, channel, &title).await?;
        info!(
            "Created new channel session {} for {} / {}",
            session.id, channel, user_id
        );
        Ok(session.id)
    }
}
