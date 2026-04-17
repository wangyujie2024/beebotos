//! API Service implementations using the advanced ApiClient

use super::client::{ApiClient, ApiError};
use serde::{Deserialize, Serialize};

/// API Service trait
pub trait ApiService {
    fn client(&self) -> &ApiClient;
}

/// Agent API Service
#[derive(Clone)]
pub struct AgentService {
    client: ApiClient,
}

impl AgentService {
    pub fn new(client: ApiClient) -> Self {
        Self { client }
    }

    pub async fn list(&self) -> Result<Vec<AgentInfo>, ApiError> {
        self.client.get("/agents").await
    }

    pub async fn get(&self, id: &str) -> Result<AgentInfo, ApiError> {
        self.client.get(&format!("/agents/{}", id)).await
    }

    pub async fn create(&self, req: CreateAgentRequest) -> Result<AgentInfo, ApiError> {
        self.client.post("/agents", &req).await
    }

    pub async fn update(&self, id: &str, req: UpdateAgentRequest) -> Result<AgentInfo, ApiError> {
        self.client.put(&format!("/agents/{}", id), &req).await
    }

    pub async fn delete(&self, id: &str) -> Result<(), ApiError> {
        self.client.delete(&format!("/agents/{}", id)).await
    }

    pub async fn start(&self, id: &str) -> Result<(), ApiError> {
        self.client
            .post(&format!("/agents/{}/start", id), &serde_json::json!({}))
            .await
    }

    pub async fn stop(&self, id: &str) -> Result<(), ApiError> {
        self.client
            .post(&format!("/agents/{}/stop", id), &serde_json::json!({}))
            .await
    }

    /// Invalidate agent list cache
    pub fn invalidate_cache(&self) {
        self.client.invalidate_cache("GET:/agents");
    }

    /// Invalidate specific agent cache
    pub fn invalidate_agent_cache(&self, id: &str) {
        self.client.invalidate_cache(&format!("GET:/agents/{}", id));
    }
}

impl ApiService for AgentService {
    fn client(&self) -> &ApiClient {
        &self.client
    }
}

/// Skill API Service
#[derive(Clone)]
pub struct SkillService {
    client: ApiClient,
}

impl SkillService {
    pub fn new(client: ApiClient) -> Self {
        Self { client }
    }

    pub async fn list(&self) -> Result<Vec<SkillInfo>, ApiError> {
        self.client.get("/skills").await
    }

    pub async fn install(&self, skill_id: &str) -> Result<(), ApiError> {
        self.client
            .post(
                &format!("/skills/{}/install", skill_id),
                &serde_json::json!({}),
            )
            .await
    }

    pub async fn uninstall(&self, skill_id: &str) -> Result<(), ApiError> {
        self.client.delete(&format!("/skills/{}", skill_id)).await
    }
}

impl ApiService for SkillService {
    fn client(&self) -> &ApiClient {
        &self.client
    }
}

/// DAO API Service
#[derive(Clone)]
pub struct DaoService {
    client: ApiClient,
}

impl DaoService {
    pub fn new(client: ApiClient) -> Self {
        Self { client }
    }

    pub async fn get_summary(&self) -> Result<DaoSummary, ApiError> {
        self.client.get("/dao/summary").await
    }

    pub async fn list_proposals(&self) -> Result<Vec<ProposalInfo>, ApiError> {
        self.client.get("/dao/proposals").await
    }

    pub async fn get_proposal(&self, id: &str) -> Result<ProposalInfo, ApiError> {
        self.client.get(&format!("/dao/proposals/{}", id)).await
    }

    pub async fn vote(
        &self,
        proposal_id: &str,
        vote_for: bool,
        voting_power: u64,
    ) -> Result<(), ApiError> {
        self.client
            .post(
                &format!("/dao/proposals/{}/vote", proposal_id),
                &serde_json::json!({
                    "vote_for": vote_for,
                    "voting_power": voting_power
                }),
            )
            .await
    }

    pub async fn create_proposal(
        &self,
        req: CreateProposalRequest,
    ) -> Result<ProposalInfo, ApiError> {
        self.client.post("/dao/proposals", &req).await
    }

    /// Invalidate proposals cache after voting
    pub fn invalidate_proposals_cache(&self) {
        self.client.invalidate_cache("GET:/dao/proposals");
        self.client.invalidate_cache("GET:/dao/summary");
    }
}

impl ApiService for DaoService {
    fn client(&self) -> &ApiClient {
        &self.client
    }
}

/// Treasury API Service
#[derive(Clone)]
pub struct TreasuryService {
    client: ApiClient,
}

impl TreasuryService {
    pub fn new(client: ApiClient) -> Self {
        Self { client }
    }

    pub async fn get_info(&self) -> Result<TreasuryInfo, ApiError> {
        self.client.get("/treasury").await
    }
}

impl ApiService for TreasuryService {
    fn client(&self) -> &ApiClient {
        &self.client
    }
}

/// Settings API Service
#[derive(Clone)]
pub struct SettingsService {
    client: ApiClient,
}

impl SettingsService {
    pub fn new(client: ApiClient) -> Self {
        Self { client }
    }

    pub async fn get(&self) -> Result<Settings, ApiError> {
        self.client.get("/settings").await
    }

    pub async fn update(&self, settings: Settings) -> Result<Settings, ApiError> {
        self.client.put("/settings", &settings).await
    }
}

impl ApiService for SettingsService {
    fn client(&self) -> &ApiClient {
        &self.client
    }
}

/// Auth API Service
#[derive(Clone)]
pub struct AuthService {
    client: ApiClient,
}

impl AuthService {
    pub fn new(client: ApiClient) -> Self {
        Self { client }
    }

    pub async fn login(&self, username: &str, password: &str) -> Result<LoginResponse, ApiError> {
        let req = LoginRequest {
            username: username.to_string(),
            password: password.to_string(),
        };
        self.client.post("/auth/login", &req).await
    }

    pub async fn register(
        &self,
        username: &str,
        email: &str,
        password: &str,
    ) -> Result<LoginResponse, ApiError> {
        let req = RegisterRequest {
            username: username.to_string(),
            email: Some(email.to_string()),
            password: password.to_string(),
        };
        self.client.post("/auth/register", &req).await
    }

    pub async fn logout(&self) -> Result<(), ApiError> {
        self.client
            .post("/auth/logout", &serde_json::json!({}))
            .await
    }

    pub async fn refresh_token(
        &self,
        refresh_token: &str,
    ) -> Result<TokenRefreshResponse, ApiError> {
        self.client
            .post(
                "/auth/refresh",
                &serde_json::json!({
                    "refresh_token": refresh_token
                }),
            )
            .await
    }

    pub async fn get_current_user(&self) -> Result<UserInfo, ApiError> {
        self.client.get("/auth/me").await
    }
}

impl ApiService for AuthService {
    fn client(&self) -> &ApiClient {
        &self.client
    }
}

/// Channel API Service
#[derive(Clone)]
pub struct ChannelService {
    client: ApiClient,
}

impl ChannelService {
    pub fn new(client: ApiClient) -> Self {
        Self { client }
    }

    /// List all channels
    pub async fn list(&self) -> Result<Vec<ChannelInfo>, ApiError> {
        self.client.get("/channels").await
    }

    /// Get channel by ID
    pub async fn get(&self, id: &str) -> Result<ChannelInfo, ApiError> {
        self.client.get(&format!("/channels/{}", id)).await
    }

    /// Update channel configuration
    pub async fn update(&self, id: &str, config: ChannelConfig) -> Result<ChannelInfo, ApiError> {
        self.client.put(&format!("/channels/{}", id), &config).await
    }

    /// Enable/disable channel
    pub async fn set_enabled(&self, id: &str, enabled: bool) -> Result<ChannelInfo, ApiError> {
        self.client
            .post(&format!("/channels/{}/enable", id), &serde_json::json!({ "enabled": enabled }))
            .await
    }

    /// Test channel connection
    pub async fn test_connection(&self, id: &str) -> Result<TestConnectionResponse, ApiError> {
        self.client
            .post(&format!("/channels/{}/test", id), &serde_json::json!({}))
            .await
    }

    /// Get WeChat QR code for login
    pub async fn get_wechat_qr(&self) -> Result<WeChatQrResponse, ApiError> {
        self.client.post("/channels/wechat/qr", &serde_json::json!({})).await
    }

    /// Check WeChat QR scan status
    pub async fn check_wechat_qr(&self, qr_code: &str) -> Result<QrStatusResponse, ApiError> {
        self.client
            .post("/channels/wechat/qr/check", &serde_json::json!({ "qr_code": qr_code }))
            .await
    }
}

impl ApiService for ChannelService {
    fn client(&self) -> &ApiClient {
        &self.client
    }
}

// ==================== Data Models ====================

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub status: AgentStatus,
    pub capabilities: Vec<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub task_count: Option<u32>,
    pub uptime_percent: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Running,
    Stopped,
    #[default]
    Idle,
    Error,
    Pending,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub description: Option<String>,
    pub capabilities: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub capabilities: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SkillInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub category: SkillCategory,
    pub installed: bool,
    pub downloads: u64,
    pub rating: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SkillCategory {
    Trading,
    Data,
    Social,
    Automation,
    Analysis,
    Other,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DaoSummary {
    pub member_count: u64,
    pub total_voting_power: u64,
    pub user_voting_power: u64,
    pub active_proposals: u32,
    pub total_proposals: u32,
    pub token_symbol: String,
    pub token_balance: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProposalInfo {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: ProposalStatus,
    pub proposer: String,
    pub created_at: String,
    pub ends_at: String,
    pub votes_for: u64,
    pub votes_against: u64,
    pub user_voted: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ProposalStatus {
    Active,
    Passed,
    Rejected,
    Executed,
    Pending,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateProposalRequest {
    pub title: String,
    pub description: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TreasuryInfo {
    pub total_balance: u64,
    pub token_symbol: String,
    pub assets: Vec<AssetInfo>,
    pub recent_transactions: Vec<TransactionInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AssetInfo {
    pub token: String,
    pub balance: u64,
    pub value_usd: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TransactionInfo {
    pub id: String,
    pub tx_type: TransactionType,
    pub amount: u64,
    pub token: String,
    pub from: String,
    pub to: String,
    pub timestamp: String,
    pub status: TransactionStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TransactionType {
    Deposit,
    Withdrawal,
    Transfer,
    Swap,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TransactionStatus {
    Pending,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub theme: Theme,
    pub language: String,
    pub notifications_enabled: bool,
    pub auto_update: bool,
    pub api_endpoint: Option<String>,
    pub wallet_address: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
    System,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: Option<String>,
    pub password: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub user: UserInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenRefreshResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>, // Optional: supports refresh token rotation
    pub expires_in: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub avatar: Option<String>,
    pub wallet_address: Option<String>,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::{LoginRequest, RegisterRequest};

    #[test]
    fn test_login_request_serialization() {
        let req = LoginRequest {
            username: "alice".to_string(),
            password: "secret".to_string(),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["username"], "alice");
        assert_eq!(json["password"], "secret");
    }

    #[test]
    fn test_register_request_serialization() {
        let req = RegisterRequest {
            username: "bob".to_string(),
            email: Some("bob@example.com".to_string()),
            password: "password123".to_string(),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["username"], "bob");
        assert_eq!(json["email"], "bob@example.com");
        assert_eq!(json["password"], "password123");
    }

    #[test]
    fn test_register_request_without_email() {
        let req = RegisterRequest {
            username: "bob".to_string(),
            email: None,
            password: "password123".to_string(),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["username"], "bob");
        assert!(json["email"].is_null());
        assert_eq!(json["password"], "password123");
    }
}

// ==================== Channel Models ====================

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ChannelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub enabled: bool,
    pub status: ChannelStatus,
    pub config: Option<ChannelConfig>,
    pub last_error: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ChannelStatus {
    Connected,
    Disconnected,
    Connecting,
    Error,
    Disabled,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ChannelConfig {
    pub base_url: Option<String>,
    pub bot_token: Option<String>,
    pub bot_base_url: Option<String>,
    pub auto_reconnect: Option<bool>,
    pub reconnect_interval_secs: Option<u64>,
    pub webhook_url: Option<String>,
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    pub extra: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TestConnectionResponse {
    pub success: bool,
    pub message: String,
    pub latency_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WeChatQrResponse {
    pub qrcode: String,
    pub qrcode_img_content: Option<String>,
    pub expires_in: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QrStatusResponse {
    pub status: String, // "pending", "scanned", "confirmed", "expired"
    pub bot_token: Option<String>,
    pub base_url: Option<String>,
    pub message: Option<String>,
}
