//! Message History Module
//!
//! Provides comprehensive message history tracking including:
//! - Edit history with version control
//! - Deletion tracking with audit trail
//! - Pin/unpin operations tracking
//! - Full-text search in history
//! - Data export capabilities

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Message history store
pub struct MessageHistoryStore {
    /// Edit history indexed by channel and message
    edits: RwLock<HashMap<String, HashMap<String, Vec<MessageEditRecord>>>>,
    /// Deletion records indexed by channel
    deletions: RwLock<HashMap<String, Vec<MessageDeletionRecord>>>,
    /// Pin records indexed by channel
    pins: RwLock<HashMap<String, Vec<MessagePinRecord>>>,
    /// Message snapshots for full history reconstruction
    snapshots: RwLock<HashMap<String, HashMap<String, Vec<MessageSnapshot>>>>,
}

/// Message edit record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEditRecord {
    pub record_id: String,
    pub message_id: String,
    pub channel_id: String,
    pub author_id: String,
    pub previous_content: String,
    pub new_content: String,
    pub edited_at: DateTime<Utc>,
    pub edit_reason: Option<String>,
    pub version: u32,
    pub is_major_edit: bool, // Significant content change
}

/// Message deletion record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDeletionRecord {
    pub record_id: String,
    pub message_id: String,
    pub channel_id: String,
    pub original_author_id: String,
    pub original_content: String,
    pub original_timestamp: DateTime<Utc>,
    pub deleted_at: DateTime<Utc>,
    pub deleted_by: String,
    pub deletion_reason: Option<String>,
    pub is_bulk_deletion: bool,
}

/// Message pin record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagePinRecord {
    pub record_id: String,
    pub message_id: String,
    pub channel_id: String,
    pub pinned_at: DateTime<Utc>,
    pub pinned_by: String,
    pub unpinned_at: Option<DateTime<Utc>>,
    pub unpinned_by: Option<String>,
    pub is_currently_pinned: bool,
}

/// Message snapshot for full reconstruction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSnapshot {
    pub snapshot_id: String,
    pub message_id: String,
    pub channel_id: String,
    pub content: String,
    pub author_id: String,
    pub timestamp: DateTime<Utc>,
    pub version: u32,
    pub metadata: HashMap<String, String>,
}

/// Message history query
#[derive(Debug, Clone)]
pub struct HistoryQuery {
    pub channel_id: Option<String>,
    pub message_id: Option<String>,
    pub author_id: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub operation_type: Option<OperationType>,
    pub limit: Option<usize>,
}

/// Operation types for history queries
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    Edit,
    Delete,
    Pin,
    Unpin,
    All,
}

/// History query result
#[derive(Debug, Clone)]
pub struct HistoryQueryResult {
    pub edits: Vec<MessageEditRecord>,
    pub deletions: Vec<MessageDeletionRecord>,
    pub pins: Vec<MessagePinRecord>,
    pub total_count: usize,
}

impl MessageHistoryStore {
    /// Create a new message history store
    pub fn new() -> Self {
        Self {
            edits: RwLock::new(HashMap::new()),
            deletions: RwLock::new(HashMap::new()),
            pins: RwLock::new(HashMap::new()),
            snapshots: RwLock::new(HashMap::new()),
        }
    }

    /// Record a message edit
    pub async fn record_edit(
        &self,
        message_id: &str,
        channel_id: &str,
        author_id: &str,
        previous_content: &str,
        new_content: &str,
        reason: Option<&str>,
    ) -> MessageEditRecord {
        let record = MessageEditRecord {
            record_id: Uuid::new_v4().to_string(),
            message_id: message_id.to_string(),
            channel_id: channel_id.to_string(),
            author_id: author_id.to_string(),
            previous_content: previous_content.to_string(),
            new_content: new_content.to_string(),
            edited_at: Utc::now(),
            edit_reason: reason.map(|s| s.to_string()),
            version: self.get_next_version(message_id, channel_id).await,
            is_major_edit: Self::is_major_edit(previous_content, new_content),
        };

        // Store edit record
        {
            let mut edits = self.edits.write().await;
            let channel_edits = edits.entry(channel_id.to_string()).or_default();
            let message_edits = channel_edits.entry(message_id.to_string()).or_default();
            message_edits.push(record.clone());
        }

        // Store snapshot
        self.store_snapshot(
            message_id,
            channel_id,
            author_id,
            new_content,
            record.version,
        )
        .await;

        record
    }

    /// Record a message deletion
    pub async fn record_deletion(
        &self,
        message_id: &str,
        channel_id: &str,
        original_author_id: &str,
        original_content: &str,
        original_timestamp: DateTime<Utc>,
        deleted_by: &str,
        reason: Option<&str>,
        is_bulk: bool,
    ) -> MessageDeletionRecord {
        let record = MessageDeletionRecord {
            record_id: Uuid::new_v4().to_string(),
            message_id: message_id.to_string(),
            channel_id: channel_id.to_string(),
            original_author_id: original_author_id.to_string(),
            original_content: original_content.to_string(),
            original_timestamp,
            deleted_at: Utc::now(),
            deleted_by: deleted_by.to_string(),
            deletion_reason: reason.map(|s| s.to_string()),
            is_bulk_deletion: is_bulk,
        };

        let mut deletions = self.deletions.write().await;
        let channel_deletions = deletions.entry(channel_id.to_string()).or_default();
        channel_deletions.push(record.clone());

        record
    }

    /// Record a pin operation
    pub async fn record_pin(
        &self,
        message_id: &str,
        channel_id: &str,
        pinned_by: &str,
    ) -> MessagePinRecord {
        let record = MessagePinRecord {
            record_id: Uuid::new_v4().to_string(),
            message_id: message_id.to_string(),
            channel_id: channel_id.to_string(),
            pinned_at: Utc::now(),
            pinned_by: pinned_by.to_string(),
            unpinned_at: None,
            unpinned_by: None,
            is_currently_pinned: true,
        };

        let mut pins = self.pins.write().await;
        let channel_pins = pins.entry(channel_id.to_string()).or_default();

        // Mark any existing pin record as unpinned
        for pin in channel_pins.iter_mut() {
            if pin.message_id == message_id && pin.is_currently_pinned {
                pin.is_currently_pinned = false;
                pin.unpinned_at = Some(Utc::now());
            }
        }

        channel_pins.push(record.clone());

        record
    }

    /// Record an unpin operation
    pub async fn record_unpin(
        &self,
        message_id: &str,
        channel_id: &str,
        unpinned_by: &str,
    ) -> Option<MessagePinRecord> {
        let mut pins = self.pins.write().await;
        let channel_pins = pins.get_mut(channel_id)?;

        for pin in channel_pins.iter_mut() {
            if pin.message_id == message_id && pin.is_currently_pinned {
                pin.is_currently_pinned = false;
                pin.unpinned_at = Some(Utc::now());
                pin.unpinned_by = Some(unpinned_by.to_string());
                return Some(pin.clone());
            }
        }

        None
    }

    /// Get edit history for a message
    pub async fn get_edit_history(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> Option<Vec<MessageEditRecord>> {
        let edits = self.edits.read().await;
        edits.get(channel_id)?.get(message_id).cloned()
    }

    /// Get all edits for a channel
    pub async fn get_channel_edits(&self, channel_id: &str) -> Vec<MessageEditRecord> {
        let edits = self.edits.read().await;
        edits
            .get(channel_id)
            .map(|m| m.values().flat_map(|v| v.clone()).collect())
            .unwrap_or_default()
    }

    /// Get deletion records for a channel
    pub async fn get_channel_deletions(&self, channel_id: &str) -> Vec<MessageDeletionRecord> {
        let deletions = self.deletions.read().await;
        deletions.get(channel_id).cloned().unwrap_or_default()
    }

    /// Get pin records for a channel
    pub async fn get_channel_pins(&self, channel_id: &str) -> Vec<MessagePinRecord> {
        let pins = self.pins.read().await;
        pins.get(channel_id).cloned().unwrap_or_default()
    }

    /// Get currently pinned messages
    pub async fn get_currently_pinned(&self, channel_id: &str) -> Vec<MessagePinRecord> {
        let pins = self.pins.read().await;
        pins.get(channel_id)
            .map(|v| {
                v.iter()
                    .filter(|p| p.is_currently_pinned)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get message snapshots
    pub async fn get_snapshots(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> Option<Vec<MessageSnapshot>> {
        let snapshots = self.snapshots.read().await;
        snapshots.get(channel_id)?.get(message_id).cloned()
    }

    /// Reconstruct message at a specific version
    pub async fn reconstruct_message_at_version(
        &self,
        channel_id: &str,
        message_id: &str,
        version: u32,
    ) -> Option<MessageSnapshot> {
        let snapshots = self.get_snapshots(channel_id, message_id).await?;
        snapshots.into_iter().find(|s| s.version == version)
    }

    /// Query history
    pub async fn query(&self, query: HistoryQuery) -> HistoryQueryResult {
        let mut edits = Vec::new();
        let mut deletions = Vec::new();
        let mut pins = Vec::new();

        // Query edits
        if query.operation_type.is_none()
            || query.operation_type == Some(OperationType::Edit)
            || query.operation_type == Some(OperationType::All)
        {
            let all_edits = self.edits.read().await;

            let channels_to_check: Vec<String> = query
                .channel_id
                .as_ref()
                .map(|id| vec![id.clone()])
                .unwrap_or_else(|| all_edits.keys().cloned().collect());

            for channel_id in channels_to_check {
                if let Some(channel_edits) = all_edits.get(&channel_id) {
                    for (msg_id, msg_edits) in channel_edits {
                        if query
                            .message_id
                            .as_ref()
                            .map(|id| id == msg_id)
                            .unwrap_or(true)
                        {
                            for edit in msg_edits {
                                if Self::matches_query(edit, &query) {
                                    edits.push(edit.clone());
                                }
                            }
                        }
                    }
                }
            }
        }

        // Query deletions
        if query.operation_type.is_none()
            || query.operation_type == Some(OperationType::Delete)
            || query.operation_type == Some(OperationType::All)
        {
            let all_deletions = self.deletions.read().await;

            let channels_to_check: Vec<String> = query
                .channel_id
                .as_ref()
                .map(|id| vec![id.clone()])
                .unwrap_or_else(|| all_deletions.keys().cloned().collect());

            for channel_id in channels_to_check {
                if let Some(channel_deletions) = all_deletions.get(&channel_id) {
                    for deletion in channel_deletions {
                        if query
                            .message_id
                            .as_ref()
                            .map(|id| id == &deletion.message_id)
                            .unwrap_or(true)
                        {
                            if Self::deletion_matches_query(deletion, &query) {
                                deletions.push(deletion.clone());
                            }
                        }
                    }
                }
            }
        }

        // Query pins
        if query.operation_type == Some(OperationType::Pin)
            || query.operation_type == Some(OperationType::Unpin)
            || query.operation_type.is_none()
            || query.operation_type == Some(OperationType::All)
        {
            let all_pins = self.pins.read().await;

            let channels_to_check: Vec<String> = query
                .channel_id
                .as_ref()
                .map(|id| vec![id.clone()])
                .unwrap_or_else(|| all_pins.keys().cloned().collect());

            for channel_id in channels_to_check {
                if let Some(channel_pins) = all_pins.get(&channel_id) {
                    for pin in channel_pins {
                        if query
                            .message_id
                            .as_ref()
                            .map(|id| id == &pin.message_id)
                            .unwrap_or(true)
                        {
                            if Self::pin_matches_query(pin, &query) {
                                pins.push(pin.clone());
                            }
                        }
                    }
                }
            }
        }

        // Apply limit
        if let Some(limit) = query.limit {
            edits.truncate(limit);
            deletions.truncate(limit);
            pins.truncate(limit);
        }

        let total_count = edits.len() + deletions.len() + pins.len();

        HistoryQueryResult {
            edits,
            deletions,
            pins,
            total_count,
        }
    }

    /// Search in message content
    pub async fn search_content(&self, query: &str, channel_id: Option<&str>) -> Vec<SearchResult> {
        let mut results = Vec::new();
        let snapshots = self.snapshots.read().await;

        let channels_to_search: Vec<String> = channel_id
            .map(|id| vec![id.to_string()])
            .unwrap_or_else(|| snapshots.keys().cloned().collect());

        for ch_id in channels_to_search {
            if let Some(channel_snapshots) = snapshots.get(&ch_id) {
                for (msg_id, msg_snapshots) in channel_snapshots {
                    for snapshot in msg_snapshots {
                        if snapshot.content.contains(query) {
                            results.push(SearchResult {
                                message_id: msg_id.clone(),
                                channel_id: ch_id.clone(),
                                content: snapshot.content.clone(),
                                author_id: snapshot.author_id.clone(),
                                timestamp: snapshot.timestamp,
                                version: snapshot.version,
                            });
                        }
                    }
                }
            }
        }

        results
    }

    /// Export history for a channel
    pub async fn export_channel_history(&self, channel_id: &str) -> ChannelHistoryExport {
        let edits = self.get_channel_edits(channel_id).await;
        let deletions = self.get_channel_deletions(channel_id).await;
        let pins = self.get_channel_pins(channel_id).await;

        ChannelHistoryExport {
            channel_id: channel_id.to_string(),
            exported_at: Utc::now(),
            edits,
            deletions,
            pins,
        }
    }

    /// Get statistics for a channel
    pub async fn get_channel_stats(&self, channel_id: &str) -> MessageHistoryStats {
        let edits = self.get_channel_edits(channel_id).await.len();
        let deletions = self.get_channel_deletions(channel_id).await.len();
        let pins = self.get_channel_pins(channel_id).await.len();
        let current_pins = self.get_currently_pinned(channel_id).await.len();

        MessageHistoryStats {
            total_edits: edits,
            total_deletions: deletions,
            total_pins: pins,
            currently_pinned: current_pins,
        }
    }

    /// Clear history for a channel
    pub async fn clear_channel_history(&self, channel_id: &str) {
        self.edits.write().await.remove(channel_id);
        self.deletions.write().await.remove(channel_id);
        self.pins.write().await.remove(channel_id);
        self.snapshots.write().await.remove(channel_id);
    }

    /// Get next version number for a message
    async fn get_next_version(&self, message_id: &str, channel_id: &str) -> u32 {
        let edits = self.edits.read().await;
        edits
            .get(channel_id)
            .and_then(|m| m.get(message_id))
            .map(|v| v.len() as u32 + 1)
            .unwrap_or(1)
    }

    /// Store a message snapshot
    async fn store_snapshot(
        &self,
        message_id: &str,
        channel_id: &str,
        author_id: &str,
        content: &str,
        version: u32,
    ) {
        let snapshot = MessageSnapshot {
            snapshot_id: Uuid::new_v4().to_string(),
            message_id: message_id.to_string(),
            channel_id: channel_id.to_string(),
            content: content.to_string(),
            author_id: author_id.to_string(),
            timestamp: Utc::now(),
            version,
            metadata: HashMap::new(),
        };

        let mut snapshots = self.snapshots.write().await;
        let channel_snapshots = snapshots.entry(channel_id.to_string()).or_default();
        let message_snapshots = channel_snapshots.entry(message_id.to_string()).or_default();
        message_snapshots.push(snapshot);
    }

    /// Check if edit matches query criteria
    fn matches_query(edit: &MessageEditRecord, query: &HistoryQuery) -> bool {
        if let Some(author) = &query.author_id {
            if &edit.author_id != author {
                return false;
            }
        }

        if let Some(start) = query.start_time {
            if edit.edited_at < start {
                return false;
            }
        }

        if let Some(end) = query.end_time {
            if edit.edited_at > end {
                return false;
            }
        }

        true
    }

    /// Check if deletion matches query criteria
    fn deletion_matches_query(deletion: &MessageDeletionRecord, query: &HistoryQuery) -> bool {
        if let Some(start) = query.start_time {
            if deletion.deleted_at < start {
                return false;
            }
        }

        if let Some(end) = query.end_time {
            if deletion.deleted_at > end {
                return false;
            }
        }

        true
    }

    /// Check if pin matches query criteria
    fn pin_matches_query(pin: &MessagePinRecord, query: &HistoryQuery) -> bool {
        if let Some(start) = query.start_time {
            if pin.pinned_at < start {
                return false;
            }
        }

        if let Some(end) = query.end_time {
            if let Some(unpin_time) = pin.unpinned_at {
                if unpin_time > end {
                    return false;
                }
            }
        }

        true
    }

    /// Determine if an edit is major (significant content change)
    fn is_major_edit(old_content: &str, new_content: &str) -> bool {
        let old_len = old_content.len();
        let new_len = new_content.len();

        // Consider major if length changes by more than 50%
        if old_len == 0 {
            return new_len > 10;
        }

        let change_ratio = (new_len as f64 - old_len as f64).abs() / old_len as f64;
        change_ratio > 0.5
    }
}

impl Default for MessageHistoryStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Search result
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub message_id: String,
    pub channel_id: String,
    pub content: String,
    pub author_id: String,
    pub timestamp: DateTime<Utc>,
    pub version: u32,
}

/// Channel history export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelHistoryExport {
    pub channel_id: String,
    pub exported_at: DateTime<Utc>,
    pub edits: Vec<MessageEditRecord>,
    pub deletions: Vec<MessageDeletionRecord>,
    pub pins: Vec<MessagePinRecord>,
}

/// Message history statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageHistoryStats {
    pub total_edits: usize,
    pub total_deletions: usize,
    pub total_pins: usize,
    pub currently_pinned: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_record_and_retrieve_edit() {
        let store = MessageHistoryStore::new();
        let channel_id = "channel-1";
        let message_id = "msg-1";

        let record = store
            .record_edit(
                message_id,
                channel_id,
                "user-1",
                "Hello",
                "Hello World",
                Some("Added more content"),
            )
            .await;

        assert_eq!(record.version, 1);
        assert!(record.is_major_edit);

        let history = store.get_edit_history(channel_id, message_id).await;
        assert!(history.is_some());
        assert_eq!(history.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_pin_unpin_cycle() {
        let store = MessageHistoryStore::new();
        let channel_id = "channel-1";
        let message_id = "msg-1";

        // Pin
        let pin_record = store.record_pin(message_id, channel_id, "admin-1").await;
        assert!(pin_record.is_currently_pinned);

        let pinned = store.get_currently_pinned(channel_id).await;
        assert_eq!(pinned.len(), 1);

        // Unpin
        store.record_unpin(message_id, channel_id, "admin-1").await;

        let pinned = store.get_currently_pinned(channel_id).await;
        assert_eq!(pinned.len(), 0);

        // Check history
        let pin_history = store.get_channel_pins(channel_id).await;
        assert_eq!(pin_history.len(), 1);
        assert!(!pin_history[0].is_currently_pinned);
    }

    #[tokio::test]
    async fn test_query_history() {
        let store = MessageHistoryStore::new();

        // Add some records
        store
            .record_edit("msg-1", "channel-1", "user-1", "A", "B", None)
            .await;
        store
            .record_edit("msg-2", "channel-1", "user-2", "C", "D", None)
            .await;
        store.record_pin("msg-1", "channel-1", "admin-1").await;

        // Query all for channel
        let query = HistoryQuery {
            channel_id: Some("channel-1".to_string()),
            message_id: None,
            author_id: None,
            start_time: None,
            end_time: None,
            operation_type: Some(OperationType::All),
            limit: None,
        };

        let result = store.query(query).await;
        assert_eq!(result.total_count, 3); // 2 edits + 1 pin
    }

    #[tokio::test]
    async fn test_search_content() {
        let store = MessageHistoryStore::new();

        store
            .record_edit(
                "msg-1",
                "channel-1",
                "user-1",
                "Hello World",
                "Hello Universe",
                None,
            )
            .await;
        store
            .record_edit(
                "msg-2",
                "channel-1",
                "user-2",
                "Test message",
                "Updated test",
                None,
            )
            .await;

        let results = store.search_content("Universe", Some("channel-1")).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].message_id, "msg-1");
    }

    #[test]
    fn test_is_major_edit() {
        assert!(MessageHistoryStore::is_major_edit(
            "Hi",
            "Hello World this is a long message"
        ));
        assert!(!MessageHistoryStore::is_major_edit(
            "Hello World",
            "Hello World!"
        ));
        assert!(MessageHistoryStore::is_major_edit(
            "",
            "This is a new message with content"
        ));
    }
}
