//! Skill Composition Module
//!
//! Provides dynamic skill composition patterns:
//! - Pipeline: sequential chaining
//! - Parallel: concurrent execution with merging
//! - Conditional: branch based on results
//! - Loop: retry until condition met

pub mod conditional;
pub mod definition;
pub mod r#loop;
pub mod parallel;
pub mod pipeline;
pub mod registry;

pub use conditional::{Condition, SkillConditional};
pub use definition::{
    CompositionConfig, CompositionDefinition, ConditionDef, InputMappingDef,
    LoopConditionDef, MergeStrategyDef, ParallelBranchDef, PipelineStepDef,
};
pub use r#loop::{LoopCondition, SkillLoop};
pub use parallel::{MergeStrategy, ParallelBranch, SkillParallel};
pub use pipeline::{InputMapping, PipelineStep, SkillPipeline};
pub use registry::{CompositionRegistry, RegistryError};

use crate::agent_impl::Agent;
use crate::error::AgentError;

/// Common trait for composition nodes
#[async_trait::async_trait]
pub trait CompositionNode: Send + Sync + std::fmt::Debug {
    async fn execute(&self, input: &str, agent: &Agent) -> Result<String, AgentError>;
}

#[async_trait::async_trait]
impl CompositionNode for SkillPipeline {
    async fn execute(&self, input: &str, agent: &Agent) -> Result<String, AgentError> {
        SkillPipeline::execute(self, input, agent).await
    }
}

#[async_trait::async_trait]
impl CompositionNode for SkillParallel {
    async fn execute(&self, input: &str, agent: &Agent) -> Result<String, AgentError> {
        SkillParallel::execute(self, input, agent).await
    }
}

#[async_trait::async_trait]
impl CompositionNode for SkillConditional {
    async fn execute(&self, input: &str, agent: &Agent) -> Result<String, AgentError> {
        SkillConditional::execute(self, input, agent).await
    }
}

#[async_trait::async_trait]
impl CompositionNode for SkillLoop {
    async fn execute(&self, input: &str, agent: &Agent) -> Result<String, AgentError> {
        SkillLoop::execute(self, input, agent).await
    }
}
