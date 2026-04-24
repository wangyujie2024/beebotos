//! iMessage Channel Implementation
//!
//! Unified Channel trait implementation for Apple iMessage.
//! Uses macOS private frameworks via AppleScript or direct SQLite access.
//! **Note: Only works on macOS with iMessage enabled.**

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};

/// Expand tilde to home directory
fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            let home_str = home.to_string_lossy();
            return format!("{}{}", home_str, &path[1..]);
        }
    }
    path.to_string()
}

use super::r#trait::{BaseChannelConfig, ConnectionMode, ContentType};
use super::{Channel, ChannelConfig, ChannelEvent, ChannelInfo, MemberInfo};
use crate::communication::{Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

/// iMessage database path
const IMESSAGE_DB_PATH: &str = "~/Library/Messages/chat.db";

/// iMessage Channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IMessageChannelConfig {
    /// Polling interval in seconds (default: 5)
    #[serde(default = "default_polling_interval")]
    pub polling_interval_secs: u64,
    /// iMessage database path
    #[serde(default = "default_db_path")]
    pub db_path: String,
    /// Apple ID to use (optional, defaults to system default)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apple_id: Option<String>,
    #[serde(flatten)]
    pub base: BaseChannelConfig,
}

fn default_polling_interval() -> u64 {
    5
}

fn default_db_path() -> String {
    IMESSAGE_DB_PATH.to_string()
}

impl Default for IMessageChannelConfig {
    fn default() -> Self {
        let mut base = BaseChannelConfig::default();
        // iMessage defaults to Polling mode
        base.connection_mode = ConnectionMode::Polling;

        Self {
            polling_interval_secs: 5,
            db_path: default_db_path(),
            apple_id: None,
            base,
        }
    }
}

impl ChannelConfig for IMessageChannelConfig {
    fn from_env() -> Option<Self>
    where
        Self: Sized,
    {
        // iMessage doesn't require credentials, but we check for macOS
        #[cfg(not(target_os = "macos"))]
        {
            warn!("iMessage channel is only available on macOS");
            return None;
        }

        #[cfg(target_os = "macos")]
        {
            let mut base = BaseChannelConfig::from_env("IMESSAGE")?;
            // iMessage uses Webhook for AppleScript mode
            if let Ok(mode) = std::env::var("IMESSAGE_CONNECTION_MODE") {
                if mode == "applescript" {
                    base.connection_mode = ConnectionMode::Webhook;
                } else {
                    base.connection_mode = ConnectionMode::Polling;
                }
            } else {
                base.connection_mode = ConnectionMode::Polling;
            }

            let polling_interval_secs = std::env::var("IMESSAGE_POLLING_INTERVAL")
                .map(|v| v.parse().unwrap_or(5))
                .unwrap_or(5);

            let db_path = std::env::var("IMESSAGE_DB_PATH").unwrap_or_else(|_| default_db_path());

            let apple_id = std::env::var("IMESSAGE_APPLE_ID").ok();

            Some(Self {
                polling_interval_secs,
                db_path,
                apple_id,
                base,
            })
        }
    }

    fn is_valid(&self) -> bool {
        // iMessage is valid if we're on macOS
        cfg!(target_os = "macos")
    }

    fn allowlist(&self) -> Vec<String> {
        vec![]
    }

    fn connection_mode(&self) -> ConnectionMode {
        self.base.connection_mode
    }

    fn auto_reconnect(&self) -> bool {
        self.base.auto_reconnect
    }

    fn max_reconnect_attempts(&self) -> u32 {
        self.base.max_reconnect_attempts
    }
}

/// iMessage from database
#[derive(Debug, Clone)]
pub struct IMessageRow {
    pub row_id: i64,
    pub text: Option<String>,
    pub handle_id: Option<i64>,
    pub service: Option<String>,
    pub date: i64,
    pub is_from_me: bool,
    pub cache_roomnames: Option<String>,
}

/// iMessage Channel implementation
pub struct IMessageChannel {
    config: IMessageChannelConfig,
    connected: Arc<RwLock<bool>>,
    last_row_id: Arc<RwLock<i64>>,
    listener_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl IMessageChannel {
    /// Create a new iMessage channel
    pub fn new(config: IMessageChannelConfig) -> Self {
        Self {
            config,
            connected: Arc::new(RwLock::new(false)),
            last_row_id: Arc::new(RwLock::new(0)),
            listener_handle: Arc::new(RwLock::new(None)),
        }
    }

    /// Create from environment variables
    #[cfg(target_os = "macos")]
    pub fn from_env() -> Result<Self> {
        let config = IMessageChannelConfig::from_env()
            .ok_or_else(|| AgentError::configuration("iMessage not available or not configured"))?;
        Ok(Self::new(config))
    }

    /// Create from environment variables (non-macOS)
    #[cfg(not(target_os = "macos"))]
    pub fn from_env() -> Result<Self> {
        Err(AgentError::platform("iMessage is only available on macOS").into())
    }

    /// Send message via AppleScript
    pub async fn send_message_applescript(&self, recipient: &str, text: &str) -> Result<()> {
        // Escape quotes in text
        let escaped_text = text.replace('"', "\\\"");
        let escaped_recipient = recipient.replace('"', "\\\"");

        let script = format!(
            r#"tell application "Messages"
                set targetService to 1st service whose service type = iMessage
                set targetBuddy to buddy "{}" of targetService
                send "{}" to targetBuddy
            end tell"#,
            escaped_recipient, escaped_text
        );

        let output = Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to execute AppleScript: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AgentError::platform(format!("AppleScript error: {}", stderr)).into());
        }

        Ok(())
    }

    /// Query new messages from database
    async fn query_new_messages(&self) -> Result<Vec<IMessageRow>> {
        let db_path = expand_tilde(&self.config.db_path);

        // Use sqlite3 command line tool to query
        let output = Command::new("sqlite3")
            .arg(&db_path)
            .arg(format!(
                "SELECT rowid, text, handle_id, service, date, is_from_me, cache_roomnames 
                 FROM message 
                 WHERE rowid > {} 
                 ORDER BY rowid ASC 
                 LIMIT 100;",
                *self.last_row_id.read().await
            ))
            .output()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to query database: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AgentError::platform(format!("Database query error: {}", stderr)).into());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut messages = Vec::new();

        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 7 {
                if let Ok(row_id) = parts[0].parse::<i64>() {
                    messages.push(IMessageRow {
                        row_id,
                        text: if parts[1].is_empty() {
                            None
                        } else {
                            Some(parts[1].to_string())
                        },
                        handle_id: parts[2].parse().ok(),
                        service: if parts[3].is_empty() {
                            None
                        } else {
                            Some(parts[3].to_string())
                        },
                        date: parts[4].parse().unwrap_or(0),
                        is_from_me: parts[5] == "1",
                        cache_roomnames: if parts[6].is_empty() {
                            None
                        } else {
                            Some(parts[6].to_string())
                        },
                    });
                }
            }
        }

        Ok(messages)
    }

    /// Get handle info from database
    async fn get_handle_info(&self, handle_id: i64) -> Result<Option<String>> {
        let db_path = expand_tilde(&self.config.db_path);

        let output = Command::new("sqlite3")
            .arg(&db_path)
            .arg(format!(
                "SELECT id FROM handle WHERE rowid = {};",
                handle_id
            ))
            .output()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to query handle: {}", e)))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let id = stdout.trim();
            if !id.is_empty() {
                return Ok(Some(id.to_string()));
            }
        }

        Ok(None)
    }

    /// Convert iMessage row to internal Message
    async fn convert_message(&self, row: &IMessageRow) -> Option<Message> {
        let content = row.text.clone().unwrap_or_default();

        // Get sender info
        let sender = if row.is_from_me {
            "me".to_string()
        } else if let Some(handle_id) = row.handle_id {
            self.get_handle_info(handle_id)
                .await
                .unwrap_or_default()
                .unwrap_or_else(|| "unknown".to_string())
        } else {
            "unknown".to_string()
        };

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("row_id".to_string(), row.row_id.to_string());
        metadata.insert("sender".to_string(), sender.clone());
        metadata.insert(
            "service".to_string(),
            row.service.clone().unwrap_or_default(),
        );
        metadata.insert("is_from_me".to_string(), row.is_from_me.to_string());

        if let Some(ref room) = row.cache_roomnames {
            metadata.insert("group".to_string(), room.clone());
        }

        // iMessage date is in nanoseconds since 2001-01-01
        let apple_epoch = chrono::DateTime::parse_from_rfc3339("2001-01-01T00:00:00Z")
            .ok()?
            .timestamp();
        let timestamp = chrono::DateTime::from_timestamp(
            apple_epoch + (row.date / 1_000_000_000),
            (row.date % 1_000_000_000) as u32,
        )?;

        Some(Message {
            id: uuid::Uuid::new_v4(),
            thread_id: uuid::Uuid::new_v4(),
            platform: PlatformType::IMessage,
            message_type: MessageType::Text,
            content,
            metadata,
            timestamp,
        })
    }

    /// Run polling listener
    async fn run_polling_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        let mut interval = interval(Duration::from_secs(self.config.polling_interval_secs));

        loop {
            interval.tick().await;

            match self.query_new_messages().await {
                Ok(messages) => {
                    for row in messages {
                        // Update last row ID
                        if row.row_id > *self.last_row_id.read().await {
                            *self.last_row_id.write().await = row.row_id;
                        }

                        // Skip messages from self
                        if row.is_from_me {
                            continue;
                        }

                        if let Some(message) = self.convert_message(&row).await {
                            let sender =
                                message.metadata.get("sender").cloned().unwrap_or_default();

                            let event = ChannelEvent::MessageReceived {
                                platform: PlatformType::IMessage,
                                channel_id: sender,
                                message,
                            };

                            if let Err(e) = event_bus.send(event).await {
                                error!("Failed to send event: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Polling error: {}", e);
                    if !self.config.base.auto_reconnect {
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Run AppleScript listener (not really a listener, just for compatibility)
    async fn run_applescript_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        info!("iMessage AppleScript mode - sending only, no real-time receiving");
        // AppleScript doesn't support receiving, we fall back to polling
        self.run_polling_listener(event_bus).await
    }
}

#[async_trait]
impl Channel for IMessageChannel {
    fn name(&self) -> &str {
        "imessage"
    }

    fn platform(&self) -> PlatformType {
        PlatformType::IMessage
    }

    fn is_connected(&self) -> bool {
        if let Ok(connected) = self.connected.try_read() {
            *connected
        } else {
            false
        }
    }

    async fn connect(&mut self) -> Result<()> {
        #[cfg(not(target_os = "macos"))]
        {
            return Err(AgentError::platform("iMessage is only available on macOS").into());
        }

        #[cfg(target_os = "macos")]
        {
            // Verify database access
            let db_path = expand_tilde(&self.config.db_path);
            if !std::path::Path::new(db_path.as_ref()).exists() {
                return Err(AgentError::platform(format!(
                    "iMessage database not found at {}",
                    db_path
                ))
                .into());
            }

            *self.connected.write().await = true;
            info!("iMessage channel connected");
            Ok(())
        }
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.stop_listener().await?;
        *self.connected.write().await = false;
        info!("Disconnected from iMessage");
        Ok(())
    }

    async fn send(&self, _channel_id: &str, _message: &Message) -> Result<()> {
        #[cfg(not(target_os = "macos"))]
        {
            return Err(AgentError::platform("iMessage is only available on macOS").into());
        }

        #[cfg(target_os = "macos")]
        {
            // For iMessage, channel_id is the phone number or email
            match message.message_type {
                MessageType::Text => {
                    self.send_message_applescript(channel_id, &message.content)
                        .await?;
                }
                _ => {
                    self.send_message_applescript(channel_id, &message.content)
                        .await?;
                }
            }
            Ok(())
        }
    }

    async fn start_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        self.stop_listener().await?;

        match self.config.base.connection_mode {
            ConnectionMode::Polling => {
                let channel = self.clone();
                let handle = tokio::spawn(async move {
                    if let Err(e) = channel.run_polling_listener(event_bus).await {
                        error!("Polling listener error: {}", e);
                    }
                });
                *self.listener_handle.write().await = Some(handle);
            }
            ConnectionMode::Webhook => {
                // AppleScript mode
                let channel = self.clone();
                let handle = tokio::spawn(async move {
                    if let Err(e) = channel.run_applescript_listener(event_bus).await {
                        error!("AppleScript listener error: {}", e);
                    }
                });
                *self.listener_handle.write().await = Some(handle);
            }
            _ => {
                return Err(AgentError::platform(
                    "iMessage does not support WebSocket mode",
                ));
            }
        }

        Ok(())
    }

    async fn stop_listener(&self) -> Result<()> {
        if let Some(handle) = self.listener_handle.write().await.take() {
            handle.abort();
        }
        Ok(())
    }

    fn supported_content_types(&self) -> Vec<ContentType> {
        vec![ContentType::Text]
    }

    async fn list_channels(&self) -> Result<Vec<ChannelInfo>> {
        // iMessage conversations can be queried from database
        Ok(vec![])
    }

    async fn list_members(&self, _channel_id: &str) -> Result<Vec<MemberInfo>> {
        // iMessage group members can be queried
        Ok(vec![])
    }

    fn connection_mode(&self) -> ConnectionMode {
        self.config.base.connection_mode
    }
}

impl Clone for IMessageChannel {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            connected: self.connected.clone(),
            last_row_id: self.last_row_id.clone(),
            listener_handle: Arc::new(RwLock::new(None)),
        }
    }
}
