//! BeeBotOS Web Frontend Library
//!
//! 100% 兼容 OpenClaw V2026.3.13 Web 模块功能
//!
//! ## 主要模块
//!
//! - `browser`: 浏览器自动化（CDP、沙箱、调试）
//! - `webchat`: Web 聊天界面（会话管理、侧边提问）
//! - `gateway`: Gateway 集成（WebSocket、认证、权限）
//! - `api`: API 客户端和服务
//! - `state`: 状态管理
//! - `components`: UI 组件
//! - `pages`: 页面组件

use wasm_bindgen::prelude::*;
use leptos::prelude::*;

/// Application entry point for WASM
#[wasm_bindgen(start)]
pub fn main() {
    // Set panic hook to log errors to console
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&format!("Panic: {}", info).into());
    }));

    web_sys::console::log_1(&"BeeBotOS starting...".into());

    leptos::mount::mount_to_body(App);

    web_sys::console::log_1(&"BeeBotOS mounted".into());
}
use leptos::view;
use leptos_meta::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::StaticSegment;

// 服务器模块（仅非 WASM 目标）
#[cfg(not(target_arch = "wasm32"))]
pub mod server;

// 核心模块
pub mod api;
pub mod browser;
pub mod components;
pub mod error;
pub mod gateway;
pub mod i18n;
pub mod pages;
pub mod state;
pub mod utils;
pub mod webchat;

use components::{AuthGuard, ContentSecurityPolicy, GlobalErrorHandler, Sidebar};
use i18n::{init_i18n, I18nContext};
use leptos_router::hooks::use_location;
use pages::{
    AgentDetail, AgentsPage, ChannelsPage, DaoPage, Home, LlmConfigPage, LlmSettingsPage, LoginPage, NotFound, RegisterPage, SettingsPage, SetupPage, SkillInstancesPage, SkillsPage, TreasuryPage, TreasuryTransactionsPage, WorkflowDashboardPage, WorkflowDetailPage,
};
use components::AccessDenied;
use state::provide_app_state;
use utils::provide_theme;

/// Page title component that shows current page name
#[component]
fn PageTitle() -> impl IntoView {
    let i18n = use_context::<I18nContext>().expect("i18n context not found");
    let location = use_location();

    let page_title = Signal::derive(move || {
        let path = location.pathname.get();
        match path.as_str() {
            p if p.starts_with("/agents") => i18n.t("nav-agents"),
            p if p.starts_with("/channels") => i18n.t("nav-channels"),
            p if p.starts_with("/dao/treasury") => i18n.t("nav-treasury"),
            p if p.starts_with("/dao") => i18n.t("nav-dao"),
            p if p.starts_with("/skill-instances") => i18n.t("nav-skill-instances"),
            p if p.starts_with("/skills") => i18n.t("nav-skills"),
            p if p.starts_with("/settings") => i18n.t("nav-settings"),
            p if p.starts_with("/llm-settings") => i18n.t("nav-llm-settings"),
            p if p.starts_with("/llm-config") => i18n.t("nav-llm-config"),
            p if p.starts_with("/browser") => i18n.t("nav-browser"),
            p if p.starts_with("/workflows") => i18n.t("nav-workflows"),
            p if p.starts_with("/chat") => i18n.t("nav-chat"),
            _ => i18n.t("nav-home"),
        }
    });

    view! {
        <h1 class="page-title">{move || page_title.get()}</h1>
    }
}

/// 应用入口组件
#[component]
pub fn App() -> impl IntoView {
    // Debug log
    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(&"App component started".into());

    provide_meta_context();

    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(&"Meta context provided".into());

    provide_app_state();

    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(&"App state provided".into());

    provide_theme();

    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(&"Theme provided".into());

    let i18n = init_i18n();

    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(&"I18n initialized".into());

    // Set default locale to Chinese
    if i18n.get_locale() != i18n::Locale::ZhCN {
        i18n.set_locale(i18n::Locale::ZhCN);
    }

    view! {
        <ContentSecurityPolicy />
        <GlobalErrorHandler>
            <Router>
                <div class="app-container">
                    <Sidebar />
                    <main class="main-content">
                        <header class="top-bar">
                            <div class="top-bar-left">
                                <button class="mobile-menu-btn" on:click=move |_| {
                                    // Toggle sidebar on mobile
                                    let sidebar = web_sys::window()
                                        .and_then(|w| w.document())
                                        .and_then(|d| d.query_selector(".sidebar").ok())
                                        .flatten();
                                    if let Some(el) = sidebar {
                                        let class_list = el.class_list();
                                        let _ = class_list.toggle("open");
                                    }
                                }>
                                    "☰"
                                </button>
                                <PageTitle />
                            </div>
                            <div class="top-bar-right">
                                <button class="icon-btn" title="Notifications">
                                    "🔔"
                                </button>
                                <button class="icon-btn" title="Settings" on:click=move |_| {
                                    let navigate = leptos_router::hooks::use_navigate();
                                    navigate("/settings", Default::default());
                                }>
                                    "⚙️"
                                </button>
                            </div>
                        </header>
                        <div class="content-area">
                            <Routes fallback=|| view! { <NotFound/> }>
                                // 首页
                                <Route path=StaticSegment("") view=Home />

                                // 登录页
                                <Route path=StaticSegment("login") view=LoginPage />

                                // 注册页
                                <Route path=StaticSegment("register") view=RegisterPage />

                                // Agent 管理
                                <Route
                                    path=StaticSegment("agents")
                                    view=move || view! {
                                        <AuthGuard>
                                            <AgentsPage />
                                        </AuthGuard>
                                    }
                                />
                                <Route
                                    path=(StaticSegment("agents"), StaticSegment(":id"))
                                    view=move || view! {
                                        <AuthGuard>
                                            <AgentDetail />
                                        </AuthGuard>
                                    }
                                />

                                // DAO 治理
                                <Route
                                    path=StaticSegment("dao")
                                    view=move || view! {
                                        <AuthGuard>
                                            <DaoPage />
                                        </AuthGuard>
                                    }
                                />
                                <Route
                                    path=(StaticSegment("dao"), StaticSegment("treasury"))
                                    view=move || view! {
                                        <AuthGuard>
                                            <TreasuryPage />
                                        </AuthGuard>
                                    }
                                />
                                <Route
                                    path=(StaticSegment("dao"), StaticSegment("treasury"), StaticSegment("transactions"))
                                    view=move || view! {
                                        <AuthGuard>
                                            <TreasuryTransactionsPage />
                                        </AuthGuard>
                                    }
                                />

                                // 技能市场
                                <Route
                                    path=StaticSegment("skills")
                                    view=move || view! {
                                        <AuthGuard>
                                            <SkillsPage />
                                        </AuthGuard>
                                    }
                                />
                                <Route
                                    path=StaticSegment("skill-instances")
                                    view=move || view! {
                                        <AuthGuard>
                                            <SkillInstancesPage />
                                        </AuthGuard>
                                    }
                                />

                                // 频道管理
                                <Route
                                    path=StaticSegment("channels")
                                    view=move || view! {
                                        <AuthGuard>
                                            <ChannelsPage />
                                        </AuthGuard>
                                    }
                                />

                                // 系统设置
                                <Route
                                    path=StaticSegment("settings")
                                    view=move || view! {
                                        <AuthGuard>
                                            <SettingsPage />
                                        </AuthGuard>
                                    }
                                />
                                <Route
                                    path=(StaticSegment("settings"), StaticSegment("wizard"))
                                    view=move || view! {
                                        <AuthGuard>
                                            <SetupPage />
                                        </AuthGuard>
                                    }
                                />
                                // Gateway 首次部署向导
                                <Route
                                    path=StaticSegment("setup")
                                    view=SetupPage
                                />
                                // LLM 配置监控
                                <Route
                                    path=StaticSegment("llm-config")
                                    view=move || view! {
                                        <AuthGuard>
                                            <LlmConfigPage />
                                        </AuthGuard>
                                    }
                                />
                                // LLM 模型设置
                                <Route
                                    path=StaticSegment("llm-settings")
                                    view=move || view! {
                                        <AuthGuard>
                                            <LlmSettingsPage />
                                        </AuthGuard>
                                    }
                                />

                                // 浏览器自动化（OpenClaw V2026.3.13 新增）
                                <Route
                                    path=StaticSegment("browser")
                                    view=move || view! {
                                        <AuthGuard>
                                            <pages::BrowserPage />
                                        </AuthGuard>
                                    }
                                />

                                // WebChat（OpenClaw V2026.3.13 新增）
                                <Route
                                    path=StaticSegment("chat")
                                    view=move || view! {
                                        <AuthGuard>
                                            <pages::WebchatPage />
                                        </AuthGuard>
                                    }
                                />

                                // Workflow Dashboard
                                <Route
                                    path=StaticSegment("workflows")
                                    view=move || view! {
                                        <AuthGuard>
                                            <WorkflowDashboardPage />
                                        </AuthGuard>
                                    }
                                />
                                <Route
                                    path=(StaticSegment("workflows"), StaticSegment(":id"))
                                    view=move || view! {
                                        <AuthGuard>
                                            <WorkflowDetailPage />
                                        </AuthGuard>
                                    }
                                />

                                // 权限不足页面
                                <Route path=StaticSegment("unauthorized") view=AccessDenied />
                            </Routes>
                        </div>
                    </main>
                </div>
            </Router>
        </GlobalErrorHandler>
    }
}

/// 初始化全局调试器
pub fn init_debugger() {
    #[cfg(debug_assertions)]
    {
        browser::debugger::init_global_debugger(browser::debugger::DebuggerConfig::default());
    }
}

/// 获取版本信息
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_exports() {
        let _ = App;
    }

    #[test]
    fn test_version() {
        assert!(!version().is_empty());
    }
}
