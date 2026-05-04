//! Gateway API 配置模块
//!
//! 提供 Gateway API 端点配置和连接管理

use crate::gateway::{GatewayConfig, GatewayScope};
use serde::{Deserialize, Serialize};

/// Gateway API 端点配置
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GatewayApiConfig {
    /// API 基础 URL
    pub base_url: String,
    /// WebSocket URL
    pub websocket_url: String,
    /// 认证配置
    pub auth: GatewayAuthConfig,
    /// 超时配置（毫秒）
    pub timeout_ms: u64,
    /// 重试次数
    pub retry_attempts: u32,
}

impl Default for GatewayApiConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:8080/api/v1".to_string(),
            websocket_url: "ws://localhost:8080/ws".to_string(),
            auth: GatewayAuthConfig::default(),
            timeout_ms: 30000,
            retry_attempts: 3,
        }
    }
}

impl GatewayApiConfig {
    /// 从 GatewayConfig 创建
    pub fn from_gateway_config(config: &GatewayConfig) -> Self {
        Self {
            base_url: config.api_base_url.clone(),
            websocket_url: config.websocket_url.clone(),
            auth: GatewayAuthConfig::from_gateway_auth(&config.auth),
            timeout_ms: config.connection.connection_timeout_ms,
            retry_attempts: config.connection.max_reconnect_attempts,
        }
    }

    /// 设置 API 基础 URL
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// 设置 WebSocket URL
    pub fn with_websocket_url(mut self, url: impl Into<String>) -> Self {
        self.websocket_url = url.into();
        self
    }

    /// 构建完整 API URL
    pub fn build_url(&self, endpoint: &str) -> String {
        format!("{}{}", self.base_url, endpoint)
    }

    /// 获取健康检查 URL
    pub fn health_url(&self) -> String {
        self.build_url("/health")
    }

    /// 获取状态 URL
    pub fn status_url(&self) -> String {
        self.build_url("/status")
    }
}

/// Gateway 认证配置
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GatewayAuthConfig {
    /// 权限范围
    pub scopes: Vec<GatewayScope>,
    /// 允许无设备连接
    pub allow_deviceless: bool,
    /// 会话超时（秒）
    pub session_timeout_seconds: u64,
}

impl Default for GatewayAuthConfig {
    fn default() -> Self {
        Self {
            scopes: vec![
                GatewayScope::BrowserRead,
                GatewayScope::BrowserWrite,
                GatewayScope::ChatRead,
                GatewayScope::ChatWrite,
            ],
            allow_deviceless: false,
            session_timeout_seconds: 3600,
        }
    }
}

impl GatewayAuthConfig {
    /// 从 Gateway Auth Config 创建
    pub fn from_gateway_auth(auth: &crate::gateway::GatewayAuthConfig) -> Self {
        Self {
            scopes: auth.scopes.clone(),
            allow_deviceless: auth.allow_deviceless,
            session_timeout_seconds: auth.session_timeout_seconds,
        }
    }

    /// 检查是否有指定权限
    pub fn has_scope(&self, scope: GatewayScope) -> bool {
        self.scopes.contains(&scope) || self.scopes.contains(&GatewayScope::Admin)
    }

    /// 添加权限
    pub fn add_scope(&mut self, scope: GatewayScope) {
        if !self.scopes.contains(&scope) {
            self.scopes.push(scope);
        }
    }

    /// 移除权限
    pub fn remove_scope(&mut self, scope: &GatewayScope) {
        self.scopes.retain(|s| s != scope);
    }
}

/// API 端点定义
pub struct ApiEndpoints;

impl ApiEndpoints {
    // 健康检查
    pub const HEALTH: &'static str = "/health";
    pub const STATUS: &'static str = "/status";
    pub const READY: &'static str = "/ready";
    pub const LIVE: &'static str = "/live";

    // Agent API
    pub const AGENTS: &'static str = "/agents";
    pub const AGENT_DETAIL: &'static str = "/agents/"; // + id
    pub const AGENT_START: &'static str = "/agents/{id}/start";
    pub const AGENT_STOP: &'static str = "/agents/{id}/stop";
    pub const AGENT_TASKS: &'static str = "/agents/{id}/tasks";

    // Browser API
    pub const BROWSER_PROFILES: &'static str = "/browser/profiles";
    pub const BROWSER_PROFILE_DETAIL: &'static str = "/browser/profiles/"; // + id
    pub const BROWSER_CONNECT: &'static str = "/browser/connect";
    pub const BROWSER_DISCONNECT: &'static str = "/browser/disconnect";
    pub const BROWSER_NAVIGATE: &'static str = "/browser/navigate";
    pub const BROWSER_EVALUATE: &'static str = "/browser/evaluate";
    pub const BROWSER_BATCH: &'static str = "/browser/batch";
    pub const BROWSER_SCREENSHOT: &'static str = "/browser/screenshot";
    pub const BROWSER_SANDBOXES: &'static str = "/browser/sandboxes";
    pub const BROWSER_SANDBOX_DETAIL: &'static str = "/browser/sandboxes/"; // + id

    // WebChat API
    pub const WEBCHAT_SESSIONS: &'static str = "/webchat/sessions";
    pub const WEBCHAT_SESSION_DETAIL: &'static str = "/webchat/sessions/"; // + id
    pub const WEBCHAT_SESSION_MESSAGES: &'static str = "/webchat/sessions/{id}/messages";
    pub const WEBCHAT_SESSION_PIN: &'static str = "/webchat/sessions/{id}/pin";
    pub const WEBCHAT_USAGE: &'static str = "/webchat/usage";
    pub const WEBCHAT_SIDE_QUESTIONS: &'static str = "/webchat/side-questions";

    // DAO API
    pub const DAO_PROPOSALS: &'static str = "/dao/proposals";
    pub const DAO_PROPOSAL_DETAIL: &'static str = "/dao/proposals/"; // + id
    pub const DAO_VOTE: &'static str = "/dao/vote";
    pub const DAO_TREASURY: &'static str = "/dao/treasury";

    // Chain API
    pub const CHAIN_STATUS: &'static str = "/chain/status";
    pub const CHAIN_WALLET: &'static str = "/chain/wallet";
    pub const CHAIN_TRANSFER: &'static str = "/chain/wallet/transfer";

    // Skills API
    pub const SKILLS: &'static str = "/skills";
    pub const SKILL_INSTALL: &'static str = "/skills/install";
    pub const SKILL_DETAIL: &'static str = "/skills/"; // + id
    pub const SKILL_UNINSTALL: &'static str = "/skills/{id}/uninstall";
    pub const SKILL_EXECUTE: &'static str = "/skills/{id}/execute";

    // Instance API
    pub const INSTANCES: &'static str = "/instances";
    pub const INSTANCE_DETAIL: &'static str = "/instances/"; // + id
    pub const INSTANCE_EXECUTE: &'static str = "/instances/{id}/execute";

    // Workflow API (v2)
    pub const WORKFLOWS: &'static str = "/workflows";
    pub const WORKFLOW_DETAIL: &'static str = "/workflows/"; // + id
    pub const WORKFLOW_EXECUTE: &'static str = "/workflows/{id}/execute";
    pub const WORKFLOW_INSTALL: &'static str = "/workflows/install";
    pub const WORKFLOW_UNINSTALL: &'static str = "/workflows/{id}/uninstall";
    pub const WORKFLOW_SOURCE: &'static str = "/workflows/"; // + id + /source
    pub const WORKFLOW_DASHBOARD_STATS: &'static str = "/workflows/dashboard/stats";
    pub const WORKFLOW_DASHBOARD_RECENT: &'static str = "/workflows/dashboard/recent-instances";
    pub const WORKFLOW_STATS: &'static str = "/workflows/"; // + id + /stats

    // Composition API (v2)
    pub const COMPOSITIONS: &'static str = "/compositions";
    pub const COMPOSITION_DETAIL: &'static str = "/compositions/"; // + id
    pub const COMPOSITION_EXECUTE: &'static str = "/compositions/{id}/execute";
}

/// Gateway 服务
#[derive(Clone)]
pub struct GatewayService {
    config: GatewayApiConfig,
    client: super::ApiClient,
}

impl GatewayService {
    pub fn new(client: super::ApiClient, config: GatewayApiConfig) -> Self {
        Self { config, client }
    }

    /// 检查 Gateway 健康状态
    pub async fn health_check(&self) -> Result<HealthResponse, super::ApiError> {
        self.client.get(ApiEndpoints::HEALTH).await
    }

    /// 获取 Gateway 状态
    pub async fn get_status(&self) -> Result<StatusResponse, super::ApiError> {
        self.client.get(ApiEndpoints::STATUS).await
    }

    /// 获取配置
    pub fn config(&self) -> &GatewayApiConfig {
        &self.config
    }

    /// 更新配置
    pub fn update_config(&mut self, config: GatewayApiConfig) {
        self.config = config;
    }

    /// 构建完整 URL
    pub fn build_url(&self, endpoint: &str) -> String {
        self.config.build_url(endpoint)
    }
}

/// 健康检查响应
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: Option<String>,
    pub timestamp: Option<String>,
}

/// 状态响应
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StatusResponse {
    pub service: String,
    pub version: String,
    pub status: String,
    pub components: Option<serde_json::Value>,
    pub agents: Option<AgentStatusInfo>,
    pub websocket: Option<bool>,
    pub timestamp: String,
}

/// Agent 状态信息
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentStatusInfo {
    pub active: u32,
    pub total: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_api_config_default() {
        let config = GatewayApiConfig::default();
        assert_eq!(config.base_url, "http://localhost:8080/api/v1");
        assert_eq!(config.websocket_url, "ws://localhost:8080/ws");
        assert_eq!(config.timeout_ms, 30000);
    }

    #[test]
    fn test_build_url() {
        let config = GatewayApiConfig::default();
        let url = config.build_url("/browser/profiles");
        assert_eq!(url, "http://localhost:8080/api/v1/browser/profiles");
    }

    #[test]
    fn test_auth_config_scopes() {
        let mut auth = GatewayAuthConfig::default();
        assert!(auth.has_scope(GatewayScope::BrowserRead));
        assert!(!auth.has_scope(GatewayScope::Admin));

        auth.add_scope(GatewayScope::Admin);
        assert!(auth.has_scope(GatewayScope::Admin));
    }
}
