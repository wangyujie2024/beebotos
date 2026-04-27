//! Channel Extension Traits
//!
//! Provides extended functionality for channels:
//! - Message pinning/unpinning
//! - Message editing with history tracking
//! - Advanced channel management

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::Channel;
use crate::error::Result;

/// Pin a message to the channel
#[async_trait]
pub trait PinnableChannel: Channel {
    /// Pin a message to the channel
    ///
    /// # Arguments
    /// * `channel_id` - The channel ID
    /// * `message_id` - The message ID to pin
    ///
    /// # Returns
    /// Result indicating success or failure
    async fn pin_message(&self, channel_id: &str, message_id: &str) -> Result<()>;

    /// Unpin a message from the channel
    ///
    /// # Arguments
    /// * `channel_id` - The channel ID
    /// * `message_id` - The message ID to unpin
    ///
    /// # Returns
    /// Result indicating success or failure
    async fn unpin_message(&self, channel_id: &str, message_id: &str) -> Result<()>;

    /// Get all pinned messages in a channel
    ///
    /// # Arguments
    /// * `channel_id` - The channel ID
    ///
    /// # Returns
    /// List of pinned messages
    async fn get_pinned_messages(&self, channel_id: &str) -> Result<Vec<PinnedMessage>>;

    /// Clear all pinned messages in a channel
    ///
    /// # Arguments
    /// * `channel_id` - The channel ID
    ///
    /// # Returns
    /// Number of messages unpinned
    async fn clear_pinned_messages(&self, channel_id: &str) -> Result<u32> {
        let pinned = self.get_pinned_messages(channel_id).await?;
        let count = pinned.len() as u32;

        for msg in pinned {
            self.unpin_message(channel_id, &msg.message_id).await?;
        }

        Ok(count)
    }
}

/// Pinned message information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedMessage {
    pub message_id: String,
    pub channel_id: String,
    pub content: String,
    pub author_id: String,
    pub pinned_at: DateTime<Utc>,
    pub pinned_by: String,
}

/// Message editing capability
#[async_trait]
pub trait EditableChannel: Channel {
    /// Edit a message
    ///
    /// # Arguments
    /// * `channel_id` - The channel ID
    /// * `message_id` - The message ID to edit
    /// * `new_content` - The new message content
    ///
    /// # Returns
    /// Result indicating success or failure
    async fn edit_message(
        &self,
        channel_id: &str,
        message_id: &str,
        new_content: &str,
    ) -> Result<()>;

    /// Delete a message
    ///
    /// # Arguments
    /// * `channel_id` - The channel ID
    /// * `message_id` - The message ID to delete
    ///
    /// # Returns
    /// Result indicating success or failure
    async fn delete_message(&self, channel_id: &str, message_id: &str) -> Result<()>;

    /// Get edit history for a message
    ///
    /// # Arguments
    /// * `channel_id` - The channel ID
    /// * `message_id` - The message ID
    ///
    /// # Returns
    /// List of edit history entries
    async fn get_message_edit_history(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> Result<Vec<MessageEditHistory>>;

    /// Bulk delete messages
    ///
    /// # Arguments
    /// * `channel_id` - The channel ID
    /// * `message_ids` - List of message IDs to delete
    ///
    /// # Returns
    /// Number of messages deleted
    async fn bulk_delete_messages(&self, channel_id: &str, message_ids: &[String]) -> Result<u32> {
        let mut count = 0;
        for msg_id in message_ids {
            if self.delete_message(channel_id, msg_id).await.is_ok() {
                count += 1;
            }
        }
        Ok(count)
    }
}

/// Message edit history entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEditHistory {
    pub edit_id: String,
    pub message_id: String,
    pub channel_id: String,
    pub previous_content: String,
    pub new_content: String,
    pub edited_by: String,
    pub edited_at: DateTime<Utc>,
    pub edit_reason: Option<String>,
    pub version: u32,
}

/// Message history tracker - tracks all message operations
#[derive(Debug, Clone)]
pub struct MessageHistoryTracker {
    /// Storage for edit history (channel_id -> message_id -> history)
    edit_history: HashMap<String, HashMap<String, Vec<MessageEditHistory>>>,
    /// Storage for pinned messages (channel_id -> messages)
    pinned_messages: HashMap<String, Vec<PinnedMessage>>,
    /// Storage for deleted messages (channel_id -> messages)
    deleted_messages: HashMap<String, Vec<DeletedMessageInfo>>,
}

/// Deleted message information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletedMessageInfo {
    pub message_id: String,
    pub channel_id: String,
    pub content: String,
    pub author_id: String,
    pub deleted_at: DateTime<Utc>,
    pub deleted_by: String,
}

impl MessageHistoryTracker {
    /// Create a new message history tracker
    pub fn new() -> Self {
        Self {
            edit_history: HashMap::new(),
            pinned_messages: HashMap::new(),
            deleted_messages: HashMap::new(),
        }
    }

    /// Record a message edit
    pub fn record_edit(&mut self, edit: MessageEditHistory) {
        let channel_history = self
            .edit_history
            .entry(edit.channel_id.clone())
            .or_default();

        let message_history = channel_history.entry(edit.message_id.clone()).or_default();

        message_history.push(edit);
    }

    /// Get edit history for a message
    pub fn get_edit_history(
        &self,
        channel_id: &str,
        message_id: &str,
    ) -> Option<&Vec<MessageEditHistory>> {
        self.edit_history.get(channel_id)?.get(message_id)
    }

    /// Record a pinned message
    pub fn record_pin(&mut self, message: PinnedMessage) {
        let channel_pins = self
            .pinned_messages
            .entry(message.channel_id.clone())
            .or_default();

        // Remove if already exists (re-pinning)
        channel_pins.retain(|m| m.message_id != message.message_id);
        channel_pins.push(message);
    }

    /// Record an unpinned message
    pub fn record_unpin(&mut self, channel_id: &str, message_id: &str) {
        if let Some(channel_pins) = self.pinned_messages.get_mut(channel_id) {
            channel_pins.retain(|m| m.message_id != message_id);
        }
    }

    /// Get pinned messages for a channel
    pub fn get_pinned_messages(&self, channel_id: &str) -> Option<&Vec<PinnedMessage>> {
        self.pinned_messages.get(channel_id)
    }

    /// Record a deleted message
    pub fn record_deletion(&mut self, info: DeletedMessageInfo) {
        let channel_deletions = self
            .deleted_messages
            .entry(info.channel_id.clone())
            .or_default();

        channel_deletions.push(info);
    }

    /// Get deleted messages for a channel
    pub fn get_deleted_messages(&self, channel_id: &str) -> Option<&Vec<DeletedMessageInfo>> {
        self.deleted_messages.get(channel_id)
    }

    /// Search in message history
    pub fn search_history(&self, channel_id: &str, query: &str) -> Vec<&MessageEditHistory> {
        let mut results = Vec::new();

        if let Some(channel_history) = self.edit_history.get(channel_id) {
            for history in channel_history.values() {
                for edit in history {
                    if edit.previous_content.contains(query) || edit.new_content.contains(query) {
                        results.push(edit);
                    }
                }
            }
        }

        results
    }

    /// Get statistics for a channel
    pub fn get_channel_stats(&self, channel_id: &str) -> ChannelHistoryStats {
        let edit_count = self
            .edit_history
            .get(channel_id)
            .map(|h| h.values().map(|v| v.len()).sum())
            .unwrap_or(0);

        let pinned_count = self
            .pinned_messages
            .get(channel_id)
            .map(|v| v.len())
            .unwrap_or(0);

        let deleted_count = self
            .deleted_messages
            .get(channel_id)
            .map(|v| v.len())
            .unwrap_or(0);

        ChannelHistoryStats {
            total_edits: edit_count,
            total_pinned: pinned_count as u32,
            total_deleted: deleted_count as u32,
        }
    }

    /// Clear all history for a channel
    pub fn clear_channel_history(&mut self, channel_id: &str) {
        self.edit_history.remove(channel_id);
        self.pinned_messages.remove(channel_id);
        self.deleted_messages.remove(channel_id);
    }

    /// Export history to JSON
    pub fn export_history(&self, channel_id: &str) -> Option<serde_json::Value> {
        let edit_history: Vec<&MessageEditHistory> = self
            .edit_history
            .get(channel_id)?
            .values()
            .flat_map(|v| v.iter())
            .collect();

        let pinned = self
            .pinned_messages
            .get(channel_id)
            .cloned()
            .unwrap_or_default();
        let deleted = self
            .deleted_messages
            .get(channel_id)
            .cloned()
            .unwrap_or_default();

        serde_json::json!({
            "channel_id": channel_id,
            "edit_history": edit_history,
            "pinned_messages": pinned,
            "deleted_messages": deleted,
        })
        .into()
    }
}

impl Default for MessageHistoryTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Channel history statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelHistoryStats {
    pub total_edits: usize,
    pub total_pinned: u32,
    pub total_deleted: u32,
}

/// Extended channel with moderation capabilities
#[async_trait]
pub trait ModeratedChannel: Channel {
    /// Kick a user from the channel
    async fn kick_user(&self, channel_id: &str, user_id: &str, reason: Option<&str>) -> Result<()>;

    /// Ban a user from the channel
    async fn ban_user(&self, channel_id: &str, user_id: &str, reason: Option<&str>) -> Result<()>;

    /// Unban a user from the channel
    async fn unban_user(&self, channel_id: &str, user_id: &str) -> Result<()>;

    /// Timeout a user (mute temporarily)
    async fn timeout_user(&self, channel_id: &str, user_id: &str, duration_secs: u64)
        -> Result<()>;

    /// Remove timeout from a user
    async fn remove_timeout(&self, channel_id: &str, user_id: &str) -> Result<()>;

    /// Get banned users list
    async fn get_banned_users(&self, channel_id: &str) -> Result<Vec<BannedUserInfo>>;
}

/// Banned user information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BannedUserInfo {
    pub user_id: String,
    pub username: String,
    pub banned_at: DateTime<Utc>,
    pub banned_by: String,
    pub reason: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Channel capability flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelCapability {
    PinMessages,
    EditMessages,
    DeleteMessages,
    BulkDeleteMessages,
    KickUsers,
    BanUsers,
    TimeoutUsers,
    CreateWebhooks,
    ManageRoles,
    ManageChannels,
}

/// Capability check trait
pub trait ChannelCapabilities {
    /// Check if channel supports a specific capability
    fn supports_capability(&self, capability: ChannelCapability) -> bool;

    /// Get all supported capabilities
    fn supported_capabilities(&self) -> Vec<ChannelCapability>;
}

/// Helper macro to implement PinnableChannel for platforms that support it
#[macro_export]
macro_rules! impl_pinnable_channel {
    ($type:ty, $pin_fn:expr, $unpin_fn:expr, $get_pinned_fn:expr) => {
        #[async_trait::async_trait]
        impl PinnableChannel for $type {
            async fn pin_message(&self, channel_id: &str, message_id: &str) -> Result<()> {
                $pin_fn(self, channel_id, message_id).await
            }

            async fn unpin_message(&self, channel_id: &str, message_id: &str) -> Result<()> {
                $unpin_fn(self, channel_id, message_id).await
            }

            async fn get_pinned_messages(&self, channel_id: &str) -> Result<Vec<PinnedMessage>> {
                $get_pinned_fn(self, channel_id).await
            }
        }
    };
}

/// Helper macro to implement EditableChannel for platforms that support it
#[macro_export]
macro_rules! impl_editable_channel {
    ($type:ty, $edit_fn:expr, $delete_fn:expr, $history_fn:expr) => {
        #[async_trait::async_trait]
        impl EditableChannel for $type {
            async fn edit_message(
                &self,
                channel_id: &str,
                message_id: &str,
                new_content: &str,
            ) -> Result<()> {
                $edit_fn(self, channel_id, message_id, new_content).await
            }

            async fn delete_message(&self, channel_id: &str, message_id: &str) -> Result<()> {
                $delete_fn(self, channel_id, message_id).await
            }

            async fn get_message_edit_history(
                &self,
                channel_id: &str,
                message_id: &str,
            ) -> Result<Vec<MessageEditHistory>> {
                $history_fn(self, channel_id, message_id).await
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_history_tracker() {
        let mut tracker = MessageHistoryTracker::new();
        let channel_id = "channel-1";
        let message_id = "msg-1";

        // Record an edit
        let edit = MessageEditHistory {
            edit_id: "edit-1".to_string(),
            message_id: message_id.to_string(),
            channel_id: channel_id.to_string(),
            previous_content: "Hello".to_string(),
            new_content: "Hello World".to_string(),
            edited_by: "user-1".to_string(),
            edited_at: Utc::now(),
            edit_reason: Some("Fixed typo".to_string()),
            version: 1,
        };

        tracker.record_edit(edit);

        // Retrieve history
        let history = tracker.get_edit_history(channel_id, message_id);
        assert!(history.is_some());
        assert_eq!(history.unwrap().len(), 1);

        // Record a pinned message
        let pinned = PinnedMessage {
            message_id: message_id.to_string(),
            channel_id: channel_id.to_string(),
            content: "Important message".to_string(),
            author_id: "user-1".to_string(),
            pinned_at: Utc::now(),
            pinned_by: "admin-1".to_string(),
        };

        tracker.record_pin(pinned);

        let pins = tracker.get_pinned_messages(channel_id);
        assert!(pins.is_some());
        assert_eq!(pins.unwrap().len(), 1);

        // Get stats
        let stats = tracker.get_channel_stats(channel_id);
        assert_eq!(stats.total_edits, 1);
        assert_eq!(stats.total_pinned, 1);
        assert_eq!(stats.total_deleted, 0);

        // Search history
        let results = tracker.search_history(channel_id, "World");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_channel_history_stats_serialization() {
        let stats = ChannelHistoryStats {
            total_edits: 10,
            total_pinned: 5,
            total_deleted: 2,
        };

        let json = serde_json::to_string(&stats).unwrap();
        let deserialized: ChannelHistoryStats = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.total_edits, 10);
        assert_eq!(deserialized.total_pinned, 5);
        assert_eq!(deserialized.total_deleted, 2);
    }
}
