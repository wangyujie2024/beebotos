//! HTTP API client for BeeBotOS

#![allow(dead_code)]

use std::pin::Pin;
use std::time::Duration;

use anyhow::{anyhow, Result};
use futures::Stream;
use reqwest::{header, Client};

use crate::logging::logger;
use crate::network::{DefaultRequestInterceptor, LoggingInterceptor, NetworkClient, NetworkConfig};

pub struct ApiClient {
    http: Client,
    base_url: String,
    api_key: String,
}

impl ApiClient {
    /// Create HTTP client using NetworkClient with production-ready
    /// configuration
    fn create_http_client(api_key: &str) -> Result<Client> {
        let config = NetworkConfig::default();
        let mut network_client = NetworkClient::new(config)?;

        // Add default request interceptor for auth headers
        network_client.add_request_interceptor(DefaultRequestInterceptor::new(
            api_key.to_string(),
            format!("BeeBotOS-CLI/{}", env!("CARGO_PKG_VERSION")),
        ));

        // Add logging interceptor if debug level is enabled
        if logger().is_debug_enabled() {
            network_client.add_request_interceptor(LoggingInterceptor);
            network_client.add_response_interceptor(LoggingInterceptor);
        }

        // Get the underlying reqwest client
        Ok(network_client.into_inner())
    }

    /// Create a simple HTTP client (fallback)
    fn create_simple_client() -> Result<Client> {
        Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| anyhow!("Failed to build HTTP client: {}", e))
    }

    pub fn new() -> Result<Self> {
        let base_url = std::env::var("BEEBOTOS_API_URL")
            .unwrap_or_else(|_| "https://api.beebotos.io/v1".to_string());

        // Enforce HTTPS in production
        if !base_url.starts_with("https://") && !Self::is_local_development(&base_url) {
            return Err(anyhow!(
                "HTTPS is required for production API endpoints. Current URL: {}. For local \
                 development only, use localhost or 127.0.0.1",
                base_url
            ));
        }

        let api_key =
            std::env::var("BEEBOTOS_API_KEY").map_err(|_| anyhow!("BEEBOTOS_API_KEY not set"))?;

        // Try to create client with NetworkClient, fall back to simple client
        let http = match Self::create_http_client(&api_key) {
            Ok(client) => {
                crate::log_debug!("Using NetworkClient with production-ready configuration");
                client
            }
            Err(e) => {
                crate::log_warn!("Failed to create NetworkClient: {}, using fallback", e);
                Self::create_simple_client()?
            }
        };

        Ok(Self {
            http,
            base_url,
            api_key,
        })
    }

    #[allow(dead_code)]
    /// Create client with custom base URL and API key
    pub fn with_config(base_url: String, api_key: String) -> Result<Self> {
        let http = Self::create_http_client(&api_key).or_else(|_| Self::create_simple_client())?;

        Ok(Self {
            http,
            base_url,
            api_key,
        })
    }

    pub(crate) fn headers(&self) -> Result<header::HeaderMap> {
        let mut headers = header::HeaderMap::new();

        let auth_value = format!("Bearer {}", self.api_key)
            .parse()
            .map_err(|_| anyhow!("Invalid API key format"))?;
        headers.insert(header::AUTHORIZATION, auth_value);

        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );

        Ok(headers)
    }

    pub(crate) fn build_url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    pub(crate) fn base_url(&self) -> &str {
        &self.base_url
    }

    pub(crate) fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// Check if URL is for local development (allows HTTP)
    fn is_local_development(url: &str) -> bool {
        url.contains("localhost")
            || url.contains("127.0.0.1")
            || url.contains("::1")
            || url.contains("192.168.")
            || url.contains("10.")
            || url.contains("172.")
    }

    // Agent operations
    pub async fn create_agent(
        &self,
        name: &str,
        template: &str,
        config: Option<&str>,
    ) -> Result<AgentInfo> {
        let url = self.build_url("/agents");
        let body = serde_json::json!({
            "name": name,
            "template": template,
            "config": config,
        });

        let resp = self
            .http
            .post(&url)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to create agent ({}): {}", status, text));
        }

        Ok(resp.json().await?)
    }

    pub async fn list_agents(&self, status: Option<&str>, all: bool) -> Result<Vec<AgentInfo>> {
        let mut url = self.build_url("/agents");
        if let Some(s) = status {
            url.push_str(&format!("?status={}", s));
        }
        if all {
            url.push_str(if status.is_some() {
                "&all=true"
            } else {
                "?all=true"
            });
        }

        let resp = self.http.get(&url).headers(self.headers()?).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to list agents ({}): {}", status, text));
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(result["data"].clone())?)
    }

    pub async fn get_agent(&self, id: &str) -> Result<AgentInfo> {
        let url = self.build_url(&format!("/agents/{}", id));
        let resp = self.http.get(&url).headers(self.headers()?).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to get agent ({}): {}", status, text));
        }

        Ok(resp.json().await?)
    }

    pub async fn start_agent(&self, id: &str) -> Result<()> {
        let url = self.build_url(&format!("/agents/{}/start", id));
        let resp = self.http.post(&url).headers(self.headers()?).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to start agent ({}): {}", status, text));
        }

        Ok(())
    }

    pub async fn stop_agent(&self, id: &str, force: bool) -> Result<()> {
        let url = self.build_url(&format!("/agents/{}/stop?force={}", id, force));
        let resp = self.http.post(&url).headers(self.headers()?).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to stop agent ({}): {}", status, text));
        }

        Ok(())
    }

    pub async fn delete_agent(&self, id: &str) -> Result<()> {
        let url = self.build_url(&format!("/agents/{}", id));
        let resp = self
            .http
            .delete(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to delete agent ({}): {}", status, text));
        }

        Ok(())
    }

    pub async fn exec_task(&self, agent_id: &str, input: &str, timeout: u64) -> Result<TaskResult> {
        let url = self.build_url(&format!("/agents/{}/tasks", agent_id));
        let body = serde_json::json!({
            "input": input,
            "timeout": timeout,
        });

        let resp = self
            .http
            .post(&url)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to execute task ({}): {}", status, text));
        }

        let task: TaskInfo = resp.json().await?;

        // Poll for result
        let result_url = self.build_url(&format!("/agents/{}/tasks/{}", agent_id, task.id));
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;

            let resp = self
                .http
                .get(&result_url)
                .headers(self.headers()?)
                .send()
                .await?;

            if resp.status().is_success() {
                let task: TaskInfo = resp.json().await?;
                if task.status == "completed" {
                    return task
                        .result
                        .ok_or_else(|| anyhow!("Task completed but no result"));
                } else if task.status == "failed" {
                    return Err(anyhow!("Task failed: {:?}", task.error));
                }
            }
        }
    }

    /// Follow logs for an agent (streaming)
    pub async fn follow_logs(&self, id: &str) -> Result<()> {
        let url = self.build_url(&format!("/agents/{}/logs/stream", id));
        let mut resp = self.http.get(&url).headers(self.headers()?).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to follow logs ({}): {}", status, text));
        }

        // Stream logs to stdout
        while let Some(chunk) = resp.chunk().await? {
            let text = String::from_utf8_lossy(&chunk);
            print!("{}", text);
        }

        Ok(())
    }

    /// Get logs for an agent
    pub async fn get_logs(&self, id: &str, lines: usize) -> Result<Vec<String>> {
        let url = self.build_url(&format!("/agents/{}/logs?lines={}", id, lines));
        let resp = self.http.get(&url).headers(self.headers()?).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to get logs ({}): {}", status, text));
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(result["logs"].clone())?)
    }

    // Message operations
    pub async fn send_message(&self, to: &str, message: &str, timeout: u64) -> Result<String> {
        let url = self.build_url("/messages");
        let body = serde_json::json!({
            "to": to,
            "content": message,
            "timeout": timeout,
        });

        let resp = self
            .http
            .post(&url)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to send message ({}): {}", status, text));
        }

        let result: serde_json::Value = resp.json().await?;
        result["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("Invalid response: missing 'content' field"))
    }

    /// Broadcast a message to multiple agents
    pub async fn broadcast_message(
        &self,
        capability: Option<&str>,
        message: &str,
    ) -> Result<Vec<String>> {
        let url = self.build_url("/messages/broadcast");
        let body = serde_json::json!({
            "capability": capability,
            "content": message,
        });

        let resp = self
            .http
            .post(&url)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Failed to broadcast message ({}): {}",
                status,
                text
            ));
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(result["recipients"].clone())?)
    }

    /// Get message history
    pub async fn get_message_history(
        &self,
        agent: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MessageInfo>> {
        let mut url = self.build_url(&format!("/messages?limit={}", limit));
        if let Some(agent_id) = agent {
            url.push_str(&format!("&agent={}", agent_id));
        }

        let resp = self.http.get(&url).headers(self.headers()?).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Failed to get message history ({}): {}",
                status,
                text
            ));
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(result["messages"].clone())?)
    }

    // Skill operations
    pub async fn list_skills(
        &self,
        category: Option<&str>,
        search: Option<&str>,
    ) -> Result<Vec<SkillInfo>> {
        let mut url = self.build_url("/skills");
        if let Some(c) = category {
            url.push_str(&format!("?category={}", c));
        }
        if let Some(s) = search {
            url.push_str(&format!("&search={}", s));
        }

        let resp = self.http.get(&url).headers(self.headers()?).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to list skills ({}): {}", status, text));
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(result["data"].clone())?)
    }

    pub async fn get_skill(&self, id: &str) -> Result<SkillInfo> {
        let url = self.build_url(&format!("/skills/{}", id));
        let resp = self.http.get(&url).headers(self.headers()?).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to get skill ({}): {}", status, text));
        }

        Ok(resp.json().await?)
    }

    pub async fn install_skill(
        &self,
        source: &str,
        agent: Option<&str>,
        version: Option<&str>,
    ) -> Result<SkillInfo> {
        let url = self.build_url("/skills/install");
        let body = serde_json::json!({
            "source": source,
            "agent_id": agent,
            "version": version,
        });

        let resp = self
            .http
            .post(&url)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to install skill ({}): {}", status, text));
        }

        Ok(resp.json().await?)
    }

    /// Uninstall a skill
    pub async fn uninstall_skill(&self, id: &str, agent: Option<&str>) -> Result<()> {
        let url = self.build_url(&format!("/skills/{}/uninstall", id));
        let body = serde_json::json!({
            "agent_id": agent,
        });

        let resp = self
            .http
            .post(&url)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to uninstall skill ({}): {}", status, text));
        }

        Ok(())
    }

    /// Update a skill
    pub async fn update_skill(&self, id: &str, agent: Option<&str>) -> Result<SkillInfo> {
        let url = self.build_url(&format!("/skills/{}/update", id));
        let body = serde_json::json!({
            "agent_id": agent,
        });

        let resp = self
            .http
            .post(&url)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to update skill ({}): {}", status, text));
        }

        Ok(resp.json().await?)
    }

    /// Create skill template
    pub async fn create_skill_template(
        &self,
        name: &str,
        template: &str,
        output: &str,
    ) -> Result<()> {
        let url = self.build_url("/skills/templates");
        let body = serde_json::json!({
            "name": name,
            "template": template,
            "output": output,
        });

        let resp = self
            .http
            .post(&url)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Failed to create skill template ({}): {}",
                status,
                text
            ));
        }

        Ok(())
    }

    /// Publish a skill to registry
    pub async fn publish_skill(&self, path: &str, registry: &str) -> Result<PublishResult> {
        let url = self.build_url("/skills/publish");
        let body = serde_json::json!({
            "path": path,
            "registry": registry,
        });

        let resp = self
            .http
            .post(&url)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to publish skill ({}): {}", status, text));
        }

        Ok(resp.json().await?)
    }

    // DAO operations
    pub async fn list_proposals(&self, status: Option<&str>) -> Result<Vec<ProposalInfo>> {
        let mut url = self.build_url("/dao/proposals");
        if let Some(s) = status {
            url.push_str(&format!("?status={}", s));
        }

        let resp = self.http.get(&url).headers(self.headers()?).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to list proposals ({}): {}", status, text));
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(result["data"].clone())?)
    }

    #[allow(unused)]
    pub async fn cast_vote(
        &self,
        proposal_id: u64,
        support: &str,
        reason: Option<String>,
    ) -> Result<()> {
        let url = self.build_url(&format!("/dao/proposals/{}/votes", proposal_id));
        let body = serde_json::json!({
            "support": support,
            "reason": reason,
        });

        let resp = self
            .http
            .post(&url)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to cast vote ({}): {}", status, text));
        }

        Ok(())
    }

    // Brain operations
    /// Get brain status
    pub async fn get_brain_status(&self) -> Result<BrainStatus> {
        let url = self.build_url("/brain/status");
        let resp = self.http.get(&url).headers(self.headers()?).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to get brain status ({}): {}", status, text));
        }

        Ok(resp.json().await?)
    }

    /// Store a memory
    pub async fn store_memory(
        &self,
        agent: &str,
        content: &str,
        memory_type: &str,
        importance: f32,
    ) -> Result<()> {
        let url = self.build_url(&format!("/brain/agents/{}/memories", agent));
        let body = serde_json::json!({
            "content": content,
            "memory_type": memory_type,
            "importance": importance,
        });

        let resp = self
            .http
            .post(&url)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to store memory ({}): {}", status, text));
        }

        Ok(())
    }

    /// Retrieve memories
    pub async fn retrieve_memories(
        &self,
        agent: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryInfo>> {
        let url = self.build_url(&format!(
            "/brain/agents/{}/memories?query={}&limit={}",
            agent, query, limit
        ));
        let resp = self.http.get(&url).headers(self.headers()?).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Failed to retrieve memories ({}): {}",
                status,
                text
            ));
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(result["memories"].clone())?)
    }

    /// Consolidate memories
    pub async fn consolidate_memories(&self, agent: &str) -> Result<()> {
        let url = self.build_url(&format!("/brain/agents/{}/consolidate", agent));
        let resp = self.http.post(&url).headers(self.headers()?).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Failed to consolidate memories ({}): {}",
                status,
                text
            ));
        }

        Ok(())
    }

    /// Get emotion state
    pub async fn get_emotion_state(&self, agent: &str) -> Result<EmotionState> {
        let url = self.build_url(&format!("/brain/agents/{}/emotion", agent));
        let resp = self.http.get(&url).headers(self.headers()?).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Failed to get emotion state ({}): {}",
                status,
                text
            ));
        }

        Ok(resp.json().await?)
    }

    /// Set emotion state
    pub async fn set_emotion_state(
        &self,
        agent: &str,
        pleasure: f32,
        arousal: f32,
        dominance: f32,
    ) -> Result<()> {
        let url = self.build_url(&format!("/brain/agents/{}/emotion", agent));
        let body = serde_json::json!({
            "pleasure": pleasure,
            "arousal": arousal,
            "dominance": dominance,
        });

        let resp = self
            .http
            .post(&url)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!(
                "Failed to set emotion state ({}): {}",
                status,
                text
            ));
        }

        Ok(())
    }

    /// Evolve agent
    pub async fn evolve_agent(&self, agent: &str, generations: u32) -> Result<EvolutionResult> {
        let url = self.build_url(&format!("/brain/agents/{}/evolve", agent));
        let body = serde_json::json!({
            "generations": generations,
        });

        let resp = self
            .http
            .post(&url)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to evolve agent ({}): {}", status, text));
        }

        Ok(resp.json().await?)
    }

    // Session operations
    /// Create a session
    pub async fn create_session(&self, agent: &str, name: Option<&str>) -> Result<SessionInfo> {
        let url = self.build_url("/sessions");
        let body = serde_json::json!({
            "agent_id": agent,
            "name": name,
        });

        let resp = self
            .http
            .post(&url)
            .headers(self.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to create session ({}): {}", status, text));
        }

        Ok(resp.json().await?)
    }

    /// List sessions
    pub async fn list_sessions(
        &self,
        agent: Option<&str>,
        active: bool,
    ) -> Result<Vec<SessionInfo>> {
        let mut url = self.build_url("/sessions");
        if let Some(agent_id) = agent {
            url.push_str(&format!("?agent={}", agent_id));
        }
        if active {
            url.push_str(if agent.is_some() {
                "&active=true"
            } else {
                "?active=true"
            });
        }

        let resp = self.http.get(&url).headers(self.headers()?).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to list sessions ({}): {}", status, text));
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(result["sessions"].clone())?)
    }

    /// Resume a session
    pub async fn resume_session(&self, id: &str) -> Result<SessionInfo> {
        let url = self.build_url(&format!("/sessions/{}/resume", id));
        let resp = self.http.post(&url).headers(self.headers()?).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to resume session ({}): {}", status, text));
        }

        Ok(resp.json().await?)
    }

    /// Get session details
    pub async fn get_session(&self, id: &str) -> Result<SessionDetail> {
        let url = self.build_url(&format!("/sessions/{}", id));
        let resp = self.http.get(&url).headers(self.headers()?).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to get session ({}): {}", status, text));
        }

        Ok(resp.json().await?)
    }

    /// Archive a session
    pub async fn archive_session(&self, id: &str) -> Result<()> {
        let url = self.build_url(&format!("/sessions/{}/archive", id));
        let resp = self.http.post(&url).headers(self.headers()?).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to archive session ({}): {}", status, text));
        }

        Ok(())
    }

    /// Delete a session
    pub async fn delete_session(&self, id: &str) -> Result<()> {
        let url = self.build_url(&format!("/sessions/{}", id));
        let resp = self
            .http
            .delete(&url)
            .headers(self.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to delete session ({}): {}", status, text));
        }

        Ok(())
    }

    #[allow(dead_code)]
    /// Convert HTTP URL to WebSocket URL
    fn http_to_ws_url(http_url: &str) -> Result<String> {
        let ws_url = if http_url.starts_with("https://") {
            http_url.replace("https://", "wss://")
        } else if http_url.starts_with("http://") {
            http_url.replace("http://", "ws://")
        } else {
            http_url.to_string()
        };

        // Ensure path ends with /ws
        let ws_url = if ws_url.ends_with("/ws") {
            ws_url
        } else if ws_url.ends_with('/') {
            format!("{}ws", ws_url)
        } else {
            format!("{}/ws", ws_url)
        };

        Ok(ws_url)
    }
}

// ChainClient for blockchain operations
pub struct ChainClient {
    client: ApiClient,
}

impl ChainClient {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: ApiClient::new()?,
        })
    }

    pub fn default_address(&self) -> String {
        std::env::var("DEFAULT_WALLET_ADDRESS")
            .unwrap_or_else(|_| "0x0000000000000000000000000000000000000000".to_string())
    }

    pub async fn get_status(&self) -> Result<ChainStatus> {
        let url = self.client.build_url("/chain/status");
        let resp = self
            .client
            .http()
            .get(&url)
            .headers(self.client.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to get chain status ({}): {}", status, text));
        }

        Ok(resp.json().await?)
    }

    pub async fn get_balance(&self, address: &str, token: Option<&str>) -> Result<String> {
        let mut url = format!(
            "{}/chain/balance?address={}",
            self.client.build_url("").trim_end_matches('/'),
            address
        );
        if let Some(t) = token {
            url.push_str(&format!("&token={}", t));
        }

        let resp = self
            .client
            .http()
            .get(&url)
            .headers(self.client.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to get balance ({}): {}", status, text));
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(result["balance"].as_str().unwrap_or("0").to_string())
    }

    pub async fn transfer(&self, to: &str, amount: &str, token: Option<&str>) -> Result<String> {
        let url = self.client.build_url("/chain/transfer");
        let body = serde_json::json!({
            "to": to,
            "amount": amount,
            "token": token,
        });

        let resp = self
            .client
            .http()
            .post(&url)
            .headers(self.client.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Transfer failed ({}): {}", status, text));
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(result["tx_hash"].as_str().unwrap_or("").to_string())
    }

    pub async fn wait_for_confirmation(&self, tx_hash: &str) -> Result<TransactionReceipt> {
        let url = self.client.build_url("/chain/chain");
        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(120);

        loop {
            if start.elapsed() > timeout {
                return Err(anyhow!("Timeout waiting for confirmation"));
            }

            let check_url = format!("{}/tx/{}", url.trim_end_matches("/chain"), tx_hash);
            let resp = self
                .client
                .http()
                .get(&check_url)
                .headers(self.client.headers()?)
                .send()
                .await?;

            if resp.status().is_success() {
                let result: serde_json::Value = resp.json().await?;
                if result["confirmed"].as_bool().unwrap_or(false) {
                    return Ok(TransactionReceipt {
                        block_number: result["block_number"].as_u64().unwrap_or(0),
                    });
                }
            }

            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    pub async fn deploy_contract(&self, artifact: &str, args: &[String]) -> Result<String> {
        let url = self.client.build_url("/chain/deploy");
        let body = serde_json::json!({
            "artifact": artifact,
            "args": args,
        });

        let resp = self
            .client
            .http()
            .post(&url)
            .headers(self.client.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Deploy failed ({}): {}", status, text));
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(result["address"].as_str().unwrap_or("").to_string())
    }

    pub async fn verify_contract(&self, address: &str, artifact: &str) -> Result<()> {
        let url = self.client.build_url("/chain/verify");
        let body = serde_json::json!({
            "address": address,
            "artifact": artifact,
        });

        let resp = self
            .client
            .http()
            .post(&url)
            .headers(self.client.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Verification failed ({}): {}", status, text));
        }

        Ok(())
    }

    pub async fn call(&self, contract: &str, function: &str, args: &[String]) -> Result<String> {
        let url = self.client.build_url("/chain/call");
        let body = serde_json::json!({
            "contract": contract,
            "function": function,
            "args": args,
        });

        let resp = self
            .client
            .http()
            .post(&url)
            .headers(self.client.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Call failed ({}): {}", status, text));
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(result["result"].as_str().unwrap_or("").to_string())
    }

    pub async fn send_transaction(
        &self,
        contract: &str,
        function: &str,
        args: &[String],
    ) -> Result<String> {
        let url = self.client.build_url("/chain/transaction");
        let body = serde_json::json!({
            "contract": contract,
            "function": function,
            "args": args,
        });

        let resp = self
            .client
            .http()
            .post(&url)
            .headers(self.client.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Transaction failed ({}): {}", status, text));
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(result["tx_hash"].as_str().unwrap_or("").to_string())
    }

    pub async fn watch_events(
        &self,
        contract: Option<&str>,
        event: Option<&str>,
        from_block: Option<u64>,
    ) -> Result<Pin<Box<dyn Stream<Item = EventInfo> + Send>>> {
        let mut url = format!(
            "{}/chain/events/stream",
            self.client.build_url("").trim_end_matches('/')
        );
        let mut params = Vec::new();

        if let Some(c) = contract {
            params.push(format!("contract={}", c));
        }
        if let Some(e) = event {
            params.push(format!("event={}", e));
        }
        if let Some(b) = from_block {
            params.push(format!("from_block={}", b));
        }

        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }

        let resp = self
            .client
            .http()
            .get(&url)
            .headers(self.client.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Event subscription failed ({}): {}", status, text));
        }

        // Return a stream that yields events
        use futures::StreamExt;
        let stream = resp.bytes_stream().filter_map(|chunk| async move {
            match chunk {
                Ok(c) => Some(EventInfo {
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    event_type: "unknown".to_string(),
                    data: serde_json::Value::String(String::from_utf8_lossy(&c).to_string()),
                }),
                Err(_) => None,
            }
        });

        Ok(Box::pin(stream))
    }

    #[allow(dead_code)]
    pub async fn watch_blocks(&self) -> Result<Pin<Box<dyn Stream<Item = BlockInfo> + Send>>> {
        let url = self.client.build_url("/chain/blocks/stream");

        let resp = self
            .client
            .http()
            .get(&url)
            .headers(self.client.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Block subscription failed ({}): {}", status, text));
        }

        use futures::StreamExt;
        let stream = resp.bytes_stream().filter_map(|chunk| async move {
            match chunk {
                Ok(_) => Some(BlockInfo {
                    number: 0,
                    tx_count: 0,
                    gas_used: 0,
                }),
                Err(_) => None,
            }
        });

        Ok(Box::pin(stream))
    }

    // Payment operations
    pub async fn store_payment_metadata(&self, tx_hash: &str, desc: &str) -> Result<()> {
        let url = self.client.build_url("/chain/payment/metadata");
        let body = serde_json::json!({
            "tx_hash": tx_hash,
            "description": desc,
        });

        let resp = self
            .client
            .http()
            .post(&url)
            .headers(self.client.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to store metadata ({}): {}", status, text));
        }

        Ok(())
    }

    pub async fn create_mandate(
        &self,
        grantee: &str,
        allowance: &str,
        token: Option<&str>,
        duration: u32,
    ) -> Result<MandateInfo> {
        let url = self.client.build_url("/chain/mandates");
        let body = serde_json::json!({
            "grantee": grantee,
            "allowance": allowance,
            "token": token,
            "duration": duration,
        });

        let resp = self
            .client
            .http()
            .post(&url)
            .headers(self.client.headers()?)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to create mandate ({}): {}", status, text));
        }

        Ok(resp.json().await?)
    }

    pub async fn list_mandates_as_grantor(&self) -> Result<Vec<MandateInfo>> {
        let url = self.client.build_url("/chain/mandates?role=grantor");
        let resp = self
            .client
            .http()
            .get(&url)
            .headers(self.client.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to list mandates ({}): {}", status, text));
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(result["mandates"].clone())?)
    }

    pub async fn list_mandates_as_grantee(&self) -> Result<Vec<MandateInfo>> {
        let url = self.client.build_url("/chain/mandates?role=grantee");
        let resp = self
            .client
            .http()
            .get(&url)
            .headers(self.client.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to list mandates ({}): {}", status, text));
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(result["mandates"].clone())?)
    }

    pub async fn list_all_mandates(&self) -> Result<Vec<MandateInfo>> {
        let url = self.client.build_url("/chain/mandates");
        let resp = self
            .client
            .http()
            .get(&url)
            .headers(self.client.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to list mandates ({}): {}", status, text));
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(result["mandates"].clone())?)
    }

    pub async fn revoke_mandate(&self, id: &str) -> Result<()> {
        let url = format!("{}/{}", self.client.build_url("/chain/mandates"), id);
        let resp = self
            .client
            .http()
            .delete(&url)
            .headers(self.client.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to revoke mandate ({}): {}", status, text));
        }

        Ok(())
    }

    pub async fn get_transactions(
        &self,
        token: Option<&str>,
        limit: usize,
    ) -> Result<Vec<TransactionInfo>> {
        let mut url = format!(
            "{}/chain/transactions?limit={}",
            self.client.build_url("").trim_end_matches('/'),
            limit
        );
        if let Some(t) = token {
            url.push_str(&format!("&token={}", t));
        }

        let resp = self
            .client
            .http()
            .get(&url)
            .headers(self.client.headers()?)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Failed to get transactions ({}): {}", status, text));
        }

        let result: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(result["transactions"].clone())?)
    }
}

// Data structures
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub status: String,
    pub last_active: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct TaskInfo {
    pub id: String,
    pub status: String,
    pub result: Option<TaskResult>,
    pub error: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct TaskResult {
    pub output: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct SkillInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub category: String,
    pub author: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub downloads: u64,
    pub rating: f32,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ProposalInfo {
    pub id: u64,
    pub title: String,
    pub status: String,
    pub votes_for: String,
    pub votes_against: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct BrainStatus {
    pub memory_used: String,
    pub memory_total: String,
    pub active_agents: usize,
    pub evolution_queue: usize,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct MemoryInfo {
    pub memory_type: String,
    pub content: String,
    pub relevance: f32,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct EmotionState {
    pub pleasure: f32,
    pub arousal: f32,
    pub dominance: f32,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct EvolutionResult {
    pub fitness: f32,
    pub generations: u32,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub name: String,
    pub status: String,
    pub last_active: String,
    pub key: String,
    pub agent_id: String,
    #[serde(default)]
    pub context_items: usize,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct SessionDetail {
    pub id: String,
    pub name: String,
    pub agent_id: String,
    pub status: String,
    pub created_at: String,
    pub context_items: usize,
    pub transcript: Vec<TranscriptEntry>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct TranscriptEntry {
    pub timestamp: String,
    pub role: String,
    pub content: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct PublishResult {
    pub id: String,
    pub version: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct MessageInfo {
    pub timestamp: String,
    pub from: String,
    pub to: Option<String>,
    pub content: String,
}

// Re-export types from websocket module
#[allow(unused_imports)]
pub use crate::websocket::{AgentUpdate, BlockInfo, EventInfo, TaskUpdate};

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ChainStatus {
    pub network: String,
    pub chain_id: u64,
    pub block_number: u64,
    pub sync_status: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct TransactionReceipt {
    pub block_number: u64,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct MandateInfo {
    pub id: String,
    pub grantor: String,
    pub grantee: String,
    pub remaining: String,
    pub active: bool,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct TransactionInfo {
    pub hash: String,
    pub amount: String,
    pub token: String,
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_info_serialization() {
        let agent = AgentInfo {
            id: "agent-123".to_string(),
            name: "Test Agent".to_string(),
            status: "running".to_string(),
            last_active: "2024-01-15T10:30:00Z".to_string(),
        };
        let json = serde_json::to_string(&agent).unwrap();
        assert!(json.contains("agent-123"));
        assert!(json.contains("Test Agent"));
    }

    #[test]
    fn test_agent_info_deserialization() {
        let json = r#"{
            "id": "agent-456",
            "name": "My Agent",
            "status": "idle",
            "last_active": "2024-01-15T10:30:00Z"
        }"#;
        let agent: AgentInfo = serde_json::from_str(json).unwrap();
        assert_eq!(agent.id, "agent-456");
        assert_eq!(agent.status, "idle");
    }

    #[test]
    fn test_task_result_serialization() {
        let result = TaskResult {
            output: "Task completed successfully".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("Task completed successfully"));
    }

    #[test]
    fn test_skill_info_serialization() {
        let skill = SkillInfo {
            id: "skill-123".to_string(),
            name: "Code Analyzer".to_string(),
            version: "1.0.0".to_string(),
            category: "development".to_string(),
            author: "BeeBotOS Team".to_string(),
            description: "Analyzes code for issues".to_string(),
            capabilities: vec!["analyze".to_string(), "review".to_string()],
            downloads: 1000,
            rating: 4.5,
        };
        let json = serde_json::to_string(&skill).unwrap();
        assert!(json.contains("Code Analyzer"));
        assert!(json.contains("1.0.0"));
    }

    #[test]
    fn test_proposal_info_serialization() {
        let proposal = ProposalInfo {
            id: 123,
            title: "Add new feature".to_string(),
            status: "active".to_string(),
            votes_for: "1000".to_string(),
            votes_against: "100".to_string(),
        };
        let json = serde_json::to_string(&proposal).unwrap();
        assert!(json.contains("Add new feature"));
    }

    #[test]
    fn test_chain_status_serialization() {
        let status = ChainStatus {
            network: "mainnet".to_string(),
            chain_id: 1,
            block_number: 12345678,
            sync_status: "synced".to_string(),
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("mainnet"));
        assert!(json.contains("12345678"));
    }

    #[test]
    fn test_session_info_serialization() {
        let session = SessionInfo {
            id: "session-123".to_string(),
            name: "My Session".to_string(),
            status: "active".to_string(),
            last_active: "2024-01-15T10:30:00Z".to_string(),
            key: "secret-key".to_string(),
            agent_id: "agent-456".to_string(),
            context_items: 5,
        };
        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("session-123"));
        assert!(json.contains("agent-456"));
    }

    #[test]
    fn test_transaction_info_serialization() {
        let tx = TransactionInfo {
            hash: "0xabc123".to_string(),
            amount: "1.5".to_string(),
            token: "BEE".to_string(),
            status: "confirmed".to_string(),
        };
        let json = serde_json::to_string(&tx).unwrap();
        assert!(json.contains("0xabc123"));
        assert!(json.contains("confirmed"));
    }

    #[test]
    fn test_emotion_state_serialization() {
        let emotion = EmotionState {
            pleasure: 0.5,
            arousal: 0.3,
            dominance: 0.4,
        };
        let json = serde_json::to_string(&emotion).unwrap();
        assert!(json.contains("0.5"));
    }

    #[test]
    fn test_evolution_result_serialization() {
        let result = EvolutionResult {
            fitness: 0.95,
            generations: 100,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("0.95"));
        assert!(json.contains("100"));
    }

    #[test]
    fn test_memory_info_serialization() {
        let memory = MemoryInfo {
            memory_type: "episodic".to_string(),
            content: "Remember this event".to_string(),
            relevance: 0.85,
        };
        let json = serde_json::to_string(&memory).unwrap();
        assert!(json.contains("episodic"));
    }

    #[test]
    fn test_message_info_serialization() {
        let msg = MessageInfo {
            timestamp: "2024-01-15T10:30:00Z".to_string(),
            from: "agent-1".to_string(),
            to: Some("agent-2".to_string()),
            content: "Hello".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("Hello"));
    }

    #[test]
    fn test_mandate_info_serialization() {
        let mandate = MandateInfo {
            id: "mandate-123".to_string(),
            grantor: "0xabc".to_string(),
            grantee: "0xdef".to_string(),
            remaining: "1000".to_string(),
            active: true,
        };
        let json = serde_json::to_string(&mandate).unwrap();
        assert!(json.contains("mandate-123"));
    }

    #[test]
    fn test_brain_status_serialization() {
        let status = BrainStatus {
            memory_used: "50MB".to_string(),
            memory_total: "100MB".to_string(),
            active_agents: 10,
            evolution_queue: 2,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("50MB"));
    }

    #[test]
    fn test_publish_result_serialization() {
        let result = PublishResult {
            id: "skill-123".to_string(),
            version: "1.0.0".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("skill-123"));
    }

    #[test]
    fn test_transcript_entry_serialization() {
        let entry = TranscriptEntry {
            timestamp: "2024-01-15T10:30:00Z".to_string(),
            role: "user".to_string(),
            content: "Hello".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("user"));
    }
}
// ============================================================================
// Extended API client implementations for new CLI command modules
// This module provides client implementations for Channel, Memory, Model, and
// Infer operations
// ============================================================================

use std::path::PathBuf;

// ============================================================================
// Channel Module Types and Trait
// ============================================================================

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Channel {
    pub id: String,
    pub r#type: String,
    pub name: String,
    pub status: String,
    pub health: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ChannelStatus {
    pub id: String,
    pub name: String,
    pub channel_type: String,
    pub status: String,
    pub health: String,
    pub connected: bool,
    pub last_activity: String,
    pub stats: Option<ChannelStats>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ChannelStats {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub error_rate: f64,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ChannelCapabilities {
    pub name: String,
    pub features: Vec<String>,
    pub content_types: Vec<String>,
    pub rate_limit: RateLimit,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct RateLimit {
    pub messages_per_minute: u32,
    pub max_message_size: u64,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ResolvedName {
    pub id: String,
    pub channel_type: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct NewChannel {
    pub id: String,
    pub status: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Message {
    pub id: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Webhook {
    pub id: String,
    pub url: String,
    pub secret: Option<String>,
    pub created_at: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct WebhookTestResult {
    pub status: String,
    pub response_time_ms: u64,
    pub error: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ChannelTestResult {
    pub success: bool,
    pub latency_ms: u64,
    pub message_delivered: bool,
    pub error: Option<String>,
}

#[async_trait::async_trait]
pub trait ChannelClient {
    async fn list_channels(&self, channel_type: Option<&str>) -> Result<Vec<Channel>>;
    async fn get_channel_status(&self, id: &str, probe: bool) -> Result<ChannelStatus>;
    async fn get_all_channel_status(&self, probe: bool) -> Result<Vec<ChannelStatus>>;
    async fn get_channel_capabilities(&self, id: &str) -> Result<ChannelCapabilities>;
    async fn resolve_channel_name(&self, channel: &str, name: &str) -> Result<ResolvedName>;
    async fn get_channel_logs(
        &self,
        id: &str,
        lines: usize,
        level: Option<&str>,
    ) -> Result<Vec<LogEntry>>;
    async fn follow_channel_logs(&self, id: &str, level: Option<&str>) -> Result<()>;
    async fn add_channel(&self, channel_type: &str, config: &str) -> Result<NewChannel>;
    async fn remove_channel(&self, id: &str, delete_data: bool) -> Result<()>;
    async fn login_channel(&self, id: &str, method: Option<String>) -> Result<Option<String>>;
    async fn wait_for_channel_auth(&self, id: &str) -> Result<()>;
    async fn logout_channel(&self, id: &str, revoke: bool) -> Result<()>;
    async fn send_channel_message(
        &self,
        channel: &str,
        target: &str,
        message: &str,
        template: Option<&str>,
        attachments: &[PathBuf],
    ) -> Result<Message>;
    async fn generate_webhook(
        &self,
        channel: &str,
        path: Option<&str>,
        secret: Option<&str>,
    ) -> Result<Webhook>;
    async fn list_webhooks(&self, channel: &str) -> Result<Vec<Webhook>>;
    async fn delete_webhook(&self, id: &str) -> Result<()>;
    async fn test_webhook(&self, id: &str, payload: Option<&str>) -> Result<WebhookTestResult>;
    async fn test_channel(&self, id: &str, message: &str) -> Result<ChannelTestResult>;
}

#[async_trait::async_trait]
impl ChannelClient for ApiClient {
    async fn list_channels(&self, _channel_type: Option<&str>) -> Result<Vec<Channel>> {
        // TODO: Implement actual API call to /channels
        Ok(vec![])
    }

    async fn get_channel_status(&self, _id: &str, _probe: bool) -> Result<ChannelStatus> {
        anyhow::bail!("Channel status check not yet implemented")
    }

    async fn get_all_channel_status(&self, _probe: bool) -> Result<Vec<ChannelStatus>> {
        Ok(vec![])
    }

    async fn get_channel_capabilities(&self, _id: &str) -> Result<ChannelCapabilities> {
        anyhow::bail!("Channel capabilities not yet implemented")
    }

    async fn resolve_channel_name(&self, _channel: &str, _name: &str) -> Result<ResolvedName> {
        anyhow::bail!("Channel name resolution not yet implemented")
    }

    async fn get_channel_logs(
        &self,
        _id: &str,
        _lines: usize,
        _level: Option<&str>,
    ) -> Result<Vec<LogEntry>> {
        Ok(vec![])
    }

    async fn follow_channel_logs(&self, _id: &str, _level: Option<&str>) -> Result<()> {
        Ok(())
    }

    async fn add_channel(&self, _channel_type: &str, _config: &str) -> Result<NewChannel> {
        anyhow::bail!("Channel addition not yet implemented")
    }

    async fn remove_channel(&self, _id: &str, _delete_data: bool) -> Result<()> {
        Ok(())
    }

    async fn login_channel(&self, _id: &str, _method: Option<String>) -> Result<Option<String>> {
        Ok(None)
    }

    async fn wait_for_channel_auth(&self, _id: &str) -> Result<()> {
        Ok(())
    }

    async fn logout_channel(&self, _id: &str, _revoke: bool) -> Result<()> {
        Ok(())
    }

    async fn send_channel_message(
        &self,
        _channel: &str,
        _target: &str,
        _message: &str,
        _template: Option<&str>,
        _attachments: &[PathBuf],
    ) -> Result<Message> {
        anyhow::bail!("Channel message sending not yet implemented")
    }

    async fn generate_webhook(
        &self,
        _channel: &str,
        _path: Option<&str>,
        _secret: Option<&str>,
    ) -> Result<Webhook> {
        anyhow::bail!("Webhook generation not yet implemented")
    }

    async fn list_webhooks(&self, _channel: &str) -> Result<Vec<Webhook>> {
        Ok(vec![])
    }

    async fn delete_webhook(&self, _id: &str) -> Result<()> {
        Ok(())
    }

    async fn test_webhook(&self, _id: &str, _payload: Option<&str>) -> Result<WebhookTestResult> {
        anyhow::bail!("Webhook testing not yet implemented")
    }

    async fn test_channel(&self, _id: &str, _message: &str) -> Result<ChannelTestResult> {
        anyhow::bail!("Channel testing not yet implemented")
    }
}

// ============================================================================
// Memory Module Types and Trait
// ============================================================================

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct MemoryStatus {
    pub agent_id: Option<String>,
    pub stm_count: usize,
    pub ltm_count: usize,
    pub em_count: usize,
    pub stm_size_mb: f64,
    pub ltm_size_mb: f64,
    pub em_size_mb: f64,
    pub index_status: String,
    pub last_consolidation: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct IndexOptions {
    pub agent_id: Option<String>,
    pub memory_types: Vec<String>,
    pub recreate: bool,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct IndexResult {
    pub indexed: usize,
    pub errors: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct SearchRequest {
    pub query: String,
    pub agent_id: Option<String>,
    pub limit: usize,
    pub semantic: bool,
    pub hybrid: bool,
    pub memory_type: Option<String>,
    pub time_range: Option<String>,
    pub importance_min: Option<f32>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct SearchResult {
    pub id: String,
    pub content: String,
    pub memory_type: String,
    pub relevance: f32,
    pub created_at: String,
}

#[derive(Debug, serde::Serialize)]
pub struct CreateMemoryRequest {
    pub content: String,
    pub memory_type: String,
    pub importance: f32,
    pub agent_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Memory {
    pub id: String,
    pub content: String,
    pub memory_type: String,
    pub importance: f32,
    pub created_at: String,
    pub updated_at: String,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, serde::Serialize)]
pub struct UpdateMemoryRequest {
    pub content: Option<String>,
    pub importance: Option<f32>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, serde::Serialize)]
pub struct MemoryFilter {
    pub agent_id: Option<String>,
    pub memory_type: Option<String>,
    pub time_range: Option<String>,
    pub limit: usize,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct MemorySummary {
    pub id: String,
    pub preview: String,
    pub memory_type: String,
    pub created_at: String,
}

#[derive(Debug, serde::Serialize)]
pub struct ExportRequest {
    pub agent_id: Option<String>,
    pub path: String,
    pub format: String,
    pub encrypt: bool,
    pub memory_types: Vec<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ExportResult {
    pub path: String,
    pub exported: usize,
    pub size_bytes: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct ImportRequest {
    pub path: String,
    pub format: String,
    pub agent_id: Option<String>,
    pub passphrase: Option<String>,
    pub dry_run: bool,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ImportResult {
    pub imported: usize,
    pub skipped: usize,
    pub merged: usize,
    pub errors: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct ConsolidateOptions {
    pub agent_id: Option<String>,
    pub dry_run: bool,
    pub force: bool,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ConsolidateResult {
    pub candidate_count: usize,
    pub consolidated: usize,
    pub discarded: usize,
    pub summaries_generated: usize,
}

#[derive(Debug, serde::Serialize)]
pub struct ForgetOptions {
    pub agent_id: Option<String>,
    pub strategy: String,
    pub older_than_days: Option<u32>,
    pub dry_run: bool,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ForgetResult {
    pub would_forget: usize,
    pub would_forget_stm: usize,
    pub would_forget_ltm: usize,
    pub would_forget_em: usize,
    pub forgotten: usize,
    pub forgotten_stm: usize,
    pub forgotten_ltm: usize,
    pub forgotten_em: usize,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct MemoryStats {
    pub total_memories: usize,
    pub stm_count: usize,
    pub ltm_count: usize,
    pub em_count: usize,
    pub avg_importance: f32,
    pub memory_growth_rate: f32,
    pub consolidation_score: f32,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct TrashedMemory {
    pub id: String,
    pub preview: String,
    pub deleted_at: String,
    pub expires_at: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct GraphStructure {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub memory_type: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub weight: f32,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct RelatedMemory {
    pub id: String,
    pub content: String,
    pub relationship: String,
    pub strength: f32,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Cluster {
    pub id: usize,
    pub size: usize,
    pub theme: String,
    pub sample_memories: Vec<String>,
}

#[async_trait::async_trait]
pub trait MemoryClient {
    async fn get_memory_status_for_agent(&self, agent_id: &str) -> Result<MemoryStatus>;
    async fn get_global_memory_status(&self) -> Result<MemoryStatus>;
    async fn rebuild_memory_index(&self, options: &IndexOptions) -> Result<IndexResult>;
    async fn search_memories(&self, request: &SearchRequest) -> Result<Vec<SearchResult>>;
    async fn create_memory(&self, request: &CreateMemoryRequest) -> Result<Memory>;
    async fn get_memory(&self, id: &str) -> Result<Memory>;
    async fn update_memory(&self, id: &str, request: &UpdateMemoryRequest) -> Result<Memory>;
    async fn list_memories(&self, filter: &MemoryFilter) -> Result<Vec<MemorySummary>>;
    async fn move_memory_to_trash(&self, id: &str) -> Result<()>;
    async fn permanently_delete_memory(&self, id: &str) -> Result<()>;
    async fn export_memories(&self, request: &ExportRequest) -> Result<ExportResult>;
    async fn import_memories(&self, request: &ImportRequest) -> Result<ImportResult>;
    async fn consolidate_memories_ext(
        &self,
        options: &ConsolidateOptions,
    ) -> Result<ConsolidateResult>;
    async fn apply_forgetting(&self, options: &ForgetOptions) -> Result<ForgetResult>;
    async fn get_agent_memory_stats(&self, agent_id: &str, range: &str) -> Result<MemoryStats>;
    async fn get_global_memory_stats(&self, range: &str) -> Result<MemoryStats>;
    async fn list_trashed_memories(
        &self,
        agent_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<TrashedMemory>>;
    async fn restore_memory(&self, id: &str) -> Result<()>;
    async fn empty_trash(&self) -> Result<()>;
    async fn purge_memory(&self, id: &str) -> Result<()>;
    async fn get_memory_graph_structure(
        &self,
        agent_id: Option<&str>,
        depth: usize,
    ) -> Result<GraphStructure>;
    async fn get_related_memories(&self, id: &str, limit: usize) -> Result<Vec<RelatedMemory>>;
    async fn get_memory_clusters(
        &self,
        agent_id: Option<&str>,
        n_clusters: usize,
    ) -> Result<Vec<Cluster>>;
}

#[async_trait::async_trait]
impl MemoryClient for ApiClient {
    async fn get_memory_status_for_agent(&self, _agent_id: &str) -> Result<MemoryStatus> {
        anyhow::bail!("Memory status not yet implemented")
    }

    async fn get_global_memory_status(&self) -> Result<MemoryStatus> {
        anyhow::bail!("Global memory status not yet implemented")
    }

    async fn rebuild_memory_index(&self, _options: &IndexOptions) -> Result<IndexResult> {
        anyhow::bail!("Memory index rebuild not yet implemented")
    }

    async fn search_memories(&self, _request: &SearchRequest) -> Result<Vec<SearchResult>> {
        Ok(vec![])
    }

    async fn create_memory(&self, _request: &CreateMemoryRequest) -> Result<Memory> {
        anyhow::bail!("Memory creation not yet implemented")
    }

    async fn get_memory(&self, _id: &str) -> Result<Memory> {
        anyhow::bail!("Memory retrieval not yet implemented")
    }

    async fn update_memory(&self, _id: &str, _request: &UpdateMemoryRequest) -> Result<Memory> {
        anyhow::bail!("Memory update not yet implemented")
    }

    async fn list_memories(&self, _filter: &MemoryFilter) -> Result<Vec<MemorySummary>> {
        Ok(vec![])
    }

    async fn move_memory_to_trash(&self, _id: &str) -> Result<()> {
        Ok(())
    }

    async fn permanently_delete_memory(&self, _id: &str) -> Result<()> {
        Ok(())
    }

    async fn export_memories(&self, _request: &ExportRequest) -> Result<ExportResult> {
        anyhow::bail!("Memory export not yet implemented")
    }

    async fn import_memories(&self, _request: &ImportRequest) -> Result<ImportResult> {
        anyhow::bail!("Memory import not yet implemented")
    }

    async fn consolidate_memories_ext(
        &self,
        _options: &ConsolidateOptions,
    ) -> Result<ConsolidateResult> {
        anyhow::bail!("Memory consolidation not yet implemented")
    }

    async fn apply_forgetting(&self, _options: &ForgetOptions) -> Result<ForgetResult> {
        anyhow::bail!("Memory forgetting not yet implemented")
    }

    async fn get_agent_memory_stats(&self, _agent_id: &str, _range: &str) -> Result<MemoryStats> {
        anyhow::bail!("Memory stats not yet implemented")
    }

    async fn get_global_memory_stats(&self, _range: &str) -> Result<MemoryStats> {
        anyhow::bail!("Global memory stats not yet implemented")
    }

    async fn list_trashed_memories(
        &self,
        _agent_id: Option<&str>,
        _limit: usize,
    ) -> Result<Vec<TrashedMemory>> {
        Ok(vec![])
    }

    async fn restore_memory(&self, _id: &str) -> Result<()> {
        Ok(())
    }

    async fn empty_trash(&self) -> Result<()> {
        Ok(())
    }

    async fn purge_memory(&self, _id: &str) -> Result<()> {
        Ok(())
    }

    async fn get_memory_graph_structure(
        &self,
        _agent_id: Option<&str>,
        _depth: usize,
    ) -> Result<GraphStructure> {
        anyhow::bail!("Memory graph structure not yet implemented")
    }

    async fn get_related_memories(&self, _id: &str, _limit: usize) -> Result<Vec<RelatedMemory>> {
        Ok(vec![])
    }

    async fn get_memory_clusters(
        &self,
        _agent_id: Option<&str>,
        _n_clusters: usize,
    ) -> Result<Vec<Cluster>> {
        Ok(vec![])
    }
}

// ============================================================================
// Model Module Types and Trait
// ============================================================================

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub capabilities: Vec<String>,
    pub context_window: usize,
    pub pricing: Option<ModelPricing>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ModelPricing {
    pub input_per_1k: f64,
    pub output_per_1k: f64,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ModelStatus {
    pub id: String,
    pub available: bool,
    pub latency_ms: u64,
    pub queue_depth: usize,
    pub error_rate: f32,
}

#[derive(Debug, serde::Serialize)]
pub struct ScanOptions {
    pub providers: Vec<String>,
    pub include_local: bool,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ScannedModel {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub source: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub context_window: usize,
    pub training_cutoff: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub enum CompareDimension {
    Quality,
    Speed,
    Cost,
    ContextWindow,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ComparisonResult {
    pub model: String,
    pub dimension: String,
    pub score: f32,
    pub details: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct TestResult {
    pub model: String,
    pub success: bool,
    pub latency_ms: u64,
    pub tokens_used: usize,
    pub output: String,
    pub error: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ProviderAuth {
    pub provider: String,
    pub authenticated: bool,
    pub expires_at: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct AuthTestResult {
    pub success: bool,
    pub provider: String,
    pub error: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ModelAlias {
    pub name: String,
    pub target: String,
    pub created_at: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct FallbackTestResult {
    pub model: String,
    pub success: bool,
    pub latency_ms: u64,
}

#[derive(Debug, serde::Serialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, serde::Serialize)]
pub struct CompletionRequest {
    pub model: Option<String>,
    pub prompt: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stream: bool,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct CompletionResponse {
    pub text: String,
    pub model: String,
    pub tokens_used: usize,
    pub finish_reason: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Embedding {
    pub vector: Vec<f32>,
    pub model: String,
    pub dimensions: usize,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ModelUpdateResult {
    pub added: usize,
    pub removed: usize,
    pub updated: usize,
}

#[async_trait::async_trait]
pub trait ModelClient {
    async fn list_models(&self, provider: Option<&str>) -> Result<Vec<Model>>;
    async fn get_model_status(&self, id: &str) -> Result<ModelStatus>;
    async fn get_all_model_status(&self) -> Result<Vec<ModelStatus>>;
    async fn set_default_model(&self, id: &str, task_type: &str) -> Result<()>;
    async fn set_default_image_model(&self, id: &str) -> Result<()>;
    async fn scan_models(&self, options: &ScanOptions) -> Result<Vec<ScannedModel>>;
    async fn get_model_info(&self, id: &str) -> Result<ModelInfo>;
    async fn compare_models(
        &self,
        models: &[String],
        dimensions: &[CompareDimension],
    ) -> Result<Vec<ComparisonResult>>;
    async fn test_model(
        &self,
        model: Option<&str>,
        prompt: &str,
        max_tokens: u32,
    ) -> Result<TestResult>;
    async fn add_provider_credentials(&self, provider: &str, key: &str) -> Result<()>;
    async fn get_provider_auth_url(&self, provider: &str) -> Result<String>;
    async fn wait_for_provider_auth(&self, provider: &str) -> Result<()>;
    async fn set_provider_token(&self, provider: &str, token: &str) -> Result<()>;
    async fn list_provider_auths(&self) -> Result<Vec<ProviderAuth>>;
    async fn remove_provider_credentials(&self, provider: &str) -> Result<()>;
    async fn test_provider_auth(&self, provider: &str) -> Result<AuthTestResult>;
    async fn get_provider_order(&self) -> Result<Vec<String>>;
    async fn set_provider_order(&self, providers: &[String]) -> Result<()>;
    async fn add_provider_to_order(&self, provider: &str, position: usize) -> Result<()>;
    async fn remove_provider_from_order(&self, provider: &str) -> Result<()>;
    async fn clear_provider_order(&self) -> Result<()>;
    async fn list_model_aliases(&self) -> Result<Vec<ModelAlias>>;
    async fn add_model_alias(&self, name: &str, target: &str) -> Result<()>;
    async fn remove_model_alias(&self, name: &str) -> Result<()>;
    async fn get_model_alias(&self, name: &str) -> Result<ModelAlias>;
    async fn get_fallback_chain(&self, task_type: &str) -> Result<Vec<String>>;
    async fn add_to_fallback_chain(
        &self,
        model: &str,
        position: Option<usize>,
        task_type: &str,
    ) -> Result<()>;
    async fn remove_from_fallback_chain(&self, model: &str, task_type: &str) -> Result<()>;
    async fn clear_fallback_chain(&self, task_type: &str) -> Result<()>;
    async fn test_fallback_chain(&self, task_type: &str) -> Result<Vec<FallbackTestResult>>;
    async fn chat(&self, model: Option<&str>, messages: &[ChatMessage]) -> Result<ChatMessage>;
    async fn chat_stream(
        &self,
        model: Option<&str>,
        messages: &[ChatMessage],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>>;
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse>;
    async fn complete_stream(
        &self,
        request: &CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>>;
    async fn generate_embeddings(&self, model: Option<&str>, text: &str) -> Result<Embedding>;
    async fn update_model_list(&self) -> Result<ModelUpdateResult>;
}

#[async_trait::async_trait]
impl ModelClient for ApiClient {
    async fn list_models(&self, _provider: Option<&str>) -> Result<Vec<Model>> {
        Ok(vec![])
    }

    async fn get_model_status(&self, _id: &str) -> Result<ModelStatus> {
        anyhow::bail!("Model status not yet implemented")
    }

    async fn get_all_model_status(&self) -> Result<Vec<ModelStatus>> {
        Ok(vec![])
    }

    async fn set_default_model(&self, _id: &str, _task_type: &str) -> Result<()> {
        Ok(())
    }

    async fn set_default_image_model(&self, _id: &str) -> Result<()> {
        Ok(())
    }

    async fn scan_models(&self, _options: &ScanOptions) -> Result<Vec<ScannedModel>> {
        Ok(vec![])
    }

    async fn get_model_info(&self, _id: &str) -> Result<ModelInfo> {
        anyhow::bail!("Model info not yet implemented")
    }

    async fn compare_models(
        &self,
        _models: &[String],
        _dimensions: &[CompareDimension],
    ) -> Result<Vec<ComparisonResult>> {
        Ok(vec![])
    }

    async fn test_model(
        &self,
        _model: Option<&str>,
        _prompt: &str,
        _max_tokens: u32,
    ) -> Result<TestResult> {
        anyhow::bail!("Model testing not yet implemented")
    }

    async fn add_provider_credentials(&self, _provider: &str, _key: &str) -> Result<()> {
        Ok(())
    }

    async fn get_provider_auth_url(&self, _provider: &str) -> Result<String> {
        anyhow::bail!("Provider auth URL not yet implemented")
    }

    async fn wait_for_provider_auth(&self, _provider: &str) -> Result<()> {
        Ok(())
    }

    async fn set_provider_token(&self, _provider: &str, _token: &str) -> Result<()> {
        Ok(())
    }

    async fn list_provider_auths(&self) -> Result<Vec<ProviderAuth>> {
        Ok(vec![])
    }

    async fn remove_provider_credentials(&self, _provider: &str) -> Result<()> {
        Ok(())
    }

    async fn test_provider_auth(&self, _provider: &str) -> Result<AuthTestResult> {
        anyhow::bail!("Provider auth test not yet implemented")
    }

    async fn get_provider_order(&self) -> Result<Vec<String>> {
        Ok(vec![])
    }

    async fn set_provider_order(&self, _providers: &[String]) -> Result<()> {
        Ok(())
    }

    async fn add_provider_to_order(&self, _provider: &str, _position: usize) -> Result<()> {
        Ok(())
    }

    async fn remove_provider_from_order(&self, _provider: &str) -> Result<()> {
        Ok(())
    }

    async fn clear_provider_order(&self) -> Result<()> {
        Ok(())
    }

    async fn list_model_aliases(&self) -> Result<Vec<ModelAlias>> {
        Ok(vec![])
    }

    async fn add_model_alias(&self, _name: &str, _target: &str) -> Result<()> {
        Ok(())
    }

    async fn remove_model_alias(&self, _name: &str) -> Result<()> {
        Ok(())
    }

    async fn get_model_alias(&self, _name: &str) -> Result<ModelAlias> {
        anyhow::bail!("Model alias not found")
    }

    async fn get_fallback_chain(&self, _task_type: &str) -> Result<Vec<String>> {
        Ok(vec![])
    }

    async fn add_to_fallback_chain(
        &self,
        _model: &str,
        _position: Option<usize>,
        _task_type: &str,
    ) -> Result<()> {
        Ok(())
    }

    async fn remove_from_fallback_chain(&self, _model: &str, _task_type: &str) -> Result<()> {
        Ok(())
    }

    async fn clear_fallback_chain(&self, _task_type: &str) -> Result<()> {
        Ok(())
    }

    async fn test_fallback_chain(&self, _task_type: &str) -> Result<Vec<FallbackTestResult>> {
        Ok(vec![])
    }

    async fn chat(&self, _model: Option<&str>, _messages: &[ChatMessage]) -> Result<ChatMessage> {
        anyhow::bail!("Chat not yet implemented")
    }

    async fn chat_stream(
        &self,
        _model: Option<&str>,
        _messages: &[ChatMessage],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        anyhow::bail!("Chat streaming not yet implemented")
    }

    async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
        anyhow::bail!("Completion not yet implemented")
    }

    async fn complete_stream(
        &self,
        _request: &CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        anyhow::bail!("Completion streaming not yet implemented")
    }

    async fn generate_embeddings(&self, _model: Option<&str>, _text: &str) -> Result<Embedding> {
        anyhow::bail!("Embeddings not yet implemented")
    }

    async fn update_model_list(&self) -> Result<ModelUpdateResult> {
        anyhow::bail!("Model list update not yet implemented")
    }
}

// ============================================================================
// Infer Module Types and Trait
// ============================================================================

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Capability {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
}

#[derive(Debug, serde::Serialize)]
pub struct TextRequest {
    pub prompt: String,
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stream: bool,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct TextResponse {
    pub text: String,
    pub model: String,
    pub tokens_used: usize,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct SentimentResult {
    pub sentiment: String,
    pub score: f32,
    pub confidence: f32,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Entity {
    pub text: String,
    pub entity_type: String,
    pub start: usize,
    pub end: usize,
    pub confidence: f32,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Translation {
    pub text: String,
    pub source_language: String,
    pub target_language: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Answer {
    pub answer: String,
    pub confidence: f32,
    pub sources: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct ImageGenRequest {
    pub prompt: String,
    pub model: Option<String>,
    pub size: Option<String>,
    pub quality: Option<String>,
    pub style: Option<String>,
    pub n: u32,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ImageGenResult {
    pub paths: Vec<PathBuf>,
    pub revised_prompts: Vec<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ImageAnalysis {
    pub description: String,
    pub objects: Vec<String>,
    pub text_found: Vec<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Transcription {
    pub text: String,
    pub language: String,
    pub duration_secs: f64,
    pub segments: Vec<TranscriptSegment>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct TranscriptSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Voice {
    pub id: String,
    pub name: String,
    pub language: String,
    pub gender: String,
    pub preview_url: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct WebSearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct CrawledPage {
    pub url: String,
    pub title: String,
    pub content: String,
    pub links: Vec<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct CodeReview {
    pub summary: String,
    pub issues: Vec<CodeIssue>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct CodeIssue {
    pub severity: String,
    pub line: Option<usize>,
    pub message: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct DocumentAnalysis {
    pub summary: String,
    pub key_points: Vec<String>,
    pub entities: Vec<Entity>,
}

#[async_trait::async_trait]
pub trait InferClient {
    async fn list_capabilities(&self, category: Option<&str>) -> Result<Vec<Capability>>;
    async fn get_capability(&self, id: &str) -> Result<Capability>;
    async fn generate_text(&self, request: &TextRequest) -> Result<TextResponse>;
    async fn stream_text(
        &self,
        request: &TextRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>>;
    async fn summarize(&self, text: &str, length: &str) -> Result<String>;
    async fn analyze_sentiment(&self, text: &str) -> Result<SentimentResult>;
    async fn extract_entities(&self, text: &str, types: &[String]) -> Result<Vec<Entity>>;
    async fn classify_text(&self, text: &str, categories: &[String]) -> Result<Vec<(String, f32)>>;
    async fn translate(&self, text: &str, to: &str, from: Option<&str>) -> Result<Translation>;
    async fn answer_question(
        &self,
        question: &str,
        context: Option<&str>,
        web_search: bool,
    ) -> Result<Answer>;
    async fn generate_image(&self, request: &ImageGenRequest) -> Result<ImageGenResult>;
    async fn edit_image(
        &self,
        image: &PathBuf,
        prompt: &str,
        mask: Option<&PathBuf>,
        output: Option<&PathBuf>,
    ) -> Result<()>;
    async fn create_image_variations(
        &self,
        image: &PathBuf,
        n: u32,
        output: Option<&PathBuf>,
    ) -> Result<Vec<PathBuf>>;
    async fn describe_image(
        &self,
        image: &str,
        detail: &str,
        question: Option<&str>,
    ) -> Result<ImageAnalysis>;
    async fn ocr_image(&self, image: &PathBuf) -> Result<String>;
    async fn transcribe_audio(
        &self,
        file: &PathBuf,
        language: Option<&str>,
        timestamps: bool,
    ) -> Result<Transcription>;
    async fn translate_audio(&self, file: &PathBuf) -> Result<Transcription>;
    async fn generate_audio(&self, prompt: &str, duration: Option<f32>) -> Result<Vec<u8>>;
    async fn text_to_speech(
        &self,
        text: &str,
        voice: Option<&str>,
        speed: Option<f32>,
    ) -> Result<Vec<u8>>;
    async fn list_voices(&self, language: Option<&str>) -> Result<Vec<Voice>>;
    async fn web_search(
        &self,
        query: &str,
        limit: usize,
        site: Option<&str>,
    ) -> Result<Vec<WebSearchResult>>;
    async fn fetch_webpage(&self, url: &str, extract: bool, format: &str) -> Result<String>;
    async fn crawl_website(
        &self,
        url: &str,
        max_pages: usize,
        same_domain: bool,
    ) -> Result<Vec<CrawledPage>>;
    async fn create_embeddings(
        &self,
        texts: &[String],
        model: Option<&str>,
        batch_size: usize,
    ) -> Result<Vec<Vec<f32>>>;
    async fn calculate_similarity(
        &self,
        text1: &str,
        text2: &str,
        model: Option<&str>,
    ) -> Result<f32>;
    async fn cluster_texts(&self, texts: &[String], n_clusters: usize) -> Result<Vec<Cluster>>;
    async fn complete_code(
        &self,
        code: &str,
        language: Option<&str>,
        context: &[String],
    ) -> Result<String>;
    async fn explain_code(&self, code: &str, detail: &str) -> Result<String>;
    async fn review_code(&self, code: &str, focus: &[String]) -> Result<CodeReview>;
    async fn fix_code(&self, code: &str, issue: Option<&str>) -> Result<String>;
    async fn generate_tests(&self, code: &str, framework: Option<&str>) -> Result<String>;
    async fn generate_docs(&self, code: &str, format: &str) -> Result<String>;
    async fn multimodal_chat(
        &self,
        images: &[PathBuf],
        prompt: &str,
        model: Option<&str>,
    ) -> Result<String>;
    async fn analyze_document(
        &self,
        file: &PathBuf,
        questions: &[String],
        structured: bool,
    ) -> Result<DocumentAnalysis>;
}

#[async_trait::async_trait]
impl InferClient for ApiClient {
    async fn list_capabilities(&self, _category: Option<&str>) -> Result<Vec<Capability>> {
        Ok(vec![])
    }

    async fn get_capability(&self, _id: &str) -> Result<Capability> {
        anyhow::bail!("Capability not found")
    }

    async fn generate_text(&self, _request: &TextRequest) -> Result<TextResponse> {
        anyhow::bail!("Text generation not yet implemented")
    }

    async fn stream_text(
        &self,
        _request: &TextRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        anyhow::bail!("Text streaming not yet implemented")
    }

    async fn summarize(&self, _text: &str, _length: &str) -> Result<String> {
        anyhow::bail!("Summarization not yet implemented")
    }

    async fn analyze_sentiment(&self, _text: &str) -> Result<SentimentResult> {
        anyhow::bail!("Sentiment analysis not yet implemented")
    }

    async fn extract_entities(&self, _text: &str, _types: &[String]) -> Result<Vec<Entity>> {
        Ok(vec![])
    }

    async fn classify_text(
        &self,
        _text: &str,
        _categories: &[String],
    ) -> Result<Vec<(String, f32)>> {
        Ok(vec![])
    }

    async fn translate(&self, _text: &str, _to: &str, _from: Option<&str>) -> Result<Translation> {
        anyhow::bail!("Translation not yet implemented")
    }

    async fn answer_question(
        &self,
        _question: &str,
        _context: Option<&str>,
        _web_search: bool,
    ) -> Result<Answer> {
        anyhow::bail!("Question answering not yet implemented")
    }

    async fn generate_image(&self, _request: &ImageGenRequest) -> Result<ImageGenResult> {
        anyhow::bail!("Image generation not yet implemented")
    }

    async fn edit_image(
        &self,
        _image: &PathBuf,
        _prompt: &str,
        _mask: Option<&PathBuf>,
        _output: Option<&PathBuf>,
    ) -> Result<()> {
        Ok(())
    }

    async fn create_image_variations(
        &self,
        _image: &PathBuf,
        _n: u32,
        _output: Option<&PathBuf>,
    ) -> Result<Vec<PathBuf>> {
        Ok(vec![])
    }

    async fn describe_image(
        &self,
        _image: &str,
        _detail: &str,
        _question: Option<&str>,
    ) -> Result<ImageAnalysis> {
        anyhow::bail!("Image description not yet implemented")
    }

    async fn ocr_image(&self, _image: &PathBuf) -> Result<String> {
        anyhow::bail!("OCR not yet implemented")
    }

    async fn transcribe_audio(
        &self,
        _file: &PathBuf,
        _language: Option<&str>,
        _timestamps: bool,
    ) -> Result<Transcription> {
        anyhow::bail!("Audio transcription not yet implemented")
    }

    async fn translate_audio(&self, _file: &PathBuf) -> Result<Transcription> {
        anyhow::bail!("Audio translation not yet implemented")
    }

    async fn generate_audio(&self, _prompt: &str, _duration: Option<f32>) -> Result<Vec<u8>> {
        anyhow::bail!("Audio generation not yet implemented")
    }

    async fn text_to_speech(
        &self,
        _text: &str,
        _voice: Option<&str>,
        _speed: Option<f32>,
    ) -> Result<Vec<u8>> {
        anyhow::bail!("Text-to-speech not yet implemented")
    }

    async fn list_voices(&self, _language: Option<&str>) -> Result<Vec<Voice>> {
        Ok(vec![])
    }

    async fn web_search(
        &self,
        _query: &str,
        _limit: usize,
        _site: Option<&str>,
    ) -> Result<Vec<WebSearchResult>> {
        Ok(vec![])
    }

    async fn fetch_webpage(&self, _url: &str, _extract: bool, _format: &str) -> Result<String> {
        anyhow::bail!("Webpage fetching not yet implemented")
    }

    async fn crawl_website(
        &self,
        _url: &str,
        _max_pages: usize,
        _same_domain: bool,
    ) -> Result<Vec<CrawledPage>> {
        Ok(vec![])
    }

    async fn create_embeddings(
        &self,
        _texts: &[String],
        _model: Option<&str>,
        _batch_size: usize,
    ) -> Result<Vec<Vec<f32>>> {
        Ok(vec![])
    }

    async fn calculate_similarity(
        &self,
        _text1: &str,
        _text2: &str,
        _model: Option<&str>,
    ) -> Result<f32> {
        Ok(0.0)
    }

    async fn cluster_texts(&self, _texts: &[String], _n_clusters: usize) -> Result<Vec<Cluster>> {
        Ok(vec![])
    }

    async fn complete_code(
        &self,
        _code: &str,
        _language: Option<&str>,
        _context: &[String],
    ) -> Result<String> {
        anyhow::bail!("Code completion not yet implemented")
    }

    async fn explain_code(&self, _code: &str, _detail: &str) -> Result<String> {
        anyhow::bail!("Code explanation not yet implemented")
    }

    async fn review_code(&self, _code: &str, _focus: &[String]) -> Result<CodeReview> {
        Ok(CodeReview {
            summary: String::new(),
            issues: vec![],
        })
    }

    async fn fix_code(&self, _code: &str, _issue: Option<&str>) -> Result<String> {
        anyhow::bail!("Code fixing not yet implemented")
    }

    async fn generate_tests(&self, _code: &str, _framework: Option<&str>) -> Result<String> {
        anyhow::bail!("Test generation not yet implemented")
    }

    async fn generate_docs(&self, _code: &str, _format: &str) -> Result<String> {
        anyhow::bail!("Documentation generation not yet implemented")
    }

    async fn multimodal_chat(
        &self,
        _images: &[PathBuf],
        _prompt: &str,
        _model: Option<&str>,
    ) -> Result<String> {
        anyhow::bail!("Multimodal chat not yet implemented")
    }

    async fn analyze_document(
        &self,
        _file: &PathBuf,
        _questions: &[String],
        _structured: bool,
    ) -> Result<DocumentAnalysis> {
        anyhow::bail!("Document analysis not yet implemented")
    }
}
