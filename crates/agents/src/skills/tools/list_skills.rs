//! Tool: list_skills
//!
//! Returns a list of available skills from the registry.

use std::sync::Arc;

use crate::llm::types::{FunctionDefinition, Tool};
use crate::llm::ToolHandler;
use crate::skills::registry::SkillRegistry;

/// List available skills tool
pub struct ListSkillsTool {
    registry: Arc<SkillRegistry>,
}

impl ListSkillsTool {
    /// Create new list skills tool
    pub fn new(registry: Arc<SkillRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait::async_trait]
impl ToolHandler for ListSkillsTool {
    fn definition(&self) -> Tool {
        Tool {
            r#type: "function".to_string(),
            function: FunctionDefinition {
                name: "list_skills".to_string(),
                description: Some("列出可用的技能，可按分类或关键词过滤".to_string()),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "category": {
                            "type": "string",
                            "description": "按分类过滤（可选）"
                        },
                        "query": {
                            "type": "string",
                            "description": "搜索关键词（可选）"
                        }
                    }
                }),
            },
        }
    }

    async fn execute(&self, arguments: &str) -> Result<String, String> {
        let args: serde_json::Value = serde_json::from_str(arguments)
            .map_err(|e| format!("Invalid arguments: {}", e))?;

        let category = args["category"].as_str();
        let query = args["query"].as_str();

        let skills = if let Some(q) = query {
            self.registry.search(q).await
        } else if let Some(cat) = category {
            self.registry.by_category(cat).await
        } else {
            self.registry.list_enabled().await
        };

        let results: Vec<serde_json::Value> = skills
            .into_iter()
            .map(|s| {
                serde_json::json!({
                    "id": s.skill.id,
                    "name": s.skill.name,
                    "description": s.skill.manifest.description,
                    "version": s.skill.version.to_string(),
                    "category": s.category,
                    "tags": s.tags,
                    "usage_count": s.usage_count,
                })
            })
            .collect();

        if results.is_empty() {
            Ok("No skills found.".to_string())
        } else {
            serde_json::to_string_pretty(&results)
                .map_err(|e| format!("Failed to serialize: {}", e))
        }
    }
}
