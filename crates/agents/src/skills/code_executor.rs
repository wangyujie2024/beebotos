//! Code Skill Executor
//!
//! Executes "code-driven" skills (SKILL.md + .py/.js/.sh scripts) by
//! loading the markdown document, listing available scripts, and
//! delegating script execution to the ReAct executor.
//!
//! 🟢 P1 OPTIMIZE: Single-shot command generation for simple requests
//! avoids the expensive multi-turn ReAct loop (~60s → ~15s).

use std::path::Path;
use std::sync::Arc;

use tracing::{debug, info};

use crate::communication::{LLMCallInterface, Message as CommMessage, PlatformType};
use crate::error::AgentError;
use crate::skills::react_executor::ReActExecutor;
use crate::skills::tool_set::{default_tool_set, ProcessExecTool, SkillTool};

/// Executor for code-based skills
pub struct CodeSkillExecutor {
    llm: Arc<dyn LLMCallInterface>,
}

impl CodeSkillExecutor {
    pub fn new(llm: Arc<dyn LLMCallInterface>) -> Self {
        Self { llm }
    }

    /// Execute a code skill
    pub async fn execute(
        &self,
        skill_path: &Path,
        user_input: &str,
    ) -> Result<String, AgentError> {
        // Normalize to absolute path so prompts and working directories are unambiguous.
        let skill_path = if skill_path.is_absolute() {
            skill_path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(skill_path))
                .unwrap_or_else(|_| skill_path.to_path_buf())
        };

        if !skill_path.is_dir() {
            return Err(AgentError::Execution(
                "Code skills must be directory-based".to_string(),
            ));
        }

        let skill_md_path = skill_path.join("SKILL.md");
        let skill_md = if skill_md_path.exists() {
            tokio::fs::read_to_string(&skill_md_path)
                .await
                .map_err(|e| AgentError::Execution(format!("Failed to read SKILL.md: {}", e)))?
        } else {
            return Err(AgentError::Execution(
                "SKILL.md not found in skill directory".to_string(),
            ));
        };

        let scripts = list_scripts(&skill_path).await;
        let scripts_info = if scripts.is_empty() {
            "No executable scripts found in this skill.".to_string()
        } else {
            format!(
                "Available scripts in this skill:\n{}",
                scripts
                    .iter()
                    .map(|(name, path)| format!("  - {} ({})", name, path))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };

        let skill_dir_str = skill_path.to_string_lossy().to_string();
        // Replace {SKILL_DIR} placeholder with actual path so the LLM generates valid commands
        let skill_md = skill_md.replace("{SKILL_DIR}", &skill_dir_str);

        // 🟢 P1 OPTIMIZE: Try single-shot command generation first.
        // For simple requests (e.g. "run hello.py") this avoids the expensive ReAct loop.
        match self
            .try_single_shot(&skill_md, &scripts_info, &skill_dir_str, user_input, &skill_path)
            .await
        {
            Ok(result) => {
                info!("Single-shot skill execution succeeded");
                return Ok(result);
            }
            Err(e) => {
                debug!("Single-shot failed ({}), falling back to ReAct", e);
            }
        }

        // Fallback: full ReAct loop
        let system_prompt = format!(
            "You are the '{}' skill. Follow the instructions below to help the user.\n\n\
            {scripts_info}\n\n\
            When constructing commands, use the absolute skill directory path: {skill_dir_str}\n\n\
            IMPORTANT: If the user has provided enough information, execute the script immediately \
            using the process_exec tool. Do not ask follow-up questions unless critical information is missing.\n\n\
            {skill_md}",
            skill_path.file_name().unwrap_or_default().to_string_lossy(),
        );

        let tools = default_tool_set(&skill_path);
        let executor = ReActExecutor::new(self.llm.clone(), tools);

        executor.execute(&system_prompt, user_input).await
    }

    /// 🟢 P1 OPTIMIZE: Single-shot command generation.
    ///
    /// Ask the LLM to produce a JSON object with the exact shell command.
    /// If parsing succeeds, execute it directly via ProcessExecTool.
    /// If the LLM signals ambiguity or JSON parsing fails, return Err so
    /// the caller can fall back to ReAct.
    async fn try_single_shot(
        &self,
        skill_md: &str,
        scripts_info: &str,
        skill_dir_str: &str,
        user_input: &str,
        skill_path: &Path,
    ) -> Result<String, AgentError> {
        let prompt = format!(
            "You are a code-skill assistant. Your job is to turn the user's request into a \
            single shell command that fulfills it.\n\n\
            Skill instructions:\n{skill_md}\n\n\
            {scripts_info}\n\n\
            When constructing commands, use the absolute skill directory path: {skill_dir_str}\n\n\
            User request: {user_input}\n\n\
            Respond with a JSON object ONLY — no markdown, no explanation outside the JSON:\n\
            {{\"command\":\"the exact shell command to run\",\"working_dir\":\"{skill_dir_str}\",\"reasoning\":\"brief explanation\"}}\n\n\
            If the request is unclear or missing critical information, respond with:\n\
            {{\"needs_react\":true,\"reasoning\":\"why\"}}"
        );

        let messages = vec![CommMessage::new(
            uuid::Uuid::new_v4(),
            PlatformType::Custom,
            prompt,
        )];

        // 🟢 P1 OPTIMIZE: Use one_shot context flag to avoid carrying heavy
        // conversation history into skill command generation.
        let mut context = std::collections::HashMap::new();
        context.insert("one_shot".to_string(), "true".to_string());

        let response = self
            .llm
            .call_llm(messages, Some(context))
            .await
            .map_err(|e| AgentError::Execution(format!("Single-shot LLM call failed: {}", e)))?;

        debug!("Single-shot LLM response: {}", response);

        // Try to extract JSON from the response (LLMs sometimes wrap it in markdown)
        let json_str = extract_json(&response);
        let parsed: serde_json::Value =
            serde_json::from_str(json_str).map_err(|e| {
                AgentError::Execution(format!("Failed to parse single-shot JSON: {}", e))
            })?;

        if parsed.get("needs_react").and_then(|v| v.as_bool()).unwrap_or(false) {
            return Err(AgentError::Execution(
                "LLM indicated single-shot is insufficient".to_string(),
            ));
        }

        let command = parsed
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AgentError::Execution("Single-shot JSON missing 'command' field".to_string())
            })?;

        // 🟢 P1 OPTIMIZE: Do NOT pass working_dir — let ProcessExecTool use its
        // default (the skill directory). Passing a relative path here causes
        // resolve_working_dir to incorrectly join it with the default dir.
        info!(
            "Single-shot command: {} (skill dir: {})",
            command, skill_dir_str
        );

        // Execute directly via ProcessExecTool
        let tool = ProcessExecTool::new(vec![skill_path.to_path_buf()]);
        let params = serde_json::json!({
            "command": command,
            "timeout_ms": 30000
        });

        match tool.execute(&params).await {
            Ok(output) => Ok(format!(
                "Command executed successfully.\n\n{}",
                output
            )),
            Err(e) => Err(AgentError::Execution(format!(
                "Single-shot command failed: {}",
                e
            ))),
        }
    }
}

/// Extract the first JSON object from a string, handling markdown fences.
fn extract_json(text: &str) -> &str {
    let trimmed = text.trim();
    // Handle ```json ... ``` fences
    if let Some(start) = trimmed.find("```json") {
        if let Some(end) = trimmed[start + 7..].find("```") {
            return trimmed[start + 7..start + 7 + end].trim();
        }
    }
    if let Some(start) = trimmed.find("```") {
        if let Some(end) = trimmed[start + 3..].find("```") {
            return trimmed[start + 3..start + 3 + end].trim();
        }
    }
    // Try to find the first '{' and last '}'
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if end > start {
                return &trimmed[start..=end];
            }
        }
    }
    trimmed
}

async fn list_scripts(dir: &Path) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(e) => e,
        Err(_) => return result,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if matches!(ext, "py" | "js" | "sh" | "ts") {
                let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                let abs = path.to_string_lossy().to_string();
                result.push((name, abs));
            }
        }
    }
    result
}
