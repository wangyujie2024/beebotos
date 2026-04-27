//! Tool: read_skill
//!
//! Reads a skill's SKILL.md content by ID.

use std::sync::Arc;

use crate::llm::types::{FunctionDefinition, Tool};
use crate::llm::ToolHandler;
use crate::skills::registry::SkillRegistry;

/// Read skill file tool
pub struct ReadSkillTool {
    registry: Arc<SkillRegistry>,
}

impl ReadSkillTool {
    /// Create new read skill tool
    pub fn new(registry: Arc<SkillRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait::async_trait]
impl ToolHandler for ReadSkillTool {
    fn definition(&self) -> Tool {
        Tool {
            r#type: "function".to_string(),
            function: FunctionDefinition {
                name: "read_skill".to_string(),
                description: Some(
                    "读取指定技能的 SKILL.md 文件内容。Agent 在确定某个技能适用于当前任务后，应调用此工具读取完整指令。"
                        .to_string(),
                ),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "skill_id": {
                            "type": "string",
                            "description": "技能 ID"
                        }
                    },
                    "required": ["skill_id"]
                }),
            },
        }
    }

    async fn execute(&self, arguments: &str) -> Result<String, String> {
        let args: serde_json::Value = serde_json::from_str(arguments)
            .map_err(|e| format!("Invalid arguments: {}", e))?;

        let skill_id = args["skill_id"].as_str().ok_or("Missing skill_id")?;

        // Find skill in registry
        let registered = self
            .registry
            .get(skill_id)
            .await
            .ok_or_else(|| format!("Skill '{}' not found", skill_id))?;

        // Record usage
        self.registry.record_usage(skill_id).await;

        // Read the SKILL.md file
        let skill_md_path = &registered.skill.skill_md_path;
        let content = tokio::fs::read_to_string(skill_md_path)
            .await
            .map_err(|e| format!("Failed to read skill file: {}", e))?;

        // Truncate if too long (max 64KB to avoid overwhelming context)
        const MAX_LEN: usize = 64 * 1024;
        if content.len() > MAX_LEN {
            let end = content[..MAX_LEN]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(MAX_LEN);
            Ok(format!(
                "{content_truncated}\n\n[... {more} more characters truncated ...]",
                content_truncated = &content[..end],
                more = content.len() - end
            ))
        } else {
            Ok(content)
        }
    }
}
