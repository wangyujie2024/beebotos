//! Task types for Agent execution
//!
//! 🔒 P0 FIX: Type-safe task processing with TaskType enum

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// 🔒 P0 FIX: TaskType enum replacing magic strings
///
/// Type-safe task types prevent runtime errors from typos
/// and provide clear documentation of supported operations.
/// 
/// 🆕 PLANNING FIX: Added planning-related task types for Agent planning integration
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskType {
    /// LLM chat completion
    #[serde(rename = "llm_chat")]
    LlmChat,
    /// Skill/WASM execution
    #[serde(rename = "skill_execution")]
    SkillExecution,
    /// MCP tool call
    #[serde(rename = "mcp_tool")]
    McpTool,
    /// File processing
    #[serde(rename = "file_processing")]
    FileProcessing,
    /// A2A message send
    #[serde(rename = "a2a_send")]
    A2aSend,
    /// Blockchain transaction
    #[serde(rename = "chain_transaction")]
    ChainTransaction,
    /// 🆕 PLANNING FIX: Plan creation task
    #[serde(rename = "plan_creation")]
    PlanCreation,
    /// 🆕 PLANNING FIX: Plan execution task
    #[serde(rename = "plan_execution")]
    PlanExecution,
    /// 🆕 PLANNING FIX: Plan adaptation/replanning task
    #[serde(rename = "plan_adaptation")]
    PlanAdaptation,
    /// 🆕 DEVICE FIX: Device automation task
    #[serde(rename = "device_automation")]
    DeviceAutomation,
    /// 🆕 DEVICE FIX: App lifecycle management task
    #[serde(rename = "app_lifecycle")]
    AppLifecycle,
    /// 🟢 P1 FIX: Workflow execution task
    #[serde(rename = "workflow_execution")]
    WorkflowExecution,
    /// Custom task type (fallback for extensibility)
    #[serde(rename = "custom")]
    Custom(String),
}

impl TaskType {
    /// Convert to string representation
    pub fn as_str(&self) -> &str {
        match self {
            TaskType::LlmChat => "llm_chat",
            TaskType::SkillExecution => "skill_execution",
            TaskType::McpTool => "mcp_tool",
            TaskType::FileProcessing => "file_processing",
            TaskType::A2aSend => "a2a_send",
            TaskType::ChainTransaction => "chain_transaction",
            TaskType::PlanCreation => "plan_creation",
            TaskType::PlanExecution => "plan_execution",
            TaskType::PlanAdaptation => "plan_adaptation",
            TaskType::DeviceAutomation => "device_automation",
            TaskType::AppLifecycle => "app_lifecycle",
            TaskType::WorkflowExecution => "workflow_execution",
            TaskType::Custom(s) => s.as_str(),
        }
    }

    /// Parse from string (convenience method that doesn't require Result)
    pub fn parse(s: &str) -> Self {
        match s {
            "llm_chat" => TaskType::LlmChat,
            "skill_execution" => TaskType::SkillExecution,
            "mcp_tool" => TaskType::McpTool,
            "file_processing" => TaskType::FileProcessing,
            "a2a_send" => TaskType::A2aSend,
            "chain_transaction" => TaskType::ChainTransaction,
            "plan_creation" => TaskType::PlanCreation,
            "plan_execution" => TaskType::PlanExecution,
            "plan_adaptation" => TaskType::PlanAdaptation,
            "device_automation" => TaskType::DeviceAutomation,
            "app_lifecycle" => TaskType::AppLifecycle,
            "workflow_execution" => TaskType::WorkflowExecution,
            other => TaskType::Custom(other.to_string()),
        }
    }
}

impl std::fmt::Display for TaskType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for TaskType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "llm_chat" => TaskType::LlmChat,
            "skill_execution" => TaskType::SkillExecution,
            "mcp_tool" => TaskType::McpTool,
            "file_processing" => TaskType::FileProcessing,
            "a2a_send" => TaskType::A2aSend,
            "chain_transaction" => TaskType::ChainTransaction,
            "plan_creation" => TaskType::PlanCreation,
            "plan_execution" => TaskType::PlanExecution,
            "plan_adaptation" => TaskType::PlanAdaptation,
            "device_automation" => TaskType::DeviceAutomation,
            "app_lifecycle" => TaskType::AppLifecycle,
            "workflow_execution" => TaskType::WorkflowExecution,
            other => TaskType::Custom(other.to_string()),
        })
    }
}

/// Agent execution task
#[derive(Debug, Clone)]
pub struct ExecutionTask {
    pub id: String,
    pub task_type: TaskType,
    pub input: String,
    pub parameters: HashMap<String, String>,
}

/// Type alias for backward compatibility
pub type Task = ExecutionTask;

#[derive(Debug, Clone)]
pub struct TaskResult {
    pub task_id: String,
    pub success: bool,
    pub output: String,
    pub artifacts: Vec<Artifact>,
    pub execution_time_ms: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Artifact {
    pub id: String,
    pub artifact_type: String,
    pub content: Vec<u8>,
    pub mime_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_task_creation() {
        let task = Task {
            id: "test-task-1".to_string(),
            task_type: TaskType::LlmChat,
            input: "Hello, world!".to_string(),
            parameters: HashMap::new(),
        };

        assert_eq!(task.id, "test-task-1");
        assert_eq!(task.task_type, TaskType::LlmChat);
        assert_eq!(task.task_type.as_str(), "llm_chat");
        assert_eq!(task.input, "Hello, world!");
    }

    #[test]
    fn test_task_type_enum() {
        assert_eq!(TaskType::LlmChat.as_str(), "llm_chat");
        assert_eq!(TaskType::SkillExecution.as_str(), "skill_execution");
        assert_eq!(TaskType::McpTool.as_str(), "mcp_tool");
        assert_eq!(TaskType::FileProcessing.as_str(), "file_processing");
        assert_eq!(TaskType::A2aSend.as_str(), "a2a_send");
        assert_eq!(TaskType::ChainTransaction.as_str(), "chain_transaction");

        let custom = TaskType::Custom("custom_task".to_string());
        assert_eq!(custom.as_str(), "custom_task");

        assert_eq!(TaskType::from_str("llm_chat").unwrap(), TaskType::LlmChat);
        assert_eq!(
            TaskType::from_str("unknown").unwrap(),
            TaskType::Custom("unknown".to_string())
        );

        assert_eq!(format!("{}", TaskType::LlmChat), "llm_chat");
    }

    #[test]
    fn test_task_result_creation() {
        let result = TaskResult {
            task_id: "test-task-1".to_string(),
            success: true,
            output: "Test output".to_string(),
            artifacts: vec![],
            execution_time_ms: 100,
        };

        assert_eq!(result.task_id, "test-task-1");
        assert!(result.success);
        assert_eq!(result.execution_time_ms, 100);
    }

    #[test]
    fn test_artifact_creation() {
        let artifact = Artifact {
            id: "artifact-1".to_string(),
            artifact_type: "text".to_string(),
            content: b"Test content".to_vec(),
            mime_type: "text/plain".to_string(),
        };

        assert_eq!(artifact.id, "artifact-1");
        assert_eq!(artifact.mime_type, "text/plain");
    }
}
