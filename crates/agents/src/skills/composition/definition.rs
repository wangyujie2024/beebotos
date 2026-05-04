//! Skill Composition Definition
//!
//! Serializable configuration structures for declarative skill composition.
//! Used for persistence (YAML/JSON) and HTTP API exchange.
//!
//! At runtime, these definitions are converted into executable composition nodes
//! (SkillPipeline, SkillParallel, SkillConditional, SkillLoop).

use serde::{Deserialize, Serialize};

use crate::agent_impl::Agent;
use crate::error::AgentError;
use crate::skills::composition::{
    CompositionNode, SkillConditional, SkillLoop, SkillParallel, SkillPipeline,
};

/// Top-level composition definition — persisted and exchangeable
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionDefinition {
    /// Unique identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Description of what this composition does
    pub description: String,
    /// The composition configuration
    #[serde(flatten)]
    pub config: CompositionConfig,
    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
    /// Creation timestamp (ISO 8601)
    #[serde(default)]
    pub created_at: String,
    /// Last update timestamp (ISO 8601)
    #[serde(default)]
    pub updated_at: String,
}

/// Discriminated union of all composition types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CompositionConfig {
    /// Sequential pipeline of skills
    Pipeline { steps: Vec<PipelineStepDef> },
    /// Concurrent execution of multiple skills with merging
    Parallel {
        branches: Vec<ParallelBranchDef>,
        #[serde(flatten)]
        merge: MergeStrategyDef,
    },
    /// Branch execution based on a condition
    Conditional {
        condition: ConditionDef,
        then_branch: Box<CompositionConfig>,
        else_branch: Option<Box<CompositionConfig>>,
    },
    /// Repeat execution until condition is met
    Loop {
        body: Box<CompositionConfig>,
        until: LoopConditionDef,
        #[serde(default = "default_max_iterations")]
        max_iterations: usize,
        #[serde(default)]
        backoff_ms: u64,
    },
}

fn default_max_iterations() -> usize {
    10
}

// ---------------------------------------------------------------------------
// Pipeline
// ---------------------------------------------------------------------------

/// A single step in a pipeline definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStepDef {
    pub skill_id: String,
    #[serde(default)]
    pub input_mapping: InputMappingDef,
    #[serde(default)]
    pub output_schema: Option<serde_json::Value>,
}

/// How to construct input for a pipeline step from previous output
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputMappingDef {
    /// Pass through the raw output string
    PassThrough,
    /// Extract a JSON field by pointer path (e.g. "/result/summary")
    JsonField { path: String },
    /// Use a format string with {input} placeholder
    Format { template: String },
    /// Static value
    Static { value: String },
    /// Combine multiple sources into a JSON object
    Combine { mappings: Vec<CombineMapping> },
}

impl Default for InputMappingDef {
    fn default() -> Self {
        Self::PassThrough
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombineMapping {
    pub key: String,
    #[serde(flatten)]
    pub mapping: InputMappingDef,
}

// ---------------------------------------------------------------------------
// Parallel
// ---------------------------------------------------------------------------

/// A single parallel branch definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelBranchDef {
    pub branch_id: String,
    pub skill_id: String,
    #[serde(default)]
    pub input_override: Option<String>,
}

/// How to merge parallel branch results
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "merge_type", rename_all = "snake_case")]
pub enum MergeStrategyDef {
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

impl Default for MergeStrategyDef {
    fn default() -> Self {
        Self::Concat
    }
}

// ---------------------------------------------------------------------------
// Conditional
// ---------------------------------------------------------------------------

/// Condition evaluation types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConditionDef {
    /// Check if output contains a substring
    OutputContains { value: String },
    /// Check if output equals a value
    OutputEquals { value: String },
    /// Check JSON field by pointer
    JsonFieldEquals { path: String, expected: String },
    /// Check exit code (for subprocess-based skills)
    ExitCode { code: i32 },
    /// Custom expression evaluated by WorkflowEngine
    Expression { expr: String },
    /// LLM-based judgment
    LlmJudge { prompt: String },
}

// ---------------------------------------------------------------------------
// Loop
// ---------------------------------------------------------------------------

/// Condition to stop the loop
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LoopConditionDef {
    /// Stop when output contains substring
    OutputContains { value: String },
    /// Stop when output equals value
    OutputEquals { value: String },
    /// Stop when exit code matches
    ExitCode { code: i32 },
    /// Stop when JSON field matches
    JsonFieldEquals { path: String, expected: String },
    /// Stop when LLM judges the output meets criteria
    LlmJudge { prompt: String },
    /// Retry up to N times regardless of output
    MaxAttempts { max: usize },
}

// ---------------------------------------------------------------------------
// Conversion helpers: Definition -> Runtime
// ---------------------------------------------------------------------------

impl CompositionDefinition {
    /// Convert this definition into an executable composition node
    pub fn to_runtime(&self, agent: &Agent) -> Result<Box<dyn CompositionNode>, AgentError> {
        self.config.to_runtime(agent)
    }
}

impl CompositionConfig {
    /// Convert configuration into an executable composition node
    pub fn to_runtime(&self, agent: &Agent) -> Result<Box<dyn CompositionNode>, AgentError> {
        match self {
            CompositionConfig::Pipeline { steps } => {
                let pipeline_steps: Vec<_> = steps
                    .iter()
                    .map(|s| super::pipeline::PipelineStep {
                        skill_id: s.skill_id.clone(),
                        input_mapping: s.input_mapping.to_runtime(),
                        output_schema: s.output_schema.clone(),
                    })
                    .collect();
                Ok(Box::new(SkillPipeline::new(pipeline_steps)))
            }
            CompositionConfig::Parallel { branches, merge } => {
                let parallel_branches: Vec<_> = branches
                    .iter()
                    .map(|b| super::parallel::ParallelBranch {
                        branch_id: b.branch_id.clone(),
                        skill_id: b.skill_id.clone(),
                        input_override: b.input_override.clone(),
                    })
                    .collect();
                Ok(Box::new(SkillParallel::new(
                    parallel_branches,
                    merge.to_runtime(),
                )))
            }
            CompositionConfig::Conditional {
                condition,
                then_branch,
                else_branch,
            } => {
                let then_node = then_branch.to_runtime(agent)?;
                let else_node = else_branch
                    .as_ref()
                    .map(|b| b.to_runtime(agent))
                    .transpose()?;
                Ok(Box::new(SkillConditional::new(
                    condition.to_runtime(),
                    then_node,
                    else_node,
                )))
            }
            CompositionConfig::Loop {
                body,
                until,
                max_iterations,
                backoff_ms,
            } => {
                let body_node = body.to_runtime(agent)?;
                Ok(Box::new(SkillLoop::new(
                    body_node,
                    until.to_runtime(),
                    *max_iterations,
                    *backoff_ms,
                )))
            }
        }
    }
}

impl InputMappingDef {
    fn to_runtime(&self) -> super::pipeline::InputMapping {
        match self {
            InputMappingDef::PassThrough => super::pipeline::InputMapping::PassThrough,
            InputMappingDef::JsonField { path } => {
                super::pipeline::InputMapping::JsonField(path.clone())
            }
            InputMappingDef::Format { template } => {
                super::pipeline::InputMapping::Format(template.clone())
            }
            InputMappingDef::Static { value } => {
                super::pipeline::InputMapping::Static(value.clone())
            }
            InputMappingDef::Combine { mappings } => super::pipeline::InputMapping::Combine(
                mappings
                    .iter()
                    .map(|m| (m.key.clone(), m.mapping.to_runtime()))
                    .collect(),
            ),
        }
    }
}

impl MergeStrategyDef {
    fn to_runtime(&self) -> super::parallel::MergeStrategy {
        match self {
            MergeStrategyDef::Concat => super::parallel::MergeStrategy::Concat,
            MergeStrategyDef::JsonArray => super::parallel::MergeStrategy::JsonArray,
            MergeStrategyDef::JsonObject => super::parallel::MergeStrategy::JsonObject,
            MergeStrategyDef::LlmSummarize { prompt_template } => {
                super::parallel::MergeStrategy::LlmSummarize {
                    prompt_template: prompt_template.clone(),
                }
            }
            MergeStrategyDef::CustomSkill { skill_id } => {
                super::parallel::MergeStrategy::CustomSkill {
                    skill_id: skill_id.clone(),
                }
            }
        }
    }
}

impl ConditionDef {
    fn to_runtime(&self) -> super::conditional::Condition {
        match self {
            ConditionDef::OutputContains { value } => {
                super::conditional::Condition::OutputContains(value.clone())
            }
            ConditionDef::OutputEquals { value } => {
                super::conditional::Condition::OutputEquals(value.clone())
            }
            ConditionDef::JsonFieldEquals { path, expected } => {
                super::conditional::Condition::JsonFieldEquals {
                    path: path.clone(),
                    expected: expected.clone(),
                }
            }
            ConditionDef::ExitCode { code } => {
                super::conditional::Condition::ExitCode(*code)
            }
            ConditionDef::Expression { expr } => {
                super::conditional::Condition::Expression(expr.clone())
            }
            ConditionDef::LlmJudge { prompt } => {
                super::conditional::Condition::LlmJudge {
                    prompt: prompt.clone(),
                }
            }
        }
    }
}

impl LoopConditionDef {
    fn to_runtime(&self) -> super::r#loop::LoopCondition {
        match self {
            LoopConditionDef::OutputContains { value } => {
                super::r#loop::LoopCondition::OutputContains(value.clone())
            }
            LoopConditionDef::OutputEquals { value } => {
                super::r#loop::LoopCondition::OutputEquals(value.clone())
            }
            LoopConditionDef::ExitCode { code } => {
                super::r#loop::LoopCondition::ExitCode(*code)
            }
            LoopConditionDef::JsonFieldEquals { path, expected } => {
                super::r#loop::LoopCondition::JsonFieldEquals {
                    path: path.clone(),
                    expected: expected.clone(),
                }
            }
            LoopConditionDef::LlmJudge { prompt } => {
                super::r#loop::LoopCondition::LlmJudge {
                    prompt: prompt.clone(),
                }
            }
            LoopConditionDef::MaxAttempts { max } => {
                super::r#loop::LoopCondition::MaxAttempts(*max)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_yaml_roundtrip() {
        let def = CompositionDefinition {
            id: "test_pipeline".to_string(),
            name: "Test Pipeline".to_string(),
            description: "A test pipeline".to_string(),
            config: CompositionConfig::Pipeline {
                steps: vec![
                    PipelineStepDef {
                        skill_id: "skill_a".to_string(),
                        input_mapping: InputMappingDef::PassThrough,
                        output_schema: None,
                    },
                    PipelineStepDef {
                        skill_id: "skill_b".to_string(),
                        input_mapping: InputMappingDef::JsonField {
                            path: "/result".to_string(),
                        },
                        output_schema: None,
                    },
                ],
            },
            tags: vec!["test".to_string()],
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        };

        let yaml = serde_yaml::to_string(&def).unwrap();
        let parsed: CompositionDefinition = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.id, "test_pipeline");
        assert_eq!(parsed.name, "Test Pipeline");
        match parsed.config {
            CompositionConfig::Pipeline { steps } => {
                assert_eq!(steps.len(), 2);
                assert_eq!(steps[0].skill_id, "skill_a");
                assert!(matches!(steps[0].input_mapping, InputMappingDef::PassThrough));
                assert_eq!(steps[1].skill_id, "skill_b");
            }
            _ => panic!("Expected Pipeline variant"),
        }
    }

    #[test]
    fn test_conditional_yaml_roundtrip() {
        let def = CompositionDefinition {
            id: "test_conditional".to_string(),
            name: "Test Conditional".to_string(),
            description: "A test conditional".to_string(),
            config: CompositionConfig::Conditional {
                condition: ConditionDef::OutputContains {
                    value: "success".to_string(),
                },
                then_branch: Box::new(CompositionConfig::Pipeline {
                    steps: vec![PipelineStepDef {
                        skill_id: "skill_then".to_string(),
                        input_mapping: InputMappingDef::PassThrough,
                        output_schema: None,
                    }],
                }),
                else_branch: Some(Box::new(CompositionConfig::Pipeline {
                    steps: vec![PipelineStepDef {
                        skill_id: "skill_else".to_string(),
                        input_mapping: InputMappingDef::PassThrough,
                        output_schema: None,
                    }],
                })),
            },
            tags: vec![],
            created_at: String::new(),
            updated_at: String::new(),
        };

        let yaml = serde_yaml::to_string(&def).unwrap();
        let parsed: CompositionDefinition = serde_yaml::from_str(&yaml).unwrap();
        match parsed.config {
            CompositionConfig::Conditional { condition, .. } => {
                assert!(matches!(
                    condition,
                    ConditionDef::OutputContains { ref value } if value == "success"
                ));
            }
            _ => panic!("Expected Conditional variant"),
        }
    }

    #[test]
    fn test_parallel_yaml_roundtrip() {
        let def = CompositionDefinition {
            id: "test_parallel".to_string(),
            name: "Test Parallel".to_string(),
            description: "A test parallel".to_string(),
            config: CompositionConfig::Parallel {
                branches: vec![
                    ParallelBranchDef {
                        branch_id: "a".to_string(),
                        skill_id: "skill_a".to_string(),
                        input_override: None,
                    },
                    ParallelBranchDef {
                        branch_id: "b".to_string(),
                        skill_id: "skill_b".to_string(),
                        input_override: Some("override".to_string()),
                    },
                ],
                merge: MergeStrategyDef::JsonObject,
            },
            tags: vec![],
            created_at: String::new(),
            updated_at: String::new(),
        };

        let yaml = serde_yaml::to_string(&def).unwrap();
        let parsed: CompositionDefinition = serde_yaml::from_str(&yaml).unwrap();
        match parsed.config {
            CompositionConfig::Parallel { branches, merge } => {
                assert_eq!(branches.len(), 2);
                assert!(matches!(merge, MergeStrategyDef::JsonObject));
            }
            _ => panic!("Expected Parallel variant"),
        }
    }

    #[test]
    fn test_loop_yaml_roundtrip() {
        let def = CompositionDefinition {
            id: "test_loop".to_string(),
            name: "Test Loop".to_string(),
            description: "A test loop".to_string(),
            config: CompositionConfig::Loop {
                body: Box::new(CompositionConfig::Pipeline {
                    steps: vec![PipelineStepDef {
                        skill_id: "skill_body".to_string(),
                        input_mapping: InputMappingDef::PassThrough,
                        output_schema: None,
                    }],
                }),
                until: LoopConditionDef::OutputContains {
                    value: "done".to_string(),
                },
                max_iterations: 5,
                backoff_ms: 100,
            },
            tags: vec![],
            created_at: String::new(),
            updated_at: String::new(),
        };

        let yaml = serde_yaml::to_string(&def).unwrap();
        let parsed: CompositionDefinition = serde_yaml::from_str(&yaml).unwrap();
        match parsed.config {
            CompositionConfig::Loop {
                max_iterations,
                backoff_ms,
                ..
            } => {
                assert_eq!(max_iterations, 5);
                assert_eq!(backoff_ms, 100);
            }
            _ => panic!("Expected Loop variant"),
        }
    }
}
