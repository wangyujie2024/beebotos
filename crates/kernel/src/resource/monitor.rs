//! Resource monitoring

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;

use super::{ResourceLimits, ResourceStatus, ResourceUsage};

/// Resource monitor for tracking usage
#[derive(Debug, Clone)]
pub struct ResourceMonitor {
    /// Usage history
    usage_history: Arc<Mutex<HashMap<u32, Vec<ResourceUsage>>>>,
    /// Resource limits
    limits: ResourceLimits,
}

impl ResourceMonitor {
    /// Create new resource monitor
    pub fn new(limits: ResourceLimits) -> Self {
        Self {
            usage_history: Arc::new(Mutex::new(HashMap::new())),
            limits,
        }
    }

    /// Record usage snapshot for a process
    pub fn record(&self, pid: u32, usage: ResourceUsage) {
        let mut history = self.usage_history.lock();
        history.entry(pid).or_insert_with(Vec::new).push(usage);
    }

    /// Get usage history for a process
    pub fn get_history(&self, pid: u32) -> Option<Vec<ResourceUsage>> {
        let history = self.usage_history.lock();
        history.get(&pid).cloned()
    }

    /// Check if usage exceeds limits
    pub fn check_limits(&self, usage: &ResourceUsage) -> ResourceStatus {
        self.limits.check_usage(usage)
    }

    /// Get current limits
    pub fn limits(&self) -> &ResourceLimits {
        &self.limits
    }

    /// Clear history for a process
    pub fn clear_history(&self, pid: u32) {
        let mut history = self.usage_history.lock();
        history.remove(&pid);
    }

    /// Get all monitored process IDs
    pub fn process_ids(&self) -> Vec<u32> {
        let history = self.usage_history.lock();
        history.keys().copied().collect()
    }
}

impl Default for ResourceMonitor {
    fn default() -> Self {
        Self::new(ResourceLimits::default())
    }
}
