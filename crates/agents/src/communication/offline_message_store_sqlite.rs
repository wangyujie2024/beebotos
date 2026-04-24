//! SQLite implementation of `OfflineMessageStore`.

use async_trait::async_trait;
use sqlx::SqlitePool;
use tracing::{info, warn};

use crate::communication::message_router_v2::UserMessageContext;
use crate::communication::offline_message_store::OfflineMessageStore;
use crate::error::{AgentError, Result};

/// Maximum number of offline messages stored per agent.
const MAX_OFFLINE_PER_AGENT: i64 = 500;

pub struct SqliteOfflineMessageStore {
    pool: SqlitePool,
}

impl SqliteOfflineMessageStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl OfflineMessageStore for SqliteOfflineMessageStore {
    async fn enqueue(&self, agent_id: &str, ctx: &UserMessageContext) -> Result<()> {
        let payload = serde_json::to_string(ctx).map_err(|e| {
            AgentError::serialization(format!("Failed to serialize offline message: {}", e))
        })?;

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AgentError::Database(format!("Failed to begin transaction: {}", e)))?;

        // Count current messages for this agent
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM agent_offline_messages WHERE agent_id = ?1"#,
        )
        .bind(agent_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AgentError::Database(format!("Failed to count offline messages: {}", e)))?;

        if count >= MAX_OFFLINE_PER_AGENT {
            // Remove oldest message to make room
            sqlx::query(
                r#"
                DELETE FROM agent_offline_messages
                WHERE id = (
                    SELECT id FROM agent_offline_messages
                    WHERE agent_id = ?1
                    ORDER BY created_at ASC
                    LIMIT 1
                )
                "#,
            )
            .bind(agent_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                AgentError::Database(format!("Failed to prune offline messages: {}", e))
            })?;
            warn!(
                "Agent {} offline queue overflow, dropped oldest message",
                agent_id
            );
        }

        sqlx::query(
            r#"
            INSERT INTO agent_offline_messages (agent_id, payload)
            VALUES (?1, ?2)
            "#,
        )
        .bind(agent_id)
        .bind(payload)
        .execute(&mut *tx)
        .await
        .map_err(|e| AgentError::Database(format!("Failed to enqueue offline message: {}", e)))?;

        tx.commit()
            .await
            .map_err(|e| AgentError::Database(format!("Failed to commit enqueue: {}", e)))?;

        Ok(())
    }

    async fn dequeue_all(&self, agent_id: &str) -> Result<Vec<UserMessageContext>> {
        let rows = sqlx::query_as::<_, OfflineMessageRow>(
            r#"
            SELECT id, payload FROM agent_offline_messages
            WHERE agent_id = ?1
            ORDER BY created_at ASC
            "#,
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AgentError::Database(format!("Failed to fetch offline messages: {}", e)))?;

        let mut messages = Vec::with_capacity(rows.len());
        for row in rows {
            match serde_json::from_str::<UserMessageContext>(&row.payload) {
                Ok(ctx) => messages.push(ctx),
                Err(e) => {
                    warn!("Failed to deserialize offline message {}: {}", row.id, e);
                    // Delete corrupted message
                    sqlx::query("DELETE FROM agent_offline_messages WHERE id = ?1")
                        .bind(&row.id)
                        .execute(&self.pool)
                        .await
                        .ok();
                }
            }
        }

        // Delete all fetched messages for this agent
        let deleted = sqlx::query("DELETE FROM agent_offline_messages WHERE agent_id = ?1")
            .bind(agent_id)
            .execute(&self.pool)
            .await
            .map_err(|e| AgentError::Database(format!("Failed to delete offline messages: {}", e)))?
            .rows_affected();

        if deleted > 0 {
            info!(
                "Dequeued {} offline messages for agent {}",
                deleted, agent_id
            );
        }

        Ok(messages)
    }

    async fn clear(&self, agent_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM agent_offline_messages WHERE agent_id = ?1")
            .bind(agent_id)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                AgentError::Database(format!("Failed to clear offline messages: {}", e))
            })?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct OfflineMessageRow {
    id: String,
    payload: String,
}

/// In-memory offline message store for testing.
pub struct MemoryOfflineMessageStore {
    max_per_agent: usize,
    data: std::sync::Arc<
        tokio::sync::RwLock<std::collections::HashMap<String, Vec<UserMessageContext>>>,
    >,
}

impl MemoryOfflineMessageStore {
    pub fn new(max_per_agent: usize) -> Self {
        Self {
            max_per_agent,
            data: std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        }
    }
}

#[async_trait]
impl OfflineMessageStore for MemoryOfflineMessageStore {
    async fn enqueue(&self, agent_id: &str, ctx: &UserMessageContext) -> Result<()> {
        let mut data = self.data.write().await;
        let queue = data.entry(agent_id.to_string()).or_default();
        if queue.len() >= self.max_per_agent {
            queue.remove(0);
        }
        queue.push(ctx.clone());
        Ok(())
    }

    async fn dequeue_all(&self, agent_id: &str) -> Result<Vec<UserMessageContext>> {
        let mut data = self.data.write().await;
        Ok(data.remove(agent_id).unwrap_or_default())
    }

    async fn clear(&self, agent_id: &str) -> Result<()> {
        let mut data = self.data.write().await;
        data.remove(agent_id);
        Ok(())
    }
}
