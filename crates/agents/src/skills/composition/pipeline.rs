//! Skill Pipeline — Sequential chaining of skills

use tracing::{info, warn};

use crate::agent_impl::Agent;
use crate::error::AgentError;

/// A sequential pipeline of skill executions
#[derive(Debug, Clone)]
pub struct SkillPipeline {
    pub steps: Vec<PipelineStep>,
}

/// A single step in a pipeline
#[derive(Debug, Clone)]
pub struct PipelineStep {
    pub skill_id: String,
    pub input_mapping: InputMapping,
    pub output_schema: Option<serde_json::Value>,
}

/// How to construct input for a pipeline step from previous output
#[derive(Debug, Clone)]
pub enum InputMapping {
    /// Pass through the raw output string
    PassThrough,
    /// Extract a JSON field by pointer path (e.g. "/result/summary")
    JsonField(String),
    /// Use a format string with {input} placeholder
    Format(String),
    /// Static value
    Static(String),
    /// Combine multiple sources
    Combine(Vec<(String, InputMapping)>),
}

impl SkillPipeline {
    /// Create a new pipeline
    pub fn new(steps: Vec<PipelineStep>) -> Self {
        Self { steps }
    }

    /// Execute the pipeline sequentially
    pub async fn execute(&self, initial_input: &str, agent: &Agent) -> Result<String, AgentError> {
        let mut current_output = initial_input.to_string();

        for (idx, step) in self.steps.iter().enumerate() {
            // Construct step input based on mapping
            let step_input = step.input_mapping.apply(&current_output)?;

            info!(
                "Pipeline step {}/{}: executing skill={}",
                idx + 1,
                self.steps.len(),
                step.skill_id
            );

            match agent.execute_skill_by_id(&step.skill_id, &step_input, None).await {
                Ok(result) => {
                    current_output = result.output;
                }
                Err(e) => {
                    warn!(
                        "Pipeline step {}/{} failed for skill '{}': {}",
                        idx + 1,
                        self.steps.len(),
                        step.skill_id,
                        e
                    );
                    return Err(e);
                }
            }
        }

        Ok(current_output)
    }
}

impl InputMapping {
    /// Apply mapping to extract input from previous output
    pub fn apply(&self, previous_output: &str) -> Result<String, AgentError> {
        match self {
            InputMapping::PassThrough => Ok(previous_output.to_string()),
            InputMapping::JsonField(path) => {
                let value: serde_json::Value = serde_json::from_str(previous_output)
                    .map_err(|e| AgentError::Execution(format!("Invalid JSON for field extraction: {}", e)))?;
                let result = value
                    .pointer(path)
                    .ok_or_else(|| AgentError::Execution(format!("JSON path not found: {}", path)))?;
                Ok(result.to_string())
            }
            InputMapping::Format(template) => Ok(template.replace("{input}", previous_output)),
            InputMapping::Static(val) => Ok(val.clone()),
            InputMapping::Combine(mappings) => {
                let mut result = serde_json::Map::new();
                for (key, mapping) in mappings {
                    let val = mapping.apply(previous_output)?;
                    result.insert(key.clone(), serde_json::Value::String(val));
                }
                Ok(serde_json::Value::Object(result).to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_mapping() {
        let json = r#"{"result": {"summary": "Hello World"}}"#;

        let pass = InputMapping::PassThrough.apply(json).unwrap();
        assert_eq!(pass, json);

        let field = InputMapping::JsonField("/result/summary".to_string()).apply(json).unwrap();
        assert_eq!(field, "\"Hello World\"");

        let fmt = InputMapping::Format("Summary: {input}".to_string()).apply(json).unwrap();
        assert!(fmt.contains("Summary:"));

        let st = InputMapping::Static("fixed".to_string()).apply(json).unwrap();
        assert_eq!(st, "fixed");
    }
}
