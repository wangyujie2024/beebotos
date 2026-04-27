//! Tool: read_file
//!
//! Reads file contents with optional offset/limit pagination.

use crate::llm::types::{FunctionDefinition, Tool};
use crate::llm::ToolHandler;

/// Read file tool
pub struct ReadFileTool;

impl ReadFileTool {
    /// Create new read file tool
    pub fn new() -> Self {
        Self
    }
}

impl Default for ReadFileTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ToolHandler for ReadFileTool {
    fn definition(&self) -> Tool {
        Tool {
            r#type: "function".to_string(),
            function: FunctionDefinition {
                name: "read_file".to_string(),
                description: Some("读取指定路径的文件内容".to_string()),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "文件路径（相对或绝对）"
                        },
                        "offset": {
                            "type": "integer",
                            "description": "起始行号（可选，从0开始）"
                        },
                        "limit": {
                            "type": "integer",
                            "description": "最大读取行数（可选，默认100）"
                        }
                    },
                    "required": ["file_path"]
                }),
            },
        }
    }

    async fn execute(&self, arguments: &str) -> Result<String, String> {
        let args: serde_json::Value = serde_json::from_str(arguments)
            .map_err(|e| format!("Invalid arguments: {}", e))?;

        let file_path = args["file_path"]
            .as_str()
            .ok_or("Missing file_path")?;

        let content = tokio::fs::read_to_string(file_path)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;

        let lines: Vec<&str> = content.lines().collect();
        let offset = args["offset"].as_u64().unwrap_or(0) as usize;
        let limit = args["limit"].as_u64().unwrap_or(100) as usize;

        let end = (offset + limit).min(lines.len());
        let selected = &lines[offset..end];

        let result = selected.join("\n");

        if lines.len() > limit && offset == 0 {
            Ok(format!(
                "{}\n\n[... {} more lines ...]",
                result,
                lines.len() - limit
            ))
        } else {
            Ok(result)
        }
    }
}
