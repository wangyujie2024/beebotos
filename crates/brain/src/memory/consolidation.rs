//! Memory Consolidation
//!
//! Sleep mode: transfers memories from STM to LTM.

use serde::{Deserialize, Serialize};

use super::{EpisodicMemory, SemanticMemory, ShortTermMemory};

/// Configuration for memory consolidation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationConfig {
    /// Minimum rehearsal count to trigger consolidation
    pub rehearsal_threshold: u32,
    /// Minimum importance for semantic extraction
    pub importance_threshold: f32,
    /// Maximum items to consolidate per cycle
    pub max_items_per_cycle: usize,
}

impl Default for ConsolidationConfig {
    fn default() -> Self {
        Self {
            rehearsal_threshold: 3,
            importance_threshold: 0.6,
            max_items_per_cycle: 100,
        }
    }
}

/// Memory consolidation engine
pub struct ConsolidationEngine {
    config: ConsolidationConfig,
}

/// Consolidation report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationReport {
    pub stm_to_episodic: usize,
    pub stm_to_semantic: usize,
    pub episodes_consolidated: usize,
    pub timestamp: u64,
}

impl ConsolidationEngine {
    pub fn new(config: ConsolidationConfig) -> Self {
        Self { config }
    }

    /// Run consolidation (sleep mode)
    pub fn consolidate(
        &self,
        stm: &mut ShortTermMemory,
        episodic: &mut EpisodicMemory,
        semantic: &mut SemanticMemory,
    ) -> ConsolidationReport {
        let mut report = ConsolidationReport {
            stm_to_episodic: 0,
            stm_to_semantic: 0,
            episodes_consolidated: 0,
            timestamp: Self::now(),
        };

        // 1. Consolidate well-rehearsed STM items to episodic memory
        let stm_items: Vec<_> = stm.items().iter().cloned().collect();
        for item in stm_items {
            if stm.rehearsal_count(&item.id) >= self.config.rehearsal_threshold {
                episodic.encode(item.content.clone(), item.timestamp, None);
                report.stm_to_episodic += 1;
            }
        }

        // 2. Extract concepts from high-importance episodes to semantic memory
        let recent_episodes = episodic.recent(100);
        for episode in recent_episodes {
            if episode.importance >= self.config.importance_threshold {
                // Simple extraction: create concept from "what"
                let concept_name = Self::extract_concept_name(&episode.what);
                if semantic.find_by_name(&concept_name).is_none() {
                    semantic.learn_concept(concept_name, episode.what.clone(), "extracted");
                    report.stm_to_semantic += 1;
                }
            }
        }

        // 3. Consolidate related episodes
        // (simplified - would group temporally/spatially related episodes)

        report
    }

    fn extract_concept_name(text: &str) -> String {
        // Simple extraction: first few words
        text.split_whitespace()
            .take(3)
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_secs()
    }
}

impl Default for ConsolidationEngine {
    fn default() -> Self {
        Self::new(ConsolidationConfig::default())
    }
}
