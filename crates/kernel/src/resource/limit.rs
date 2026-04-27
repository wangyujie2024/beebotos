//! Resource limit management

use std::time::Duration;

use super::{ResourceLimits, ResourceType};

/// Resource limit manager
#[derive(Debug, Clone)]
pub struct LimitManager {
    /// Resource limits
    limits: ResourceLimits,
}

impl LimitManager {
    /// Create new limit manager with default limits
    pub fn new(limits: ResourceLimits) -> Self {
        Self { limits }
    }

    /// Get current limits
    pub fn limits(&self) -> &ResourceLimits {
        &self.limits
    }

    /// Update limits
    pub fn set_limits(&mut self, limits: ResourceLimits) {
        self.limits = limits;
    }

    /// Check if CPU time is within limit
    pub fn check_cpu(&self, used: Duration) -> bool {
        self.limits
            .max_cpu_time
            .map(|max| used <= max)
            .unwrap_or(true)
    }

    /// Check if memory is within limit
    pub fn check_memory(&self, used: u64) -> bool {
        self.limits
            .max_memory_bytes
            .map(|max| used <= max)
            .unwrap_or(true)
    }

    /// Check if IO is within limit
    pub fn check_io(&self, read: u64, written: u64) -> bool {
        self.limits
            .max_io_bytes
            .map(|max| read + written <= max)
            .unwrap_or(true)
    }

    /// Get limit for specific resource type
    pub fn get_limit(&self, resource_type: ResourceType) -> Option<u64> {
        match resource_type {
            ResourceType::Memory => self.limits.max_memory_bytes,
            ResourceType::Io => self.limits.max_io_bytes,
            ResourceType::Network => self.limits.max_network_bytes,
            ResourceType::FileDescriptors => self.limits.max_file_descriptors.map(|v| v as u64),
            ResourceType::Processes => self.limits.max_processes.map(|v| v as u64),
            ResourceType::CpuTime => self.limits.max_cpu_time.map(|v| v.as_secs()),
        }
    }
}

impl Default for LimitManager {
    fn default() -> Self {
        Self::new(ResourceLimits::default())
    }
}
