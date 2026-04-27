//! Tool: write_file
//!
//! Creates or overwrites a file with the given content.

use crate::llm::types::{FunctionDefinition, Tool};
use crate::llm::ToolHandler;

/// Write file tool
pub struct WriteFileTool;

impl WriteFileTool {
    /// Create new write file tool
    pub fn new() -> Self {
        Self
    }
}

impl Default for WriteFileTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ToolHandler for WriteFileTool {
    fn definition(&self) -> Tool {
        Tool {
            r#type: "function".to_string(),
            function: FunctionDefinition {
                name: "write_file".to_string(),
                description: Some("创建或覆盖文件".to_string()),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "文件路径"
                        },
                        "content": {
                            "type": "string",
                            "description": "文件内容"
                        }
                    },
                    "required": ["file_path", "content"]
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
        let content = args["content"]
            .as_str()
            .ok_or("Missing content")?;

        // Ensure parent directory exists
        if let Some(parent) = std::path::Path::new(file_path).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create directory: {}", e))?;
        }

        tokio::fs::write(file_path, content)
            .await
            .map_err(|e| format!("Failed to write file: {}", e))?;

        Ok(format!("File written successfully: {}", file_path))
    }
}
