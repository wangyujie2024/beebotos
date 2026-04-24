//! WebSocket 客户端
//!
//! 实现与 Gateway 的实时 WebSocket 通信

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// WebSocket 消息类型
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebSocketMessage {
    // 认证
    Auth {
        token: String,
    },
    AuthResponse {
        success: bool,
        #[serde(default)]
        error: Option<String>,
    },

    // 心跳
    Ping,
    Pong,

    // 订阅
    Subscribe {
        channel: String,
    },
    Unsubscribe {
        channel: String,
    },
    SubscribeResponse {
        channel: String,
        success: bool,
    },

    // 聊天消息
    ChatMessage {
        session_id: String,
        message: super::super::webchat::ChatMessage,
    },
    ChatStream {
        session_id: String,
        chunk: String,
        done: bool,
    },
    ChatTyping {
        session_id: String,
        is_typing: bool,
    },

    // 浏览器事件
    BrowserEvent {
        instance_id: String,
        event: super::super::browser::BrowserEvent,
    },
    BrowserScreenshot {
        instance_id: String,
        data: String,
    },
    BrowserStatus {
        instance_id: String,
        status: super::super::browser::ConnectionStatus,
    },

    // Agent 事件
    AgentStatus {
        agent_id: String,
        status: String,
    },
    AgentLog {
        agent_id: String,
        level: String,
        message: String,
    },
    AgentOutput {
        agent_id: String,
        output: String,
    },

    // 系统通知
    Notification {
        title: String,
        message: String,
        level: NotificationLevel,
    },
    SystemStatus {
        status: String,
        message: String,
    },

    // 错误
    Error {
        code: String,
        message: String,
    },
}

/// 通知级别
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationLevel {
    Info,
    Success,
    Warning,
    Error,
}

/// WebSocket 客户端
#[derive(Clone, Debug)]
pub struct WebSocketClient {
    _url: String,
    connected: bool,
    subscribed_channels: HashSet<String>,
}

impl WebSocketClient {
    /// 创建新的 WebSocket 客户端
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            _url: url.into(),
            connected: false,
            subscribed_channels: HashSet::new(),
        }
    }

    /// 连接到 WebSocket
    pub async fn connect(&mut self) -> Result<(), WebSocketError> {
        // 在实际实现中，这里会创建 WebSocket 连接
        // 由于 WASM 环境限制，需要通过 Gateway API 代理
        self.connected = true;
        Ok(())
    }

    /// 断开连接
    pub fn disconnect(&mut self) {
        self.connected = false;
        self.subscribed_channels.clear();
    }

    /// 检查是否已连接
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// 发送消息
    pub async fn send(&mut self, message: WebSocketMessage) -> Result<(), WebSocketError> {
        if !self.connected {
            return Err(WebSocketError::NotConnected);
        }

        // 实际发送逻辑
        let _json = serde_json::to_string(&message)
            .map_err(|e| WebSocketError::Serialization(e.to_string()))?;

        // 发送到 WebSocket
        Ok(())
    }

    /// 订阅频道
    pub async fn subscribe(&mut self, channel: &str) -> Result<(), WebSocketError> {
        if !self.connected {
            return Err(WebSocketError::NotConnected);
        }

        let msg = WebSocketMessage::Subscribe {
            channel: channel.to_string(),
        };

        self.send(msg).await?;
        self.subscribed_channels.insert(channel.to_string());

        Ok(())
    }

    /// 取消订阅
    pub async fn unsubscribe(&mut self, channel: &str) -> Result<(), WebSocketError> {
        if !self.connected {
            return Err(WebSocketError::NotConnected);
        }

        let msg = WebSocketMessage::Unsubscribe {
            channel: channel.to_string(),
        };

        self.send(msg).await?;
        self.subscribed_channels.remove(channel);

        Ok(())
    }

    /// 发送 ping
    pub async fn ping(&mut self) -> Result<(), WebSocketError> {
        self.send(WebSocketMessage::Ping).await
    }

    /// 获取已订阅的频道
    pub fn subscribed_channels(&self) -> &HashSet<String> {
        &self.subscribed_channels
    }
}

/// WebSocket 错误
#[derive(Clone, Debug)]
pub enum WebSocketError {
    NotConnected,
    ConnectionFailed(String),
    Serialization(String),
    SendFailed(String),
    ReceiveFailed(String),
}

impl std::fmt::Display for WebSocketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WebSocketError::NotConnected => write!(f, "WebSocket not connected"),
            WebSocketError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            WebSocketError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            WebSocketError::SendFailed(msg) => write!(f, "Send failed: {}", msg),
            WebSocketError::ReceiveFailed(msg) => write!(f, "Receive failed: {}", msg),
        }
    }
}

/// WebSocket 连接管理器
pub struct WebSocketManager {
    clients: Vec<WebSocketClient>,
}

impl WebSocketManager {
    pub fn new() -> Self {
        Self {
            clients: Vec::new(),
        }
    }

    pub fn add_client(&mut self, client: WebSocketClient) {
        self.clients.push(client);
    }

    pub fn disconnect_all(&mut self) {
        for client in &mut self.clients {
            client.disconnect();
        }
        self.clients.clear();
    }
}

impl Default for WebSocketManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_websocket_message_serialization() {
        let msg = WebSocketMessage::Ping;
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("ping"));

        let msg = WebSocketMessage::Auth {
            token: "test-token".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("auth"));
        assert!(json.contains("test-token"));
    }

    #[test]
    fn test_notification_level() {
        let level = NotificationLevel::Warning;
        let json = serde_json::to_string(&level).unwrap();
        assert_eq!(json, "\"warning\"");
    }
}
