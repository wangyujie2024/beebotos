//! Unified Memory Search Interface
//!
//! Provides a common trait for all memory search implementations.
//! Unifies hybrid_search, hybrid_search_sqlite, markdown_search, and
//! markdown_storage.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;

/// Default weight for vector search results
pub const DEFAULT_VECTOR_WEIGHT: f32 = 0.7;
/// Default weight for BM25 keyword search results
pub const DEFAULT_BM25_WEIGHT: f32 = 0.3;
/// Default maximum number of results
pub const DEFAULT_MAX_RESULTS: usize = 10;

/// Search result with relevance score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Memory entry ID
    pub id: Uuid,
    /// Content snippet
    pub content: String,
    /// Final combined score (0.0 - 1.0)
    pub score: f32,
    /// Vector search score component
    pub vector_score: Option<f32>,
    /// BM25 keyword search score component
    pub bm25_score: Option<f32>,
    /// Metadata
    pub metadata: HashMap<String, String>,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Source file path (for file-based storage)
    pub source_path: Option<String>,
}

/// Search configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    /// Weight for vector search (0.0 - 1.0)
    pub vector_weight: f32,
    /// Weight for BM25 search (0.0 - 1.0)
    pub bm25_weight: f32,
    /// Maximum results to return
    pub max_results: usize,
    /// Minimum score threshold
    pub min_score_threshold: f32,
    /// Enable semantic search
    pub enable_semantic: bool,
    /// Enable keyword search
    pub enable_keyword: bool,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            vector_weight: DEFAULT_VECTOR_WEIGHT,
            bm25_weight: DEFAULT_BM25_WEIGHT,
            max_results: DEFAULT_MAX_RESULTS,
            min_score_threshold: 0.0,
            enable_semantic: true,
            enable_keyword: true,
        }
    }
}

/// Unified memory search trait
#[async_trait::async_trait]
pub trait MemorySearch: Send + Sync {
    /// Search memories by query string
    ///
    /// Performs hybrid search (vector + keyword) if both are enabled,
    /// otherwise performs the enabled search type.
    async fn search(&self, query: &str) -> Result<Vec<SearchResult>>;

    /// Search with custom configuration
    async fn search_with_config(
        &self,
        query: &str,
        config: SearchConfig,
    ) -> Result<Vec<SearchResult>>;

    /// Semantic search only
    async fn semantic_search(&self, query_embedding: &[f32]) -> Result<Vec<SearchResult>>;

    /// Keyword search only
    async fn keyword_search(&self, keywords: &[String]) -> Result<Vec<SearchResult>>;

    /// Add a memory entry to the search index
    async fn add_entry(
        &self,
        id: Uuid,
        content: &str,
        metadata: HashMap<String, String>,
    ) -> Result<()>;

    /// Remove a memory entry from the search index
    async fn remove_entry(&self, id: Uuid) -> Result<()>;

    /// Update a memory entry in the search index
    async fn update_entry(
        &self,
        id: Uuid,
        content: &str,
        metadata: HashMap<String, String>,
    ) -> Result<()>;

    /// Get search statistics
    fn stats(&self) -> SearchStats;

    /// Clear all entries from the search index
    async fn clear(&self) -> Result<()>;
}

/// Search statistics
#[derive(Debug, Clone, Default)]
pub struct SearchStats {
    /// Total indexed entries
    pub total_entries: usize,
    /// Total search queries performed
    pub total_queries: u64,
    /// Average query latency (ms)
    pub avg_latency_ms: f64,
    /// Index size in bytes (approximate)
    pub index_size_bytes: usize,
}

/// Vector embedding for semantic search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorEmbedding {
    /// Entry ID
    pub id: Uuid,
    /// Embedding vector (normalized)
    pub vector: Vec<f32>,
    /// Source content hash for invalidation
    pub content_hash: String,
}

/// BM25 index entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BM25IndexEntry {
    /// Entry ID
    pub id: Uuid,
    /// Tokenized content for FTS
    pub tokens: Vec<String>,
    /// Document frequency data
    pub term_frequencies: HashMap<String, u32>,
    /// Document length in tokens
    pub doc_length: u32,
}

/// Utility functions for search
pub mod utils {
    use super::*;

    /// Calculate cosine similarity between two vectors
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }

        dot_product / (norm_a * norm_b)
    }

    /// Normalize a vector to unit length
    pub fn normalize_vector(v: &[f32]) -> Vec<f32> {
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm == 0.0 {
            return v.to_vec();
        }
        v.iter().map(|x| x / norm).collect()
    }

    /// Hash content for cache invalidation
    pub fn hash_content(content: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    /// Merge and rerank results from multiple sources
    pub fn merge_results(
        vector_results: Vec<SearchResult>,
        keyword_results: Vec<SearchResult>,
        vector_weight: f32,
        bm25_weight: f32,
        max_results: usize,
    ) -> Vec<SearchResult> {
        use std::collections::HashMap;

        let mut combined: HashMap<Uuid, SearchResult> = HashMap::new();

        // Add vector results
        for mut result in vector_results {
            result.score = result.vector_score.unwrap_or(0.0) * vector_weight;
            combined.insert(result.id, result);
        }

        // Add/merge keyword results
        for mut result in keyword_results {
            result.score = result.bm25_score.unwrap_or(0.0) * bm25_weight;
            if let Some(existing) = combined.get_mut(&result.id) {
                existing.score += result.score;
                existing.bm25_score = result.bm25_score;
            } else {
                combined.insert(result.id, result);
            }
        }

        // Convert to vec, sort by score, and take top results
        let mut results: Vec<SearchResult> = combined.into_values().collect();
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        results.truncate(max_results);

        results
    }
}

// Re-export types from hybrid_search for backward compatibility
pub use utils::{cosine_similarity, hash_content, normalize_vector};
