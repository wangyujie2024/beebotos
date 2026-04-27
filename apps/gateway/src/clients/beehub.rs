//! BeeHub Client
//!
//! Client for interacting with internal BeeHub skill registry.

use std::time::Duration;

use reqwest::Client;

use super::{HubError, SkillMetadata};

/// BeeHub API client
#[derive(Debug, Clone)]
pub struct BeeHubClient {
    http: Client,
    base_url: String,
    api_key: Option<String>,
}

impl BeeHubClient {
    /// Create new BeeHub client
    pub fn new() -> Result<Self, HubError> {
        let base_url =
            std::env::var("BEEHUB_URL").unwrap_or_else(|_| "http://localhost:3001".to_string());

        let api_key = std::env::var("BEEHUB_API_KEY").ok();

        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| HubError::Network(e.to_string()))?;

        Ok(Self {
            http,
            base_url,
            api_key,
        })
    }

    /// Create with custom configuration
    pub fn with_config(base_url: String, api_key: Option<String>) -> Result<Self, HubError> {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| HubError::Network(e.to_string()))?;

        Ok(Self {
            http,
            base_url,
            api_key,
        })
    }

    /// Build request with auth headers
    fn build_request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.http.request(method, &url);

        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        req.header("User-Agent", "BeeBotOS-Gateway/1.0")
    }

    /// List all skills from BeeHub
    pub async fn list_skills(&self) -> Result<Vec<SkillMetadata>, HubError> {
        self.search_skills("").await
    }

    /// Search skills from BeeHub
    pub async fn search_skills(&self, query: &str) -> Result<Vec<SkillMetadata>, HubError> {
        let path = if query.is_empty() {
            "/skills".to_string()
        } else {
            format!("/skills?search={}", urlencoding::encode(query))
        };

        let req = self.build_request(reqwest::Method::GET, &path);

        let resp = req
            .send()
            .await
            .map_err(|e| HubError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(HubError::InvalidResponse(format!(
                "List skills failed: {}",
                resp.status()
            )));
        }

        let skills: Vec<SkillMetadata> = resp
            .json()
            .await
            .map_err(|e| HubError::InvalidResponse(e.to_string()))?;

        Ok(skills)
    }

    /// Get skill metadata
    pub async fn get_skill(&self, id: &str) -> Result<SkillMetadata, HubError> {
        let req = self.build_request(reqwest::Method::GET, &format!("/skills/{}", id));

        let resp = req
            .send()
            .await
            .map_err(|e| HubError::Network(e.to_string()))?;

        match resp.status() {
            reqwest::StatusCode::OK => {
                let skill: SkillMetadata = resp
                    .json()
                    .await
                    .map_err(|e| HubError::InvalidResponse(e.to_string()))?;
                Ok(skill)
            }
            reqwest::StatusCode::NOT_FOUND => Err(HubError::NotFound(id.to_string())),
            status => Err(HubError::InvalidResponse(format!(
                "Get skill failed: {}",
                status
            ))),
        }
    }

    /// Download skill package
    pub async fn download_skill(
        &self,
        id: &str,
        _version: Option<&str>,
    ) -> Result<Vec<u8>, HubError> {
        let req = self.build_request(reqwest::Method::GET, &format!("/skills/{}/download", id));

        let resp = req
            .send()
            .await
            .map_err(|e| HubError::Network(e.to_string()))?;

        match resp.status() {
            reqwest::StatusCode::OK => {
                let bytes = resp
                    .bytes()
                    .await
                    .map_err(|e| HubError::DownloadFailed(e.to_string()))?;
                Ok(bytes.to_vec())
            }
            reqwest::StatusCode::NOT_FOUND => Err(HubError::NotFound(id.to_string())),
            status => Err(HubError::DownloadFailed(format!(
                "Download failed: {}",
                status
            ))),
        }
    }

    /// Publish skill to BeeHub (internal use)
    pub async fn publish_skill(
        &self,
        id: &str,
        name: &str,
        version: &str,
        package: Vec<u8>,
    ) -> Result<String, HubError> {
        let req = self
            .build_request(reqwest::Method::POST, "/publish")
            .multipart(
                reqwest::multipart::Form::new()
                    .text("id", id.to_string())
                    .text("name", name.to_string())
                    .text("version", version.to_string())
                    .part("package", reqwest::multipart::Part::bytes(package)),
            );

        let resp = req
            .send()
            .await
            .map_err(|e| HubError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(HubError::InvalidResponse(format!(
                "Publish failed: {}",
                resp.status()
            )));
        }

        #[derive(serde::Deserialize)]
        struct PublishResponse {
            id: String,
        }

        let result: PublishResponse = resp
            .json()
            .await
            .map_err(|e| HubError::InvalidResponse(e.to_string()))?;

        Ok(result.id)
    }

    /// Check if BeeHub is available
    pub async fn health_check(&self) -> Result<bool, HubError> {
        let req = self.build_request(reqwest::Method::GET, "/health");

        match req.send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }
}

impl Default for BeeHubClient {
    fn default() -> Self {
        Self::new().expect("Failed to create BeeHubClient")
    }
}
