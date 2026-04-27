//! Skills Module
//!
//! Skill system for agent capabilities (ClawHub integration).

pub mod builtin_loader;
pub mod command_handler;
pub mod link_handler;
pub mod loader;
pub mod lockfile;
pub mod rating;
pub mod registry;
pub mod skill_prompt;
pub mod tools;

pub use command_handler::{
    CommandContext, CommandHandler, CommandResult, RuntimeInfo, RuntimeStatus,
};
pub use link_handler::{format_summary_for_display, ContentType, LinkHandler, LinkSummary};
pub use loader::{
    LoadedSkill, SkillLoadError, SkillLoader, SkillManifest,
};
pub use rating::{RatingSummary, SkillRating, SkillRatingStore};
pub use lockfile::{LockEntry, SkillLockfile};
pub use registry::{RegisteredSkill, SkillDefinition, SkillRegistry, Version, VersionError};
pub use skill_prompt::build_skills_prompt;
