//! ClawHub Client
//!
//! Client for interacting with ClawHub skill marketplace.

use std::time::Duration;

use reqwest::Client;

use super::{HubError, SkillMetadata};

/// ClawHub API client
#[derive(Debug, Clone)]
pub struct ClawHubClient {
    http: Client,
    base_url: String,
    api_key: Option<String>,
}

impl ClawHubClient {
    /// Create new ClawHub client
    pub fn new() -> Result<Self, HubError> {
        let base_url =
            std::env::var("CLAWHUB_URL").unwrap_or_else(|_| "https://hub.claw.dev/v1".to_string());

        let api_key = std::env::var("CLAWHUB_API_KEY").ok();

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

    /// Search skills on ClawHub
    pub async fn search_skills(&self, query: &str) -> Result<Vec<SkillMetadata>, HubError> {
        let req = self.build_request(
            reqwest::Method::GET,
            &format!("/skills/search?q={}", urlencoding::encode(query)),
        );

        let resp = req
            .send()
            .await
            .map_err(|e| HubError::Network(e.to_string()))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(vec![]);
        }

        if !resp.status().is_success() {
            return Err(HubError::InvalidResponse(format!(
                "Search failed: {}",
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

    /// Download skill package (WASM + manifest)
    pub async fn download_skill(
        &self,
        id: &str,
        version: Option<&str>,
    ) -> Result<Vec<u8>, HubError> {
        let path = if let Some(ver) = version {
            format!("/skills/{}/download?version={}", id, ver)
        } else {
            format!("/skills/{}/download", id)
        };

        let req = self.build_request(reqwest::Method::GET, &path);

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
            reqwest::StatusCode::UNAUTHORIZED => {
                Err(HubError::AuthFailed("Invalid API key".to_string()))
            }
            status => Err(HubError::DownloadFailed(format!(
                "Download failed: {}",
                status
            ))),
        }
    }

    /// Get skill versions
    pub async fn get_versions(&self, id: &str) -> Result<Vec<String>, HubError> {
        let req = self.build_request(reqwest::Method::GET, &format!("/skills/{}/versions", id));

        let resp = req
            .send()
            .await
            .map_err(|e| HubError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(HubError::InvalidResponse(format!(
                "Get versions failed: {}",
                resp.status()
            )));
        }

        let versions: Vec<String> = resp
            .json()
            .await
            .map_err(|e| HubError::InvalidResponse(e.to_string()))?;

        Ok(versions)
    }

    /// Check if ClawHub is available
    pub async fn health_check(&self) -> Result<bool, HubError> {
        let req = self.build_request(reqwest::Method::GET, "/health");

        match req.send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }
}
