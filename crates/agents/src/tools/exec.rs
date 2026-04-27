//! Tool: exec
//!
//! Executes a shell command and returns stdout/stderr.

use std::time::Duration;

use crate::llm::types::{FunctionDefinition, Tool};
use crate::llm::ToolHandler;

/// Exec tool — execute shell commands
pub struct ExecTool;

impl ExecTool {
    /// Create new exec tool
    pub fn new() -> Self {
        Self
    }
}

impl Default for ExecTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ToolHandler for ExecTool {
    fn definition(&self) -> Tool {
        Tool {
            r#type: "function".to_string(),
            function: FunctionDefinition {
                name: "exec".to_string(),
                description: Some("执行 shell 命令，返回标准输出和标准错误".to_string()),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "要执行的命令"
                        },
                        "timeout": {
                            "type": "integer",
                            "description": "超时秒数（可选，默认30）"
                        },
                        "cwd": {
                            "type": "string",
                            "description": "工作目录（可选）"
                        }
                    },
                    "required": ["command"]
                }),
            },
        }
    }

    async fn execute(&self, arguments: &str) -> Result<String, String> {
        let args: serde_json::Value = serde_json::from_str(arguments)
            .map_err(|e| format!("Invalid arguments: {}", e))?;

        let command = args["command"].as_str().ok_or("Missing command")?;
        let timeout_secs = args["timeout"].as_u64().unwrap_or(30);
        let cwd = args["cwd"].as_str();

        // Use shell to execute the command so quoting and pipes work naturally
        #[cfg(target_os = "windows")]
        let mut cmd = {
            let mut c = tokio::process::Command::new("cmd");
            c.args(&["/C", command]);
            c
        };
        #[cfg(not(target_os = "windows"))]
        let mut cmd = {
            let mut c = tokio::process::Command::new("sh");
            c.args(&["-c", command]);
            c
        };

        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }

        let output = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            cmd.output(),
        )
        .await
        .map_err(|_| format!("Command timed out after {} seconds", timeout_secs))?
        .map_err(|e| format!("Failed to execute command: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let mut result = String::new();

        if !stdout.is_empty() {
            result.push_str("STDOUT:\n");
            result.push_str(&truncate_output(&stdout, 16 * 1024));
        }

        if !stderr.is_empty() {
            if !result.is_empty() {
                result.push_str("\n\n");
            }
            result.push_str("STDERR:\n");
            result.push_str(&truncate_output(&stderr, 8 * 1024));
        }

        if result.is_empty() {
            result = format!(
                "Command completed with exit code: {}",
                output.status.code().unwrap_or(-1)
            );
        } else {
            result.push_str(&format!(
                "\n\nExit code: {}",
                output.status.code().unwrap_or(-1)
            ));
        }

        Ok(result)
    }
}

/// Truncate output if it exceeds max bytes (safe UTF-8)
fn truncate_output(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        s.to_string()
    } else {
        let end = s[..max_bytes]
            .char_indices()
            .last()
            .map(|(i, _)| i)
            .unwrap_or(max_bytes);
        format!(
            "{}\n\n[... {} more bytes truncated ...]",
            &s[..end],
            s.len() - end
        )
    }
}
