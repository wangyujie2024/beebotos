//! Sandbox for agent isolation

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// Sandbox configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Sandbox identifier
    pub id: String,
    /// Allowed syscall numbers
    pub allowed_syscalls: HashSet<u64>,
    /// Maximum memory in bytes
    pub max_memory: usize,
    /// Maximum CPU time in milliseconds
    pub max_cpu_time_ms: u64,
    /// Whether network access is allowed
    pub network_allowed: bool,
    /// Whether filesystem access is allowed
    pub filesystem_allowed: bool,
}

impl SandboxConfig {
    /// Create restrictive sandbox
    pub fn restrictive(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            allowed_syscalls: [0, 1, 2, 10, 11].iter().copied().collect(),
            max_memory: 64 * 1024 * 1024, // 64MB
            max_cpu_time_ms: 30000,       // 30 seconds
            network_allowed: false,
            filesystem_allowed: false,
        }
    }

    /// Create standard sandbox
    pub fn standard(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            allowed_syscalls: [0, 1, 2, 3, 10, 11, 12, 20, 21, 30, 31, 32]
                .iter()
                .copied()
                .collect(),
            max_memory: 256 * 1024 * 1024, // 256MB
            max_cpu_time_ms: 300000,       // 5 minutes
            network_allowed: true,
            filesystem_allowed: true,
        }
    }

    /// Check if syscall is allowed in this sandbox
    pub fn is_syscall_allowed(&self, syscall: u64) -> bool {
        self.allowed_syscalls.contains(&syscall)
    }
}

/// Sandbox instance
pub struct Sandbox {
    /// Sandbox configuration
    pub config: SandboxConfig,
    /// Current memory usage in bytes
    pub memory_used: usize,
    /// Current CPU time used in milliseconds
    pub cpu_time_used_ms: u64,
}

impl Sandbox {
    /// Create new sandbox
    pub fn new(config: SandboxConfig) -> Self {
        Self {
            config,
            memory_used: 0,
            cpu_time_used_ms: 0,
        }
    }

    /// Check if syscall allowed
    pub fn is_syscall_allowed(&self, syscall: u64) -> bool {
        self.config.allowed_syscalls.contains(&syscall)
    }

    /// Check memory limit
    pub fn check_memory(&self, requested: usize) -> bool {
        self.memory_used + requested <= self.config.max_memory
    }

    /// Check CPU time limit
    pub fn check_cpu_time(&self, additional_ms: u64) -> bool {
        self.cpu_time_used_ms + additional_ms <= self.config.max_cpu_time_ms
    }
}
