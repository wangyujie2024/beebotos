//! ACP 2.0 (Agent Collaboration Protocol) Implementation
//!
//! ACP 2.0 is an advanced protocol for multi-agent collaboration, extending A2A
//! with:
//! - Agent Capability Profiles (standardized capability description)
//! - Session-based collaboration lifecycle
//! - Service discovery and negotiation
//! - Fault tolerance and recovery
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    ACP 2.0 Protocol Stack                       │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
//! │  │  Capability  │  │   Session    │  │   Service    │          │
//! │  │   Profile    │  │   Manager    │  │   Registry   │          │
//! │  └──────────────┘  └──────────────┘  └──────────────┘          │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
//! │  │  Collaboration│  │    Fault     │  │   Security   │          │
//! │  │   Engine     │  │   Tolerance  │  │    Layer     │          │
//! │  └──────────────┘  └──────────────┘  └──────────────┘          │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                     A2A Transport Layer                         │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex, RwLock};
use tracing::{debug, info};
use uuid::Uuid;

use crate::a2a::message::A2AMessage;
use crate::types::AgentId;

/// ACP 2.0 Protocol version
pub const ACP20_VERSION: &str = "2.0.0";

/// Agent Capability Profile
///
/// Standardized description of an agent's capabilities, constraints, and
/// preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCapabilityProfile {
    /// Unique profile ID
    pub profile_id: String,
    /// Agent ID
    pub agent_id: AgentId,
    /// Protocol version
    pub protocol_version: String,
    /// Capabilities offered by this agent
    pub capabilities: Vec<Capability>,
    /// Resource constraints
    pub constraints: ResourceConstraints,
    /// Service endpoints
    pub endpoints: Vec<ServiceEndpoint>,
    /// Communication preferences
    pub preferences: CommunicationPreferences,
    /// Created at
    pub created_at: DateTime<Utc>,
    /// Expires at (optional)
    pub expires_at: Option<DateTime<Utc>>,
}

/// Capability definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    /// Capability ID (URI format recommended)
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Capability description
    pub description: String,
    /// Capability type
    pub capability_type: CapabilityType,
    /// Input schema (JSON Schema)
    pub input_schema: serde_json::Value,
    /// Output schema (JSON Schema)
    pub output_schema: serde_json::Value,
    /// Performance metrics
    pub performance: PerformanceMetrics,
    /// Required resources
    pub resource_requirements: ResourceRequirements,
}

/// Capability types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityType {
    /// LLM/text generation
    TextGeneration,
    /// Code execution
    CodeExecution,
    /// Data analysis
    DataAnalysis,
    /// File processing
    FileProcessing,
    /// Web search
    WebSearch,
    /// Communication/Routing
    Communication,
    /// Custom capability
    Custom(String),
}

/// Performance metrics for a capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Average latency in milliseconds
    pub avg_latency_ms: u64,
    /// Throughput (requests per minute)
    pub throughput_rpm: u64,
    /// Success rate (0.0 - 1.0)
    pub success_rate: f64,
    /// Quality score (0.0 - 1.0)
    pub quality_score: f64,
}

/// Resource requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequirements {
    /// Minimum memory in MB
    pub min_memory_mb: u64,
    /// Minimum CPU cores
    pub min_cpu_cores: u32,
    /// Required GPU memory in MB (0 if CPU only)
    pub gpu_memory_mb: u64,
    /// Network bandwidth requirements
    pub network_mbps: u64,
}

/// Resource constraints for an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceConstraints {
    /// Maximum concurrent tasks
    pub max_concurrent_tasks: u32,
    /// Maximum memory usage in MB
    pub max_memory_mb: u64,
    /// Maximum CPU usage percentage
    pub max_cpu_percent: u32,
    /// Rate limits (requests per minute)
    pub rate_limit_rpm: u32,
    /// Daily quota (if applicable)
    pub daily_quota: Option<u32>,
}

/// Service endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceEndpoint {
    /// Endpoint ID
    pub id: String,
    /// Transport type
    pub transport: TransportType,
    /// Endpoint URL
    pub url: String,
    /// Supported protocols
    pub protocols: Vec<String>,
    /// Authentication method
    pub auth_method: AuthMethod,
    /// Health check URL
    pub health_check_url: Option<String>,
    /// Region/location
    pub region: Option<String>,
    /// Current status
    pub status: EndpointStatus,
}

/// Transport types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportType {
    Http,
    Https,
    WebSocket,
    WebSocketSecure,
    Grpc,
    P2p,
}

/// Authentication methods
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    None,
    ApiKey { header: String },
    Jwt { issuer: String },
    MutualTls,
    OAuth2 { scopes: Vec<String> },
}

/// Endpoint status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointStatus {
    Online,
    Degraded,
    Offline,
    Maintenance,
}

/// Communication preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunicationPreferences {
    /// Preferred message format
    pub message_format: MessageFormat,
    /// Maximum message size in bytes
    pub max_message_size: u64,
    /// Preferred languages
    pub languages: Vec<String>,
    /// Timezone
    pub timezone: String,
    /// Retry policy
    pub retry_policy: RetryPolicy,
    /// Timeout settings
    pub timeout_settings: TimeoutSettings,
}

/// Message formats
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageFormat {
    Json,
    Protobuf,
    Xml,
    MessagePack,
}

/// Retry policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Initial backoff in milliseconds
    pub initial_backoff_ms: u64,
    /// Maximum backoff in milliseconds
    pub max_backoff_ms: u64,
    /// Backoff multiplier
    pub backoff_multiplier: f64,
    /// Retryable error codes
    pub retryable_errors: Vec<u16>,
}

/// Timeout settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutSettings {
    /// Connection timeout in seconds
    pub connection_timeout_sec: u64,
    /// Request timeout in seconds
    pub request_timeout_sec: u64,
    /// Session timeout in seconds
    pub session_timeout_sec: u64,
}

/// ACP 2.0 Session
///
/// A collaboration session between multiple agents
#[derive(Debug, Clone)]
pub struct AcpSession {
    /// Session ID
    pub session_id: String,
    /// Session type
    pub session_type: SessionType,
    /// Participating agents
    pub participants: Vec<SessionParticipant>,
    /// Session state
    pub state: SessionState,
    /// Collaboration context
    pub context: CollaborationContext,
    /// Created at
    pub created_at: DateTime<Utc>,
    /// Last activity
    pub last_activity: DateTime<Utc>,
    /// Expires at
    pub expires_at: DateTime<Utc>,
}

/// Session types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionType {
    /// One-to-one collaboration
    Direct,
    /// Group collaboration
    Group,
    /// Hierarchical (leader-followers)
    Hierarchical,
    /// Peer-to-peer (round table)
    PeerToPeer,
    /// Marketplace (buyer-seller)
    Marketplace,
}

/// Session participant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionParticipant {
    /// Agent ID
    pub agent_id: AgentId,
    /// Role in session
    pub role: ParticipantRole,
    /// Agent profile (cached)
    pub profile: Option<AgentCapabilityProfile>,
    /// Joined at
    pub joined_at: DateTime<Utc>,
    /// Current status
    pub status: ParticipantStatus,
    /// Permissions
    pub permissions: Vec<Permission>,
}

/// Participant roles
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantRole {
    /// Session leader (for hierarchical sessions)
    Leader,
    /// Regular participant
    Participant,
    /// Observer (read-only)
    Observer,
    /// Facilitator (mediator)
    Facilitator,
}

/// Participant status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantStatus {
    Active,
    Away,
    Busy,
    Offline,
    Disconnected,
}

/// Permissions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    SendMessages,
    ExecuteTasks,
    InviteOthers,
    RemoveParticipants,
    ModifyContext,
    EndSession,
    ViewHistory,
}

/// Session states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// Session created, waiting for participants
    Pending,
    /// Active collaboration
    Active,
    /// Paused (e.g., waiting for input)
    Paused,
    /// Completed successfully
    Completed,
    /// Failed or cancelled
    Terminated,
}

/// Collaboration context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationContext {
    /// Shared data store
    pub shared_data: HashMap<String, serde_json::Value>,
    /// Task assignments
    pub task_assignments: Vec<TaskAssignment>,
    /// Session goals
    pub goals: Vec<SessionGoal>,
    /// Custom metadata
    pub metadata: HashMap<String, String>,
}

/// Task assignment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAssignment {
    /// Task ID
    pub task_id: String,
    /// Assigned agent
    pub assignee: AgentId,
    /// Task description
    pub description: String,
    /// Task status
    pub status: TaskAssignmentStatus,
    /// Dependencies
    pub dependencies: Vec<String>,
    /// Deadline
    pub deadline: Option<DateTime<Utc>>,
    /// Result
    pub result: Option<serde_json::Value>,
}

/// Task assignment status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskAssignmentStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

/// Session goal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionGoal {
    /// Goal ID
    pub goal_id: String,
    /// Goal description
    pub description: String,
    /// Priority
    pub priority: GoalPriority,
    /// Status
    pub status: GoalStatus,
    /// Completion criteria
    pub completion_criteria: Vec<String>,
}

/// Goal priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum GoalPriority {
    Low,
    Medium,
    High,
    Critical,
}

/// Goal status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    NotStarted,
    InProgress,
    Achieved,
    Failed,
}

/// ACP 2.0 Protocol Handler
pub struct Acp20Protocol {
    /// Local agent ID
    agent_id: AgentId,
    /// Capability profile
    profile: AgentCapabilityProfile,
    /// Active sessions
    sessions: Arc<RwLock<HashMap<String, AcpSession>>>,
    /// Session event sender
    event_sender: mpsc::Sender<AcpEvent>,
    /// Service registry
    service_registry: Arc<RwLock<ServiceRegistry>>,
    /// Collaboration engine
    collaboration_engine: Arc<Mutex<CollaborationEngine>>,
}

/// ACP Events
#[derive(Debug, Clone)]
pub enum AcpEvent {
    /// Session created
    SessionCreated {
        session_id: String,
        creator: AgentId,
    },
    /// Participant joined
    ParticipantJoined {
        session_id: String,
        participant: AgentId,
    },
    /// Participant left
    ParticipantLeft {
        session_id: String,
        participant: AgentId,
    },
    /// Task assigned
    TaskAssigned {
        session_id: String,
        task: TaskAssignment,
    },
    /// Task completed
    TaskCompleted {
        session_id: String,
        task_id: String,
        result: serde_json::Value,
    },
    /// Session state changed
    SessionStateChanged {
        session_id: String,
        from: SessionState,
        to: SessionState,
    },
    /// Message received
    MessageReceived {
        session_id: String,
        from: AgentId,
        message: A2AMessage,
    },
    /// Error occurred
    Error {
        session_id: Option<String>,
        error: AcpError,
    },
}

/// ACP Errors
#[derive(Debug, thiserror::Error, Clone)]
pub enum AcpError {
    #[error("Session not found: {0}")]
    SessionNotFound(String),
    #[error("Agent not authorized: {0}")]
    NotAuthorized(String),
    #[error("Invalid capability: {0}")]
    InvalidCapability(String),
    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),
    #[error("Session expired: {0}")]
    SessionExpired(String),
    #[error("Protocol version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: String, actual: String },
    #[error("A2A error: {0}")]
    A2AError(String),
}

/// Service registry
#[derive(Debug, Default)]
pub struct ServiceRegistry {
    /// Registered agents
    agents: HashMap<AgentId, AgentCapabilityProfile>,
    /// Service capabilities index
    capabilities: HashMap<String, Vec<AgentId>>,
    /// Last update timestamp
    last_update: DateTime<Utc>,
}

/// Collaboration engine
#[derive(Debug, Default)]
pub struct CollaborationEngine {
    /// Active collaborations
    #[allow(dead_code)]
    collaborations: HashMap<String, CollaborationState>,
    /// Pending negotiations
    negotiations: Vec<NegotiationState>,
}

/// Collaboration state
#[derive(Debug, Clone)]
pub struct CollaborationState {
    /// Collaboration ID
    pub id: String,
    /// Session reference
    pub session_id: String,
    /// Collaboration type
    pub collab_type: CollaborationType,
    /// Status
    pub status: CollaborationStatus,
}

/// Collaboration types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollaborationType {
    TaskDelegation,
    ConsensusBuilding,
    ResourceSharing,
    KnowledgeExchange,
}

/// Collaboration status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollaborationStatus {
    Initiating,
    Negotiating,
    Executing,
    Verifying,
    Completed,
    Failed,
}

/// Negotiation state
#[derive(Debug, Clone)]
pub struct NegotiationState {
    /// Negotiation ID
    pub id: String,
    /// Initiator
    pub initiator: AgentId,
    /// Participants
    pub participants: Vec<AgentId>,
    /// Proposal
    pub proposal: CollaborationProposal,
    /// Responses
    pub responses: HashMap<AgentId, NegotiationResponse>,
    /// Deadline
    pub deadline: DateTime<Utc>,
}

/// Collaboration proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationProposal {
    /// Proposal type
    pub proposal_type: ProposalType,
    /// Description
    pub description: String,
    /// Required capabilities
    pub required_capabilities: Vec<String>,
    /// Resource requirements
    pub resource_requirements: ResourceRequirements,
    /// Expected duration in seconds
    pub expected_duration_sec: u64,
    /// Compensation (if applicable)
    pub compensation: Option<u64>,
}

/// Proposal types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalType {
    TaskRequest,
    ResourceOffer,
    Partnership,
    Consultation,
}

/// Negotiation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegotiationResponse {
    /// Response type
    pub response_type: ResponseType,
    /// Counter proposal (if applicable)
    pub counter_proposal: Option<CollaborationProposal>,
    /// Comments
    pub comments: Option<String>,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Response types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseType {
    Accept,
    Reject,
    Counter,
    Abstain,
}

impl Acp20Protocol {
    /// Create new ACP 2.0 protocol handler
    pub fn new(
        agent_id: AgentId,
        profile: AgentCapabilityProfile,
    ) -> (Self, mpsc::Receiver<AcpEvent>) {
        let (event_sender, event_receiver) = mpsc::channel(1000);

        let protocol = Self {
            agent_id,
            profile,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            event_sender,
            service_registry: Arc::new(RwLock::new(ServiceRegistry::default())),
            collaboration_engine: Arc::new(Mutex::new(CollaborationEngine::default())),
        };

        (protocol, event_receiver)
    }

    /// Get local capability profile
    pub fn profile(&self) -> &AgentCapabilityProfile {
        &self.profile
    }

    /// Create a new collaboration session
    pub async fn create_session(
        &self,
        session_type: SessionType,
        initial_participants: Vec<AgentId>,
        goals: Vec<SessionGoal>,
    ) -> Result<AcpSession, AcpError> {
        let session_id = Uuid::new_v4().to_string();
        let now = Utc::now();

        // Create participants list (including self)
        let mut participants = vec![SessionParticipant {
            agent_id: self.agent_id.clone(),
            role: ParticipantRole::Facilitator,
            profile: Some(self.profile.clone()),
            joined_at: now,
            status: ParticipantStatus::Active,
            permissions: vec![
                Permission::SendMessages,
                Permission::ExecuteTasks,
                Permission::InviteOthers,
                Permission::RemoveParticipants,
                Permission::ModifyContext,
                Permission::EndSession,
                Permission::ViewHistory,
            ],
        }];

        for agent_id in initial_participants {
            if agent_id != self.agent_id {
                participants.push(SessionParticipant {
                    agent_id,
                    role: ParticipantRole::Participant,
                    profile: None,
                    joined_at: now,
                    status: ParticipantStatus::Active,
                    permissions: vec![
                        Permission::SendMessages,
                        Permission::ExecuteTasks,
                        Permission::ViewHistory,
                    ],
                });
            }
        }

        let session = AcpSession {
            session_id: session_id.clone(),
            session_type,
            participants,
            state: SessionState::Pending,
            context: CollaborationContext {
                shared_data: HashMap::new(),
                task_assignments: Vec::new(),
                goals,
                metadata: HashMap::new(),
            },
            created_at: now,
            last_activity: now,
            expires_at: now + chrono::Duration::hours(24),
        };

        // Store session
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.clone(), session.clone());
        }

        // Emit event
        let _ = self
            .event_sender
            .send(AcpEvent::SessionCreated {
                session_id: session_id.clone(),
                creator: self.agent_id.clone(),
            })
            .await;

        info!("Created ACP 2.0 session: {}", session_id);
        Ok(session)
    }

    /// Join an existing session
    pub async fn join_session(
        &self,
        session_id: &str,
        agent_profile: Option<AgentCapabilityProfile>,
    ) -> Result<AcpSession, AcpError> {
        let mut sessions = self.sessions.write().await;

        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| AcpError::SessionNotFound(session_id.to_string()))?;

        // Check if already joined
        if session
            .participants
            .iter()
            .any(|p| p.agent_id == self.agent_id)
        {
            return Ok(session.clone());
        }

        // Add as participant
        let participant = SessionParticipant {
            agent_id: self.agent_id.clone(),
            role: ParticipantRole::Participant,
            profile: agent_profile,
            joined_at: Utc::now(),
            status: ParticipantStatus::Active,
            permissions: vec![
                Permission::SendMessages,
                Permission::ExecuteTasks,
                Permission::ViewHistory,
            ],
        };

        session.participants.push(participant);
        session.last_activity = Utc::now();

        // Emit event
        let _ = self
            .event_sender
            .send(AcpEvent::ParticipantJoined {
                session_id: session_id.to_string(),
                participant: self.agent_id.clone(),
            })
            .await;

        info!("Joined ACP 2.0 session: {}", session_id);
        Ok(session.clone())
    }

    /// Leave a session
    pub async fn leave_session(&self, session_id: &str) -> Result<(), AcpError> {
        let mut sessions = self.sessions.write().await;

        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| AcpError::SessionNotFound(session_id.to_string()))?;

        session.participants.retain(|p| p.agent_id != self.agent_id);
        session.last_activity = Utc::now();

        // Emit event
        let _ = self
            .event_sender
            .send(AcpEvent::ParticipantLeft {
                session_id: session_id.to_string(),
                participant: self.agent_id.clone(),
            })
            .await;

        info!("Left ACP 2.0 session: {}", session_id);
        Ok(())
    }

    /// Assign a task to a participant
    pub async fn assign_task(
        &self,
        session_id: &str,
        assignee: AgentId,
        description: impl Into<String>,
        dependencies: Vec<String>,
        deadline: Option<DateTime<Utc>>,
    ) -> Result<TaskAssignment, AcpError> {
        let mut sessions = self.sessions.write().await;

        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| AcpError::SessionNotFound(session_id.to_string()))?;

        // Verify assignee is a participant
        if !session.participants.iter().any(|p| p.agent_id == assignee) {
            return Err(AcpError::NotAuthorized(format!(
                "Agent {} is not a participant in session {}",
                assignee, session_id
            )));
        }

        let task = TaskAssignment {
            task_id: Uuid::new_v4().to_string(),
            assignee,
            description: description.into(),
            status: TaskAssignmentStatus::Pending,
            dependencies,
            deadline,
            result: None,
        };

        session.context.task_assignments.push(task.clone());
        session.last_activity = Utc::now();

        // Emit event
        let _ = self
            .event_sender
            .send(AcpEvent::TaskAssigned {
                session_id: session_id.to_string(),
                task: task.clone(),
            })
            .await;

        debug!(
            "Assigned task {} to {} in session {}",
            task.task_id, assignee, session_id
        );
        Ok(task)
    }

    /// Update task status
    pub async fn update_task_status(
        &self,
        session_id: &str,
        task_id: &str,
        status: TaskAssignmentStatus,
        result: Option<serde_json::Value>,
    ) -> Result<(), AcpError> {
        let mut sessions = self.sessions.write().await;

        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| AcpError::SessionNotFound(session_id.to_string()))?;

        let task = session
            .context
            .task_assignments
            .iter_mut()
            .find(|t| t.task_id == task_id)
            .ok_or_else(|| AcpError::A2AError(format!("Task {} not found", task_id)))?;

        task.status = status;
        task.result = result.clone();
        session.last_activity = Utc::now();

        if status == TaskAssignmentStatus::Completed {
            let _ = self
                .event_sender
                .send(AcpEvent::TaskCompleted {
                    session_id: session_id.to_string(),
                    task_id: task_id.to_string(),
                    result: result.unwrap_or(serde_json::Value::Null),
                })
                .await;
        }

        Ok(())
    }

    /// Send a message in a session
    pub async fn send_session_message(
        &self,
        session_id: &str,
        message: A2AMessage,
    ) -> Result<(), AcpError> {
        let sessions = self.sessions.read().await;

        let session = sessions
            .get(session_id)
            .ok_or_else(|| AcpError::SessionNotFound(session_id.to_string()))?;

        // Verify sender is a participant
        if !session
            .participants
            .iter()
            .any(|p| p.agent_id == self.agent_id)
        {
            return Err(AcpError::NotAuthorized(
                "Not a participant in this session".to_string(),
            ));
        }

        // Emit event
        let _ = self
            .event_sender
            .send(AcpEvent::MessageReceived {
                session_id: session_id.to_string(),
                from: self.agent_id.clone(),
                message,
            })
            .await;

        Ok(())
    }

    /// Update session state
    pub async fn update_session_state(
        &self,
        session_id: &str,
        new_state: SessionState,
    ) -> Result<(), AcpError> {
        let mut sessions = self.sessions.write().await;

        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| AcpError::SessionNotFound(session_id.to_string()))?;

        let old_state = session.state;
        session.state = new_state;
        session.last_activity = Utc::now();

        let _ = self
            .event_sender
            .send(AcpEvent::SessionStateChanged {
                session_id: session_id.to_string(),
                from: old_state,
                to: new_state,
            })
            .await;

        Ok(())
    }

    /// Get session info
    pub async fn get_session(&self, session_id: &str) -> Option<AcpSession> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).cloned()
    }

    /// List all active sessions
    pub async fn list_sessions(&self) -> Vec<AcpSession> {
        let sessions = self.sessions.read().await;
        sessions.values().cloned().collect()
    }

    /// Register an agent in the service registry
    pub async fn register_agent(&self, profile: AgentCapabilityProfile) {
        let mut registry = self.service_registry.write().await;

        // Index capabilities
        for capability in &profile.capabilities {
            registry
                .capabilities
                .entry(capability.id.clone())
                .or_default()
                .push(profile.agent_id.clone());
        }

        registry.agents.insert(profile.agent_id.clone(), profile);
        registry.last_update = Utc::now();
    }

    /// Find agents by capability
    pub async fn find_agents_by_capability(
        &self,
        capability_id: &str,
    ) -> Vec<AgentCapabilityProfile> {
        let registry = self.service_registry.read().await;

        registry
            .capabilities
            .get(capability_id)
            .map(|agent_ids| {
                agent_ids
                    .iter()
                    .filter_map(|id| registry.agents.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get agent profile
    pub async fn get_agent_profile(&self, agent_id: &AgentId) -> Option<AgentCapabilityProfile> {
        let registry = self.service_registry.read().await;
        registry.agents.get(agent_id).cloned()
    }

    /// Propose a collaboration
    pub async fn propose_collaboration(
        &self,
        to: AgentId,
        proposal: CollaborationProposal,
    ) -> Result<String, AcpError> {
        let negotiation_id = Uuid::new_v4().to_string();
        let deadline = Utc::now() + chrono::Duration::hours(1);

        let negotiation = NegotiationState {
            id: negotiation_id.clone(),
            initiator: self.agent_id.clone(),
            participants: vec![to],
            proposal,
            responses: HashMap::new(),
            deadline,
        };

        let mut engine = self.collaboration_engine.lock().await;
        engine.negotiations.push(negotiation);

        Ok(negotiation_id)
    }

    /// Respond to a collaboration proposal
    pub async fn respond_to_proposal(
        &self,
        negotiation_id: &str,
        response: NegotiationResponse,
    ) -> Result<(), AcpError> {
        let mut engine = self.collaboration_engine.lock().await;

        let negotiation = engine
            .negotiations
            .iter_mut()
            .find(|n| n.id == negotiation_id)
            .ok_or_else(|| {
                AcpError::A2AError(format!("Negotiation {} not found", negotiation_id))
            })?;

        negotiation
            .responses
            .insert(self.agent_id.clone(), response);
        Ok(())
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff_ms: 1000,
            max_backoff_ms: 30000,
            backoff_multiplier: 2.0,
            retryable_errors: vec![500, 502, 503, 504],
        }
    }
}

impl Default for TimeoutSettings {
    fn default() -> Self {
        Self {
            connection_timeout_sec: 10,
            request_timeout_sec: 60,
            session_timeout_sec: 3600,
        }
    }
}

impl Default for CommunicationPreferences {
    fn default() -> Self {
        Self {
            message_format: MessageFormat::Json,
            max_message_size: 10 * 1024 * 1024, // 10MB
            languages: vec!["en".to_string()],
            timezone: "UTC".to_string(),
            retry_policy: RetryPolicy::default(),
            timeout_settings: TimeoutSettings::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_profile(agent_id: &str) -> AgentCapabilityProfile {
        AgentCapabilityProfile {
            profile_id: Uuid::new_v4().to_string(),
            agent_id: AgentId::from_string(agent_id),
            protocol_version: ACP20_VERSION.to_string(),
            capabilities: vec![Capability {
                id: "test_capability".to_string(),
                name: "Test Capability".to_string(),
                description: "For testing".to_string(),
                capability_type: CapabilityType::TextGeneration,
                input_schema: serde_json::json!({}),
                output_schema: serde_json::json!({}),
                performance: PerformanceMetrics {
                    avg_latency_ms: 100,
                    throughput_rpm: 60,
                    success_rate: 0.99,
                    quality_score: 0.95,
                },
                resource_requirements: ResourceRequirements {
                    min_memory_mb: 512,
                    min_cpu_cores: 1,
                    gpu_memory_mb: 0,
                    network_mbps: 10,
                },
            }],
            constraints: ResourceConstraints {
                max_concurrent_tasks: 10,
                max_memory_mb: 2048,
                max_cpu_percent: 80,
                rate_limit_rpm: 100,
                daily_quota: None,
            },
            endpoints: vec![],
            preferences: CommunicationPreferences::default(),
            created_at: Utc::now(),
            expires_at: None,
        }
    }

    #[tokio::test]
    async fn test_acp20_protocol_creation() {
        let agent_id = AgentId::from_string("test-agent");
        let profile = create_test_profile("test-agent");

        let (protocol, _receiver) = Acp20Protocol::new(agent_id, profile);

        assert_eq!(protocol.profile().protocol_version, ACP20_VERSION);
    }

    #[tokio::test]
    async fn test_create_session() {
        let agent_id = AgentId::from_string("test-agent");
        let profile = create_test_profile("test-agent");

        let (protocol, _receiver) = Acp20Protocol::new(agent_id.clone(), profile);

        let goals = vec![SessionGoal {
            goal_id: "goal-1".to_string(),
            description: "Test goal".to_string(),
            priority: GoalPriority::High,
            status: GoalStatus::NotStarted,
            completion_criteria: vec!["Done".to_string()],
        }];

        let session = protocol
            .create_session(SessionType::PeerToPeer, vec![], goals)
            .await
            .unwrap();

        assert_eq!(session.session_type, SessionType::PeerToPeer);
        assert_eq!(session.participants.len(), 1);
        assert_eq!(session.participants[0].agent_id, agent_id);
        assert_eq!(session.state, SessionState::Pending);
    }

    #[tokio::test]
    async fn test_assign_and_update_task() {
        let agent_id = AgentId::from_string("test-agent");
        let profile = create_test_profile("test-agent");

        let (protocol, mut receiver) = Acp20Protocol::new(agent_id.clone(), profile);

        let session = protocol
            .create_session(SessionType::Direct, vec![], vec![])
            .await
            .unwrap();

        let task = protocol
            .assign_task(
                &session.session_id,
                agent_id.clone(),
                "Test task",
                vec![],
                None,
            )
            .await
            .unwrap();

        assert_eq!(task.description, "Test task");
        assert_eq!(task.status, TaskAssignmentStatus::Pending);

        // Update task status
        protocol
            .update_task_status(
                &session.session_id,
                &task.task_id,
                TaskAssignmentStatus::Completed,
                Some(serde_json::json!({"result": "success"})),
            )
            .await
            .unwrap();

        // Check event was emitted (skip SessionCreated and TaskAssigned events)
        use tokio::time::{timeout, Duration};
        let result = timeout(Duration::from_secs(5), async {
            loop {
                let event = receiver.recv().await.unwrap();
                match event {
                    AcpEvent::TaskCompleted {
                        session_id,
                        task_id,
                        ..
                    } => {
                        assert_eq!(session_id, session.session_id);
                        assert_eq!(task_id, task.task_id);
                        break;
                    }
                    AcpEvent::SessionCreated { .. } | AcpEvent::TaskAssigned { .. } => {
                        // Skip these events, continue waiting for completion
                        continue;
                    }
                    other => panic!("Expected TaskCompleted event, got {:?}", other),
                }
            }
        })
        .await;

        assert!(result.is_ok(), "Timeout waiting for TaskCompleted event");
    }

    #[tokio::test]
    async fn test_service_registry() {
        let agent_id = AgentId::from_string("test-agent");
        let profile = create_test_profile("test-agent");

        let (protocol, _receiver) = Acp20Protocol::new(agent_id, profile.clone());

        // Register agent
        protocol.register_agent(profile).await;

        // Find by capability
        let agents = protocol.find_agents_by_capability("test_capability").await;
        assert_eq!(agents.len(), 1);
    }
}
