//! Knowledge Skill Executor
//!
//! Executes "knowledge-driven" skills (pure SKILL.md) by loading the
//! markdown document and running it through the ReAct executor.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::communication::LLMCallInterface;
use crate::error::AgentError;
use crate::skills::react_executor::ReActExecutor;
use crate::skills::tool_set::default_tool_set;

/// Executor for knowledge-based skills
pub struct KnowledgeSkillExecutor {
    llm: Arc<dyn LLMCallInterface>,
}

impl KnowledgeSkillExecutor {
    pub fn new(llm: Arc<dyn LLMCallInterface>) -> Self {
        Self { llm }
    }

    /// Execute a knowledge skill
    pub async fn execute(
        &self,
        skill_path: &Path,
        user_input: &str,
    ) -> Result<String, AgentError> {
        let (skill_md, skill_name, tool_root) = if skill_path.is_dir() {
            let md = skill_path.join("SKILL.md");
            let content = tokio::fs::read_to_string(&md)
                .await
                .map_err(|e| AgentError::Execution(format!("Failed to read SKILL.md: {}", e)))?;
            let name = skill_path.file_name().unwrap_or_default().to_string_lossy().to_string();
            (content, name, skill_path.to_path_buf())
        } else if skill_path.extension().and_then(|e| e.to_str()) == Some("md") {
            let content = tokio::fs::read_to_string(skill_path)
                .await
                .map_err(|e| AgentError::Execution(format!("Failed to read skill file: {}", e)))?;
            let name = skill_path.file_stem().unwrap_or_default().to_string_lossy().to_string();
            let root = skill_path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
            (content, name, root)
        } else {
            return Err(AgentError::Execution(
                "SKILL.md not found in skill directory".to_string(),
            ));
        };

        let system_prompt = format!(
            "You are the '{}' skill. Follow the instructions below to help the user.\n\n{}",
            skill_name,
            skill_md
        );

        let tools = default_tool_set(&tool_root);
        let executor = ReActExecutor::new(self.llm.clone(), tools);

        executor.execute(&system_prompt, user_input).await
    }
}
