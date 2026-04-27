//! Gateway 认证模块
//!
//! 处理 Token 管理和认证流程

use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};

const TOKEN_STORAGE_KEY: &str = "beebotos_gateway_token";
const REFRESH_TOKEN_KEY: &str = "beebotos_gateway_refresh_token";

/// Gateway 认证信息
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GatewayAuth {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<String>,
    pub token_type: String,
}

impl GatewayAuth {
    /// 创建新的认证信息
    pub fn new(access_token: impl Into<String>) -> Self {
        Self {
            access_token: access_token.into(),
            refresh_token: None,
            expires_at: None,
            token_type: "Bearer".to_string(),
        }
    }

    /// 设置刷新令牌
    pub fn with_refresh_token(mut self, token: impl Into<String>) -> Self {
        self.refresh_token = Some(token.into());
        self
    }

    /// 设置过期时间
    pub fn with_expires_at(mut self, expires_at: impl Into<String>) -> Self {
        self.expires_at = Some(expires_at.into());
        self
    }

    /// 获取 Authorization 头部值
    pub fn authorization_header(&self) -> String {
        format!("{} {}", self.token_type, self.access_token)
    }

    /// 检查是否已过期
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = &self.expires_at {
            if let Ok(expires) = chrono::DateTime::parse_from_rfc3339(expires_at) {
                return chrono::Utc::now() > expires;
            }
        }
        false
    }

    /// 保存到本地存储
    pub fn save_to_storage(&self) -> Result<(), AuthError> {
        let json =
            serde_json::to_string(self).map_err(|e| AuthError::Serialization(e.to_string()))?;

        LocalStorage::set(TOKEN_STORAGE_KEY, json)
            .map_err(|e| AuthError::Storage(e.to_string()))?;

        Ok(())
    }

    /// 从本地存储加载
    pub fn load_from_storage() -> Result<Option<Self>, AuthError> {
        let json: Result<String, _> = LocalStorage::get(TOKEN_STORAGE_KEY);

        match json {
            Ok(json) => {
                let auth: GatewayAuth = serde_json::from_str(&json)
                    .map_err(|e| AuthError::Serialization(e.to_string()))?;
                Ok(Some(auth))
            }
            Err(_) => Ok(None),
        }
    }

    /// 清除存储
    pub fn clear_storage() {
        LocalStorage::delete(TOKEN_STORAGE_KEY);
        LocalStorage::delete(REFRESH_TOKEN_KEY);
    }
}

/// Token 管理器
#[derive(Clone, Debug)]
pub struct TokenManager {
    auth: Option<GatewayAuth>,
    auto_refresh: bool,
    refresh_threshold_seconds: i64,
}

impl TokenManager {
    /// 创建新的 Token 管理器
    pub fn new() -> Self {
        Self {
            auth: None,
            auto_refresh: true,
            refresh_threshold_seconds: 300, // 5 minutes
        }
    }

    /// 设置认证信息
    pub fn set_auth(&mut self, auth: GatewayAuth) {
        let _ = auth.save_to_storage();
        self.auth = Some(auth);
    }

    /// 获取认证信息
    pub fn get_auth(&self) -> Option<&GatewayAuth> {
        self.auth.as_ref()
    }

    /// 获取访问令牌
    pub fn get_token(&self) -> Option<String> {
        self.auth.as_ref().map(|a| a.access_token.clone())
    }

    /// 获取 Authorization 头部
    pub fn get_authorization_header(&self) -> Option<String> {
        self.auth.as_ref().map(|a| a.authorization_header())
    }

    /// 检查是否已认证
    pub fn is_authenticated(&self) -> bool {
        self.auth.is_some()
    }

    /// 检查是否需要刷新
    pub fn needs_refresh(&self) -> bool {
        if let Some(auth) = &self.auth {
            if let Some(expires_at) = &auth.expires_at {
                if let Ok(expires) = chrono::DateTime::parse_from_rfc3339(expires_at) {
                    let now = chrono::Utc::now();
                    let threshold = chrono::Duration::seconds(self.refresh_threshold_seconds);
                    return now + threshold > expires;
                }
            }
        }
        false
    }

    /// 清除认证
    pub fn clear(&mut self) {
        GatewayAuth::clear_storage();
        self.auth = None;
    }

    /// 从存储加载
    pub fn load_from_storage(&mut self) -> Result<(), AuthError> {
        self.auth = GatewayAuth::load_from_storage()?;
        Ok(())
    }

    /// 设置自动刷新
    pub fn set_auto_refresh(&mut self, enabled: bool) {
        self.auto_refresh = enabled;
    }

    /// 刷新令牌
    pub async fn refresh_token(&mut self) -> Result<(), AuthError> {
        if let Some(auth) = &self.auth {
            if let Some(refresh_token) = &auth.refresh_token {
                // 调用刷新 API
                // 这里需要实现实际的刷新逻辑
                let _ = refresh_token;

                // 模拟刷新成功
                return Ok(());
            }
        }

        Err(AuthError::NoRefreshToken)
    }
}

impl Default for TokenManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 认证错误
#[derive(Clone, Debug)]
pub enum AuthError {
    Serialization(String),
    Storage(String),
    RefreshFailed(String),
    NoRefreshToken,
    InvalidToken,
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            AuthError::Storage(msg) => write!(f, "Storage error: {}", msg),
            AuthError::RefreshFailed(msg) => write!(f, "Token refresh failed: {}", msg),
            AuthError::NoRefreshToken => write!(f, "No refresh token available"),
            AuthError::InvalidToken => write!(f, "Invalid token"),
        }
    }
}

/// 登录请求
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub device_id: Option<String>,
}

/// 登录响应
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub token_type: String,
    pub user: UserInfo,
}

/// 用户信息
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

/// Token 刷新请求
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenRefreshRequest {
    pub refresh_token: String,
}

/// Token 刷新响应
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenRefreshResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub expires_in: i64,
    pub token_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_auth_creation() {
        let auth = GatewayAuth::new("test-token")
            .with_refresh_token("refresh-token")
            .with_expires_at("2026-12-31T23:59:59Z");

        assert_eq!(auth.access_token, "test-token");
        assert_eq!(auth.refresh_token, Some("refresh-token".to_string()));
        assert_eq!(auth.authorization_header(), "Bearer test-token");
    }

    #[test]
    #[cfg(target_arch = "wasm32")]
    fn test_token_manager() {
        let mut manager = TokenManager::new();
        assert!(!manager.is_authenticated());

        let auth = GatewayAuth::new("test-token");
        manager.set_auth(auth);

        assert!(manager.is_authenticated());
        assert_eq!(manager.get_token(), Some("test-token".to_string()));
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_token_manager() {
        // In non-wasm environment, we can't test storage functionality
        // Just test basic TokenManager functionality
        let manager = TokenManager::new();
        assert!(!manager.is_authenticated());
        assert_eq!(manager.get_token(), None);
    }
}
