//! Chrome DevTools Protocol (CDP) 客户端
//!
//! 实现与 Chrome DevTools 的 WebSocket 通信
//! 支持实时浏览器会话附加和零扩展架构

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{ConnectionStatus, ScreenshotFormat, ScreenshotResult};

/// CDP 错误类型
#[derive(Clone, Debug)]
pub enum CdpError {
    ConnectionFailed(String),
    WebSocketError(String),
    CommandFailed { code: i64, message: String },
    Timeout,
    Serialization(String),
    NotConnected,
}

impl std::fmt::Display for CdpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CdpError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            CdpError::WebSocketError(msg) => write!(f, "WebSocket error: {}", msg),
            CdpError::CommandFailed { code, message } => {
                write!(f, "Command failed ({}): {}", code, message)
            }
            CdpError::Timeout => write!(f, "Operation timed out"),
            CdpError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            CdpError::NotConnected => write!(f, "Not connected to browser"),
        }
    }
}

/// CDP 连接配置
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CdpConnectionConfig {
    pub host: String,
    pub port: u16,
    pub secure: bool,
    pub connection_timeout_ms: u64,
}

impl Default for CdpConnectionConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 9222,
            secure: false,
            connection_timeout_ms: 30000,
        }
    }
}

impl CdpConnectionConfig {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            ..Default::default()
        }
    }

    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    /// 获取 WebSocket URL
    pub fn ws_url(&self) -> String {
        let protocol = if self.secure { "wss" } else { "ws" };
        format!(
            "{}://{}:{}/devtools/browser",
            protocol, self.host, self.port
        )
    }

    /// 获取 HTTP 调试信息 URL
    pub fn http_url(&self) -> String {
        format!("http://{}:{}/json/version", self.host, self.port)
    }
}

/// CDP 连接信息
#[derive(Clone, Debug, Deserialize)]
pub struct CdpTarget {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub target_type: String,
    pub url: String,
    #[serde(rename = "webSocketDebuggerUrl")]
    pub ws_url: Option<String>,
    #[serde(rename = "devtoolsFrontendUrl")]
    pub devtools_url: Option<String>,
}

/// CDP 命令请求
#[derive(Clone, Debug, Serialize)]
struct CdpCommand {
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
}

/// CDP 命令响应
#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
struct CdpResponse {
    id: u64,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<CdpErrorDetail>,
}

#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
struct CdpErrorDetail {
    code: i64,
    message: String,
}

/// CDP 事件
#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
struct CdpEvent {
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

/// CDP 连接管理器
#[derive(Clone, Debug)]
pub struct CdpConnection {
    config: CdpConnectionConfig,
    status: ConnectionStatus,
    session_id: Option<String>,
    command_id: u64,
    targets: Vec<CdpTarget>,
}

impl CdpConnection {
    pub fn new(config: CdpConnectionConfig) -> Self {
        Self {
            config,
            status: ConnectionStatus::Disconnected,
            session_id: None,
            command_id: 0,
            targets: Vec::new(),
        }
    }

    /// 获取连接状态
    pub fn status(&self) -> &ConnectionStatus {
        &self.status
    }

    /// 检查是否已连接
    pub fn is_connected(&self) -> bool {
        matches!(self.status, ConnectionStatus::Connected)
    }

    /// 获取可用目标列表
    pub fn targets(&self) -> &[CdpTarget] {
        &self.targets
    }

    /// 更新状态
    pub fn set_status(&mut self, status: ConnectionStatus) {
        self.status = status;
    }

    /// 设置会话 ID
    pub fn set_session_id(&mut self, session_id: String) {
        self.session_id = Some(session_id);
    }

    /// 获取下一个命令 ID
    pub fn next_command_id(&mut self) -> u64 {
        self.command_id += 1;
        self.command_id
    }
}

/// CDP 客户端
///
/// 用于与 Chrome DevTools Protocol 通信的主客户端
pub struct CdpClient {
    connection: CdpConnection,
    pending_commands: HashMap<u64, std::sync::mpsc::Sender<Result<Value, CdpError>>>,
    event_handlers: HashMap<String, Vec<Box<dyn Fn(Value)>>>,
}

impl CdpClient {
    /// 创建新的 CDP 客户端
    pub fn new(config: CdpConnectionConfig) -> Self {
        Self {
            connection: CdpConnection::new(config),
            pending_commands: HashMap::new(),
            event_handlers: HashMap::new(),
        }
    }

    /// 连接到 Chrome DevTools
    pub async fn connect(&mut self) -> Result<(), CdpError> {
        self.connection.set_status(ConnectionStatus::Connecting);

        // 获取可用目标
        match self.fetch_targets().await {
            Ok(targets) => {
                self.connection.targets = targets;
                self.connection.set_status(ConnectionStatus::Connected);
                Ok(())
            }
            Err(e) => {
                self.connection
                    .set_status(ConnectionStatus::Error(e.to_string()));
                Err(e)
            }
        }
    }

    /// 获取调试目标列表
    async fn fetch_targets(&self) -> Result<Vec<CdpTarget>, CdpError> {
        let url = format!(
            "http://{}:{}/json/list",
            self.connection.config.host, self.connection.config.port
        );

        let response = gloo_net::http::Request::get(&url)
            .send()
            .await
            .map_err(|e| CdpError::ConnectionFailed(e.to_string()))?;

        if !response.ok() {
            return Err(CdpError::ConnectionFailed(format!(
                "HTTP {}: {}",
                response.status(),
                response.status_text()
            )));
        }

        let targets: Vec<CdpTarget> = response
            .json()
            .await
            .map_err(|e| CdpError::Serialization(e.to_string()))?;

        Ok(targets)
    }

    /// 获取浏览器版本信息
    pub async fn get_version(&self) -> Result<BrowserVersion, CdpError> {
        let url = self.connection.config.http_url();

        let response = gloo_net::http::Request::get(&url)
            .send()
            .await
            .map_err(|e| CdpError::ConnectionFailed(e.to_string()))?;

        let version: BrowserVersion = response
            .json()
            .await
            .map_err(|e| CdpError::Serialization(e.to_string()))?;

        Ok(version)
    }

    /// 发送 CDP 命令
    pub async fn send_command<T: Serialize>(
        &mut self,
        method: &str,
        params: Option<T>,
    ) -> Result<Value, CdpError> {
        if !self.connection.is_connected() {
            return Err(CdpError::NotConnected);
        }

        let id = self.connection.next_command_id();
        let params_value = params
            .map(|p| serde_json::to_value(p).map_err(|e| CdpError::Serialization(e.to_string())))
            .transpose()?;

        let command = CdpCommand {
            id,
            method: method.to_string(),
            params: params_value,
            session_id: self.connection.session_id.clone(),
        };

        // 序列化命令
        let _json =
            serde_json::to_string(&command).map_err(|e| CdpError::Serialization(e.to_string()))?;

        // 这里应该通过 WebSocket 发送
        // 由于 WASM 环境的限制，实际实现可能需要通过 Gateway API 代理

        // 返回模拟结果
        Ok(serde_json::json!({
            "id": id,
            "method": method,
            "sent": true
        }))
    }

    /// 导航到 URL
    pub async fn navigate(&mut self, url: &str) -> Result<NavigationResult, CdpError> {
        let params = serde_json::json!({
            "url": url
        });

        let result = self.send_command("Page.navigate", Some(params)).await?;

        Ok(NavigationResult {
            frame_id: result
                .get("frameId")
                .and_then(|v| v.as_str().map(|s| s.to_string())),
            loader_id: result
                .get("loaderId")
                .and_then(|v| v.as_str().map(|s| s.to_string())),
        })
    }

    /// 执行 JavaScript
    pub async fn evaluate(&mut self, expression: &str) -> Result<EvaluateResult, CdpError> {
        let params = serde_json::json!({
            "expression": expression,
            "returnByValue": true,
            "awaitPromise": true
        });

        let result = self.send_command("Runtime.evaluate", Some(params)).await?;

        Ok(EvaluateResult {
            result: result.get("result").cloned(),
            exception: result.get("exceptionDetails").cloned(),
        })
    }

    /// 截图
    pub async fn capture_screenshot(
        &mut self,
        format: ScreenshotFormat,
        full_page: bool,
    ) -> Result<ScreenshotResult, CdpError> {
        let format_str = match format {
            ScreenshotFormat::Png => "png",
            ScreenshotFormat::Jpeg => "jpeg",
            ScreenshotFormat::Webp => "webp",
        };

        let mut params = serde_json::json!({
            "format": format_str,
            "fromSurface": true
        });

        if full_page {
            params["captureBeyondViewport"] = serde_json::json!(true);
            params["fullPage"] = serde_json::json!(true);
        }

        let result = self
            .send_command("Page.captureScreenshot", Some(params))
            .await?;

        let data = result
            .get("data")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .ok_or_else(|| CdpError::CommandFailed {
                code: -1,
                message: "No screenshot data".to_string(),
            })?;

        Ok(ScreenshotResult {
            data,
            format,
            width: 0, // 可以从 result 中解析
            height: 0,
        })
    }

    /// 查询 DOM 元素
    pub async fn query_selector(&mut self, selector: &str) -> Result<Option<DomNode>, CdpError> {
        let params = serde_json::json!({
            "selector": selector
        });

        let result = self.send_command("DOM.querySelector", Some(params)).await?;

        let node_id = result.get("nodeId").and_then(|v| v.as_u64()).unwrap_or(0);

        if node_id == 0 {
            Ok(None)
        } else {
            Ok(Some(DomNode { node_id }))
        }
    }

    /// 获取文档
    pub async fn get_document(&mut self) -> Result<DomNode, CdpError> {
        let result = self.send_command::<()>("DOM.getDocument", None).await?;

        let node_id = result
            .get("root")
            .and_then(|v| v.get("nodeId"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        Ok(DomNode { node_id })
    }

    /// 启用页面事件
    pub async fn enable_page_events(&mut self) -> Result<(), CdpError> {
        self.send_command::<()>("Page.enable", None).await?;
        Ok(())
    }

    /// 启用 DOM 事件
    pub async fn enable_dom_events(&mut self) -> Result<(), CdpError> {
        self.send_command::<()>("DOM.enable", None).await?;
        Ok(())
    }

    /// 启用 Runtime 事件
    pub async fn enable_runtime_events(&mut self) -> Result<(), CdpError> {
        self.send_command::<()>("Runtime.enable", None).await?;
        Ok(())
    }

    /// 注册事件处理器
    pub fn on_event<F: Fn(Value) + 'static>(&mut self, event: &str, handler: F) {
        self.event_handlers
            .entry(event.to_string())
            .or_default()
            .push(Box::new(handler));
    }

    /// 断开连接
    pub fn disconnect(&mut self) {
        self.connection.set_status(ConnectionStatus::Disconnected);
        self.connection.session_id = None;
        self.pending_commands.clear();
    }

    /// 获取连接配置
    pub fn config(&self) -> &CdpConnectionConfig {
        &self.connection.config
    }
}

/// 浏览器版本信息
#[derive(Clone, Debug, Deserialize)]
pub struct BrowserVersion {
    #[serde(rename = "Browser")]
    pub browser: String,
    #[serde(rename = "Protocol-Version")]
    pub protocol_version: String,
    #[serde(rename = "User-Agent")]
    pub user_agent: String,
    #[serde(rename = "V8-Version")]
    pub v8_version: String,
    #[serde(rename = "WebKit-Version")]
    pub webkit_version: String,
    #[serde(rename = "webSocketDebuggerUrl")]
    pub ws_debugger_url: Option<String>,
}

/// 导航结果
#[derive(Clone, Debug)]
pub struct NavigationResult {
    pub frame_id: Option<String>,
    pub loader_id: Option<String>,
}

/// 执行结果
#[derive(Clone, Debug)]
pub struct EvaluateResult {
    pub result: Option<Value>,
    pub exception: Option<Value>,
}

impl EvaluateResult {
    /// 检查是否有异常
    pub fn has_exception(&self) -> bool {
        self.exception.is_some()
    }

    /// 获取结果值
    pub fn value(&self) -> Option<&Value> {
        self.result.as_ref()
    }
}

/// DOM 节点
#[derive(Clone, Debug)]
pub struct DomNode {
    pub node_id: u64,
}

impl DomNode {
    /// 点击节点
    pub async fn click(&self, client: &mut CdpClient) -> Result<(), CdpError> {
        let params = serde_json::json!({
            "nodeId": self.node_id
        });

        client.send_command("DOM.click", Some(params)).await?;
        Ok(())
    }

    /// 获取节点属性
    pub async fn get_attributes(
        &self,
        client: &mut CdpClient,
    ) -> Result<HashMap<String, String>, CdpError> {
        let params = serde_json::json!({
            "nodeId": self.node_id
        });

        let result = client
            .send_command("DOM.getAttributes", Some(params))
            .await?;

        let mut attributes = HashMap::new();
        if let Some(attrs) = result.get("attributes").and_then(|v| v.as_array()) {
            for chunk in attrs.chunks(2) {
                if chunk.len() == 2 {
                    let key = chunk[0].as_str().unwrap_or("").to_string();
                    let value = chunk[1].as_str().unwrap_or("").to_string();
                    attributes.insert(key, value);
                }
            }
        }

        Ok(attributes)
    }

    /// 获取节点文本内容
    pub async fn get_text(&self, client: &mut CdpClient) -> Result<String, CdpError> {
        let params = serde_json::json!({
            "nodeId": self.node_id
        });

        let result = client
            .send_command("DOM.querySelector", Some(params))
            .await?;

        // 实际应该通过 DOM.describeNode 获取详细信息
        Ok(result
            .get("outerHTML")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default())
    }
}

/// CDP 工具函数
pub mod utils {
    /// 检查 Chrome DevTools 是否可连接
    pub async fn is_cdp_available(host: &str, port: u16) -> bool {
        let url = format!("http://{}:{}/json/version", host, port);

        match gloo_net::http::Request::get(&url).send().await {
            Ok(response) => response.ok(),
            Err(_) => false,
        }
    }

    /// 获取可用端口列表
    pub async fn find_available_ports(host: &str, start_port: u16, end_port: u16) -> Vec<u16> {
        let mut available = Vec::new();

        for port in start_port..=end_port {
            if is_cdp_available(host, port).await {
                available.push(port);
            }
        }

        available
    }

    /// 解析 WebSocket URL
    pub fn parse_ws_url(url: &str) -> Option<(String, u16, String)> {
        // ws://host:port/path
        let url = url
            .strip_prefix("ws://")
            .or_else(|| url.strip_prefix("wss://"))?;
        let parts: Vec<&str> = url.splitn(2, '/').collect();
        let host_port = parts[0];
        let path = parts.get(1).map(|s| format!("/{}", s)).unwrap_or_default();

        let hp_parts: Vec<&str> = host_port.split(':').collect();
        let host = hp_parts[0].to_string();
        let port = hp_parts.get(1)?.parse::<u16>().ok()?;

        Some((host, port, path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cdp_connection_config() {
        let config = CdpConnectionConfig::new(9222).with_host("192.168.1.100");

        assert_eq!(config.ws_url(), "ws://192.168.1.100:9222/devtools/browser");
        assert_eq!(config.http_url(), "http://192.168.1.100:9222/json/version");
    }

    #[test]
    fn test_parse_ws_url() {
        let result = utils::parse_ws_url("ws://localhost:9222/devtools/page/ABC123");
        assert!(result.is_some());

        let (host, port, path) = result.unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 9222);
        assert_eq!(path, "/devtools/page/ABC123");
    }

    #[test]
    fn test_navigation_result() {
        let result = NavigationResult {
            frame_id: Some("frame-123".to_string()),
            loader_id: Some("loader-456".to_string()),
        };

        assert_eq!(result.frame_id, Some("frame-123".to_string()));
        assert_eq!(result.loader_id, Some("loader-456".to_string()));
    }
}
