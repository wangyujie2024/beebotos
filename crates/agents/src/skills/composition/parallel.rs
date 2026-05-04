//! Skill Parallel — Concurrent execution of multiple skills with result merging

use futures::future::join_all;
use tracing::{info, warn};

use crate::agent_impl::Agent;
use crate::error::AgentError;

/// Parallel skill execution group
#[derive(Debug, Clone)]
pub struct SkillParallel {
    pub branches: Vec<ParallelBranch>,
    pub merge_strategy: MergeStrategy,
}

/// A single parallel branch
#[derive(Debug, Clone)]
pub struct ParallelBranch {
    pub branch_id: String,
    pub skill_id: String,
    pub input_override: Option<String>,
}

/// How to merge parallel branch results
#[derive(Debug, Clone)]
pub enum MergeStrategy {
    /// Concatenate all outputs as strings
    Concat,
    /// Merge as JSON array
    JsonArray,
    /// Merge as JSON object keyed by branch_id
    JsonObject,
    /// Call LLM to summarize results
    LlmSummarize { prompt_template: String },
    /// Custom skill to handle merging
    CustomSkill { skill_id: String },
}

impl SkillParallel {
    /// Create new parallel execution
    pub fn new(branches: Vec<ParallelBranch>, merge_strategy: MergeStrategy) -> Self {
        Self {
            branches,
            merge_strategy,
        }
    }

    /// Execute branches concurrently and merge results
    pub async fn execute(&self, input: &str, agent: &Agent) -> Result<String, AgentError> {
        info!(
            "Executing {} parallel branches with strategy {:?}",
            self.branches.len(),
            self.merge_strategy
        );

        let futures: Vec<_> = self
            .branches
            .iter()
            .map(|branch| {
                let branch_input = branch.input_override.as_deref().unwrap_or(input).to_string();
                let branch_id = branch.branch_id.clone();
                let skill_id = branch.skill_id.clone();
                async move {
                    info!("Parallel branch {}: executing skill={}", branch_id, skill_id);
                    match agent.execute_skill_by_id(&skill_id, &branch_input, None).await {
                        Ok(result) => (branch_id, result.output),
                        Err(e) => {
                            warn!(
                                "Parallel branch {} skill '{}' failed: {}",
                                branch_id, skill_id, e
                            );
                            (branch_id, format!("[ERROR: {}]", e))
                        }
                    }
                }
            })
            .collect();

        let results = join_all(futures).await;
        self.merge_strategy.merge(results, agent).await
    }
}

impl MergeStrategy {
    /// Merge branch results
    pub async fn merge(
        &self,
        results: Vec<(String, String)>,
        agent: &Agent,
    ) -> Result<String, AgentError> {
        match self {
            MergeStrategy::Concat => {
                let output = results
                    .into_iter()
                    .map(|(_, r)| r)
                    .collect::<Vec<_>>()
                    .join("\n\n---\n\n");
                Ok(output)
            }
            MergeStrategy::JsonArray => {
                let array: Vec<serde_json::Value> = results
                    .into_iter()
                    .map(|(id, r)| {
                        serde_json::json!({
                            "branch_id": id,
                            "output": r
                        })
                    })
                    .collect();
                Ok(serde_json::to_string(&array).unwrap_or_default())
            }
            MergeStrategy::JsonObject => {
                let mut obj = serde_json::Map::new();
                for (id, r) in results {
                    obj.insert(id, serde_json::Value::String(r));
                }
                Ok(serde_json::Value::Object(obj).to_string())
            }
            MergeStrategy::LlmSummarize { prompt_template } => {
                let combined = results
                    .into_iter()
                    .map(|(id, r)| format!("[{}]: {}", id, r))
                    .collect::<Vec<_>>()
                    .join("\n");
                let prompt = format!(
                    "{}\n\n请对以下多个分支的结果进行汇总和总结：\n\n{}",
                    prompt_template, combined
                );
                match agent.call_llm_prompt(&prompt, Some::<String>(
                    "你是一个结果汇总助手。你的任务是将多个并行执行分支的输出合并为一个连贯、简洁的总结。".into()
                )).await {
                    Ok(summary) => Ok(summary),
                    Err(e) => {
                        warn!("LlmSummarize failed, falling back to concatenation: {}", e);
                        Ok(format!("{combined}\n\n[LLM summarization failed: {e}]"))
                    }
                }
            }
            MergeStrategy::CustomSkill { skill_id } => {
                let combined = results
                    .into_iter()
                    .map(|(id, r)| format!("[{}]: {}", id, r))
                    .collect::<Vec<_>>()
                    .join("\n");
                match agent.execute_skill_by_id(skill_id, &combined, None).await {
                    Ok(result) => Ok(result.output),
                    Err(e) => {
                        warn!("CustomSkill merge failed for skill '{}': {}", skill_id, e);
                        Ok(format!("{combined}\n\n[Custom skill merge failed: {e}]"))
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_agent() -> Agent {
        crate::AgentBuilder::new("test-agent").build()
    }

    #[tokio::test]
    async fn test_merge_concat() {
        let strategy = MergeStrategy::Concat;
        let results = vec![
            ("a".to_string(), "Result A".to_string()),
            ("b".to_string(), "Result B".to_string()),
        ];
        let agent = test_agent();
        let merged = strategy.merge(results, &agent).await.unwrap();
        assert!(merged.contains("Result A"));
        assert!(merged.contains("Result B"));
    }

    #[tokio::test]
    async fn test_merge_json_object() {
        let strategy = MergeStrategy::JsonObject;
        let results = vec![
            ("a".to_string(), "Result A".to_string()),
            ("b".to_string(), "Result B".to_string()),
        ];
        let agent = test_agent();
        let merged = strategy.merge(results, &agent).await.unwrap();
        assert!(merged.contains("\"a\":\"Result A\""));
    }
}
