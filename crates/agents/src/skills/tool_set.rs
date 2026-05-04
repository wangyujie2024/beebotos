//! Skill Tool Set
//!
//! General-purpose tools that skills can use via the ReAct executor:
//! file_read, file_write, file_list, process_exec, bash_shell.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use regex::Regex;
use serde_json::Value;
use tracing::{info, warn};
use crate::skills::process_sandbox::apply_sandbox;
use crate::Agent;

/// Trait for tools usable inside the skill ReAct loop
#[async_trait::async_trait]
pub trait SkillTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> Value;
    async fn execute(&self, params: &Value) -> Result<String, String>;
}

/// Read a file from the filesystem
pub struct FileReadTool;

#[async_trait::async_trait]
impl SkillTool for FileReadTool {
    fn name(&self) -> &str {
        "file_read"
    }

    fn description(&self) -> &str {
        "Read the contents of a file. Parameters: path (string)"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute or relative file path" }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, params: &Value) -> Result<String, String> {
        let path = params["path"].as_str().ok_or("Missing 'path' parameter")?;
        let path = PathBuf::from(path);
        tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| format!("Failed to read file '{}': {}", path.display(), e))
    }
}

/// Write text to a file
pub struct FileWriteTool;

#[async_trait::async_trait]
impl SkillTool for FileWriteTool {
    fn name(&self) -> &str {
        "file_write"
    }

    fn description(&self) -> &str {
        "Write text content to a file. Creates the file if it does not exist. Parameters: path (string), content (string)"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute or relative file path" },
                "content": { "type": "string", "description": "Text content to write" }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, params: &Value) -> Result<String, String> {
        let path = params["path"].as_str().ok_or("Missing 'path' parameter")?;
        let content = params["content"].as_str().ok_or("Missing 'content' parameter")?;
        let path = PathBuf::from(path);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create parent directories: {}", e))?;
        }
        tokio::fs::write(&path, content)
            .await
            .map_err(|e| format!("Failed to write file '{}': {}", path.display(), e))?;
        Ok(format!("File '{}' written successfully.", path.display()))
    }
}

/// List files in a directory
pub struct FileListTool;

#[async_trait::async_trait]
impl SkillTool for FileListTool {
    fn name(&self) -> &str {
        "file_list"
    }

    fn description(&self) -> &str {
        "List files and directories at a given path. Parameters: path (string)"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Directory path to list" }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, params: &Value) -> Result<String, String> {
        let path = params["path"].as_str().ok_or("Missing 'path' parameter")?;
        let mut entries = tokio::fs::read_dir(path)
            .await
            .map_err(|e| format!("Failed to read directory '{}': {}", path, e))?;

        let mut lines = vec![format!("Contents of '{}'", path)];
        while let Ok(Some(entry)) = entries.next_entry().await {
            let meta = entry.metadata().await.ok();
            let name = entry.file_name().to_string_lossy().to_string();
            let ty = if meta.map(|m| m.is_dir()).unwrap_or(false) {
                "dir"
            } else {
                "file"
            };
            lines.push(format!("  [{}] {}", ty, name));
        }
        Ok(lines.join("\n"))
    }
}

/// Execute an external process (python, node, shell script, etc.)
pub struct ProcessExecTool {
    allowed_work_dirs: Vec<PathBuf>,
}

impl ProcessExecTool {
    pub fn new(allowed_work_dirs: Vec<PathBuf>) -> Self {
        Self { allowed_work_dirs }
    }

    fn validate_command(&self, command: &str) -> Result<(), String> {
        let lower = command.to_lowercase();

        // Exact string matches for dangerous commands
        let dangerous_exact = [
            "rm -rf /",
            "rm -rf /*",
            ":(){ :|:& };:",
            "> /dev/sda",
            "dd if=/dev/zero",
        ];
        for d in &dangerous_exact {
            if lower.contains(*d) {
                return Err(format!("Dangerous command pattern blocked: {}", d));
            }
        }

        // Prefix match for mkfs
        if lower.contains("mkfs.") {
            return Err("Dangerous command pattern blocked: mkfs.".to_string());
        }

        // Regex matches for pipe-to-shell attacks
        let pipe_patterns = [
            Regex::new(r"curl\s+.*\|\s*(ba)?sh").unwrap(),
            Regex::new(r"wget\s+.*\|\s*(ba)?sh").unwrap(),
            Regex::new(r"curl\s+.*-\s*(ba)?sh").unwrap(),
            Regex::new(r"fetch\s+.*\|\s*(ba)?sh").unwrap(),
        ];
        for re in &pipe_patterns {
            if re.is_match(&lower) {
                return Err(format!(
                    "Dangerous command pattern blocked: pipe-to-shell ({}",
                    re.as_str()
                ));
            }
        }

        Ok(())
    }

    fn resolve_working_dir(&self, specified: Option<&str>, default: &Path) -> Result<PathBuf, String> {
        let dir = if let Some(d) = specified {
            let p = PathBuf::from(d);
            if p.is_absolute() {
                p
            } else {
                default.join(p)
            }
        } else {
            default.to_path_buf()
        };

        // Security: ensure the resolved directory is within allowed prefixes
        let canonical = std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
        let allowed = self.allowed_work_dirs.iter().any(|allowed| {
            let allowed_canonical = std::fs::canonicalize(allowed).unwrap_or_else(|_| allowed.clone());
            canonical.starts_with(&allowed_canonical)
        });
        if !allowed {
            return Err(format!(
                "Working directory '{}' is outside allowed skill directories.",
                dir.display()
            ));
        }
        Ok(dir)
    }
}

#[async_trait::async_trait]
impl SkillTool for ProcessExecTool {
    fn name(&self) -> &str {
        "process_exec"
    }

    fn description(&self) -> &str {
        "Execute an external command in a subprocess (e.g. python3 script.py, node script.js). \
Parameters: command (string), working_dir (string, optional), timeout_ms (integer, optional, default 30000)"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Shell command to execute" },
                "working_dir": { "type": "string", "description": "Working directory for the command" },
                "timeout_ms": { "type": "integer", "description": "Timeout in milliseconds", "default": 30000 }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, params: &Value) -> Result<String, String> {
        let command = params["command"].as_str().ok_or("Missing 'command' parameter")?;
        self.validate_command(command)?;

        let default_dir = self.allowed_work_dirs.first().cloned().unwrap_or_else(|| PathBuf::from("."));
        let work_dir = self.resolve_working_dir(params["working_dir"].as_str(), &default_dir)?;

        let timeout_ms = params["timeout_ms"].as_u64().unwrap_or(30000);

        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg(command);
        cmd.current_dir(&work_dir);
        cmd.kill_on_drop(true);
        // Clear environment to avoid leaking secrets
        cmd.env_clear();
        cmd.env("PATH", std::env::var("PATH").unwrap_or_default());

        // 🆕 FIX: Apply Linux sandbox (namespaces, rlimits, privilege drop)
        apply_sandbox(&mut cmd, &default_dir);

        let output = tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            cmd.output(),
        )
        .await
        .map_err(|_| {
            format!(
                "❌ COMMAND TIMEOUT after {timeout_ms}ms: '{command}'\n\
                 The command took too long to finish. Possible causes:\n\
                 1. Infinite loop or blocking input in the script\n\
                 2. Processing too much data — try filtering or sampling\n\
                 3. Network request hanging — consider adding a shorter internal timeout\n\
                 Tip: If this is expected, increase timeout_ms parameter."
            )
        })?
        .map_err(|e| {
            format!(
                "❌ FAILED TO EXECUTE COMMAND: '{command}'\n\
                 Reason: {e}\n\
                 Common causes:\n\
                 1. Command not found in PATH — check the executable name\n\
                 2. Working directory does not exist: '{}'\n\
                 3. Insufficient permissions — the sandbox may block this operation\n\
                 4. Missing interpreter (e.g. python3, node) — verify installation",
                work_dir.display()
            )
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        let mut result = if exit_code != 0 {
            format!(
                "⚠️  COMMAND EXECUTED BUT RETURNED NON-ZERO EXIT CODE: {exit_code}\n\
                 Command: '{command}'\n\
                 Working directory: '{}'\n\
                 Review STDERR below for error details. Do NOT blindly retry the same command — \
                 analyze the error and fix the underlying issue first.\n\n",
                work_dir.display()
            )
        } else {
            format!("✅ Exit code: {exit_code}\n")
        };
        if !stdout.is_empty() {
            result.push_str(&format!("STDOUT:\n{}\n", stdout));
        }
        if !stderr.is_empty() {
            result.push_str(&format!("STDERR:\n{}\n", stderr));
        }
        Ok(result.trim().to_string())
    }
}

/// Execute a bash shell command
pub struct BashShellTool {
    allowed_work_dirs: Vec<PathBuf>,
}

impl BashShellTool {
    pub fn new(allowed_work_dirs: Vec<PathBuf>) -> Self {
        Self { allowed_work_dirs }
    }
}

#[async_trait::async_trait]
impl SkillTool for BashShellTool {
    fn name(&self) -> &str {
        "bash_shell"
    }

    fn description(&self) -> &str {
        "Execute a bash command. Same as process_exec but explicitly for bash. \
Parameters: command (string), working_dir (string, optional), timeout_ms (integer, optional, default 30000)"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Bash command to execute" },
                "working_dir": { "type": "string", "description": "Working directory" },
                "timeout_ms": { "type": "integer", "description": "Timeout in milliseconds", "default": 30000 }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, params: &Value) -> Result<String, String> {
        // Delegate to ProcessExecTool with bash explicitly
        let exec_tool = ProcessExecTool::new(self.allowed_work_dirs.clone());
        exec_tool.execute(params).await
    }
}

/// Call another registered skill from within a skill execution
pub struct SkillCallTool {
    agent: Arc<Agent>,
}

impl SkillCallTool {
    pub fn new(agent: Arc<Agent>) -> Self {
        Self { agent }
    }
}

#[async_trait::async_trait]
impl SkillTool for SkillCallTool {
    fn name(&self) -> &str {
        "skill_call"
    }

    fn description(&self) -> &str {
        "Call another registered skill by ID. Parameters: skill_id (string), input (string), params (object, optional)"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "skill_id": { "type": "string", "description": "ID of the skill to call" },
                "input": { "type": "string", "description": "Input text to pass to the skill" },
                "params": { "type": "object", "description": "Optional parameters" }
            },
            "required": ["skill_id"]
        })
    }

    async fn execute(&self, params: &Value) -> Result<String, String> {
        let skill_id = params["skill_id"].as_str()
            .ok_or("Missing 'skill_id' parameter")?;
        let input = params["input"].as_str().unwrap_or("");

        info!("SkillCallTool: executing skill '{}' with input: {}", skill_id, input);

        match self.agent.execute_skill_by_id(skill_id, input, None).await {
            Ok(result) => {
                info!("SkillCallTool: skill '{}' executed successfully in {}ms", skill_id, result.execution_time_ms);
                Ok(result.output)
            }
            Err(e) => {
                warn!("SkillCallTool: skill '{}' execution failed: {}", skill_id, e);
                Err(format!("Skill execution failed: {}", e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_call_tool_name_and_schema() {
        let agent = Arc::new(crate::AgentBuilder::new("test").build());
        let tool = SkillCallTool::new(agent);
        assert_eq!(tool.name(), "skill_call");
        let schema = tool.parameters_schema();
        assert!(schema.get("properties").unwrap().get("skill_id").is_some());
    }

    #[tokio::test]
    async fn test_skill_call_tool_missing_skill_id() {
        let agent = Arc::new(crate::AgentBuilder::new("test").build());
        let tool = SkillCallTool::new(agent);
        let params = serde_json::json!({"input": "hello"});
        let result = tool.execute(&params).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing 'skill_id'"));
    }
}

/// Build the default tool set for skill execution
pub fn default_tool_set(skill_dir: &Path) -> HashMap<String, Box<dyn SkillTool>> {
    let dirs = vec![skill_dir.to_path_buf()];
    let mut tools: HashMap<String, Box<dyn SkillTool>> = HashMap::new();
    tools.insert("file_read".to_string(), Box::new(FileReadTool));
    tools.insert("file_write".to_string(), Box::new(FileWriteTool));
    tools.insert("file_list".to_string(), Box::new(FileListTool));
    tools.insert(
        "process_exec".to_string(),
        Box::new(ProcessExecTool::new(dirs.clone())),
    );
    tools.insert(
        "bash_shell".to_string(),
        Box::new(BashShellTool::new(dirs)),
    );
    tools
}

/// Build extended tool set including skill_call (requires Agent)
pub fn extended_tool_set(skill_dir: &Path, agent: Arc<Agent>) -> HashMap<String, Box<dyn SkillTool>> {
    let mut tools = default_tool_set(skill_dir);
    tools.insert(
        "skill_call".to_string(),
        Box::new(SkillCallTool::new(agent)),
    );
    tools
}

/// Render tool definitions as a compact markdown list for the ReAct system prompt
pub fn render_tools_for_prompt(tools: &HashMap<String, Box<dyn SkillTool>>) -> String {
    let mut lines = vec!["Available tools:".to_string()];
    for tool in tools.values() {
        lines.push(format!(
            "- {}: {} Schema: {}",
            tool.name(),
            tool.description(),
            tool.parameters_schema()
        ));
    }
    lines.join("\n")
}
