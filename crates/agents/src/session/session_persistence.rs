//! Session Persistence Layer
//!
//! Provides persistent storage for session state across restarts.
//! Uses SQLite for durable storage.
//!
//! # Features
//! - SQLite: Durable relational storage
//! - In-memory: For testing

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::error::Result;
use crate::session::unified_session::{SessionPersistence, UnifiedSession, UnifiedSessionState};

/// SQLite session persistence
#[cfg(feature = "sqlite")]
pub struct SqliteSessionPersistence {
    pool: sqlx::SqlitePool,
}

#[cfg(feature = "sqlite")]
impl SqliteSessionPersistence {
    /// Create new SQLite persistence
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
            .map_err(|e| {
                crate::error::AgentError::configuration(format!(
                    "Failed to connect to SQLite: {}",
                    e
                ))
            })?;

        // Initialize schema
        Self::init_schema(&pool).await?;

        info!("Connected to SQLite for session persistence");

        Ok(Self { pool })
    }

    async fn init_schema(pool: &sqlx::SqlitePool) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                pool_session_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                state TEXT NOT NULL,
                capabilities TEXT,
                context TEXT,
                metadata TEXT,
                parent_session TEXT,
                child_sessions TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(pool)
        .await
        .map_err(|e| {
            crate::error::AgentError::storage(format!("Failed to create sessions table: {}", e))
        })?;

        // Create index on agent_id
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_sessions_agent_id ON sessions(agent_id)")
            .execute(pool)
            .await
            .ok();

        // Create index on state
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_sessions_state ON sessions(state)")
            .execute(pool)
            .await
            .ok();

        Ok(())
    }
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl SessionPersistence for SqliteSessionPersistence {
    async fn save_session(&self, session: &UnifiedSession) -> Result<()> {
        let session_json = serde_json::to_string(session).map_err(|e| {
            crate::error::AgentError::serialization(format!("Failed to serialize session: {}", e))
        })?;
        let session_value: serde_json::Value =
            serde_json::from_str(&session_json).map_err(|e| {
                crate::error::AgentError::serialization(format!(
                    "Failed to parse session JSON: {}",
                    e
                ))
            })?;

        sqlx::query(
            r#"
            INSERT INTO sessions (
                id, pool_session_id, agent_id, state, capabilities, context, metadata,
                parent_session, child_sessions, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'))
            ON CONFLICT (id) DO UPDATE SET
                pool_session_id = excluded.pool_session_id,
                agent_id = excluded.agent_id,
                state = excluded.state,
                capabilities = excluded.capabilities,
                context = excluded.context,
                metadata = excluded.metadata,
                parent_session = excluded.parent_session,
                child_sessions = excluded.child_sessions,
                updated_at = datetime('now')
            "#,
        )
        .bind(&session.session_key.to_string())
        .bind(&session.pool_session_id.to_string())
        .bind(&session.agent_id)
        .bind(format!("{:?}", session.state))
        .bind(session_value.get("capabilities").map(|v| v.to_string()))
        .bind(session_value.get("context").map(|v| v.to_string()))
        .bind(session_value.get("metadata").map(|v| v.to_string()))
        .bind(session.parent_session.as_ref().map(|k| k.to_string()))
        .bind(session_value.get("child_sessions").map(|v| v.to_string()))
        .execute(&self.pool)
        .await
        .map_err(|e| {
            crate::error::AgentError::storage(format!("Failed to save session to SQLite: {}", e))
        })?;

        debug!("Saved session to SQLite: {}", session.session_key);
        Ok(())
    }

    async fn load_session(&self, session_id: &str) -> Result<Option<UnifiedSession>> {
        let row: Option<(
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        )> = sqlx::query_as(
            r#"
            SELECT
                id, pool_session_id, agent_id, state, capabilities, context, metadata,
                parent_session, child_sessions
            FROM sessions WHERE id = ?1
            "#,
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            crate::error::AgentError::storage(format!("Failed to load session from SQLite: {}", e))
        })?;

        match row {
            Some((id, pool_id, agent_id, state_str, caps, ctx, meta, parent, children)) => {
                // Reconstruct session from parts
                let session = UnifiedSession {
                    pool_session_id: pool_id.parse().map_err(|_| {
                        crate::error::AgentError::serialization(
                            "Invalid pool_session_id".to_string(),
                        )
                    })?,
                    session_key: id.parse().map_err(|_| {
                        crate::error::AgentError::serialization("Invalid session_key".to_string())
                    })?,
                    agent_id,
                    capabilities: caps
                        .and_then(|c| serde_json::from_str(&c).ok())
                        .unwrap_or_default(),
                    state: parse_state(&state_str),
                    context: ctx
                        .and_then(|c| serde_json::from_str(&c).ok())
                        .unwrap_or_default(),
                    metadata: meta
                        .and_then(|m| serde_json::from_str(&m).ok())
                        .unwrap_or_default(),
                    parent_session: parent.and_then(|p| p.parse().ok()),
                    child_sessions: children
                        .and_then(|c| serde_json::from_str(&c).ok())
                        .unwrap_or_default(),
                };
                debug!("Loaded session from SQLite: {}", session_id);
                Ok(Some(session))
            }
            None => Ok(None),
        }
    }

    async fn delete_session(&self, session_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM sessions WHERE id = ?1")
            .bind(session_id)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                crate::error::AgentError::storage(format!(
                    "Failed to delete session from SQLite: {}",
                    e
                ))
            })?;

        debug!("Deleted session from SQLite: {}", session_id);
        Ok(())
    }

    async fn list_sessions(&self) -> Result<Vec<UnifiedSession>> {
        let rows: Vec<(
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        )> = sqlx::query_as(
            r#"
            SELECT
                id, pool_session_id, agent_id, state, capabilities, context, metadata,
                parent_session, child_sessions
            FROM sessions
            WHERE state NOT IN ('Closed', 'Terminating')
            ORDER BY updated_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            crate::error::AgentError::storage(format!("Failed to list sessions from SQLite: {}", e))
        })?;

        let mut sessions = Vec::new();
        for (id, pool_id, agent_id, state_str, caps, ctx, meta, parent, children) in rows {
            if let (Ok(pool_id), Ok(session_key)) = (pool_id.parse(), id.parse()) {
                sessions.push(UnifiedSession {
                    pool_session_id: pool_id,
                    session_key,
                    agent_id,
                    capabilities: caps
                        .and_then(|c| serde_json::from_str(&c).ok())
                        .unwrap_or_default(),
                    state: parse_state(&state_str),
                    context: ctx
                        .and_then(|c| serde_json::from_str(&c).ok())
                        .unwrap_or_default(),
                    metadata: meta
                        .and_then(|m| serde_json::from_str(&m).ok())
                        .unwrap_or_default(),
                    parent_session: parent.and_then(|p| p.parse().ok()),
                    child_sessions: children
                        .and_then(|c| serde_json::from_str(&c).ok())
                        .unwrap_or_default(),
                });
            }
        }

        Ok(sessions)
    }

    async fn update_state(&self, session_id: &str, state: UnifiedSessionState) -> Result<()> {
        sqlx::query("UPDATE sessions SET state = ?1, updated_at = datetime('now') WHERE id = ?2")
            .bind(format!("{:?}", state))
            .bind(session_id)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                crate::error::AgentError::storage(format!(
                    "Failed to update session state in SQLite: {}",
                    e
                ))
            })?;

        Ok(())
    }
}

#[cfg(feature = "sqlite")]
fn parse_state(state_str: &str) -> UnifiedSessionState {
    match state_str {
        "Initializing" => UnifiedSessionState::Initializing,
        "Active" => UnifiedSessionState::Active,
        "Busy" => UnifiedSessionState::Busy,
        "Idle" => UnifiedSessionState::Idle,
        "Paused" => UnifiedSessionState::Paused,
        "Hibernating" => UnifiedSessionState::Hibernating,
        "Unhealthy" => UnifiedSessionState::Unhealthy,
        "Terminating" => UnifiedSessionState::Terminating,
        "Closed" => UnifiedSessionState::Closed,
        _ => UnifiedSessionState::Active,
    }
}

/// In-memory session persistence (for testing)
pub struct InMemorySessionPersistence {
    sessions: Arc<RwLock<HashMap<String, UnifiedSession>>>,
}

impl InMemorySessionPersistence {
    /// Create new in-memory persistence
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemorySessionPersistence {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SessionPersistence for InMemorySessionPersistence {
    async fn save_session(&self, session: &UnifiedSession) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions.insert(session.session_key.to_string(), session.clone());
        Ok(())
    }

    async fn load_session(&self, session_id: &str) -> Result<Option<UnifiedSession>> {
        let sessions = self.sessions.read().await;
        Ok(sessions.get(session_id).cloned())
    }

    async fn delete_session(&self, session_id: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id);
        Ok(())
    }

    async fn list_sessions(&self) -> Result<Vec<UnifiedSession>> {
        let sessions = self.sessions.read().await;
        Ok(sessions.values().cloned().collect())
    }

    async fn update_state(&self, session_id: &str, state: UnifiedSessionState) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(session_id) {
            session.state = state;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_persistence() {
        let persistence = InMemorySessionPersistence::new();

        // Create a mock session with valid session key format: agent:<id>:<type>:<uuid>
        let session = UnifiedSession {
            pool_session_id: uuid::Uuid::new_v4(),
            session_key: format!("agent:test-agent:standard:{}", uuid::Uuid::new_v4())
                .parse()
                .unwrap(),
            agent_id: "test-agent".to_string(),
            capabilities: crate::runtime::session_pool::SessionCapabilities::default(),
            state: UnifiedSessionState::Active,
            context: crate::session::SessionContext::new("test".to_string()),
            metadata: crate::session::unified_session::SessionMetadata::default(),
            parent_session: None,
            child_sessions: vec![],
        };

        let session_key = session.session_key.to_string();

        // Save
        persistence.save_session(&session).await.unwrap();

        // Load using the correct session key
        let loaded = persistence.load_session(&session_key).await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().agent_id, "test-agent");

        // Update state
        persistence
            .update_state(&session_key, UnifiedSessionState::Busy)
            .await
            .unwrap();

        // Verify
        let loaded = persistence
            .load_session(&session_key)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(loaded.state, UnifiedSessionState::Busy));

        // Delete
        persistence.delete_session(&session_key).await.unwrap();
        let loaded = persistence.load_session(&session_key).await.unwrap();
        assert!(loaded.is_none());
    }
}
