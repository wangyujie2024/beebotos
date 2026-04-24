//! Local Memory

use std::collections::VecDeque;

use uuid::Uuid;

use super::MemoryEntry;
#[allow(unused_imports)]
use crate::error::Result;

/// Local in-memory storage
pub struct LocalMemory {
    entries: VecDeque<MemoryEntry>,
    max_size: usize,
}

impl LocalMemory {
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            max_size,
        }
    }

    pub fn store(&mut self, content: impl Into<String>) -> Uuid {
        let entry = MemoryEntry {
            id: Uuid::new_v4(),
            content: content.into(),
            timestamp: chrono::Utc::now(),
            metadata: std::collections::HashMap::new(),
        };

        let id = entry.id;
        self.entries.push_back(entry);

        // Trim if exceeds max size
        while self.entries.len() > self.max_size {
            self.entries.pop_front();
        }

        id
    }

    pub fn retrieve(&self, id: Uuid) -> Option<&MemoryEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn search(&self, query: &str) -> Vec<&MemoryEntry> {
        self.entries
            .iter()
            .filter(|e| e.content.contains(query))
            .collect()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
