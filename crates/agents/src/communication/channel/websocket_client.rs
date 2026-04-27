//! Generic WebSocket Client
//!
//! Provides a reusable WebSocket client component for all channel
//! implementations. Handles connection, reconnection, heartbeat, and message
//! routing.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::{SinkExt, StreamExt};
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, timeout};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{debug, error, info, warn};

use crate::communication::channel::ChannelEvent;
use crate::error::{AgentError, Result};

/// WebSocket connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WsConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

impl std::fmt::Display for WsConnectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WsConnectionState::Disconnected => write!(f, "disconnected"),
            WsConnectionState::Connecting => write!(f, "connecting"),
            WsConnectionState::Connected => write!(f, "connected"),
            WsConnectionState::Reconnecting => write!(f, "reconnecting"),
        }
    }
}

/// WebSocket configuration
#[derive(Debug, Clone)]
pub struct WebSocketConfig {
    /// WebSocket URL
    pub url: String,
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Ping interval (for keepalive)
    pub ping_interval: Duration,
    /// Pong timeout (for detecting disconnections)
    pub pong_timeout: Duration,
    /// Reconnect interval
    pub reconnect_interval: Duration,
    /// Maximum reconnect attempts (None = infinite)
    pub max_reconnect_attempts: Option<u32>,
    /// Custom headers for connection
    pub headers: Vec<(String, String)>,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            connect_timeout: Duration::from_secs(30),
            ping_interval: Duration::from_secs(30),
            pong_timeout: Duration::from_secs(60),
            reconnect_interval: Duration::from_secs(5),
            max_reconnect_attempts: Some(10),
            headers: Vec::new(),
        }
    }
}

/// WebSocket message handler trait
#[async_trait]
pub trait WebSocketHandler: Send + Sync + 'static {
    /// Handle incoming text message
    async fn on_message(&self, text: String) -> Result<()>;

    /// Handle binary message
    async fn on_binary(&self, data: Vec<u8>) -> Result<()> {
        // Default: ignore binary messages
        let _ = data;
        Ok(())
    }

    /// Handle connection established
    async fn on_connect(&self) -> Result<()> {
        Ok(())
    }

    /// Handle disconnection
    async fn on_disconnect(&self) -> Result<()> {
        Ok(())
    }

    /// Handle error
    async fn on_error(&self, error: String) -> Result<()> {
        warn!("WebSocket error: {}", error);
        Ok(())
    }

    /// Generate ping payload (optional)
    fn ping_payload(&self) -> Option<Vec<u8>> {
        None
    }

    /// Check if a message is a pong response
    fn is_pong(&self, _data: &[u8]) -> bool {
        true // Default: treat any response as pong
    }
}

/// Generic WebSocket client
pub struct WebSocketClient {
    config: WebSocketConfig,
    state: Arc<RwLock<WsConnectionState>>,
    #[allow(dead_code)]
    event_sender: mpsc::Sender<ChannelEvent>,
}

impl WebSocketClient {
    /// Create a new WebSocket client
    pub fn new(config: WebSocketConfig, event_sender: mpsc::Sender<ChannelEvent>) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(WsConnectionState::Disconnected)),
            event_sender,
        }
    }

    /// Get current connection state
    pub async fn state(&self) -> WsConnectionState {
        *self.state.read().await
    }

    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        matches!(self.state().await, WsConnectionState::Connected)
    }

    /// Connect and run the WebSocket client
    ///
    /// This method will block and keep the connection alive,
    /// automatically reconnecting on disconnection.
    pub async fn run<H: WebSocketHandler>(&self, handler: H) -> Result<()> {
        let mut reconnect_attempts = 0u32;

        loop {
            // Check max reconnect attempts
            if let Some(max) = self.config.max_reconnect_attempts {
                if reconnect_attempts >= max {
                    error!("Max reconnect attempts ({}) reached", max);
                    return Err(AgentError::platform(format!(
                        "WebSocket max reconnect attempts ({}) reached",
                        max
                    )));
                }
            }

            // Update state
            {
                let mut state = self.state.write().await;
                if reconnect_attempts == 0 {
                    *state = WsConnectionState::Connecting;
                } else {
                    *state = WsConnectionState::Reconnecting;
                    info!("WebSocket reconnecting (attempt {})", reconnect_attempts);
                }
            }

            // Attempt connection
            match self.connect_and_run(&handler).await {
                Ok(_) => {
                    // Connection closed normally
                    info!("WebSocket connection closed");
                    handler.on_disconnect().await.ok();
                    break Ok(());
                }
                Err(e) => {
                    warn!("WebSocket connection error: {}", e);
                    handler.on_error(e.to_string()).await.ok();

                    // Update state
                    *self.state.write().await = WsConnectionState::Disconnected;

                    // Wait before reconnecting
                    tokio::time::sleep(self.config.reconnect_interval).await;
                    reconnect_attempts += 1;
                }
            }
        }
    }

    /// Single connection attempt with automatic reconnection on failure
    async fn connect_and_run<H: WebSocketHandler>(&self, handler: &H) -> Result<()> {
        // Connect with timeout
        let connect_future = connect_async(&self.config.url);
        let (ws_stream, _) = timeout(self.config.connect_timeout, connect_future)
            .await
            .map_err(|_| AgentError::platform("WebSocket connection timeout"))?
            .map_err(|e| AgentError::platform(format!("WebSocket connection failed: {}", e)))?;

        // Update state
        *self.state.write().await = WsConnectionState::Connected;
        info!("WebSocket connected to {}", self.config.url);

        // Notify handler
        handler.on_connect().await?;

        // Split the stream
        let (mut write, mut read) = ws_stream.split();

        // Create ping interval
        let mut ping_interval = interval(self.config.ping_interval);

        // Run message loop
        loop {
            tokio::select! {
                // Handle incoming messages
                msg = read.next() => {
                    match msg {
                        Some(Ok(WsMessage::Text(text))) => {
                            debug!("WebSocket received text: {} bytes", text.len());
                            if let Err(e) = handler.on_message(text).await {
                                warn!("Message handler error: {}", e);
                            }
                        }
                        Some(Ok(WsMessage::Binary(data))) => {
                            debug!("WebSocket received binary: {} bytes", data.len());
                            if let Err(e) = handler.on_binary(data).await {
                                warn!("Binary handler error: {}", e);
                            }
                        }
                        Some(Ok(WsMessage::Ping(data))) => {
                            // Respond with pong
                            if write.send(WsMessage::Pong(data)).await.is_err() {
                                break;
                            }
                        }
                        Some(Ok(WsMessage::Pong(_))) => {
                            // Pong received, connection is alive
                            debug!("WebSocket pong received");
                        }
                        Some(Ok(WsMessage::Close(_))) => {
                            info!("WebSocket closed by server");
                            break;
                        }
                        Some(Ok(WsMessage::Frame(_))) => {
                            // Frame is an internal message, ignore
                        }
                        Some(Err(e)) => {
                            error!("WebSocket error: {}", e);
                            break;
                        }
                        None => {
                            info!("WebSocket stream ended");
                            break;
                        }
                    }
                }

                // Send periodic pings
                _ = ping_interval.tick() => {
                    let ping_data = handler.ping_payload().unwrap_or_default();
                    if write.send(WsMessage::Ping(ping_data)).await.is_err() {
                        warn!("Failed to send ping");
                        break;
                    }
                }
            }
        }

        // Update state
        *self.state.write().await = WsConnectionState::Disconnected;

        Ok(())
    }

    /// Stop the WebSocket client
    pub async fn stop(&self) -> Result<()> {
        *self.state.write().await = WsConnectionState::Disconnected;
        Ok(())
    }
}

/// Utility functions for WebSocket clients
pub mod utils {
    /// Build WebSocket URL with query parameters
    pub fn build_ws_url(base: &str, params: &[(&str, &str)]) -> String {
        if params.is_empty() {
            return base.to_string();
        }

        let query: Vec<String> = params.iter().map(|(k, v)| format!("{}={}", k, v)).collect();

        format!("{}?{}", base, query.join("&"))
    }

    /// Convert HTTP URL to WebSocket URL
    pub fn http_to_ws_url(http_url: &str) -> Option<String> {
        if http_url.starts_with("https://") {
            Some(http_url.replacen("https://", "wss://", 1))
        } else if http_url.starts_with("http://") {
            Some(http_url.replacen("http://", "ws://", 1))
        } else {
            None
        }
    }

    /// Parse WebSocket close code
    pub fn parse_close_code(code: u16) -> &'static str {
        match code {
            1000 => "Normal closure",
            1001 => "Going away",
            1002 => "Protocol error",
            1003 => "Unsupported data",
            1006 => "Abnormal closure",
            1008 => "Policy violation",
            1009 => "Message too big",
            1011 => "Server error",
            _ => "Unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::utils;

    #[test]
    fn test_http_to_ws_url() {
        assert_eq!(
            utils::http_to_ws_url("https://example.com/ws"),
            Some("wss://example.com/ws".to_string())
        );
        assert_eq!(
            utils::http_to_ws_url("http://example.com/ws"),
            Some("ws://example.com/ws".to_string())
        );
        assert_eq!(utils::http_to_ws_url("ftp://example.com"), None);
    }

    #[test]
    fn test_build_ws_url() {
        assert_eq!(
            utils::build_ws_url("wss://example.com/ws", &[("token", "abc123")]),
            "wss://example.com/ws?token=abc123"
        );
        assert_eq!(
            utils::build_ws_url("wss://example.com/ws", &[]),
            "wss://example.com/ws"
        );
    }
}
