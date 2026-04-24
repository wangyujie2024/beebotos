//! Capability Levels - 11-tier security model

use std::fmt;

use serde::{Deserialize, Serialize};

/// Capability levels from L0 (lowest) to L10 (highest)
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[repr(u8)]
#[allow(non_camel_case_types)]
pub enum CapabilityLevel {
    /// L0: Local computation only (sandboxed)
    #[default]
    L0LocalCompute = 0,
    /// L1: Read file system (read-only)
    L1FileRead = 1,
    /// L2: Write file system
    L2FileWrite = 2,
    /// L3: Outbound network access
    L3NetworkOut = 3,
    /// L4: Inbound network access (server)
    L4NetworkIn = 4,
    /// L5: Spawn limited child agents
    L5SpawnLimited = 5,
    /// L6: Spawn unlimited child agents
    L6SpawnUnlimited = 6,
    /// L7: Read blockchain state
    L7ChainRead = 7,
    /// L8: Write blockchain (low value)
    L8ChainWriteLow = 8,
    /// L9: Write blockchain (high value)
    L9ChainWriteHigh = 9,
    /// L10: System administration
    L10SystemAdmin = 10,
}

impl CapabilityLevel {
    /// Get level as number
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }

    /// Create from number
    pub fn from_u8(n: u8) -> Option<Self> {
        match n {
            0 => Some(Self::L0LocalCompute),
            1 => Some(Self::L1FileRead),
            2 => Some(Self::L2FileWrite),
            3 => Some(Self::L3NetworkOut),
            4 => Some(Self::L4NetworkIn),
            5 => Some(Self::L5SpawnLimited),
            6 => Some(Self::L6SpawnUnlimited),
            7 => Some(Self::L7ChainRead),
            8 => Some(Self::L8ChainWriteLow),
            9 => Some(Self::L9ChainWriteHigh),
            10 => Some(Self::L10SystemAdmin),
            _ => None,
        }
    }

    /// Get level name
    pub fn name(&self) -> &'static str {
        match self {
            Self::L0LocalCompute => "Local Compute",
            Self::L1FileRead => "File Read",
            Self::L2FileWrite => "File Write",
            Self::L3NetworkOut => "Network Outbound",
            Self::L4NetworkIn => "Network Inbound",
            Self::L5SpawnLimited => "Spawn Limited",
            Self::L6SpawnUnlimited => "Spawn Unlimited",
            Self::L7ChainRead => "Chain Read",
            Self::L8ChainWriteLow => "Chain Write Low",
            Self::L9ChainWriteHigh => "Chain Write High",
            Self::L10SystemAdmin => "System Admin",
        }
    }

    /// Get description
    pub fn description(&self) -> &'static str {
        match self {
            Self::L0LocalCompute => "Pure local computation, no external access",
            Self::L1FileRead => "Read-only access to filesystem",
            Self::L2FileWrite => "Read and write access to filesystem",
            Self::L3NetworkOut => "Make outbound network connections",
            Self::L4NetworkIn => "Accept inbound network connections",
            Self::L5SpawnLimited => "Spawn up to 10 child agents",
            Self::L6SpawnUnlimited => "Spawn unlimited child agents",
            Self::L7ChainRead => "Read blockchain state and events",
            Self::L8ChainWriteLow => "Execute low-value transactions (< 1 ETH)",
            Self::L9ChainWriteHigh => "Execute high-value transactions",
            Self::L10SystemAdmin => "Full system control",
        }
    }

    /// Check if level requires TEE
    pub fn requires_tee(&self) -> bool {
        matches!(self, Self::L9ChainWriteHigh | Self::L10SystemAdmin)
    }

    /// Check if level requires multi-sig
    pub fn requires_multisig(&self) -> bool {
        matches!(self, Self::L9ChainWriteHigh)
    }

    /// Get recommended timeout for this level
    pub fn recommended_timeout(&self) -> std::time::Duration {
        match self {
            Self::L0LocalCompute => std::time::Duration::from_secs(30),
            Self::L1FileRead => std::time::Duration::from_secs(60),
            Self::L2FileWrite => std::time::Duration::from_secs(60),
            Self::L3NetworkOut => std::time::Duration::from_secs(120),
            Self::L4NetworkIn => std::time::Duration::from_secs(120),
            Self::L5SpawnLimited => std::time::Duration::from_secs(300),
            Self::L6SpawnUnlimited => std::time::Duration::from_secs(600),
            Self::L7ChainRead => std::time::Duration::from_secs(60),
            Self::L8ChainWriteLow => std::time::Duration::from_secs(180),
            Self::L9ChainWriteHigh => std::time::Duration::from_secs(300),
            Self::L10SystemAdmin => std::time::Duration::from_secs(600),
        }
    }

    /// Get maximum value at risk for this level
    pub fn max_value_at_risk(&self) -> u64 {
        match self {
            Self::L0LocalCompute => 0,
            Self::L1FileRead => 0,
            Self::L2FileWrite => 0,
            Self::L3NetworkOut => 0,
            Self::L4NetworkIn => 0,
            Self::L5SpawnLimited => 0,
            Self::L6SpawnUnlimited => 0,
            Self::L7ChainRead => 0,
            Self::L8ChainWriteLow => 1_000_000_000_000_000_000, // 1 ETH
            Self::L9ChainWriteHigh => u64::MAX,
            Self::L10SystemAdmin => u64::MAX,
        }
    }

    /// Get associated syscalls for this level
    pub fn syscalls(&self) -> &'static [u64] {
        match self {
            Self::L0LocalCompute => &[0, 1, 2, 3], // Basic compute
            Self::L1FileRead => &[0, 1, 2, 3, 10, 11, 12],
            Self::L2FileWrite => &[0, 1, 2, 3, 10, 11, 12, 13, 14],
            Self::L3NetworkOut => &[0, 1, 2, 3, 10, 11, 12, 13, 14, 20, 21],
            Self::L4NetworkIn => &[0, 1, 2, 3, 10, 11, 12, 13, 14, 20, 21, 22, 23],
            Self::L5SpawnLimited => &[0, 1, 2, 3, 10, 11, 12, 13, 14, 20, 21, 22, 23, 30, 31],
            Self::L6SpawnUnlimited => &[0, 1, 2, 3, 10, 11, 12, 13, 14, 20, 21, 22, 23, 30, 31],
            Self::L7ChainRead => &[
                0, 1, 2, 3, 10, 11, 12, 13, 14, 20, 21, 22, 23, 30, 31, 40, 41, 42,
            ],
            Self::L8ChainWriteLow => &[
                0, 1, 2, 3, 10, 11, 12, 13, 14, 20, 21, 22, 23, 30, 31, 40, 41, 42, 43, 44,
            ],
            Self::L9ChainWriteHigh => &[
                0, 1, 2, 3, 10, 11, 12, 13, 14, 20, 21, 22, 23, 30, 31, 40, 41, 42, 43, 44, 45,
            ],
            Self::L10SystemAdmin => &[
                0, 1, 2, 3, 10, 11, 12, 13, 14, 20, 21, 22, 23, 30, 31, 40, 41, 42, 43, 44, 45, 50,
                51, 52,
            ],
        }
    }
}

impl fmt::Display for CapabilityLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (L{})", self.name(), self.as_u8())
    }
}

/// Time-decaying capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecayingCapability {
    /// The capability level
    pub level: CapabilityLevel,
    /// Timestamp when capability was granted
    pub granted_at: u64,
    /// Rate at which the capability decays
    pub decay_rate: DecayRate,
}

impl DecayingCapability {
    /// Create new decaying capability
    pub fn new(level: CapabilityLevel, decay_rate: DecayRate) -> Self {
        Self {
            level,
            granted_at: now(),
            decay_rate,
        }
    }

    /// Get current effective level
    pub fn current_level(&self) -> CapabilityLevel {
        let elapsed = now() - self.granted_at;
        let decay_levels = match self.decay_rate {
            DecayRate::Slow => elapsed / 86400,  // 1 day per level
            DecayRate::Normal => elapsed / 3600, // 1 hour per level
            DecayRate::Fast => elapsed / 600,    // 10 minutes per level
        };

        let current = self.level.as_u8().saturating_sub(decay_levels as u8);
        CapabilityLevel::from_u8(current).unwrap_or(CapabilityLevel::L0LocalCompute)
    }

    /// Check if completely decayed
    pub fn is_expired(&self) -> bool {
        self.current_level() == CapabilityLevel::L0LocalCompute
    }

    /// Refresh capability
    pub fn refresh(&mut self) {
        self.granted_at = now();
    }
}

/// Decay rate for capabilities
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DecayRate {
    /// Slow decay (1 level per day)
    Slow,
    /// Normal decay (1 level per hour)
    Normal,
    /// Fast decay (1 level per 10 minutes)
    Fast,
}

/// Capability escalation
#[derive(Debug, Clone)]
pub struct Escalation {
    /// Original capability level
    pub from: CapabilityLevel,
    /// Target capability level
    pub to: CapabilityLevel,
    /// Reason for escalation
    pub reason: String,
    /// List of approvers for this escalation
    pub approved_by: Vec<String>,
    /// Timestamp when escalation expires
    pub expires_at: u64,
}

impl Escalation {
    /// Check if escalation is active
    pub fn is_active(&self) -> bool {
        now() < self.expires_at
    }

    /// Check if properly approved
    pub fn is_approved(&self, required_approvals: usize) -> bool {
        self.approved_by.len() >= required_approvals
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level_ordering() {
        assert!(CapabilityLevel::L10SystemAdmin > CapabilityLevel::L0LocalCompute);
        assert!(CapabilityLevel::L5SpawnLimited > CapabilityLevel::L4NetworkIn);
    }

    #[test]
    fn test_from_u8() {
        assert_eq!(
            CapabilityLevel::from_u8(5),
            Some(CapabilityLevel::L5SpawnLimited)
        );
        assert_eq!(CapabilityLevel::from_u8(99), None);
    }

    #[test]
    fn test_decay() {
        let cap = DecayingCapability::new(CapabilityLevel::L5SpawnLimited, DecayRate::Fast);

        // Initially at L5
        assert_eq!(cap.current_level(), CapabilityLevel::L5SpawnLimited);
    }
}
