//! Resource Management Module
//!
//! Provides resource tracking, limiting, and monitoring for agents.
//!
//! ## Submodules
//!
//! - `cgroup`: Control group management
//! - `circuit_breaker`: Circuit breaker pattern for fault tolerance
//! - `limit`: Resource limit enforcement
//! - `metrics`: Resource metrics collection
//! - `monitor`: Resource monitoring and alerts

pub mod cgroup;
pub mod circuit_breaker;
pub mod limit;
pub mod metrics;
pub mod monitor;

use std::collections::HashMap;
use std::time::Duration;

pub use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitState, CircuitStats};
pub use metrics::{MetricsCollector, Timer};
use serde::{Deserialize, Serialize};

/// Resource usage statistics for a process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsage {
    /// CPU time consumed
    pub cpu_time: Duration,
    /// Memory usage in bytes
    pub memory_bytes: u64,
    /// Bytes read from storage
    pub io_read_bytes: u64,
    /// Bytes written to storage
    pub io_write_bytes: u64,
    /// Bytes received from network
    pub network_rx_bytes: u64,
    /// Bytes sent over network
    pub network_tx_bytes: u64,
}

impl Default for ResourceUsage {
    fn default() -> Self {
        Self {
            cpu_time: Duration::ZERO,
            memory_bytes: 0,
            io_read_bytes: 0,
            io_write_bytes: 0,
            network_rx_bytes: 0,
            network_tx_bytes: 0,
        }
    }
}

impl ResourceUsage {
    /// Create new resource usage with zero values
    pub fn new() -> Self {
        Self::default()
    }

    /// Add other usage to this one
    pub fn add(&mut self, other: &ResourceUsage) {
        self.cpu_time += other.cpu_time;
        self.memory_bytes = self.memory_bytes.max(other.memory_bytes);
        self.io_read_bytes += other.io_read_bytes;
        self.io_write_bytes += other.io_write_bytes;
        self.network_rx_bytes += other.network_rx_bytes;
        self.network_tx_bytes += other.network_tx_bytes;
    }
}

/// Resource limits for a process
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceLimits {
    /// Maximum CPU time allowed
    pub max_cpu_time: Option<Duration>,
    /// Maximum memory usage in bytes
    pub max_memory_bytes: Option<u64>,
    /// Maximum IO bytes (read + write)
    pub max_io_bytes: Option<u64>,
    /// Maximum network bytes (rx + tx)
    pub max_network_bytes: Option<u64>,
    /// Maximum number of file descriptors
    pub max_file_descriptors: Option<u32>,
    /// Maximum number of child processes
    pub max_processes: Option<u32>,
}

impl ResourceLimits {
    /// Create limits with no restrictions
    pub fn none() -> Self {
        Self {
            max_cpu_time: None,
            max_memory_bytes: None,
            max_io_bytes: None,
            max_network_bytes: None,
            max_file_descriptors: None,
            max_processes: None,
        }
    }

    /// Create default resource limits
    pub fn default() -> Self {
        Self {
            max_cpu_time: Some(Duration::from_secs(3600)),
            max_memory_bytes: Some(1024 * 1024 * 1024), // 1GB
            max_io_bytes: Some(1024 * 1024 * 1024 * 10), // 10GB
            max_network_bytes: Some(1024 * 1024 * 1024 * 10), // 10GB
            max_file_descriptors: Some(1024),
            max_processes: Some(100),
        }
    }

    /// Check if usage exceeds limits
    pub fn check_usage(&self, usage: &ResourceUsage) -> ResourceStatus {
        if let Some(max_cpu) = self.max_cpu_time {
            if usage.cpu_time > max_cpu {
                return ResourceStatus::Exceeded(ResourceType::CpuTime);
            }
        }

        if let Some(max_mem) = self.max_memory_bytes {
            if usage.memory_bytes > max_mem {
                return ResourceStatus::Exceeded(ResourceType::Memory);
            }
        }

        if let Some(max_io) = self.max_io_bytes {
            if usage.io_read_bytes + usage.io_write_bytes > max_io {
                return ResourceStatus::Exceeded(ResourceType::Io);
            }
        }

        if let Some(max_net) = self.max_network_bytes {
            if usage.network_rx_bytes + usage.network_tx_bytes > max_net {
                return ResourceStatus::Exceeded(ResourceType::Network);
            }
        }

        ResourceStatus::WithinLimits
    }
}

/// Status of resource usage against limits
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResourceStatus {
    /// Usage is within configured limits
    WithinLimits,
    /// Limit has been exceeded
    Exceeded(ResourceType),
    /// Approaching limit (with percentage)
    Warning(ResourceType, f32),
}

/// Types of resources that can be limited
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    /// CPU time resource
    CpuTime,
    /// Memory resource
    Memory,
    /// IO operations
    Io,
    /// Network operations
    Network,
    /// File descriptor count
    FileDescriptors,
    /// Process count
    Processes,
}

/// Manages resource limits and usage tracking
pub struct ResourceManager {
    /// Usage statistics by process ID
    process_usage: HashMap<u32, ResourceUsage>,
    /// Per-process resource limits
    process_limits: HashMap<u32, ResourceLimits>,
    /// Global resource limits
    global_limits: ResourceLimits,
}

impl ResourceManager {
    /// Create new resource manager with global limits
    pub fn new(global_limits: ResourceLimits) -> Self {
        Self {
            process_usage: HashMap::new(),
            process_limits: HashMap::new(),
            global_limits,
        }
    }

    /// Set resource limits for process
    pub fn set_process_limits(&mut self, pid: u32, limits: ResourceLimits) {
        self.process_limits.insert(pid, limits);
    }

    /// Get resource limits for process
    pub fn get_process_limits(&self, pid: u32) -> Option<&ResourceLimits> {
        self.process_limits.get(&pid)
    }

    /// Update resource usage for process
    pub fn update_usage(&mut self, pid: u32, usage: ResourceUsage) {
        self.process_usage.insert(pid, usage);
    }

    /// Get resource usage for process
    pub fn get_usage(&self, pid: u32) -> Option<&ResourceUsage> {
        self.process_usage.get(&pid)
    }

    /// Check if process resource usage is within limits
    pub fn check_process_resources(&self, pid: u32) -> ResourceStatus {
        let usage = match self.process_usage.get(&pid) {
            Some(u) => u,
            None => return ResourceStatus::WithinLimits,
        };

        if let Some(limits) = self.process_limits.get(&pid) {
            let status = limits.check_usage(usage);
            if status != ResourceStatus::WithinLimits {
                return status;
            }
        }

        self.global_limits.check_usage(usage)
    }

    /// Clean up process tracking data
    pub fn cleanup_process(&mut self, pid: u32) {
        self.process_usage.remove(&pid);
        self.process_limits.remove(&pid);
    }

    /// Get aggregated usage across all processes
    pub fn get_global_usage(&self) -> ResourceUsage {
        let mut total = ResourceUsage::new();
        for usage in self.process_usage.values() {
            total.add(usage);
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_limits_check() {
        let limits = ResourceLimits {
            max_cpu_time: Some(Duration::from_secs(60)),
            max_memory_bytes: Some(1024 * 1024),
            max_io_bytes: None,
            max_network_bytes: None,
            max_file_descriptors: None,
            max_processes: None,
        };

        let mut usage = ResourceUsage::new();
        usage.cpu_time = Duration::from_secs(30);
        usage.memory_bytes = 512 * 1024;

        assert_eq!(limits.check_usage(&usage), ResourceStatus::WithinLimits);

        usage.cpu_time = Duration::from_secs(90);
        assert_eq!(
            limits.check_usage(&usage),
            ResourceStatus::Exceeded(ResourceType::CpuTime)
        );
    }
}
