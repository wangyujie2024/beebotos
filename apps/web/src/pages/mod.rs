//! 页面模块
//!
//! 包含应用的所有页面组件

pub mod agent_detail;
pub mod agents;
pub mod browser;
pub mod channels;
pub mod dao;
pub mod home;
pub mod llm_config;
pub mod llm_settings;
pub mod login;
pub mod not_found;
pub mod register;
pub mod settings;
pub mod setup;
pub mod skill_instances;
pub mod skills;
pub mod treasury;
pub mod webchat;
pub mod workflow_detail;
pub mod workflows;

pub use agent_detail::AgentDetail;
pub use agents::AgentsPage;
pub use browser::BrowserPage;
pub use channels::ChannelsPage;
pub use dao::DaoPage;
pub use home::Home;
pub use llm_config::LlmConfigPage;
pub use llm_settings::LlmSettingsPage;
pub use login::LoginPage;
pub use not_found::NotFound;
pub use register::RegisterPage;
pub use settings::SettingsPage;
pub use setup::SetupPage;
pub use skill_instances::SkillInstancesPage;
pub use skills::SkillsPage;
pub use treasury::{TreasuryPage, TreasuryTransactionsPage};
pub use webchat::WebchatPage;
pub use workflow_detail::WorkflowDetailPage;
pub use workflows::WorkflowDashboardPage;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_exports() {
        // 验证页面组件正确导出
        let _ = Home;
        let _ = AgentsPage;
        let _ = AgentDetail;
        let _ = DaoPage;
        let _ = TreasuryPage;
        let _ = SkillsPage;
        let _ = SettingsPage;
        let _ = NotFound;
        let _ = BrowserPage;
        let _ = WebchatPage;
        let _ = ChannelsPage;
        let _ = LoginPage;
        let _ = RegisterPage;
    }
}
