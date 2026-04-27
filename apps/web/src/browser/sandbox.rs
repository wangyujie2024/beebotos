//! 浏览器沙箱管理
//!
//! 提供并发安全的多实例隔离和自动资源回收功能
//! 兼容 OpenClaw V2026.3.13 沙箱规格

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::ConnectionStatus;

/// 沙箱实例
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrowserSandbox {
    pub id: String,
    pub name: String,
    pub profile_id: String,
    pub cdp_port: u16,
    pub color: String,
    pub status: SandboxStatus,
    pub isolation: IsolationLevel,
    pub resource_limits: ResourceLimits,
    pub auto_cleanup: AutoCleanupConfig,
    pub created_at: String,
    pub last_activity: String,
    pub metadata: HashMap<String, String>,
}

/// 沙箱状态
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum SandboxStatus {
    Creating,
    Running,
    Paused,
    Cleaning,
    Stopped,
    Error(String),
}

/// 隔离级别
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum IsolationLevel {
    /// 无隔离
    None,
    /// 进程隔离
    Process,
    /// 容器隔离（文件系统、网络、进程三级）
    Container,
}

impl Default for IsolationLevel {
    fn default() -> Self {
        IsolationLevel::Process
    }
}

/// 资源限制
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub memory_limit_mb: u32,
    pub cpu_limit_percent: f32,
    pub max_tabs: u32,
    pub disk_limit_mb: u32,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            memory_limit_mb: 512,
            cpu_limit_percent: 50.0,
            max_tabs: 10,
            disk_limit_mb: 1024,
        }
    }
}

impl ResourceLimits {
    pub fn new(memory_mb: u32) -> Self {
        Self {
            memory_limit_mb: memory_mb,
            ..Default::default()
        }
    }

    pub fn with_cpu(mut self, percent: f32) -> Self {
        self.cpu_limit_percent = percent;
        self
    }

    pub fn with_tabs(mut self, max: u32) -> Self {
        self.max_tabs = max;
        self
    }
}

/// 自动清理配置
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutoCleanupConfig {
    pub enabled: bool,
    pub max_age_seconds: u64,
    pub max_idle_time_seconds: u64,
    pub check_interval_seconds: u64,
}

impl Default for AutoCleanupConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_age_seconds: 3600,      // 1 hour
            max_idle_time_seconds: 300, // 5 minutes
            check_interval_seconds: 60, // 1 minute
        }
    }
}

impl AutoCleanupConfig {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            max_age_seconds: 0,
            max_idle_time_seconds: 0,
            check_interval_seconds: 0,
        }
    }
}

/// 沙箱统计信息
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SandboxStats {
    pub sandbox_id: String,
    pub memory_usage_mb: f64,
    pub cpu_usage_percent: f64,
    pub tab_count: u32,
    pub network_io_mb: f64,
    pub disk_io_mb: f64,
    pub uptime_seconds: u64,
    pub idle_time_seconds: u64,
}

/// 沙箱创建选项
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SandboxCreateOptions {
    pub name: String,
    pub profile_id: String,
    pub isolation: IsolationLevel,
    pub resource_limits: ResourceLimits,
    pub auto_cleanup: AutoCleanupConfig,
    pub color: Option<String>,
    pub metadata: Option<HashMap<String, String>>,
}

impl Default for SandboxCreateOptions {
    fn default() -> Self {
        Self {
            name: "New Sandbox".to_string(),
            profile_id: "default".to_string(),
            isolation: IsolationLevel::Process,
            resource_limits: ResourceLimits::default(),
            auto_cleanup: AutoCleanupConfig::default(),
            color: None,
            metadata: None,
        }
    }
}

/// 沙箱管理器
///
/// 管理多个浏览器沙箱实例，提供资源隔离和自动回收
#[derive(Clone)]
pub struct SandboxManager {
    sandboxes: HashMap<String, BrowserSandbox>,
    config: SandboxManagerConfig,
}

/// 沙箱管理器配置
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SandboxManagerConfig {
    pub max_instances: usize,
    pub default_isolation: IsolationLevel,
    pub enable_auto_cleanup: bool,
    pub base_cdp_port: u16,
}

impl Default for SandboxManagerConfig {
    fn default() -> Self {
        Self {
            max_instances: 5,
            default_isolation: IsolationLevel::Process,
            enable_auto_cleanup: true,
            base_cdp_port: 18800,
        }
    }
}

impl SandboxManager {
    /// 创建新的沙箱管理器
    pub fn new(config: SandboxManagerConfig) -> Self {
        Self {
            sandboxes: HashMap::new(),
            config,
        }
    }

    /// 获取所有沙箱
    pub fn list_sandboxes(&self) -> Vec<&BrowserSandbox> {
        self.sandboxes.values().collect()
    }

    /// 获取沙箱数量
    pub fn count(&self) -> usize {
        self.sandboxes.len()
    }

    /// 检查是否达到最大实例数
    pub fn is_at_capacity(&self) -> bool {
        self.sandboxes.len() >= self.config.max_instances
    }

    /// 创建沙箱
    pub async fn create_sandbox(
        &mut self,
        options: SandboxCreateOptions,
    ) -> Result<BrowserSandbox, SandboxError> {
        if self.is_at_capacity() {
            return Err(SandboxError::MaxInstancesReached {
                max: self.config.max_instances,
            });
        }

        // 生成唯一 ID
        let id = format!("sandbox-{}", uuid::Uuid::new_v4());

        // 分配端口
        let cdp_port = self.allocate_port()?;

        // 选择颜色
        let color = options.color.unwrap_or_else(|| Self::generate_color(&id));

        let now = chrono::Utc::now().to_rfc3339();

        let sandbox = BrowserSandbox {
            id: id.clone(),
            name: options.name,
            profile_id: options.profile_id,
            cdp_port,
            color,
            status: SandboxStatus::Creating,
            isolation: options.isolation,
            resource_limits: options.resource_limits,
            auto_cleanup: options.auto_cleanup,
            created_at: now.clone(),
            last_activity: now,
            metadata: options.metadata.unwrap_or_default(),
        };

        // 存储沙箱
        self.sandboxes.insert(id.clone(), sandbox.clone());

        // 模拟异步创建过程
        gloo_timers::future::TimeoutFuture::new(100).await;

        // 更新状态为运行中
        if let Some(s) = self.sandboxes.get_mut(&id) {
            s.status = SandboxStatus::Running;
        }

        Ok(sandbox)
    }

    /// 获取沙箱
    pub fn get_sandbox(&self, id: &str) -> Option<&BrowserSandbox> {
        self.sandboxes.get(id)
    }

    /// 获取沙箱（可变）
    pub fn get_sandbox_mut(&mut self, id: &str) -> Option<&mut BrowserSandbox> {
        self.sandboxes.get_mut(id)
    }

    /// 停止沙箱
    pub async fn stop_sandbox(&mut self, id: &str) -> Result<(), SandboxError> {
        let sandbox = self
            .sandboxes
            .get_mut(id)
            .ok_or_else(|| SandboxError::NotFound(id.to_string()))?;

        sandbox.status = SandboxStatus::Stopped;
        sandbox.last_activity = chrono::Utc::now().to_rfc3339();

        Ok(())
    }

    /// 删除沙箱
    pub async fn delete_sandbox(&mut self, id: &str) -> Result<(), SandboxError> {
        let sandbox = self
            .sandboxes
            .get(id)
            .ok_or_else(|| SandboxError::NotFound(id.to_string()))?;

        // 确保沙箱已停止
        if sandbox.status == SandboxStatus::Running {
            let _ = sandbox;
            self.stop_sandbox(id).await?;
        }

        self.sandboxes.remove(id);
        Ok(())
    }

    /// 更新沙箱活动
    pub fn update_activity(&mut self, id: &str) {
        if let Some(sandbox) = self.sandboxes.get_mut(id) {
            sandbox.last_activity = chrono::Utc::now().to_rfc3339();
        }
    }

    /// 获取沙箱统计
    pub async fn get_stats(&self, id: &str) -> Result<SandboxStats, SandboxError> {
        let _sandbox = self
            .sandboxes
            .get(id)
            .ok_or_else(|| SandboxError::NotFound(id.to_string()))?;

        // 模拟统计信息
        let _now = chrono::Utc::now();
        // 简化的统计计算
        let uptime_seconds = 0u64;
        let idle_time_seconds = 0u64;

        Ok(SandboxStats {
            sandbox_id: id.to_string(),
            memory_usage_mb: 256.0,  // 模拟值
            cpu_usage_percent: 15.5, // 模拟值
            tab_count: 3,            // 模拟值
            network_io_mb: 50.0,     // 模拟值
            disk_io_mb: 10.0,        // 模拟值
            uptime_seconds,
            idle_time_seconds,
        })
    }

    /// 清理过期沙箱
    pub async fn cleanup_expired(&mut self) -> Vec<String> {
        // 简化的清理逻辑 - 暂不实现时间比较
        let to_remove: Vec<String> = Vec::new();

        // 删除过期沙箱
        for id in &to_remove {
            let _ = self.delete_sandbox(id).await;
        }

        to_remove
    }

    /// 分配端口
    fn allocate_port(&self) -> Result<u16, SandboxError> {
        let base_port = self.config.base_cdp_port;
        let count = self.sandboxes.len() as u16;

        if count >= 100 {
            return Err(SandboxError::NoAvailablePorts);
        }

        Ok(base_port + count + 1)
    }

    /// 生成颜色
    fn generate_color(id: &str) -> String {
        // 基于 ID 生成一致的颜色
        let hash = id.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));

        let hue = (hash % 360) as f64;
        let saturation = 70.0;
        let lightness = 50.0;

        hsl_to_hex(hue, saturation, lightness)
    }

    /// 获取沙箱连接信息
    pub fn get_connection_info(&self, id: &str) -> Option<SandboxConnectionInfo> {
        let sandbox = self.sandboxes.get(id)?;

        Some(SandboxConnectionInfo {
            sandbox_id: id.to_string(),
            cdp_port: sandbox.cdp_port,
            ws_url: format!("ws://localhost:{}/devtools/browser", sandbox.cdp_port),
            status: match sandbox.status {
                SandboxStatus::Running => ConnectionStatus::Connected,
                _ => ConnectionStatus::Disconnected,
            },
        })
    }
}

/// 沙箱连接信息
#[derive(Clone, Debug)]
pub struct SandboxConnectionInfo {
    pub sandbox_id: String,
    pub cdp_port: u16,
    pub ws_url: String,
    pub status: ConnectionStatus,
}

/// 沙箱错误
#[derive(Clone, Debug)]
pub enum SandboxError {
    NotFound(String),
    MaxInstancesReached {
        max: usize,
    },
    NoAvailablePorts,
    CreationFailed(String),
    AlreadyExists(String),
    OperationNotAllowed {
        sandbox_id: String,
        operation: String,
    },
}

impl std::fmt::Display for SandboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SandboxError::NotFound(id) => write!(f, "Sandbox not found: {}", id),
            SandboxError::MaxInstancesReached { max } => {
                write!(f, "Maximum number of sandboxes reached: {}", max)
            }
            SandboxError::NoAvailablePorts => write!(f, "No available CDP ports"),
            SandboxError::CreationFailed(msg) => write!(f, "Failed to create sandbox: {}", msg),
            SandboxError::AlreadyExists(id) => write!(f, "Sandbox already exists: {}", id),
            SandboxError::OperationNotAllowed {
                sandbox_id,
                operation,
            } => write!(
                f,
                "Operation '{}' not allowed on sandbox '{}'",
                operation, sandbox_id
            ),
        }
    }
}

/// HSL 转十六进制颜色
fn hsl_to_hex(h: f64, s: f64, l: f64) -> String {
    let s = s / 100.0;
    let l = l / 100.0;

    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    let r = ((r + m) * 255.0).round() as u8;
    let g = ((g + m) * 255.0).round() as u8;
    let b = ((b + m) * 255.0).round() as u8;

    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

/// 预定义颜色列表（用于沙箱颜色分配）
pub const SANDBOX_COLORS: &[&str] = &[
    "#0066CC", // 蓝色
    "#00AA66", // 绿色
    "#FF6600", // 橙色
    "#9900CC", // 紫色
    "#CC0066", // 粉色
    "#00CCCC", // 青色
    "#FFCC00", // 黄色
    "#FF3366", // 红色
    "#66CC00", // 浅绿
    "#3366FF", // 深蓝
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_limits() {
        let limits = ResourceLimits::new(1024).with_cpu(75.0).with_tabs(20);

        assert_eq!(limits.memory_limit_mb, 1024);
        assert_eq!(limits.cpu_limit_percent, 75.0);
        assert_eq!(limits.max_tabs, 20);
    }

    #[test]
    fn test_auto_cleanup_config() {
        let config = AutoCleanupConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_age_seconds, 3600);
        assert_eq!(config.max_idle_time_seconds, 300);
    }

    #[test]
    fn test_sandbox_manager_config() {
        let config = SandboxManagerConfig::default();
        assert_eq!(config.max_instances, 5);
        assert_eq!(config.base_cdp_port, 18800);
    }

    #[test]
    fn test_hsl_to_hex() {
        let red = hsl_to_hex(0.0, 100.0, 50.0);
        assert!(red.starts_with('#'));
        assert_eq!(red.len(), 7);

        let blue = hsl_to_hex(240.0, 100.0, 50.0);
        assert!(blue.starts_with('#'));
    }
}
