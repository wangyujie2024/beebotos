//! BeeBotOS Core Types and Utilities
//!
//! This crate provides shared types, error definitions, and utilities
//! used across all BeeBotOS layers.

#![warn(missing_docs)]

pub mod config;
pub mod error;
pub mod event;
pub mod event_bus;
pub mod message_bus;
pub mod types;

// 🟢 P1 FIX: Export unified configuration management
pub use config::{Config, ConfigCenter, Environment};
// 🟢 P1 FIX: New unified error types
pub use error::{BeeBotOSError, ErrorBuilder, ErrorCode, ErrorContext, Result, Severity};
// 🟢 P1 FIX: Export unified error types (legacy aliases for backward compatibility)
pub use error::{BeeBotOSError as Error, Result as LegacyResult};
// Note: bail and err macros are #[macro_export] and available at crate root automatically
pub use event::{Event, EventBus};
pub use event_bus::{
    EventBusError, EventBusHandle, SystemEvent, SystemEventBus, TypedEventReceiver,
};
pub use message_bus::{init_message_bus, message_bus, CoreMessageBus};
pub use types::*;

/// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// AgentOS identifier
pub const AGENT_OS_ID: &str = "beebotos";
