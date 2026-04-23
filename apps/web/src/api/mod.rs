//! BeeBotOS Web API Module
//!
//! 此模块提供与 Gateway 的完整 API 对接：
//! - 浏览器自动化 API
//! - WebChat API
//! - Gateway API 配置
//! - 通用 API 客户端

pub mod browser;
pub mod client;
pub mod gateway;
pub mod llm_provider_service;
pub mod services;
pub mod webchat;

// Re-export 浏览器 API
pub use browser::{BrowserApiService, ClickRequest, EvaluateResponse, InputRequest, NavigationResponse};

// Re-export WebChat API
pub use webchat::{
    EditMessageRequest, ExportResponse, SideQuestionResponse, SlashCommandRequest,
    SlashCommandResponse, StreamingResponse, UploadAttachmentRequest, UploadAttachmentResponse,
    WebchatApiService,
};

// Re-export Gateway API
pub use gateway::{
    AgentStatusInfo, ApiEndpoints, GatewayApiConfig, GatewayAuthConfig, GatewayService,
    HealthResponse, StatusResponse,
};

// Re-export 通用客户端
pub use client::{
    sanitize_for_log, ApiClient, ApiError, ApiResponse, ClientConfig, RequestBuilder,
    RequestInterceptor, ResponseInterceptor,
};

// Re-export 服务
pub use services::{
    AgentInfo, AgentLogEntry, AgentService, AgentStatus, ApiService, AssetInfo, AuthService,
    ChannelConfig, ChannelInfo, ChannelService, ChannelStatus, CreateAgentRequest,
    CreateInstanceRequest, CreateProposalRequest, DaoService, DaoSummary, ExecuteSkillResponse,
    InstanceInfo, InstallSkillRequest, InstallSkillResponse, LoginResponse, LlmConfigService,
    LlmGlobalConfig, LlmHealthResponse, LlmLatency, LlmMetricsResponse, LlmProviderConfig,
    LlmProviderHealth, LlmSummary, LlmTokens, PaginatedResponse, ProposalInfo, ProposalStatus,
    QrStatusResponse, Settings, SettingsService, SkillCategory, SkillInfo, SkillService,
    TestConnectionResponse, Theme, TokenRefreshResponse, TransactionInfo, TransactionStatus,
    TransactionType, TreasuryInfo, TreasuryService, UpdateAgentRequest, UserInfo, WeChatQrResponse,
};

/// 创建默认 API 客户端
pub fn create_client() -> ApiClient {
    ApiClient::default_client()
}

/// 创建带自定义配置的 API 客户端
pub fn create_client_with_config(config: ClientConfig) -> ApiClient {
    ApiClient::new(config)
}

/// 创建浏览器 API 服务
pub fn create_browser_service(client: ApiClient) -> BrowserApiService {
    BrowserApiService::new(client)
}

/// 创建 WebChat API 服务
pub fn create_webchat_service(client: ApiClient) -> WebchatApiService {
    WebchatApiService::new(client)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_client() {
        let client = create_client();
        // 验证客户端创建成功
        let _ = client;
    }

    #[test]
    fn test_service_creation() {
        let client = create_client();
        let _browser_service = create_browser_service(client.clone());
        let _webchat_service = create_webchat_service(client);
    }
}
