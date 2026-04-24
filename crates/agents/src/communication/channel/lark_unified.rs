//! 飞书 (Lark) 统一通道实现
//!
//! 基于 beebot 的 Channel trait 设计，支持 WebSocket 和 Webhook 两种模式

use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::mpsc;
use tracing::{info, warn};

use super::lark_ws_impl::LarkWebSocketClient;
use super::r#trait::{BaseChannelConfig, ConnectionMode, ContentType};
use super::{Channel, ChannelConfig, ChannelEvent, ChannelInfo, MemberInfo};
use crate::communication::{Message, PlatformType};
use crate::error::{AgentError, Result};

/// 飞书配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkConfig {
    pub app_id: String,
    pub app_secret: String,
    pub encrypt_key: Option<String>,
    pub verification_token: Option<String>,
    #[serde(flatten)]
    pub base: BaseChannelConfig,
}

impl LarkConfig {
    /// 从环境变量创建配置
    pub fn from_env() -> Option<Self> {
        let app_id = std::env::var("LARK_APP_ID").ok()?;
        let app_secret = std::env::var("LARK_APP_SECRET").ok()?;
        let base = BaseChannelConfig::from_env("LARK")?;

        Some(Self {
            app_id,
            app_secret,
            encrypt_key: std::env::var("LARK_ENCRYPT_KEY").ok(),
            verification_token: std::env::var("LARK_VERIFICATION_TOKEN").ok(),
            base,
        })
    }
}

impl Default for LarkConfig {
    fn default() -> Self {
        Self {
            app_id: String::new(),
            app_secret: String::new(),
            encrypt_key: None,
            verification_token: None,
            base: BaseChannelConfig::default(),
        }
    }
}

impl ChannelConfig for LarkConfig {
    fn from_env() -> Option<Self>
    where
        Self: Sized,
    {
        Self::from_env()
    }

    fn is_valid(&self) -> bool {
        !self.app_id.is_empty() && !self.app_secret.is_empty()
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

/// 飞书统一通道
pub struct LarkChannel {
    config: LarkConfig,
    connected: AtomicBool,
    ws_client: Option<LarkWebSocketClient>,
    http_client: reqwest::Client,
}

impl LarkChannel {
    pub fn new(config: LarkConfig) -> Self {
        let http_client = reqwest::Client::new();

        // 根据连接模式创建相应的客户端
        let ws_client = if config.base.connection_mode == ConnectionMode::WebSocket {
            Some(LarkWebSocketClient::new(
                config.app_id.clone(),
                config.app_secret.clone(),
            ))
        } else {
            None
        };

        Self {
            config,
            connected: AtomicBool::new(false),
            ws_client,
            http_client,
        }
    }

    /// 获取 WebSocket 客户端（用于图片下载等操作）
    pub fn ws_client(&self) -> Option<&LarkWebSocketClient> {
        self.ws_client.as_ref()
    }

    /// 获取 tenant_access_token
    async fn get_access_token(&self) -> Result<String> {
        let url = format!("https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal");

        let response = self
            .http_client
            .post(&url)
            .json(&json!({
                "app_id": self.config.app_id,
                "app_secret": self.config.app_secret,
            }))
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("请求 token 失败: {}", e)))?;

        let data: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("解析 token 失败: {}", e)))?;

        if data.get("code").and_then(|c| c.as_i64()) != Some(0) {
            return Err(AgentError::platform(format!(
                "获取 token 失败: {:?}",
                data.get("msg")
            )));
        }

        data.get("tenant_access_token")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| AgentError::platform("无法获取 tenant_access_token".to_string()))
    }

    /// 发送文本消息
    async fn send_text(&self, receive_id_type: &str, receive_id: &str, text: &str) -> Result<()> {
        let token = self.get_access_token().await?;
        let url = format!(
            "https://open.feishu.cn/open-apis/im/v1/messages?receive_id_type={}",
            receive_id_type
        );

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&json!({
                "receive_id": receive_id,
                "msg_type": "text",
                "content": json!({"text": text}).to_string(),
            }))
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("发送消息失败: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AgentError::platform(format!(
                "发送消息失败: HTTP {} - {}",
                status, text
            )));
        }

        Ok(())
    }

    /// 解析接收者类型
    fn resolve_receiver_type<'a>(&self, channel: &'a str) -> (&'a str, &'a str) {
        if channel.starts_with("ou_") {
            ("open_id", channel)
        } else if channel.starts_with("oc_") {
            ("chat_id", channel)
        } else {
            ("open_id", channel)
        }
    }
}

#[async_trait]
impl Channel for LarkChannel {
    fn name(&self) -> &str {
        "lark"
    }

    fn platform(&self) -> PlatformType {
        PlatformType::Lark
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    async fn connect(&mut self) -> Result<()> {
        info!(
            "🚀 连接飞书 (模式: {})...",
            self.config.base.connection_mode
        );

        // 验证凭证
        let _token = self.get_access_token().await?;
        self.connected.store(true, Ordering::Relaxed);

        info!("✅ 飞书连接成功");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        if let Some(ref ws_client) = self.ws_client {
            ws_client.stop();
        }
        self.connected.store(false, Ordering::Relaxed);
        info!("飞书已断开连接");
        Ok(())
    }

    async fn send(&self, channel: &str, message: &Message) -> Result<()> {
        let (receive_id_type, receive_id) = self.resolve_receiver_type(channel);

        // Send message content as text
        self.send_text(receive_id_type, receive_id, &message.content)
            .await?;

        Ok(())
    }

    async fn start_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        match self.config.base.connection_mode {
            ConnectionMode::WebSocket => {
                info!("🎧 启动飞书 WebSocket 监听...");
                if let Some(ref ws_client) = self.ws_client {
                    ws_client.start(event_bus).await?;
                } else {
                    return Err(AgentError::platform("WebSocket 客户端未初始化".to_string()));
                }
            }
            ConnectionMode::Webhook => {
                info!("🎧 飞书 Webhook 模式，等待 HTTP 回调...");
                // Webhook 模式由外部 HTTP 服务器处理
                // 这里只需要保持运行状态
                loop {
                    if !self.is_connected() {
                        break;
                    }
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
            }
            ConnectionMode::Polling => {
                warn!("飞书不支持 Polling 模式");
                return Err(AgentError::platform("飞书不支持 Polling 模式".to_string()));
            }
        }

        Ok(())
    }

    async fn stop_listener(&self) -> Result<()> {
        if let Some(ref ws_client) = self.ws_client {
            ws_client.stop();
        }
        Ok(())
    }

    fn supported_content_types(&self) -> Vec<ContentType> {
        vec![
            ContentType::Text,
            ContentType::Image,
            ContentType::File,
            ContentType::Rich,
            ContentType::Card,
        ]
    }

    async fn list_channels(&self) -> Result<Vec<ChannelInfo>> {
        // TODO: 实现获取频道列表
        Ok(vec![])
    }

    async fn list_members(&self, _channel_id: &str) -> Result<Vec<MemberInfo>> {
        // TODO: 实现获取成员列表
        Ok(vec![])
    }

    fn connection_mode(&self) -> ConnectionMode {
        self.config.base.connection_mode
    }

    async fn download_image(&self, file_key: &str, message_id: Option<&str>) -> Result<Vec<u8>> {
        if let Some(ref ws_client) = self.ws_client {
            match message_id {
                Some(msg_id) => {
                    // Use message resource API for received messages
                    ws_client.download_image(msg_id, file_key).await
                }
                None => {
                    // Fallback: try to use the old images API (for backward compatibility)
                    // This won't work for received images due to Feishu API limitations
                    Err(AgentError::platform(
                        "Message ID required to download received images".to_string(),
                    ))
                }
            }
        } else {
            Err(AgentError::platform(
                "WebSocket 客户端未初始化，无法下载图片".to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lark_config_from_env() {
        // 测试默认连接模式 (websocket)
        std::env::set_var("LARK_APP_ID", "test_app_id");
        std::env::set_var("LARK_APP_SECRET", "test_app_secret");
        std::env::set_var("LARK_CONNECTION_MODE", "websocket");

        let config = LarkConfig::from_env();
        assert!(config.is_some());

        let config = config.unwrap();
        assert_eq!(config.app_id, "test_app_id");
        assert_eq!(config.app_secret, "test_app_secret");
        assert_eq!(config.base.connection_mode, ConnectionMode::WebSocket);
        assert!(config.base.auto_reconnect);
        assert_eq!(config.base.max_reconnect_attempts, 10);

        // 测试 webhook 连接模式
        std::env::set_var("LARK_CONNECTION_MODE", "webhook");
        let config = LarkConfig::from_env().unwrap();
        assert_eq!(config.base.connection_mode, ConnectionMode::Webhook);
    }
}

// ============================================================================
// Lark Channel Factory
// ============================================================================

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::RwLock;

use super::r#trait::ChannelFactory;

/// Lark channel factory
#[derive(Debug, Clone)]
pub struct LarkChannelFactory;

impl LarkChannelFactory {
    pub fn new() -> Self {
        Self
    }

    pub fn default_config() -> Value {
        json!({
            "app_id": "",
            "app_secret": "",
            "encrypt_key": null,
            "verification_token": null,
            "connection_mode": "websocket",
            "auto_reconnect": true,
            "max_reconnect_attempts": 10,
        })
    }
}

#[async_trait]
impl ChannelFactory for LarkChannelFactory {
    fn name(&self) -> &str {
        "lark"
    }

    fn platform_type(&self) -> super::PlatformType {
        super::PlatformType::Lark
    }

    async fn create(
        &self,
        config: &Value,
    ) -> crate::error::Result<Arc<RwLock<dyn super::Channel>>> {
        use crate::error::AgentError;

        info!("🔨 Creating Lark channel...");

        let app_id = config
            .get("app_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AgentError::platform("Lark app_id is required"))?
            .to_string();

        let app_secret = config
            .get("app_secret")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AgentError::platform("Lark app_secret is required"))?
            .to_string();

        let channel = LarkChannel::new(LarkConfig {
            app_id,
            app_secret,
            encrypt_key: config
                .get("encrypt_key")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            verification_token: config
                .get("verification_token")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            base: BaseChannelConfig::default(),
        });

        info!("✅ Lark channel created");
        Ok(Arc::new(RwLock::new(channel)))
    }

    fn validate_config(&self, config: &Value) -> bool {
        let has_app_id = config
            .get("app_id")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false);

        let has_app_secret = config
            .get("app_secret")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false);

        has_app_id && has_app_secret
    }

    fn default_config(&self) -> Value {
        Self::default_config()
    }
}
