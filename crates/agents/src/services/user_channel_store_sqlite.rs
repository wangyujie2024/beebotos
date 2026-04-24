//! SQLite implementation of `UserChannelStore`.

use async_trait::async_trait;
use sqlx::SqlitePool;

use super::user_channel_store::UserChannelStore;
use crate::communication::user_channel::{ChannelBindingStatus, UserChannelBinding};
use crate::communication::PlatformType;
use crate::error::{AgentError, Result};

pub struct SqliteUserChannelStore {
    pool: SqlitePool,
}

impl SqliteUserChannelStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn status_to_string(status: ChannelBindingStatus) -> String {
        match status {
            ChannelBindingStatus::Active => "active".to_string(),
            ChannelBindingStatus::Paused => "paused".to_string(),
            ChannelBindingStatus::Error => "error".to_string(),
        }
    }

    fn string_to_status(s: &str) -> ChannelBindingStatus {
        match s {
            "paused" => ChannelBindingStatus::Paused,
            "error" => ChannelBindingStatus::Error,
            _ => ChannelBindingStatus::Active,
        }
    }
}

#[async_trait]
impl UserChannelStore for SqliteUserChannelStore {
    async fn create(&self, binding: &UserChannelBinding, config_encrypted: &str) -> Result<()> {
        // 🟢 P1 FIX: Auto-create placeholder user record to satisfy FOREIGN KEY
        // constraint. External platform users (WeChat, Telegram, etc.) won't
        // exist in the users table, so we upsert a minimal placeholder before
        // inserting the user_channel binding.
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO users (id, username, email, password_hash)
            VALUES (?1, ?2, ?3, 'no_password')
            "#,
        )
        .bind(&binding.user_id)
        .bind(&binding.user_id)
        .bind(format!("{}@placeholder.local", binding.user_id))
        .execute(&self.pool)
        .await
        .map_err(|e| AgentError::Database(format!("Failed to upsert placeholder user: {}", e)))?;

        let status = Self::status_to_string(binding.status);
        sqlx::query(
            r#"
            INSERT INTO user_channels (id, user_id, platform, instance_name, platform_user_id, config_encrypted, status, webhook_path)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
        )
        .bind(&binding.id)
        .bind(&binding.user_id)
        .bind(format!("{}", binding.platform))
        .bind(&binding.instance_name)
        .bind(&binding.platform_user_id)
        .bind(config_encrypted)
        .bind(status)
        .bind(&binding.webhook_path)
        .execute(&self.pool)
        .await
        .map_err(|e| AgentError::Database(format!("Failed to create user channel: {}", e)))?;

        Ok(())
    }

    async fn get(&self, id: &str) -> Result<Option<UserChannelBinding>> {
        let row = sqlx::query_as::<_, UserChannelRow>(
            r#"
            SELECT id, user_id, platform, instance_name, platform_user_id, status, webhook_path
            FROM user_channels
            WHERE id = ?1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AgentError::Database(format!("Failed to get user channel: {}", e)))?;

        Ok(row.map(|r| r.into_binding()))
    }

    async fn find_by_platform_user(
        &self,
        platform: PlatformType,
        platform_user_id: &str,
    ) -> Result<Option<UserChannelBinding>> {
        let row = sqlx::query_as::<_, UserChannelRow>(
            r#"
            SELECT id, user_id, platform, instance_name, platform_user_id, status, webhook_path
            FROM user_channels
            WHERE platform = ?1 AND platform_user_id = ?2
            "#,
        )
        .bind(format!("{}", platform))
        .bind(platform_user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AgentError::Database(format!("Failed to find user channel: {}", e)))?;

        Ok(row.map(|r| r.into_binding()))
    }

    async fn list_by_user(&self, user_id: &str) -> Result<Vec<UserChannelBinding>> {
        let rows = sqlx::query_as::<_, UserChannelRow>(
            r#"
            SELECT id, user_id, platform, instance_name, platform_user_id, status, webhook_path
            FROM user_channels
            WHERE user_id = ?1
            ORDER BY created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AgentError::Database(format!("Failed to list user channels: {}", e)))?;

        Ok(rows.into_iter().map(|r| r.into_binding()).collect())
    }

    async fn update_status(&self, id: &str, status: ChannelBindingStatus) -> Result<()> {
        let status_str = Self::status_to_string(status);
        sqlx::query(
            r#"
            UPDATE user_channels SET status = ?1 WHERE id = ?2
            "#,
        )
        .bind(status_str)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            AgentError::Database(format!("Failed to update user channel status: {}", e))
        })?;

        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query(
            r#"
            DELETE FROM user_channels WHERE id = ?1
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| AgentError::Database(format!("Failed to delete user channel: {}", e)))?;

        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct UserChannelRow {
    id: String,
    user_id: String,
    platform: String,
    instance_name: String,
    platform_user_id: Option<String>,
    status: String,
    webhook_path: Option<String>,
}

impl UserChannelRow {
    fn into_binding(self) -> UserChannelBinding {
        UserChannelBinding {
            id: self.id,
            user_id: self.user_id,
            platform: parse_platform(&self.platform),
            instance_name: self.instance_name,
            platform_user_id: self.platform_user_id,
            status: SqliteUserChannelStore::string_to_status(&self.status),
            webhook_path: self.webhook_path,
        }
    }
}

fn parse_platform(s: &str) -> PlatformType {
    match s.to_lowercase().as_str() {
        "slack" => PlatformType::Slack,
        "telegram" => PlatformType::Telegram,
        "discord" => PlatformType::Discord,
        "whatsapp" => PlatformType::WhatsApp,
        "signal" => PlatformType::Signal,
        "imessage" => PlatformType::IMessage,
        "wechat" => PlatformType::WeChat,
        "teams" => PlatformType::Teams,
        "twitter" => PlatformType::Twitter,
        "lark" => PlatformType::Lark,
        "dingtalk" => PlatformType::DingTalk,
        "matrix" => PlatformType::Matrix,
        "googlechat" => PlatformType::GoogleChat,
        "line" => PlatformType::Line,
        "qq" => PlatformType::QQ,
        "irc" => PlatformType::IRC,
        "webchat" => PlatformType::WebChat,
        _ => PlatformType::Custom,
    }
}
