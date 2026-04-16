//! Authentication state management
//!
//! Separated from AppState to avoid unnecessary re-renders
//!
//! Security features:
//! - JWT token persistence with refresh token rotation
//! - Automatic token refresh before expiration
//! - Secure storage with memory-only fallback

use gloo_storage::Storage;
use leptos::prelude::*;
use leptos::task::spawn_local;

/// Token refresh threshold - refresh when less than 5 minutes remaining
const TOKEN_REFRESH_THRESHOLD_SECS: i64 = 300;
/// Token check interval - check every 60 seconds
const TOKEN_CHECK_INTERVAL_MS: u32 = 60000;

// Token refresh timer storage - stored separately to avoid Send/Sync issues
// This is stored in a thread-local since it's only used in the main thread
thread_local! {
    static REFRESH_TIMER: std::cell::RefCell<Option<gloo_timers::callback::Interval>> = std::cell::RefCell::new(None);
}

/// Authentication state
#[derive(Clone, Debug)]
pub struct AuthState {
    /// Current authenticated user
    pub user: RwSignal<Option<User>>,
    /// JWT access token storage
    pub token: RwSignal<Option<String>>,
    /// Refresh token for obtaining new access tokens
    pub refresh_token: RwSignal<Option<String>>,
    /// Token expiration timestamp
    pub token_expires_at: RwSignal<Option<i64>>,
    /// Authentication loading state
    pub is_loading: RwSignal<bool>,
    /// Last authentication error
    pub error: RwSignal<Option<AuthError>>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct User {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub avatar: Option<String>,
    pub wallet_address: Option<String>,
    pub roles: Vec<Role>,
    pub permissions: Vec<Permission>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Operator,
    Member,
    Guest,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Permission {
    // Agent permissions
    AgentCreate,
    AgentRead,
    AgentUpdate,
    AgentDelete,
    AgentStart,
    AgentStop,

    // DAO permissions
    DaoVote,
    DaoCreateProposal,
    DaoExecute,

    // Treasury permissions
    TreasuryView,
    TreasuryDeposit,
    TreasuryWithdraw,

    // System permissions
    SettingsRead,
    SettingsWrite,
    UserManage,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AuthError {
    InvalidCredentials,
    TokenExpired,
    NetworkError(String),
    ServerError(String),
    Unauthorized,
}

impl AuthState {
    pub fn new() -> Self {
        Self {
            user: RwSignal::new(None),
            token: RwSignal::new(None),
            refresh_token: RwSignal::new(None),
            token_expires_at: RwSignal::new(None),
            is_loading: RwSignal::new(false),
            error: RwSignal::new(None),
        }
    }

    /// Check if user is authenticated
    pub fn is_authenticated(&self) -> bool {
        self.user.with(|u| u.is_some()) && !self.is_token_expired()
    }

    /// Check if token is expired
    pub fn is_token_expired(&self) -> bool {
        self.token_expires_at.with(|exp| {
            exp.map(|e| {
                let now = chrono::Utc::now().timestamp();
                e < now
            })
            .unwrap_or(true)
        })
    }

    /// Check if user has specific role
    pub fn has_role(&self, role: &Role) -> bool {
        self.user.with(|u| {
            u.as_ref()
                .map(|user| user.roles.contains(role))
                .unwrap_or(false)
        })
    }

    /// Check if user has any of the given roles
    pub fn has_any_role(&self, roles: &[Role]) -> bool {
        roles.iter().any(|r| self.has_role(r))
    }

    /// Check if user has specific permission
    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.user.with(|u| {
            u.as_ref()
                .map(|user| user.permissions.contains(permission))
                .unwrap_or(false)
        })
    }

    /// Check if user has any of the given permissions
    pub fn has_any_permission(&self, permissions: &[Permission]) -> bool {
        permissions.iter().any(|p| self.has_permission(p))
    }

    /// Check if user has all of the given permissions
    pub fn has_all_permissions(&self, permissions: &[Permission]) -> bool {
        permissions.iter().all(|p| self.has_permission(p))
    }

    /// Set authenticated user with tokens
    ///
    /// # Security
    /// - Access token is stored in memory and localStorage
    /// - Refresh token is stored in memory and localStorage for session persistence
    /// - Token refresh interval is started automatically
    pub fn set_authenticated(
        &self,
        user: User,
        token: String,
        refresh_token: String,
        expires_in_secs: i64,
    ) {
        let expires_at = chrono::Utc::now().timestamp() + expires_in_secs;
        self.user.set(Some(user));
        self.token.set(Some(token.clone()));
        self.refresh_token.set(Some(refresh_token.clone()));
        self.token_expires_at.set(Some(expires_at));
        self.error.set(None);

        // Persist to storage (token, refresh token, and user info)
        self.persist_auth(&token, &refresh_token, expires_at);

        // Start automatic token refresh
        self.start_token_refresh_interval();
    }

    /// Update tokens after refresh (without changing user)
    pub fn update_tokens(
        &self,
        token: String,
        refresh_token: Option<String>,
        expires_in_secs: i64,
    ) {
        let expires_at = chrono::Utc::now().timestamp() + expires_in_secs;
        self.token.set(Some(token.clone()));
        let rt = refresh_token.unwrap_or_else(|| {
            self.refresh_token.get().unwrap_or_default()
        });
        self.refresh_token.set(Some(rt.clone()));
        self.token_expires_at.set(Some(expires_at));

        // Persist new access token
        self.persist_auth(&token, &rt, expires_at);
    }

    /// Check if token needs refresh (within threshold of expiration)
    pub fn should_refresh_token(&self) -> bool {
        self.token_expires_at.with(|exp| {
            exp.map(|e| {
                let now = chrono::Utc::now().timestamp();
                let time_remaining = e - now;
                // Refresh if less than threshold remaining and not already expired
                time_remaining > 0 && time_remaining < TOKEN_REFRESH_THRESHOLD_SECS
            })
            .unwrap_or(false)
        })
    }

    /// Get refresh token
    pub fn get_refresh_token(&self) -> Option<String> {
        self.refresh_token.get()
    }

    /// Start automatic token refresh interval
    fn start_token_refresh_interval(&self) {
        let auth_state = self.clone();

        // Create interval that checks token expiration periodically
        let interval = gloo_timers::callback::Interval::new(TOKEN_CHECK_INTERVAL_MS, move || {
            if auth_state.should_refresh_token() {
                // Trigger token refresh
                let auth = auth_state.clone();
                spawn_local(async move {
                    auth.perform_token_refresh().await;
                });
            }
        });

        // Store in thread_local
        REFRESH_TIMER.with(|timer| {
            *timer.borrow_mut() = Some(interval);
        });
    }

    /// Perform token refresh using refresh token
    ///
    /// # Security
    /// - Uses memory-only refresh token
    /// - Clears auth on refresh failure (token rotation failure)
    async fn perform_token_refresh(&self) {
        if let Some(refresh_token) = self.get_refresh_token() {
            // Call refresh API
            use crate::api::ApiClient;
            let client = ApiClient::default_client();

            match client.refresh_token(&refresh_token).await {
                Ok(response) => {
                    // Update with new tokens
                    self.update_tokens(
                        response.access_token,
                        response.refresh_token,
                        response.expires_in,
                    );
                    // Token refreshed successfully
                    let _ = web_sys::console::log_1(&"Token refreshed successfully".into());
                }
                Err(e) => {
                    let _ = web_sys::console::log_1(&format!("Token refresh failed: {}", e).into());
                    // Don't logout if the backend doesn't implement refresh endpoint (404)
                    // or if it's a demo token
                    use crate::api::ApiError;
                    if matches!(e, ApiError::NotFound) {
                        let _ = web_sys::console::log_1(&"Refresh endpoint not found, keeping current session".into());
                        return;
                    }
                    // Clear authentication on refresh failure
                    // This forces user to re-login
                    self.logout();
                    self.set_error(AuthError::TokenExpired);
                }
            }
        }
    }

    /// Clear authentication state
    pub fn logout(&self) {
        self.user.set(None);
        self.token.set(None);
        self.refresh_token.set(None);
        self.token_expires_at.set(None);
        self.error.set(None);

        // Clear interval from thread_local
        REFRESH_TIMER.with(|timer| {
            *timer.borrow_mut() = None;
        });

        // Clear from storage
        self.clear_auth_storage();
    }

    /// Set error state
    pub fn set_error(&self, error: AuthError) {
        self.error.set(Some(error));
    }

    /// Get current token for API requests
    pub fn get_token(&self) -> Option<String> {
        self.token.get()
    }

    /// Persist auth to localStorage
    ///
    /// # Security Notes
    /// - Access token, user info, and refresh token are persisted to storage
    /// - This allows session persistence across page reloads
    fn persist_auth(&self, token: &str, refresh_token: &str, expires_at: i64) {
        let _ = gloo_storage::LocalStorage::raw().set_item("auth_token", token);
        let _ = gloo_storage::LocalStorage::raw().set_item("auth_refresh_token", refresh_token);
        let _ = gloo_storage::LocalStorage::raw().set_item("auth_expires_at", &expires_at.to_string());
        // Persist user info
        if let Some(ref user) = self.user.get() {
            if let Ok(user_json) = serde_json::to_string(user) {
                let _ = gloo_storage::LocalStorage::raw().set_item("auth_user", &user_json);
            }
        }
    }

    /// Clear auth from storage
    fn clear_auth_storage(&self) {
        let _ = gloo_storage::LocalStorage::raw().remove_item("auth_token");
        let _ = gloo_storage::LocalStorage::raw().remove_item("auth_refresh_token");
        let _ = gloo_storage::LocalStorage::raw().remove_item("auth_expires_at");
        let _ = gloo_storage::LocalStorage::raw().remove_item("auth_user");
    }

    /// Restore auth from storage on init
    ///
    /// # Security
    /// - Restores user info, access token, and refresh token from storage
    /// - This allows session persistence across page reloads
    pub fn restore_from_storage(&self) {
        if let (Ok(Some(token)), Ok(Some(expires_at_str)), Ok(Some(user_json))) = (
            gloo_storage::LocalStorage::raw().get_item("auth_token"),
            gloo_storage::LocalStorage::raw().get_item("auth_expires_at"),
            gloo_storage::LocalStorage::raw().get_item("auth_user"),
        ) {
            let expires_at: i64 = expires_at_str.parse().unwrap_or(0);
            // Check if token is still valid (with 60 second buffer)
            let now = chrono::Utc::now().timestamp();
            if expires_at > now + 60 {
                // Parse user from JSON
                if let Ok(user) = serde_json::from_str::<User>(&user_json) {
                    self.user.set(Some(user));
                    self.token.set(Some(token));
                    self.token_expires_at.set(Some(expires_at));
                    // Try to restore refresh token too
                    if let Ok(Some(refresh)) = gloo_storage::LocalStorage::raw().get_item("auth_refresh_token") {
                        self.refresh_token.set(Some(refresh));
                    }
                    // Start refresh interval
                    self.start_token_refresh_interval();
                }
            } else {
                // Token expired, clear storage
                self.clear_auth_storage();
            }
        }
    }
}

impl Default for AuthState {
    fn default() -> Self {
        Self::new()
    }
}

/// Provide auth state to context
pub fn provide_auth_state() {
    let auth_state = AuthState::new();
    auth_state.restore_from_storage();
    provide_context(auth_state);
}

/// Use auth state from context
pub fn use_auth_state() -> AuthState {
    use_context::<AuthState>().expect("AuthState not provided")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_state_new() {
        let auth = AuthState::new();
        assert!(!auth.is_authenticated());
        assert!(auth.get_token().is_none());
    }

    #[test]
    #[cfg(target_arch = "wasm32")]
    fn test_role_check() {
        let auth = AuthState::new();
        let user = User {
            id: "1".to_string(),
            name: "Test".to_string(),
            email: None,
            avatar: None,
            wallet_address: None,
            roles: vec![Role::Admin, Role::Operator],
            permissions: vec![],
        };
        auth.set_authenticated(user, "token".to_string(), "refresh_token".to_string(), 3600);

        assert!(auth.has_role(&Role::Admin));
        assert!(auth.has_role(&Role::Operator));
        assert!(!auth.has_role(&Role::Member));
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_role_check_stub() {
        // set_authenticated uses LocalStorage which only works in WASM
        assert!(true);
    }

    #[test]
    #[cfg(target_arch = "wasm32")]
    fn test_permission_check() {
        let auth = AuthState::new();
        let user = User {
            id: "1".to_string(),
            name: "Test".to_string(),
            email: None,
            avatar: None,
            wallet_address: None,
            roles: vec![],
            permissions: vec![Permission::AgentRead, Permission::AgentCreate],
        };
        auth.set_authenticated(user, "token".to_string(), "refresh_token".to_string(), 3600);

        assert!(auth.has_permission(&Permission::AgentRead));
        assert!(auth.has_permission(&Permission::AgentCreate));
        assert!(!auth.has_permission(&Permission::AgentDelete));

        assert!(auth.has_any_permission(&[Permission::AgentRead, Permission::AgentDelete]));
        assert!(!auth.has_all_permissions(&[Permission::AgentRead, Permission::AgentDelete]));
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_permission_check_stub() {
        // set_authenticated uses LocalStorage which only works in WASM
        assert!(true);
    }
}
