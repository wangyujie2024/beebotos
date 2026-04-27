//! 统一 Channel Trait 设计
//!
//! 参考 beebot 项目的 Channel trait 设计，提供统一的通讯平台接口

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{mpsc, RwLock};

use crate::communication::{Message, PlatformType};
use crate::error::Result;

/// Channel factory trait
///
/// Implement this trait to create a new channel type.
/// Factories are registered with the ChannelRegistry and used to
/// create channel instances from configuration.
#[async_trait]
pub trait ChannelFactory: Send + Sync {
    /// Factory name (e.g., "lark", "dingtalk", "telegram")
    fn name(&self) -> &str;

    /// Get platform type
    fn platform_type(&self) -> PlatformType;

    /// Create a channel instance from configuration
    ///
    /// # Arguments
    /// * `config` - JSON configuration for the channel
    ///
    /// # Returns
    /// A new channel instance wrapped in Arc<RwLock<>>
    async fn create(&self, config: &Value) -> Result<Arc<RwLock<dyn Channel>>>;

    /// Validate configuration
    ///
    /// Returns true if the configuration is valid for this channel type.
    fn validate_config(&self, config: &Value) -> bool;

    /// Get default configuration
    ///
    /// Returns a JSON value with default settings for this channel.
    fn default_config(&self) -> Value;
}

/// 统一通道 trait
///
/// 所有通讯平台适配器必须实现此 trait
#[async_trait]
pub trait Channel: Send + Sync + 'static {
    /// 获取通道名称
    fn name(&self) -> &str;

    /// 获取平台类型
    fn platform(&self) -> PlatformType;

    /// 检查是否已连接
    fn is_connected(&self) -> bool;

    /// 建立连接
    ///
    /// 根据配置自动选择合适的连接模式（WebSocket/Webhook/Polling）
    async fn connect(&mut self) -> Result<()>;

    /// 断开连接
    async fn disconnect(&mut self) -> Result<()>;

    /// 发送消息到指定频道
    ///
    /// # Arguments
    /// * `channel_id` - 目标频道/聊天 ID
    /// * `message` - 要发送的消息
    async fn send(&self, channel_id: &str, message: &Message) -> Result<()>;

    /// 启动消息监听
    ///
    /// 根据配置的连接模式启动相应的监听器：
    /// - WebSocket: 建立长连接，实时接收消息
    /// - Webhook: 启动 HTTP 服务器接收回调
    /// - Polling: 定期轮询获取新消息
    ///
    /// 收到的消息通过 event_bus 发送给上层处理
    async fn start_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()>;

    /// 停止消息监听
    async fn stop_listener(&self) -> Result<()>;

    /// 获取支持的内容类型
    fn supported_content_types(&self) -> Vec<ContentType>;

    /// 获取频道信息列表
    async fn list_channels(&self) -> Result<Vec<ChannelInfo>>;

    /// 获取频道成员列表
    async fn list_members(&self, channel_id: &str) -> Result<Vec<MemberInfo>>;

    /// 获取连接模式
    fn connection_mode(&self) -> ConnectionMode;

    /// 下载图片（可选功能）
    ///
    /// # Arguments
    /// * `file_key` - 文件/图片的 key
    /// * `message_id` - 可选的消息
    ///   ID，某些平台（如飞书）需要它来下载消息中的资源
    ///
    /// 返回图片的二进制数据
    async fn download_image(
        &self,
        _file_key: &str,
        _message_id: Option<&str>,
    ) -> crate::error::Result<Vec<u8>> {
        Err(crate::error::AgentError::platform(
            "Image download not supported for this channel",
        ))
    }

    /// 转换为 Any 引用，用于 downcast 到具体类型
    fn as_any(&self) -> &dyn std::any::Any {
        panic!("as_any not implemented for this channel")
    }
}

/// 连接模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionMode {
    /// WebSocket 长连接（推荐）
    #[default]
    WebSocket,
    /// HTTP Webhook 回调
    Webhook,
    /// 轮询（用于不支持 WebSocket 的平台）
    Polling,
}

impl ConnectionMode {
    pub fn is_websocket(&self) -> bool {
        matches!(self, ConnectionMode::WebSocket)
    }

    pub fn is_webhook(&self) -> bool {
        matches!(self, ConnectionMode::Webhook)
    }

    pub fn is_polling(&self) -> bool {
        matches!(self, ConnectionMode::Polling)
    }
}

impl std::fmt::Display for ConnectionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionMode::WebSocket => write!(f, "websocket"),
            ConnectionMode::Webhook => write!(f, "webhook"),
            ConnectionMode::Polling => write!(f, "polling"),
        }
    }
}

/// 通道事件
#[derive(Debug, Clone)]
pub enum ChannelEvent {
    /// 收到消息
    MessageReceived {
        platform: PlatformType,
        channel_id: String,
        message: Message,
    },
    /// 连接状态变化
    ConnectionStateChanged {
        platform: PlatformType,
        connected: bool,
        reason: Option<String>,
    },
    /// 错误
    Error {
        platform: PlatformType,
        error: String,
    },
}

/// 内容类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Text,
    Image,
    File,
    Audio,
    Video,
    Location,
    Sticker,
    Reaction,
    Rich,
    Card,
}

/// 频道信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelInfo {
    pub id: String,
    pub name: String,
    pub channel_type: ChannelType,
    pub unread_count: u32,
    pub metadata: HashMap<String, String>,
}

/// 频道类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelType {
    Direct,
    Group,
    Thread,
    Channel,
    Broadcast,
}

/// 成员信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberInfo {
    pub id: String,
    pub name: String,
    pub username: Option<String>,
    pub avatar: Option<String>,
    pub is_bot: bool,
    pub role: MemberRole,
}

/// 成员角色
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberRole {
    Owner,
    Admin,
    Member,
    Guest,
}

/// Base channel configuration
///
/// Common configuration fields shared by all channel implementations.
/// Use `#[serde(flatten)]` to include this in specific channel configs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseChannelConfig {
    /// Connection mode (default: WebSocket)
    #[serde(default)]
    pub connection_mode: ConnectionMode,
    /// Auto-reconnect enabled (default: true)
    #[serde(default = "default_auto_reconnect")]
    pub auto_reconnect: bool,
    /// Maximum reconnect attempts (default: 10)
    #[serde(default = "default_max_reconnect")]
    pub max_reconnect_attempts: u32,
    /// Webhook port (default: 8080)
    #[serde(default = "default_webhook_port")]
    pub webhook_port: u16,
    /// Webhook URL for receiving events (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_url: Option<String>,
}

impl Default for BaseChannelConfig {
    fn default() -> Self {
        Self {
            connection_mode: ConnectionMode::WebSocket,
            auto_reconnect: true,
            max_reconnect_attempts: 10,
            webhook_port: 8080,
            webhook_url: None,
        }
    }
}

fn default_auto_reconnect() -> bool {
    true
}

fn default_max_reconnect() -> u32 {
    10
}

fn default_webhook_port() -> u16 {
    8080
}

impl BaseChannelConfig {
    /// Create base config from environment with given prefix
    pub fn from_env(prefix: &str) -> Option<Self> {
        use std::env;

        let connection_mode = env::var(format!("{}_CONNECTION_MODE", prefix))
            .ok()
            .and_then(|m| match m.as_str() {
                "webhook" => Some(ConnectionMode::Webhook),
                "polling" => Some(ConnectionMode::Polling),
                "websocket" => Some(ConnectionMode::WebSocket),
                _ => None,
            })
            .unwrap_or_default();

        let auto_reconnect = env::var(format!("{}_AUTO_RECONNECT", prefix))
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(true);

        let max_reconnect_attempts = env::var(format!("{}_MAX_RECONNECT", prefix))
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10);

        let webhook_port = env::var(format!("{}_WEBHOOK_PORT", prefix))
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8080);

        Some(Self {
            connection_mode,
            auto_reconnect,
            max_reconnect_attempts,
            webhook_port,
            webhook_url: None,
        })
    }
}

/// 通道配置 trait
///
/// 每个通道的配置需要实现此 trait
pub trait ChannelConfig: Send + Sync + Clone {
    /// 从配置创建
    fn from_env() -> Option<Self>
    where
        Self: Sized;

    /// 验证配置是否有效
    fn is_valid(&self) -> bool;

    /// 获取白名单
    fn allowlist(&self) -> Vec<String>;

    /// 获取连接模式
    fn connection_mode(&self) -> ConnectionMode;

    /// 是否自动重连
    fn auto_reconnect(&self) -> bool;

    /// 最大重连次数
    fn max_reconnect_attempts(&self) -> u32;
}

/// WebSocket 连接配置
#[derive(Debug, Clone)]
pub struct WebSocketConfig {
    pub url: String,
    pub ping_interval_secs: u64,
    pub pong_timeout_secs: u64,
    pub reconnect_interval_secs: u64,
    pub max_reconnect_attempts: u32,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            ping_interval_secs: 30,
            pong_timeout_secs: 60,
            reconnect_interval_secs: 5,
            max_reconnect_attempts: 10,
        }
    }
}

/// Webhook 配置
#[derive(Debug, Clone)]
pub struct WebhookConfig {
    pub path: String,
    pub port: u16,
    pub verification_token: Option<String>,
    pub encrypt_key: Option<String>,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            path: String::new(),
            port: 8080,
            verification_token: None,
            encrypt_key: None,
        }
    }
}

/// Polling 配置
#[derive(Debug, Clone)]
pub struct PollingConfig {
    pub interval_secs: u64,
    pub timeout_secs: u64,
    pub retry_interval_secs: u64,
    pub max_retry_attempts: u32,
}

impl Default for PollingConfig {
    fn default() -> Self {
        Self {
            interval_secs: 1,
            timeout_secs: 30,
            retry_interval_secs: 5,
            max_retry_attempts: 10,
        }
    }
}
