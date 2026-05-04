//! Application-level state composition
//!
//! This module composes domain-specific states into the global app state.
//! Each domain state is provided separately to avoid unnecessary re-renders.
//!
//! ## OpenClaw V2026.3.13 新增功能
//! - 浏览器自动化服务集成
//! - WebChat API 服务集成
//! - Gateway 连接服务

use crate::api::{
    AgentService, ApiClient, AuthService, BrowserApiService, CompositionService, DaoService,
    LlmConfigService, SettingsService, SkillService, TreasuryService, WebchatApiService,
    WorkflowService,
};
use crate::state::{
    agent::{provide_agent_state, AgentState},
    auth::{AuthState},
    browser::{provide_browser_state, BrowserState},
    dao::{provide_dao_state, DaoState},
    gateway::{provide_gateway_state, GatewayConnectionState},
    notification::{provide_notification_state, NotificationState},
    webchat::{provide_webchat_state, WebchatState},
};
use leptos::prelude::*;

/// Global application state
///
/// This is a composition of domain-specific states.
/// Each domain state is stored separately to minimize re-renders.
#[derive(Clone)]
pub struct AppState {
    /// Authentication state
    pub auth: AuthState,
    /// Agent domain state
    pub agent: AgentState,
    /// DAO domain state
    pub dao: DaoState,
    /// Notification state
    pub notification: NotificationState,
    /// Browser automation state
    pub browser: BrowserState,
    /// WebChat state
    pub webchat: WebchatState,
    /// Gateway connection state
    pub gateway: GatewayConnectionState,
    /// Online status
    pub is_online: RwSignal<bool>,
    /// Settings signal
    pub settings: RwSignal<crate::api::Settings>,
    /// Skills loading state
    pub skills_loading: RwSignal<bool>,
    /// Settings loading state
    pub settings_loading: RwSignal<bool>,
    /// API Client
    api_client: ApiClient,
}

impl AppState {
    pub fn new() -> Self {
        let api_client = ApiClient::default_client();

        // Create auth state and restore from storage
        let auth = AuthState::new();
        auth.restore_from_storage();

        Self {
            auth,
            agent: AgentState::new(),
            dao: DaoState::new(),
            notification: NotificationState::new(),
            browser: BrowserState::new(),
            webchat: WebchatState::new(),
            gateway: GatewayConnectionState::new(),
            is_online: RwSignal::new(true),
            settings: RwSignal::new(crate::api::Settings {
                theme: crate::api::Theme::Dark,
                language: "en".to_string(),
                notifications_enabled: true,
                auto_update: true,
                api_endpoint: None,
                wallet_address: None,
            }),
            skills_loading: RwSignal::new(false),
            settings_loading: RwSignal::new(false),
            api_client: api_client.clone(),
        }
    }

    /// Create AppState with an existing AuthState (to ensure consistency)
    pub fn new_with_auth(auth: AuthState) -> Self {
        let api_client = ApiClient::default_client();

        Self {
            auth,
            agent: AgentState::new(),
            dao: DaoState::new(),
            notification: NotificationState::new(),
            browser: BrowserState::new(),
            webchat: WebchatState::new(),
            gateway: GatewayConnectionState::new(),
            is_online: RwSignal::new(true),
            settings: RwSignal::new(crate::api::Settings {
                theme: crate::api::Theme::Dark,
                language: "en".to_string(),
                notifications_enabled: true,
                auto_update: true,
                api_endpoint: None,
                wallet_address: None,
            }),
            skills_loading: RwSignal::new(false),
            settings_loading: RwSignal::new(false),
            api_client: api_client.clone(),
        }
    }

    /// Get API client (自动注入当前 auth token)
    pub fn api_client(&self) -> ApiClient {
        let client = self.api_client.clone();
        client.set_auth_token(self.auth.get_token());
        client
    }

    /// Get agent service
    pub fn agent_service(&self) -> AgentService {
        AgentService::new(self.api_client())
    }

    /// Get DAO service
    pub fn dao_service(&self) -> DaoService {
        DaoService::new(self.api_client())
    }

    /// Get skill service
    pub fn skill_service(&self) -> SkillService {
        SkillService::new(self.api_client())
    }

    /// Get treasury service
    pub fn treasury_service(&self) -> TreasuryService {
        TreasuryService::new(self.api_client())
    }

    /// Get settings service
    pub fn settings_service(&self) -> SettingsService {
        SettingsService::new(self.api_client())
    }

    /// Get auth service
    pub fn auth_service(&self) -> AuthService {
        AuthService::new(self.api_client())
    }

    /// Get browser API service (OpenClaw V2026.3.13 新增)
    pub fn browser_service(&self) -> BrowserApiService {
        BrowserApiService::new(self.api_client())
    }

    /// Get WebChat API service (OpenClaw V2026.3.13 新增)
    pub fn webchat_service(&self) -> WebchatApiService {
        WebchatApiService::new(self.api_client())
    }

    /// Get LLM config service
    pub fn llm_config_service(&self) -> LlmConfigService {
        LlmConfigService::new(self.api_client())
    }

    /// Get workflow service
    pub fn workflow_service(&self) -> WorkflowService {
        WorkflowService::new(self.api_client())
    }

    /// Get composition service
    pub fn composition_service(&self) -> CompositionService {
        CompositionService::new(self.api_client())
    }

    /// Set online status
    pub fn set_online(&self, online: bool) {
        self.is_online.set(online);
    }

    /// Check if app is online
    pub fn is_online(&self) -> bool {
        self.is_online.get()
    }

    // ==================== Backwards Compatibility ====================
    // These methods provide compatibility with old code that expects
    // the monolithic AppState structure

    /// Check if user is authenticated
    pub fn is_authenticated(&self) -> bool {
        self.auth.is_authenticated()
    }

    /// Get unread notification count
    pub fn unread_count(&self) -> usize {
        self.notification.unread_count()
    }

    /// Get user info
    pub fn user(&self) -> RwSignal<Option<crate::state::auth::User>> {
        self.auth.user
    }

    /// Notify helper
    pub fn notify(
        &self,
        notification_type: crate::state::notification::NotificationType,
        title: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.notification.add(notification_type, title, message);
    }

    /// Get loading state (compatibility)
    pub fn loading(&self) -> LoadingCompat {
        LoadingCompat {
            agents: self.agent.is_list_loading,
            skills: self.skills_loading,
            dao: self.dao.is_summary_loading,
            treasury: self.dao.is_summary_loading,
            settings: self.settings_loading,
            browser: self.browser.is_loading,
            webchat: self.webchat.is_sending,
        }
    }

    /// Get settings signal (compatibility)
    pub fn settings(&self) -> RwSignal<crate::api::Settings> {
        self.settings
    }

    /// Get browser connection status (OpenClaw V2026.3.13 新增)
    pub fn is_browser_connected(&self) -> bool {
        self.browser.is_connected()
    }

    /// Get gateway connection status (OpenClaw V2026.3.13 新增)
    pub fn is_gateway_connected(&self) -> bool {
        self.gateway.is_connected()
    }
}

/// Loading state compatibility struct
#[derive(Clone, Copy)]
pub struct LoadingCompat {
    pub agents: RwSignal<bool>,
    pub skills: RwSignal<bool>,
    pub dao: RwSignal<bool>,
    pub treasury: RwSignal<bool>,
    pub settings: RwSignal<bool>,
    pub browser: RwSignal<bool>,
    pub webchat: RwSignal<bool>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Provide all application states to context
///
/// This function provides both the composed AppState and individual domain states
/// to allow components to subscribe to only the state they need.
pub fn provide_app_state() {
    // Create a single auth state instance and provide it
    let auth_state = AuthState::new();
    auth_state.restore_from_storage();
    provide_context(auth_state.clone());

    provide_agent_state();
    provide_dao_state();
    provide_notification_state();
    provide_browser_state();
    provide_webchat_state();
    provide_gateway_state();

    // Provide composed app state using the SAME auth state instance
    provide_context(AppState::new_with_auth(auth_state));

    // Setup online/offline status monitoring
    #[cfg(target_arch = "wasm32")]
    setup_online_status_monitoring();
}

/// Setup online/offline status monitoring
#[allow(dead_code)]
fn setup_online_status_monitoring() {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    if let Some(window) = web_sys::window() {
        let app_state = use_app_state();

        // Online handler
        let online_callback = Closure::wrap(Box::new(move || {
            app_state.set_online(true);
            app_state
                .notification
                .success("Back Online", "Connection restored");
        }) as Box<dyn FnMut()>);

        // Offline handler
        let offline_callback = Closure::wrap(Box::new(move || {
            let app_state = use_app_state();
            app_state.set_online(false);
            app_state
                .notification
                .warning("Offline", "Working in offline mode");
        }) as Box<dyn FnMut()>);

        let _ = window
            .add_event_listener_with_callback("online", online_callback.as_ref().unchecked_ref());
        let _ = window
            .add_event_listener_with_callback("offline", offline_callback.as_ref().unchecked_ref());

        // Leak the closures (they live for the lifetime of the app)
        online_callback.forget();
        offline_callback.forget();
    }
}

/// Use the composed app state
pub fn use_app_state() -> AppState {
    use_context::<AppState>().expect("AppState not provided")
}

/// Convenience hook for using specific domain states
///
/// These hooks should be preferred over use_app_state() when only
/// one domain state is needed to avoid unnecessary re-renders.
pub mod hooks {
    pub use crate::state::{
        agent::use_agent_state,
        auth::use_auth_state,
        browser::use_browser_state,
        dao::use_dao_state,
        gateway::use_gateway_state,
        notification::use_notification_state,
        webchat::use_webchat_state,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_creation() {
        let state = AppState::new();
        assert!(state.is_online());
    }

    #[test]
    fn test_new_services() {
        let state = AppState::new();
        // 验证新服务可用
        let _browser_service = state.browser_service();
        let _webchat_service = state.webchat_service();
    }
}
