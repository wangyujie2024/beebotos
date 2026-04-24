//! 浏览器自动化模块
//!
//! 提供 Chrome DevTools Protocol (CDP) 支持，包括：
//! - 浏览器连接管理
//! - 自动化批处理操作
//! - 并发安全沙箱
//! - 调试工具
//!
//! 兼容 OpenClaw V2026.3.13 的浏览器自动化功能

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub mod automation;
pub mod cdp;
pub mod debugger;
pub mod sandbox;

pub use automation::{BatchOperation, BrowserAction, BrowserAutomation, SelectorChain};
pub use cdp::{CdpClient, CdpConnection};
pub use debugger::{
    sanitize_for_log, // 安全规范 10.1
    BrowserDebugger,
    BrowserLogEntry,
    DebugReport,
    DebugStatistics,
    ErrorSummary,
    LogLevel,
    LogMetadata,
    LogSource,
    PerformanceMetrics, // 规范 9.2
    SessionInfo,
};
pub use sandbox::{BrowserSandbox, ResourceLimits, SandboxManager};

/// 浏览器配置文件
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BrowserProfile {
    pub id: String,
    pub name: String,
    pub cdp_port: u16,
    pub color: String,
    #[serde(default)]
    pub profile_type: ProfileType,
    #[serde(default)]
    pub is_default: bool,
}

impl Default for BrowserProfile {
    fn default() -> Self {
        Self {
            id: "default".to_string(),
            name: "Default Profile".to_string(),
            cdp_port: 9222,
            color: "#0066CC".to_string(),
            profile_type: ProfileType::User,
            is_default: true,
        }
    }
}

impl BrowserProfile {
    pub fn new(name: impl Into<String>, cdp_port: u16) -> Self {
        let name = name.into();
        let id = name.to_lowercase().replace(" ", "-");
        Self {
            id,
            name,
            cdp_port,
            color: "#0066CC".to_string(),
            profile_type: ProfileType::User,
            is_default: false,
        }
    }

    pub fn with_color(mut self, color: impl Into<String>) -> Self {
        self.color = color.into();
        self
    }

    pub fn with_type(mut self, profile_type: ProfileType) -> Self {
        self.profile_type = profile_type;
        self
    }
}

/// 浏览器 Profile 类型
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum ProfileType {
    /// 使用已登录的主浏览器
    User,
    /// 扩展中继模式
    ChromeRelay,
    /// 隔离模式
    Isolated,
}

impl Default for ProfileType {
    fn default() -> Self {
        ProfileType::User
    }
}

/// 浏览器连接状态
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

impl Default for ConnectionStatus {
    fn default() -> Self {
        ConnectionStatus::Disconnected
    }
}

/// 浏览器实例信息
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrowserInstance {
    pub id: String,
    pub profile_id: String,
    pub status: ConnectionStatus,
    pub current_url: Option<String>,
    #[serde(default)]
    pub page_title: Option<String>,
    #[serde(default)]
    pub connected_at: Option<String>,
    #[serde(default)]
    pub last_activity: Option<String>,
}

/// 浏览器状态
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrowserStatus {
    pub enabled: bool,
    #[serde(default)]
    pub connected: bool,
    #[serde(default)]
    pub instance_count: usize,
    #[serde(default)]
    pub profiles_count: i64,
    #[serde(default)]
    pub active_instances: i64,
    #[serde(default)]
    pub profiles: Vec<BrowserProfileStatus>,
}

/// 浏览器配置状态
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrowserProfileStatus {
    pub profile_id: String,
    pub name: String,
    pub status: ConnectionStatus,
    pub url: Option<String>,
    pub title: Option<String>,
}

/// 选择器类型
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Selector {
    /// CSS 选择器
    Css(String),
    /// XPath 选择器
    XPath(String),
    /// 文本内容
    Text(String),
    /// ARIA 属性
    Aria { role: String, name: Option<String> },
    /// ID
    Id(String),
    /// Class
    Class(String),
}

impl Selector {
    pub fn css(selector: impl Into<String>) -> Self {
        Selector::Css(selector.into())
    }

    pub fn xpath(selector: impl Into<String>) -> Self {
        Selector::XPath(selector.into())
    }

    pub fn text(text: impl Into<String>) -> Self {
        Selector::Text(text.into())
    }

    pub fn id(id: impl Into<String>) -> Self {
        Selector::Id(id.into())
    }

    /// 转换为 CDP 可用的选择器格式
    pub fn to_cdp_selector(&self) -> String {
        match self {
            Selector::Css(s) => s.clone(),
            Selector::XPath(s) => format!("xpath={}", s),
            Selector::Text(s) => format!("text={}", s),
            Selector::Aria { role, name } => {
                if let Some(name) = name {
                    format!("aria/{}[name=\"{}\"]", role, name)
                } else {
                    format!("aria/{}", role)
                }
            }
            Selector::Id(s) => format!("# {}", s),
            Selector::Class(s) => format!(".{}", s),
        }
    }
}

/// 回退策略
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum FallbackStrategy {
    /// 按顺序尝试，直到成功
    Sequential,
    /// 所有选择器必须匹配
    All,
    /// 任意一个匹配即可
    Any,
}

impl Default for FallbackStrategy {
    fn default() -> Self {
        FallbackStrategy::Sequential
    }
}

/// 等待条件
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "condition", rename_all = "snake_case")]
pub enum WaitCondition {
    /// 元素可见
    ElementVisible { selector: Selector },
    /// 元素存在
    ElementPresent { selector: Selector },
    /// 元素可点击
    ElementClickable { selector: Selector },
    /// 导航完成
    NavigationComplete,
    /// 网络空闲
    NetworkIdle,
    /// 固定延迟
    FixedDelay { milliseconds: u64 },
}

/// 导航等待条件
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum NavigationWait {
    Load,
    DOMContentLoaded,
    NetworkIdle,
    Complete,
}

/// 批处理结果
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchResult {
    pub success: bool,
    pub completed_actions: usize,
    pub failed_actions: usize,
    pub results: Vec<ActionResult>,
    pub execution_time_ms: u64,
}

/// 单个动作结果
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActionResult {
    pub action_index: usize,
    pub success: bool,
    pub action_type: String,
    pub error: Option<String>,
    pub screenshot_url: Option<String>,
    pub data: Option<serde_json::Value>,
}

/// 浏览器错误类型
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrowserErrorType {
    ElementNotFound {
        attempted_selectors: Vec<String>,
    },
    Timeout {
        waited_for_ms: u64,
    },
    NavigationFailed {
        url: String,
        reason: String,
    },
    JavaScriptError {
        message: String,
        stack: Option<String>,
    },
    ConnectionLost,
    InvalidProfile,
    SandboxError {
        code: String,
    },
}

/// 浏览器错误
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrowserError {
    pub error_type: BrowserErrorType,
    pub message: String,
    pub current_url: Option<String>,
    pub screenshot_path: Option<String>,
    pub suggestions: Vec<String>,
}

impl BrowserError {
    pub fn element_not_found(selectors: Vec<String>, url: Option<String>) -> Self {
        let suggestions = vec![
            "检查选择器语法是否正确".to_string(),
            "确认元素是否已加载".to_string(),
            "尝试使用不同的选择器类型".to_string(),
        ];
        Self {
            error_type: BrowserErrorType::ElementNotFound {
                attempted_selectors: selectors.clone(),
            },
            message: format!("元素未找到，尝试的选择器: {:?}", selectors),
            current_url: url,
            screenshot_path: None,
            suggestions,
        }
    }

    pub fn timeout(waited_ms: u64, condition: &str) -> Self {
        Self {
            error_type: BrowserErrorType::Timeout {
                waited_for_ms: waited_ms,
            },
            message: format!("等待条件超时: {}，等待了 {}ms", condition, waited_ms),
            current_url: None,
            screenshot_path: None,
            suggestions: vec![
                "增加超时时间".to_string(),
                "检查网络连接".to_string(),
                "确认条件是否可以达成".to_string(),
            ],
        }
    }
}

/// 截图结果
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScreenshotResult {
    pub data: String, // base64 encoded
    pub format: ScreenshotFormat,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum ScreenshotFormat {
    Png,
    Jpeg,
    Webp,
}

/// 浏览器事件
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum BrowserEvent {
    PageLoad { url: String, title: String },
    ElementClick { selector: String },
    ConsoleLog { level: String, message: String },
    Navigation { from: String, to: String },
    Screenshot { path: String },
    Error { error: BrowserError },
}

/// 浏览器配置
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrowserConfig {
    pub profiles: HashMap<String, BrowserProfile>,
    pub sandbox: SandboxConfig,
    pub debugger: DebuggerConfig,
    /// Docker 时区配置（规范 7.1 要求）
    pub docker: DockerConfig,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        let mut profiles = HashMap::new();
        let default_profile = BrowserProfile::default();
        profiles.insert(default_profile.id.clone(), default_profile);

        Self {
            profiles,
            sandbox: SandboxConfig::default(),
            debugger: DebuggerConfig::default(),
            docker: DockerConfig::default(),
        }
    }
}

/// Docker 配置（规范 7.1 要求）
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DockerConfig {
    /// 时区设置
    pub timezone: String,
    /// 容器镜像
    pub image: String,
    /// 是否自动重启
    pub auto_restart: bool,
}

impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            timezone: "Asia/Shanghai".to_string(),
            image: "beebotos/browser:latest".to_string(),
            auto_restart: true,
        }
    }
}

/// 沙箱安全策略（规范 10.2 要求）
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SandboxSecurityPolicy {
    /// 网络隔离
    pub network_isolation: bool,
    /// 文件系统隔离
    pub filesystem_isolation: bool,
    /// 进程隔离
    pub process_isolation: bool,
    /// 允许的源
    pub allowed_origins: Vec<String>,
    /// 阻止的 URL
    pub blocked_urls: Vec<String>,
}

impl Default for SandboxSecurityPolicy {
    fn default() -> Self {
        Self {
            network_isolation: true,
            filesystem_isolation: true,
            process_isolation: true,
            allowed_origins: vec!["*".to_string()],
            blocked_urls: vec![],
        }
    }
}

/// 沙箱配置
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SandboxConfig {
    pub enabled: bool,
    pub memory_limit_mb: u32,
    pub cpu_limit_percent: f32,
    pub auto_cleanup: bool,
    pub max_age_seconds: u64,
    pub max_idle_time_seconds: u64,
    /// 安全策略（规范 10.2）
    pub security_policy: SandboxSecurityPolicy,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            memory_limit_mb: 512,
            cpu_limit_percent: 0.5,
            auto_cleanup: true,
            max_age_seconds: 3600,
            max_idle_time_seconds: 300,
            security_policy: SandboxSecurityPolicy::default(),
        }
    }
}

/// 调试配置
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DebuggerConfig {
    pub enabled: bool,
    pub auto_screenshot: bool,
    pub screenshot_before_after: bool,
    pub log_level: LogLevel,
}

impl Default for DebuggerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_screenshot: true,
            screenshot_before_after: true,
            log_level: LogLevel::Info,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_profile_creation() {
        let profile = BrowserProfile::new("Work Profile", 18801)
            .with_color("#FF0000")
            .with_type(ProfileType::Isolated);

        assert_eq!(profile.name, "Work Profile");
        assert_eq!(profile.cdp_port, 18801);
        assert_eq!(profile.color, "#FF0000");
        assert_eq!(profile.profile_type, ProfileType::Isolated);
    }

    #[test]
    fn test_selector_creation() {
        let css = Selector::css("#button");
        assert_eq!(css.to_cdp_selector(), "#button");

        let xpath = Selector::xpath("//div[@class='item']");
        assert_eq!(xpath.to_cdp_selector(), "xpath=//div[@class='item']");

        let text = Selector::text("Click me");
        assert_eq!(text.to_cdp_selector(), "text=Click me");
    }

    #[test]
    fn test_browser_error_creation() {
        let error = BrowserError::element_not_found(
            vec!["#btn".to_string(), "//button".to_string()],
            Some("https://example.com".to_string()),
        );

        match error.error_type {
            BrowserErrorType::ElementNotFound {
                attempted_selectors,
            } => {
                assert_eq!(attempted_selectors.len(), 2);
            }
            _ => panic!("Expected ElementNotFound error"),
        }
    }
}
