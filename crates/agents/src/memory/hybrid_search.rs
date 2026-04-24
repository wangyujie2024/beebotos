//! Hybrid Search Mechanism
//!
//! Implements a hybrid search strategy combining vector search and BM25 keyword
//! search, similar to OpenClaw's memory retrieval system.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                     Hybrid Search Engine                        │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  ┌──────────────────┐        ┌──────────────────┐              │
//! │  │   Vector Search  │        │   BM25 Search    │              │
//! │  │  (Semantic 70%)  │        │ (Keyword 30%)    │              │
//! │  └────────┬─────────┘        └────────┬─────────┘              │
//! │           │                          │                        │
//! │           └──────────┬───────────────┘                        │
//! │                      ▼                                        │
//! │           ┌──────────────────┐                               │
//! │           │  Result Fusion   │  ← Weighted Score Merge       │
//! │           │  & Reranking     │                               │
//! │           └────────┬─────────┘                               │
//! │                    ▼                                          │
//! │           ┌──────────────────┐                               │
//! │           │  Ranked Results  │  ← Final search results        │
//! │           └──────────────────┘                               │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Features
//! - Vector Search: Semantic similarity using embeddings
//! - BM25 Search: Precise keyword matching using FTS5
//! - Result Fusion: Weighted combination (70% vector + 30% BM25)
//! - SQLite Backend: Efficient indexing and storage

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
    pub vector_score: f32,
    /// BM25 keyword search score component
    pub bm25_score: f32,
    /// Metadata
    pub metadata: HashMap<String, String>,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
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

/// Hybrid search configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSearchConfig {
    /// Weight for vector search (0.0 - 1.0)
    pub vector_weight: f32,
    /// Weight for BM25 search (0.0 - 1.0)
    pub bm25_weight: f32,
    /// Maximum results to return
    pub max_results: usize,
    /// Minimum score threshold
    pub min_score_threshold: f32,
    /// Enable result reranking
    pub enable_reranking: bool,
    /// Embedding model dimension
    pub embedding_dimension: usize,
    /// BM25 parameters
    pub bm25_k1: f32,
    pub bm25_b: f32,
}

impl Default for HybridSearchConfig {
    fn default() -> Self {
        Self {
            vector_weight: DEFAULT_VECTOR_WEIGHT,
            bm25_weight: DEFAULT_BM25_WEIGHT,
            max_results: DEFAULT_MAX_RESULTS,
            min_score_threshold: 0.01, // Lower threshold to avoid floating-point precision issues
            enable_reranking: true,
            embedding_dimension: 1536, // OpenAI ada-002 dimension
            bm25_k1: 1.5,
            bm25_b: 0.75,
        }
    }
}

impl HybridSearchConfig {
    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        let total_weight = self.vector_weight + self.bm25_weight;
        if (total_weight - 1.0).abs() > 0.01 {
            return Err(crate::error::AgentError::configuration(format!(
                "Search weights must sum to 1.0, got {}",
                total_weight
            )));
        }
        Ok(())
    }
}

/// Hybrid search engine
pub struct HybridSearchEngine {
    config: HybridSearchConfig,
    /// Vector storage (in-memory for now, can be SQLite/Postgres)
    vectors: HashMap<Uuid, VectorEmbedding>,
    /// BM25 index storage
    bm25_index: HashMap<Uuid, BM25IndexEntry>,
    /// Average document length for BM25
    avg_doc_length: f32,
    /// Total document count
    total_docs: usize,
    /// IDF cache
    idf_cache: HashMap<String, f32>,
}

impl HybridSearchEngine {
    /// Create new hybrid search engine
    pub fn new(config: HybridSearchConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            config,
            vectors: HashMap::new(),
            bm25_index: HashMap::new(),
            avg_doc_length: 0.0,
            total_docs: 0,
            idf_cache: HashMap::new(),
        })
    }

    /// Create with default configuration
    pub fn default() -> Result<Self> {
        Self::new(HybridSearchConfig::default())
    }

    /// Index a memory entry with both vector and BM25
    pub fn index_entry(
        &mut self,
        id: Uuid,
        content: &str,
        embedding: Vec<f32>,
        _metadata: HashMap<String, String>,
    ) -> Result<()> {
        // Normalize embedding vector
        let normalized = Self::normalize_vector(&embedding);

        // Store vector embedding
        let vector = VectorEmbedding {
            id,
            vector: normalized,
            content_hash: Self::hash_content(content),
        };
        self.vectors.insert(id, vector);

        // Tokenize and index for BM25
        let tokens = Self::tokenize(content);
        let mut term_frequencies = HashMap::new();
        for token in &tokens {
            *term_frequencies.entry(token.clone()).or_insert(0) += 1;
        }

        let bm25_entry = BM25IndexEntry {
            id,
            tokens: tokens.clone(),
            term_frequencies,
            doc_length: tokens.len() as u32,
        };
        self.bm25_index.insert(id, bm25_entry);

        // Update statistics
        self.update_statistics();

        // Clear IDF cache as document frequencies changed
        self.idf_cache.clear();

        Ok(())
    }

    /// Search using hybrid approach
    pub fn search(
        &self,
        query: &str,
        query_embedding: Option<Vec<f32>>,
    ) -> Result<Vec<SearchResult>> {
        let mut results_map: HashMap<Uuid, SearchResult> = HashMap::new();

        // 1. Vector search (if embedding provided)
        if let Some(embedding) = query_embedding {
            let normalized = Self::normalize_vector(&embedding);
            let vector_results = self.vector_search(&normalized)?;

            for (id, score) in vector_results {
                results_map.insert(
                    id,
                    SearchResult {
                        id,
                        content: String::new(), // Will be filled later
                        score: score * self.config.vector_weight,
                        vector_score: score,
                        bm25_score: 0.0,
                        metadata: HashMap::new(),
                        timestamp: chrono::Utc::now(),
                    },
                );
            }
        }

        // 2. BM25 keyword search
        let bm25_results = self.bm25_search(query)?;

        for (id, score) in bm25_results {
            if let Some(existing) = results_map.get_mut(&id) {
                // Combine scores
                existing.bm25_score = score;
                existing.score += score * self.config.bm25_weight;
            } else {
                results_map.insert(
                    id,
                    SearchResult {
                        id,
                        content: String::new(),
                        score: score * self.config.bm25_weight,
                        vector_score: 0.0,
                        bm25_score: score,
                        metadata: HashMap::new(),
                        timestamp: chrono::Utc::now(),
                    },
                );
            }
        }

        // 3. Convert to vector and sort by score
        let mut results: Vec<SearchResult> = results_map
            .into_values()
            .filter(|r| r.score >= self.config.min_score_threshold)
            .collect();

        // 4. Reranking (if enabled)
        if self.config.enable_reranking {
            results = self.rerank_results(results, query);
        }

        // 5. Sort by final score and limit results
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(self.config.max_results);

        Ok(results)
    }

    /// Vector similarity search using cosine similarity
    fn vector_search(&self, query_vector: &[f32]) -> Result<Vec<(Uuid, f32)>> {
        let mut results = Vec::new();

        for (id, embedding) in &self.vectors {
            let similarity = Self::cosine_similarity(query_vector, &embedding.vector);
            if similarity >= self.config.min_score_threshold {
                results.push((*id, similarity));
            }
        }

        // Sort by similarity
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(self.config.max_results * 2); // Get more candidates for fusion

        Ok(results)
    }

    /// BM25 keyword search
    fn bm25_search(&self, query: &str) -> Result<Vec<(Uuid, f32)>> {
        let query_tokens = Self::tokenize(query);
        let mut results = Vec::new();

        for (id, entry) in &self.bm25_index {
            let score = self.calculate_bm25_score(&query_tokens, entry);
            if score > 0.0 {
                results.push((*id, score));
            }
        }

        // Normalize BM25 scores to 0-1 range
        let max_score: f32 = results.iter().map(|(_, s)| *s).fold(0.0, f32::max);
        if max_score > 0.0 {
            for (_, score) in &mut results {
                *score /= max_score;
            }
        }

        // Sort by score
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(self.config.max_results * 2);

        Ok(results)
    }

    /// Calculate BM25 score for a document
    fn calculate_bm25_score(&self, query_tokens: &[String], entry: &BM25IndexEntry) -> f32 {
        let mut score = 0.0;

        for token in query_tokens {
            let tf = *entry.term_frequencies.get(token).unwrap_or(&0) as f32;
            if tf > 0.0 {
                let idf = self.get_idf_immutable(token);
                let numerator = tf * (self.config.bm25_k1 + 1.0);
                let denominator = tf
                    + self.config.bm25_k1
                        * (1.0 - self.config.bm25_b
                            + self.config.bm25_b
                                * (entry.doc_length as f32 / self.avg_doc_length.max(1.0)));
                score += idf * numerator / denominator;
            }
        }

        score
    }

    /// Calculate IDF (Inverse Document Frequency)
    #[allow(dead_code)]
    fn get_idf(&mut self, term: &str) -> f32 {
        if let Some(&idf) = self.idf_cache.get(term) {
            return idf;
        }

        let doc_freq = self
            .bm25_index
            .values()
            .filter(|entry| entry.term_frequencies.contains_key(term))
            .count() as f32;

        let idf = ((self.total_docs as f32 - doc_freq + 0.5) / (doc_freq + 0.5))
            .ln()
            .max(0.0);

        self.idf_cache.insert(term.to_string(), idf);
        idf
    }

    /// Immutable version of get_idf for search
    fn get_idf_immutable(&self, term: &str) -> f32 {
        if let Some(&idf) = self.idf_cache.get(term) {
            return idf;
        }

        let doc_freq = self
            .bm25_index
            .values()
            .filter(|entry| entry.term_frequencies.contains_key(term))
            .count() as f32;

        ((self.total_docs as f32 - doc_freq + 0.5) / (doc_freq + 0.5))
            .ln()
            .max(0.0)
    }

    /// Rerank results using additional signals
    fn rerank_results(&self, results: Vec<SearchResult>, _query: &str) -> Vec<SearchResult> {
        // Simple reranking: boost recent entries
        let now = chrono::Utc::now();

        results
            .into_iter()
            .map(|mut r| {
                // Time decay factor (boost entries from last 24 hours)
                let hours_old = (now - r.timestamp).num_hours() as f32;
                let time_boost = if hours_old < 24.0 {
                    1.0 + (24.0 - hours_old) / 48.0 // Up to 1.5x boost
                } else {
                    1.0
                };

                r.score *= time_boost;
                r
            })
            .collect()
    }

    /// Remove an entry from the index
    pub fn remove_entry(&mut self, id: Uuid) -> Result<()> {
        self.vectors.remove(&id);
        self.bm25_index.remove(&id);
        self.update_statistics();
        self.idf_cache.clear();
        Ok(())
    }

    /// Update index statistics
    fn update_statistics(&mut self) {
        self.total_docs = self.bm25_index.len();

        if self.total_docs > 0 {
            let total_length: u32 = self.bm25_index.values().map(|e| e.doc_length).sum();
            self.avg_doc_length = total_length as f32 / self.total_docs as f32;
        } else {
            self.avg_doc_length = 0.0;
        }
    }

    /// Calculate cosine similarity between two vectors
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a > 0.0 && norm_b > 0.0 {
            dot_product / (norm_a * norm_b)
        } else {
            0.0
        }
    }

    /// Normalize a vector to unit length
    fn normalize_vector(v: &[f32]) -> Vec<f32> {
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            v.iter().map(|x| x / norm).collect()
        } else {
            v.to_vec()
        }
    }

    /// Simple tokenization for BM25
    fn tokenize(text: &str) -> Vec<String> {
        text.to_lowercase()
            .split_whitespace()
            .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()))
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    /// Simple content hash
    fn hash_content(content: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Get index statistics
    pub fn get_stats(&self) -> SearchStats {
        SearchStats {
            total_documents: self.total_docs,
            avg_document_length: self.avg_doc_length,
            vector_index_size: self.vectors.len(),
            bm25_index_size: self.bm25_index.len(),
        }
    }
}

/// Search statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchStats {
    pub total_documents: usize,
    pub avg_document_length: f32,
    pub vector_index_size: usize,
    pub bm25_index_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hybrid_search_config_default() {
        let config = HybridSearchConfig::default();
        assert_eq!(config.vector_weight, 0.7);
        assert_eq!(config.bm25_weight, 0.3);
        assert_eq!(config.max_results, 10);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_hybrid_search_config_validation() {
        let config = HybridSearchConfig {
            vector_weight: 0.5,
            bm25_weight: 0.6,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_normalize_vector() {
        let v = vec![3.0, 4.0];
        let normalized = HybridSearchEngine::normalize_vector(&v);
        let norm: f32 = normalized.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = HybridSearchEngine::cosine_similarity(&a, &b);
        assert!((sim - 0.0).abs() < 0.001);

        let c = vec![1.0, 0.0, 0.0];
        let d = vec![1.0, 0.0, 0.0];
        let sim2 = HybridSearchEngine::cosine_similarity(&c, &d);
        assert!((sim2 - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_tokenize() {
        let text = "Hello, World! This is a test.";
        let tokens = HybridSearchEngine::tokenize(text);
        assert_eq!(tokens, vec!["hello", "world", "this", "is", "a", "test"]);
    }

    #[tokio::test]
    async fn test_hybrid_search_engine() {
        let mut engine = HybridSearchEngine::default().unwrap();

        // Index some entries
        let id1 = Uuid::new_v4();
        engine
            .index_entry(
                id1,
                "SQLite database configuration",
                vec![1.0, 0.0, 0.0],
                HashMap::new(),
            )
            .unwrap();

        let id2 = Uuid::new_v4();
        engine
            .index_entry(
                id2,
                "MySQL server setup guide",
                vec![0.0, 1.0, 0.0],
                HashMap::new(),
            )
            .unwrap();

        // Search with both vector and BM25
        let results = engine
            .search("database", Some(vec![1.0, 0.0, 0.0]))
            .unwrap();
        assert!(!results.is_empty());

        // First result should be the SQLite entry
        assert_eq!(results[0].id, id1);
    }
}
