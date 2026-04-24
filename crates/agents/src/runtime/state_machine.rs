//! Agent State Machine
//!
//! 🔒 P0 FIX: Complete state machine with proper error recovery and transition
//! rules. Implements a robust finite state machine for agent lifecycle
//! management.

use std::fmt;

use ::tracing::{debug, error, info, warn};

/// Agent states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    /// Initial state - agent is being set up
    Initializing,
    /// Ready to accept tasks
    Idle,
    /// Actively processing a task
    Processing,
    /// Waiting for external input/resource
    Waiting,
    /// Error state - recoverable error occurred
    Error,
    /// Recovering from error state
    Recovering,
    /// Paused - temporarily suspended
    Paused,
    /// Graceful shutdown in progress
    ShuttingDown,
    /// Fully terminated
    Terminated,
}

impl fmt::Display for AgentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentState::Initializing => write!(f, "initializing"),
            AgentState::Idle => write!(f, "idle"),
            AgentState::Processing => write!(f, "processing"),
            AgentState::Waiting => write!(f, "waiting"),
            AgentState::Error => write!(f, "error"),
            AgentState::Recovering => write!(f, "recovering"),
            AgentState::Paused => write!(f, "paused"),
            AgentState::ShuttingDown => write!(f, "shutting_down"),
            AgentState::Terminated => write!(f, "terminated"),
        }
    }
}

impl AgentState {
    /// Check if the state is active (can process tasks)
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            AgentState::Idle | AgentState::Processing | AgentState::Waiting
        )
    }

    /// Check if the state is terminal
    pub fn is_terminal(&self) -> bool {
        matches!(self, AgentState::Terminated)
    }

    /// Check if the state allows task execution
    pub fn can_execute_tasks(&self) -> bool {
        matches!(self, AgentState::Idle | AgentState::Processing)
    }

    /// Check if the state is an error state
    pub fn is_error(&self) -> bool {
        matches!(self, AgentState::Error)
    }

    /// Check if the state can be shut down from
    pub fn can_shutdown(&self) -> bool {
        !matches!(self, AgentState::ShuttingDown | AgentState::Terminated)
    }
}

/// State transition result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionResult {
    /// Transition succeeded
    Success,
    /// Transition failed - invalid from current state
    InvalidTransition { from: AgentState, to: AgentState },
    /// Transition not allowed in current context
    NotAllowed { reason: String },
}

/// State machine with error recovery
///
/// 🔒 P0 FIX: Complete state machine implementation with:
/// - Valid state transition rules
/// - Error recovery paths
/// - State history for debugging
/// - Transition hooks
pub struct StateMachine {
    /// Current state
    current: AgentState,
    /// Previous state (for recovery)
    previous: Option<AgentState>,
    /// State history for debugging (keeps last N states)
    history: Vec<(AgentState, std::time::Instant)>,
    /// Maximum history size
    max_history: usize,
    /// Error count for recovery logic
    error_count: u32,
    /// Maximum consecutive errors before forced shutdown
    max_errors: u32,
}

impl StateMachine {
    /// Create new state machine in Initializing state
    pub fn new() -> Self {
        let now = std::time::Instant::now();
        Self {
            current: AgentState::Initializing,
            previous: None,
            history: vec![(AgentState::Initializing, now)],
            max_history: 10,
            error_count: 0,
            max_errors: 5,
        }
    }

    /// Get current state
    pub fn current(&self) -> AgentState {
        self.current
    }

    /// Get previous state
    pub fn previous(&self) -> Option<AgentState> {
        self.previous
    }

    /// Get error count
    pub fn error_count(&self) -> u32 {
        self.error_count
    }

    /// Reset error count (call after successful recovery)
    pub fn reset_error_count(&mut self) {
        self.error_count = 0;
        debug!("Error count reset");
    }

    /// Attempt state transition
    ///
    /// 🔒 P0 FIX: Validates transitions according to the state diagram
    pub fn transition(&mut self, new_state: AgentState) -> TransitionResult {
        // Validate transition
        let valid = self.is_valid_transition(self.current, new_state);

        if !valid {
            warn!(
                "Invalid state transition attempted: {} -> {}",
                self.current, new_state
            );
            return TransitionResult::InvalidTransition {
                from: self.current,
                to: new_state,
            };
        }

        // Check error threshold for error -> other transitions
        // Allow transition to Recovering or ShuttingDown even at max errors
        if self.current == AgentState::Error
            && new_state != AgentState::Recovering
            && new_state != AgentState::ShuttingDown
        {
            if self.error_count >= self.max_errors {
                error!(
                    "Maximum error count ({}) reached, forcing shutdown",
                    self.max_errors
                );
                return TransitionResult::NotAllowed {
                    reason: format!("Maximum error count ({}) reached", self.max_errors),
                };
            }
        }

        // Perform transition
        info!("State transition: {} -> {}", self.current, new_state);

        self.previous = Some(self.current);
        self.current = new_state;

        // Update history
        self.history.push((new_state, std::time::Instant::now()));
        if self.history.len() > self.max_history {
            self.history.remove(0);
        }

        // Update error count
        if new_state == AgentState::Error {
            self.error_count += 1;
        } else if new_state == AgentState::Recovering || new_state == AgentState::Idle {
            // Reset error count on successful recovery
            if self.error_count > 0 && new_state == AgentState::Idle {
                self.reset_error_count();
            }
        }

        TransitionResult::Success
    }

    /// 🔒 P0 FIX: Complete state transition validation
    ///
    /// Defines valid transitions between all states:
    /// ```text
    /// Initializing -> Idle | Error | ShuttingDown
    /// Idle -> Processing | Paused | Error | ShuttingDown
    /// Processing -> Idle | Waiting | Error | ShuttingDown
    /// Waiting -> Processing | Idle | Error | ShuttingDown
    /// Error -> Recovering | ShuttingDown | Terminated (if max errors)
    /// Recovering -> Idle | Error
    /// Paused -> Idle | Error | ShuttingDown
    /// ShuttingDown -> Terminated
    /// Terminated -> (no transitions)
    /// ```
    fn is_valid_transition(&self, from: AgentState, to: AgentState) -> bool {
        let valid = match (from, to) {
            // Initializing can go to Idle (success), Error (failure), or ShuttingDown (abort)
            (AgentState::Initializing, AgentState::Idle) => true,
            (AgentState::Initializing, AgentState::Error) => true,
            (AgentState::Initializing, AgentState::ShuttingDown) => true,

            // Idle can start processing, pause, error, or shutdown
            (AgentState::Idle, AgentState::Processing) => true,
            (AgentState::Idle, AgentState::Paused) => true,
            (AgentState::Idle, AgentState::Error) => true,
            (AgentState::Idle, AgentState::ShuttingDown) => true,

            // Processing can complete (Idle), wait, error, or shutdown
            (AgentState::Processing, AgentState::Idle) => true,
            (AgentState::Processing, AgentState::Waiting) => true,
            (AgentState::Processing, AgentState::Error) => true,
            (AgentState::Processing, AgentState::ShuttingDown) => true,

            // Waiting can resume processing, go idle, error, or shutdown
            (AgentState::Waiting, AgentState::Processing) => true,
            (AgentState::Waiting, AgentState::Idle) => true, // 🔒 P0 FIX: Added
            (AgentState::Waiting, AgentState::Error) => true,
            (AgentState::Waiting, AgentState::ShuttingDown) => true, // 🔒 P0 FIX: Added

            // Error can recover, retry processing, shutdown, or terminate (if max errors)
            (AgentState::Error, AgentState::Recovering) => true,
            (AgentState::Error, AgentState::Processing) => true, // Allow retry after error
            (AgentState::Error, AgentState::ShuttingDown) => true, // 🔒 P0 FIX: Added
            (AgentState::Error, AgentState::Terminated) => self.error_count >= self.max_errors,

            // Recovering can succeed (Idle) or fail again (Error)
            (AgentState::Recovering, AgentState::Idle) => true,
            (AgentState::Recovering, AgentState::Error) => true,

            // Paused can resume (Idle), error, or shutdown
            (AgentState::Paused, AgentState::Idle) => true,
            (AgentState::Paused, AgentState::Error) => true,
            (AgentState::Paused, AgentState::ShuttingDown) => true,

            // ShuttingDown can only go to Terminated
            (AgentState::ShuttingDown, AgentState::Terminated) => true,

            // Terminated is final - no transitions allowed
            (AgentState::Terminated, _) => false,

            // Self-transitions are not allowed (except for specific cases)
            (s1, s2) if s1 == s2 => false,

            // All other transitions are invalid
            _ => false,
        };

        debug!("Transition validation: {} -> {} = {}", from, to, valid);
        valid
    }

    /// 🔒 P0 FIX: Attempt error recovery
    ///
    /// Tries to recover from Error state to Idle via Recovering state.
    /// Returns true if recovery was initiated successfully.
    pub fn attempt_recovery(&mut self) -> bool {
        if self.current != AgentState::Error {
            warn!("Cannot recover from state: {}", self.current);
            return false;
        }

        if self.error_count >= self.max_errors {
            error!("Maximum error count reached, cannot recover");
            return false;
        }

        info!(
            "Attempting error recovery (error count: {})",
            self.error_count
        );

        // Transition to Recovering first
        match self.transition(AgentState::Recovering) {
            TransitionResult::Success => {
                // In a real implementation, recovery logic would happen here
                // For now, we transition directly to Idle
                match self.transition(AgentState::Idle) {
                    TransitionResult::Success => {
                        info!("Recovery successful, agent is now Idle");
                        true
                    }
                    _ => {
                        error!("Recovery failed: could not transition to Idle");
                        false
                    }
                }
            }
            _ => {
                error!("Recovery failed: could not transition to Recovering");
                false
            }
        }
    }

    /// 🔒 P0 FIX: Graceful shutdown with state validation
    ///
    /// Initiates shutdown from any valid state.
    /// Returns true if shutdown was initiated successfully.
    pub fn shutdown(&mut self) -> bool {
        if !self.current.can_shutdown() {
            warn!("Cannot shutdown from state: {}", self.current);
            return false;
        }

        match self.transition(AgentState::ShuttingDown) {
            TransitionResult::Success => {
                info!("Shutdown initiated from state: {:?}", self.previous);
                true
            }
            TransitionResult::InvalidTransition { from, to } => {
                error!("Invalid shutdown transition: {:?} -> {:?}", from, to);
                false
            }
            TransitionResult::NotAllowed { reason } => {
                error!("Shutdown not allowed: {}", reason);
                false
            }
        }
    }

    /// 🔒 P0 FIX: Complete shutdown - transition to Terminated
    pub fn terminate(&mut self) -> bool {
        if self.current != AgentState::ShuttingDown {
            warn!(
                "Cannot terminate from state: {} (must be ShuttingDown)",
                self.current
            );
            return false;
        }

        match self.transition(AgentState::Terminated) {
            TransitionResult::Success => {
                info!("Agent terminated successfully");
                true
            }
            _ => {
                error!("Failed to terminate agent");
                false
            }
        }
    }

    /// Get state history
    pub fn history(&self) -> &[(AgentState, std::time::Instant)] {
        &self.history
    }

    /// Get time spent in current state
    pub fn time_in_current_state(&self) -> Option<std::time::Duration> {
        self.history.last().map(|(_, time)| time.elapsed())
    }

    /// Set maximum error count before forced termination
    pub fn set_max_errors(&mut self, max: u32) {
        self.max_errors = max;
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_machine_new() {
        let sm = StateMachine::new();
        assert_eq!(sm.current(), AgentState::Initializing);
        assert!(sm.previous().is_none());
    }

    #[test]
    fn test_valid_transitions() {
        let mut sm = StateMachine::new();

        // Initializing -> Idle
        assert!(matches!(
            sm.transition(AgentState::Idle),
            TransitionResult::Success
        ));
        assert_eq!(sm.current(), AgentState::Idle);

        // Idle -> Processing
        assert!(matches!(
            sm.transition(AgentState::Processing),
            TransitionResult::Success
        ));
        assert_eq!(sm.current(), AgentState::Processing);

        // Processing -> Waiting
        assert!(matches!(
            sm.transition(AgentState::Waiting),
            TransitionResult::Success
        ));
        assert_eq!(sm.current(), AgentState::Waiting);

        // Waiting -> Idle (P0 FIX: Added)
        assert!(matches!(
            sm.transition(AgentState::Idle),
            TransitionResult::Success
        ));
        assert_eq!(sm.current(), AgentState::Idle);

        // Idle -> ShuttingDown
        assert!(matches!(
            sm.transition(AgentState::ShuttingDown),
            TransitionResult::Success
        ));
        assert_eq!(sm.current(), AgentState::ShuttingDown);

        // ShuttingDown -> Terminated
        assert!(matches!(
            sm.transition(AgentState::Terminated),
            TransitionResult::Success
        ));
        assert_eq!(sm.current(), AgentState::Terminated);
    }

    #[test]
    fn test_invalid_transitions() {
        let mut sm = StateMachine::new();

        // Cannot go from Initializing to Processing (must go through Idle)
        assert!(matches!(
            sm.transition(AgentState::Processing),
            TransitionResult::InvalidTransition { .. }
        ));

        // Cannot go from Idle to Waiting (must go through Processing)
        sm.transition(AgentState::Idle);
        assert!(matches!(
            sm.transition(AgentState::Waiting),
            TransitionResult::InvalidTransition { .. }
        ));

        // Cannot go from Terminated to anything
        sm.transition(AgentState::ShuttingDown);
        sm.transition(AgentState::Terminated);
        assert!(matches!(
            sm.transition(AgentState::Idle),
            TransitionResult::InvalidTransition { .. }
        ));
    }

    #[test]
    fn test_error_recovery() {
        let mut sm = StateMachine::new();

        // Initializing -> Idle -> Processing -> Error
        sm.transition(AgentState::Idle);
        sm.transition(AgentState::Processing);
        sm.transition(AgentState::Error);
        assert_eq!(sm.error_count(), 1);

        // Try recovery
        assert!(sm.attempt_recovery());
        assert_eq!(sm.current(), AgentState::Idle);
        assert_eq!(sm.error_count(), 0); // Reset after successful recovery
    }

    #[test]
    fn test_max_error_limit() {
        let mut sm = StateMachine::new();
        sm.set_max_errors(3);

        // Initializing -> Idle -> Processing -> Error (x3)
        sm.transition(AgentState::Idle);

        for _ in 0..3 {
            sm.transition(AgentState::Processing);
            sm.transition(AgentState::Error);
        }

        assert_eq!(sm.error_count(), 3);

        // Recovery should fail at max errors
        assert!(!sm.attempt_recovery());

        // But shutdown should still work
        assert!(sm.shutdown());
    }

    #[test]
    fn test_shutdown_from_error() {
        // P0 FIX: Test that we can shutdown from Error state
        let mut sm = StateMachine::new();
        sm.transition(AgentState::Idle);
        sm.transition(AgentState::Processing);
        sm.transition(AgentState::Error);

        // Should be able to shutdown from Error
        assert!(sm.shutdown());
        assert_eq!(sm.current(), AgentState::ShuttingDown);
    }

    #[test]
    fn test_shutdown_from_waiting() {
        // P0 FIX: Test that we can shutdown from Waiting state
        let mut sm = StateMachine::new();
        sm.transition(AgentState::Idle);
        sm.transition(AgentState::Processing);
        sm.transition(AgentState::Waiting);

        // Should be able to shutdown from Waiting
        assert!(sm.shutdown());
        assert_eq!(sm.current(), AgentState::ShuttingDown);
    }

    #[test]
    fn test_state_history() {
        let mut sm = StateMachine::new();
        sm.transition(AgentState::Idle);
        sm.transition(AgentState::Processing);
        sm.transition(AgentState::Idle);

        let history = sm.history();
        assert_eq!(history.len(), 4); // Initializing + 3 transitions
    }

    #[test]
    fn test_state_helpers() {
        let mut sm = StateMachine::new();

        // Initializing is not active
        assert!(!sm.current().is_active());
        assert!(!sm.current().can_execute_tasks());

        sm.transition(AgentState::Idle);

        // Idle is active and can execute
        assert!(sm.current().is_active());
        assert!(sm.current().can_execute_tasks());

        sm.transition(AgentState::Processing);

        // Processing is active and can execute
        assert!(sm.current().is_active());
        assert!(sm.current().can_execute_tasks());

        sm.transition(AgentState::Waiting);

        // Waiting is active but cannot execute
        assert!(sm.current().is_active());
        assert!(!sm.current().can_execute_tasks());

        sm.transition(AgentState::Error);

        // Error is not active
        assert!(!sm.current().is_active());
        assert!(sm.current().is_error());
    }
}
