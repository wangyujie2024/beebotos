//! Skills Module
//!
//! Skill system for agent capabilities (ClawHub integration).

pub mod builtin_loader;
pub mod command_handler;
pub mod dynamic;
pub mod executor;
pub mod hub;
pub mod instance_manager;
pub mod link_handler;
pub mod loader;
pub mod rating;
pub mod registry;
pub mod security;

pub use command_handler::{
    CommandContext, CommandHandler, CommandResult, RuntimeInfo, RuntimeStatus,
};
pub use dynamic::{DynamicSkill, DynamicSkillLoader};
pub use executor::{
    SkillContext, SkillExecutionError, SkillExecutionResult, SkillExecutor, StreamChunk,
};
pub use hub::{SkillInfo, SkillsHub};
pub use instance_manager::{
    InstanceError, InstanceFilter, InstanceManager, InstanceStatus, SkillInstance, UsageStats,
};
pub use link_handler::{format_summary_for_display, ContentType, LinkHandler, LinkSummary};
pub use loader::{
    FunctionDef, FunctionParameter, LoadedSkill, SkillLoadError, SkillLoader, SkillManifest,
};
pub use rating::{RatingSummary, SkillRating, SkillRatingStore};
pub use registry::{RegisteredSkill, SkillDefinition, SkillRegistry, Version, VersionError};
pub use security::{SkillSecurityPolicy, SkillSecurityValidator, ValidationError};
