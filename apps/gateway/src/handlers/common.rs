//! Common Handler Utilities
//!
//! Shared helper functions for HTTP handlers to reduce code duplication.

use gateway::error::GatewayError;
use gateway::middleware::AuthUser;

use crate::models::AgentRecord;

/// Check if user is admin or owns the agent
///
/// Returns Ok(()) if user has permission, Err(GatewayError::forbidden)
/// otherwise
///
/// # Arguments
/// * `user` - The authenticated user
/// * `agent` - The agent record to check ownership
///
/// # Example
/// ```rust
/// check_ownership(&user, &agent)?;
/// ```
pub fn check_ownership(user: &AuthUser, agent: &AgentRecord) -> Result<(), GatewayError> {
    if user.is_admin() || agent.owner_id.as_deref() == Some(&user.user_id) {
        Ok(())
    } else {
        Err(GatewayError::forbidden(
            "You don't have permission to access this agent",
        ))
    }
}

/// Check if user is admin
///
/// Returns Ok(()) if user is admin, Err(GatewayError::forbidden) otherwise
#[allow(dead_code)]
pub fn require_admin(user: &AuthUser) -> Result<(), GatewayError> {
    if user.is_admin() {
        Ok(())
    } else {
        Err(GatewayError::forbidden("Admin access required"))
    }
}

/// Get user ID or return error if not authenticated
///
/// Helper for handlers that need the user ID
#[allow(dead_code)]
pub fn get_user_id(user: &AuthUser) -> &str {
    &user.user_id
}

/// Check if user can access agent (admin or owner)
///
/// Returns true if user has permission, false otherwise
#[allow(dead_code)]
pub fn can_access_agent(user: &AuthUser, agent: &AgentRecord) -> bool {
    user.is_admin() || agent.owner_id.as_deref() == Some(&user.user_id)
}

/// Build forbidden error for agent access
#[allow(dead_code)]
pub fn agent_access_denied() -> GatewayError {
    GatewayError::forbidden("You don't have permission to access this agent")
}

#[cfg(test)]
mod tests {
    // Note: These tests would require mocking AuthUser and AgentRecord
    // which depends on the gateway-lib internals
}
