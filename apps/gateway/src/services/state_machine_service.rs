//! State Machine Service
//!
//! Integrates the enhanced state machine with the existing AgentStateManager
//! from the agents crate, providing type-safe state transitions and lifecycle
//! management.
//!
//! 🔒 P1 FIX: Enhanced state machine integration with validation and tracking.

use std::collections::HashMap;
// Arc is used in tests
#[allow(unused_imports)]
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tracing::{info, instrument, warn};

use crate::error::AppError;
#[allow(unused_imports)]
use crate::state_machine::{
    AgentLifecycleState, StateMachineContext, StateMachineError, StateMachineManager,
    StateTransition,
};

/// Service for managing agent state machines
pub struct StateMachineService {
    /// Local state machine manager (enhanced)
    local_manager: RwLock<StateMachineManager>,
    /// Reference to agents crate state manager handle
    agents_state_manager: beebotos_agents::StateManagerHandle,
    /// Event callback handlers
    event_handlers: RwLock<Vec<Box<dyn StateEventHandler + Send + Sync>>>,
}

impl StateMachineService {
    /// Create new state machine service
    pub fn new(agents_state_manager: beebotos_agents::StateManagerHandle) -> Self {
        Self {
            local_manager: RwLock::new(StateMachineManager::new()),
            agents_state_manager,
            event_handlers: RwLock::new(Vec::new()),
        }
    }

    /// Register an agent with the state machine
    #[instrument(skip(self), fields(agent_id = %agent_id))]
    pub async fn register_agent(
        &self,
        agent_id: impl Into<String> + std::fmt::Display,
    ) -> Result<(), AppError> {
        let agent_id = agent_id.into();

        // Register in local manager
        {
            let mut manager = self.local_manager.write().await;
            manager.register_agent(&agent_id);
        }

        // Register in agents crate state manager
        let mut metadata = HashMap::new();
        metadata.insert("registered_at".to_string(), chrono::Utc::now().to_rfc3339());

        self.agents_state_manager
            .register_agent(&agent_id, metadata)
            .await
            .map_err(|e| AppError::Agent(e))?;

        info!("Agent {} registered in state machine", agent_id);
        Ok(())
    }

    /// Perform state transition with validation
    #[instrument(skip(self, reason), fields(agent_id = %agent_id))]
    pub async fn transition_to(
        &self,
        agent_id: &str,
        target_state: AgentLifecycleState,
        reason: impl Into<String> + std::fmt::Display,
    ) -> Result<(), AppError> {
        let reason = reason.into();

        // Get current state
        let current_state = {
            let manager = self.local_manager.read().await;
            manager
                .get_state(agent_id)
                .ok_or_else(|| AppError::not_found("Agent state machine", agent_id))?
        };

        // Validate transition
        if !current_state.can_transition_to(target_state) {
            warn!(
                "Invalid state transition attempt: {:?} -> {:?}",
                current_state, target_state
            );
            return Err(StateMachineError::InvalidTransition {
                from: current_state,
                to: target_state,
                valid_transitions: current_state.valid_transitions(),
            }
            .into());
        }

        // Perform local transition
        {
            let mut manager = self.local_manager.write().await;
            manager.transition(agent_id, target_state, &reason)?;
        }

        // Map to agents crate transition
        let agents_transition = map_to_agents_transition(current_state, target_state, &reason);

        // Perform agents crate transition
        self.agents_state_manager
            .transition(agent_id, agents_transition)
            .await
            .map_err(|e| AppError::Agent(e))?;

        // Emit event
        self.emit_event(StateEvent {
            agent_id: agent_id.to_string(),
            from_state: current_state,
            to_state: target_state,
            reason: reason.clone(),
            timestamp: std::time::Instant::now(),
        })
        .await;

        info!(
            "Agent {} transitioned from {:?} to {:?}: {}",
            agent_id, current_state, target_state, reason
        );

        Ok(())
    }

    /// Get current state
    pub async fn get_state(&self, agent_id: &str) -> Option<AgentLifecycleState> {
        let manager = self.local_manager.read().await;
        manager.get_state(agent_id)
    }

    /// Get full context
    pub async fn get_context(&self, agent_id: &str) -> Option<StateMachineContext> {
        let manager = self.local_manager.read().await;
        manager.get_context(agent_id).cloned()
    }

    /// Check if can transition to a state
    pub async fn can_transition_to(&self, agent_id: &str, target: AgentLifecycleState) -> bool {
        let manager = self.local_manager.read().await;
        if let Some(ctx) = manager.get_context(agent_id) {
            ctx.can_transition_to(target)
        } else {
            false
        }
    }

    /// Get valid transitions from current state
    pub async fn valid_transitions(&self, agent_id: &str) -> Vec<AgentLifecycleState> {
        let manager = self.local_manager.read().await;
        if let Some(ctx) = manager.get_context(agent_id) {
            ctx.state().valid_transitions()
        } else {
            Vec::new()
        }
    }

    /// Start an agent (Pending -> Initializing)
    pub async fn start_agent(&self, agent_id: &str) -> Result<(), AppError> {
        self.transition_to(
            agent_id,
            AgentLifecycleState::Initializing,
            "User initiated start",
        )
        .await
    }

    /// Mark initialization complete (Initializing -> Idle)
    pub async fn initialization_complete(&self, agent_id: &str) -> Result<(), AppError> {
        self.transition_to(
            agent_id,
            AgentLifecycleState::Idle,
            "Initialization complete",
        )
        .await
    }

    /// Start task execution (Idle -> Working)
    pub async fn begin_task(&self, agent_id: &str, task_id: &str) -> Result<(), AppError> {
        self.transition_to(
            agent_id,
            AgentLifecycleState::Working,
            format!("Beginning task: {}", task_id),
        )
        .await
    }

    /// Complete task execution (Working -> Idle)
    pub async fn complete_task(&self, agent_id: &str, success: bool) -> Result<(), AppError> {
        let reason = if success {
            "Task completed successfully"
        } else {
            "Task failed"
        };
        self.transition_to(agent_id, AgentLifecycleState::Idle, reason)
            .await
    }

    /// Pause agent (Idle/Working -> Paused)
    pub async fn pause_agent(&self, agent_id: &str) -> Result<(), AppError> {
        self.transition_to(agent_id, AgentLifecycleState::Paused, "User paused agent")
            .await
    }

    /// Resume agent (Paused -> Idle)
    pub async fn resume_agent(&self, agent_id: &str) -> Result<(), AppError> {
        self.transition_to(agent_id, AgentLifecycleState::Idle, "User resumed agent")
            .await
    }

    /// Shutdown agent (any -> ShuttingDown)
    pub async fn shutdown_agent(&self, agent_id: &str) -> Result<(), AppError> {
        // Check if agent is already in terminal state
        let current = self.get_state(agent_id).await;
        if let Some(state) = current {
            if state.is_terminal() {
                return Ok(()); // Already stopped
            }
        }

        self.transition_to(
            agent_id,
            AgentLifecycleState::ShuttingDown,
            "Shutdown initiated",
        )
        .await
    }

    /// Mark agent as stopped
    pub async fn stop_agent(&self, agent_id: &str) -> Result<(), AppError> {
        self.transition_to(agent_id, AgentLifecycleState::Stopped, "Agent stopped")
            .await
    }

    /// Report error (any -> Error)
    pub async fn report_error(
        &self,
        agent_id: &str,
        error_message: impl Into<String>,
    ) -> Result<(), AppError> {
        let message = error_message.into();
        self.transition_to(
            agent_id,
            AgentLifecycleState::Error,
            format!("Error: {}", message),
        )
        .await
    }

    /// Retry from error (Error -> Initializing)
    pub async fn retry_agent(&self, agent_id: &str) -> Result<(), AppError> {
        let ctx = self
            .get_context(agent_id)
            .await
            .ok_or_else(|| AppError::not_found("Agent state", agent_id))?;

        // Check retry count
        if ctx.should_stop_retrying(3) {
            return Err(StateMachineError::MaxRetriesExceeded.into());
        }

        self.transition_to(
            agent_id,
            AgentLifecycleState::Initializing,
            "Retrying after error",
        )
        .await
    }

    /// Unregister agent
    pub async fn unregister_agent(&self, agent_id: &str) -> Result<(), AppError> {
        // Unregister from agents crate
        self.agents_state_manager
            .unregister_agent(agent_id)
            .await
            .map_err(|e| AppError::Agent(e))?;

        // Unregister from local manager
        {
            let mut manager = self.local_manager.write().await;
            manager.unregister_agent(agent_id);
        }

        info!("Agent {} unregistered from state machine", agent_id);
        Ok(())
    }

    /// List all agents
    pub async fn list_agents(&self) -> Vec<String> {
        let manager = self.local_manager.read().await;
        manager.list_agents()
    }

    /// List agents in a specific state
    pub async fn list_agents_in_state(&self, state: AgentLifecycleState) -> Vec<String> {
        let manager = self.local_manager.read().await;
        manager.list_agents_in_state(state)
    }

    /// Check for timed out agents
    pub async fn check_timeouts(&self) -> Vec<(String, AgentLifecycleState, Duration)> {
        let manager = self.local_manager.read().await;
        manager.check_timeouts()
    }

    /// Add event handler
    pub async fn add_event_handler<H>(&self, handler: H)
    where
        H: StateEventHandler + Send + Sync + 'static,
    {
        let mut handlers = self.event_handlers.write().await;
        handlers.push(Box::new(handler));
    }

    /// Emit state event to all handlers
    async fn emit_event(&self, event: StateEvent) {
        let handlers = self.event_handlers.read().await;
        for handler in handlers.iter() {
            handler.on_state_change(&event).await;
        }
    }

    /// Get state machine statistics
    pub async fn get_statistics(&self) -> StateMachineStatistics {
        let manager = self.local_manager.read().await;
        let agents = manager.list_agents();

        let mut state_counts: HashMap<AgentLifecycleState, usize> = HashMap::new();
        for agent_id in &agents {
            if let Some(state) = manager.get_state(agent_id) {
                *state_counts.entry(state).or_insert(0) += 1;
            }
        }

        StateMachineStatistics {
            total_agents: agents.len(),
            state_counts,
            timed_out_agents: manager.check_timeouts().len(),
        }
    }
}

impl From<StateMachineError> for AppError {
    fn from(err: StateMachineError) -> Self {
        match err {
            StateMachineError::InvalidTransition {
                from,
                to,
                valid_transitions,
            } => AppError::validation(vec![crate::error::ValidationError {
                field: "state_transition".to_string(),
                message: format!(
                    "Cannot transition from {:?} to {:?}. Valid: {:?}",
                    from, to, valid_transitions
                ),
                code: "invalid_transition".to_string(),
            }]),
            StateMachineError::MaxRetriesExceeded => {
                AppError::Internal("Maximum retry attempts exceeded".to_string())
            }
            StateMachineError::StateTimeout {
                state,
                timeout,
                actual,
            } => AppError::Internal(format!(
                "State {:?} timed out after {:?} (limit: {:?})",
                state, actual, timeout
            )),
            StateMachineError::TerminalState(s) => {
                AppError::Internal(format!("Agent is in terminal state: {:?}", s))
            }
        }
    }
}

/// State event
#[derive(Debug, Clone)]
pub struct StateEvent {
    pub agent_id: String,
    pub from_state: AgentLifecycleState,
    pub to_state: AgentLifecycleState,
    pub reason: String,
    pub timestamp: std::time::Instant,
}

/// State event handler trait
#[async_trait::async_trait]
pub trait StateEventHandler {
    async fn on_state_change(&self, event: &StateEvent);
}

/// State machine statistics
#[derive(Debug, Clone)]
pub struct StateMachineStatistics {
    pub total_agents: usize,
    pub state_counts: HashMap<AgentLifecycleState, usize>,
    pub timed_out_agents: usize,
}

/// Map state transition to agents crate StateTransition
///
/// This function maps based on current state and target state combination
fn map_to_agents_transition(
    current: AgentLifecycleState,
    target: AgentLifecycleState,
    reason: &str,
) -> beebotos_agents::StateTransition {
    use beebotos_agents::StateTransition;

    match (current, target) {
        // Pending -> Initializing
        (AgentLifecycleState::Pending, AgentLifecycleState::Initializing) => StateTransition::Start,
        // Pending -> Error
        (AgentLifecycleState::Pending, AgentLifecycleState::Error) => StateTransition::Error {
            message: reason.to_string(),
        },

        // Initializing -> Idle
        (AgentLifecycleState::Initializing, AgentLifecycleState::Idle) => {
            StateTransition::InitializationComplete
        }
        // Initializing -> Error
        (AgentLifecycleState::Initializing, AgentLifecycleState::Error) => StateTransition::Error {
            message: reason.to_string(),
        },

        // Idle -> Working
        (AgentLifecycleState::Idle, AgentLifecycleState::Working) => StateTransition::BeginTask {
            task_id: reason.to_string(),
        },
        // Idle -> Paused
        (AgentLifecycleState::Idle, AgentLifecycleState::Paused) => StateTransition::Pause,
        // Idle -> ShuttingDown
        (AgentLifecycleState::Idle, AgentLifecycleState::ShuttingDown) => StateTransition::Shutdown,
        // Idle -> Error
        (AgentLifecycleState::Idle, AgentLifecycleState::Error) => StateTransition::Error {
            message: reason.to_string(),
        },

        // Working -> Idle
        (AgentLifecycleState::Working, AgentLifecycleState::Idle) => {
            StateTransition::CompleteTask { success: true }
        }
        // Working -> Paused
        (AgentLifecycleState::Working, AgentLifecycleState::Paused) => StateTransition::Pause,
        // Working -> Error
        (AgentLifecycleState::Working, AgentLifecycleState::Error) => StateTransition::Error {
            message: reason.to_string(),
        },

        // Paused -> Idle (Resume)
        (AgentLifecycleState::Paused, AgentLifecycleState::Idle) => StateTransition::Resume,
        // Paused -> Working
        (AgentLifecycleState::Paused, AgentLifecycleState::Working) => StateTransition::BeginTask {
            task_id: reason.to_string(),
        },
        // Paused -> ShuttingDown
        (AgentLifecycleState::Paused, AgentLifecycleState::ShuttingDown) => {
            StateTransition::Shutdown
        }

        // ShuttingDown -> Stopped
        (AgentLifecycleState::ShuttingDown, AgentLifecycleState::Stopped) => {
            StateTransition::Stopped
        }
        // ShuttingDown -> Error
        (AgentLifecycleState::ShuttingDown, AgentLifecycleState::Error) => StateTransition::Error {
            message: reason.to_string(),
        },

        // Error -> Stopped
        (AgentLifecycleState::Error, AgentLifecycleState::Stopped) => StateTransition::Shutdown,
        // Error -> Initializing (retry)
        (AgentLifecycleState::Error, AgentLifecycleState::Initializing) => StateTransition::Start,

        // Fallback for any other transitions (should not happen with valid transitions)
        _ => StateTransition::Error {
            message: format!("Unexpected transition: {:?} -> {:?}", current, target),
        },
    }
}

/// Example event handler for logging
#[allow(dead_code)]
pub struct LoggingStateEventHandler;

#[async_trait::async_trait]
impl StateEventHandler for LoggingStateEventHandler {
    async fn on_state_change(&self, event: &StateEvent) {
        info!(
            "[StateEvent] Agent {}: {:?} -> {:?} | Reason: {}",
            event.agent_id, event.from_state, event.to_state, event.reason
        );
    }
}

/// Example event handler for metrics
#[allow(dead_code)]
pub struct MetricsStateEventHandler {
    // Could include metrics client here
}

#[async_trait::async_trait]
impl StateEventHandler for MetricsStateEventHandler {
    async fn on_state_change(&self, _event: &StateEvent) {
        // Increment metrics counters
        // metrics::counter!("agent_state_transitions_total", 1,
        //     "from" => event.from_state.to_string(),
        //     "to" => event.to_state.to_string()
        // );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_state_machine_service() {
        let state_manager = Arc::new(beebotos_agents::AgentStateManager::new(None));
        let service = StateMachineService::new(state_manager);

        // Register agent
        service.register_agent("test-agent").await.unwrap();

        // Start agent
        service.start_agent("test-agent").await.unwrap();
        assert_eq!(
            service.get_state("test-agent").await,
            Some(AgentLifecycleState::Initializing)
        );

        // Complete initialization
        service.initialization_complete("test-agent").await.unwrap();
        assert_eq!(
            service.get_state("test-agent").await,
            Some(AgentLifecycleState::Idle)
        );

        // Begin task
        service.begin_task("test-agent", "task-1").await.unwrap();
        assert_eq!(
            service.get_state("test-agent").await,
            Some(AgentLifecycleState::Working)
        );

        // Complete task
        service.complete_task("test-agent", true).await.unwrap();
        assert_eq!(
            service.get_state("test-agent").await,
            Some(AgentLifecycleState::Idle)
        );
    }

    #[tokio::test]
    async fn test_invalid_transition() {
        let state_manager = Arc::new(beebotos_agents::AgentStateManager::new(None));
        let service = StateMachineService::new(state_manager);

        service.register_agent("test-agent").await.unwrap();

        // Try to go directly to Working (invalid)
        let result = service.begin_task("test-agent", "task-1").await;
        assert!(result.is_err());
    }
}
