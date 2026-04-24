//! Markdown Search Integration
//!
//! Integrates MarkdownStorage with HybridSearchSqlite and EmbeddingProvider
//! to provide a unified memory system with both file-based storage and
//! efficient hybrid search capabilities.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                 Unified Memory System                           │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                                                                  │
//! │  ┌──────────────┐      ┌──────────────┐      ┌──────────────┐  │
//! │  │   Markdown   │      │   Hybrid     │      │  Embedding   │  │
//! │  │   Storage    │◄────►│   Search     │◄────►│  Provider    │  │
//! │  │ (File-based) │      │ (SQLite+Vec) │      │(OpenAI/Local)│  │
//! │  └──────────────┘      └──────────────┘      └──────────────┘  │
//! │           │                    │                                │
//! │           └────────┬───────────┘                                │
//! │                    ▼                                             │
//! │           ┌──────────────┐                                      │
//! │           │ Search Index │  ← Auto-index on write              │
//! │           │   Manager    │                                      │
//! │           └──────────────┘                                      │
//! │                                                                  │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Features
//! - Automatic indexing on Markdown write
//! - Hybrid search (semantic + keyword) across all memory files
//! - File change detection and re-indexing
//! - Batch indexing for initial import

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{debug, info};
#[allow(unused_imports)]
use uuid::Uuid;

use crate::error::Result;
use crate::memory::embedding::{EmbeddingConfig, EmbeddingProvider, EmbeddingProviderFactory};
use crate::memory::hybrid_search_sqlite::{
    HybridSearchSqlite, SearchDatabaseStats, SqliteMemoryEntry, SqliteSearchResult,
};
use crate::memory::markdown_storage::{
    MarkdownMemoryEntry, MarkdownStorage, MarkdownStorageConfig, MemoryFileType,
};
use crate::memory::search::{MemorySearch, SearchConfig, SearchResult, SearchStats};

/// Unified memory system configuration
#[derive(Debug, Clone)]
pub struct UnifiedMemoryConfig {
    /// Markdown storage configuration
    pub storage_config: MarkdownStorageConfig,
    /// Search database path
    pub search_db_path: PathBuf,
    /// Embedding provider configuration
    pub embedding_config: EmbeddingConfig,
    /// Auto-index on write
    pub auto_index: bool,
    /// Index batch size
    pub index_batch_size: usize,
}

impl Default for UnifiedMemoryConfig {
    fn default() -> Self {
        Self {
            storage_config: MarkdownStorageConfig::default(),
            search_db_path: PathBuf::from("data/search_index.db"),
            embedding_config: EmbeddingConfig::default(),
            auto_index: true,
            index_batch_size: 10,
        }
    }
}

/// Unified memory system
pub struct UnifiedMemorySystem {
    config: UnifiedMemoryConfig,
    /// Markdown storage
    storage: MarkdownStorage,
    /// Hybrid search engine
    search: Arc<RwLock<HybridSearchSqlite>>,
    /// Embedding provider
    embedding: Arc<dyn EmbeddingProvider>,
    /// Index status tracking
    indexed_hashes: Arc<RwLock<HashMap<String, bool>>>,
}

/// Search result with file context
#[derive(Debug, Clone)]
pub struct MemorySearchResult {
    /// Memory entry
    pub entry: MarkdownMemoryEntry,
    /// Source file type
    pub file_type: MemoryFileType,
    /// Source file path
    pub file_path: PathBuf,
    /// Search relevance scores
    pub vector_score: f32,
    pub bm25_score: f32,
    pub combined_score: f32,
}

/// Indexing progress callback
pub type IndexingProgressCallback = Box<dyn Fn(usize, usize) + Send + Sync>;

impl UnifiedMemorySystem {
    /// Create new unified memory system
    pub async fn new(config: UnifiedMemoryConfig) -> Result<Self> {
        Self::new_internal(config, true).await
    }

    /// Create new unified memory system without initial indexing (for tests)
    pub async fn new_without_initial_indexing(config: UnifiedMemoryConfig) -> Result<Self> {
        Self::new_internal(config, false).await
    }

    /// Internal constructor
    async fn new_internal(config: UnifiedMemoryConfig, do_initial_indexing: bool) -> Result<Self> {
        // Initialize storage
        let storage = MarkdownStorage::new(config.storage_config.clone())?;
        storage.initialize_workspace().await?;

        // Initialize search engine
        let search = HybridSearchSqlite::new(&config.search_db_path, Default::default())?;

        // Initialize embedding provider
        let embedding = EmbeddingProviderFactory::create(config.embedding_config.clone())?;

        info!(
            "Unified memory system initialized with {}-dimensional embeddings",
            embedding.dimension()
        );

        let system = Self {
            config,
            storage,
            search: Arc::new(RwLock::new(search)),
            embedding: Arc::from(embedding),
            indexed_hashes: Arc::new(RwLock::new(HashMap::new())),
        };

        // Perform initial indexing if needed
        if do_initial_indexing {
            system.perform_initial_indexing(None).await?;
        }

        Ok(system)
    }

    /// Create with default configuration
    pub async fn default() -> Result<Self> {
        Self::new(UnifiedMemoryConfig::default()).await
    }

    /// Store a memory entry with automatic indexing
    pub async fn store(
        &self,
        file_type: MemoryFileType,
        entry: &MarkdownMemoryEntry,
        date: Option<chrono::NaiveDate>,
    ) -> Result<()> {
        // 1. Store in Markdown file
        self.storage.append_entry(file_type, entry, date).await?;

        // 2. Auto-index if enabled
        if self.config.auto_index {
            self.index_entry(entry).await?;
        }

        debug!("Stored and indexed entry: {}", entry.id);
        Ok(())
    }

    /// Index a single entry
    pub async fn index_entry(&self, entry: &MarkdownMemoryEntry) -> Result<()> {
        let content_hash = Self::hash_content(&format!("{}{}", entry.title, entry.content));

        // Check if already indexed
        {
            let indexed = self.indexed_hashes.read().await;
            if indexed.get(&content_hash).copied().unwrap_or(false) {
                debug!("Entry {} already indexed, skipping", entry.id);
                return Ok(());
            }
        }

        // Generate embedding
        let text_to_embed = format!("{} {}", entry.title, entry.content);
        let embedding = self.embedding.embed(&text_to_embed).await?;

        // Convert HashMap<String, String> for metadata
        let mut metadata = entry.metadata.clone();
        metadata.insert("title".to_string(), entry.title.clone());
        metadata.insert("category".to_string(), entry.category.clone());

        // Index in SQLite
        let file_path = self.get_file_path_for_entry(entry);
        let search = self.search.read().await;
        search.index_entry(
            entry.id,
            &format!("{}\n{}", entry.title, entry.content),
            &embedding,
            metadata,
            file_path.as_deref(),
        )?;

        // Mark as indexed
        let mut indexed = self.indexed_hashes.write().await;
        indexed.insert(content_hash, true);

        debug!("Indexed entry: {} ({} dims)", entry.id, embedding.len());
        Ok(())
    }

    /// Index multiple entries in batch
    pub async fn index_entries_batch(
        &self,
        entries: &[MarkdownMemoryEntry],
        progress_callback: Option<IndexingProgressCallback>,
    ) -> Result<()> {
        let total = entries.len();
        let mut processed = 0;

        for chunk in entries.chunks(self.config.index_batch_size) {
            // Prepare texts for batch embedding
            let texts: Vec<String> = chunk
                .iter()
                .map(|e| format!("{} {}", e.title, e.content))
                .collect();

            // Generate embeddings in batch
            let embeddings = self.embedding.embed_batch(&texts).await?;

            // Index each entry
            for (entry, embedding) in chunk.iter().zip(embeddings.iter()) {
                let content_hash = Self::hash_content(&format!("{}{}", entry.title, entry.content));

                // Check if already indexed
                {
                    let indexed = self.indexed_hashes.read().await;
                    if indexed.get(&content_hash).copied().unwrap_or(false) {
                        continue;
                    }
                }

                let mut metadata = entry.metadata.clone();
                metadata.insert("title".to_string(), entry.title.clone());
                metadata.insert("category".to_string(), entry.category.clone());

                let file_path = self.get_file_path_for_entry(entry);
                let search = self.search.read().await;
                search.index_entry(
                    entry.id,
                    &format!("{}\n{}", entry.title, entry.content),
                    embedding,
                    metadata,
                    file_path.as_deref(),
                )?;

                let mut indexed = self.indexed_hashes.write().await;
                indexed.insert(content_hash, true);

                processed += 1;
            }

            // Report progress
            if let Some(ref callback) = progress_callback {
                callback(processed, total);
            }

            info!("Indexed batch: {}/{} entries", processed, total);
        }

        Ok(())
    }

    /// Search across all memory files
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<MemorySearchResult>> {
        // Generate query embedding
        let query_embedding = self.embedding.embed(query).await.ok();

        // Perform hybrid search
        let search = self.search.read().await;
        let sqlite_results = search.search(query, query_embedding.as_deref(), limit)?;

        // Convert to MemorySearchResult
        let mut results = Vec::with_capacity(sqlite_results.len());
        for sqlite_result in sqlite_results {
            if let Some(memory_result) = self.convert_sqlite_result(&sqlite_result, query).await? {
                results.push(memory_result);
            }
        }

        // Sort by combined score
        results.sort_by(|a, b| {
            b.combined_score
                .partial_cmp(&a.combined_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(results)
    }

    /// Search with options
    pub async fn search_with_options(
        &self,
        query: &str,
        file_types: Option<&[MemoryFileType]>,
        date_range: Option<(chrono::NaiveDate, chrono::NaiveDate)>,
        limit: usize,
    ) -> Result<Vec<MemorySearchResult>> {
        // Perform search
        let all_results = self.search(query, limit * 2).await?;

        // Filter results
        let filtered: Vec<_> = all_results
            .into_iter()
            .filter(|r| {
                // Filter by file type
                if let Some(types) = file_types {
                    if !types.contains(&r.file_type) {
                        return false;
                    }
                }

                // Filter by date range
                if let Some((start, end)) = date_range {
                    let entry_date = r.entry.timestamp.date_naive();
                    if entry_date < start || entry_date > end {
                        return false;
                    }
                }

                true
            })
            .take(limit)
            .collect();

        Ok(filtered)
    }

    /// Rebuild search index from all Markdown files
    pub async fn rebuild_index(
        &self,
        progress_callback: Option<IndexingProgressCallback>,
    ) -> Result<IndexingStats> {
        info!("Starting full index rebuild...");

        let start_time = std::time::Instant::now();

        // Clear existing index
        {
            let _search = self.search.read().await;
            // Note: In a real implementation, we might want a method to clear
            // the index For now, we'll just re-index everything
        }

        let mut indexed = self.indexed_hashes.write().await;
        indexed.clear();
        drop(indexed);

        // Collect all entries from Markdown files
        let mut all_entries: Vec<MarkdownMemoryEntry> = Vec::new();

        // Core memory
        if let Ok(entries) = self.storage.read_entries(MemoryFileType::Core, None).await {
            info!(
                "📖 Loaded {} entries from core memory (MEMORY.md)",
                entries.len()
            );
            all_entries.extend(entries);
        }

        // User profile
        if let Ok(entries) = self.storage.read_entries(MemoryFileType::User, None).await {
            info!(
                "👤 Loaded {} entries from user profile (USER.md)",
                entries.len()
            );
            all_entries.extend(entries);
        }

        // Recent daily logs (last 30 days)
        let today = chrono::Local::now().date_naive();
        let mut daily_count = 0;
        for i in 0..30 {
            let date = today - chrono::Duration::days(i);
            if let Ok(entries) = self
                .storage
                .read_entries(MemoryFileType::Daily, Some(date))
                .await
            {
                daily_count += entries.len();
                all_entries.extend(entries);
            }
        }
        if daily_count > 0 {
            info!(
                "📅 Loaded {} entries from daily logs (last 30 days)",
                daily_count
            );
        }

        // Other memory types
        for file_type in [
            MemoryFileType::Soul,
            MemoryFileType::Agents,
            MemoryFileType::Heartbeat,
        ] {
            if let Ok(entries) = self.storage.read_entries(file_type, None).await {
                let name = match file_type {
                    MemoryFileType::Soul => "SOUL.md",
                    MemoryFileType::Agents => "AGENTS.md",
                    MemoryFileType::Heartbeat => "HEARTBEAT.md",
                    _ => "unknown",
                };
                info!(
                    "📖 Loaded {} entries from {} ({})",
                    entries.len(),
                    name,
                    file_type.filename(None)
                );
                all_entries.extend(entries);
            }
        }

        let total_entries = all_entries.len();
        info!(
            "🔍 Found {} total entries to index across all memory files",
            total_entries
        );

        // Batch index all entries
        self.index_entries_batch(&all_entries, progress_callback)
            .await?;

        let duration = start_time.elapsed();
        let stats = IndexingStats {
            total_entries,
            indexed_entries: total_entries,
            duration_secs: duration.as_secs_f64(),
            avg_time_per_entry_ms: if total_entries > 0 {
                duration.as_millis() as f64 / total_entries as f64
            } else {
                0.0
            },
        };

        info!("Index rebuild completed: {:?}", stats);
        Ok(stats)
    }

    /// Perform initial indexing on startup
    async fn perform_initial_indexing(
        &self,
        progress_callback: Option<IndexingProgressCallback>,
    ) -> Result<()> {
        // Check if database is empty
        let search = self.search.read().await;
        let stats = search.get_stats()?;

        if stats.total_entries == 0 {
            info!("Search index is empty, performing initial indexing...");
            drop(search);
            self.rebuild_index(progress_callback).await?;
        } else {
            info!(
                "Search index already contains {} entries",
                stats.total_entries
            );
        }

        Ok(())
    }

    /// Get search database statistics
    pub async fn get_search_stats(&self) -> Result<SearchDatabaseStats> {
        let search = self.search.read().await;
        search.get_stats()
    }

    /// Get storage directory
    pub fn workspace_dir(&self) -> &Path {
        self.storage.workspace_dir()
    }

    /// Convert SQLite search result to MemorySearchResult
    async fn convert_sqlite_result(
        &self,
        sqlite_result: &SqliteSearchResult,
        _query: &str,
    ) -> Result<Option<MemorySearchResult>> {
        // Try to find the corresponding file type and path
        let file_path = sqlite_result
            .entry
            .file_path
            .as_ref()
            .map(PathBuf::from)
            .or_else(|| self.infer_file_path(&sqlite_result.entry));

        let file_type = file_path
            .as_ref()
            .and_then(|p| self.infer_file_type(p))
            .unwrap_or(MemoryFileType::Core);

        // Reconstruct MarkdownMemoryEntry from SqliteMemoryEntry
        let md_entry = MarkdownMemoryEntry {
            id: sqlite_result.entry.id,
            title: sqlite_result
                .entry
                .metadata
                .get("title")
                .cloned()
                .unwrap_or_else(|| "Untitled".to_string()),
            content: sqlite_result.entry.content.clone(),
            timestamp: sqlite_result.entry.timestamp,
            category: sqlite_result
                .entry
                .metadata
                .get("category")
                .cloned()
                .unwrap_or_else(|| "general".to_string()),
            importance: sqlite_result
                .entry
                .metadata
                .get("importance")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.5),
            metadata: sqlite_result.entry.metadata.clone(),
            session_id: sqlite_result.entry.metadata.get("session_id").cloned(),
        };

        Ok(Some(MemorySearchResult {
            entry: md_entry,
            file_type,
            file_path: file_path.unwrap_or_else(|| self.workspace_dir().join("MEMORY.md")),
            vector_score: sqlite_result.vector_score,
            bm25_score: sqlite_result.bm25_score,
            combined_score: sqlite_result.combined_score,
        }))
    }

    /// Infer file path from entry
    fn get_file_path_for_entry(&self, entry: &MarkdownMemoryEntry) -> Option<String> {
        // Infer from metadata or category
        if let Some(path) = entry.metadata.get("file_path") {
            return Some(path.clone());
        }

        // Default paths based on category
        let filename = match entry.category.as_str() {
            "user" | "profile" => "USER.md",
            "soul" => "SOUL.md",
            "agents" => "AGENTS.md",
            "heartbeat" => "HEARTBEAT.md",
            "daily" => return Some(format!("memory/{}.md", entry.timestamp.format("%Y-%m-%d"))),
            _ => "MEMORY.md",
        };

        Some(filename.to_string())
    }

    /// Infer file path from entry content/metadata
    fn infer_file_path(&self, entry: &SqliteMemoryEntry) -> Option<PathBuf> {
        entry
            .file_path
            .as_ref()
            .map(|p| self.workspace_dir().join(p))
    }

    /// Infer file type from path
    fn infer_file_type(&self, path: &Path) -> Option<MemoryFileType> {
        let file_name = path.file_name()?.to_str()?;

        match file_name {
            "MEMORY.md" => Some(MemoryFileType::Core),
            "USER.md" => Some(MemoryFileType::User),
            "SOUL.md" => Some(MemoryFileType::Soul),
            "AGENTS.md" => Some(MemoryFileType::Agents),
            "HEARTBEAT.md" => Some(MemoryFileType::Heartbeat),
            _ if file_name.ends_with(".md") => {
                // Check if in memory subdirectory
                if path.parent()?.file_name()?.to_str()? == "memory" {
                    Some(MemoryFileType::Daily)
                } else {
                    None
                }
            }
            _ => None,
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

    /// Access raw storage (for advanced use cases)
    pub fn storage(&self) -> &MarkdownStorage {
        &self.storage
    }

    /// Access raw search engine (for advanced use cases)
    pub fn search_engine(&self) -> &Arc<RwLock<HybridSearchSqlite>> {
        &self.search
    }

    /// Access embedding provider (for advanced use cases)
    pub fn embedding(&self) -> &Arc<dyn EmbeddingProvider> {
        &self.embedding
    }
}

#[async_trait::async_trait]
impl MemorySearch for UnifiedMemorySystem {
    async fn search(&self, query: &str) -> Result<Vec<SearchResult>> {
        let search = self.search.read().await;
        MemorySearch::search(&*search, query).await
    }

    async fn search_with_config(
        &self,
        query: &str,
        config: SearchConfig,
    ) -> Result<Vec<SearchResult>> {
        let search = self.search.read().await;
        MemorySearch::search_with_config(&*search, query, config).await
    }

    async fn semantic_search(&self, query_embedding: &[f32]) -> Result<Vec<SearchResult>> {
        let search = self.search.read().await;
        MemorySearch::semantic_search(&*search, query_embedding).await
    }

    async fn keyword_search(&self, keywords: &[String]) -> Result<Vec<SearchResult>> {
        let search = self.search.read().await;
        MemorySearch::keyword_search(&*search, keywords).await
    }

    async fn add_entry(
        &self,
        id: Uuid,
        content: &str,
        metadata: HashMap<String, String>,
    ) -> Result<()> {
        let search = self.search.read().await;
        MemorySearch::add_entry(&*search, id, content, metadata).await
    }

    async fn remove_entry(&self, id: Uuid) -> Result<()> {
        let search = self.search.read().await;
        MemorySearch::remove_entry(&*search, id).await
    }

    async fn update_entry(
        &self,
        id: Uuid,
        content: &str,
        metadata: HashMap<String, String>,
    ) -> Result<()> {
        let search = self.search.read().await;
        MemorySearch::update_entry(&*search, id, content, metadata).await
    }

    fn stats(&self) -> SearchStats {
        if let Ok(search) = self.search.try_read() {
            MemorySearch::stats(&*search)
        } else {
            SearchStats::default()
        }
    }

    async fn clear(&self) -> Result<()> {
        let search = self.search.read().await;
        MemorySearch::clear(&*search).await
    }
}

/// Indexing statistics
#[derive(Debug, Clone)]
pub struct IndexingStats {
    pub total_entries: usize,
    pub indexed_entries: usize,
    pub duration_secs: f64,
    pub avg_time_per_entry_ms: f64,
}

/// File watcher for detecting external changes
pub struct MemoryFileWatcher {
    #[allow(dead_code)]
    storage: Arc<MarkdownStorage>,
    #[allow(dead_code)]
    search: Arc<RwLock<HybridSearchSqlite>>,
    #[allow(dead_code)]
    embedding: Arc<dyn EmbeddingProvider>,
}

impl MemoryFileWatcher {
    /// Create new file watcher
    pub fn new(
        storage: Arc<MarkdownStorage>,
        search: Arc<RwLock<HybridSearchSqlite>>,
        embedding: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        Self {
            storage,
            search,
            embedding,
        }
    }

    /// Start watching for file changes
    pub async fn start_watching(&self) -> Result<()> {
        // TODO: Implement using notify crate
        // This would watch the workspace directory for changes
        // and re-index modified files automatically
        info!("File watcher started (TODO: implement with notify crate)");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::memory::embedding::EmbeddingConfig;

    async fn create_test_system() -> (UnifiedMemorySystem, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path().join("workspace");

        let config = UnifiedMemoryConfig {
            storage_config: MarkdownStorageConfig {
                workspace_dir: workspace.clone(),
                ..Default::default()
            },
            search_db_path: workspace.join("search.db"),
            embedding_config: EmbeddingConfig::mock(64), // Use mock for tests
            auto_index: true,                            // Enable auto-index for tests
            index_batch_size: 5,
        };

        let system = UnifiedMemorySystem::new_without_initial_indexing(config)
            .await
            .unwrap();
        (system, temp_dir)
    }

    #[tokio::test]
    async fn test_store_and_search() {
        let (system, _temp) = create_test_system().await;

        // Store an entry
        let entry = MarkdownMemoryEntry::new("Test Title", "Test content about databases")
            .with_category("technical")
            .with_importance(0.8);

        system
            .store(MemoryFileType::Core, &entry, None)
            .await
            .unwrap();

        // Search
        let results = system.search("database", 10).await.unwrap();
        assert!(!results.is_empty());

        // Verify result content
        let first = &results[0];
        assert_eq!(first.entry.title, "Test Title");
        assert!(first.combined_score > 0.0);
    }

    #[tokio::test]
    async fn test_search_with_filter() {
        let (system, _temp) = create_test_system().await;

        // Store entries in different file types
        let core_entry = MarkdownMemoryEntry::new("Core Memory", "Important preference")
            .with_category("preference");
        system
            .store(MemoryFileType::Core, &core_entry, None)
            .await
            .unwrap();

        let user_entry =
            MarkdownMemoryEntry::new("User Profile", "User name is John").with_category("profile");
        system
            .store(MemoryFileType::User, &user_entry, None)
            .await
            .unwrap();

        // Search with file type filter
        let results = system
            .search_with_options("Profile", Some(&[MemoryFileType::User]), None, 10)
            .await
            .unwrap();

        assert!(!results.is_empty());
        assert_eq!(results[0].file_type, MemoryFileType::User);
    }

    #[tokio::test]
    async fn test_batch_indexing() {
        let (system, _temp) = create_test_system().await;

        let entries: Vec<_> = (0..10)
            .map(|i| {
                MarkdownMemoryEntry::new(format!("Entry {}", i), format!("Content for entry {}", i))
            })
            .collect();

        // Batch index
        use std::sync::atomic::{AtomicUsize, Ordering};
        let progress_calls = Arc::new(AtomicUsize::new(0));
        let callback_calls = progress_calls.clone();
        let callback: IndexingProgressCallback = Box::new(move |_processed, _total| {
            callback_calls.fetch_add(1, Ordering::SeqCst);
        });

        system
            .index_entries_batch(&entries, Some(callback))
            .await
            .unwrap();
        // Verify callback was called
        assert!(progress_calls.load(Ordering::SeqCst) > 0);

        // All entries should be searchable
        let results = system.search("Content", 20).await.unwrap();
        assert_eq!(results.len(), 10);
    }

    #[tokio::test]
    async fn test_rebuild_index() {
        let (system, _temp) = create_test_system().await;

        // Store some entries
        for i in 0..5 {
            let entry = MarkdownMemoryEntry::new(format!("Title {}", i), format!("Content {}", i));
            system
                .store(MemoryFileType::Core, &entry, None)
                .await
                .unwrap();
        }

        // Rebuild index
        let stats = system.rebuild_index(None).await.unwrap();
        // Note: rebuild_index indexes all entries from storage, including default
        // templates We just verify that it successfully indexes entries and
        // returns valid stats
        assert!(
            stats.total_entries >= 5,
            "Expected at least 5 entries, got {}",
            stats.total_entries
        );
        assert!(stats.duration_secs > 0.0);
    }
}
