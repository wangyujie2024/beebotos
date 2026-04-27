//! Unified Session Management
//!
//! Integrates SessionPool (from runtime) with SessionManager to provide
//! a unified session management interface with persistence support.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                Unified Session Manager                          │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                                                                  │
//! │  ┌─────────────────┐      ┌─────────────────┐                  │
//! │  │   SessionPool   │◄────►│  SessionManager │                  │
//! │  │   (runtime)     │      │   (session)     │                  │
//! │  │                 │      │                 │                  │
//! │  │ - Pre-warmed    │      │ - WebSocket     │                  │
//! │  │ - Capability    │      │ - Lifecycle     │                  │
//! │  │   matching      │      │ - Metadata      │                  │
//! │  └────────┬────────┘      └────────┬────────┘                  │
//! │           │                        │                           │
//! │           └────────┬───────────────┘                           │
//! │                    ▼                                            │
//! │           ┌─────────────────┐                                  │
//! │           │ UnifiedSession  │  ← Single interface              │
//! │           │    Manager      │                                  │
//! │           └────────┬────────┘                                  │
//! │                    │                                            │
//! │           ┌────────┴────────┐                                  │
//! │           ▼                 ▼                                  │
//! │  ┌─────────────────┐  ┌─────────────────┐                     │
//! │  │   Persistence   │  │   Persistence   │                     │
//! │  │   (Memory)      │  │  (SQLite)       │                     │
//! │  └─────────────────┘  └─────────────────┘                     │
//! │                                                                  │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::error::{AgentError, Result};
use crate::runtime::session_pool::{
    PooledSession, PooledSessionState, SessionCapabilities, SessionPool, SessionPoolConfig,
    SessionPoolStats, SessionRequirements,
};
use crate::session::key::{SessionKey, SessionType};
use crate::session::websocket::{WebSocketSessionManager, WsSessionManagerConfig};
use crate::session::SessionContext;

// =============================================================================
// Session State (from former manager.rs)
// =============================================================================

/// Session state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    /// Session is being created
    Creating,
    /// Session is active
    Active,
    /// Session is paused (e.g., user away)
    Paused,
    /// Session is being terminated
    Terminating,
    /// Session is closed
    Closed,
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionState::Creating => write!(f, "creating"),
            SessionState::Active => write!(f, "active"),
            SessionState::Paused => write!(f, "paused"),
            SessionState::Terminating => write!(f, "terminating"),
            SessionState::Closed => write!(f, "closed"),
        }
    }
}

/// Session metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    /// Session creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last activity timestamp
    pub last_activity: chrono::DateTime<chrono::Utc>,
    /// Session expiration timestamp
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// User agent string
    pub user_agent: Option<String>,
    /// IP address
    pub ip_address: Option<String>,
    /// Platform information
    pub platform: Option<String>,
    /// Custom metadata
    pub custom: HashMap<String, String>,
}

impl Default for SessionMetadata {
    fn default() -> Self {
        let now = chrono::Utc::now();
        Self {
            created_at: now,
            last_activity: now,
            expires_at: None,
            user_agent: None,
            ip_address: None,
            platform: None,
            custom: HashMap::new(),
        }
    }
}

/// Managed session information
#[derive(Debug, Clone)]
pub struct ManagedSession {
    /// Session key
    pub key: SessionKey,
    /// Agent ID
    pub agent_id: String,
    /// Session state
    pub state: SessionState,
    /// Session context
    pub context: SessionContext,
    /// Session metadata
    pub metadata: SessionMetadata,
    /// Associated WebSocket session IDs
    pub websocket_sessions: Vec<String>,
    /// Parent session (for subagents)
    pub parent_session: Option<SessionKey>,
    /// Child sessions (subagents)
    pub child_sessions: Vec<SessionKey>,
}

impl ManagedSession {
    /// Create a new managed session
    pub fn new(key: SessionKey, context: SessionContext, agent_id: impl Into<String>) -> Self {
        Self {
            key,
            agent_id: agent_id.into(),
            state: SessionState::Creating,
            context,
            metadata: SessionMetadata::default(),
            websocket_sessions: Vec::new(),
            parent_session: None,
            child_sessions: Vec::new(),
        }
    }

    /// Check if session is active
    pub fn is_active(&self) -> bool {
        matches!(self.state, SessionState::Active | SessionState::Paused)
    }

    /// Update last activity timestamp
    pub fn touch(&mut self) {
        self.metadata.last_activity = chrono::Utc::now();
    }

    /// Check if session has expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.metadata.expires_at {
            chrono::Utc::now() > expires_at
        } else {
            false
        }
    }

    /// Associate a WebSocket session
    pub fn associate_websocket(&mut self, ws_session_id: String) {
        if !self.websocket_sessions.contains(&ws_session_id) {
            self.websocket_sessions.push(ws_session_id);
        }
    }

    /// Remove WebSocket session association
    pub fn remove_websocket(&mut self, ws_session_id: &str) {
        self.websocket_sessions.retain(|id| id != ws_session_id);
    }
}

/// Unified session that combines pool and managed session
#[derive(Debug, Clone)]
pub struct UnifiedSession {
    /// Pooled session ID
    pub pool_session_id: Uuid,
    /// Managed session key
    pub session_key: SessionKey,
    /// Agent ID
    pub agent_id: String,
    /// Session capabilities
    pub capabilities: SessionCapabilities,
    /// Session state
    pub state: UnifiedSessionState,
    /// Session context
    pub context: SessionContext,
    /// Session metadata
    pub metadata: SessionMetadata,
    /// Parent session (for subagents)
    pub parent_session: Option<SessionKey>,
    /// Child sessions
    pub child_sessions: Vec<SessionKey>,
}

/// Unified session state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnifiedSessionState {
    Initializing,
    Active,
    Busy,
    Idle,
    Paused,
    Hibernating,
    Unhealthy,
    Terminating,
    Closed,
}

impl From<PooledSessionState> for UnifiedSessionState {
    fn from(state: PooledSessionState) -> Self {
        match state {
            PooledSessionState::Active => UnifiedSessionState::Active,
            PooledSessionState::Busy => UnifiedSessionState::Busy,
            PooledSessionState::Idle => UnifiedSessionState::Idle,
            PooledSessionState::Hibernating => UnifiedSessionState::Hibernating,
            PooledSessionState::Initializing => UnifiedSessionState::Initializing,
            PooledSessionState::Unhealthy => UnifiedSessionState::Unhealthy,
            PooledSessionState::Terminating => UnifiedSessionState::Terminating,
        }
    }
}

impl From<SessionState> for UnifiedSessionState {
    fn from(state: SessionState) -> Self {
        match state {
            SessionState::Creating => UnifiedSessionState::Initializing,
            SessionState::Active => UnifiedSessionState::Active,
            SessionState::Paused => UnifiedSessionState::Paused,
            SessionState::Terminating => UnifiedSessionState::Terminating,
            SessionState::Closed => UnifiedSessionState::Closed,
        }
    }
}

// =============================================================================
// Session Manager (from former manager.rs)
// =============================================================================

/// Session manager configuration
#[derive(Debug, Clone)]
pub struct SessionManagerConfig {
    /// Enable WebSocket support
    pub enable_websocket: bool,
    /// WebSocket configuration
    pub websocket_config: Option<WsSessionManagerConfig>,
    /// Default session timeout (seconds)
    pub default_timeout_secs: u64,
    /// Maximum sessions per agent
    pub max_sessions_per_agent: usize,
    /// Enable session persistence
    pub enable_persistence: bool,
}

impl Default for SessionManagerConfig {
    fn default() -> Self {
        Self {
            enable_websocket: true,
            websocket_config: Some(WsSessionManagerConfig::default()),
            default_timeout_secs: 3600, // 1 hour
            max_sessions_per_agent: 100,
            enable_persistence: false,
        }
    }
}

/// Session manager
///
/// Central manager for all agent sessions, integrating WebSocket support
/// and providing session lifecycle management.
pub struct SessionManager {
    config: SessionManagerConfig,
    /// Active sessions by session key
    sessions: Arc<RwLock<HashMap<String, ManagedSession>>>,
    /// Sessions by agent ID
    agent_sessions: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// WebSocket session manager (optional)
    websocket_manager: Option<Arc<RwLock<WebSocketSessionManager>>>,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new(config: SessionManagerConfig) -> Self {
        let websocket_manager = if config.enable_websocket {
            config
                .websocket_config
                .clone()
                .map(|ws_config| Arc::new(RwLock::new(WebSocketSessionManager::new(ws_config))))
        } else {
            None
        };

        Self {
            config,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            agent_sessions: Arc::new(RwLock::new(HashMap::new())),
            websocket_manager,
        }
    }

    /// Create with default configuration
    pub fn default() -> Self {
        Self::new(SessionManagerConfig::default())
    }

    /// Start the session manager
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting session manager");

        // Start WebSocket manager if enabled
        if let Some(ws_manager) = &self.websocket_manager {
            let mut ws = ws_manager.write().await;
            ws.start().await?;
            info!("WebSocket session manager started");
        }

        // Start session cleanup task
        self.start_cleanup_task();

        info!("Session manager started successfully");
        Ok(())
    }

    /// Stop the session manager
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping session manager");

        // Stop WebSocket manager
        if let Some(ws_manager) = &self.websocket_manager {
            let ws = ws_manager.read().await;
            ws.stop().await?;
        }

        info!("Session manager stopped");
        Ok(())
    }

    /// Create a new session
    pub async fn create_session(
        &self,
        agent_id: &str,
        session_type: SessionType,
        context: SessionContext,
    ) -> Result<SessionKey> {
        // Check session limit
        {
            let agent_sessions = self.agent_sessions.read().await;
            let count = agent_sessions.get(agent_id).map(|s| s.len()).unwrap_or(0);
            if count >= self.config.max_sessions_per_agent {
                return Err(AgentError::platform(format!(
                    "Maximum sessions reached for agent: {}",
                    agent_id
                )));
            }
        }

        // Create session key
        let session_key = SessionKey::new(agent_id, session_type);
        let key_string = session_key.to_string();

        // Create managed session
        let mut managed_session = ManagedSession::new(session_key.clone(), context, agent_id);
        managed_session.state = SessionState::Active;

        // Store session
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(key_string.clone(), managed_session);
        }

        // Register with agent
        {
            let mut agent_sessions = self.agent_sessions.write().await;
            agent_sessions
                .entry(agent_id.to_string())
                .or_insert_with(Vec::new)
                .push(key_string);
        }

        info!("Created session: {} for agent: {}", session_key, agent_id);
        Ok(session_key)
    }

    /// Get a session by key
    pub async fn get_session(&self, key: &SessionKey) -> Option<ManagedSession> {
        let sessions = self.sessions.read().await;
        sessions.get(&key.to_string()).cloned()
    }

    /// Update session state
    pub async fn update_session_state(&self, key: &SessionKey, state: SessionState) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(&key.to_string())
            .ok_or_else(|| AgentError::not_found(format!("Session not found: {}", key)))?;

        session.state = state;
        session.touch();

        info!("Updated session {} state to: {}", key, state);
        Ok(())
    }

    /// Get all sessions for an agent
    pub async fn get_agent_sessions(&self, agent_id: &str) -> Vec<SessionKey> {
        let agent_sessions = self.agent_sessions.read().await;
        agent_sessions
            .get(agent_id)
            .map(|keys| {
                keys.iter()
                    .filter_map(|k| SessionKey::parse(k).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get session count for an agent
    pub async fn get_agent_session_count(&self, agent_id: &str) -> usize {
        let agent_sessions = self.agent_sessions.read().await;
        agent_sessions.get(agent_id).map(|s| s.len()).unwrap_or(0)
    }

    /// Get total active session count
    pub async fn get_total_session_count(&self) -> usize {
        self.sessions.read().await.len()
    }

    /// Start session cleanup task
    fn start_cleanup_task(&self) {
        let sessions = self.sessions.clone();
        let default_timeout = std::time::Duration::from_secs(self.config.default_timeout_secs);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));

            loop {
                interval.tick().await;

                let sessions_guard = sessions.read().await;
                let expired_sessions: Vec<String> = sessions_guard
                    .iter()
                    .filter(|(_, s)| s.is_expired())
                    .map(|(k, _)| k.clone())
                    .collect();

                let inactive_sessions: Vec<String> = sessions_guard
                    .iter()
                    .filter(|(_, s)| {
                        let elapsed =
                            chrono::Utc::now().signed_duration_since(s.metadata.last_activity);
                        elapsed
                            > chrono::Duration::from_std(default_timeout)
                                .unwrap_or(chrono::Duration::seconds(3600))
                    })
                    .map(|(k, _)| k.clone())
                    .collect();

                drop(sessions_guard);

                for key in expired_sessions {
                    warn!("Removing expired session: {}", key);
                    let mut sessions = sessions.write().await;
                    sessions.remove(&key);
                }

                for key in inactive_sessions {
                    warn!("Marking inactive session: {}", key);
                    let mut sessions = sessions.write().await;
                    if let Some(session) = sessions.get_mut(&key) {
                        session.state = SessionState::Paused;
                    }
                }
            }
        });
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new(SessionManagerConfig::default())
    }
}

/// Session persistence trait
#[async_trait]
pub trait SessionPersistence: Send + Sync {
    /// Save session state
    async fn save_session(&self, session: &UnifiedSession) -> Result<()>;

    /// Load session by ID
    async fn load_session(&self, session_id: &str) -> Result<Option<UnifiedSession>>;

    /// Delete session
    async fn delete_session(&self, session_id: &str) -> Result<()>;

    /// List active sessions
    async fn list_sessions(&self) -> Result<Vec<UnifiedSession>>;

    /// Update session state
    async fn update_state(&self, session_id: &str, state: UnifiedSessionState) -> Result<()>;
}

/// Unified session manager configuration
#[derive(Clone)]
pub struct UnifiedSessionManagerConfig {
    /// Pool configuration
    pub pool_config: SessionPoolConfig,
    /// Manager configuration
    pub manager_config: SessionManagerConfig,
    /// Enable persistence
    pub enable_persistence: bool,
    /// Persistence provider
    pub persistence: Option<Arc<dyn SessionPersistence>>,
    /// Auto-save interval (seconds)
    pub auto_save_interval_secs: u64,
}

impl Default for UnifiedSessionManagerConfig {
    fn default() -> Self {
        Self {
            pool_config: SessionPoolConfig::default(),
            manager_config: SessionManagerConfig::default(),
            enable_persistence: false,
            persistence: None,
            auto_save_interval_secs: 60,
        }
    }
}

/// Unified session manager
pub struct UnifiedSessionManager {
    config: UnifiedSessionManagerConfig,
    /// Session pool
    pool: Arc<RwLock<SessionPool>>,
    /// Session manager
    manager: Arc<RwLock<SessionManager>>,
    /// Session mappings (pool_id -> session_key)
    session_mappings: Arc<RwLock<HashMap<Uuid, SessionKey>>>,
    /// Reverse mappings (session_key -> pool_id)
    reverse_mappings: Arc<RwLock<HashMap<String, Uuid>>>,
    /// Unified session cache
    session_cache: Arc<RwLock<HashMap<String, UnifiedSession>>>,
}

impl UnifiedSessionManager {
    /// Create new unified session manager
    pub async fn new(config: UnifiedSessionManagerConfig) -> Result<Self> {
        let pool = Arc::new(RwLock::new(SessionPool::new(config.pool_config.clone())));
        let manager = Arc::new(RwLock::new(SessionManager::new(
            config.manager_config.clone(),
        )));

        // Start pool and manager
        {
            let pool_guard = pool.write().await;
            pool_guard.start().await?;
        }
        {
            let mut manager_guard = manager.write().await;
            manager_guard.start().await?;
        }

        let unified = Self {
            config,
            pool,
            manager,
            session_mappings: Arc::new(RwLock::new(HashMap::new())),
            reverse_mappings: Arc::new(RwLock::new(HashMap::new())),
            session_cache: Arc::new(RwLock::new(HashMap::new())),
        };

        // Start background tasks
        unified.start_maintenance_tasks();

        info!("Unified session manager initialized");
        Ok(unified)
    }

    /// Create a new managed session
    pub async fn create_session(
        &self,
        agent_id: &str,
        session_type: SessionType,
        context: SessionContext,
        capabilities: SessionCapabilities,
    ) -> Result<UnifiedSession> {
        // 1. Create managed session via SessionManager
        let manager = self.manager.write().await;
        let session_key = manager
            .create_session(agent_id, session_type, context.clone())
            .await?;
        drop(manager);

        // 2. Create pooled session
        let pool_session = PooledSession::new(capabilities.clone());
        let pool_session_id = pool_session.id;

        {
            let _pool = self.pool.write().await;
            // Create session in pool (this is a simplified version)
            // In reality, we'd properly initialize the pooled session
        }

        // 3. Create unified session
        let unified_session = UnifiedSession {
            pool_session_id,
            session_key: session_key.clone(),
            agent_id: agent_id.to_string(),
            capabilities,
            state: UnifiedSessionState::Active,
            context,
            metadata: SessionMetadata::default(),
            parent_session: None,
            child_sessions: vec![],
        };

        // 4. Store mappings
        {
            let mut mappings = self.session_mappings.write().await;
            mappings.insert(pool_session_id, session_key.clone());
        }
        {
            let mut reverse = self.reverse_mappings.write().await;
            reverse.insert(session_key.to_string(), pool_session_id);
        }
        {
            let mut cache = self.session_cache.write().await;
            cache.insert(session_key.to_string(), unified_session.clone());
        }

        // 5. Persist if enabled
        if self.config.enable_persistence {
            if let Some(ref persistence) = self.config.persistence {
                persistence.save_session(&unified_session).await?;
            }
        }

        info!(
            "Created unified session: {} (pool: {}, key: {})",
            agent_id, pool_session_id, session_key
        );
        Ok(unified_session)
    }

    /// Acquire a session for task execution
    pub async fn acquire_session(
        &self,
        agent_id: &str,
        requirements: SessionRequirements,
    ) -> Result<Option<UnifiedSession>> {
        // 1. Try to find from pool
        let pool = self.pool.write().await;
        let task_id = Uuid::new_v4();

        let pool_session_id = match pool.acquire_session(task_id, requirements).await? {
            Some(id) => id,
            None => return Ok(None),
        };
        drop(pool);

        // 2. Get managed session key
        let session_key = {
            let mappings = self.session_mappings.read().await;
            match mappings.get(&pool_session_id) {
                Some(key) => key.clone(),
                None => {
                    // Create new managed session for this pool session
                    let context = SessionContext::new(format!("pool-{}", pool_session_id));
                    let manager = self.manager.write().await;
                    let key = manager
                        .create_session(agent_id, SessionType::Standard, context.clone())
                        .await?;

                    // Update mappings
                    drop(manager);
                    let mut mappings = self.session_mappings.write().await;
                    mappings.insert(pool_session_id, key.clone());
                    drop(mappings);

                    let mut reverse = self.reverse_mappings.write().await;
                    reverse.insert(key.to_string(), pool_session_id);

                    key
                }
            }
        };

        // 3. Get or create unified session
        let unified_session = {
            let cache = self.session_cache.read().await;
            match cache.get(&session_key.to_string()) {
                Some(session) => {
                    let mut session = session.clone();
                    session.state = UnifiedSessionState::Busy;
                    session
                }
                None => {
                    // Create new unified session
                    UnifiedSession {
                        pool_session_id,
                        session_key: session_key.clone(),
                        agent_id: agent_id.to_string(),
                        capabilities: SessionCapabilities::default(),
                        state: UnifiedSessionState::Busy,
                        context: SessionContext::new(session_key.to_string()),
                        metadata: SessionMetadata::default(),
                        parent_session: None,
                        child_sessions: vec![],
                    }
                }
            }
        };

        // 4. Update cache
        {
            let mut cache = self.session_cache.write().await;
            cache.insert(session_key.to_string(), unified_session.clone());
        }

        // 5. Update managed session state
        {
            let manager = self.manager.write().await;
            manager
                .update_session_state(&session_key, SessionState::Active)
                .await
                .ok();
        }

        debug!("Acquired session for {}: {}", agent_id, session_key);
        Ok(Some(unified_session))
    }

    /// Release a session after task completion
    pub async fn release_session(&self, session: &UnifiedSession) -> Result<()> {
        // 1. Release from pool
        let pool = self.pool.write().await;
        pool.release_session(session.pool_session_id, Uuid::new_v4())
            .await?;
        drop(pool);

        // 2. Update managed session
        let manager = self.manager.write().await;
        manager
            .update_session_state(&session.session_key, SessionState::Active)
            .await
            .ok();
        drop(manager);

        // 3. Update unified session
        {
            let mut cache = self.session_cache.write().await;
            if let Some(s) = cache.get_mut(&session.session_key.to_string()) {
                s.state = UnifiedSessionState::Idle;
            }
        }

        // 4. Persist state change
        if self.config.enable_persistence {
            if let Some(ref persistence) = self.config.persistence {
                let cache = self.session_cache.read().await;
                if let Some(s) = cache.get(&session.session_key.to_string()) {
                    persistence
                        .update_state(&s.session_key.to_string(), UnifiedSessionState::Idle)
                        .await
                        .ok();
                }
            }
        }

        debug!("Released session: {}", session.session_key);
        Ok(())
    }

    /// Terminate a session
    pub async fn terminate_session(&self, session_key: &SessionKey, reason: &str) -> Result<()> {
        // 1. Get pool session ID
        let pool_session_id = {
            let reverse = self.reverse_mappings.read().await;
            reverse.get(&session_key.to_string()).copied()
        };

        // 2. Update managed session state
        let manager = self.manager.write().await;
        manager
            .update_session_state(session_key, SessionState::Terminating)
            .await
            .ok();
        drop(manager);

        // 3. Terminate pooled session if exists
        if let Some(pool_id) = pool_session_id {
            let pool = self.pool.write().await;
            pool.terminate_session(pool_id).await.ok();
            drop(pool);
        }

        // 4. Clean up mappings
        {
            let mut mappings = self.session_mappings.write().await;
            if let Some(pool_id) = pool_session_id {
                mappings.remove(&pool_id);
            }
        }
        {
            let mut reverse = self.reverse_mappings.write().await;
            reverse.remove(&session_key.to_string());
        }
        {
            let mut cache = self.session_cache.write().await;
            cache.remove(&session_key.to_string());
        }

        // 5. Delete from persistence
        if self.config.enable_persistence {
            if let Some(ref persistence) = self.config.persistence {
                persistence
                    .delete_session(&session_key.to_string())
                    .await
                    .ok();
            }
        }

        info!("Terminated session: {} (reason: {})", session_key, reason);
        Ok(())
    }

    /// Get session by key
    pub async fn get_session(&self, session_key: &SessionKey) -> Option<UnifiedSession> {
        let cache = self.session_cache.read().await;
        cache.get(&session_key.to_string()).cloned()
    }

    /// Get pool statistics
    pub async fn get_pool_stats(&self) -> SessionPoolStats {
        let pool = self.pool.read().await;
        pool.get_stats().await
    }

    /// Get all active sessions
    pub async fn list_active_sessions(&self) -> Vec<UnifiedSession> {
        let cache = self.session_cache.read().await;
        cache
            .values()
            .filter(|s| {
                matches!(
                    s.state,
                    UnifiedSessionState::Active
                        | UnifiedSessionState::Idle
                        | UnifiedSessionState::Busy
                )
            })
            .cloned()
            .collect()
    }

    /// Hibernate a session
    pub async fn hibernate_session(&self, session_key: &SessionKey) -> Result<()> {
        // Get pool session ID
        let pool_session_id = {
            let reverse = self.reverse_mappings.read().await;
            reverse.get(&session_key.to_string()).copied()
        };

        // Hibernate in pool
        if let Some(pool_id) = pool_session_id {
            let pool = self.pool.write().await;
            pool.hibernate_session(pool_id).await?;
        }

        // Update state
        {
            let mut cache = self.session_cache.write().await;
            if let Some(s) = cache.get_mut(&session_key.to_string()) {
                s.state = UnifiedSessionState::Hibernating;
            }
        }

        // Persist
        if self.config.enable_persistence {
            if let Some(ref persistence) = self.config.persistence {
                persistence
                    .update_state(&session_key.to_string(), UnifiedSessionState::Hibernating)
                    .await
                    .ok();
            }
        }

        info!("Hibernated session: {}", session_key);
        Ok(())
    }

    /// Wake a hibernating session
    pub async fn wake_session(&self, session_key: &SessionKey) -> Result<()> {
        // Get pool session ID
        let pool_session_id = {
            let reverse = self.reverse_mappings.read().await;
            reverse.get(&session_key.to_string()).copied()
        };

        // Wake in pool
        if let Some(pool_id) = pool_session_id {
            let pool = self.pool.write().await;
            pool.wake_session(pool_id).await?;
        }

        // Update state
        {
            let mut cache = self.session_cache.write().await;
            if let Some(s) = cache.get_mut(&session_key.to_string()) {
                s.state = UnifiedSessionState::Active;
            }
        }

        info!("Woke session: {}", session_key);
        Ok(())
    }

    /// Start maintenance tasks
    fn start_maintenance_tasks(&self) {
        let cache = self.session_cache.clone();
        let persistence = self.config.persistence.clone();
        let interval_secs = self.config.auto_save_interval_secs;

        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));

            loop {
                interval.tick().await;

                // Auto-save sessions to persistence
                if let Some(ref pers) = persistence {
                    let cache_guard = cache.read().await;
                    for (_, session) in cache_guard.iter() {
                        if matches!(
                            session.state,
                            UnifiedSessionState::Active | UnifiedSessionState::Idle
                        ) {
                            pers.save_session(session).await.ok();
                        }
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_unified_session_manager() {
        let config = UnifiedSessionManagerConfig::default();
        let manager = UnifiedSessionManager::new(config).await.unwrap();

        // Create session
        let context = SessionContext::new("test-context".to_string());
        let capabilities = SessionCapabilities::default();

        let session = manager
            .create_session("test-agent", SessionType::Standard, context, capabilities)
            .await
            .unwrap();

        assert_eq!(session.agent_id, "test-agent");
        assert_eq!(session.state, UnifiedSessionState::Active);

        // Get session
        let retrieved = manager.get_session(&session.session_key).await;
        assert!(retrieved.is_some());

        // List sessions
        let sessions = manager.list_active_sessions().await;
        assert_eq!(sessions.len(), 1);
    }
}
