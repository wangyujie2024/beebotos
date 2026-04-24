//! Enhanced Agent State Machine
//!
//! Provides a type-safe, validated state machine for agent lifecycle management
//! with support for state transitions, callbacks, and metadata.
//!
//! 🔒 P1 FIX: Enhanced state machine with strict transition validation and
//! comprehensive lifecycle management.

use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Agent lifecycle states
///
/// Represents the complete lifecycle of an agent from registration to
/// termination. Each state transition is validated to ensure correct lifecycle
/// progression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentLifecycleState {
    /// Agent is registered but not yet initialized
    ///
    /// Valid transitions: Initializing, Error
    Pending,

    /// Agent is initializing (loading config, connecting to services)
    ///
    /// Valid transitions: Idle, Error
    Initializing,

    /// Agent is idle and ready to accept tasks
    ///
    /// Valid transitions: Working, Paused, ShuttingDown, Error
    Idle,

    /// Agent is actively processing a task
    ///
    /// Valid transitions: Idle, Paused, Error
    Working,

    /// Agent is paused (can be resumed)
    ///
    /// Valid transitions: Idle, Working, ShuttingDown
    Paused,

    /// Agent is shutting down gracefully
    ///
    /// Valid transitions: Stopped, Error
    ShuttingDown,

    /// Agent has stopped
    ///
    /// Valid transitions: None (terminal state)
    Stopped,

    /// Agent encountered an unrecoverable error
    ///
    /// Valid transitions: Stopped, Initializing (retry)
    Error,
}

impl fmt::Display for AgentLifecycleState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentLifecycleState::Pending => write!(f, "pending"),
            AgentLifecycleState::Initializing => write!(f, "initializing"),
            AgentLifecycleState::Idle => write!(f, "idle"),
            AgentLifecycleState::Working => write!(f, "working"),
            AgentLifecycleState::Paused => write!(f, "paused"),
            AgentLifecycleState::ShuttingDown => write!(f, "shutting_down"),
            AgentLifecycleState::Stopped => write!(f, "stopped"),
            AgentLifecycleState::Error => write!(f, "error"),
        }
    }
}

impl AgentLifecycleState {
    /// Check if a state transition is valid
    ///
    /// Returns true if the transition from self to next is allowed
    /// according to the state machine rules.
    pub fn can_transition_to(&self, next: AgentLifecycleState) -> bool {
        use AgentLifecycleState::*;

        match (self, next) {
            // Pending can transition to Initializing or Error
            (Pending, Initializing) => true,
            (Pending, Error) => true,

            // Initializing can transition to Idle or Error
            (Initializing, Idle) => true,
            (Initializing, Error) => true,

            // Idle can transition to Working, Paused, ShuttingDown, or Error
            (Idle, Working) => true,
            (Idle, Paused) => true,
            (Idle, ShuttingDown) => true,
            (Idle, Error) => true,

            // Working can transition to Idle, Paused, or Error
            (Working, Idle) => true,
            (Working, Paused) => true,
            (Working, Error) => true,

            // Paused can transition to Idle, Working, or ShuttingDown
            (Paused, Idle) => true,
            (Paused, Working) => true,
            (Paused, ShuttingDown) => true,

            // ShuttingDown can transition to Stopped or Error
            (ShuttingDown, Stopped) => true,
            (ShuttingDown, Error) => true,

            // Error can transition to Stopped or back to Initializing (retry)
            (Error, Stopped) => true,
            (Error, Initializing) => true,

            // Stopped is terminal - no transitions allowed
            (Stopped, _) => false,

            // All other transitions are invalid
            _ => false,
        }
    }

    /// Get all valid next states from current state
    pub fn valid_transitions(&self) -> Vec<AgentLifecycleState> {
        use AgentLifecycleState::*;

        match self {
            Pending => vec![Initializing, Error],
            Initializing => vec![Idle, Error],
            Idle => vec![Working, Paused, ShuttingDown, Error],
            Working => vec![Idle, Paused, Error],
            Paused => vec![Idle, Working, ShuttingDown],
            ShuttingDown => vec![Stopped, Error],
            Error => vec![Stopped, Initializing],
            Stopped => vec![], // Terminal state
        }
    }

    /// Check if this is a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self, AgentLifecycleState::Stopped)
    }

    /// Check if this state allows task execution
    pub fn can_execute_tasks(&self) -> bool {
        matches!(
            self,
            AgentLifecycleState::Idle | AgentLifecycleState::Working
        )
    }

    /// Check if this state allows configuration changes
    pub fn can_modify_config(&self) -> bool {
        matches!(
            self,
            AgentLifecycleState::Pending
                | AgentLifecycleState::Idle
                | AgentLifecycleState::Paused
                | AgentLifecycleState::Error
        )
    }

    /// Get a human-readable description of the state
    pub fn description(&self) -> &'static str {
        match self {
            AgentLifecycleState::Pending => "Agent is registered but not yet initialized",
            AgentLifecycleState::Initializing => {
                "Agent is loading configuration and connecting to services"
            }
            AgentLifecycleState::Idle => "Agent is ready to accept tasks",
            AgentLifecycleState::Working => "Agent is actively processing a task",
            AgentLifecycleState::Paused => "Agent is paused and can be resumed",
            AgentLifecycleState::ShuttingDown => "Agent is gracefully shutting down",
            AgentLifecycleState::Stopped => "Agent has stopped",
            AgentLifecycleState::Error => "Agent encountered an error",
        }
    }

    /// Convert to agents crate AgentState
    pub fn to_agent_state(&self, task_id: Option<String>) -> beebotos_agents::AgentState {
        match self {
            AgentLifecycleState::Pending => beebotos_agents::AgentState::Registered,
            AgentLifecycleState::Initializing => beebotos_agents::AgentState::Initializing,
            AgentLifecycleState::Idle => beebotos_agents::AgentState::Idle,
            AgentLifecycleState::Working => beebotos_agents::AgentState::Working {
                task_id: task_id.unwrap_or_default(),
            },
            AgentLifecycleState::Paused => beebotos_agents::AgentState::Paused,
            AgentLifecycleState::ShuttingDown => beebotos_agents::AgentState::ShuttingDown,
            AgentLifecycleState::Stopped => beebotos_agents::AgentState::Stopped,
            AgentLifecycleState::Error => beebotos_agents::AgentState::Error {
                message: "State machine error".to_string(),
            },
        }
    }

    /// Convert from agents crate AgentState
    pub fn from_agent_state(state: &beebotos_agents::AgentState) -> Self {
        match state {
            beebotos_agents::AgentState::Registered => AgentLifecycleState::Pending,
            beebotos_agents::AgentState::Initializing => AgentLifecycleState::Initializing,
            beebotos_agents::AgentState::Idle => AgentLifecycleState::Idle,
            beebotos_agents::AgentState::Working { .. } => AgentLifecycleState::Working,
            beebotos_agents::AgentState::Paused => AgentLifecycleState::Paused,
            beebotos_agents::AgentState::ShuttingDown => AgentLifecycleState::ShuttingDown,
            beebotos_agents::AgentState::Stopped => AgentLifecycleState::Stopped,
            beebotos_agents::AgentState::Error { .. } => AgentLifecycleState::Error,
        }
    }
}

/// State transition with metadata
#[derive(Debug, Clone)]
pub struct StateTransition {
    /// The target state
    pub to_state: AgentLifecycleState,
    /// Reason for the transition
    pub reason: String,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
    /// Timestamp of the transition
    pub timestamp: Instant,
}

impl StateTransition {
    /// Create a new state transition
    pub fn new(to_state: AgentLifecycleState, reason: impl Into<String>) -> Self {
        Self {
            to_state,
            reason: reason.into(),
            metadata: HashMap::new(),
            timestamp: Instant::now(),
        }
    }

    /// Add metadata to the transition
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Get duration since transition
    pub fn elapsed(&self) -> Duration {
        self.timestamp.elapsed()
    }
}

/// State machine context for an agent
#[derive(Debug, Clone)]
pub struct StateMachineContext {
    /// Current state
    pub current_state: AgentLifecycleState,
    /// Previous state (if any)
    pub previous_state: Option<AgentLifecycleState>,
    /// State transition history
    pub history: Vec<StateTransition>,
    /// State entry timestamp
    pub state_entered_at: Instant,
    /// Total time spent in each state
    pub state_durations: HashMap<AgentLifecycleState, Duration>,
    /// Maximum time allowed in current state (for timeout detection)
    pub state_timeout: Option<Duration>,
    /// Number of state transitions
    pub transition_count: u64,
    /// Error count (for error state tracking)
    pub error_count: u32,
}

impl StateMachineContext {
    /// Create new state machine context
    pub fn new() -> Self {
        Self {
            current_state: AgentLifecycleState::Pending,
            previous_state: None,
            history: Vec::new(),
            state_entered_at: Instant::now(),
            state_durations: HashMap::new(),
            state_timeout: None,
            transition_count: 0,
            error_count: 0,
        }
    }

    /// Get current state
    pub fn state(&self) -> AgentLifecycleState {
        self.current_state
    }

    /// Get time spent in current state
    pub fn current_state_duration(&self) -> Duration {
        self.state_entered_at.elapsed()
    }

    /// Get total time spent in a specific state
    pub fn total_time_in_state(&self, state: AgentLifecycleState) -> Duration {
        *self.state_durations.get(&state).unwrap_or(&Duration::ZERO)
    }

    /// Check if state has timed out
    pub fn is_timed_out(&self) -> bool {
        if let Some(timeout) = self.state_timeout {
            self.current_state_duration() > timeout
        } else {
            false
        }
    }

    /// Set timeout for current state
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.state_timeout = Some(timeout);
    }

    /// Record a state transition
    pub fn record_transition(&mut self, transition: StateTransition) {
        // Update state durations
        let time_in_state = self.state_entered_at.elapsed();
        *self
            .state_durations
            .entry(self.current_state)
            .or_insert(Duration::ZERO) += time_in_state;

        // Update state
        self.previous_state = Some(self.current_state);
        self.current_state = transition.to_state;
        self.state_entered_at = Instant::now();
        self.state_timeout = None;
        self.transition_count += 1;

        // Track errors
        if transition.to_state == AgentLifecycleState::Error {
            self.error_count += 1;
        }

        // Record in history
        self.history.push(transition);

        // Limit history size
        if self.history.len() > 100 {
            self.history.remove(0);
        }
    }

    /// Get last transition
    pub fn last_transition(&self) -> Option<&StateTransition> {
        self.history.last()
    }

    /// Get transition history
    pub fn history(&self) -> &[StateTransition] {
        &self.history
    }

    /// Check if can transition to a state
    pub fn can_transition_to(&self, next: AgentLifecycleState) -> bool {
        self.current_state.can_transition_to(next)
    }

    /// Validate and perform transition
    pub fn transition_to(
        &mut self,
        next: AgentLifecycleState,
        reason: impl Into<String>,
    ) -> Result<(), StateMachineError> {
        if !self.can_transition_to(next) {
            return Err(StateMachineError::InvalidTransition {
                from: self.current_state,
                to: next,
                valid_transitions: self.current_state.valid_transitions(),
            });
        }

        let transition = StateTransition::new(next, reason);
        self.record_transition(transition);
        Ok(())
    }

    /// Get retry count for error recovery
    pub fn error_retry_count(&self) -> u32 {
        // Count consecutive Error -> Initializing transitions
        let mut count = 0;
        for transition in self.history.iter().rev() {
            if transition.to_state == AgentLifecycleState::Initializing {
                count += 1;
            } else if transition.to_state == AgentLifecycleState::Error {
                continue;
            } else {
                break;
            }
        }
        count
    }

    /// Check if should stop retrying
    pub fn should_stop_retrying(&self, max_retries: u32) -> bool {
        self.error_retry_count() >= max_retries
    }
}

impl Default for StateMachineContext {
    fn default() -> Self {
        Self::new()
    }
}

/// State machine errors
#[derive(Debug, thiserror::Error)]
pub enum StateMachineError {
    #[error(
        "Invalid state transition from {from} to {to}. Valid transitions: {valid_transitions:?}"
    )]
    InvalidTransition {
        from: AgentLifecycleState,
        to: AgentLifecycleState,
        valid_transitions: Vec<AgentLifecycleState>,
    },

    #[error("State timeout: {state} exceeded {timeout:?}")]
    StateTimeout {
        state: AgentLifecycleState,
        timeout: Duration,
        actual: Duration,
    },

    #[error("Maximum retry count exceeded")]
    MaxRetriesExceeded,

    #[error("State machine is in terminal state: {0}")]
    TerminalState(AgentLifecycleState),
}

/// State machine manager for multiple agents
#[allow(dead_code)]
pub struct StateMachineManager {
    contexts: HashMap<String, StateMachineContext>,
    max_retries: u32,
    default_timeouts: HashMap<AgentLifecycleState, Duration>,
}

impl StateMachineManager {
    /// Create new state machine manager
    pub fn new() -> Self {
        let mut default_timeouts = HashMap::new();
        default_timeouts.insert(AgentLifecycleState::Initializing, Duration::from_secs(60));
        default_timeouts.insert(AgentLifecycleState::Working, Duration::from_secs(300));
        default_timeouts.insert(AgentLifecycleState::ShuttingDown, Duration::from_secs(30));

        Self {
            contexts: HashMap::new(),
            max_retries: 3,
            default_timeouts,
        }
    }

    /// Register a new agent
    pub fn register_agent(&mut self, agent_id: impl Into<String>) -> &mut StateMachineContext {
        let agent_id = agent_id.into();
        let context = StateMachineContext::new();
        self.contexts.insert(agent_id.clone(), context);
        self.contexts.get_mut(&agent_id).unwrap()
    }

    /// Get context for an agent
    pub fn get_context(&self, agent_id: &str) -> Option<&StateMachineContext> {
        self.contexts.get(agent_id)
    }

    /// Get mutable context for an agent
    #[allow(dead_code)]
    pub fn get_context_mut(&mut self, agent_id: &str) -> Option<&mut StateMachineContext> {
        self.contexts.get_mut(agent_id)
    }

    /// Unregister an agent
    pub fn unregister_agent(&mut self, agent_id: &str) {
        self.contexts.remove(agent_id);
    }

    /// Perform state transition
    pub fn transition(
        &mut self,
        agent_id: &str,
        next: AgentLifecycleState,
        reason: impl Into<String>,
    ) -> Result<(), StateMachineError> {
        let context = self
            .contexts
            .get_mut(agent_id)
            .ok_or_else(|| StateMachineError::TerminalState(AgentLifecycleState::Stopped))?;

        // Check for retries
        if next == AgentLifecycleState::Initializing
            && context.should_stop_retrying(self.max_retries)
        {
            return Err(StateMachineError::MaxRetriesExceeded);
        }

        context.transition_to(next, reason)
    }

    /// Get current state
    pub fn get_state(&self, agent_id: &str) -> Option<AgentLifecycleState> {
        self.contexts.get(agent_id).map(|c| c.state())
    }

    /// Check if agent exists
    #[allow(dead_code)]
    pub fn has_agent(&self, agent_id: &str) -> bool {
        self.contexts.contains_key(agent_id)
    }

    /// List all agents
    pub fn list_agents(&self) -> Vec<String> {
        self.contexts.keys().cloned().collect()
    }

    /// List agents in a specific state
    pub fn list_agents_in_state(&self, state: AgentLifecycleState) -> Vec<String> {
        self.contexts
            .iter()
            .filter(|(_, ctx)| ctx.state() == state)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Check for timed out agents
    pub fn check_timeouts(&self) -> Vec<(String, AgentLifecycleState, Duration)> {
        self.contexts
            .iter()
            .filter(|(_, ctx)| ctx.is_timed_out())
            .map(|(id, ctx)| {
                let timeout = ctx.state_timeout.unwrap_or(Duration::MAX);
                (id.clone(), ctx.state(), timeout)
            })
            .collect()
    }

    /// Set max retry count
    #[allow(dead_code)]
    pub fn set_max_retries(&mut self, max: u32) {
        self.max_retries = max;
    }

    /// Set default timeout for a state
    #[allow(dead_code)]
    pub fn set_default_timeout(&mut self, state: AgentLifecycleState, timeout: Duration) {
        self.default_timeouts.insert(state, timeout);
    }
}

impl Default for StateMachineManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_transitions() {
        use AgentLifecycleState::*;

        assert!(Pending.can_transition_to(Initializing));
        assert!(Pending.can_transition_to(Error));
        assert!(!Pending.can_transition_to(Stopped));

        assert!(Initializing.can_transition_to(Idle));
        assert!(Initializing.can_transition_to(Error));
        assert!(!Initializing.can_transition_to(Working));

        assert!(Idle.can_transition_to(Working));
        assert!(Idle.can_transition_to(Paused));
        assert!(Idle.can_transition_to(ShuttingDown));
        assert!(!Idle.can_transition_to(Pending));

        // Stopped is terminal state - no transitions allowed
        assert!(!Stopped.can_transition_to(Pending));
    }

    #[test]
    fn test_state_machine_context() {
        let mut ctx = StateMachineContext::new();
        assert_eq!(ctx.state(), AgentLifecycleState::Pending);

        ctx.transition_to(AgentLifecycleState::Initializing, "Starting initialization")
            .unwrap();
        assert_eq!(ctx.state(), AgentLifecycleState::Initializing);
        assert_eq!(ctx.previous_state, Some(AgentLifecycleState::Pending));

        ctx.transition_to(AgentLifecycleState::Idle, "Init complete")
            .unwrap();
        assert_eq!(ctx.state(), AgentLifecycleState::Idle);
    }

    #[test]
    fn test_invalid_transition() {
        let mut ctx = StateMachineContext::new();
        let result = ctx.transition_to(AgentLifecycleState::Working, "Try to work");
        assert!(result.is_err());
    }

    #[test]
    fn test_state_machine_manager() {
        let mut manager = StateMachineManager::new();

        manager.register_agent("agent-1");
        assert_eq!(
            manager.get_state("agent-1"),
            Some(AgentLifecycleState::Pending)
        );

        manager
            .transition("agent-1", AgentLifecycleState::Initializing, "Start")
            .unwrap();
        assert_eq!(
            manager.get_state("agent-1"),
            Some(AgentLifecycleState::Initializing)
        );
    }
}
