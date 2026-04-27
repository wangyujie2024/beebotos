//! Database Models
//!
//! SQLx-compatible models for database operations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Agent database model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRecord {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub capabilities: Vec<String>,
    pub model_provider: Option<String>,
    pub model_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub owner_id: Option<String>,
    pub metadata: serde_json::Value,
}

/// SQLite-compatible row for AgentRecord
#[derive(sqlx::FromRow)]
pub struct AgentRecordRow {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub capabilities: String,
    pub model_provider: Option<String>,
    pub model_name: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_heartbeat: Option<String>,
    pub owner_id: Option<String>,
    pub metadata: String,
}

impl TryFrom<AgentRecordRow> for AgentRecord {
    type Error = String;

    fn try_from(row: AgentRecordRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id.parse().map_err(|e| format!("Invalid UUID: {}", e))?,
            name: row.name,
            description: row.description,
            status: row.status,
            capabilities: serde_json::from_str(&row.capabilities)
                .map_err(|e| format!("Invalid capabilities JSON: {}", e))?,
            model_provider: row.model_provider,
            model_name: row.model_name,
            created_at: row
                .created_at
                .parse()
                .map_err(|e| format!("Invalid datetime: {}", e))?,
            updated_at: row
                .updated_at
                .parse()
                .map_err(|e| format!("Invalid datetime: {}", e))?,
            last_heartbeat: row
                .last_heartbeat
                .map(|s| s.parse())
                .transpose()
                .map_err(|e| format!("Invalid datetime: {}", e))?,
            owner_id: row.owner_id,
            metadata: serde_json::from_str(&row.metadata)
                .map_err(|e| format!("Invalid metadata JSON: {}", e))?,
        })
    }
}

/// Agent creation request
#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub description: Option<String>,
    /// Agent capabilities as structured objects or legacy strings
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_capabilities")]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub model_provider: Option<String>,
    #[serde(default)]
    pub model_name: Option<String>,
}

/// Custom deserializer for capabilities supporting both new structured format
/// and legacy strings
fn deserialize_capabilities<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use crate::capability::AgentCapability;

    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    enum CapabilityInput {
        Structured(AgentCapability),
        Legacy(String),
    }

    let inputs: Vec<CapabilityInput> = Vec::deserialize(deserializer)?;
    let capabilities: Vec<String> = inputs
        .into_iter()
        .map(|input| match input {
            CapabilityInput::Structured(cap) => cap.to_compact_string(),
            CapabilityInput::Legacy(s) => s,
        })
        .collect();

    Ok(capabilities)
}

/// Agent update request
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    #[serde(default)]
    pub capabilities: Option<Vec<String>>,
    #[serde(default)]
    pub model_provider: Option<String>,
    #[serde(default)]
    pub model_name: Option<String>,
}

/// Agent response
#[derive(Debug, Serialize)]
pub struct AgentResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub status: String,
    pub capabilities: Vec<String>,
    pub model: ModelInfo,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_heartbeat: Option<DateTime<Utc>>,
}

/// Model information
#[derive(Debug, Serialize)]
pub struct ModelInfo {
    pub provider: String,
    pub name: String,
}

impl From<AgentRecord> for AgentResponse {
    fn from(agent: AgentRecord) -> Self {
        Self {
            id: agent.id.to_string(),
            name: agent.name,
            description: agent.description,
            status: agent.status,
            capabilities: agent.capabilities,
            model: ModelInfo {
                provider: agent.model_provider.unwrap_or_else(|| "openai".to_string()),
                name: agent.model_name.unwrap_or_else(|| "gpt-4".to_string()),
            },
            created_at: agent.created_at,
            updated_at: agent.updated_at,
            last_heartbeat: agent.last_heartbeat,
        }
    }
}

/// API Key database model
#[derive(Debug, Clone, FromRow)]
#[allow(dead_code)]
pub struct ApiKey {
    pub id: Uuid,
    pub key_hash: String,
    pub name: String,
    pub owner_id: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub is_revoked: bool,
}

/// API Key creation request
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct CreateApiKeyRequest {
    pub name: String,
    #[serde(default)]
    pub scopes: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// API Key response (includes the plain key, only shown once)
#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct ApiKeyResponse {
    pub id: String,
    pub key: String, // Plain key, only shown on creation
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// API Key info (without the actual key)
#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct ApiKeyInfo {
    pub id: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub is_revoked: bool,
}

impl From<ApiKey> for ApiKeyInfo {
    fn from(key: ApiKey) -> Self {
        Self {
            id: key.id.to_string(),
            name: key.name,
            scopes: key.scopes,
            expires_at: key.expires_at,
            last_used_at: key.last_used_at,
            created_at: key.created_at,
            is_revoked: key.is_revoked,
        }
    }
}

/// Session database model
#[derive(Debug, Clone, FromRow)]
#[allow(dead_code)]
pub struct Session {
    pub id: Uuid,
    pub user_id: String,
    pub token_jti: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub is_revoked: bool,
    pub ip_address: Option<std::net::IpAddr>,
    pub user_agent: Option<String>,
}

/// Audit log entry
#[derive(Debug, Clone, FromRow, Serialize)]
#[allow(dead_code)]
pub struct AuditLog {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub user_id: Option<String>,
    pub ip_address: Option<std::net::IpAddr>,
    pub user_agent: Option<String>,
    pub request_id: Option<String>,
    pub details: serde_json::Value,
    pub success: bool,
    pub error_message: Option<String>,
}

/// Agent status history entry
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct AgentStatusHistory {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub status: String,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Pagination parameters
#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_per_page")]
    pub per_page: i64,
}

fn default_page() -> i64 {
    1
}
fn default_per_page() -> i64 {
    20
}

impl PaginationParams {
    pub fn offset(&self) -> i64 {
        (self.page - 1) * self.per_page
    }
}

/// Paginated response
#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub total_pages: i64,
}

impl<T> PaginatedResponse<T> {
    pub fn new(data: Vec<T>, total: i64, page: i64, per_page: i64) -> Self {
        let total_pages = (total as f64 / per_page as f64).ceil() as i64;
        Self {
            data,
            total,
            page,
            per_page,
            total_pages,
        }
    }
}
