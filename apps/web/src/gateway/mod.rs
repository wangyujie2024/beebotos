//! Gateway 集成模块
//!
//! 提供与 beebotos-gateway 的完整对接，包括：
//! - WebSocket 实时通信
//! - 认证集成
//! - 权限范围控制
//! - 浏览器状态检查

use serde::{Deserialize, Serialize};
// HashSet may be used in future implementations

pub mod auth;
pub mod scopes;
pub mod websocket;

pub use auth::{GatewayAuth, TokenManager};
pub use scopes::{GatewayScope, ScopeManager};
pub use websocket::{WebSocketClient, WebSocketMessage};

/// Gateway 配置
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GatewayConfig {
    pub api_base_url: String,
    pub websocket_url: String,
    pub auth: GatewayAuthConfig,
    pub connection: ConnectionConfig,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            // Gateway API runs on port 8000 per project docs
            api_base_url: "http://localhost:8000/api/v1".to_string(),
            websocket_url: "ws://localhost:8000/ws".to_string(),
            auth: GatewayAuthConfig::default(),
            connection: ConnectionConfig::default(),
        }
    }
}

impl GatewayConfig {
    pub fn new(api_base_url: impl Into<String>, websocket_url: impl Into<String>) -> Self {
        Self {
            api_base_url: api_base_url.into(),
            websocket_url: websocket_url.into(),
            ..Default::default()
        }
    }
}

/// Gateway 认证配置
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GatewayAuthConfig {
    pub scopes: Vec<GatewayScope>,
    pub allow_deviceless: bool,
    pub session_timeout_seconds: u64,
    pub auto_refresh_token: bool,
}

impl Default for GatewayAuthConfig {
    fn default() -> Self {
        Self {
            scopes: vec![
                GatewayScope::BrowserRead,
                GatewayScope::BrowserWrite,
                GatewayScope::ChatRead,
                GatewayScope::ChatWrite,
            ],
            allow_deviceless: false,
            session_timeout_seconds: 3600,
            auto_refresh_token: true,
        }
    }
}

/// 连接配置
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub reconnect_interval_ms: u64,
    pub max_reconnect_attempts: u32,
    pub heartbeat_interval_ms: u64,
    pub connection_timeout_ms: u64,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            reconnect_interval_ms: 3000,
            max_reconnect_attempts: 5,
            heartbeat_interval_ms: 30000,
            connection_timeout_ms: 10000,
        }
    }
}

/// Gateway 连接状态
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum GatewayStatus {
    Disconnected,
    Connecting,
    Connected,
    Authenticating,
    Authenticated,
    Reconnecting,
    Error(String),
}

impl Default for GatewayStatus {
    fn default() -> Self {
        GatewayStatus::Disconnected
    }
}

/// Gateway 客户端
#[derive(Clone, Debug)]
pub struct GatewayClient {
    config: GatewayConfig,
    status: GatewayStatus,
    websocket: Option<WebSocketClient>,
    _auth: Option<GatewayAuth>,
    scopes: ScopeManager,
}

impl GatewayClient {
    /// 创建新的 Gateway 客户端
    pub fn new(config: GatewayConfig) -> Self {
        Self {
            config: config.clone(),
            status: GatewayStatus::Disconnected,
            websocket: None,
            _auth: None,
            scopes: ScopeManager::new(config.auth.scopes.clone()),
        }
    }

    /// 获取当前状态
    pub fn status(&self) -> &GatewayStatus {
        &self.status
    }

    /// 检查是否已连接
    pub fn is_connected(&self) -> bool {
        matches!(self.status, GatewayStatus::Connected | GatewayStatus::Authenticated)
    }

    /// 检查是否已认证
    pub fn is_authenticated(&self) -> bool {
        matches!(self.status, GatewayStatus::Authenticated)
    }

    /// 连接到 Gateway
    pub async fn connect(&mut self) -> Result<(), GatewayError> {
        self.status = GatewayStatus::Connecting;

        // 创建 WebSocket 连接
        let mut ws_client = WebSocketClient::new(&self.config.websocket_url);

        match ws_client.connect().await {
            Ok(_) => {
                self.status = GatewayStatus::Connected;
                self.websocket = Some(ws_client);
                Ok(())
            }
            Err(e) => {
                self.status = GatewayStatus::Error(e.to_string());
                Err(GatewayError::ConnectionFailed(e.to_string()))
            }
        }
    }

    /// 断开连接
    pub fn disconnect(&mut self) {
        if let Some(mut ws) = self.websocket.take() {
            ws.disconnect();
        }
        self.status = GatewayStatus::Disconnected;
    }

    /// 认证
    pub async fn authenticate(&mut self, token: &str) -> Result<(), GatewayError> {
        self.status = GatewayStatus::Authenticating;

        if let Some(ws) = &mut self.websocket {
            // 发送认证消息
            let auth_msg = WebSocketMessage::Auth {
                token: token.to_string(),
            };

            ws.send(auth_msg).await.map_err(|e| GatewayError::AuthFailed(e.to_string()))?;

            self.status = GatewayStatus::Authenticated;
            Ok(())
        } else {
            Err(GatewayError::NotConnected)
        }
    }

    /// 订阅频道
    pub async fn subscribe(&mut self, channel: &str) -> Result<(), GatewayError> {
        if let Some(ws) = &mut self.websocket {
            ws.subscribe(channel).await.map_err(|e| GatewayError::SubscribeFailed(e.to_string()))
        } else {
            Err(GatewayError::NotConnected)
        }
    }

    /// 检查权限范围
    pub fn has_scope(&self, scope: GatewayScope) -> bool {
        self.scopes.has_scope(scope)
    }

    /// 获取配置
    pub fn config(&self) -> &GatewayConfig {
        &self.config
    }

    /// 获取 WebSocket 客户端（可变）
    pub fn websocket_mut(&mut self) -> Option<&mut WebSocketClient> {
        self.websocket.as_mut()
    }
}

/// Gateway 错误
#[derive(Clone, Debug)]
pub enum GatewayError {
    ConnectionFailed(String),
    NotConnected,
    AuthFailed(String),
    SubscribeFailed(String),
    SendFailed(String),
    ScopeDenied(GatewayScope),
    Timeout,
}

impl std::fmt::Display for GatewayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GatewayError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            GatewayError::NotConnected => write!(f, "Not connected to gateway"),
            GatewayError::AuthFailed(msg) => write!(f, "Authentication failed: {}", msg),
            GatewayError::SubscribeFailed(msg) => write!(f, "Subscribe failed: {}", msg),
            GatewayError::SendFailed(msg) => write!(f, "Send failed: {}", msg),
            GatewayError::ScopeDenied(scope) => write!(f, "Scope denied: {:?}", scope),
            GatewayError::Timeout => write!(f, "Operation timed out"),
        }
    }
}

/// Gateway 状态信息
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GatewayStatusInfo {
    pub status: GatewayStatus,
    pub connected_at: Option<String>,
    pub last_ping_at: Option<String>,
    pub latency_ms: Option<u64>,
    pub subscribed_channels: Vec<String>,
    pub active_scopes: Vec<GatewayScope>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_config() {
        let config = GatewayConfig::new(
            "http://api.example.com",
            "ws://ws.example.com",
        );

        assert_eq!(config.api_base_url, "http://api.example.com");
        assert_eq!(config.websocket_url, "ws://ws.example.com");
    }

    #[test]
    fn test_gateway_status() {
        let status = GatewayStatus::Connected;
        assert!(matches!(status, GatewayStatus::Connected));
    }

    #[test]
    fn test_scope_check() {
        let config = GatewayConfig::default();
        let client = GatewayClient::new(config);

        assert!(client.has_scope(GatewayScope::BrowserRead));
        assert!(!client.has_scope(GatewayScope::Admin));
    }
}
