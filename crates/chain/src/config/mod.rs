//! Chain Configuration Module
//!
//! Provides chain configuration management with validation and hot-reloading
//! support.

mod config;
pub mod reloadable;

pub use config::ChainConfig;
pub use reloadable::{ConfigReloader, ConfigSnapshot, VersionedConfig};
