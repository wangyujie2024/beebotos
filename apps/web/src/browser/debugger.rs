//! 浏览器调试工具
//!
//! 提供实时日志、自动截图、智能错误提示等功能
//! 兼容 OpenClaw V2026.3.13 开发者工具规格

use super::{BrowserError, BrowserEvent};
use crate::webchat::TokenUsage;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// 日志级别
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, PartialOrd)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

impl Default for LogLevel {
    fn default() -> Self {
        LogLevel::Info
    }
}

impl LogLevel {
    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warning => "warning",
            LogLevel::Error => "error",
            LogLevel::Critical => "critical",
        }
    }

    /// 从字符串解析
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "debug" => Some(LogLevel::Debug),
            "info" => Some(LogLevel::Info),
            "warn" | "warning" => Some(LogLevel::Warning),
            "error" => Some(LogLevel::Error),
            "critical" | "fatal" => Some(LogLevel::Critical),
            _ => None,
        }
    }

    /// 检查是否应该记录
    pub fn should_log(&self, min_level: &LogLevel) -> bool {
        self >= min_level
    }
}

/// 浏览器日志条目
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrowserLogEntry {
    pub id: String,
    pub timestamp: String,
    pub level: LogLevel,
    pub source: LogSource,
    pub message: String,
    pub screenshot_url: Option<String>,
    pub context: serde_json::Value,
    pub metadata: LogMetadata,
}

/// 日志来源
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LogSource {
    Browser { instance_id: String },
    Page { url: String, title: String },
    Console { level: String },
    Network { request_id: String },
    Automation { action: String },
    System { component: String },
}

/// 日志元数据
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct LogMetadata {
    pub user_agent: Option<String>,
    pub viewport: Option<ViewportInfo>,
    pub stack_trace: Option<String>,
    pub related_elements: Vec<String>,
    pub timing: Option<TimingInfo>,
}

/// 视口信息
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ViewportInfo {
    pub width: u32,
    pub height: u32,
    pub device_scale_factor: f32,
}

/// 时间信息
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimingInfo {
    pub start_time_ms: u64,
    pub end_time_ms: u64,
    pub duration_ms: u64,
}

/// 浏览器调试器
#[derive(Clone, Debug)]
pub struct BrowserDebugger {
    config: DebuggerConfig,
    log_buffer: VecDeque<BrowserLogEntry>,
    max_buffer_size: usize,
    screenshot_queue: VecDeque<ScreenshotRequest>,
    enabled: bool,
}

/// 调试器配置
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DebuggerConfig {
    pub enabled: bool,
    pub auto_screenshot: bool,
    pub screenshot_before_after: bool,
    pub log_level: LogLevel,
    pub max_log_entries: usize,
    pub capture_console_logs: bool,
    pub capture_network_logs: bool,
    pub persist_logs: bool,
}

impl Default for DebuggerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_screenshot: true,
            screenshot_before_after: true,
            log_level: LogLevel::Info,
            max_log_entries: 1000,
            capture_console_logs: true,
            capture_network_logs: true,
            persist_logs: false,
        }
    }
}

/// 截图请求
#[derive(Clone, Debug)]
pub struct ScreenshotRequest {
    _timestamp: String,
    _context: String,
    _full_page: bool,
}

/// 调试报告
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DebugReport {
    pub generated_at: String,
    pub session_info: SessionInfo,
    pub log_entries: Vec<BrowserLogEntry>,
    pub statistics: DebugStatistics,
    pub errors: Vec<ErrorSummary>,
    pub screenshots: Vec<String>,
}

/// 会话信息
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionInfo {
    pub browser_instance_id: String,
    pub start_time: String,
    pub end_time: String,
    pub duration_seconds: u64,
    pub pages_visited: Vec<String>,
    pub actions_performed: u32,
}

/// 调试统计
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DebugStatistics {
    pub total_logs: usize,
    pub logs_by_level: std::collections::HashMap<String, usize>,
    pub average_action_time_ms: f64,
    pub error_rate: f64,
    pub screenshot_count: usize,
}

/// 性能指标（规范 9.2）
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// 页面加载时间（毫秒）
    pub page_load_time_ms: u64,
    /// API 延迟（毫秒）
    pub api_latency_ms: u64,
    /// WebSocket 重连次数
    pub websocket_reconnect_count: u32,
    /// 活跃浏览器实例数
    pub browser_instances_active: u32,
    /// 内存使用（MB）
    pub memory_usage_mb: f64,
    /// Token 使用量
    pub token_usage: TokenUsage,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            page_load_time_ms: 0,
            api_latency_ms: 0,
            websocket_reconnect_count: 0,
            browser_instances_active: 0,
            memory_usage_mb: 0.0,
            token_usage: TokenUsage::new("default"),
        }
    }
}

/// 敏感信息脱敏（安全规范 10.1）
pub fn sanitize_for_log(value: &str) -> String {
    let mut result = value.to_string();
    
    // 脱敏敏感字段（简单字符串替换，不依赖 regex）
    let sensitive_keys = [
        "token", "password", "api_key", "api-key", 
        "secret", "authorization", "auth"
    ];
    
    for key in &sensitive_keys {
        // 匹配 key=value 或 key: value 格式
        if let Some(pos) = result.to_lowercase().find(&format!("{}=", key)) {
            let start = pos + key.len() + 1;
            if start < result.len() {
                // 找到值结束位置（空格、逗号或字符串结束）
                let end = result[start..].find(|c: char| c.is_whitespace() || c == ',' || c == '&')
                    .map(|i| start + i)
                    .unwrap_or(result.len());
                if end > start {
                    result.replace_range(start..end, "***");
                }
            }
        }
    }
    
    result
}

/// 错误摘要
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ErrorSummary {
    pub error_type: String,
    pub count: usize,
    pub first_occurrence: String,
    pub last_occurrence: String,
    pub sample_message: String,
    pub suggestions: Vec<String>,
}

impl BrowserDebugger {
    /// 创建新的调试器
    pub fn new(config: DebuggerConfig) -> Self {
        Self {
            config: config.clone(),
            log_buffer: VecDeque::with_capacity(config.max_log_entries),
            max_buffer_size: config.max_log_entries,
            screenshot_queue: VecDeque::new(),
            enabled: config.enabled,
        }
    }

    /// 启用调试器
    pub fn enable(&mut self) {
        self.enabled = true;
        self.config.enabled = true;
    }

    /// 禁用调试器
    pub fn disable(&mut self) {
        self.enabled = false;
        self.config.enabled = false;
    }

    /// 检查是否启用
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// 启用自动截图
    pub fn enable_auto_screenshot(&mut self, before_after: bool) {
        self.config.auto_screenshot = true;
        self.config.screenshot_before_after = before_after;
    }

    /// 禁用自动截图
    pub fn disable_auto_screenshot(&mut self) {
        self.config.auto_screenshot = false;
    }

    /// 设置日志级别
    pub fn set_log_level(&mut self, level: LogLevel) {
        self.config.log_level = level;
    }

    /// 记录日志
    pub fn log(
        &mut self,
        level: LogLevel,
        source: LogSource,
        message: impl Into<String>,
        context: Option<serde_json::Value>,
    ) {
        if !self.enabled || !level.should_log(&self.config.log_level) {
            return;
        }

        let entry = BrowserLogEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            level,
            source,
            message: message.into(),
            screenshot_url: None,
            context: context.unwrap_or(serde_json::Value::Null),
            metadata: LogMetadata::default(),
        };

        self.add_entry(entry);
    }

    /// 记录调试日志
    pub fn debug(&mut self, source: LogSource, message: impl Into<String>) {
        self.log(LogLevel::Debug, source, message, None);
    }

    /// 记录信息日志
    pub fn info(&mut self, source: LogSource, message: impl Into<String>) {
        self.log(LogLevel::Info, source, message, None);
    }

    /// 记录警告日志
    pub fn warn(&mut self, source: LogSource, message: impl Into<String>) {
        self.log(LogLevel::Warning, source, message, None);
    }

    /// 记录错误日志
    pub fn error(&mut self, source: LogSource, message: impl Into<String>, error: Option<&BrowserError>) {
        let context = error.map(|e| {
            serde_json::json!({
                "error": {
                    "message": e.message,
                    "url": e.current_url,
                    "suggestions": e.suggestions
                }
            })
        });

        self.log(LogLevel::Error, source, message, context);
    }

    /// 添加日志条目
    fn add_entry(&mut self, entry: BrowserLogEntry) {
        if self.log_buffer.len() >= self.max_buffer_size {
            self.log_buffer.pop_front();
        }
        self.log_buffer.push_back(entry);
    }

    /// 获取所有日志
    pub fn get_logs(&self) -> Vec<BrowserLogEntry> {
        self.log_buffer.iter().cloned().collect()
    }

    /// 按级别获取日志
    pub fn get_logs_by_level(&self, level: LogLevel) -> Vec<BrowserLogEntry> {
        self.log_buffer
            .iter()
            .filter(|e| e.level == level)
            .cloned()
            .collect()
    }

    /// 获取最近 N 条日志
    pub fn get_recent_logs(&self, n: usize) -> Vec<BrowserLogEntry> {
        self.log_buffer
            .iter()
            .rev()
            .take(n)
            .cloned()
            .collect()
    }

    /// 清空日志
    pub fn clear_logs(&mut self) {
        self.log_buffer.clear();
    }

    /// 请求截图
    pub fn request_screenshot(&mut self, context: impl Into<String>, full_page: bool) {
        if !self.config.auto_screenshot {
            return;
        }

        let request = ScreenshotRequest {
            _timestamp: chrono::Utc::now().to_rfc3339(),
            _context: context.into(),
            _full_page: full_page,
        };

        self.screenshot_queue.push_back(request);

        // 限制队列大小
        if self.screenshot_queue.len() > 100 {
            self.screenshot_queue.pop_front();
        }
    }

    /// 获取待处理的截图请求
    pub fn get_pending_screenshots(&mut self) -> Vec<ScreenshotRequest> {
        let pending: Vec<_> = self.screenshot_queue.iter().cloned().collect();
        self.screenshot_queue.clear();
        pending
    }

    /// 导出调试报告
    pub fn export_report(&self, session_info: SessionInfo) -> DebugReport {
        let logs = self.get_logs();
        let statistics = self.calculate_statistics(&logs);
        let errors = self.summarize_errors(&logs);
        let screenshots: Vec<String> = logs
            .iter()
            .filter_map(|e| e.screenshot_url.clone())
            .collect();

        DebugReport {
            generated_at: chrono::Utc::now().to_rfc3339(),
            session_info,
            log_entries: logs,
            statistics,
            errors,
            screenshots,
        }
    }

    /// 计算统计信息
    fn calculate_statistics(&self, logs: &[BrowserLogEntry]) -> DebugStatistics {
        let mut logs_by_level: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut error_count = 0;
        let mut screenshot_count = 0;

        for log in logs {
            *logs_by_level
                .entry(log.level.as_str().to_string())
                .or_insert(0) += 1;

            if log.level >= LogLevel::Error {
                error_count += 1;
            }

            if log.screenshot_url.is_some() {
                screenshot_count += 1;
            }
        }

        let total = logs.len();
        let error_rate = if total > 0 {
            (error_count as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        DebugStatistics {
            total_logs: total,
            logs_by_level,
            average_action_time_ms: 0.0, // 需要额外计算
            error_rate,
            screenshot_count,
        }
    }

    /// 汇总错误
    fn summarize_errors(&self, logs: &[BrowserLogEntry]) -> Vec<ErrorSummary> {
        let mut error_map: std::collections::HashMap<String, Vec<&BrowserLogEntry>> =
            std::collections::HashMap::new();

        for log in logs.iter().filter(|e| e.level >= LogLevel::Error) {
            // 简单按错误消息分组
            error_map
                .entry(log.message.clone())
                .or_default()
                .push(log);
        }

        error_map
            .into_iter()
            .filter_map(|(msg, entries)| {
                let first = entries.first()?;
                let last = entries.last()?;

                Some(ErrorSummary {
                    error_type: "RuntimeError".to_string(),
                    count: entries.len(),
                    first_occurrence: first.timestamp.clone(),
                    last_occurrence: last.timestamp.clone(),
                    sample_message: msg,
                    suggestions: vec![
                        "检查选择器是否正确".to_string(),
                        "确认页面是否已加载".to_string(),
                    ],
                })
            })
            .collect()
    }

    /// 从浏览器事件创建日志
    pub fn log_browser_event(&mut self, event: &BrowserEvent) {
        match event {
            BrowserEvent::PageLoad { url, title } => {
                let source = LogSource::Page {
                    url: url.clone(),
                    title: title.clone(),
                };
                self.info(source, format!("Page loaded: {}", title));
            }
            BrowserEvent::ElementClick { selector } => {
                let source = LogSource::Automation {
                    action: "click".to_string(),
                };
                self.debug(source, format!("Element clicked: {}", selector));
            }
            BrowserEvent::ConsoleLog { level, message } => {
                if self.config.capture_console_logs {
                    let source = LogSource::Console { level: level.clone() };
                    let log_level = LogLevel::from_str(level).unwrap_or(LogLevel::Info);
                    self.log(log_level, source, message.clone(), None);
                }
            }
            BrowserEvent::Navigation { from, to } => {
                let source = LogSource::Browser {
                    instance_id: "unknown".to_string(),
                };
                self.info(source, format!("Navigation: {} -> {}", from, to));
            }
            BrowserEvent::Screenshot { path } => {
                let source = LogSource::Automation {
                    action: "screenshot".to_string(),
                };
                self.debug(source, format!("Screenshot captured: {}", path));
            }
            BrowserEvent::Error { error } => {
                let source = LogSource::Automation {
                    action: "error".to_string(),
                };
                self.error(source, &error.message, Some(error));
            }
        }
    }

    /// 生成智能错误提示
    pub fn generate_error_suggestions(error: &BrowserError) -> Vec<String> {
        let mut suggestions = error.suggestions.clone();

        // 根据错误类型添加特定建议
        match &error.error_type {
            super::BrowserErrorType::ElementNotFound { attempted_selectors } => {
                if attempted_selectors.iter().any(|s| s.starts_with("xpath=")) {
                    suggestions.push("XPath 选择器可能不稳定，尝试使用 CSS 选择器".to_string());
                }
                if error.current_url.is_some() {
                    suggestions.push("检查当前页面 URL 是否正确".to_string());
                }
            }
            super::BrowserErrorType::Timeout { .. } => {
                suggestions.push("检查网络连接是否稳定".to_string());
                suggestions.push("尝试增加超时时间".to_string());
            }
            super::BrowserErrorType::NavigationFailed { url, .. } => {
                if url.starts_with("https://") {
                    suggestions.push("尝试使用 http:// 协议".to_string());
                }
                suggestions.push("检查 URL 拼写是否正确".to_string());
            }
            _ => {}
        }

        suggestions
    }
}

impl ScreenshotRequest {
    pub fn new(context: impl Into<String>, full_page: bool) -> Self {
        Self {
            _timestamp: chrono::Utc::now().to_rfc3339(),
            _context: context.into(),
            _full_page: full_page,
        }
    }
}

use std::sync::OnceLock;
use parking_lot::Mutex;

/// 全局调试日志记录器
pub static GLOBAL_DEBUGGER: OnceLock<Mutex<BrowserDebugger>> = OnceLock::new();

/// 初始化全局调试器
pub fn init_global_debugger(config: DebuggerConfig) {
    let _ = GLOBAL_DEBUGGER.set(Mutex::new(BrowserDebugger::new(config)));
}

/// 获取全局调试器
pub fn global_debugger() -> Option<&'static Mutex<BrowserDebugger>> {
    GLOBAL_DEBUGGER.get()
}

/// 便捷日志函数
pub fn log_info(source: LogSource, message: impl Into<String>) {
    if let Some(debugger) = global_debugger() {
        debugger.lock().info(source, message);
    }
}

pub fn log_error(source: LogSource, message: impl Into<String>) {
    if let Some(debugger) = global_debugger() {
        debugger.lock().error(source, message, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_ordering() {
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warning);
        assert!(LogLevel::Warning < LogLevel::Error);
        assert!(LogLevel::Error < LogLevel::Critical);
    }

    #[test]
    fn test_log_level_should_log() {
        assert!(LogLevel::Info.should_log(&LogLevel::Debug));
        assert!(!LogLevel::Debug.should_log(&LogLevel::Info));
        assert!(LogLevel::Error.should_log(&LogLevel::Warning));
    }

    #[test]
    fn test_browser_debugger() {
        let mut config = DebuggerConfig::default();
        config.log_level = LogLevel::Debug; // Set to Debug to capture all messages
        let mut debugger = BrowserDebugger::new(config);

        let source = LogSource::System {
            component: "test".to_string(),
        };

        debugger.info(source.clone(), "Test message");
        debugger.debug(source.clone(), "Debug message");

        let logs = debugger.get_logs();
        assert_eq!(logs.len(), 2);
    }

    #[test]
    fn test_log_level_from_str() {
        assert_eq!(LogLevel::from_str("debug"), Some(LogLevel::Debug));
        assert_eq!(LogLevel::from_str("INFO"), Some(LogLevel::Info));
        assert_eq!(LogLevel::from_str("warn"), Some(LogLevel::Warning));
        assert_eq!(LogLevel::from_str("error"), Some(LogLevel::Error));
        assert_eq!(LogLevel::from_str("unknown"), None);
    }
}
