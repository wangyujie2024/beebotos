//! Short-Term Memory
//!
//! Limited capacity (7±2 items) with rehearsal support.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::utils::current_timestamp_secs;

/// Short-term memory buffer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortTermMemory {
    buffer: VecDeque<MemoryChunk>,
    capacity: usize,
    rehearsal_counts: std::collections::HashMap<String, u32>,
}

/// Memory chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryChunk {
    pub id: String,
    pub content: String,
    pub timestamp: u64,
    pub priority: Priority,
    pub emotional_tag: Option<EmotionalTag>,
}

/// Priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    Low = 0,
    Medium = 1,
    High = 2,
    Critical = 3,
}

/// Emotional tag
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmotionalTag {
    Positive,
    Negative,
    Neutral,
    Urgent,
}

impl ShortTermMemory {
    /// Default capacity (7±2)
    pub const DEFAULT_CAPACITY: usize = 7;
    pub const MAX_CAPACITY: usize = 9;

    pub fn new() -> Self {
        Self::with_capacity(Self::DEFAULT_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity: capacity.min(Self::MAX_CAPACITY),
            rehearsal_counts: std::collections::HashMap::new(),
        }
    }

    /// Push item to STM (FIFO with priority)
    pub fn push(&mut self, content: impl Into<String>) -> Option<MemoryChunk> {
        self.push_with_priority(content, Priority::Medium)
    }

    /// Push with priority
    pub fn push_with_priority(
        &mut self,
        content: impl Into<String>,
        priority: Priority,
    ) -> Option<MemoryChunk> {
        let chunk = MemoryChunk {
            id: uuid::Uuid::new_v4().to_string(),
            content: content.into(),
            timestamp: current_timestamp_secs(),
            priority,
            emotional_tag: None,
        };

        // If at capacity, remove lowest priority item
        if self.buffer.len() >= self.capacity {
            if let Some(evicted) = self.evict_lowest_priority() {
                self.buffer.push_back(chunk);
                return Some(evicted);
            }
        }

        self.buffer.push_back(chunk);
        None
    }

    /// Rehearse (strengthen) a memory item
    pub fn rehearse(&mut self, id: &str) -> Result<(), MemoryError> {
        if let Some(pos) = self.buffer.iter().position(|c| c.id == id) {
            // Move to front (most recent)
            if let Some(chunk) = self.buffer.remove(pos) {
                self.buffer.push_front(chunk);

                // Increment rehearsal count
                *self.rehearsal_counts.entry(id.to_string()).or_insert(0) += 1;

                Ok(())
            } else {
                Err(MemoryError::ItemNotFound)
            }
        } else {
            Err(MemoryError::ItemNotFound)
        }
    }

    /// Retrieve by content similarity (simplified)
    pub fn retrieve(&self, cue: &str) -> Vec<&MemoryChunk> {
        let cue_lower = cue.to_lowercase();
        self.buffer
            .iter()
            .filter(|c| c.content.to_lowercase().contains(&cue_lower))
            .collect()
    }

    /// Get all items
    pub fn items(&self) -> &VecDeque<MemoryChunk> {
        &self.buffer
    }

    /// Get item count
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Clear all items
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.rehearsal_counts.clear();
    }

    /// Get rehearsal count
    pub fn rehearsal_count(&self, id: &str) -> u32 {
        self.rehearsal_counts.get(id).copied().unwrap_or(0)
    }

    /// Items ready for consolidation (high rehearsal count)
    pub fn ready_for_consolidation(&self, threshold: u32) -> Vec<&MemoryChunk> {
        self.buffer
            .iter()
            .filter(|c| self.rehearsal_count(&c.id) >= threshold)
            .collect()
    }

    fn evict_lowest_priority(&mut self) -> Option<MemoryChunk> {
        // Find lowest priority item
        let min_priority = self.buffer.iter().map(|c| c.priority).min()?;
        let pos = self
            .buffer
            .iter()
            .position(|c| c.priority == min_priority)?;

        let evicted = self.buffer.remove(pos)?;
        self.rehearsal_counts.remove(&evicted.id);
        Some(evicted)
    }

    #[allow(dead_code)]
    fn now() -> u64 {
        current_timestamp_secs()
    }
}

impl Default for ShortTermMemory {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory errors
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MemoryError {
    ItemNotFound,
    CapacityExceeded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capacity_limit() {
        let mut stm = ShortTermMemory::with_capacity(3);
        stm.push("item1");
        stm.push("item2");
        stm.push("item3");

        let evicted = stm.push("item4");
        assert!(evicted.is_some());
        assert_eq!(stm.len(), 3);
    }

    #[test]
    fn test_rehearse() {
        let mut stm = ShortTermMemory::new();
        stm.push("test content");

        let id = stm.items()[0].id.clone();
        assert!(stm.rehearse(&id).is_ok());
        assert_eq!(stm.rehearsal_count(&id), 1);
    }

    #[test]
    fn test_retrieve() {
        let mut stm = ShortTermMemory::new();
        stm.push("hello world");
        stm.push("foo bar");

        let results = stm.retrieve("hello");
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("hello"));
    }
}
