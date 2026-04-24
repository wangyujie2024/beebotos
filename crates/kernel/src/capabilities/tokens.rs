//! Capability Tokens
//!
//! Tokens for temporary capability elevation.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::CapabilityLevel;
use crate::AgentId;

/// Capability token for temporary elevation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityToken {
    /// Token ID
    pub id: String,
    /// Agent ID
    pub agent_id: AgentId,
    /// Granted capability level
    pub level: CapabilityLevel,
    /// Creation timestamp
    pub created_at: u64,
    /// Expiration timestamp (None for permanent)
    pub expires_at: Option<u64>,
    /// Token status
    pub status: TokenStatus,
    /// Justification for elevation
    pub justification: String,
}

/// Token status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenStatus {
    /// Pending approval
    Pending,
    /// Approved and active
    Active,
    /// Expired
    Expired,
    /// Revoked
    Revoked,
}

impl CapabilityToken {
    /// Create new active token
    pub fn new(agent_id: AgentId, level: CapabilityLevel, duration_secs: Option<u64>) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            agent_id,
            level,
            created_at: now,
            expires_at: duration_secs.map(|d| now + d),
            status: TokenStatus::Active,
            justification: String::new(),
        }
    }

    /// Create pending token
    pub fn new_pending(
        agent_id: AgentId,
        level: CapabilityLevel,
        justification: String,
        duration_secs: Option<u64>,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            agent_id,
            level,
            created_at: now,
            expires_at: duration_secs.map(|d| now + d),
            status: TokenStatus::Pending,
            justification,
        }
    }

    /// Get token ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Check if token is valid
    pub fn is_valid(&self) -> bool {
        if self.status != TokenStatus::Active {
            return false;
        }

        if let Some(exp) = self.expires_at {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            return now < exp;
        }

        true
    }

    /// Check if expired
    pub fn is_expired(&self) -> bool {
        if let Some(exp) = self.expires_at {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            return now >= exp;
        }
        false
    }

    /// Approve pending token
    pub fn approve(&mut self) {
        if self.status == TokenStatus::Pending {
            self.status = TokenStatus::Active;
        }
    }

    /// Revoke token
    pub fn revoke(&mut self) {
        self.status = TokenStatus::Revoked;
    }
}
