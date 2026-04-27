//! SQLite implementation of `AgentChannelBindingStore`.

use async_trait::async_trait;
use sqlx::SqlitePool;

use super::agent_channel_store::AgentChannelBindingStore;
use crate::communication::agent_channel::{AgentChannelBinding, RoutingRules};
use crate::error::{AgentError, Result};

pub struct SqliteAgentChannelBindingStore {
    pool: SqlitePool,
}

impl SqliteAgentChannelBindingStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AgentChannelBindingStore for SqliteAgentChannelBindingStore {
    async fn bind(&self, binding: &AgentChannelBinding) -> Result<()> {
        let rules_json = serde_json::to_string(&binding.routing_rules)
            .map_err(|e| AgentError::serialization(e.to_string()))?;

        sqlx::query(
            r#"
            INSERT INTO agent_channel_bindings (id, agent_id, user_channel_id, binding_name, is_default, priority, routing_rules)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(agent_id, user_channel_id) DO UPDATE SET
                binding_name = excluded.binding_name,
                is_default = excluded.is_default,
                priority = excluded.priority,
                routing_rules = excluded.routing_rules
            "#,
        )
        .bind(&binding.id)
        .bind(&binding.agent_id)
        .bind(&binding.user_channel_id)
        .bind(&binding.binding_name)
        .bind(binding.is_default)
        .bind(binding.priority)
        .bind(rules_json)
        .execute(&self.pool)
        .await
        .map_err(|e| AgentError::Database(format!("Failed to bind agent channel: {}", e)))?;

        Ok(())
    }

    async fn unbind(&self, agent_id: &str, user_channel_id: &str) -> Result<()> {
        sqlx::query(
            r#"
            DELETE FROM agent_channel_bindings WHERE agent_id = ?1 AND user_channel_id = ?2
            "#,
        )
        .bind(agent_id)
        .bind(user_channel_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AgentError::Database(format!("Failed to unbind agent channel: {}", e)))?;

        Ok(())
    }

    async fn list_by_agent(&self, agent_id: &str) -> Result<Vec<AgentChannelBinding>> {
        let rows = sqlx::query_as::<_, AgentChannelBindingRow>(
            r#"
            SELECT id, agent_id, user_channel_id, binding_name, is_default, priority, routing_rules
            FROM agent_channel_bindings
            WHERE agent_id = ?1
            ORDER BY priority DESC
            "#,
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AgentError::Database(format!("Failed to list agent channels: {}", e)))?;

        Ok(rows
            .into_iter()
            .filter_map(|r| r.into_binding().ok())
            .collect())
    }

    async fn list_by_user_channel(
        &self,
        user_channel_id: &str,
    ) -> Result<Vec<AgentChannelBinding>> {
        let rows = sqlx::query_as::<_, AgentChannelBindingRow>(
            r#"
            SELECT id, agent_id, user_channel_id, binding_name, is_default, priority, routing_rules
            FROM agent_channel_bindings
            WHERE user_channel_id = ?1
            ORDER BY priority DESC
            "#,
        )
        .bind(user_channel_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AgentError::Database(format!("Failed to list agent channels: {}", e)))?;

        Ok(rows
            .into_iter()
            .filter_map(|r| r.into_binding().ok())
            .collect())
    }

    async fn set_default(&self, user_channel_id: &str, agent_id: &str) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AgentError::Database(format!("Failed to begin transaction: {}", e)))?;

        sqlx::query(
            r#"
            UPDATE agent_channel_bindings SET is_default = 0 WHERE user_channel_id = ?1
            "#,
        )
        .bind(user_channel_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AgentError::Database(format!("Failed to clear default: {}", e)))?;

        let rows_affected = sqlx::query(
            r#"
            UPDATE agent_channel_bindings SET is_default = 1 WHERE user_channel_id = ?1 AND agent_id = ?2
            "#,
        )
        .bind(user_channel_id)
        .bind(agent_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AgentError::Database(format!("Failed to set default: {}", e)))?
        .rows_affected();

        if rows_affected == 0 {
            tx.rollback()
                .await
                .map_err(|e| AgentError::Database(format!("Rollback failed: {}", e)))?;
            return Err(AgentError::not_found(format!(
                "No binding found for user_channel_id={} agent_id={}",
                user_channel_id, agent_id
            )));
        }

        tx.commit()
            .await
            .map_err(|e| AgentError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    async fn find_default_agent_by_platform_channel(
        &self,
        platform: crate::communication::PlatformType,
        platform_channel_id: &str,
    ) -> Result<Option<String>> {
        let platform_str = platform.to_string();
        let row: Option<(String,)> = sqlx::query_as(
            r#"
            SELECT acb.agent_id
            FROM agent_channel_bindings acb
            JOIN user_channels uc ON uc.id = acb.user_channel_id
            WHERE uc.platform = ?1 AND uc.platform_user_id = ?2 AND acb.is_default = 1
            ORDER BY acb.priority DESC
            LIMIT 1
            "#,
        )
        .bind(platform_str)
        .bind(platform_channel_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            AgentError::Database(format!(
                "Failed to find default agent by platform channel: {}",
                e
            ))
        })?;

        Ok(row.map(|r| r.0))
    }
}

#[derive(sqlx::FromRow)]
struct AgentChannelBindingRow {
    id: String,
    agent_id: String,
    user_channel_id: String,
    binding_name: Option<String>,
    is_default: i32,
    priority: i32,
    routing_rules: String,
}

impl AgentChannelBindingRow {
    fn into_binding(self) -> Result<AgentChannelBinding> {
        let routing_rules: RoutingRules = serde_json::from_str(&self.routing_rules)
            .map_err(|e| AgentError::serialization(format!("Invalid routing_rules: {}", e)))?;

        Ok(AgentChannelBinding {
            id: self.id,
            agent_id: self.agent_id,
            user_channel_id: self.user_channel_id,
            binding_name: self.binding_name,
            is_default: self.is_default != 0,
            priority: self.priority,
            routing_rules,
        })
    }
}
