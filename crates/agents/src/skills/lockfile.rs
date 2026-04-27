//! Skill Lockfile
//!
//! Tracks installed skills with their origin, version, and install time.
//! Stored at `skills/lock.json`.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Lockfile containing all installed skill entries.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillLockfile {
    pub entries: Vec<LockEntry>,
}

/// A single installed skill entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockEntry {
    pub id: String,
    pub version: String,
    pub installed_at: u64,
    /// Origin: "clawhub", "beehub", "local"
    pub origin: String,
    /// Source-specific ID (e.g., clawhub slug)
    pub source_id: String,
}

impl SkillLockfile {
    /// Load lockfile from path, or return empty if not exists.
    pub async fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| format!("Failed to read lockfile: {}", e))?;

        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse lockfile: {}", e))
    }

    /// Save lockfile to path.
    pub async fn save(&self, path: impl AsRef<Path>) -> Result<(), String> {
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize lockfile: {}", e))?;

        tokio::fs::write(path, content)
            .await
            .map_err(|e| format!("Failed to write lockfile: {}", e))
    }

    /// Add or update an entry.
    pub fn upsert(&mut self, id: String, version: String, origin: String, source_id: String) {
        let installed_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Remove existing entry with same ID
        self.entries.retain(|e| e.id != id);

        self.entries.push(LockEntry {
            id,
            version,
            installed_at,
            origin,
            source_id,
        });
    }

    /// Remove an entry by ID.
    pub fn remove(&mut self, id: &str) -> bool {
        let len_before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        self.entries.len() < len_before
    }

    /// Find entry by ID.
    pub fn get(&self, id: &str) -> Option<&LockEntry> {
        self.entries.iter().find(|e| e.id == id)
    }
}
