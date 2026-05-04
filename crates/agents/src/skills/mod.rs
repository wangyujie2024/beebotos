//! Skills Module
//!
//! Skill system for agent capabilities (ClawHub integration).

pub mod builtin_loader;
pub mod code_executor;
pub mod command_handler;
// 🟢 P1 FIX: Skill composition patterns (pipeline, parallel, conditional, loop)
pub mod composition;
pub mod discovery;
pub mod dynamic;
pub mod executor;
pub mod hub;
pub mod instance_manager;
pub mod knowledge_executor;
pub mod link_handler;
pub mod loader;
pub mod process_sandbox;
pub mod rating;
pub mod react_executor;
pub mod registry;
pub mod security;
pub mod tool_set;

pub use code_executor::CodeSkillExecutor;
pub use command_handler::{CommandContext, CommandHandler, CommandResult, RuntimeInfo, RuntimeStatus};
pub use discovery::{SkillDiscovery, SkillKind, SkillMetadata};
pub use dynamic::{DynamicSkill, DynamicSkillLoader};
pub use executor::{SkillContext, SkillExecutionError, SkillExecutionResult, SkillExecutor, StreamChunk};
pub use hub::{SkillInfo, SkillsHub};
pub use instance_manager::{InstanceError, InstanceFilter, InstanceManager, InstanceStatus, SkillInstance, UsageStats};
pub use knowledge_executor::KnowledgeSkillExecutor;
pub use link_handler::{format_summary_for_display, LinkHandler, LinkSummary, ContentType};
pub use loader::{FunctionDef, FunctionParameter, LoadedSkill, SkillLoadError, SkillLoader, SkillManifest};
pub use rating::{RatingSummary, SkillRating, SkillRatingStore};
pub use react_executor::ReActExecutor;
pub use registry::{RegisteredSkill, SkillDefinition, SkillRegistry, Version, VersionError};
pub use security::{SkillSecurityPolicy, SkillSecurityValidator, ValidationError};
pub use tool_set::{default_tool_set, BashShellTool, FileListTool, FileReadTool, FileWriteTool, ProcessExecTool, SkillTool};
