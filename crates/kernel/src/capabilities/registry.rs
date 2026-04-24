//! Capability Registry
//!
//! Registry for managing capability tokens.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use super::tokens::TokenStatus;
use super::CapabilityToken;

/// Registry for capability tokens
#[derive(Debug, Clone)]
pub struct CapabilityRegistry {
    tokens: Arc<RwLock<HashMap<String, CapabilityToken>>>,
}

impl CapabilityRegistry {
    /// Create new registry
    pub fn new() -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a token
    pub fn register(&self, token: CapabilityToken) {
        let mut tokens = self.tokens.write();
        tokens.insert(token.id.clone(), token);
    }

    /// Get token by ID
    pub fn get(&self, token_id: &str) -> Option<CapabilityToken> {
        let tokens = self.tokens.read();
        tokens.get(token_id).cloned()
    }

    /// Approve pending token
    pub fn approve(&self, token_id: &str) -> Result<CapabilityToken, RegistryError> {
        let mut tokens = self.tokens.write();

        if let Some(token) = tokens.get_mut(token_id) {
            if token.status != TokenStatus::Pending {
                return Err(RegistryError::NotPending);
            }
            token.approve();
            Ok(token.clone())
        } else {
            Err(RegistryError::NotFound)
        }
    }

    /// Verify token is valid
    pub fn verify(&self, token: &CapabilityToken) -> Result<(), RegistryError> {
        let tokens = self.tokens.read();

        if let Some(stored) = tokens.get(&token.id) {
            if stored.status == TokenStatus::Revoked {
                return Err(RegistryError::Revoked);
            }
            if stored.is_expired() {
                return Err(RegistryError::Expired);
            }
            if stored.status != TokenStatus::Active {
                return Err(RegistryError::NotActive);
            }
            Ok(())
        } else {
            Err(RegistryError::NotFound)
        }
    }

    /// Revoke token
    pub fn revoke(&self, token_id: &str) -> Result<(), RegistryError> {
        let mut tokens = self.tokens.write();

        if let Some(token) = tokens.get_mut(token_id) {
            token.revoke();
            Ok(())
        } else {
            Err(RegistryError::NotFound)
        }
    }

    /// Clean up expired tokens
    pub fn cleanup_expired(&self) {
        let mut tokens = self.tokens.write();
        tokens.retain(|_, token| !token.is_expired());
    }

    /// Get all tokens for an agent
    pub fn get_for_agent(&self, agent_id: &crate::AgentId) -> Vec<CapabilityToken> {
        let tokens = self.tokens.read();
        tokens
            .values()
            .filter(|t| t.agent_id == *agent_id)
            .cloned()
            .collect()
    }
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Registry errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryError {
    /// Token not found
    NotFound,
    /// Token not pending approval
    NotPending,
    /// Token not active
    NotActive,
    /// Token expired
    Expired,
    /// Token revoked
    Revoked,
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::NotFound => write!(f, "Token not found"),
            RegistryError::NotPending => write!(f, "Token not pending approval"),
            RegistryError::NotActive => write!(f, "Token not active"),
            RegistryError::Expired => write!(f, "Token expired"),
            RegistryError::Revoked => write!(f, "Token revoked"),
        }
    }
}

impl std::error::Error for RegistryError {}
