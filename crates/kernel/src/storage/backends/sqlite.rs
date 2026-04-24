//! SQLite Storage Backend
//!
//! ACID-compliant persistent storage using SQLite via rusqlite.
//! Requires `sqlite` feature to be enabled.

#[cfg(feature = "sqlite")]
use std::path::Path;

#[cfg(feature = "sqlite")]
use parking_lot::Mutex;

#[cfg(feature = "sqlite")]
use crate::storage::{EntryMetadata, StorageBackend, StorageEntry, StorageError};

/// SQLite storage backend
#[cfg(feature = "sqlite")]
pub struct SqliteStorage {
    conn: Mutex<rusqlite::Connection>,
}

#[cfg(feature = "sqlite")]
impl std::fmt::Debug for SqliteStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteStorage")
            .field("conn", &"<Connection>")
            .finish()
    }
}

#[cfg(feature = "sqlite")]
impl SqliteStorage {
    /// Open or create SQLite database at given path
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, StorageError> {
        let conn = rusqlite::Connection::open(path)
            .map_err(|e| StorageError::IoError(format!("Failed to open SQLite: {}", e)))?;

        let storage = Self {
            conn: Mutex::new(conn),
        };

        // Initialize schema
        storage.init_schema()?;

        Ok(storage)
    }

    /// Create in-memory SQLite database (for testing)
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let conn = rusqlite::Connection::open_in_memory().map_err(|e| {
            StorageError::IoError(format!("Failed to create in-memory SQLite: {}", e))
        })?;

        let storage = Self {
            conn: std::sync::Mutex::new(conn),
        };

        storage.init_schema()?;

        Ok(storage)
    }

    /// Initialize database schema
    fn init_schema(&self) -> Result<(), StorageError> {
        let conn = self.conn.lock();

        // Create main storage table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS storage (
                key TEXT PRIMARY KEY,
                data BLOB NOT NULL,
                created_at INTEGER NOT NULL,
                modified_at INTEGER NOT NULL,
                size_bytes INTEGER NOT NULL,
                content_type TEXT NOT NULL,
                checksum TEXT NOT NULL,
                tags TEXT
            )",
            [],
        )
        .map_err(|e| StorageError::IoError(format!("Failed to create table: {}", e)))?;

        // Create index for prefix search
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_key_prefix ON storage(key)",
            [],
        )
        .map_err(|e| StorageError::IoError(format!("Failed to create index: {}", e)))?;

        // Enable WAL mode for better concurrency
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;",
        )
        .map_err(|e| StorageError::IoError(format!("Failed to set pragmas: {}", e)))?;

        Ok(())
    }

    /// Begin transaction
    pub fn begin_transaction(&self) -> Result<SqliteTransaction, StorageError> {
        let conn = self.conn.lock();
        conn.execute("BEGIN TRANSACTION", [])
            .map_err(|e| StorageError::IoError(format!("Failed to begin transaction: {}", e)))?;
        Ok(SqliteTransaction { storage: self })
    }

    /// Get database statistics
    pub fn stats(&self) -> Result<SqliteStats, StorageError> {
        let conn = self.conn.lock();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM storage", [], |row| row.get(0))
            .map_err(|e| StorageError::IoError(format!("Failed to count: {}", e)))?;

        let size: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(LENGTH(data)), 0) FROM storage",
                [],
                |row| row.get(0),
            )
            .map_err(|e| StorageError::IoError(format!("Failed to sum: {}", e)))?;

        Ok(SqliteStats {
            entry_count: count as u64,
            total_data_size: size as u64,
        })
    }

    /// Vacuum database (reclaim space)
    pub fn vacuum(&self) -> Result<(), StorageError> {
        let conn = self.conn.lock();
        conn.execute("VACUUM", [])
            .map_err(|e| StorageError::IoError(format!("Failed to vacuum: {}", e)))
    }

    /// Execute custom query (use with caution)
    pub fn execute(&self, sql: &str) -> Result<(), StorageError> {
        let conn = self.conn.lock();
        conn.execute(sql, [])
            .map_err(|e| StorageError::IoError(format!("Query failed: {}", e)))?;
        Ok(())
    }
}

#[cfg(feature = "sqlite")]
impl StorageBackend for SqliteStorage {
    fn put(&self, key: &str, data: &[u8], metadata: EntryMetadata) -> Result<(), StorageError> {
        let conn = self.conn.lock();

        let tags = metadata.tags.join(",");

        conn.execute(
            "INSERT OR REPLACE INTO storage 
             (key, data, created_at, modified_at, size_bytes, content_type, checksum, tags)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                key,
                data,
                metadata.created_at as i64,
                metadata.modified_at as i64,
                metadata.size_bytes as i64,
                metadata.content_type,
                metadata.checksum,
                tags,
            ],
        )
        .map_err(|e| StorageError::IoError(format!("Failed to insert: {}", e)))?;

        Ok(())
    }

    fn get(&self, key: &str) -> Result<Option<StorageEntry>, StorageError> {
        let conn = self.conn.lock();

        let mut stmt = conn
            .prepare(
                "SELECT key, data, created_at, modified_at, size_bytes, content_type, checksum, \
                 tags 
                 FROM storage WHERE key = ?1",
            )
            .map_err(|e| StorageError::IoError(format!("Failed to prepare: {}", e)))?;

        let result = stmt
            .query_row([key], |row| {
                let tags_str: String = row.get(7).unwrap_or_default();
                let tags: Vec<String> = if tags_str.is_empty() {
                    vec![]
                } else {
                    tags_str.split(',').map(|s| s.to_string()).collect()
                };

                Ok(StorageEntry {
                    key: row.get(0)?,
                    data: row.get(1)?,
                    metadata: EntryMetadata {
                        created_at: row.get::<_, i64>(2)? as u64,
                        modified_at: row.get::<_, i64>(3)? as u64,
                        size_bytes: row.get::<_, i64>(4)? as u64,
                        content_type: row.get(5)?,
                        checksum: row.get(6)?,
                        tags,
                    },
                })
            })
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                _ => Err(e),
            })
            .map_err(|e| StorageError::IoError(format!("Query failed: {}", e)))?;

        Ok(result)
    }

    fn delete(&self, key: &str) -> Result<(), StorageError> {
        let conn = self.conn.lock();

        conn.execute("DELETE FROM storage WHERE key = ?1", [key])
            .map_err(|e| StorageError::IoError(format!("Failed to delete: {}", e)))?;

        Ok(())
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
        let conn = self.conn.lock();

        let pattern = format!("{}%", prefix);
        let mut stmt = conn
            .prepare("SELECT key FROM storage WHERE key LIKE ?1 ORDER BY key")
            .map_err(|e| StorageError::IoError(format!("Failed to prepare: {}", e)))?;

        let keys: Result<Vec<String>, _> = stmt
            .query_map([&pattern], |row| row.get(0))
            .map_err(|e| StorageError::IoError(format!("Query failed: {}", e)))?
            .collect();

        keys.map_err(|e| StorageError::IoError(format!("Row mapping failed: {}", e)))
    }

    // Uses default exists() implementation from trait
}

/// SQLite transaction handle
#[cfg(feature = "sqlite")]
pub struct SqliteTransaction<'a> {
    storage: &'a SqliteStorage,
}

#[cfg(feature = "sqlite")]
impl<'a> SqliteTransaction<'a> {
    /// Commit transaction
    pub fn commit(self) -> Result<(), StorageError> {
        let conn = self.storage.conn.lock().unwrap();
        conn.execute("COMMIT", [])
            .map_err(|e| StorageError::IoError(format!("Failed to commit: {}", e)))
    }

    /// Rollback transaction
    pub fn rollback(self) -> Result<(), StorageError> {
        let conn = self.storage.conn.lock().unwrap();
        conn.execute("ROLLBACK", [])
            .map_err(|e| StorageError::IoError(format!("Failed to rollback: {}", e)))
    }
}

/// SQLite statistics
#[derive(Debug, Clone)]
pub struct SqliteStats {
    /// Number of entries in database
    pub entry_count: u64,
    /// Total data size in bytes
    pub total_data_size: u64,
}

// Generate stub implementation using macro
crate::define_stub_backend!(SqliteStorage, "sqlite");

#[cfg(test)]
#[cfg(feature = "sqlite")]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::storage::test_utils::create_test_metadata;

    #[test]
    fn test_basic_operations() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SqliteStorage::open(temp_dir.path().join("test.db")).unwrap();
        let mut metadata = create_test_metadata();
        metadata.tags = vec!["tag1".to_string(), "tag2".to_string()];

        // Test put and get
        storage.put("key1", b"value1", metadata.clone()).unwrap();
        let entry = storage.get("key1").unwrap().unwrap();
        assert_eq!(entry.data, b"value1");
        assert_eq!(entry.metadata.tags, vec!["tag1", "tag2"]);

        // Test exists
        assert!(storage.exists("key1").unwrap());
        assert!(!storage.exists("nonexistent").unwrap());

        // Test delete
        storage.delete("key1").unwrap();
        assert!(!storage.exists("key1").unwrap());
    }

    #[test]
    fn test_list_with_prefix() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SqliteStorage::open(temp_dir.path().join("test.db")).unwrap();
        let metadata = create_test_metadata();

        storage.put("prefix:1", b"v1", metadata.clone()).unwrap();
        storage.put("prefix:2", b"v2", metadata.clone()).unwrap();
        storage.put("other", b"v3", metadata.clone()).unwrap();

        let keys = storage.list("prefix:").unwrap();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"prefix:1".to_string()));
        assert!(keys.contains(&"prefix:2".to_string()));
    }

    #[test]
    fn test_in_memory() {
        let storage = SqliteStorage::open_in_memory().unwrap();
        let metadata = create_test_metadata();

        storage.put("mem_key", b"mem_value", metadata).unwrap();
        let entry = storage.get("mem_key").unwrap().unwrap();
        assert_eq!(entry.data, b"mem_value");
    }

    #[test]
    fn test_stats() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SqliteStorage::open(temp_dir.path().join("test.db")).unwrap();
        let metadata = create_test_metadata();

        storage.put("key1", b"value1", metadata.clone()).unwrap();
        storage.put("key2", b"value2", metadata).unwrap();

        let stats = storage.stats().unwrap();
        assert_eq!(stats.entry_count, 2);
        assert!(stats.total_data_size > 0);
    }

    #[test]
    fn test_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("persistent.db");
        let metadata = create_test_metadata();

        // Create and write
        {
            let storage = SqliteStorage::open(&path).unwrap();
            storage.put("persistent", b"data", metadata).unwrap();
        }

        // Reopen and verify
        {
            let storage = SqliteStorage::open(&path).unwrap();
            let entry = storage.get("persistent").unwrap().unwrap();
            assert_eq!(entry.data, b"data");
        }
    }

    #[test]
    fn test_update_existing() {
        let temp_dir = TempDir::new().unwrap();
        let storage = SqliteStorage::open(temp_dir.path().join("test.db")).unwrap();

        let mut metadata = create_test_metadata();
        storage.put("key1", b"original", metadata.clone()).unwrap();

        metadata.modified_at = 9999999999;
        storage.put("key1", b"updated", metadata.clone()).unwrap();

        let entry = storage.get("key1").unwrap().unwrap();
        assert_eq!(entry.data, b"updated");
        assert_eq!(entry.metadata.modified_at, 9999999999);
    }
}
