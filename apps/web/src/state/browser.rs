//! 浏览器自动化状态管理

// SandboxStats may be used in future implementations
use leptos::prelude::*;

use crate::browser::{
    BrowserError, BrowserInstance, BrowserProfile, BrowserSandbox, ConnectionStatus,
};

/// 浏览器状态
#[derive(Clone, Debug)]
pub struct BrowserState {
    /// 当前选中的配置 ID
    pub selected_profile_id: RwSignal<Option<String>>,
    /// 当前连接的实例
    pub current_instance: RwSignal<Option<BrowserInstance>>,
    /// 所有配置
    pub profiles: RwSignal<Vec<BrowserProfile>>,
    /// 所有沙箱
    pub sandboxes: RwSignal<Vec<BrowserSandbox>>,
    /// 当前选中的沙箱
    pub selected_sandbox_id: RwSignal<Option<String>>,
    /// 连接状态
    pub connection_status: RwSignal<ConnectionStatus>,
    /// 是否正在加载
    pub is_loading: RwSignal<bool>,
    /// 当前错误
    pub error: RwSignal<Option<BrowserError>>,
    /// 当前 URL
    pub current_url: RwSignal<String>,
    /// 日志消息
    pub logs: RwSignal<Vec<String>>,
    /// 是否正在执行批处理
    pub is_executing_batch: RwSignal<bool>,
}

impl BrowserState {
    pub fn new() -> Self {
        Self {
            selected_profile_id: RwSignal::new(None),
            current_instance: RwSignal::new(None),
            profiles: RwSignal::new(Vec::new()),
            sandboxes: RwSignal::new(Vec::new()),
            selected_sandbox_id: RwSignal::new(None),
            connection_status: RwSignal::new(ConnectionStatus::Disconnected),
            is_loading: RwSignal::new(false),
            error: RwSignal::new(None),
            current_url: RwSignal::new("about:blank".to_string()),
            logs: RwSignal::new(Vec::new()),
            is_executing_batch: RwSignal::new(false),
        }
    }

    /// 选中配置
    pub fn select_profile(&self, id: impl Into<String>) {
        self.selected_profile_id.set(Some(id.into()));
    }

    /// 清除选中的配置
    pub fn clear_selected_profile(&self) {
        self.selected_profile_id.set(None);
    }

    /// 设置连接状态
    pub fn set_connection_status(&self, status: ConnectionStatus) {
        self.connection_status.set(status);
    }

    /// 设置加载状态
    pub fn set_loading(&self, loading: bool) {
        self.is_loading.set(loading);
    }

    /// 设置错误
    pub fn set_error(&self, error: Option<BrowserError>) {
        self.error.set(error);
    }

    /// 添加日志
    pub fn add_log(&self, message: impl Into<String>) {
        let msg = message.into();
        self.logs.update(|logs| {
            logs.push(msg);
            if logs.len() > 1000 {
                logs.remove(0);
            }
        });
    }

    /// 清空日志
    pub fn clear_logs(&self) {
        self.logs.set(Vec::new());
    }

    /// 导航到 URL
    pub fn navigate(&self, url: impl Into<String>) {
        self.current_url.set(url.into());
    }

    /// 检查是否已连接
    pub fn is_connected(&self) -> bool {
        matches!(self.connection_status.get(), ConnectionStatus::Connected)
    }
}

impl Default for BrowserState {
    fn default() -> Self {
        Self::new()
    }
}

/// 浏览器 UI 状态
#[derive(Clone, Debug)]
pub struct BrowserUIState {
    /// 是否显示配置面板
    pub show_profiles_panel: RwSignal<bool>,
    /// 是否显示沙箱面板
    pub show_sandboxes_panel: RwSignal<bool>,
    /// 是否显示调试面板
    pub show_debug_panel: RwSignal<bool>,
    /// 是否全屏
    pub is_fullscreen: RwSignal<bool>,
    /// 是否显示添加配置弹窗
    pub show_add_profile_modal: RwSignal<bool>,
    /// 是否显示创建沙箱弹窗
    pub show_create_sandbox_modal: RwSignal<bool>,
    /// 配置表单数据
    pub profile_form: RwSignal<ProfileFormData>,
}

impl BrowserUIState {
    pub fn new() -> Self {
        Self {
            show_profiles_panel: RwSignal::new(true),
            show_sandboxes_panel: RwSignal::new(false),
            show_debug_panel: RwSignal::new(false),
            is_fullscreen: RwSignal::new(false),
            show_add_profile_modal: RwSignal::new(false),
            show_create_sandbox_modal: RwSignal::new(false),
            profile_form: RwSignal::new(ProfileFormData::default()),
        }
    }

    pub fn toggle_profiles_panel(&self) {
        self.show_profiles_panel.update(|v| *v = !*v);
    }

    pub fn toggle_sandboxes_panel(&self) {
        self.show_sandboxes_panel.update(|v| *v = !*v);
    }

    pub fn toggle_debug_panel(&self) {
        self.show_debug_panel.update(|v| *v = !*v);
    }

    pub fn toggle_fullscreen(&self) {
        self.is_fullscreen.update(|v| *v = !*v);
    }

    pub fn open_add_profile_modal(&self) {
        self.profile_form.set(ProfileFormData::default());
        self.show_add_profile_modal.set(true);
    }

    pub fn close_add_profile_modal(&self) {
        self.show_add_profile_modal.set(false);
    }

    pub fn open_create_sandbox_modal(&self) {
        self.show_create_sandbox_modal.set(true);
    }

    pub fn close_create_sandbox_modal(&self) {
        self.show_create_sandbox_modal.set(false);
    }
}

impl Default for BrowserUIState {
    fn default() -> Self {
        Self::new()
    }
}

/// 配置表单数据
#[derive(Clone, Debug, Default)]
pub struct ProfileFormData {
    pub name: String,
    pub cdp_port: u16,
    pub color: String,
    pub profile_type: String,
}

/// 提供浏览器状态到上下文
pub fn provide_browser_state() {
    provide_context(BrowserState::new());
    provide_context(BrowserUIState::new());
}

/// 使用浏览器状态
pub fn use_browser_state() -> BrowserState {
    use_context::<BrowserState>().expect("BrowserState not provided")
}

/// 使用浏览器 UI 状态
pub fn use_browser_ui_state() -> BrowserUIState {
    use_context::<BrowserUIState>().expect("BrowserUIState not provided")
}
