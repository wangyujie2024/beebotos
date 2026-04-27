//! Channel-Agent Binding Store
//!
//! Lightweight SQLite-backed store with DashMap in-memory cache
//! for managing channel-to-agent bindings.
//!
//! Key format: "{platform}:{channel_id}"

use std::sync::Arc;

use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tracing::{debug, info, warn};

use crate::error::{GatewayError, Result};

/// Channel-Agent binding record (LEGACY — deprecated, use
/// `agent_channel_bindings` table instead)
///
/// P2 OPTIMIZE: This struct and the `ChannelBindingStore` below are part of the
/// legacy binding system. New code should use `AgentChannelService` +
/// `AgentChannelBinding` from `beebotos_agents::services` and
/// `beebotos_agents::communication::agent_channel`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelBinding {
    pub platform: String,
    pub channel_id: String,
    pub agent_id: String,
    pub created_at: String,
}

/// Store for channel-agent bindings (LEGACY — deprecated)
///
/// P2 OPTIMIZE: This store is maintained for backward compatibility only.
/// The new multi-user multi-agent architecture uses:
/// - `UserChannelService` for user-channel lifecycle
/// - `AgentChannelService` for agent-channel bindings with routing rules
///
/// To migrate existing data, call `POST /api/v1/admin/migrate-bindings` once
/// after the new-system services are initialized.
#[derive(Clone)]
pub struct ChannelBindingStore {
    db: SqlitePool,
    cache: Arc<DashMap<String, String>>,
}

impl ChannelBindingStore {
    /// Create a new binding store, ensuring the DB schema exists
    pub async fn new(db: SqlitePool) -> Result<Self> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS channel_agent_bindings (
                platform TEXT NOT NULL,
                channel_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                PRIMARY KEY (platform, channel_id)
            );
            CREATE INDEX IF NOT EXISTS idx_bindings_agent_id ON channel_agent_bindings(agent_id);
            "#,
        )
        .execute(&db)
        .await
        .map_err(|e| GatewayError::Internal {
            message: format!("Failed to create channel bindings table: {}", e),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

        let cache = Arc::new(DashMap::new());

        // Warm cache from DB
        let rows: Vec<(String, String, String)> =
            sqlx::query_as("SELECT platform, channel_id, agent_id FROM channel_agent_bindings")
                .fetch_all(&db)
                .await
                .map_err(|e| GatewayError::Internal {
                    correlation_id: uuid::Uuid::new_v4().to_string(),
                    message: format!("Failed to load channel bindings: {}", e),
                })?;

        for (platform, channel_id, agent_id) in rows {
            let key = format!("{}:{}", platform, channel_id);
            cache.insert(key, agent_id);
        }

        info!(
            "ChannelBindingStore initialized with {} cached bindings",
            cache.len()
        );

        Ok(Self { db, cache })
    }

    /// Bind a channel to an agent
    pub async fn bind(&self, platform: &str, channel_id: &str, agent_id: &str) -> Result<()> {
        let created_at = Utc::now().to_rfc3339();
        let key = format!("{}:{}", platform, channel_id);

        sqlx::query(
            r#"
            INSERT INTO channel_agent_bindings (platform, channel_id, agent_id, created_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(platform, channel_id) DO UPDATE SET
                agent_id = excluded.agent_id,
                created_at = excluded.created_at
            "#,
        )
        .bind(platform)
        .bind(channel_id)
        .bind(agent_id)
        .bind(&created_at)
        .execute(&self.db)
        .await
        .map_err(|e| GatewayError::Internal {
            correlation_id: uuid::Uuid::new_v4().to_string(),
            message: format!("Failed to bind channel to agent: {}", e),
        })?;

        self.cache.insert(key, agent_id.to_string());
        info!(
            "Bound channel {}:{} to agent {}",
            platform, channel_id, agent_id
        );
        Ok(())
    }

    /// Unbind a channel from its agent
    pub async fn unbind(&self, platform: &str, channel_id: &str) -> Result<()> {
        let key = format!("{}:{}", platform, channel_id);

        let result = sqlx::query(
            "DELETE FROM channel_agent_bindings WHERE platform = ?1 AND channel_id = ?2",
        )
        .bind(platform)
        .bind(channel_id)
        .execute(&self.db)
        .await
        .map_err(|e| GatewayError::Internal {
            correlation_id: uuid::Uuid::new_v4().to_string(),
            message: format!("Failed to unbind channel from agent: {}", e),
        })?;

        self.cache.remove(&key);

        if result.rows_affected() > 0 {
            info!("Unbound channel {}:{}", platform, channel_id);
        } else {
            warn!(
                "Attempted to unbind non-existent channel {}:{}",
                platform, channel_id
            );
        }

        Ok(())
    }

    /// Resolve the bound agent_id for a given platform + channel_id
    pub async fn resolve_agent(&self, platform: &str, channel_id: &str) -> Option<String> {
        let key = format!("{}:{}", platform, channel_id);

        if let Some(entry) = self.cache.get(&key) {
            debug!("Resolved agent {} for {} from cache", entry.value(), key);
            return Some(entry.value().clone());
        }

        // Cache miss — fallback to DB (unlikely since we warm cache)
        match sqlx::query_as::<_, (String,)>(
            "SELECT agent_id FROM channel_agent_bindings WHERE platform = ?1 AND channel_id = ?2",
        )
        .bind(platform)
        .bind(channel_id)
        .fetch_optional(&self.db)
        .await
        {
            Ok(Some((agent_id,))) => {
                self.cache.insert(key, agent_id.clone());
                Some(agent_id)
            }
            Ok(None) => None,
            Err(e) => {
                warn!("DB lookup failed for channel binding {}: {}", key, e);
                None
            }
        }
    }

    /// List all bindings for a specific agent
    pub async fn list_bindings_for_agent(&self, agent_id: &str) -> Result<Vec<ChannelBinding>> {
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT platform, channel_id, agent_id, created_at FROM channel_agent_bindings WHERE \
             agent_id = ?1",
        )
        .bind(agent_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| GatewayError::Internal {
            correlation_id: uuid::Uuid::new_v4().to_string(),
            message: format!("Failed to list bindings for agent: {}", e),
        })?;

        Ok(rows
            .into_iter()
            .map(
                |(platform, channel_id, agent_id, created_at)| ChannelBinding {
                    platform,
                    channel_id,
                    agent_id,
                    created_at,
                },
            )
            .collect())
    }

    /// List all bindings (paginated, optional)
    pub async fn list_all_bindings(&self) -> Result<Vec<ChannelBinding>> {
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT platform, channel_id, agent_id, created_at FROM channel_agent_bindings",
        )
        .fetch_all(&self.db)
        .await
        .map_err(|e| GatewayError::Internal {
            correlation_id: uuid::Uuid::new_v4().to_string(),
            message: format!("Failed to list all channel bindings: {}", e),
        })?;

        Ok(rows
            .into_iter()
            .map(
                |(platform, channel_id, agent_id, created_at)| ChannelBinding {
                    platform,
                    channel_id,
                    agent_id,
                    created_at,
                },
            )
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_test_store() -> ChannelBindingStore {
        let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
        ChannelBindingStore::new(db).await.unwrap()
    }

    #[tokio::test]
    async fn test_bind_and_resolve() {
        let store = create_test_store().await;

        store.bind("webchat", "session-1", "agent-a").await.unwrap();

        let resolved = store.resolve_agent("webchat", "session-1").await;
        assert_eq!(resolved, Some("agent-a".to_string()));
    }

    #[tokio::test]
    async fn test_unbind() {
        let store = create_test_store().await;

        store.bind("webchat", "session-1", "agent-a").await.unwrap();
        store.unbind("webchat", "session-1").await.unwrap();

        let resolved = store.resolve_agent("webchat", "session-1").await;
        assert_eq!(resolved, None);
    }

    #[tokio::test]
    async fn test_bind_overwrite() {
        let store = create_test_store().await;

        store.bind("webchat", "session-1", "agent-a").await.unwrap();
        store.bind("webchat", "session-1", "agent-b").await.unwrap();

        let resolved = store.resolve_agent("webchat", "session-1").await;
        assert_eq!(resolved, Some("agent-b".to_string()));
    }

    #[tokio::test]
    async fn test_list_bindings_for_agent() {
        let store = create_test_store().await;

        store.bind("webchat", "s1", "agent-a").await.unwrap();
        store.bind("lark", "s2", "agent-a").await.unwrap();
        store.bind("discord", "s3", "agent-b").await.unwrap();

        let bindings = store.list_bindings_for_agent("agent-a").await.unwrap();
        assert_eq!(bindings.len(), 2);
    }
}
