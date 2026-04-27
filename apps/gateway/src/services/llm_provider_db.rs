//! LLM Provider Database Access Layer

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

/// Database model for LLM provider
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct LlmProviderDb {
    pub id: i64,
    pub provider_id: String,
    pub name: String,
    pub protocol: String,
    pub base_url: Option<String>,
    pub api_key_encrypted: Option<String>,
    pub enabled: bool,
    pub is_default_provider: bool,
    pub icon: Option<String>,
    pub icon_color: Option<String>,
    pub type_label: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Database model for LLM model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct LlmModelDb {
    pub id: i64,
    pub provider_id: i64,
    pub name: String,
    pub display_name: Option<String>,
    pub is_default_model: bool,
    pub created_at: String,
}

/// Preset default models for each provider: (provider_id, model_name)
const PRESET_MODELS: &[(&str, &str)] = &[
    ("kimi", "kimi-k2.5"),
    ("kimi-china", "kimi-k2.5"),
    ("openai", "gpt-4o"),
    ("zhipu", "glm-4"),
    ("deepseek", "deepseek-chat"),
    ("anthropic", "claude-3-5-sonnet-20241022"),
    ("ollama", "llama3.2"),
];

/// Preset provider data: (provider_id, name, protocol, base_url, icon, icon_color, type_label)
const PRESET_PROVIDERS: &[(&str, &str, &str, &str, &str, &str, &str)] = &[
    (
        "kimi",
        "Moonshot AI",
        "openai-compatible",
        "https://api.moonshot.cn/v1",
        "🌙",
        "#4f6ef7",
        "内置",
    ),
    (
        "kimi-china",
        "Kimi (China)",
        "openai-compatible",
        "https://api.moonshot.cn/v1",
        "🌙",
        "#4f6ef7",
        "代理",
    ),
    (
        "openai",
        "OpenAI",
        "openai-compatible",
        "https://api.openai.com/v1",
        "🤖",
        "#10a37f",
        "内置",
    ),
    (
        "zhipu",
        "智谱 AI",
        "openai-compatible",
        "https://open.bigmodel.cn/api/paas/v4",
        "🧠",
        "#3b82f6",
        "内置",
    ),
    (
        "deepseek",
        "DeepSeek",
        "openai-compatible",
        "https://api.deepseek.com/v1",
        "🔍",
        "#4d6bfa",
        "内置",
    ),
    (
        "anthropic",
        "Anthropic",
        "anthropic",
        "https://api.anthropic.com/v1",
        "🅰️",
        "#d4a574",
        "内置",
    ),
    (
        "ollama",
        "Ollama (本地)",
        "openai-compatible",
        "http://localhost:11434",
        "🦙",
        "#ff6b6b",
        "本地",
    ),
];

/// Seed preset providers and their default models into database if they don't exist
pub async fn seed_providers(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    for (provider_id, name, protocol, base_url, icon, icon_color, type_label) in PRESET_PROVIDERS {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM llm_providers WHERE provider_id = ?)")
                .bind(provider_id)
                .fetch_one(pool)
                .await?;

        if !exists {
            sqlx::query(
                "INSERT INTO llm_providers (provider_id, name, protocol, base_url, enabled, \
                 is_default_provider, icon, icon_color, type_label)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(provider_id)
            .bind(name)
            .bind(protocol)
            .bind(base_url)
            .bind(true)
            .bind(false)
            .bind(icon)
            .bind(icon_color)
            .bind(type_label)
            .execute(pool)
            .await?;
        }
    }

    // Seed default models for preset providers
    seed_default_models(pool).await?;

    Ok(())
}

/// Seed default models for preset providers
async fn seed_default_models(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    for (provider_id, model_name) in PRESET_MODELS {
        let provider_row: Option<(i64,)> =
            sqlx::query_as("SELECT id FROM llm_providers WHERE provider_id = ?")
                .bind(provider_id)
                .fetch_optional(pool)
                .await?;

        let Some((provider_db_id,)) = provider_row else {
            continue;
        };

        let model_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM llm_models WHERE provider_id = ? AND name = ?)",
        )
        .bind(provider_db_id)
        .bind(model_name)
        .fetch_one(pool)
        .await?;

        if !model_exists {
            sqlx::query(
                "INSERT INTO llm_models (provider_id, name, is_default_model) VALUES (?, ?, ?)",
            )
            .bind(provider_db_id)
            .bind(model_name)
            .bind(true)
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

/// List all providers with their models
pub async fn list_providers_with_models(
    pool: &SqlitePool,
) -> Result<Vec<(LlmProviderDb, Vec<LlmModelDb>)>, sqlx::Error> {
    let providers: Vec<LlmProviderDb> =
        sqlx::query_as("SELECT * FROM llm_providers ORDER BY created_at")
            .fetch_all(pool)
            .await?;

    let mut result = Vec::new();
    for provider in providers {
        let models: Vec<LlmModelDb> =
            sqlx::query_as("SELECT * FROM llm_models WHERE provider_id = ? ORDER BY created_at")
                .bind(provider.id)
                .fetch_all(pool)
                .await?;
        result.push((provider, models));
    }
    Ok(result)
}

/// Get provider by ID
pub async fn get_provider_by_id(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<LlmProviderDb>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM llm_providers WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// Create a custom provider
pub async fn create_provider(
    pool: &SqlitePool,
    provider_id: &str,
    name: &str,
    protocol: &str,
    base_url: Option<&str>,
    api_key_encrypted: Option<&str>,
) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO llm_providers (provider_id, name, protocol, base_url, api_key_encrypted, \
         enabled)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(provider_id)
    .bind(name)
    .bind(protocol)
    .bind(base_url)
    .bind(api_key_encrypted)
    .bind(true)
    .execute(pool)
    .await?;

    Ok(result.last_insert_rowid())
}

/// Update provider
pub async fn update_provider(
    pool: &SqlitePool,
    id: i64,
    name: Option<&str>,
    base_url: Option<&str>,
    api_key_encrypted: Option<&str>,
    enabled: Option<bool>,
) -> Result<(), sqlx::Error> {
    let mut updates = Vec::new();
    let mut query = String::from("UPDATE llm_providers SET updated_at = CURRENT_TIMESTAMP");

    if let Some(name) = name {
        query.push_str(", name = ?");
        updates.push(name.to_string());
    }
    if let Some(base_url) = base_url {
        query.push_str(", base_url = ?");
        updates.push(base_url.to_string());
    }
    if let Some(api_key) = api_key_encrypted {
        query.push_str(", api_key_encrypted = ?");
        updates.push(api_key.to_string());
    }
    if let Some(enabled) = enabled {
        query.push_str(", enabled = ?");
        updates.push(if enabled { "1" } else { "0" }.to_string());
    }

    query.push_str(" WHERE id = ?");

    let mut q = sqlx::query(&query);
    for val in &updates {
        q = q.bind(val);
    }
    q.bind(id).execute(pool).await?;
    Ok(())
}

/// Delete provider (cascades to models)
pub async fn delete_provider(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM llm_providers WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Set default provider (clears previous default)
pub async fn set_default_provider(pool: &SqlitePool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE llm_providers SET is_default_provider = false")
        .execute(pool)
        .await?;
    sqlx::query("UPDATE llm_providers SET is_default_provider = true WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Add model to provider
pub async fn add_model(
    pool: &SqlitePool,
    provider_id: i64,
    name: &str,
    display_name: Option<&str>,
) -> Result<i64, sqlx::Error> {
    let result =
        sqlx::query("INSERT INTO llm_models (provider_id, name, display_name) VALUES (?, ?, ?)")
            .bind(provider_id)
            .bind(name)
            .bind(display_name)
            .execute(pool)
            .await?;

    Ok(result.last_insert_rowid())
}

/// Delete model
pub async fn delete_model(pool: &SqlitePool, model_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM llm_models WHERE id = ?")
        .bind(model_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Set default model for provider
pub async fn set_default_model(
    pool: &SqlitePool,
    provider_id: i64,
    model_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE llm_models SET is_default_model = false WHERE provider_id = ?")
        .bind(provider_id)
        .execute(pool)
        .await?;
    sqlx::query("UPDATE llm_models SET is_default_model = true WHERE id = ? AND provider_id = ?")
        .bind(model_id)
        .bind(provider_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Get default provider
pub async fn get_default_provider(pool: &SqlitePool) -> Result<Option<LlmProviderDb>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM llm_providers WHERE is_default_provider = true LIMIT 1")
        .fetch_optional(pool)
        .await
}

/// Get default model for provider
pub async fn get_default_model(
    pool: &SqlitePool,
    provider_id: i64,
) -> Result<Option<LlmModelDb>, sqlx::Error> {
    sqlx::query_as(
        "SELECT * FROM llm_models WHERE provider_id = ? AND is_default_model = true LIMIT 1",
    )
    .bind(provider_id)
    .fetch_optional(pool)
    .await
}

/// Get all models for a provider
pub async fn get_models_for_provider(
    pool: &SqlitePool,
    provider_id: i64,
) -> Result<Vec<LlmModelDb>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM llm_models WHERE provider_id = ? ORDER BY created_at")
        .bind(provider_id)
        .fetch_all(pool)
        .await
}
