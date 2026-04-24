//! Memory Backup

use std::path::Path;

use super::MemoryEntry;
use crate::error::Result;

/// Memory backup manager
pub struct MemoryBackup {
    backup_path: std::path::PathBuf,
}

impl MemoryBackup {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            backup_path: path.as_ref().to_path_buf(),
        }
    }

    pub async fn backup(&self, entries: &[MemoryEntry]) -> Result<()> {
        let json = serde_json::to_string_pretty(entries)
            .map_err(|e| crate::error::AgentError::serialization(e.to_string()))?;

        tokio::fs::write(&self.backup_path, json)
            .await
            .map_err(|e| crate::error::AgentError::io(e.to_string()))?;

        Ok(())
    }

    pub async fn restore(&self) -> Result<Vec<MemoryEntry>> {
        if !self.backup_path.exists() {
            return Ok(Vec::new());
        }

        let json = tokio::fs::read_to_string(&self.backup_path)
            .await
            .map_err(|e| crate::error::AgentError::io(e.to_string()))?;

        let entries: Vec<MemoryEntry> = serde_json::from_str(&json)
            .map_err(|e| crate::error::AgentError::serialization(e.to_string()))?;

        Ok(entries)
    }
}
