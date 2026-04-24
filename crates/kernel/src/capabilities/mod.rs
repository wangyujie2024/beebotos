//! Capability-Based Security System
//!
//! Implements capability-based access control for agent operations.

pub mod levels;
pub mod registry;
pub mod tokens;

use std::collections::{HashMap, HashSet};

pub use levels::CapabilityLevel;
pub use registry::{CapabilityRegistry, RegistryError};
use serde::{Deserialize, Serialize};
pub use tokens::CapabilityToken;

use crate::error::SecurityError;
use crate::{AgentId, Result};

/// Set of capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitySet {
    /// Maximum capability level
    pub max_level: CapabilityLevel,
    /// Specific permissions
    pub permissions: HashSet<String>,
    /// Time-based expiration
    pub expires_at: Option<u64>,
    /// Delegation allowed
    pub delegable: bool,
}

impl CapabilitySet {
    /// Empty capability set
    pub fn empty() -> Self {
        Self {
            max_level: CapabilityLevel::L0LocalCompute,
            permissions: HashSet::new(),
            expires_at: None,
            delegable: false,
        }
    }

    /// Full capability set (system agent)
    pub fn full() -> Self {
        Self {
            max_level: CapabilityLevel::L10SystemAdmin,
            permissions: ["*"].iter().map(|s| s.to_string()).collect(),
            expires_at: None,
            delegable: true,
        }
    }

    /// Standard agent capabilities
    pub fn standard() -> Self {
        let mut permissions = HashSet::new();
        permissions.insert("compute".to_string());
        permissions.insert("file:read".to_string());
        permissions.insert("network:outbound".to_string());
        permissions.insert("spawn:limited".to_string());

        Self {
            max_level: CapabilityLevel::L5SpawnLimited,
            permissions,
            expires_at: None,
            delegable: false,
        }
    }

    /// With level
    pub fn with_level(mut self, level: CapabilityLevel) -> Self {
        self.max_level = level;
        self
    }

    /// With permission
    pub fn with_permission(mut self, perm: impl Into<String>) -> Self {
        self.permissions.insert(perm.into());
        self
    }

    /// With expiration
    pub fn with_expiration(mut self, expires_at: u64) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// Check if has capability
    pub fn has(&self, level: CapabilityLevel) -> bool {
        if self.is_expired() {
            return false;
        }
        self.max_level >= level
    }

    /// Check if has permission
    pub fn has_permission(&self, perm: &str) -> bool {
        if self.is_expired() {
            return false;
        }
        self.permissions.contains("*") || self.permissions.contains(perm)
    }

    /// Check if expired
    pub fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|exp| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            now > exp
        })
    }

    /// Verify capability
    pub fn verify(&self, required: CapabilityLevel) -> Result<()> {
        if self.is_expired() {
            return Err(SecurityError::CapabilityExpired.into());
        }
        if self.max_level < required {
            return Err(SecurityError::InsufficientCapability {
                required,
                current: self.max_level,
            }
            .into());
        }
        Ok(())
    }

    /// Intersection of two capability sets
    pub fn intersect(&self, other: &Self) -> Self {
        let max_level = self.max_level.min(other.max_level);
        let permissions: HashSet<_> = self
            .permissions
            .intersection(&other.permissions)
            .cloned()
            .collect();

        Self {
            max_level,
            permissions,
            expires_at: self.expires_at.min(other.expires_at),
            delegable: self.delegable && other.delegable,
        }
    }

    /// Union of two capability sets
    pub fn union(&self, other: &Self) -> Self {
        let max_level = self.max_level.max(other.max_level);
        let permissions: HashSet<_> = self
            .permissions
            .union(&other.permissions)
            .cloned()
            .collect();

        Self {
            max_level,
            permissions,
            expires_at: self.expires_at.max(other.expires_at),
            delegable: self.delegable || other.delegable,
        }
    }
}

impl Default for CapabilitySet {
    fn default() -> Self {
        Self::empty()
    }
}

/// Capability request
#[derive(Debug, Clone)]
pub struct CapabilityRequest {
    /// Requested capability level
    pub level: CapabilityLevel,
    /// Request justification
    pub justification: String,
    /// Request duration in seconds
    pub duration_seconds: Option<u64>,
}

/// Capability manager
pub struct CapabilityManager {
    /// Agent capabilities
    agent_caps: HashMap<AgentId, CapabilitySet>,
    /// Registry for capability tokens
    registry: CapabilityRegistry,
}

impl CapabilityManager {
    /// Create new capability manager
    pub fn new() -> Self {
        Self {
            agent_caps: HashMap::new(),
            registry: CapabilityRegistry::new(),
        }
    }

    /// Assign capabilities to agent
    pub fn assign(&mut self, agent_id: AgentId, caps: CapabilitySet) {
        self.agent_caps.insert(agent_id, caps);
    }

    /// Get agent capabilities
    pub fn get(&self, agent_id: &AgentId) -> Option<&CapabilitySet> {
        self.agent_caps.get(agent_id)
    }

    /// Revoke agent capabilities
    pub fn revoke(&mut self, agent_id: &AgentId) -> Option<CapabilitySet> {
        self.agent_caps.remove(agent_id)
    }

    /// Check if agent has capability
    pub fn check(&self, agent_id: &AgentId, level: CapabilityLevel) -> Result<()> {
        let caps = self
            .agent_caps
            .get(agent_id)
            .ok_or(SecurityError::NoCapabilities)?;
        caps.verify(level)
    }

    /// Request elevated capability
    pub fn request_elevation(
        &mut self,
        agent_id: AgentId,
        request: CapabilityRequest,
    ) -> Result<CapabilityToken> {
        let current = self
            .agent_caps
            .get(&agent_id)
            .ok_or(SecurityError::NoCapabilities)?;

        if current.has(request.level) {
            // Already has capability
            return Ok(CapabilityToken::new(
                agent_id,
                request.level,
                request.duration_seconds,
            ));
        }

        // Requires approval - create pending token
        let token = CapabilityToken::new_pending(
            agent_id,
            request.level,
            request.justification,
            request.duration_seconds,
        );

        self.registry.register(token.clone());

        Ok(token)
    }

    /// Approve capability elevation
    pub fn approve_elevation(&mut self, token_id: &str) -> Result<CapabilityToken> {
        self.registry.approve(token_id).map_err(|e| match e {
            RegistryError::NotFound => crate::error::KernelError::InvalidCapability,
            RegistryError::NotPending => crate::error::KernelError::InvalidCapability,
            RegistryError::NotActive => crate::error::KernelError::InvalidCapability,
            RegistryError::Expired => crate::error::KernelError::CapabilityExpired,
            RegistryError::Revoked => crate::error::KernelError::InvalidCapability,
        })
    }

    /// Verify token
    pub fn verify_token(&self, token: &CapabilityToken) -> Result<()> {
        self.registry.verify(token).map_err(|e| match e {
            RegistryError::NotFound => crate::error::KernelError::InvalidCapability,
            RegistryError::NotPending => crate::error::KernelError::InvalidCapability,
            RegistryError::NotActive => crate::error::KernelError::InvalidCapability,
            RegistryError::Expired => crate::error::KernelError::CapabilityExpired,
            RegistryError::Revoked => crate::error::KernelError::InvalidCapability,
        })
    }

    /// Clean up expired capabilities
    pub fn cleanup_expired(&mut self) {
        self.agent_caps.retain(|_, caps| !caps.is_expired());
        self.registry.cleanup_expired();
    }
}

impl Default for CapabilityManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Attenuation - reduce capabilities
pub fn attenuate(caps: &CapabilitySet, max_level: CapabilityLevel) -> CapabilitySet {
    caps.clone().with_level(caps.max_level.min(max_level))
}

/// Delegation - transfer capabilities
pub fn delegate(
    caps: &CapabilitySet,
    _to: AgentId,
    max_level: CapabilityLevel,
) -> Result<CapabilitySet> {
    if !caps.delegable {
        return Err(SecurityError::NotDelegable.into());
    }

    Ok(attenuate(caps, max_level))
}
