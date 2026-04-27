//! Gateway 连接状态管理

use leptos::prelude::*;

use crate::gateway::{GatewayConfig, GatewayError, GatewayScope, GatewayStatus};

/// Gateway 连接状态
#[derive(Clone, Debug)]
pub struct GatewayConnectionState {
    /// 连接状态
    pub status: RwSignal<GatewayStatus>,
    /// 是否正在连接
    pub is_connecting: RwSignal<bool>,
    /// 最后错误
    pub last_error: RwSignal<Option<GatewayError>>,
    /// 连接配置
    pub config: RwSignal<GatewayConfig>,
    /// 已订阅的频道
    pub subscribed_channels: RwSignal<Vec<String>>,
    /// 可用权限范围
    pub available_scopes: RwSignal<Vec<GatewayScope>>,
    /// 延迟 (ms)
    pub latency_ms: RwSignal<Option<u64>>,
    /// 最后 ping 时间
    pub last_ping_at: RwSignal<Option<String>>,
    /// 重连尝试次数
    pub reconnect_attempts: RwSignal<u32>,
}

impl GatewayConnectionState {
    pub fn new() -> Self {
        Self {
            status: RwSignal::new(GatewayStatus::Disconnected),
            is_connecting: RwSignal::new(false),
            last_error: RwSignal::new(None),
            config: RwSignal::new(GatewayConfig::default()),
            subscribed_channels: RwSignal::new(Vec::new()),
            available_scopes: RwSignal::new(Vec::new()),
            latency_ms: RwSignal::new(None),
            last_ping_at: RwSignal::new(None),
            reconnect_attempts: RwSignal::new(0),
        }
    }

    /// 设置连接状态
    pub fn set_status(&self, status: GatewayStatus) {
        self.status.set(status);
    }

    /// 设置连接中状态
    pub fn set_connecting(&self, connecting: bool) {
        self.is_connecting.set(connecting);
    }

    /// 设置错误
    pub fn set_error(&self, error: Option<GatewayError>) {
        self.last_error.set(error);
    }

    /// 检查是否已连接
    pub fn is_connected(&self) -> bool {
        matches!(
            self.status.get(),
            GatewayStatus::Connected | GatewayStatus::Authenticated
        )
    }

    /// 检查是否已认证
    pub fn is_authenticated(&self) -> bool {
        matches!(self.status.get(), GatewayStatus::Authenticated)
    }

    /// 更新配置
    pub fn update_config(&self, config: GatewayConfig) {
        self.config.set(config);
    }

    /// 添加订阅频道
    pub fn subscribe_channel(&self, channel: impl Into<String>) {
        let channel = channel.into();
        self.subscribed_channels.update(|channels| {
            if !channels.contains(&channel) {
                channels.push(channel);
            }
        });
    }

    /// 取消订阅频道
    pub fn unsubscribe_channel(&self, channel: &str) {
        self.subscribed_channels.update(|channels| {
            channels.retain(|c| c != channel);
        });
    }

    /// 更新延迟
    pub fn update_latency(&self, latency: u64) {
        self.latency_ms.set(Some(latency));
        self.last_ping_at.set(Some(chrono::Utc::now().to_rfc3339()));
    }

    /// 增加重连尝试
    pub fn increment_reconnect(&self) {
        self.reconnect_attempts.update(|n| *n += 1);
    }

    /// 重置重连尝试
    pub fn reset_reconnect(&self) {
        self.reconnect_attempts.set(0);
    }

    /// 检查是否有权限
    pub fn has_scope(&self, scope: GatewayScope) -> bool {
        self.available_scopes.get().contains(&scope)
    }
}

impl Default for GatewayConnectionState {
    fn default() -> Self {
        Self::new()
    }
}

/// Gateway UI 状态
#[derive(Clone, Debug)]
pub struct GatewayUIState {
    /// 是否显示连接设置弹窗
    pub show_connection_settings: RwSignal<bool>,
    /// 是否显示权限详情
    pub show_scope_details: RwSignal<bool>,
    /// 是否显示连接日志
    pub show_connection_logs: RwSignal<bool>,
    /// 连接日志
    pub connection_logs: RwSignal<Vec<String>>,
    /// 编辑中的 API URL
    pub editing_api_url: RwSignal<String>,
    /// 编辑中的 WebSocket URL
    pub editing_ws_url: RwSignal<String>,
}

impl GatewayUIState {
    pub fn new() -> Self {
        Self {
            show_connection_settings: RwSignal::new(false),
            show_scope_details: RwSignal::new(false),
            show_connection_logs: RwSignal::new(false),
            connection_logs: RwSignal::new(Vec::new()),
            editing_api_url: RwSignal::new(String::new()),
            editing_ws_url: RwSignal::new(String::new()),
        }
    }

    pub fn open_connection_settings(&self, current_config: &GatewayConfig) {
        self.editing_api_url
            .set(current_config.api_base_url.clone());
        self.editing_ws_url
            .set(current_config.websocket_url.clone());
        self.show_connection_settings.set(true);
    }

    pub fn close_connection_settings(&self) {
        self.show_connection_settings.set(false);
    }

    pub fn toggle_scope_details(&self) {
        self.show_scope_details.update(|v| *v = !*v);
    }

    pub fn toggle_connection_logs(&self) {
        self.show_connection_logs.update(|v| *v = !*v);
    }

    pub fn add_log(&self, message: impl Into<String>) {
        let msg = format!(
            "[{}] {}",
            chrono::Local::now().format("%H:%M:%S"),
            message.into()
        );
        self.connection_logs.update(|logs| {
            logs.push(msg);
            if logs.len() > 100 {
                logs.remove(0);
            }
        });
    }

    pub fn clear_logs(&self) {
        self.connection_logs.set(Vec::new());
    }
}

impl Default for GatewayUIState {
    fn default() -> Self {
        Self::new()
    }
}

/// 提供 Gateway 状态到上下文
pub fn provide_gateway_state() {
    provide_context(GatewayConnectionState::new());
    provide_context(GatewayUIState::new());
}

/// 使用 Gateway 连接状态
pub fn use_gateway_state() -> GatewayConnectionState {
    use_context::<GatewayConnectionState>().expect("GatewayConnectionState not provided")
}

/// 使用 Gateway UI 状态
pub fn use_gateway_ui_state() -> GatewayUIState {
    use_context::<GatewayUIState>().expect("GatewayUIState not provided")
}
