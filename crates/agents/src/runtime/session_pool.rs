//! Session Pool Implementation
//!
//! Implements an optimized session pool for efficient agent scheduling and
//! large-scale deployment support. Unlike creating new sessions for each task,
//! this maintains a pool of pre-initialized or hibernating agent sessions.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Session Pool Manager                         │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                                                                  │
//! │   ┌─────────────────────────────────────────────────────────┐  │
//! │   │                    Session Pool                          │  │
//! │   │  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐        │  │
//!   │   │  │ Active  │ │ Active  │ │Hibernat-│ │Hibernat-│  ...   │  │
//! │   │  │Session 1│ │Session 2│ │ ing 3   │ │ ing 4   │        │  │
//! │   │  │ (LLM)   │ │ (Code)  │ │(Research│ │ (Data)  │        │  │
//! │   │  └─────────┘ └─────────┘ └─────────┘ └─────────┘        │  │
//! │   └─────────────────────────────────────────────────────────┘  │
//! │                            │                                     │
//! │                            ▼                                     │
//! │   ┌─────────────────────────────────────────────────────────┐  │
//! │   │              Intelligent Orchestrator                    │  │
//! │   │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │  │
//! │   │  │ Task Analysis│  │ Capability   │  │   Session    │  │  │
//! │   │  │   & Routing  │──│   Matching   │──│  Assignment  │  │  │
//! │   │  └──────────────┘  └──────────────┘  └──────────────┘  │  │
//! │   └─────────────────────────────────────────────────────────┘  │
//! │                                                                  │
//! │   Pool Operations:                                              │
//! │   - Pre-warming: Keep hot sessions ready                        │
//! │   - Hibernate: Save state, free resources                       │
//! │   - Wake: Restore from hibernation                              │
//! │   - Scale: Dynamic pool sizing                                  │
//! │                                                                  │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Features
//! - Pre-initialized sessions for fast task assignment
//! - Hibernate/wake mechanism for resource efficiency
//! - Capability-based session matching
//! - Dynamic pool scaling
//! - Session health monitoring
//! - Workload balancing

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio::time::{interval, timeout, Duration, Instant};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::error::Result;

/// Default session pool configuration
pub const DEFAULT_MIN_POOL_SIZE: usize = 2;
pub const DEFAULT_MAX_POOL_SIZE: usize = 50;
pub const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 300; // 5 minutes
pub const DEFAULT_HIBERNATE_TIMEOUT_SECS: u64 = 600; // 10 minutes
pub const DEFAULT_SESSION_WARMUP_BATCH_SIZE: usize = 5;
pub const DEFAULT_HEALTH_CHECK_INTERVAL_SECS: u64 = 30;

/// Session state in the pool
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PooledSessionState {
    /// Active and available for tasks
    Active,
    /// Currently processing a task
    Busy,
    /// Idle but ready
    Idle,
    /// Hibernating (state saved, resources freed)
    Hibernating,
    /// Being initialized
    Initializing,
    /// Unhealthy/error state
    Unhealthy,
    /// Being terminated
    Terminating,
}

impl std::fmt::Display for PooledSessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PooledSessionState::Active => write!(f, "active"),
            PooledSessionState::Busy => write!(f, "busy"),
            PooledSessionState::Idle => write!(f, "idle"),
            PooledSessionState::Hibernating => write!(f, "hibernating"),
            PooledSessionState::Initializing => write!(f, "initializing"),
            PooledSessionState::Unhealthy => write!(f, "unhealthy"),
            PooledSessionState::Terminating => write!(f, "terminating"),
        }
    }
}

/// Session capabilities for matching
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionCapabilities {
    /// LLM model identifier
    pub model_id: Option<String>,
    /// Supported skills/tools
    pub skills: Vec<String>,
    /// Specializations (e.g., "coding", "research", "writing")
    pub specializations: Vec<String>,
    /// Context window size
    pub context_window: usize,
    /// Supported languages
    pub languages: Vec<String>,
    /// Custom capability tags
    pub tags: Vec<String>,
}

impl SessionCapabilities {
    /// Check if this session matches required capabilities
    pub fn matches(&self, requirements: &SessionRequirements) -> f32 {
        let mut score = 0.0;
        let mut total_weight = 0.0;

        // Check model match
        if let Some(ref required_model) = requirements.model_id {
            total_weight += 1.0;
            if self.model_id.as_ref() == Some(required_model) {
                score += 1.0;
            }
        }

        // Check skills
        if !requirements.required_skills.is_empty() {
            total_weight += 1.0;
            let required: std::collections::HashSet<_> =
                requirements.required_skills.iter().collect();
            let available: std::collections::HashSet<_> = self.skills.iter().collect();
            let intersection: std::collections::HashSet<_> =
                required.intersection(&available).collect();
            if intersection.len() == required.len() {
                score += 1.0;
            } else {
                score += intersection.len() as f32 / required.len() as f32;
            }
        }

        // Check specializations
        if !requirements.specializations.is_empty() {
            total_weight += 1.0;
            for spec in &requirements.specializations {
                if self.specializations.contains(spec) {
                    score += 1.0 / requirements.specializations.len() as f32;
                }
            }
        }

        // Check tags
        if !requirements.tags.is_empty() {
            total_weight += 1.0;
            let required: std::collections::HashSet<_> = requirements.tags.iter().collect();
            let available: std::collections::HashSet<_> = self.tags.iter().collect();
            let intersection: std::collections::HashSet<_> =
                required.intersection(&available).collect();
            score += intersection.len() as f32 / required.len() as f32;
        }

        if total_weight > 0.0 {
            score / total_weight
        } else {
            1.0 // No requirements means full match
        }
    }
}

/// Session requirements for task assignment
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionRequirements {
    pub model_id: Option<String>,
    pub required_skills: Vec<String>,
    pub specializations: Vec<String>,
    pub min_context_window: usize,
    pub tags: Vec<String>,
    /// Priority level (higher = more important)
    pub priority: u8,
}

/// Session metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionMetrics {
    /// Total tasks processed
    pub tasks_processed: u64,
    /// Total tokens processed
    pub tokens_processed: u64,
    /// Average response time (ms)
    pub avg_response_time_ms: f64,
    /// Error count
    pub error_count: u64,
    /// Last activity timestamp
    pub last_activity: Option<chrono::DateTime<chrono::Utc>>,
    /// Time spent in current state
    pub current_state_duration_secs: u64,
}

/// A pooled session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PooledSession {
    /// Session ID
    pub id: Uuid,
    /// Session state
    pub state: PooledSessionState,
    /// Session capabilities
    pub capabilities: SessionCapabilities,
    /// Session metrics
    pub metrics: SessionMetrics,
    /// Created at
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last state change
    pub state_changed_at: chrono::DateTime<chrono::Utc>,
    /// Hibernation state (serialized)
    pub hibernation_state: Option<String>,
    /// Current task ID (if busy)
    pub current_task_id: Option<Uuid>,
    /// Warm priority (higher = keep in pool longer)
    pub warmup_priority: u8,
}

impl PooledSession {
    /// Create new pooled session
    pub fn new(capabilities: SessionCapabilities) -> Self {
        let now = chrono::Utc::now();

        Self {
            id: Uuid::new_v4(),
            state: PooledSessionState::Initializing,
            capabilities,
            metrics: SessionMetrics::default(),
            created_at: now,
            state_changed_at: now,
            hibernation_state: None,
            current_task_id: None,
            warmup_priority: 0,
        }
    }

    /// Transition to new state
    pub fn transition_to(&mut self, new_state: PooledSessionState) {
        let now = chrono::Utc::now();
        self.metrics.current_state_duration_secs =
            (now - self.state_changed_at).num_seconds() as u64;
        self.state = new_state;
        self.state_changed_at = now;
    }

    /// Check if session is available for assignment
    pub fn is_available(&self) -> bool {
        matches!(
            self.state,
            PooledSessionState::Active | PooledSessionState::Idle
        )
    }

    /// Check if session needs hibernation
    pub fn should_hibernate(&self, idle_timeout: Duration) -> bool {
        if self.state != PooledSessionState::Idle {
            return false;
        }

        let elapsed = Instant::now()
            - Instant::now()
            - Duration::from_secs(self.metrics.current_state_duration_secs);
        elapsed > idle_timeout
    }
}

/// Session pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionPoolConfig {
    /// Minimum pool size (always keep warm)
    pub min_pool_size: usize,
    /// Maximum pool size
    pub max_pool_size: usize,
    /// Idle timeout before considering hibernation
    pub idle_timeout_secs: u64,
    /// Hibernation timeout (how long to keep hibernating before termination)
    pub hibernate_timeout_secs: u64,
    /// Batch size for session warmup
    pub warmup_batch_size: usize,
    /// Health check interval
    pub health_check_interval_secs: u64,
    /// Enable auto-scaling
    pub enable_auto_scaling: bool,
    /// Target utilization ratio for scaling
    pub target_utilization: f32,
    /// ARCHITECTURE FIX: Enable LRU eviction when max_pool_size is reached
    pub enable_lru_eviction: bool,
    /// ARCHITECTURE FIX: Evict sessions idle longer than this (seconds)
    pub lru_eviction_threshold_secs: u64,
}

impl Default for SessionPoolConfig {
    fn default() -> Self {
        Self {
            min_pool_size: DEFAULT_MIN_POOL_SIZE,
            max_pool_size: DEFAULT_MAX_POOL_SIZE,
            idle_timeout_secs: DEFAULT_IDLE_TIMEOUT_SECS,
            hibernate_timeout_secs: DEFAULT_HIBERNATE_TIMEOUT_SECS,
            warmup_batch_size: DEFAULT_SESSION_WARMUP_BATCH_SIZE,
            health_check_interval_secs: DEFAULT_HEALTH_CHECK_INTERVAL_SECS,
            enable_auto_scaling: true,
            target_utilization: 0.7,
            // ARCHITECTURE FIX: LRU eviction enabled by default
            enable_lru_eviction: true,
            lru_eviction_threshold_secs: 300, // 5 minutes
        }
    }
}

/// Task assignment
#[derive(Debug, Clone)]
pub struct TaskAssignment {
    pub task_id: Uuid,
    pub session_id: Uuid,
    pub assigned_at: chrono::DateTime<chrono::Utc>,
}

/// Session pool statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionPoolStats {
    pub total_sessions: usize,
    pub active_sessions: usize,
    pub idle_sessions: usize,
    pub busy_sessions: usize,
    pub hibernating_sessions: usize,
    pub unhealthy_sessions: usize,
    pub total_tasks_processed: u64,
    pub avg_wait_time_ms: f64,
    pub pool_utilization: f32,
}

/// Session pool manager
pub struct SessionPool {
    config: SessionPoolConfig,
    /// All sessions
    sessions: Arc<RwLock<HashMap<Uuid, PooledSession>>>,
    /// Available sessions (for quick lookup)
    available_queue: Arc<Mutex<VecDeque<Uuid>>>,
    /// Active assignments
    assignments: Arc<RwLock<HashMap<Uuid, TaskAssignment>>>,
    /// Assignment request queue
    assignment_queue: Arc<Mutex<VecDeque<AssignmentRequest>>>,
    /// Shutdown signal
    shutdown_tx: mpsc::Sender<()>,
    /// Shutdown receiver (kept to prevent sender drop)
    _shutdown_rx: Arc<Mutex<mpsc::Receiver<()>>>,
    /// Pool metrics
    #[allow(dead_code)]
    stats: Arc<RwLock<SessionPoolStats>>,
}

/// Assignment request
#[derive(Debug)]
struct AssignmentRequest {
    task_id: Uuid,
    requirements: SessionRequirements,
    response_tx: tokio::sync::oneshot::Sender<Option<Uuid>>,
}

impl SessionPool {
    /// Create new session pool
    pub fn new(config: SessionPoolConfig) -> Self {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

        let pool = Self {
            config,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            available_queue: Arc::new(Mutex::new(VecDeque::new())),
            assignments: Arc::new(RwLock::new(HashMap::new())),
            assignment_queue: Arc::new(Mutex::new(VecDeque::new())),
            shutdown_tx,
            _shutdown_rx: Arc::new(Mutex::new(shutdown_rx)),
            stats: Arc::new(RwLock::new(SessionPoolStats::default())),
        };

        pool
    }

    /// Create with default configuration
    pub fn default() -> Self {
        Self::new(SessionPoolConfig::default())
    }

    /// Start the session pool
    pub async fn start(&self) -> Result<()> {
        info!("Starting session pool with config: {:?}", self.config);

        // Initialize minimum pool size
        self.warmup_sessions(self.config.min_pool_size).await?;

        // Start background tasks
        self.start_maintenance_task();
        self.start_assignment_processor();
        self.start_health_monitor();

        info!("Session pool started");
        Ok(())
    }

    /// Stop the session pool
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping session pool");

        // Signal shutdown
        let _ = self.shutdown_tx.send(()).await;

        // Terminate all sessions
        let mut sessions = self.sessions.write().await;
        for (_, session) in sessions.iter_mut() {
            session.transition_to(PooledSessionState::Terminating);
        }
        sessions.clear();

        info!("Session pool stopped");
        Ok(())
    }

    /// Warm up sessions to minimum pool size
    async fn warmup_sessions(&self, count: usize) -> Result<()> {
        let current_count = self.sessions.read().await.len();
        let to_create = count.saturating_sub(current_count);

        for _ in 0..to_create {
            self.create_session(SessionCapabilities::default()).await?;
        }

        info!("Warmed up {} sessions", to_create);
        Ok(())
    }

    /// Create a new session in the pool
    ///
    /// ARCHITECTURE FIX: Enforces max_pool_size with LRU eviction.
    ///
    /// QUALITY FIX: Fixed race condition by holding both locks simultaneously
    /// during LRU eviction to maintain consistency between sessions and
    /// available_queue.
    pub async fn create_session(&self, capabilities: SessionCapabilities) -> Result<Uuid> {
        let session = PooledSession::new(capabilities);
        let id = session.id;

        // Initialize session (would call actual session initialization in real impl)
        let mut session = session;
        session.transition_to(PooledSessionState::Idle);

        // QUALITY FIX: Check capacity and evict LRU if needed
        // Hold both locks simultaneously to prevent race conditions
        {
            let mut sessions = self.sessions.write().await;

            // Check if we're at capacity
            if sessions.len() >= self.config.max_pool_size {
                if self.config.enable_lru_eviction {
                    // Find LRU candidate (idle session with oldest last activity)
                    let lru_candidate = sessions
                        .values()
                        .filter(|s| s.state == PooledSessionState::Idle)
                        .min_by_key(|s| s.metrics.last_activity.unwrap_or(s.created_at));

                    if let Some(candidate) = lru_candidate {
                        let lru_id = candidate.id;
                        let idle_duration = chrono::Utc::now().signed_duration_since(
                            candidate
                                .metrics
                                .last_activity
                                .unwrap_or(candidate.created_at),
                        );

                        // Only evict if idle longer than threshold
                        if idle_duration.num_seconds()
                            > self.config.lru_eviction_threshold_secs as i64
                        {
                            info!(
                                "Evicting LRU session {} (idle for {}s)",
                                lru_id,
                                idle_duration.num_seconds()
                            );

                            // QUALITY FIX: Hold both locks during eviction to maintain consistency
                            let mut queue = self.available_queue.lock().await;
                            sessions.remove(&lru_id);
                            queue.retain(|&sid| sid != lru_id);
                            // Both locks released here
                        } else {
                            return Err(crate::error::AgentError::ResourceLimit(format!(
                                "Max pool size ({}) reached and no idle sessions available for \
                                 eviction",
                                self.config.max_pool_size
                            )));
                        }
                    } else {
                        return Err(crate::error::AgentError::ResourceLimit(format!(
                            "Max pool size ({}) reached with no idle sessions available",
                            self.config.max_pool_size
                        )));
                    }
                } else {
                    return Err(crate::error::AgentError::ResourceLimit(format!(
                        "Max pool size ({}) reached",
                        self.config.max_pool_size
                    )));
                }
            }
            // QUALITY FIX: Session stays locked, insert while still holding the lock
            sessions.insert(id, session);
        } // sessions lock released here

        // QUALITY FIX: Add to available queue separately since we need the write lock
        // At this point the session is already in sessions, so we just need to update
        // the queue
        {
            let mut queue = self.available_queue.lock().await;
            queue.push_back(id);
        }

        debug!("Created new session: {}", id);
        Ok(id)
    }

    /// Acquire a session for a task
    pub async fn acquire_session(
        &self,
        task_id: Uuid,
        requirements: SessionRequirements,
    ) -> Result<Option<Uuid>> {
        // Try to find matching session immediately
        if let Some(session_id) = self.find_matching_session(&requirements).await {
            self.assign_session(session_id, task_id).await?;
            return Ok(Some(session_id));
        }

        // Queue request if no session available
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut queue = self.assignment_queue.lock().await;
            queue.push_back(AssignmentRequest {
                task_id,
                requirements,
                response_tx: tx,
            });
        }

        // Wait for assignment with timeout
        match timeout(Duration::from_secs(30), rx).await {
            Ok(Ok(session_id)) => Ok(session_id),
            _ => Ok(None),
        }
    }

    /// Find a matching session
    async fn find_matching_session(&self, requirements: &SessionRequirements) -> Option<Uuid> {
        let sessions = self.sessions.read().await;
        let available = self.available_queue.lock().await.clone();

        let mut best_match: Option<(Uuid, f32)> = None;

        for id in available {
            if let Some(session) = sessions.get(&id) {
                if session.is_available() {
                    let score = session.capabilities.matches(requirements);
                    if score > 0.0 {
                        match best_match {
                            None => best_match = Some((id, score)),
                            Some((_, current_score)) if score > current_score => {
                                best_match = Some((id, score));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        best_match.map(|(id, _)| id)
    }

    /// Assign a session to a task
    async fn assign_session(&self, session_id: Uuid, task_id: Uuid) -> Result<()> {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get_mut(&session_id) {
            session.transition_to(PooledSessionState::Busy);
            session.current_task_id = Some(task_id);
            session.metrics.last_activity = Some(chrono::Utc::now());
        }

        // Record assignment
        let assignment = TaskAssignment {
            task_id,
            session_id,
            assigned_at: chrono::Utc::now(),
        };

        {
            let mut assignments = self.assignments.write().await;
            assignments.insert(task_id, assignment);
        }

        // Remove from available queue
        {
            let mut queue = self.available_queue.lock().await;
            queue.retain(|&id| id != session_id);
        }

        debug!("Assigned session {} to task {}", session_id, task_id);
        Ok(())
    }

    /// Release a session after task completion
    ///
    /// ARCHITECTURE FIX: Updates last_activity for LRU tracking.
    pub async fn release_session(&self, session_id: Uuid, task_id: Uuid) -> Result<()> {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get_mut(&session_id) {
            session.transition_to(PooledSessionState::Idle);
            session.current_task_id = None;
            session.metrics.tasks_processed += 1;
            // ARCHITECTURE FIX: Update last activity for LRU tracking
            session.metrics.last_activity = Some(chrono::Utc::now());
        }

        // Remove assignment
        {
            let mut assignments = self.assignments.write().await;
            assignments.remove(&task_id);
        }

        // Add back to available queue
        {
            let mut queue = self.available_queue.lock().await;
            queue.push_back(session_id);
        }

        debug!("Released session {} from task {}", session_id, task_id);
        Ok(())
    }

    /// Hibernate a session to free resources
    pub async fn hibernate_session(&self, session_id: Uuid) -> Result<()> {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get_mut(&session_id) {
            if session.state == PooledSessionState::Idle {
                // Serialize session state
                session.hibernation_state = Some(serde_json::to_string(session)?);
                session.transition_to(PooledSessionState::Hibernating);

                // Remove from available queue
                drop(sessions);
                {
                    let mut queue = self.available_queue.lock().await;
                    queue.retain(|&id| id != session_id);
                }

                info!("Hibernated session {}", session_id);
            }
        }

        Ok(())
    }

    /// Wake a hibernating session
    pub async fn wake_session(&self, session_id: Uuid) -> Result<()> {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get_mut(&session_id) {
            if session.state == PooledSessionState::Hibernating {
                // Restore session state
                session.hibernation_state = None;
                session.transition_to(PooledSessionState::Idle);

                // Add to available queue
                drop(sessions);
                {
                    let mut queue = self.available_queue.lock().await;
                    queue.push_back(session_id);
                }

                info!("Woke session {}", session_id);
            }
        }

        Ok(())
    }

    /// Terminate a session
    pub async fn terminate_session(&self, session_id: Uuid) -> Result<()> {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get_mut(&session_id) {
            session.transition_to(PooledSessionState::Terminating);
        }

        sessions.remove(&session_id);

        // Remove from available queue
        {
            let mut queue = self.available_queue.lock().await;
            queue.retain(|&id| id != session_id);
        }

        info!("Terminated session {}", session_id);
        Ok(())
    }

    /// Get pool statistics
    pub async fn get_stats(&self) -> SessionPoolStats {
        let sessions = self.sessions.read().await;
        let _assignments = self.assignments.read().await;

        let mut stats = SessionPoolStats {
            total_sessions: sessions.len(),
            total_tasks_processed: 0,
            ..Default::default()
        };

        for (_, session) in sessions.iter() {
            match session.state {
                PooledSessionState::Active => stats.active_sessions += 1,
                PooledSessionState::Idle => stats.idle_sessions += 1,
                PooledSessionState::Busy => stats.busy_sessions += 1,
                PooledSessionState::Hibernating => stats.hibernating_sessions += 1,
                PooledSessionState::Unhealthy => stats.unhealthy_sessions += 1,
                _ => {}
            }
            stats.total_tasks_processed += session.metrics.tasks_processed;
        }

        stats.pool_utilization = if stats.total_sessions > 0 {
            stats.busy_sessions as f32 / stats.total_sessions as f32
        } else {
            0.0
        };

        stats
    }

    /// Start maintenance background task
    ///
    /// ARCHITECTURE FIX: Includes LRU eviction for sessions beyond
    /// max_pool_size.
    fn start_maintenance_task(&self) {
        let sessions = self.sessions.clone();
        let available_queue = self.available_queue.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(10));

            loop {
                interval.tick().await;

                let _idle_timeout = Duration::from_secs(config.idle_timeout_secs);
                let _hibernate_timeout = Duration::from_secs(config.hibernate_timeout_secs);

                let now = chrono::Utc::now();

                // Check for sessions to hibernate
                let mut to_hibernate = Vec::new();
                let mut to_terminate = Vec::new();

                {
                    let sessions_guard = sessions.read().await;
                    for (id, session) in sessions_guard.iter() {
                        let idle_duration = now - session.state_changed_at;

                        if session.state == PooledSessionState::Idle
                            && idle_duration.num_seconds() > config.idle_timeout_secs as i64
                        {
                            to_hibernate.push(*id);
                        }

                        if session.state == PooledSessionState::Hibernating
                            && idle_duration.num_seconds() > config.hibernate_timeout_secs as i64
                        {
                            to_terminate.push(*id);
                        }
                    }
                }

                // Hibernate idle sessions
                for id in to_hibernate {
                    let mut sessions_guard = sessions.write().await;
                    if let Some(session) = sessions_guard.get_mut(&id) {
                        session.hibernation_state =
                            Some(serde_json::to_string(session).unwrap_or_default());
                        session.transition_to(PooledSessionState::Hibernating);
                    }

                    // Remove from available queue
                    let mut queue = available_queue.lock().await;
                    queue.retain(|&session_id| session_id != id);
                }

                // Terminate old hibernating sessions
                for id in to_terminate {
                    let mut sessions_guard = sessions.write().await;
                    sessions_guard.remove(&id);
                }

                // ARCHITECTURE FIX: LRU eviction - remove excess idle sessions beyond
                // max_pool_size
                if config.enable_lru_eviction {
                    let sessions_guard = sessions.write().await;
                    let current_count = sessions_guard.len();

                    if current_count > config.max_pool_size {
                        let excess = current_count - config.max_pool_size;

                        // Find LRU idle sessions to evict
                        let mut lru_candidates: Vec<_> = sessions_guard
                            .values()
                            .filter(|s| s.state == PooledSessionState::Idle)
                            .map(|s| (s.id, s.metrics.last_activity.unwrap_or(s.created_at)))
                            .collect();

                        // Sort by last activity (oldest first)
                        lru_candidates.sort_by_key(|&(_, activity)| activity);

                        // Evict excess sessions
                        let to_evict: Vec<_> = lru_candidates
                            .into_iter()
                            .take(excess)
                            .filter(|(_, activity)| {
                                let idle_duration = now.signed_duration_since(*activity);
                                idle_duration.num_seconds()
                                    > config.lru_eviction_threshold_secs as i64
                            })
                            .map(|(id, _)| id)
                            .collect();

                        drop(sessions_guard);

                        for id in to_evict {
                            info!("LRU eviction: removing idle session {}", id);
                            let mut sessions_guard = sessions.write().await;
                            sessions_guard.remove(&id);
                            drop(sessions_guard);

                            // Remove from available queue
                            let mut queue = available_queue.lock().await;
                            queue.retain(|&session_id| session_id != id);
                        }
                    }
                }
            }
        });
    }

    /// Start assignment processor
    fn start_assignment_processor(&self) {
        let sessions = self.sessions.clone();
        let available_queue = self.available_queue.clone();
        let assignment_queue = self.assignment_queue.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(100));

            loop {
                interval.tick().await;

                while let Some(request) = {
                    let mut queue = assignment_queue.lock().await;
                    queue.pop_front()
                } {
                    // Try to find matching session
                    let sessions_guard = sessions.read().await;
                    let available = available_queue.lock().await.clone();

                    let mut best_match: Option<(Uuid, f32)> = None;

                    for id in available {
                        if let Some(session) = sessions_guard.get(&id) {
                            if session.is_available() {
                                let score = session.capabilities.matches(&request.requirements);
                                if score > 0.0 {
                                    match best_match {
                                        None => best_match = Some((id, score)),
                                        Some((_, current_score)) if score > current_score => {
                                            best_match = Some((id, score));
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }

                    drop(sessions_guard);

                    if let Some((session_id, _)) = best_match {
                        // Assign session
                        let mut sessions_guard = sessions.write().await;
                        if let Some(session) = sessions_guard.get_mut(&session_id) {
                            session.transition_to(PooledSessionState::Busy);
                            session.current_task_id = Some(request.task_id);
                            session.metrics.last_activity = Some(chrono::Utc::now());
                        }

                        // Remove from available queue
                        let mut queue = available_queue.lock().await;
                        queue.retain(|&id| id != session_id);

                        // Send response
                        let _ = request.response_tx.send(Some(session_id));
                    } else {
                        // Re-queue if no session available
                        let mut queue = assignment_queue.lock().await;
                        queue.push_back(request);
                    }
                }
            }
        });
    }

    /// Start health monitor
    fn start_health_monitor(&self) {
        let sessions = self.sessions.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(config.health_check_interval_secs));

            loop {
                interval.tick().await;

                let mut sessions_guard = sessions.write().await;
                let now = chrono::Utc::now();

                for (_, session) in sessions_guard.iter_mut() {
                    // Check if session has been busy too long (potential deadlock)
                    if session.state == PooledSessionState::Busy {
                        let busy_duration = now - session.state_changed_at;
                        if busy_duration.num_minutes() > 30 {
                            warn!(
                                "Session {} has been busy for {} minutes, marking unhealthy",
                                session.id,
                                busy_duration.num_minutes()
                            );
                            session.transition_to(PooledSessionState::Unhealthy);
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

    #[test]
    fn test_session_capabilities_matches() {
        let caps = SessionCapabilities {
            model_id: Some("gpt-4".to_string()),
            skills: vec!["coding".to_string(), "writing".to_string()],
            specializations: vec!["rust".to_string()],
            tags: vec!["fast".to_string()],
            ..Default::default()
        };

        // Exact match
        let req1 = SessionRequirements {
            model_id: Some("gpt-4".to_string()),
            required_skills: vec!["coding".to_string()],
            specializations: vec!["rust".to_string()],
            tags: vec!["fast".to_string()],
            ..Default::default()
        };
        assert!(caps.matches(&req1) > 0.9);

        // Partial match
        let req2 = SessionRequirements {
            model_id: Some("gpt-3".to_string()),
            required_skills: vec!["coding".to_string()],
            ..Default::default()
        };
        assert!(caps.matches(&req2) > 0.0);
        assert!(caps.matches(&req2) < 1.0);
    }

    #[test]
    fn test_pooled_session_state_display() {
        assert_eq!(format!("{}", PooledSessionState::Active), "active");
        assert_eq!(
            format!("{}", PooledSessionState::Hibernating),
            "hibernating"
        );
    }

    #[test]
    fn test_session_pool_config_default() {
        let config = SessionPoolConfig::default();
        assert_eq!(config.min_pool_size, 2);
        assert_eq!(config.max_pool_size, 50);
        assert!(config.enable_auto_scaling);
    }

    #[tokio::test]
    async fn test_session_pool_lifecycle() {
        let config = SessionPoolConfig {
            min_pool_size: 1,
            max_pool_size: 5,
            ..Default::default()
        };

        let pool = SessionPool::new(config);
        pool.start().await.unwrap();

        // Create a session
        let caps = SessionCapabilities {
            model_id: Some("test-model".to_string()),
            skills: vec!["test".to_string()],
            ..Default::default()
        };
        let session_id = pool.create_session(caps).await.unwrap();

        // Acquire session
        let task_id = Uuid::new_v4();
        let requirements = SessionRequirements {
            model_id: Some("test-model".to_string()),
            ..Default::default()
        };
        let acquired = pool.acquire_session(task_id, requirements).await.unwrap();
        assert!(acquired.is_some());

        // Release session
        pool.release_session(session_id, task_id).await.unwrap();

        // Get stats
        let stats = pool.get_stats().await;
        assert!(stats.total_sessions > 0);

        pool.stop().await.unwrap();
    }

    #[test]
    fn test_pooled_session_transitions() {
        let mut session = PooledSession::new(SessionCapabilities::default());
        assert_eq!(session.state, PooledSessionState::Initializing);

        session.transition_to(PooledSessionState::Idle);
        assert_eq!(session.state, PooledSessionState::Idle);
        assert!(session.is_available());

        session.transition_to(PooledSessionState::Busy);
        assert_eq!(session.state, PooledSessionState::Busy);
        assert!(!session.is_available());
    }
}
