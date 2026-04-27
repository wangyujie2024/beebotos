//! Tool: process
//!
//! Process management: list or kill processes.

use crate::llm::types::{FunctionDefinition, Tool};
use crate::llm::ToolHandler;

/// Process management tool
pub struct ProcessTool;

impl ProcessTool {
    /// Create new process tool
    pub fn new() -> Self {
        Self
    }
}

impl Default for ProcessTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ToolHandler for ProcessTool {
    fn definition(&self) -> Tool {
        Tool {
            r#type: "function".to_string(),
            function: FunctionDefinition {
                name: "process".to_string(),
                description: Some("进程管理：列出或终止进程".to_string()),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["list", "kill"],
                            "description": "操作类型"
                        },
                        "pattern": {
                            "type": "string",
                            "description": "list 时用于过滤的进程名模式（可选）"
                        },
                        "pid": {
                            "type": "integer",
                            "description": "kill 时指定进程 ID"
                        }
                    },
                    "required": ["action"]
                }),
            },
        }
    }

    async fn execute(&self, arguments: &str) -> Result<String, String> {
        let args: serde_json::Value = serde_json::from_str(arguments)
            .map_err(|e| format!("Invalid arguments: {}", e))?;

        let action = args["action"].as_str().ok_or("Missing action")?;

        match action {
            "list" => {
                let pattern = args["pattern"].as_str();
                list_processes(pattern).await
            }
            "kill" => {
                let pid = args["pid"].as_u64().ok_or("Missing pid for kill action")?;
                kill_process(pid).await
            }
            _ => Err(format!("Unknown action: {}", action)),
        }
    }
}

/// List processes using shell commands
async fn list_processes(pattern: Option<&str>) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        let output = tokio::process::Command::new("tasklist")
            .args(&["/FO", "CSV", "/NH"])
            .output()
            .await
            .map_err(|e| format!("Failed to run tasklist: {}", e))?;

        let text = String::from_utf8_lossy(&output.stdout);
        let mut lines: Vec<String> = Vec::new();
        lines.push("PID,Process Name,Memory".to_string());

        for line in text.lines() {
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() >= 2 {
                let name = parts[0].trim_matches('"');
                let pid = parts[1].trim_matches('"');
                if let Some(p) = pattern {
                    if name.to_lowercase().contains(&p.to_lowercase()) {
                        lines.push(format!("{},{}" , pid, name));
                    }
                } else {
                    lines.push(format!("{},{}" , pid, name));
                }
            }
        }

        if lines.len() <= 1 {
            return Ok("No processes found.".to_string());
        }

        Ok(lines.join("\n"))
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut cmd = tokio::process::Command::new("ps");
        cmd.args(&["-eo", "pid,comm,pcpu,pmem"]);

        let output = cmd
            .output()
            .await
            .map_err(|e| format!("Failed to run ps: {}", e))?;

        let text = String::from_utf8_lossy(&output.stdout);
        let mut lines: Vec<String> = Vec::new();
        lines.push("PID,Command,CPU%,MEM%".to_string());

        for (i, line) in text.lines().enumerate() {
            if i == 0 {
                continue; // skip header from ps
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(p) = pattern {
                if trimmed.to_lowercase().contains(&p.to_lowercase()) {
                    lines.push(trimmed.to_string());
                }
            } else {
                lines.push(trimmed.to_string());
            }
        }

        if lines.len() <= 1 {
            return Ok("No processes found.".to_string());
        }

        Ok(lines.join("\n"))
    }
}

/// Kill a process by PID
async fn kill_process(pid: u64) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        let output = tokio::process::Command::new("taskkill")
            .args(&["/PID", &pid.to_string(), "/F"])
            .output()
            .await
            .map_err(|e| format!("Failed to run taskkill: {}", e))?;

        if output.status.success() {
            Ok(format!("Process {} terminated successfully.", pid))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("Failed to kill process {}: {}", pid, stderr))
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Try SIGTERM first, then SIGKILL
        let output = tokio::process::Command::new("kill")
            .arg(pid.to_string())
            .output()
            .await
            .map_err(|e| format!("Failed to run kill: {}", e))?;

        if output.status.success() {
            Ok(format!(
                "Process {} sent SIGTERM (graceful shutdown).",
                pid
            ))
        } else {
            // Try SIGKILL
            let output = tokio::process::Command::new("kill")
                .args(&["-9", &pid.to_string()])
                .output()
                .await
                .map_err(|e| format!("Failed to run kill -9: {}", e))?;

            if output.status.success() {
                Ok(format!("Process {} force-killed with SIGKILL.", pid))
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(format!("Failed to kill process {}: {}", pid, stderr))
            }
        }
    }
}
