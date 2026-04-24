//! State Management Module
//!
//! 按领域组织的状态管理，避免无关状态变化导致的不必要重渲染
//!
//! # 架构
//!
//! 状态按领域组织：
//! - `auth`: 认证和授权状态
//! - `agent`: Agent 管理状态
//! - `dao`: DAO 治理状态
//! - `notification`: 通知和 toast 状态
//! - `browser`: 浏览器自动化状态
//! - `webchat`: WebChat 聊天状态
//! - `gateway`: Gateway 连接状态
//! - `app`: 应用级组合

use leptos::prelude::Get;

pub mod agent;
pub mod app;
pub mod auth;
pub mod browser;
pub mod dao;
pub mod gateway;
pub mod notification;
pub mod webchat;
pub mod wizard;

// Re-export 常用项
pub use agent::{
    provide_agent_state, use_agent_state, AgentFilters, AgentPagination, AgentSortBy, AgentState,
    SortOrder,
};
pub use app::{hooks, provide_app_state, use_app_state, AppState};
pub use auth::{provide_auth_state, use_auth_state, AuthError, AuthState, User};
pub use browser::{
    provide_browser_state, use_browser_state, use_browser_ui_state, BrowserState, BrowserUIState,
    ProfileFormData,
};
pub use dao::{provide_dao_state, use_dao_state, DaoState, ProposalFilters, VoteRecord};
pub use gateway::{
    provide_gateway_state, use_gateway_state, GatewayConnectionState, GatewayUIState,
};
pub use notification::{
    provide_notification_state, use_notification_state, Notification, NotificationState,
    NotificationType,
};
pub use webchat::{
    provide_webchat_state, use_chat_ui_state, use_session_ui_state, use_webchat_state, ChatUIState,
    SessionUIState, WebchatState,
};

// 向后兼容 - 重新导出旧函数名
// 这些已弃用，应迁移到领域特定的 hooks
#[deprecated(since = "1.0.0", note = "Use use_auth_state() instead")]
pub fn use_user() -> Option<User> {
    use_auth_state().user.get()
}

#[deprecated(since = "1.0.0", note = "Use use_notification_state() instead")]
pub fn use_notifications() -> NotificationState {
    use_notification_state()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // 验证类型可访问
        let _: fn() = provide_auth_state;
        let _: fn() = provide_agent_state;
        let _: fn() = provide_dao_state;
        let _: fn() = provide_notification_state;
        let _: fn() = provide_browser_state;
        let _: fn() = provide_webchat_state;
        let _: fn() = provide_gateway_state;
        let _: fn() = provide_app_state;
    }
}
