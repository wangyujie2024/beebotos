//! Memory Synchronization

use std::collections::HashMap;

#[allow(unused_imports)]
use uuid::Uuid;

#[allow(unused_imports)]
use super::MemoryEntry;
#[allow(unused_imports)]
use crate::error::Result;

/// Sync coordinator for distributed memory
pub struct MemorySync {
    local_version: u64,
    remote_versions: HashMap<String, u64>,
}

impl MemorySync {
    pub fn new() -> Self {
        Self {
            local_version: 0,
            remote_versions: HashMap::new(),
        }
    }

    pub fn increment_version(&mut self) {
        self.local_version += 1;
    }

    pub fn get_version(&self) -> u64 {
        self.local_version
    }

    pub fn update_remote(&mut self, node: impl Into<String>, version: u64) {
        self.remote_versions.insert(node.into(), version);
    }

    pub fn needs_sync(&self, node: &str) -> bool {
        self.remote_versions
            .get(node)
            .map(|&v| v < self.local_version)
            .unwrap_or(true)
    }
}

impl Default for MemorySync {
    fn default() -> Self {
        Self::new()
    }
}
