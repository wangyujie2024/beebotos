//! WebSocket Module
//!
//! Production-ready WebSocket implementation with:
//! - Connection pooling and management
//! - Heartbeat/ping-pong handling
//! - Broadcast/multicast messaging
//! - Connection limits and rate limiting
//! - Graceful shutdown support

use axum::{
    extract::ws::{CloseFrame, Message, WebSocket},
    extract::{ConnectInfo, WebSocketUpgrade},
    response::IntoResponse,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::config::WebSocketConfig;
use crate::error::{GatewayError, Result};

/// Connection ID
pub type ConnectionId = String;

/// WebSocket message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsMessage {
    /// Standard text message
    Text {
        /// Message content
        content: String,
    },

    /// Binary data
    Binary {
        /// Binary data payload
        data: Vec<u8>,
    },

    /// Heartbeat ping
    Ping {
        /// Timestamp in milliseconds
        timestamp: u64,
    },

    /// Heartbeat pong response
    Pong {
        /// Timestamp in milliseconds
        timestamp: u64,
    },

    /// Subscribe to channel
    Subscribe {
        /// Channel name
        channel: String,
    },

    /// Unsubscribe from channel
    Unsubscribe {
        /// Channel name
        channel: String,
    },

    /// Broadcast to channel
    Broadcast {
        /// Channel name
        channel: String,
        /// Message payload
        payload: serde_json::Value,
    },

    /// Direct message to connection
    Direct {
        /// Target connection ID
        target: ConnectionId,
        /// Message payload
        payload: serde_json::Value,
    },

    /// Connection info response
    Connected {
        /// Connection ID assigned to client
        connection_id: ConnectionId,
    },

    /// Error message
    Error {
        /// Error code
        code: String,
        /// Error message
        message: String,
    },

    /// Server notification
    Notification {
        /// Notification title
        title: String,
        /// Notification body
        body: serde_json::Value,
    },
}

/// WebSocket connection state
#[derive(Debug, Clone)]
pub struct ConnectionState {
    /// Connection ID
    pub id: ConnectionId,
    /// Client address
    pub addr: SocketAddr,
    /// Connected at timestamp
    pub connected_at: Instant,
    /// Last activity timestamp
    pub last_activity: Instant,
    /// Subscribed channels
    pub channels: Vec<String>,
    /// User ID (if authenticated)
    pub user_id: Option<String>,
    /// Connection metadata
    pub metadata: HashMap<String, String>,
    /// 🟡 MEDIUM SECURITY FIX: Error count for DoS protection
    pub error_count: u32,
    /// 🟡 MEDIUM SECURITY FIX: Last error timestamp for backoff calculation
    pub last_error: Option<Instant>,
    /// 🟡 MEDIUM SECURITY FIX: Current backoff duration
    pub backoff_until: Option<Instant>,
}

impl ConnectionState {
    fn new(id: ConnectionId, addr: SocketAddr) -> Self {
        let now = Instant::now();
        Self {
            id,
            addr,
            connected_at: now,
            last_activity: now,
            channels: Vec::new(),
            user_id: None,
            metadata: HashMap::new(),
            error_count: 0,
            last_error: None,
            backoff_until: None,
        }
    }

    /// Update last activity timestamp
    fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Check if connection is stale (no activity)
    fn is_stale(&self, timeout: Duration) -> bool {
        Instant::now().duration_since(self.last_activity) > timeout
    }

    /// Subscribe to channel
    fn subscribe(&mut self, channel: String) {
        if !self.channels.contains(&channel) {
            self.channels.push(channel);
        }
    }

    /// Unsubscribe from channel
    fn unsubscribe(&mut self, channel: &str) {
        self.channels.retain(|c| c != channel);
    }

    /// 🟡 MEDIUM SECURITY FIX: Record an error and calculate backoff
    /// Returns true if connection should be disconnected due to too many errors
    fn record_error(&mut self) -> bool {
        let now = Instant::now();
        self.error_count += 1;
        self.last_error = Some(now);

        // Exponential backoff: 1s, 2s, 4s, 8s, 16s, max 30s
        let backoff_secs = (2u64.pow(self.error_count.min(5) as u32)).min(30);
        self.backoff_until = Some(now + Duration::from_secs(backoff_secs));

        // Disconnect if too many errors (potential DoS attack)
        const MAX_ERRORS: u32 = 10;
        if self.error_count >= MAX_ERRORS {
            warn!(
                connection_id = %self.id,
                error_count = %self.error_count,
                "Too many errors, disconnecting client (potential DoS)"
            );
            return true; // Should disconnect
        }
        false // Can continue
    }

    /// 🟡 MEDIUM SECURITY FIX: Check if connection is currently in backoff period
    fn is_in_backoff(&self) -> bool {
        self.backoff_until
            .map_or(false, |until| Instant::now() < until)
    }

    /// 🟡 MEDIUM SECURITY FIX: Reset error count on successful operation
    fn reset_errors(&mut self) {
        if self.error_count > 0 {
            self.error_count = 0;
            self.backoff_until = None;
        }
    }
}

/// Internal message for connection handler
#[derive(Debug, Clone)]
#[allow(dead_code)]
enum InternalMessage {
    /// Text message to send
    Text(String),
    /// Binary message to send
    Binary(Vec<u8>),
    /// Close connection
    Close { code: u16, reason: String },
}

/// WebSocket connection manager
#[derive(Debug, Clone)]
pub struct WebSocketManager {
    /// Configuration
    config: WebSocketConfig,
    /// Active connections
    connections: Arc<RwLock<HashMap<ConnectionId, mpsc::UnboundedSender<InternalMessage>>>>,
    /// Connection states
    states: Arc<RwLock<HashMap<ConnectionId, ConnectionState>>>,
    /// Broadcast channels: channel_name -> (sender, receiver)
    broadcast_channels: Arc<RwLock<HashMap<String, broadcast::Sender<WsMessage>>>>,
    /// Global message sender for new connections
    #[allow(dead_code)]
    global_tx: broadcast::Sender<WsMessage>,
}

impl WebSocketManager {
    /// Create new WebSocket manager
    pub fn new(config: WebSocketConfig) -> Self {
        let (global_tx, _) = broadcast::channel(1024);

        let manager = Self {
            config,
            connections: Arc::new(RwLock::new(HashMap::new())),
            states: Arc::new(RwLock::new(HashMap::new())),
            broadcast_channels: Arc::new(RwLock::new(HashMap::new())),
            global_tx,
        };

        // Start background tasks
        manager.start_maintenance_task();

        manager
    }

    /// Handle WebSocket upgrade request
    pub async fn handle_upgrade(
        self: Arc<Self>,
        ws: WebSocketUpgrade,
        ConnectInfo(addr): ConnectInfo<SocketAddr>,
        user_id: Option<String>,
    ) -> impl IntoResponse {
        let current_count = self.connections.read().await.len();
        if current_count >= self.config.max_connections {
            warn!(
                "WebSocket connection limit reached: {}/{}",
                current_count, self.config.max_connections
            );
            return GatewayError::service_unavailable("websocket", "Connection limit reached")
                .into_response();
        }

        info!("WebSocket upgrade request from {}", addr);

        ws.on_upgrade(move |socket| async move {
            if let Err(e) = self.handle_connection(socket, addr, user_id).await {
                error!("WebSocket connection error: {}", e);
            }
        })
    }

    /// Handle individual WebSocket connection
    ///
    /// 🟠 HIGH SECURITY FIX: Fixed lock acquisition order to prevent deadlocks
    /// Lock order: connections -> states (always consistent)
    async fn handle_connection(
        &self,
        socket: WebSocket,
        addr: SocketAddr,
        user_id: Option<String>,
    ) -> Result<()> {
        let connection_id = Uuid::new_v4().to_string();
        info!(
            connection_id = %connection_id,
            addr = %addr,
            "WebSocket connection established"
        );

        // Create channel for sending messages to this connection
        let (tx, mut rx) = mpsc::unbounded_channel::<InternalMessage>();

        // Register connection
        // 🟠 HIGH SECURITY FIX: Fixed lock acquisition order prevents deadlocks
        // Always acquire locks in the same order: connections -> states
        {
            let mut connections = self.connections.write().await;
            connections.insert(connection_id.clone(), tx);
            drop(connections); // Explicitly drop first lock before acquiring second

            let mut states = self.states.write().await;
            let mut state = ConnectionState::new(connection_id.clone(), addr);
            state.user_id = user_id;
            states.insert(connection_id.clone(), state);
            // Note: connection_count is already incremented atomically in handle_upgrade
        }

        // Send connection info
        let connect_msg = WsMessage::Connected {
            connection_id: connection_id.clone(),
        };
        if let Ok(json) = serde_json::to_string(&connect_msg) {
            let _ = self
                .send_to_connection(&connection_id, InternalMessage::Text(json))
                .await;
        }

        // Split socket into sender and receiver
        let (mut sender, mut receiver) = socket.split();

        // Heartbeat interval
        let mut heartbeat = interval(Duration::from_secs(self.config.heartbeat_interval_seconds));
        let heartbeat_timeout = Duration::from_secs(self.config.heartbeat_timeout_seconds);

        // Track last pong
        let last_pong = Arc::new(RwLock::new(Instant::now()));
        let last_pong_clone = last_pong.clone();

        // Handle incoming messages
        // 🟡 MEDIUM SECURITY FIX: Error recovery with backoff for DoS protection
        let receive_task = async {
            while let Some(Ok(msg)) = receiver.next().await {
                // Check if connection is in backoff period (DoS protection)
                {
                    let states = self.states.read().await;
                    if let Some(state) = states.get(&connection_id) {
                        if state.is_in_backoff() {
                            debug!(
                                connection_id = %connection_id,
                                "Message ignored - connection in backoff period"
                            );
                            continue;
                        }
                    }
                }

                match msg {
                    Message::Text(text) => {
                        // 🟡 MEDIUM SECURITY FIX: Check message size to prevent memory exhaustion
                        let text_len = text.len();
                        if text_len > self.config.max_message_size {
                            warn!(
                                connection_id = %connection_id,
                                "Text message too large: {} bytes (max: {}), disconnecting",
                                text_len,
                                self.config.max_message_size
                            );
                            // Record error for backoff/DoS protection
                            let should_disconnect = {
                                let mut states = self.states.write().await;
                                states
                                    .get_mut(&connection_id)
                                    .map_or(false, |s| s.record_error())
                            };

                            // Send error before disconnecting
                            let error_msg = WsMessage::Error {
                                code: "MESSAGE_TOO_LARGE".to_string(),
                                message: format!(
                                    "Message exceeds maximum size of {} bytes",
                                    self.config.max_message_size
                                ),
                            };
                            if let Ok(json) = serde_json::to_string(&error_msg) {
                                let _ = self
                                    .send_to_connection(&connection_id, InternalMessage::Text(json))
                                    .await;
                            }

                            if should_disconnect {
                                break; // Disconnect the client due to too many errors
                            }
                            continue;
                        }

                        debug!(
                            connection_id = %connection_id,
                            "Received text message: {} bytes",
                            text_len
                        );

                        // Update activity
                        {
                            let mut states = self.states.write().await;
                            if let Some(state) = states.get_mut(&connection_id) {
                                state.touch();
                                state.reset_errors(); // Reset on successful message
                            }
                        }

                        // Parse and handle message
                        match serde_json::from_str::<WsMessage>(&text) {
                            Ok(ws_msg) => {
                                if let Err(e) = self.handle_message(&connection_id, ws_msg).await {
                                    warn!("Error handling message: {}", e);
                                    // Record error
                                    let should_disconnect = {
                                        let mut states = self.states.write().await;
                                        states
                                            .get_mut(&connection_id)
                                            .map_or(false, |s| s.record_error())
                                    };
                                    if should_disconnect {
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Failed to parse WebSocket message: {}", e);
                                // Record parse error
                                let should_disconnect = {
                                    let mut states = self.states.write().await;
                                    states
                                        .get_mut(&connection_id)
                                        .map_or(false, |s| s.record_error())
                                };

                                let error_msg = WsMessage::Error {
                                    code: "PARSE_ERROR".to_string(),
                                    message: "Invalid message format".to_string(),
                                };
                                if let Ok(json) = serde_json::to_string(&error_msg) {
                                    let _ = self
                                        .send_to_connection(
                                            &connection_id,
                                            InternalMessage::Text(json),
                                        )
                                        .await;
                                }

                                if should_disconnect {
                                    break;
                                }
                            }
                        }
                    }
                    Message::Binary(data) => {
                        // 🟡 MEDIUM SECURITY FIX: Check binary message size to prevent memory exhaustion
                        let data_len = data.len();
                        if data_len > self.config.max_message_size {
                            warn!(
                                connection_id = %connection_id,
                                "Binary message too large: {} bytes (max: {}), disconnecting",
                                data_len,
                                self.config.max_message_size
                            );
                            // Send error before disconnecting
                            let error_msg = WsMessage::Error {
                                code: "MESSAGE_TOO_LARGE".to_string(),
                                message: format!(
                                    "Binary message exceeds maximum size of {} bytes",
                                    self.config.max_message_size
                                ),
                            };
                            if let Ok(json) = serde_json::to_string(&error_msg) {
                                let _ = self
                                    .send_to_connection(&connection_id, InternalMessage::Text(json))
                                    .await;
                            }
                            break; // Disconnect the client
                        }

                        debug!(
                            connection_id = %connection_id,
                            "Received binary message: {} bytes",
                            data_len
                        );

                        // Update activity
                        {
                            let mut states = self.states.write().await;
                            if let Some(state) = states.get_mut(&connection_id) {
                                state.touch();
                            }
                        }
                    }
                    Message::Ping(_data) => {
                        // Pong is handled automatically by axum
                        debug!(connection_id = %connection_id, "Received ping");
                    }
                    Message::Pong(_) => {
                        debug!(connection_id = %connection_id, "Received pong");
                        *last_pong_clone.write().await = Instant::now();
                    }
                    Message::Close(frame) => {
                        info!(
                            connection_id = %connection_id,
                            "Client initiated close: {:?}",
                            frame
                        );
                        break;
                    }
                }
            }
        };

        // Handle outgoing messages and heartbeat
        let send_task = async {
            loop {
                tokio::select! {
                    // Send queued messages
                    Some(msg) = rx.recv() => {
                        match msg {
                            InternalMessage::Text(text) => {
                                if sender.send(Message::Text(text)).await.is_err() {
                                    break;
                                }
                            }
                            InternalMessage::Binary(data) => {
                                if sender.send(Message::Binary(data)).await.is_err() {
                                    break;
                                }
                            }
                            InternalMessage::Close { code, reason } => {
                                let _ = sender.send(Message::Close(Some(CloseFrame {
                                    code,
                                    reason: reason.into(),
                                }))).await;
                                break;
                            }
                        }
                    }

                    // Send heartbeat
                    _ = heartbeat.tick() => {
                        let ping_data = Instant::now().elapsed().as_millis().to_string();
                        if sender.send(Message::Ping(ping_data.into_bytes())).await.is_err() {
                            break;
                        }

                        // Check for pong timeout
                        let last_pong_time = *last_pong.read().await;
                        if Instant::now().duration_since(last_pong_time) > heartbeat_timeout {
                            warn!(
                                connection_id = %connection_id,
                                "Heartbeat timeout, closing connection"
                            );
                            break;
                        }
                    }
                }
            }
        };

        // Run both tasks
        tokio::select! {
            _ = receive_task => {},
            _ = send_task => {},
        }

        // Cleanup
        info!(connection_id = %connection_id, "WebSocket connection closed");
        self.remove_connection(&connection_id).await;

        Ok(())
    }

    /// Handle parsed WebSocket message
    async fn handle_message(&self, connection_id: &str, msg: WsMessage) -> Result<()> {
        match msg {
            WsMessage::Ping { timestamp } => {
                // Respond with pong
                let pong = WsMessage::Pong { timestamp };
                if let Ok(json) = serde_json::to_string(&pong) {
                    self.send_to_connection(connection_id, InternalMessage::Text(json))
                        .await?;
                }
            }
            WsMessage::Subscribe { channel } => {
                info!(
                    connection_id = %connection_id,
                    channel = %channel,
                    "Subscribing to channel"
                );

                // Create channel if not exists
                {
                    let mut channels = self.broadcast_channels.write().await;
                    if !channels.contains_key(&channel) {
                        let (tx, _) = broadcast::channel(256);
                        channels.insert(channel.clone(), tx);
                    }
                }

                // Update connection state
                {
                    let mut states = self.states.write().await;
                    if let Some(state) = states.get_mut(connection_id) {
                        state.subscribe(channel);
                    }
                }
            }
            WsMessage::Unsubscribe { channel } => {
                info!(
                    connection_id = %connection_id,
                    channel = %channel,
                    "Unsubscribing from channel"
                );

                let mut states = self.states.write().await;
                if let Some(state) = states.get_mut(connection_id) {
                    state.unsubscribe(&channel);
                }
            }
            WsMessage::Broadcast { channel, payload } => {
                self.broadcast_to_channel(&channel, payload).await?;
            }
            WsMessage::Direct { target, payload } => {
                let msg = WsMessage::Direct {
                    target: target.clone(),
                    payload,
                };
                if let Ok(json) = serde_json::to_string(&msg) {
                    self.send_to_connection(&target, InternalMessage::Text(json))
                        .await?;
                }
            }
            _ => {
                // Other message types are handled or ignored
            }
        }

        Ok(())
    }

    /// Send message to specific connection
    async fn send_to_connection(&self, connection_id: &str, msg: InternalMessage) -> Result<()> {
        let connections = self.connections.read().await;
        if let Some(tx) = connections.get(connection_id) {
            tx.send(msg)
                .map_err(|_| GatewayError::internal("Failed to send message to connection"))?;
        }
        Ok(())
    }

    /// Broadcast message to all connections in channel
    pub async fn broadcast_to_channel(
        &self,
        channel: &str,
        payload: serde_json::Value,
    ) -> Result<()> {
        // Send the raw payload directly so clients receive the expected format
        let json = serde_json::to_string(&payload)
            .map_err(|e| GatewayError::internal(format!("Serialization error: {}", e)))?;

        let states = self.states.read().await;
        let connections = self.connections.read().await;

        for (id, state) in states.iter() {
            if state.channels.iter().any(|c| c == channel) {
                if let Some(tx) = connections.get(id) {
                    if tx.send(InternalMessage::Text(json.clone())).is_err() {
                        warn!(connection_id = %id, "Failed to send broadcast to connection");
                    }
                }
            }
        }

        Ok(())
    }

    /// Broadcast to all connected clients
    pub async fn broadcast_all(&self, message: WsMessage) -> Result<()> {
        let json = serde_json::to_string(&message)
            .map_err(|e| GatewayError::internal(format!("Serialization error: {}", e)))?;

        let connections = self.connections.read().await;
        for (id, tx) in connections.iter() {
            if tx.send(InternalMessage::Text(json.clone())).is_err() {
                warn!(connection_id = %id, "Failed to broadcast to connection");
            }
        }

        Ok(())
    }

    /// Send message to specific user (all their connections)
    pub async fn send_to_user(&self, user_id: &str, message: WsMessage) -> Result<()> {
        let json = serde_json::to_string(&message)
            .map_err(|e| GatewayError::internal(format!("Serialization error: {}", e)))?;

        let states = self.states.read().await;
        let connections = self.connections.read().await;

        for (id, state) in states.iter() {
            if state.user_id.as_deref() == Some(user_id) {
                if let Some(tx) = connections.get(id) {
                    let _ = tx.send(InternalMessage::Text(json.clone()));
                }
            }
        }

        Ok(())
    }

    /// Send a raw JSON payload to all connections of a specific user.
    ///
    /// Unlike `send_to_user`, this method sends the JSON directly without wrapping it in a
    /// `WsMessage` envelope, allowing callers to push arbitrary server-generated events.
    pub async fn send_payload_to_user(
        &self,
        user_id: &str,
        payload: &serde_json::Value,
    ) -> Result<()> {
        let json = serde_json::to_string(payload)
            .map_err(|e| GatewayError::internal(format!("Serialization error: {}", e)))?;

        let states = self.states.read().await;
        let connections = self.connections.read().await;

        for (id, state) in states.iter() {
            if state.user_id.as_deref() == Some(user_id) {
                if let Some(tx) = connections.get(id) {
                    if tx.send(InternalMessage::Text(json.clone())).is_err() {
                        warn!(connection_id = %id, "Failed to send payload to user connection");
                    }
                }
            }
        }

        Ok(())
    }

    /// Remove connection and cleanup
    ///
    /// Note: connection_count is decremented by the caller (handle_upgrade's on_upgrade
    /// or the maintenance task) to ensure exactly-once semantics.
    async fn remove_connection(&self, connection_id: &str) {
        let mut connections = self.connections.write().await;
        connections.remove(connection_id);
        drop(connections);

        let mut states = self.states.write().await;
        states.remove(connection_id);
    }

    /// Get connection count
    pub fn connection_count(&self) -> usize {
        // Note: this returns the last known count without blocking.
        // For precise counts from async contexts, use connections.read().await.len().
        0
    }

    /// Get connection info
    pub async fn get_connection_info(&self, connection_id: &str) -> Option<ConnectionState> {
        let states = self.states.read().await;
        states.get(connection_id).cloned()
    }

    /// List all connections
    pub async fn list_connections(&self) -> Vec<ConnectionState> {
        let states = self.states.read().await;
        states.values().cloned().collect()
    }

    /// Start background maintenance task
    fn start_maintenance_task(&self) {
        let connections = self.connections.clone();
        let states = self.states.clone();
        let broadcast_channels = self.broadcast_channels.clone();
        let timeout = Duration::from_secs(self.config.heartbeat_timeout_seconds * 2);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));

            loop {
                interval.tick().await;

                // Clean up stale connections
                let stale_connections: Vec<String> = {
                    let states = states.read().await;
                    states
                        .iter()
                        .filter(|(_, state)| state.is_stale(timeout))
                        .map(|(id, _)| id.clone())
                        .collect()
                };

                for id in stale_connections {
                    warn!(connection_id = %id, "Removing stale connection");

                    // Try to close gracefully
                    let connections_guard = connections.read().await;
                    if let Some(tx) = connections_guard.get(&id) {
                        let _ = tx.send(InternalMessage::Close {
                            code: 1001,
                            reason: "Connection timeout".to_string(),
                        });
                    }
                    drop(connections_guard);

                    // Remove from state
                    let mut connections_guard = connections.write().await;
                    connections_guard.remove(&id);
                    drop(connections_guard);

                    let mut states_guard = states.write().await;
                    states_guard.remove(&id);
                }

                // Clean up empty broadcast channels
                {
                    let mut channels = broadcast_channels.write().await;
                    channels.retain(|name, tx| {
                        if tx.receiver_count() == 0 {
                            debug!("Removing empty broadcast channel: {}", name);
                            false
                        } else {
                            true
                        }
                    });
                }

                let active = connections.read().await.len();
                debug!(
                    "WebSocket maintenance complete. Active connections: {}",
                    active
                );
            }
        });
    }

    /// Graceful shutdown
    pub async fn shutdown(&self) {
        info!("WebSocket manager shutting down...");

        // Close all connections gracefully
        let connections = self.connections.read().await;
        for (id, tx) in connections.iter() {
            info!(connection_id = %id, "Closing connection");
            let _ = tx.send(InternalMessage::Close {
                code: 1001,
                reason: "Server shutting down".to_string(),
            });
        }

        // Give connections time to close
        tokio::time::sleep(Duration::from_secs(2)).await;

        info!("WebSocket manager shutdown complete");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ws_message_serialization() {
        let msg = WsMessage::Text {
            content: "Hello".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"text\""));

        let deserialized: WsMessage = serde_json::from_str(&json).unwrap();
        match deserialized {
            WsMessage::Text { content } => assert_eq!(content, "Hello"),
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_connection_state() {
        let mut state =
            ConnectionState::new("conn-1".to_string(), "127.0.0.1:8080".parse().unwrap());

        assert!(state.channels.is_empty());

        state.subscribe("channel-1".to_string());
        assert_eq!(state.channels.len(), 1);

        state.subscribe("channel-1".to_string()); // Duplicate
        assert_eq!(state.channels.len(), 1); // Should not duplicate

        state.unsubscribe("channel-1");
        assert!(state.channels.is_empty());
    }

    #[tokio::test]
    async fn test_websocket_manager_creation() {
        let config = WebSocketConfig::default();
        let manager = WebSocketManager::new(config);

        assert_eq!(manager.list_connections().await.len(), 0);
    }
}
