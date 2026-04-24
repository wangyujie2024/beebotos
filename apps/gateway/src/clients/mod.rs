//! HTTP Clients for external services
//!
//! Provides clients for ClawHub, BeeHub, and other external services.

pub mod beehub;
pub mod clawhub;

pub use beehub::BeeHubClient;
pub use clawhub::ClawHubClient;

/// Hub type for skill discovery and download
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HubType {
    /// ClawHub - primary skill marketplace
    ClawHub,
    /// BeeHub - internal skill registry
    BeeHub,
}

impl Default for HubType {
    fn default() -> Self {
        HubType::ClawHub
    }
}

impl std::fmt::Display for HubType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HubType::ClawHub => write!(f, "clawhub"),
            HubType::BeeHub => write!(f, "beehub"),
        }
    }
}

impl std::str::FromStr for HubType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "clawhub" | "claw" => Ok(HubType::ClawHub),
            "beehub" | "bee" => Ok(HubType::BeeHub),
            _ => Err(format!("Unknown hub type: {}", s)),
        }
    }
}

/// Skill metadata from hub
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkillMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub license: String,
    pub repository: Option<String>,
    pub hash: String,
    pub downloads: u64,
    pub rating: f32,
    pub capabilities: Vec<String>,
    pub tags: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Hub client errors
#[derive(Debug, thiserror::Error)]
pub enum HubError {
    #[error("Skill not found: {0}")]
    NotFound(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    #[error("Download failed: {0}")]
    DownloadFailed(String),
    #[error("Authentication failed: {0}")]
    AuthFailed(String),
}
