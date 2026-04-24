//! WebSocket Client for BeeBotOS CLI
//!
//! Provides real-time streaming capabilities for watching agents, blocks,
//! events, and tasks. Uses tokio-tungstenite for WebSocket connectivity.

#![allow(dead_code)]

use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use url::Url;

/// WebSocket message types (matching gateway protocol)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsMessage {
    /// Standard text message
    Text { content: String },
    /// Binary data
    Binary { data: Vec<u8> },
    /// Heartbeat ping
    Ping { timestamp: u64 },
    /// Heartbeat pong response
    Pong { timestamp: u64 },
    /// Subscribe to channel
    Subscribe { channel: String },
    /// Unsubscribe from channel
    Unsubscribe { channel: String },
    /// Broadcast to channel
    Broadcast { channel: String, payload: Value },
    /// Connection info response
    Connected { connection_id: String },
    /// Error message
    Error { code: String, message: String },
    /// Server notification
    Notification { title: String, body: Value },
}

/// WebSocket client for BeeBotOS
pub struct WebSocketClient {
    url: String,
    api_key: String,
    connection_id: Option<String>,
    ws_stream: Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,
}

impl WebSocketClient {
    /// Create a new WebSocket client
    pub fn new(url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            api_key: api_key.into(),
            connection_id: None,
            ws_stream: None,
        }
    }

    #[allow(dead_code)]
    /// Create client from environment variables
    pub fn from_env() -> Result<Self> {
        let url = std::env::var("BEEBOTOS_WS_URL")
            .unwrap_or_else(|_| "ws://localhost:8080/ws".to_string());
        let api_key =
            std::env::var("BEEBOTOS_API_KEY").map_err(|_| anyhow!("BEEBOTOS_API_KEY not set"))?;
        Ok(Self::new(url, api_key))
    }

    #[allow(dead_code)]
    /// Convert HTTP URL to WebSocket URL
    pub fn from_http_url(http_url: &str, api_key: &str) -> Result<Self> {
        let ws_url = if http_url.starts_with("https://") {
            http_url.replace("https://", "wss://")
        } else if http_url.starts_with("http://") {
            http_url.replace("http://", "ws://")
        } else {
            http_url.to_string()
        };

        // Ensure path ends with /ws
        let ws_url = if ws_url.ends_with("/ws") {
            ws_url
        } else if ws_url.ends_with('/') {
            format!("{}ws", ws_url)
        } else {
            format!("{}/ws", ws_url)
        };

        Ok(Self::new(ws_url, api_key.to_string()))
    }

    /// Connect to the WebSocket server
    pub async fn connect(&mut self) -> Result<()> {
        // Parse and validate URL
        let url = Url::parse(&self.url)
            .with_context(|| format!("Invalid WebSocket URL: {}", self.url))?;

        // API key should be passed in the connection headers, not URL
        // Note: tokio-tungstenite doesn't support custom headers directly,
        // so we use a subprotocol or send auth message after connection
        let url_with_auth = url.to_string();

        // Connect with timeout
        let (ws_stream, _) = timeout(Duration::from_secs(10), connect_async(&url_with_auth))
            .await
            .with_context(|| "WebSocket connection timeout")?
            .with_context(|| format!("Failed to connect to WebSocket at {}", self.url))?;

        self.ws_stream = Some(ws_stream);

        // Wait for connection confirmation
        self.wait_for_connection().await?;

        Ok(())
    }

    /// Wait for connection confirmation from server
    async fn wait_for_connection(&mut self) -> Result<()> {
        let ws_stream = self
            .ws_stream
            .as_mut()
            .ok_or_else(|| anyhow!("Not connected"))?;

        // Wait up to 5 seconds for connection message
        let result = timeout(Duration::from_secs(5), async {
            while let Some(msg) = ws_stream.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if let Ok(ws_msg) = serde_json::from_str::<WsMessage>(&text) {
                            match ws_msg {
                                WsMessage::Connected { connection_id } => {
                                    return Ok(connection_id);
                                }
                                WsMessage::Error { code, message } => {
                                    return Err(anyhow!("Server error: {} - {}", code, message));
                                }
                                _ => continue,
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        return Err(anyhow!("Connection closed by server"));
                    }
                    Err(e) => {
                        return Err(anyhow!("WebSocket error: {}", e));
                    }
                    _ => continue,
                }
            }
            Err(anyhow!("Stream ended without connection confirmation"))
        })
        .await;

        match result {
            Ok(Ok(connection_id)) => {
                self.connection_id = Some(connection_id);
                Ok(())
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(anyhow!("Timeout waiting for connection confirmation")),
        }
    }

    /// Subscribe to a channel
    pub async fn subscribe(&mut self, channel: impl Into<String>) -> Result<()> {
        let channel = channel.into();
        let msg = WsMessage::Subscribe { channel };
        self.send_message(&msg).await
    }

    #[allow(dead_code)]
    /// Unsubscribe from a channel
    pub async fn unsubscribe(&mut self, channel: impl Into<String>) -> Result<()> {
        let channel = channel.into();
        let msg = WsMessage::Unsubscribe { channel };
        self.send_message(&msg).await
    }

    /// Send a message to the server
    async fn send_message(&mut self, msg: &WsMessage) -> Result<()> {
        let ws_stream = self
            .ws_stream
            .as_mut()
            .ok_or_else(|| anyhow!("Not connected"))?;

        let json = serde_json::to_string(msg)?;
        ws_stream
            .send(Message::Text(json))
            .await
            .with_context(|| "Failed to send WebSocket message")?;
        Ok(())
    }

    #[allow(dead_code)]
    /// Receive next message from the server
    pub async fn recv(&mut self) -> Result<Option<WsMessage>> {
        let ws_stream = self
            .ws_stream
            .as_mut()
            .ok_or_else(|| anyhow!("Not connected"))?;

        loop {
            match ws_stream.next().await {
                Some(Ok(Message::Text(text))) => {
                    if let Ok(msg) = serde_json::from_str::<WsMessage>(&text) {
                        return Ok(Some(msg));
                    }
                }
                Some(Ok(Message::Ping(data))) => {
                    // Respond with pong
                    ws_stream.send(Message::Pong(data)).await.ok();
                    continue;
                }
                Some(Ok(Message::Close(_))) => {
                    return Ok(None);
                }
                Some(Err(e)) => {
                    return Err(anyhow!("WebSocket error: {}", e));
                }
                _ => continue,
            }
        }
    }

    /// Watch agents - returns a stream
    pub async fn watch_agents(
        &mut self,
    ) -> Result<futures::stream::BoxStream<'static, Result<AgentUpdate>>> {
        self.subscribe("agents").await?;

        // Take ownership of ws_stream for the stream
        let ws_stream = self
            .ws_stream
            .take()
            .ok_or_else(|| anyhow!("Not connected"))?;

        let stream = futures::stream::unfold(ws_stream, |mut ws_stream| async move {
            loop {
                match ws_stream.next().await {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(json) = serde_json::from_str::<Value>(&text) {
                            let value = json.get("body").cloned().unwrap_or(json);
                            match serde_json::from_value::<AgentUpdate>(value) {
                                Ok(update) => return Some((Ok(update), ws_stream)),
                                Err(e) => {
                                    return Some((Err(anyhow!("Parse error: {}", e)), ws_stream))
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = ws_stream.send(Message::Pong(data)).await;
                        continue;
                    }
                    Some(Ok(Message::Close(_))) => {
                        return Some((Err(anyhow!("WebSocket connection closed")), ws_stream));
                    }
                    Some(Err(e)) => {
                        return Some((Err(anyhow!("WebSocket error: {}", e)), ws_stream));
                    }
                    _ => continue,
                }
            }
        });

        Ok(Box::pin(stream))
    }

    /// Watch blocks - returns a stream
    pub async fn watch_blocks(
        &mut self,
    ) -> Result<futures::stream::BoxStream<'static, Result<BlockInfo>>> {
        self.subscribe("blocks").await?;

        let ws_stream = self
            .ws_stream
            .take()
            .ok_or_else(|| anyhow!("Not connected"))?;

        let stream = futures::stream::unfold(ws_stream, |mut ws_stream| async move {
            loop {
                match ws_stream.next().await {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(json) = serde_json::from_str::<Value>(&text) {
                            let value = json.get("body").cloned().unwrap_or(json);
                            match serde_json::from_value::<BlockInfo>(value) {
                                Ok(info) => return Some((Ok(info), ws_stream)),
                                Err(e) => {
                                    return Some((Err(anyhow!("Parse error: {}", e)), ws_stream))
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = ws_stream.send(Message::Pong(data)).await;
                        continue;
                    }
                    Some(Ok(Message::Close(_))) => {
                        return Some((Err(anyhow!("WebSocket connection closed")), ws_stream));
                    }
                    Some(Err(e)) => {
                        return Some((Err(anyhow!("WebSocket error: {}", e)), ws_stream));
                    }
                    _ => continue,
                }
            }
        });

        Ok(Box::pin(stream))
    }

    /// Watch events - returns a stream
    pub async fn watch_events(
        &mut self,
        agent_id: Option<&str>,
    ) -> Result<futures::stream::BoxStream<'static, Result<EventInfo>>> {
        let channel = agent_id
            .map(|id| format!("events:{}", id))
            .unwrap_or_else(|| "events".to_string());
        self.subscribe(channel).await?;

        let ws_stream = self
            .ws_stream
            .take()
            .ok_or_else(|| anyhow!("Not connected"))?;

        let stream = futures::stream::unfold(ws_stream, |mut ws_stream| async move {
            loop {
                match ws_stream.next().await {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(json) = serde_json::from_str::<Value>(&text) {
                            let value = json.get("body").cloned().unwrap_or(json);
                            match serde_json::from_value::<EventInfo>(value) {
                                Ok(info) => return Some((Ok(info), ws_stream)),
                                Err(e) => {
                                    return Some((Err(anyhow!("Parse error: {}", e)), ws_stream))
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = ws_stream.send(Message::Pong(data)).await;
                        continue;
                    }
                    Some(Ok(Message::Close(_))) => {
                        return Some((Err(anyhow!("WebSocket connection closed")), ws_stream));
                    }
                    Some(Err(e)) => {
                        return Some((Err(anyhow!("WebSocket error: {}", e)), ws_stream));
                    }
                    _ => continue,
                }
            }
        });

        Ok(Box::pin(stream))
    }

    /// Watch tasks - returns a stream
    pub async fn watch_tasks(
        &mut self,
        agent_id: Option<&str>,
    ) -> Result<futures::stream::BoxStream<'static, Result<TaskUpdate>>> {
        let channel = agent_id
            .map(|id| format!("tasks:{}", id))
            .unwrap_or_else(|| "tasks".to_string());
        self.subscribe(channel).await?;

        let ws_stream = self
            .ws_stream
            .take()
            .ok_or_else(|| anyhow!("Not connected"))?;

        let stream = futures::stream::unfold(ws_stream, |mut ws_stream| async move {
            loop {
                match ws_stream.next().await {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(json) = serde_json::from_str::<Value>(&text) {
                            let value = json.get("body").cloned().unwrap_or(json);
                            match serde_json::from_value::<TaskUpdate>(value) {
                                Ok(update) => return Some((Ok(update), ws_stream)),
                                Err(e) => {
                                    return Some((Err(anyhow!("Parse error: {}", e)), ws_stream))
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = ws_stream.send(Message::Pong(data)).await;
                        continue;
                    }
                    Some(Ok(Message::Close(_))) => {
                        return Some((Err(anyhow!("WebSocket connection closed")), ws_stream));
                    }
                    Some(Err(e)) => {
                        return Some((Err(anyhow!("WebSocket error: {}", e)), ws_stream));
                    }
                    _ => continue,
                }
            }
        });

        Ok(Box::pin(stream))
    }

    #[allow(dead_code)]
    /// Close the connection gracefully
    pub async fn close(&mut self) -> Result<()> {
        if let Some(mut ws_stream) = self.ws_stream.take() {
            let _ = ws_stream.close(None).await;
        }
        self.connection_id = None;
        Ok(())
    }

    #[allow(dead_code)]
    /// Get connection ID
    pub fn connection_id(&self) -> Option<&str> {
        self.connection_id.as_deref()
    }
}

/// Agent update from WebSocket
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentUpdate {
    pub timestamp: String,
    pub agent_id: String,
    pub old_status: String,
    pub new_status: String,
}

/// Block info from WebSocket
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BlockInfo {
    pub number: u64,
    pub tx_count: usize,
    pub gas_used: u64,
}

/// Event info from WebSocket
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventInfo {
    pub timestamp: String,
    pub event_type: String,
    pub data: Value,
}

/// Task update from WebSocket
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskUpdate {
    pub timestamp: String,
    pub id: String,
    pub status: String,
    pub agent_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_websocket_client_new() {
        let client = WebSocketClient::new("ws://localhost:8080/ws", "test-key");
        assert_eq!(client.url, "ws://localhost:8080/ws");
        assert_eq!(client.api_key, "test-key");
        assert!(client.connection_id.is_none());
        assert!(client.ws_stream.is_none());
    }

    #[test]
    fn test_url_conversion_http() {
        let client = WebSocketClient::from_http_url("http://localhost:8080", "test-key").unwrap();
        assert!(client.url.starts_with("ws://"));
        assert!(client.url.contains("/ws"));
    }

    #[test]
    fn test_url_conversion_https() {
        let client = WebSocketClient::from_http_url("https://api.beebotos.io", "test-key").unwrap();
        assert!(client.url.starts_with("wss://"));
        assert!(client.url.contains("/ws"));
    }

    #[test]
    fn test_url_conversion_already_ws() {
        let client = WebSocketClient::from_http_url("ws://localhost:8080/ws", "test-key").unwrap();
        assert_eq!(client.url, "ws://localhost:8080/ws");
    }

    #[test]
    fn test_ws_message_serialization_subscribe() {
        let msg = WsMessage::Subscribe {
            channel: "agents".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("subscribe"));
        assert!(json.contains("agents"));
    }

    #[test]
    fn test_ws_message_serialization_connected() {
        let msg = WsMessage::Connected {
            connection_id: "conn-123".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("connected"));
        assert!(json.contains("conn-123"));
    }

    #[test]
    fn test_ws_message_serialization_error() {
        let msg = WsMessage::Error {
            code: "AUTH_ERROR".to_string(),
            message: "Invalid API key".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("error"));
        assert!(json.contains("AUTH_ERROR"));
    }

    #[test]
    fn test_agent_update_deserialization() {
        let json = r#"{
            "timestamp": "2024-01-15T10:30:00Z",
            "agent_id": "agent-123",
            "old_status": "idle",
            "new_status": "running"
        }"#;
        let update: AgentUpdate = serde_json::from_str(json).unwrap();
        assert_eq!(update.agent_id, "agent-123");
        assert_eq!(update.old_status, "idle");
        assert_eq!(update.new_status, "running");
    }

    #[test]
    fn test_block_info_deserialization() {
        let json = r#"{
            "number": 12345,
            "tx_count": 10,
            "gas_used": 21000
        }"#;
        let block: BlockInfo = serde_json::from_str(json).unwrap();
        assert_eq!(block.number, 12345);
        assert_eq!(block.tx_count, 10);
        assert_eq!(block.gas_used, 21000);
    }

    #[test]
    fn test_event_info_deserialization() {
        let json = r#"{
            "timestamp": "2024-01-15T10:30:00Z",
            "event_type": "agent_created",
            "data": {"agent_id": "agent-123"}
        }"#;
        let event: EventInfo = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "agent_created");
    }

    #[test]
    fn test_task_update_deserialization() {
        let json = r#"{
            "timestamp": "2024-01-15T10:30:00Z",
            "id": "task-456",
            "status": "completed",
            "agent_id": "agent-123"
        }"#;
        let task: TaskUpdate = serde_json::from_str(json).unwrap();
        assert_eq!(task.id, "task-456");
        assert_eq!(task.status, "completed");
    }

    #[tokio::test]
    async fn test_websocket_client_creation() {
        let client = WebSocketClient::new("ws://localhost:8080/ws", "test-key");
        assert!(!client.connection_id().is_some());
    }

    #[test]
    fn test_invalid_url_parsing() {
        // This should fail URL parsing
        let result = Url::parse("not-a-valid-url");
        assert!(result.is_err());
    }
}
