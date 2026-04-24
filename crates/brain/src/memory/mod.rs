//! Memory Module
//!
//! Multi-modal memory system:
//! - Short-term memory (7±2 items)
//! - Episodic memory (events with spatiotemporal context)
//! - Semantic memory (concepts and facts)
//! - Procedural memory (skills and procedures)
//! - Consolidation (sleep mode)

pub mod consolidation;
pub mod embeddings;
pub mod episodic;
pub mod index;
pub mod procedural;
pub mod semantic;
pub mod short_term;

pub use consolidation::{ConsolidationConfig, ConsolidationEngine};
pub use episodic::{EmotionalValence, Episode, EpisodicMemory, Location};
pub use procedural::{ProceduralMemory, Procedure, Step};
pub use semantic::{Concept, PropertyValue, Relation, RelationType, SemanticMemory};
use serde::{Deserialize, Serialize};
pub use short_term::{EmotionalTag, MemoryChunk, Priority, ShortTermMemory};

// MemoryIndex is re-exported from lib.rs directly
use crate::error::BrainResult;

/// Memory query for searching across all memory types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQuery {
    /// Search text/content
    pub query: String,
    /// Memory types to search
    pub memory_types: Vec<MemoryType>,
    /// Time range filter (start, end)
    pub time_range: Option<(u64, u64)>,
    /// Location filter
    pub location: Option<String>,
    /// Minimum importance/confidence threshold
    pub min_importance: f32,
    /// Maximum number of results
    pub limit: usize,
    /// Emotional valence filter
    pub emotional_filter: Option<EmotionalFilter>,
}

/// Types of memory to query
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryType {
    ShortTerm,
    Episodic,
    Semantic,
    Procedural,
}

/// Emotional filter for memory queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionalFilter {
    pub emotion: String,
    pub min_intensity: f32,
    pub max_intensity: f32,
}

impl MemoryQuery {
    /// Create a new memory query
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            memory_types: vec![
                MemoryType::ShortTerm,
                MemoryType::Episodic,
                MemoryType::Semantic,
            ],
            time_range: None,
            location: None,
            min_importance: 0.0,
            limit: 10,
            emotional_filter: None,
        }
    }

    /// Filter by memory types
    pub fn with_types(mut self, types: Vec<MemoryType>) -> Self {
        self.memory_types = types;
        self
    }

    /// Filter by time range
    pub fn with_time_range(mut self, start: u64, end: u64) -> Self {
        self.time_range = Some((start, end));
        self
    }

    /// Filter by location
    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }

    /// Set minimum importance
    pub fn with_min_importance(mut self, importance: f32) -> Self {
        self.min_importance = importance.clamp(0.0, 1.0);
        self
    }

    /// Set result limit
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Filter by emotion
    pub fn with_emotion(
        mut self,
        emotion: impl Into<String>,
        min_intensity: f32,
        max_intensity: f32,
    ) -> Self {
        self.emotional_filter = Some(EmotionalFilter {
            emotion: emotion.into(),
            min_intensity: min_intensity.clamp(0.0, 1.0),
            max_intensity: max_intensity.clamp(0.0, 1.0),
        });
        self
    }
}

/// Unified memory system managing all memory types
pub struct UnifiedMemory {
    pub short_term: ShortTermMemory,
    pub episodic: EpisodicMemory,
    pub semantic: SemanticMemory,
    pub procedural: ProceduralMemory,
    pub consolidation: ConsolidationEngine,
}

impl UnifiedMemory {
    /// Create new unified memory system
    pub fn new() -> Self {
        Self {
            short_term: ShortTermMemory::new(),
            episodic: EpisodicMemory::new(),
            semantic: SemanticMemory::new(),
            procedural: ProceduralMemory::new(),
            consolidation: ConsolidationEngine::new(ConsolidationConfig::default()),
        }
    }

    /// Query across all memory types
    pub fn query(&self, query: &MemoryQuery) -> BrainResult<MemoryResults> {
        let mut results = MemoryResults::default();

        for memory_type in &query.memory_types {
            match memory_type {
                MemoryType::ShortTerm => {
                    // Search short-term memory
                    let stm_results: Vec<String> = self
                        .short_term
                        .retrieve(&query.query)
                        .into_iter()
                        .map(|chunk| chunk.content.clone())
                        .take(query.limit)
                        .collect();
                    results.short_term = stm_results;
                }
                MemoryType::Episodic => {
                    // Search episodic memory
                    let episodes = self.episodic.search(&query.query);
                    results.episodic = episodes
                        .into_iter()
                        .filter(|e| e.importance >= query.min_importance)
                        .take(query.limit)
                        .map(|e| e.what.clone())
                        .collect();
                }
                MemoryType::Semantic => {
                    // Search semantic memory
                    if let Some(concept) = self.semantic.find_by_name(&query.query) {
                        results
                            .semantic
                            .push(format!("{}: {}", concept.name, concept.definition));
                    }
                }
                MemoryType::Procedural => {
                    // Search procedural memory
                    let procedures = self.procedural.search(&query.query);
                    results.procedural = procedures
                        .into_iter()
                        .take(query.limit)
                        .map(|p| p.name.clone())
                        .collect();
                }
            }
        }

        Ok(results)
    }

    /// Consolidate memories (sleep mode)
    pub fn consolidate(&mut self) -> BrainResult<usize> {
        let threshold = 3; // Minimum rehearsal count
        let ready = self.short_term.ready_for_consolidation(threshold);
        let count = ready.len();

        for chunk in ready {
            // Move from STM to episodic/semantic based on content
            if chunk.content.len() > 50 {
                // Create episode for complex memories
                let _id = self
                    .episodic
                    .encode(chunk.content.clone(), Self::now(), None);
            }
        }

        Ok(count)
    }

    fn now() -> u64 {
        crate::utils::current_timestamp_secs()
    }
}

impl Default for UnifiedMemory {
    fn default() -> Self {
        Self::new()
    }
}

/// Results from a memory query
#[derive(Debug, Clone, Default)]
pub struct MemoryResults {
    pub short_term: Vec<String>,
    pub episodic: Vec<String>,
    pub semantic: Vec<String>,
    pub procedural: Vec<String>,
}

impl MemoryResults {
    /// Check if any results were found
    pub fn is_empty(&self) -> bool {
        self.short_term.is_empty()
            && self.episodic.is_empty()
            && self.semantic.is_empty()
            && self.procedural.is_empty()
    }

    /// Get total result count
    pub fn total_count(&self) -> usize {
        self.short_term.len() + self.episodic.len() + self.semantic.len() + self.procedural.len()
    }

    /// Merge all results into a single list
    pub fn all_results(&self) -> Vec<String> {
        let mut all = Vec::new();
        all.extend(self.short_term.clone());
        all.extend(self.episodic.clone());
        all.extend(self.semantic.clone());
        all.extend(self.procedural.clone());
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_query_builder() {
        let query = MemoryQuery::new("test")
            .with_limit(5)
            .with_min_importance(0.5)
            .with_location("room1");

        assert_eq!(query.query, "test");
        assert_eq!(query.limit, 5);
        assert_eq!(query.min_importance, 0.5);
        assert_eq!(query.location, Some("room1".to_string()));
    }

    #[test]
    fn test_memory_results() {
        let results = MemoryResults {
            short_term: vec!["item1".to_string()],
            episodic: vec!["episode1".to_string()],
            semantic: vec![],
            procedural: vec![],
        };

        assert!(!results.is_empty());
        assert_eq!(results.total_count(), 2);
    }

    #[test]
    fn test_unified_memory_creation() {
        let memory = UnifiedMemory::new();
        assert!(memory.short_term.is_empty());
        assert!(memory.episodic.is_empty());
        assert!(memory.semantic.is_empty());
    }
}
