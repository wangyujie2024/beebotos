//! Core Types
//!
//! Fundamental types for the SDK.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Agent identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(String);

impl AgentId {
    /// Generate new random agent ID
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// From string
    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// As string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for AgentId {
    fn from(s: &str) -> Self {
        Self::from_string(s)
    }
}

impl From<String> for AgentId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Session identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(String);

impl SessionId {
    /// Generate new session ID
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// From string
    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// As string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Task identifier
///
/// 🟠 HIGH FIX: Uses UUID v4 instead of atomic counter
/// to prevent overflow and provide better uniqueness guarantees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskId(pub uuid::Uuid);

impl TaskId {
    /// Generate new task ID using UUID v4
    pub fn new() -> Self {
        // UUID v4 provides 122 bits of randomness - no overflow risk
        Self(uuid::Uuid::new_v4())
    }

    /// From string (parses UUID string)
    pub fn from_string(s: impl AsRef<str>) -> Result<Self, uuid::Error> {
        Ok(Self(uuid::Uuid::parse_str(s.as_ref())?))
    }

    /// As string
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Run identifier (for subagent spawning)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(String);

impl RunId {
    /// Generate new run ID
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// As string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

/// Agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub description: String,
    pub version: String,
    pub capabilities: Vec<String>,
    pub model: ModelConfig,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: "unnamed".to_string(),
            description: String::new(),
            version: "0.1.0".to_string(),
            capabilities: vec![],
            model: ModelConfig::default(),
        }
    }
}

/// Model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: "openai".to_string(),
            model: "gpt-4".to_string(),
            temperature: 0.7,
            max_tokens: 2048,
        }
    }
}

/// Task configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConfig {
    pub task_type: String,
    pub priority: Priority,
    pub timeout_ms: u64,
    pub max_retries: u32,
}

impl Default for TaskConfig {
    fn default() -> Self {
        Self {
            task_type: "generic".to_string(),
            priority: Priority::Normal,
            timeout_ms: 300_000,
            max_retries: 3,
        }
    }
}

/// Priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low = 1,
    #[default]
    Normal = 2,
    High = 3,
    Critical = 4,
}

impl From<i32> for Priority {
    fn from(n: i32) -> Self {
        match n {
            1 => Priority::Low,
            2 => Priority::Normal,
            3 => Priority::High,
            4 => Priority::Critical,
            _ => Priority::Normal,
        }
    }
}

/// Task status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Agent capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    pub can_spawn: bool,
    pub can_pay: bool,
    pub can_access_internet: bool,
    pub can_use_filesystem: bool,
    pub custom: Vec<String>,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            can_spawn: false,
            can_pay: false,
            can_access_internet: false,
            can_use_filesystem: true,
            custom: vec![],
        }
    }
}

/// Token usage
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

impl TokenUsage {
    pub fn new(prompt: u64, completion: u64) -> Self {
        Self {
            prompt_tokens: prompt,
            completion_tokens: completion,
            total_tokens: prompt + completion,
        }
    }

    pub fn add(&mut self, other: &TokenUsage) {
        self.prompt_tokens += other.prompt_tokens;
        self.completion_tokens += other.completion_tokens;
        self.total_tokens += other.total_tokens;
    }
}

/// Cost information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostInfo {
    pub token_cost: f64,
    pub compute_cost: f64,
    pub storage_cost: f64,
    pub total: f64,
    pub currency: String,
}

impl Default for CostInfo {
    fn default() -> Self {
        Self {
            token_cost: 0.0,
            compute_cost: 0.0,
            storage_cost: 0.0,
            total: 0.0,
            currency: "USD".to_string(),
        }
    }
}

/// Pagination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pagination {
    pub page: u32,
    pub per_page: u32,
    pub total: u64,
    pub total_pages: u32,
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: 20,
            total: 0,
            total_pages: 0,
        }
    }
}

/// Paginated response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub pagination: Pagination,
}

impl<T> PaginatedResponse<T> {
    pub fn new(data: Vec<T>, pagination: Pagination) -> Self {
        Self { data, pagination }
    }
}
