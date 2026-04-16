//! Hybrid Search with SQLite Persistence
//!
//! Persistent hybrid search implementation using SQLite with FTS5 and sqlite-vec extensions.
//! Provides durable storage for vector embeddings and BM25 full-text search.
//!
//! # Database Schema
//!
//! ```sql
//! -- Vector embeddings table (using sqlite-vec)
//! CREATE VIRTUAL TABLE vec_items USING vec0(
//!     embedding float[1536]
//! );
//!
//! -- Full-text search table (using FTS5)
//! CREATE VIRTUAL TABLE ft_items USING fts5(
//!     content,
//!     metadata
//! );
//!
//! -- Memory entries metadata
//! CREATE TABLE memory_entries (
//!     id TEXT PRIMARY KEY,
//!     content TEXT NOT NULL,
//!     content_hash TEXT NOT NULL,
//!     timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
//!     metadata JSON,
//!     file_path TEXT
//! );
//!
//! -- Search index mapping
//! CREATE TABLE search_index (
//!     entry_id TEXT PRIMARY KEY,
//!     vec_rowid INTEGER,
//!     fts_rowid INTEGER,
//!     FOREIGN KEY (entry_id) REFERENCES memory_entries(id)
//! );
//! ```

use crate::error::Result;
use crate::memory::hybrid_search::HybridSearchConfig;
use crate::memory::search::{MemorySearch, SearchConfig, SearchResult, SearchStats, DEFAULT_MAX_RESULTS};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tracing::{debug, info};
use uuid::Uuid;

/// Default SQLite database file name
pub const DEFAULT_SEARCH_DB: &str = "data/search_index.db";

/// SQLite-backed hybrid search engine
pub struct HybridSearchSqlite {
    /// Database connection (wrapped in Mutex for thread safety)
    conn: Arc<Mutex<Connection>>,
    /// Search configuration
    config: HybridSearchConfig,
    /// Database file path
    db_path: PathBuf,
}

/// Memory entry with full data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqliteMemoryEntry {
    pub id: Uuid,
    pub content: String,
    pub content_hash: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub metadata: HashMap<String, String>,
    pub file_path: Option<String>,
}

/// Search result from SQLite
#[derive(Debug, Clone)]
pub struct SqliteSearchResult {
    pub entry: SqliteMemoryEntry,
    pub vector_score: f32,
    pub bm25_score: f32,
    pub combined_score: f32,
}

impl HybridSearchSqlite {
    /// Create or open a SQLite-backed search engine
    pub fn new(db_path: impl AsRef<Path>, config: HybridSearchConfig) -> Result<Self> {
        let db_path = db_path.as_ref().to_path_buf();
        
        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                crate::error::AgentError::storage(format!(
                    "Failed to create database directory: {}",
                    e
                ))
            })?;
        }

        let conn = Connection::open(&db_path).map_err(|e| {
            crate::error::AgentError::storage(format!(
                "Failed to open database: {}",
                e
            ))
        })?;

        let mut engine = Self {
            conn: Arc::new(Mutex::new(conn)),
            config,
            db_path,
        };

        // Initialize database schema
        engine.init_schema()?;

        info!("SQLite hybrid search engine initialized at: {:?}", engine.db_path);
        Ok(engine)
    }

    /// Create with default configuration
    pub fn default_with_path(db_path: impl AsRef<Path>) -> Result<Self> {
        Self::new(db_path, HybridSearchConfig::default())
    }

    /// Initialize database schema
    fn init_schema(&mut self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // Create memory entries table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS memory_entries (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                metadata TEXT,
                file_path TEXT
            )",
            [],
        )
        .map_err(|e| {
            crate::error::AgentError::storage(format!(
                "Failed to create memory_entries table: {}",
                e
            ))
        })?;

        // Create FTS5 virtual table for full-text search
        // Note: content_rowid is not a column; it's a special directive that must be used with
        // content='table_name' option. We use a simpler schema without it.
        conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS ft_items USING fts5(
                content,
                metadata
            )",
            [],
        )
        .map_err(|e| {
            crate::error::AgentError::storage(format!(
                "Failed to create FTS5 table: {}. Make sure FTS5 is enabled in SQLite", 
                e
            ))
        })?;

        // Create index for faster lookups
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_entries_timestamp ON memory_entries(timestamp)",
            [],
        )
        .ok(); // Ignore error if index already exists

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_entries_hash ON memory_entries(content_hash)",
            [],
        )
        .ok();

        // sqlite-vec extension is not available in bundled SQLite
        // Always use fallback vector storage
        debug!("Using fallback vector storage (sqlite-vec not available in bundled SQLite)");
        conn.execute(
            "CREATE TABLE IF NOT EXISTS vector_fallback (
                entry_id TEXT PRIMARY KEY,
                vector BLOB NOT NULL
            )",
            [],
        )
        .map_err(|e| {
            crate::error::AgentError::storage(format!(
                "Failed to create vector fallback table: {}",
                e
            ))
        })?;

        Ok(())
    }

    /// Check if sqlite-vec is available
    /// Note: Always returns false as we use fallback vector storage
    pub fn has_vec_extension(&self) -> bool {
        false
    }

    /// Index a memory entry with vector embedding
    pub fn index_entry(
        &self,
        id: Uuid,
        content: &str,
        embedding: &[f32],
        metadata: HashMap<String, String>,
        file_path: Option<&str>,
    ) -> Result<()> {
        let content_hash = Self::hash_content(content);
        let metadata_json = serde_json::to_string(&metadata).unwrap_or_default();
        let timestamp = chrono::Utc::now();

        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| {
            crate::error::AgentError::storage(format!(
                "Failed to start transaction: {}",
                e
            ))
        })?;

        // 1. Insert into memory_entries
        tx.execute(
            "INSERT OR REPLACE INTO memory_entries 
             (id, content, content_hash, timestamp, metadata, file_path) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id.to_string(),
                content,
                content_hash,
                timestamp.to_rfc3339(),
                metadata_json,
                file_path
            ],
        )
        .map_err(|e| {
            crate::error::AgentError::storage(format!(
                "Failed to insert memory entry: {}",
                e
            ))
        })?;

        // 2. Insert into FTS5 for BM25 search
        // Store entry_id in metadata JSON to enable JOIN with memory_entries
        let metadata_with_id = format!("{{\"entry_id\":\"{}\",\"data\":{}}}", id, metadata_json);
        tx.execute(
            "INSERT INTO ft_items (content, metadata) VALUES (?1, ?2)",
            params![content, metadata_with_id],
        )
        .map_err(|e| {
            crate::error::AgentError::storage(format!(
                "Failed to insert FTS5 entry: {}",
                e
            ))
        })?;

        // 3. Insert vector (try sqlite-vec first, fallback to blob)
        let vector_blob = Self::vector_to_blob(embedding);
        
        if self.has_vec_extension() {
            // Use sqlite-vec
            let vector_str = embedding.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(",");
            tx.execute(
                "INSERT INTO vec_items (rowid, embedding) 
                 VALUES ((SELECT rowid FROM memory_entries WHERE id = ?1), vec_f32(?2))",
                params![id.to_string(), vector_str],
            )
            .or_else(|_| {
                // Fallback if vec_f32 doesn't work
                tx.execute(
                    "INSERT INTO vector_fallback (entry_id, vector) VALUES (?1, ?2)",
                    params![id.to_string(), &vector_blob],
                )
            })
            .map_err(|e| {
                crate::error::AgentError::storage(format!(
                    "Failed to insert vector: {}",
                    e
                ))
            })?;
        } else {
            // Use fallback
            tx.execute(
                "INSERT OR REPLACE INTO vector_fallback (entry_id, vector) VALUES (?1, ?2)",
                params![id.to_string(), &vector_blob],
            )
            .map_err(|e| {
                crate::error::AgentError::storage(format!(
                    "Failed to insert fallback vector: {}",
                    e
                ))
            })?;
        }

        tx.commit().map_err(|e| {
            crate::error::AgentError::storage(format!(
                "Failed to commit transaction: {}",
                e
            ))
        })?;

        debug!("Indexed entry {} with {}-dimensional vector", id, embedding.len());
        Ok(())
    }

    /// Search using hybrid approach (vector + BM25)
    pub fn search(
        &self,
        query: &str,
        query_embedding: Option<&[f32]>,
        limit: usize,
    ) -> Result<Vec<SqliteSearchResult>> {
        let mut results_map: HashMap<Uuid, SqliteSearchResult> = HashMap::new();

        // 1. Vector search (if embedding provided)
        if let Some(embedding) = query_embedding {
            let vector_results = self.vector_search(embedding, limit * 2)?;
            for (id, score, entry) in vector_results {
                results_map.insert(
                    id,
                    SqliteSearchResult {
                        entry,
                        vector_score: score,
                        bm25_score: 0.0,
                        combined_score: score * self.config.vector_weight,
                    },
                );
            }
        }

        // 2. BM25 search using FTS5
        let bm25_results = self.bm25_search(query, limit * 2)?;
        for (id, score, entry) in bm25_results {
            if let Some(existing) = results_map.get_mut(&id) {
                existing.bm25_score = score;
                existing.combined_score += score * self.config.bm25_weight;
            } else {
                results_map.insert(
                    id,
                    SqliteSearchResult {
                        entry,
                        vector_score: 0.0,
                        bm25_score: score,
                        combined_score: score * self.config.bm25_weight,
                    },
                );
            }
        }

        // 3. Convert to vector and sort
        let mut results: Vec<SqliteSearchResult> = results_map
            .into_values()
            .filter(|r| r.combined_score >= self.config.min_score_threshold)
            .collect();

        // 4. Sort by combined score
        results.sort_by(|a, b| {
            b.combined_score
                .partial_cmp(&a.combined_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 5. Limit results
        results.truncate(limit);

        Ok(results)
    }

    /// Vector similarity search
    fn vector_search(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<(Uuid, f32, SqliteMemoryEntry)>> {
        let conn = self.conn.lock().unwrap();
        let mut results = Vec::new();

        if self.has_vec_extension() {
            // Use sqlite-vec for vector search
            let query_str = query_embedding
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(",");

            let mut stmt = conn.prepare(
                "SELECT e.id, e.content, e.content_hash, e.timestamp, e.metadata, e.file_path,
                        vec_distance_L2(v.embedding, vec_f32(?1)) as distance
                 FROM vec_items v
                 JOIN memory_entries e ON v.rowid = e.rowid
                 ORDER BY distance
                 LIMIT ?2"
            ).map_err(|e| {
                crate::error::AgentError::storage(format!(
                    "Failed to prepare vector search: {}",
                    e
                ))
            })?;

            let rows = stmt.query_map(params![query_str, limit as i64], |row| {
                let distance: f64 = row.get(6)?;
                let score = 1.0 / (1.0 + distance as f32); // Convert distance to similarity
                Ok((Self::row_to_entry(row)?, score))
            }).map_err(|e| {
                crate::error::AgentError::storage(format!(
                    "Vector search failed: {}",
                    e
                ))
            })?;

            for row in rows {
                let (entry, score) = row.map_err(|e| {
                    crate::error::AgentError::storage(format!(
                        "Failed to read vector search row: {}",
                        e
                    ))
                })?;
                results.push((entry.id, score, entry));
            }
        } else {
            // Fallback: Brute force cosine similarity
            let mut stmt = conn.prepare(
                "SELECT e.id, e.content, e.content_hash, e.timestamp, e.metadata, e.file_path,
                        v.vector
                 FROM vector_fallback v
                 JOIN memory_entries e ON v.entry_id = e.id"
            ).map_err(|e| {
                crate::error::AgentError::storage(format!(
                    "Failed to prepare fallback vector search: {}",
                    e
                ))
            })?;

            let rows = stmt.query_map([], |row| {
                let vector_blob: Vec<u8> = row.get(6)?;
                let stored_vector = Self::blob_to_vector(&vector_blob);
                let similarity = Self::cosine_similarity(query_embedding, &stored_vector);
                Ok((Self::row_to_entry(row)?, similarity))
            }).map_err(|e| {
                crate::error::AgentError::storage(format!(
                    "Fallback vector search failed: {}",
                    e
                ))
            })?;

            for row in rows {
                let (entry, score) = row.map_err(|e| {
                    crate::error::AgentError::storage(format!(
                        "Failed to read fallback vector search row: {}",
                        e
                    ))
                })?;
                if score >= self.config.min_score_threshold {
                    results.push((entry.id, score, entry));
                }
            }

            // Sort and limit
            results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            results.truncate(limit);
        }

        Ok(results)
    }

    /// BM25 search using FTS5
    pub(crate) fn bm25_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(Uuid, f32, SqliteMemoryEntry)>> {
        let conn = self.conn.lock().unwrap();
        let mut results = Vec::new();

        // Use FTS5 bm25() function for ranking
        // Note: bm25() returns negative values (lower is better/more relevant)
        let mut stmt = conn.prepare(
            "SELECT e.id, e.content, e.content_hash, e.timestamp, e.metadata, e.file_path,
                    bm25(ft_items) as rank
             FROM ft_items
             JOIN memory_entries e ON json_extract(ft_items.metadata, '$.entry_id') = e.id
             WHERE ft_items MATCH ?1
             ORDER BY rank ASC
             LIMIT ?2"
        ).map_err(|e| {
            crate::error::AgentError::storage(format!(
                "Failed to prepare BM25 search: {}",
                e
            ))
        })?;

        // Convert query to FTS5 query syntax (add wildcards for prefix matching)
        // Sanitize each word to avoid FTS5 syntax errors from special characters like ?, ", *, etc.
        // Use OR semantics for better recall in memory retrieval.
        let fts_query: String = query
            .split_whitespace()
            .filter_map(|word| {
                let sanitized: String = word
                    .chars()
                    .filter(|c| c.is_alphanumeric())
                    .collect();
                if sanitized.is_empty() {
                    None
                } else {
                    Some(format!("{}*", sanitized))
                }
            })
            .collect::<Vec<_>>()
            .join(" OR ");

        let rows = stmt.query_map(params![fts_query, limit as i64], |row| {
            let rank: f64 = row.get(6)?;
            // BM25 rank is negative (lower is better in SQLite FTS5), normalize to 0-1
            // Higher score = better match. The old formula was inverted.
            let score = 1.0 - (rank.abs() as f32 + 1.0).recip();
            Ok((Self::row_to_entry(row)?, score))
        }).map_err(|e| {
            crate::error::AgentError::storage(format!(
                "BM25 search failed: {}",
                e
            ))
        })?;

        for row in rows {
            let (entry, score) = row.map_err(|e| {
                crate::error::AgentError::storage(format!(
                    "Failed to read BM25 search row: {}",
                    e
                ))
            })?;
            results.push((entry.id, score, entry));
        }

        Ok(results)
    }

    /// Delete an entry from the index
    pub fn delete_entry(&self, id: Uuid) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| {
            crate::error::AgentError::storage(format!(
                "Failed to start delete transaction: {}",
                e
            ))
        })?;
        
        // Delete from FTS5 index using metadata JSON match
        tx.execute(
            "DELETE FROM ft_items WHERE json_extract(metadata, '$.entry_id') = ?1",
            params![id.to_string()],
        )
        .ok(); // Ignore error if fts_items doesn't exist

        // Clean up vector fallback if exists
        tx.execute(
            "DELETE FROM vector_fallback WHERE entry_id = ?1",
            params![id.to_string()],
        )
        .ok();

        // Delete from memory_entries last
        tx.execute(
            "DELETE FROM memory_entries WHERE id = ?1",
            params![id.to_string()],
        )
        .map_err(|e| {
            crate::error::AgentError::storage(format!(
                "Failed to delete entry: {}",
                e
            ))
        })?;

        tx.commit().map_err(|e| {
            crate::error::AgentError::storage(format!(
                "Failed to commit delete transaction: {}",
                e
            ))
        })?;

        info!("Deleted entry {} from search index", id);
        Ok(())
    }

    /// Get entry by ID
    pub fn get_entry(&self, id: Uuid) -> Result<Option<SqliteMemoryEntry>> {
        let conn = self.conn.lock().unwrap();
        
        let mut stmt = conn.prepare(
            "SELECT id, content, content_hash, timestamp, metadata, file_path 
             FROM memory_entries WHERE id = ?1"
        ).map_err(|e| {
            crate::error::AgentError::storage(format!(
                "Failed to prepare get_entry: {}",
                e
            ))
        })?;

        let result = stmt
            .query_row(params![id.to_string()], |row| Self::row_to_entry(row))
            .optional()
            .map_err(|e| {
                crate::error::AgentError::storage(format!(
                    "Failed to get entry: {}",
                    e
                ))
            })?;

        Ok(result)
    }

    /// Get database statistics
    pub fn get_stats(&self) -> Result<SearchDatabaseStats> {
        let conn = self.conn.lock().unwrap();

        let total_entries: i64 = conn
            .query_row("SELECT COUNT(*) FROM memory_entries", [], |row| row.get(0))
            .unwrap_or(0);

        let vector_entries: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM vector_fallback",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let fts_entries: i64 = conn
            .query_row("SELECT COUNT(*) FROM ft_items", [], |row| row.get(0))
            .unwrap_or(0);

        let db_size: i64 = std::fs::metadata(&self.db_path)
            .map(|m| m.len() as i64)
            .unwrap_or(0);

        Ok(SearchDatabaseStats {
            total_entries: total_entries as usize,
            vector_entries: vector_entries as usize,
            fts_entries: fts_entries as usize,
            db_size_bytes: db_size as usize,
            has_vec_extension: self.has_vec_extension(),
        })
    }

    /// Vacuum database to reclaim space
    pub fn vacuum(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("VACUUM", []).map_err(|e| {
            crate::error::AgentError::storage(format!(
                "Failed to vacuum database: {}",
                e
            ))
        })?;
        info!("Database vacuum completed");
        Ok(())
    }

    /// Convert database row to entry
    fn row_to_entry(row: &Row) -> rusqlite::Result<SqliteMemoryEntry> {
        let id_str: String = row.get(0)?;
        let id = Uuid::parse_str(&id_str)
            .map_err(|e| rusqlite::Error::InvalidParameterName(format!("Invalid UUID: {}", e)))?;

        let content: String = row.get(1)?;
        let content_hash: String = row.get(2)?;

        let timestamp_str: String = row.get(3)?;
        let timestamp = chrono::DateTime::parse_from_rfc3339(&timestamp_str)
            .map_err(|e| rusqlite::Error::InvalidParameterName(format!("Invalid timestamp: {}", e)))?
            .with_timezone(&chrono::Utc);

        let metadata_json: String = row.get(4).unwrap_or_default();
        let metadata: HashMap<String, String> =
            serde_json::from_str(&metadata_json).unwrap_or_default();

        let file_path: Option<String> = row.get(5).ok();

        Ok(SqliteMemoryEntry {
            id,
            content,
            content_hash,
            timestamp,
            metadata,
            file_path,
        })
    }

    /// Convert vector to blob
    fn vector_to_blob(vector: &[f32]) -> Vec<u8> {
        vector
            .iter()
            .flat_map(|&f| f.to_le_bytes().to_vec())
            .collect()
    }

    /// Convert blob to vector
    fn blob_to_vector(blob: &[u8]) -> Vec<f32> {
        blob.chunks_exact(4)
            .map(|chunk| {
                let bytes: [u8; 4] = chunk.try_into().unwrap_or_default();
                f32::from_le_bytes(bytes)
            })
            .collect()
    }

    /// Calculate cosine similarity
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
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

    /// Hash content for deduplication
    fn hash_content(content: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Get database file path
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }
}

#[async_trait::async_trait]
impl MemorySearch for HybridSearchSqlite {
    async fn search(&self, query: &str) -> Result<Vec<SearchResult>> {
        let results = self.search(query, None, DEFAULT_MAX_RESULTS)?;
        Ok(results.into_iter().map(into_search_result).collect())
    }

    async fn search_with_config(
        &self,
        query: &str,
        config: SearchConfig,
    ) -> Result<Vec<SearchResult>> {
        let results = self.search(query, None, config.max_results)?;
        Ok(results.into_iter().map(into_search_result).collect())
    }

    async fn semantic_search(&self, query_embedding: &[f32]) -> Result<Vec<SearchResult>> {
        let results = self.vector_search(query_embedding, DEFAULT_MAX_RESULTS)?;
        Ok(results
            .into_iter()
            .map(|(id, score, entry)| SearchResult {
                id,
                content: entry.content,
                score,
                vector_score: Some(score),
                bm25_score: None,
                metadata: entry.metadata,
                timestamp: entry.timestamp,
                source_path: entry.file_path,
            })
            .collect())
    }

    async fn keyword_search(&self, keywords: &[String]) -> Result<Vec<SearchResult>> {
        let query = keywords.join(" ");
        let results = self.bm25_search(&query, DEFAULT_MAX_RESULTS)?;
        Ok(results
            .into_iter()
            .map(|(id, score, entry)| SearchResult {
                id,
                content: entry.content,
                score,
                vector_score: None,
                bm25_score: Some(score),
                metadata: entry.metadata,
                timestamp: entry.timestamp,
                source_path: entry.file_path,
            })
            .collect())
    }

    async fn add_entry(
        &self,
        id: Uuid,
        content: &str,
        metadata: HashMap<String, String>,
    ) -> Result<()> {
        self.index_entry(id, content, &[0.0f32], metadata, None)
    }

    async fn remove_entry(&self, id: Uuid) -> Result<()> {
        self.delete_entry(id)
    }

    async fn update_entry(
        &self,
        id: Uuid,
        content: &str,
        metadata: HashMap<String, String>,
    ) -> Result<()> {
        self.delete_entry(id)?;
        self.index_entry(id, content, &[0.0f32], metadata, None)
    }

    fn stats(&self) -> SearchStats {
        match self.get_stats() {
            Ok(stats) => SearchStats {
                total_entries: stats.total_entries,
                total_queries: 0,
                avg_latency_ms: 0.0,
                index_size_bytes: stats.db_size_bytes,
            },
            Err(_) => SearchStats::default(),
        }
    }

    async fn clear(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM ft_items", []).ok();
        conn.execute("DELETE FROM vector_fallback", []).ok();
        conn.execute("DELETE FROM memory_entries", [])
            .map_err(|e| {
                crate::error::AgentError::storage(format!(
                    "Failed to clear memory entries: {}",
                    e
                ))
            })?;
        Ok(())
    }
}

fn into_search_result(result: SqliteSearchResult) -> SearchResult {
    SearchResult {
        id: result.entry.id,
        content: result.entry.content,
        score: result.combined_score,
        vector_score: Some(result.vector_score),
        bm25_score: Some(result.bm25_score),
        metadata: result.entry.metadata,
        timestamp: result.entry.timestamp,
        source_path: result.entry.file_path,
    }
}

/// Database statistics
#[derive(Debug, Clone)]
pub struct SearchDatabaseStats {
    pub total_entries: usize,
    pub vector_entries: usize,
    pub fts_entries: usize,
    pub db_size_bytes: usize,
    pub has_vec_extension: bool,
}

impl std::fmt::Display for SearchDatabaseStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Search DB: {} entries, {} vectors, {} FTS docs, {:.2} MB, vec_ext: {}",
            self.total_entries,
            self.vector_entries,
            self.fts_entries,
            self.db_size_bytes as f64 / 1024.0 / 1024.0,
            self.has_vec_extension
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_engine() -> (HybridSearchSqlite, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let engine = HybridSearchSqlite::default_with_path(&db_path).unwrap();
        (engine, temp_dir)
    }

    #[test]
    fn test_init_schema() {
        let (engine, _temp) = create_test_engine();
        let stats = engine.get_stats().unwrap();
        assert_eq!(stats.total_entries, 0);
    }

    #[test]
    fn test_index_and_search() {
        let (engine, _temp) = create_test_engine();

        // Index some entries
        let id1 = Uuid::new_v4();
        engine
            .index_entry(
                id1,
                "SQLite database configuration guide",
                &[1.0, 0.0, 0.0, 0.0],
                HashMap::new(),
                None,
            )
            .unwrap();

        let id2 = Uuid::new_v4();
        engine
            .index_entry(
                id2,
                "MySQL server setup tutorial",
                &[0.0, 1.0, 0.0, 0.0],
                HashMap::new(),
                None,
            )
            .unwrap();

        // Verify entries are in the database
        let stats = engine.get_stats().unwrap();
        assert_eq!(stats.total_entries, 2, "Expected 2 entries in database");

        // Search with BM25
        let results = engine.search("database", None, 10).unwrap();
        assert!(!results.is_empty(), "BM25 search should return results for 'database'");

        // Search with vector
        let results = engine
            .search("database", Some(&[1.0, 0.0, 0.0, 0.0]), 10)
            .unwrap();
        assert!(!results.is_empty());
        // First result should be SQLite entry (better vector match)
        assert_eq!(results[0].entry.id, id1);
    }

    #[test]
    fn test_delete_entry() {
        let (engine, _temp) = create_test_engine();

        let id = Uuid::new_v4();
        engine
            .index_entry(id, "Test content", &[1.0, 0.0], HashMap::new(), None)
            .unwrap();

        assert!(engine.get_entry(id).unwrap().is_some());

        engine.delete_entry(id).unwrap();

        assert!(engine.get_entry(id).unwrap().is_none());
    }

    #[test]
    fn test_vector_blob_conversion() {
        let original = vec![1.0f32, 2.0, 3.0, 4.0, 5.0];
        let blob = HybridSearchSqlite::vector_to_blob(&original);
        let converted = HybridSearchSqlite::blob_to_vector(&blob);
        assert_eq!(original, converted);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!((HybridSearchSqlite::cosine_similarity(&a, &b) - 0.0).abs() < 0.001);

        let c = vec![1.0, 0.0, 0.0];
        assert!((HybridSearchSqlite::cosine_similarity(&a, &c) - 1.0).abs() < 0.001);
    }
}
