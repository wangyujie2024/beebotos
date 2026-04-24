//! Session Management Module
//!
//! Implements OpenClaw-style session isolation:
//! - SessionKey format: agent:<agentId>:session:<uuid>
//! - Subagent format: agent:<agentId>:subagent:<uuid>
//! - Cron format: agent:<agentId>:cron:<uuid>
//! - Webhook format: agent:<agentId>:webhook:<uuid>

pub mod context;
pub mod key;
pub mod session_persistence;
pub mod transcript;
pub mod unified_session;
pub mod websocket;
pub mod workspace;

pub use context::SessionContext;
pub use key::{SessionKey, SessionKeyError, SessionType};
pub use session_persistence::InMemorySessionPersistence;
#[cfg(feature = "sqlite")]
pub use session_persistence::SqliteSessionPersistence;
pub use transcript::Transcript;
// Unified session management (consolidated from former manager.rs and persistence.rs)
pub use unified_session::{
    ManagedSession, SessionManager, SessionManagerConfig, SessionMetadata, SessionPersistence,
    SessionState, UnifiedSession, UnifiedSessionManager, UnifiedSessionManagerConfig,
    UnifiedSessionState,
};
pub use websocket::{
    ConnectionState, WebSocketSession, WebSocketSessionManager, WsProtocolMessage,
    WsSessionManagerConfig,
};
pub use workspace::Workspace;

pub use crate::security::session_isolation::{IsolationConfig, IsolationLevel};
