//! API Service implementations using the advanced ApiClient

use super::client::{ApiClient, ApiError};
use super::gateway::ApiEndpoints;
use serde::{Deserialize, Serialize};
use serde_json::json;

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

    pub async fn list(&self) -> Result<PaginatedResponse<AgentInfo>, ApiError> {
        self.client.get(ApiEndpoints::AGENTS).await
    }

    pub async fn list_paginated(&self, page: usize, per_page: usize) -> Result<PaginatedResponse<AgentInfo>, ApiError> {
        self.client.get(&format!("{}?page={}&per_page={}", ApiEndpoints::AGENTS, page, per_page)).await
    }

    pub async fn get(&self, id: &str) -> Result<AgentInfo, ApiError> {
        self.client.get(&format!("{}{}", ApiEndpoints::AGENT_DETAIL, js_sys::encode_uri_component(id))).await
    }

    pub async fn get_logs(&self, id: &str) -> Result<Vec<AgentLogEntry>, ApiError> {
        self.client.get(&format!("{}{}/logs", ApiEndpoints::AGENT_DETAIL, js_sys::encode_uri_component(id))).await
    }

    pub async fn create(&self, req: CreateAgentRequest) -> Result<AgentInfo, ApiError> {
        self.client.post(ApiEndpoints::AGENTS, &req).await
    }

    pub async fn update(&self, id: &str, req: UpdateAgentRequest) -> Result<AgentInfo, ApiError> {
        self.client.put(&format!("{}{}", ApiEndpoints::AGENT_DETAIL, js_sys::encode_uri_component(id)), &req).await
    }

    pub async fn delete(&self, id: &str) -> Result<(), ApiError> {
        self.client.delete(&format!("{}{}", ApiEndpoints::AGENT_DETAIL, js_sys::encode_uri_component(id))).await
    }

    pub async fn start(&self, id: &str) -> Result<serde_json::Value, ApiError> {
        self.client
            .post(&ApiEndpoints::AGENT_START.replace("{id}", id), &serde_json::json!({}))
            .await
    }

    pub async fn stop(&self, id: &str) -> Result<serde_json::Value, ApiError> {
        self.client
            .post(&ApiEndpoints::AGENT_STOP.replace("{id}", id), &serde_json::json!({}))
            .await
    }

    /// Invalidate agent list cache
    pub fn invalidate_cache(&self) {
        self.client.invalidate_cache(&format!("GET:{}", ApiEndpoints::AGENTS));
    }

    /// Invalidate specific agent cache
    pub fn invalidate_agent_cache(&self, id: &str) {
        self.client.invalidate_cache(&format!("GET:{}{}", ApiEndpoints::AGENT_DETAIL, js_sys::encode_uri_component(id)));
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

    pub async fn list(&self, hub: Option<&str>, search: Option<&str>) -> Result<Vec<SkillInfo>, ApiError> {
        let mut path = ApiEndpoints::SKILLS.to_string();
        let mut params = Vec::new();
        if let Some(h) = hub {
            let encoded = js_sys::encode_uri_component(h);
            params.push(format!("hub={}", encoded));
        }
        if let Some(s) = search {
            let encoded = js_sys::encode_uri_component(s);
            params.push(format!("search={}", encoded));
        }
        if !params.is_empty() {
            path.push('?');
            path.push_str(&params.join("&"));
        }
        self.client.get(&path).await
    }

    pub async fn install(&self, req: InstallSkillRequest) -> Result<InstallSkillResponse, ApiError> {
        self.client.post(ApiEndpoints::SKILL_INSTALL, &req).await
    }

    pub async fn uninstall(&self, skill_id: &str) -> Result<(), ApiError> {
        self.client.delete(&ApiEndpoints::SKILL_UNINSTALL.replace("{id}", skill_id)).await
    }

    pub async fn execute(&self, skill_id: &str, input: serde_json::Value) -> Result<ExecuteSkillResponse, ApiError> {
        self.client.post(&ApiEndpoints::SKILL_EXECUTE.replace("{id}", skill_id), &json!({ "input": input })).await
    }

    // Instance-based skill management
    pub async fn list_instances(&self) -> Result<Vec<InstanceInfo>, ApiError> {
        self.client.get(ApiEndpoints::INSTANCES).await
    }

    pub async fn get_instance(&self, instance_id: &str) -> Result<InstanceInfo, ApiError> {
        self.client.get(&format!("{}{}", ApiEndpoints::INSTANCE_DETAIL, js_sys::encode_uri_component(instance_id))).await
    }

    pub async fn create_instance(&self, req: CreateInstanceRequest) -> Result<InstanceInfo, ApiError> {
        self.client.post(ApiEndpoints::INSTANCES, &req).await
    }

    pub async fn delete_instance(&self, instance_id: &str) -> Result<(), ApiError> {
        self.client.delete(&format!("{}{}", ApiEndpoints::INSTANCE_DETAIL, js_sys::encode_uri_component(instance_id))).await
    }

    pub async fn execute_instance(&self, instance_id: &str) -> Result<ExecuteSkillResponse, ApiError> {
        self.client.post(&ApiEndpoints::INSTANCE_EXECUTE.replace("{id}", instance_id), &json!({})).await
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
        self.client.get("/chain/dao/summary").await
    }

    pub async fn list_proposals(&self) -> Result<Vec<ProposalInfo>, ApiError> {
        self.client.get("/chain/dao/proposals").await
    }

    pub async fn get_proposal(&self, id: &str) -> Result<ProposalInfo, ApiError> {
        self.client.get(&format!("/chain/dao/proposals/{}", js_sys::encode_uri_component(id))).await
    }

    pub async fn vote(
        &self,
        proposal_id: &str,
        vote_for: bool,
        _voting_power: u64,
    ) -> Result<serde_json::Value, ApiError> {
        self.client
            .post(
                &format!("/chain/dao/proposals/{}/vote", js_sys::encode_uri_component(proposal_id)),
                &serde_json::json!({
                    "vote": if vote_for { "for" } else { "against" },
                }),
            )
            .await
    }

    pub async fn create_proposal(
        &self,
        req: CreateProposalRequest,
    ) -> Result<serde_json::Value, ApiError> {
        self.client.post("/chain/dao/proposals", &req).await
    }

    /// Invalidate proposals cache after voting
    pub fn invalidate_proposals_cache(&self) {
        self.client.invalidate_cache("GET:/chain/dao/proposals");
        self.client.invalidate_cache("GET:/chain/dao/summary");
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

    pub async fn transfer(&self, to: &str, amount: &str) -> Result<serde_json::Value, ApiError> {
        self.client.post("/treasury/transfer", &serde_json::json!({
            "to": to,
            "amount": amount,
        })).await
    }
}

impl ApiService for TreasuryService {
    fn client(&self) -> &ApiClient {
        &self.client
    }
}

/// Workflow trigger info
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkflowTriggerInfo {
    pub trigger_type: String,
    pub detail: String,
}

/// Workflow step info (for DAG visualization)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkflowStepInfo {
    pub id: String,
    pub name: String,
    pub skill: String,
    pub depends_on: Option<Vec<String>>,
    pub condition: Option<String>,
}

/// Workflow info
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkflowInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: Option<String>,
    pub tags: Vec<String>,
    pub steps_count: usize,
    pub triggers: Vec<WorkflowTriggerInfo>,
    pub steps: Vec<WorkflowStepInfo>,
}

/// Workflow instance summary
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkflowInstanceSummary {
    pub instance_id: String,
    pub workflow_id: String,
    pub workflow_name: String,
    pub status: String,
    pub completion_pct: f32,
    pub duration_secs: u64,
    pub started_at: String,
    pub step_count: usize,
}

/// Dashboard stats response
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DashboardStats {
    pub total_workflows: usize,
    pub total_instances: usize,
    pub completed: usize,
    pub failed: usize,
    pub running: usize,
    pub pending: usize,
}

/// Workflow execution request
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecuteWorkflowRequest {
    #[serde(default)]
    pub context: serde_json::Value,
    pub agent_id: Option<String>,
}

/// Workflow execution response
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkflowExecutionResponse {
    pub instance_id: String,
    pub workflow_id: String,
    pub status: String,
    pub message: String,
}

/// Install workflow request
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstallWorkflowRequest {
    pub source_path: String,
}

/// Install workflow response
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstallWorkflowResponse {
    pub success: bool,
    pub id: String,
    pub name: String,
    pub message: String,
    pub installed_path: String,
}

/// Workflow source response
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkflowSourceResponse {
    pub yaml: String,
}

/// Workflow API Service
#[derive(Clone)]
pub struct WorkflowService {
    client: ApiClient,
}

impl WorkflowService {
    pub fn new(client: ApiClient) -> Self {
        Self { client }
    }

    pub async fn list(&self) -> Result<Vec<WorkflowInfo>, ApiError> {
        self.client.get(ApiEndpoints::WORKFLOWS).await
    }

    pub async fn get(&self, id: &str) -> Result<WorkflowInfo, ApiError> {
        self.client.get(&format!("{}{}", ApiEndpoints::WORKFLOW_DETAIL, js_sys::encode_uri_component(id))).await
    }

    pub async fn execute(&self, id: &str, req: &ExecuteWorkflowRequest) -> Result<WorkflowExecutionResponse, ApiError> {
        self.client.post(&format!("{}{}", ApiEndpoints::WORKFLOW_EXECUTE, js_sys::encode_uri_component(id)), req).await
    }

    pub async fn dashboard_stats(&self) -> Result<DashboardStats, ApiError> {
        self.client.get(ApiEndpoints::WORKFLOW_DASHBOARD_STATS).await
    }

    pub async fn recent_instances(&self, limit: usize) -> Result<Vec<WorkflowInstanceSummary>, ApiError> {
        self.client.get(&format!("{}?limit={}", ApiEndpoints::WORKFLOW_DASHBOARD_RECENT, limit)).await
    }

    pub async fn workflow_stats(&self, id: &str) -> Result<serde_json::Value, ApiError> {
        self.client.get(&format!("{}{}", ApiEndpoints::WORKFLOW_STATS, js_sys::encode_uri_component(id))).await
    }

    pub async fn get_source(&self, id: &str) -> Result<serde_json::Value, ApiError> {
        self.client.get(&format!("{}{}source", ApiEndpoints::WORKFLOW_SOURCE, js_sys::encode_uri_component(id))).await
    }

    pub async fn delete(&self, id: &str) -> Result<(), ApiError> {
        self.client.delete(&format!("{}{}", ApiEndpoints::WORKFLOW_DETAIL, js_sys::encode_uri_component(id))).await
    }

    pub async fn update(&self, id: &str, yaml: &str) -> Result<WorkflowInfo, ApiError> {
        let req = UpdateWorkflowRequest { yaml: yaml.to_string() };
        self.client.put(&format!("{}{}", ApiEndpoints::WORKFLOW_DETAIL, js_sys::encode_uri_component(id)), &req).await
    }

    pub async fn install(&self, req: &InstallWorkflowRequest) -> Result<InstallWorkflowResponse, ApiError> {
        self.client.post(ApiEndpoints::WORKFLOW_INSTALL, req).await
    }

    pub async fn uninstall(&self, id: &str) -> Result<serde_json::Value, ApiError> {
        self.client.post(&ApiEndpoints::WORKFLOW_UNINSTALL.replace("{id}", id), &serde_json::json!({})).await
    }
}

impl ApiService for WorkflowService {
    fn client(&self) -> &ApiClient {
        &self.client
    }
}

/// Update workflow request
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateWorkflowRequest {
    pub yaml: String,
}

// ============================================================================
// Composition API Service
// ============================================================================

/// Composition info returned by the API
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompositionInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub composition_type: String,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Create composition request
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateCompositionRequest {
    pub content: String,
    pub id: Option<String>,
}

/// Execute composition request
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ExecuteCompositionRequest {
    #[serde(default)]
    pub input: String,
    pub agent_id: Option<String>,
}

/// Composition execution response
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompositionExecutionResponse {
    pub composition_id: String,
    pub status: String,
    pub output: String,
    pub message: String,
}

/// Composition API Service
#[derive(Clone)]
pub struct CompositionService {
    client: ApiClient,
}

impl CompositionService {
    pub fn new(client: ApiClient) -> Self {
        Self { client }
    }

    pub async fn list(&self) -> Result<Vec<CompositionInfo>, ApiError> {
        self.client.get(ApiEndpoints::COMPOSITIONS).await
    }

    pub async fn get(&self, id: &str) -> Result<CompositionInfo, ApiError> {
        self.client.get(&format!("{}{}", ApiEndpoints::COMPOSITION_DETAIL, js_sys::encode_uri_component(id))).await
    }

    pub async fn create(&self, req: &CreateCompositionRequest) -> Result<CompositionInfo, ApiError> {
        self.client.post(ApiEndpoints::COMPOSITIONS, req).await
    }

    pub async fn delete(&self, id: &str) -> Result<(), ApiError> {
        self.client.delete(&format!("{}{}", ApiEndpoints::COMPOSITION_DETAIL, js_sys::encode_uri_component(id))).await
    }

    pub async fn execute(&self, id: &str, req: &ExecuteCompositionRequest) -> Result<CompositionExecutionResponse, ApiError> {
        self.client.post(&ApiEndpoints::COMPOSITION_EXECUTE.replace("{id}", id), req).await
    }
}

impl ApiService for CompositionService {
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
        self.client.get("/user/settings").await
    }

    pub async fn update(&self, settings: &Settings) -> Result<serde_json::Value, ApiError> {
        self.client.put("/user/settings", &settings).await
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
        self.client
            .post(
                "/auth/login",
                &serde_json::json!({
                    "username": username,
                    "password": password
                }),
            )
            .await
    }

    pub async fn register(
        &self,
        username: &str,
        email: Option<&str>,
        password: &str,
    ) -> Result<LoginResponse, ApiError> {
        let mut body = serde_json::json!({
            "username": username,
            "password": password
        });
        if let Some(e) = email {
            body["email"] = serde_json::json!(e);
        }
        self.client.post("/auth/register", &body).await
    }

    pub async fn logout(&self) -> Result<serde_json::Value, ApiError> {
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
        self.client.get(&format!("/channels/{}", js_sys::encode_uri_component(id))).await
    }

    /// Update channel configuration
    pub async fn update(&self, id: &str, config: ChannelConfig) -> Result<serde_json::Value, ApiError> {
        self.client.put(&format!("/channels/{}", js_sys::encode_uri_component(id)), &config).await
    }

    /// Enable/disable channel
    pub async fn set_enabled(&self, id: &str, enabled: bool) -> Result<serde_json::Value, ApiError> {
        self.client
            .post(&format!("/channels/{}/enable", js_sys::encode_uri_component(id)), &serde_json::json!({ "enabled": enabled }))
            .await
    }

    /// Test channel connection
    pub async fn test_connection(&self, id: &str) -> Result<TestConnectionResponse, ApiError> {
        self.client
            .post(&format!("/channels/{}/test", js_sys::encode_uri_component(id)), &serde_json::json!({}))
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

/// Paginated response wrapper
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub total_pages: i64,
}

/// LLM Global Configuration Service
#[derive(Clone)]
pub struct LlmConfigService {
    client: ApiClient,
}

impl LlmConfigService {
    pub fn new(client: ApiClient) -> Self {
        Self { client }
    }

    /// Get global LLM configuration (read-only, masked API keys)
    pub async fn get_config(&self) -> Result<LlmGlobalConfig, ApiError> {
        self.client.get("/llm/config").await
    }

    /// Update LLM provider configuration (model & temperature)
    pub async fn update_config(&self, req: &UpdateLlmConfigRequest) -> Result<serde_json::Value, ApiError> {
        self.client.put("/llm/config", req).await
    }

    /// Get LLM metrics
    pub async fn get_metrics(&self) -> Result<LlmMetricsResponse, ApiError> {
        self.client.get("/llm/metrics").await
    }

    /// Get LLM health status
    pub async fn get_health(&self) -> Result<LlmHealthResponse, ApiError> {
        self.client.get("/llm/health").await
    }
}

impl ApiService for LlmConfigService {
    fn client(&self) -> &ApiClient {
        &self.client
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub status: AgentStatus,
    pub capabilities: Vec<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    #[serde(default)]
    pub task_count: Option<u32>,
    #[serde(default)]
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
    #[serde(default)]
    pub model_provider: Option<String>,
    #[serde(default)]
    pub model_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub capabilities: Option<Vec<String>>,
    #[serde(default)]
    pub model_provider: Option<String>,
    #[serde(default)]
    pub model_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SkillInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub license: String,
    pub installed: bool,
    pub capabilities: Vec<String>,
    pub tags: Vec<String>,
    #[serde(default)]
    pub downloads: u64,
    #[serde(default)]
    pub rating: f32,
}

/// Install skill request (aligns with Gateway API)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstallSkillRequest {
    pub source: String,
    pub agent_id: Option<String>,
    pub version: Option<String>,
    pub hub: Option<String>,
}

/// Install skill response
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstallSkillResponse {
    pub success: bool,
    pub skill_id: String,
    pub name: String,
    pub version: String,
    pub message: String,
    pub installed_path: String,
}

/// Execute skill response
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecuteSkillResponse {
    pub success: bool,
    pub output: String,
    pub execution_time_ms: u64,
}

/// Usage stats for an instance
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct UsageStats {
    pub total_calls: u64,
    pub successful_calls: u64,
    pub failed_calls: u64,
    pub avg_latency_ms: f64,
}

/// Instance info
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct InstanceInfo {
    pub instance_id: String,
    pub skill_id: String,
    pub agent_id: String,
    pub status: String,
    #[serde(default)]
    pub config: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub started_at: i64,
    #[serde(default)]
    pub last_active: i64,
    #[serde(default)]
    pub usage: UsageStats,
}

/// Create instance request
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateInstanceRequest {
    pub skill_id: String,
    pub agent_id: String,
    #[serde(default)]
    pub config: std::collections::HashMap<String, String>,
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
    #[serde(default)]
    pub proposer: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub ends_at: String,
    pub votes_for: u64,
    pub votes_against: u64,
    #[serde(default)]
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
    pub proposal_type: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AgentLogEntry {
    pub id: i64,
    pub agent_id: String,
    pub level: String,
    pub message: String,
    pub source: Option<String>,
    pub timestamp: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TreasuryInfo {
    pub total_balance: String,
    pub token_symbol: String,
    pub assets: Vec<AssetInfo>,
    pub recent_transactions: Vec<TransactionInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AssetInfo {
    pub token: String,
    pub balance: String,
    pub value_usd: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TransactionInfo {
    pub id: String,
    pub tx_type: TransactionType,
    pub amount: String,
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

/// LLM Global Configuration
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LlmGlobalConfig {
    pub default_provider: String,
    pub fallback_chain: Vec<String>,
    pub cost_optimization: bool,
    pub max_tokens: u32,
    pub system_prompt: String,
    pub request_timeout: u64,
    pub providers: Vec<LlmProviderConfig>,
}

/// LLM Provider Configuration (masked API key)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LlmProviderConfig {
    pub name: String,
    pub api_key_masked: String,
    pub model: String,
    pub base_url: String,
    pub temperature: f32,
    pub context_window: Option<u32>,
}

/// Request to update LLM provider configuration
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UpdateLlmConfigRequest {
    pub provider: String,
    pub model: String,
    pub temperature: f32,
    pub set_default: Option<bool>,
}

/// LLM Metrics Response
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LlmMetricsResponse {
    pub summary: LlmSummary,
    pub tokens: LlmTokens,
    pub latency: LlmLatency,
    pub providers: Vec<LlmProviderHealth>,
    pub timestamp: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LlmSummary {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub success_rate_percent: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LlmTokens {
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LlmLatency {
    pub average_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LlmProviderHealth {
    pub name: String,
    pub healthy: bool,
    pub consecutive_failures: u32,
}

/// LLM Health Response
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LlmHealthResponse {
    pub status: String,
    pub providers: Vec<LlmProviderHealth>,
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
    #[serde(default)]
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

#[cfg(test)]
mod qa_tests {
    use super::*;
    use crate::api::gateway::ApiEndpoints;
    use serde_json::json;

    // ========== ApiEndpoints Path Validation ==========

    #[test]
    fn test_api_endpoints_agents() {
        assert_eq!(ApiEndpoints::AGENTS, "/agents");
        assert_eq!(ApiEndpoints::AGENT_DETAIL, "/agents/");
        assert_eq!(ApiEndpoints::AGENT_START, "/agents/{id}/start");
        assert_eq!(ApiEndpoints::AGENT_STOP, "/agents/{id}/stop");
    }

    #[test]
    fn test_api_endpoints_skills() {
        assert_eq!(ApiEndpoints::SKILLS, "/skills");
        assert_eq!(ApiEndpoints::SKILL_INSTALL, "/skills/install");
    }

    #[test]
    fn test_api_endpoints_instances() {
        assert_eq!(ApiEndpoints::INSTANCES, "/instances");
        assert_eq!(ApiEndpoints::INSTANCE_DETAIL, "/instances/");
    }

    // ========== Base URL + Path Concatenation ==========

    #[test]
    fn test_base_url_concatenation() {
        let base = "/api/v1";
        assert_eq!(format!("{}{}", base, ApiEndpoints::AGENTS), "/api/v1/agents");
        assert_eq!(format!("{}{}", base, "/agents/123/logs"), "/api/v1/agents/123/logs");
        assert_eq!(format!("{}{}", base, "/chain/dao/proposals"), "/api/v1/chain/dao/proposals");
        assert_eq!(format!("{}{}", base, "/treasury"), "/api/v1/treasury");
    }

    // ========== Model Deserialization Compatibility ==========

    #[test]
    fn test_agent_info_deserialization() {
        let json = json!({
            "id": "agent-1",
            "name": "Test Agent",
            "description": "desc",
            "status": "running",
            "capabilities": ["read"],
            "created_at": "2024-01-15T10:30:00Z",
            "updated_at": "2024-01-15T10:30:00Z"
        });
        let agent: AgentInfo = serde_json::from_value(json).expect("AgentInfo should deserialize");
        assert_eq!(agent.id, "agent-1");
        assert_eq!(agent.status, AgentStatus::Running);
        assert_eq!(agent.task_count, None); // #[serde(default)]
    }

    #[test]
    fn test_proposal_info_deserialization() {
        let json = json!({
            "id": "prop-1",
            "title": "Title",
            "description": "Desc",
            "status": "active",
            "proposer": "0xabc",
            "created_at": "2024-01-01",
            "ends_at": "2024-01-10",
            "votes_for": 10,
            "votes_against": 2,
            "user_voted": true
        });
        let prop: ProposalInfo = serde_json::from_value(json).expect("ProposalInfo should deserialize");
        assert_eq!(prop.status, ProposalStatus::Active);
        assert_eq!(prop.votes_for, 10);
    }

    #[test]
    fn test_proposal_info_with_defaults() {
        // Backend may omit some fields — verify #[serde(default)] works
        let json = json!({
            "id": "prop-2",
            "title": "Title",
            "description": "Desc",
            "status": "pending",
            "votes_for": 0,
            "votes_against": 0
        });
        let prop: ProposalInfo = serde_json::from_value(json).expect("ProposalInfo with missing fields should deserialize");
        assert_eq!(prop.proposer, "");
        assert_eq!(prop.created_at, "");
        assert_eq!(prop.user_voted, None);
    }

    #[test]
    fn test_browser_instance_deserialization() {
        let json = json!({
            "id": "inst-1",
            "profile_id": "prof-1",
            "status": "connected",
            "current_url": "https://example.com"
        });
        let inst: crate::browser::BrowserInstance = serde_json::from_value(json).expect("BrowserInstance should deserialize");
        assert_eq!(inst.id, "inst-1");
        assert_eq!(inst.status, crate::browser::ConnectionStatus::Connected);
        assert_eq!(inst.page_title, None); // #[serde(default)]
    }

    #[test]
    fn test_llm_health_response_deserialization() {
        let json = json!({
            "status": "healthy",
            "providers": [
                { "name": "openai", "healthy": true, "consecutive_failures": 0 }
            ]
        });
        let health: LlmHealthResponse = serde_json::from_value(json).expect("LlmHealthResponse should deserialize");
        assert_eq!(health.status, "healthy");
        assert_eq!(health.providers.len(), 1);
    }

    #[test]
    fn test_treasury_info_deserialization() {
        let json = json!({
            "total_balance": "1000",
            "token_symbol": "ETH",
            "assets": [
                { "token": "ETH", "balance": "1000", "value_usd": 2000.0 }
            ],
            "recent_transactions": []
        });
        let info: TreasuryInfo = serde_json::from_value(json).expect("TreasuryInfo should deserialize");
        assert_eq!(info.total_balance, "1000");
    }

    #[test]
    fn test_settings_deserialization() {
        let json = json!({
            "theme": "dark",
            "language": "en",
            "notifications_enabled": true,
            "auto_update": false
        });
        let settings: Settings = serde_json::from_value(json).expect("Settings should deserialize");
        assert_eq!(settings.theme, Theme::Dark);
    }

    #[test]
    fn test_channel_info_deserialization() {
        let json = json!({
            "id": "wechat",
            "name": "微信",
            "description": "WeChat",
            "icon": "💬",
            "enabled": true,
            "status": "connected"
        });
        let ch: ChannelInfo = serde_json::from_value(json).expect("ChannelInfo should deserialize");
        assert_eq!(ch.status, ChannelStatus::Connected);
    }

    #[test]
    fn test_update_agent_request_serialization() {
        let req = UpdateAgentRequest {
            name: Some("New Name".to_string()),
            description: None,
            status: Some("running".to_string()),
            capabilities: Some(vec!["read".to_string()]),
            model_provider: Some("openai".to_string()),
            model_name: Some("gpt-4".to_string()),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["name"], "New Name");
        assert_eq!(json["status"], "running");
        assert!(json["description"].is_null());
    }

    #[test]
    fn test_create_proposal_request_serialization() {
        let req = CreateProposalRequest {
            title: "Test Proposal".to_string(),
            description: "Description".to_string(),
            proposal_type: "funding".to_string(),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["proposal_type"], "funding");
    }
}
