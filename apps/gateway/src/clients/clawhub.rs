//! ClawHub Client
//!
//! Client for interacting with ClawHub skill marketplace.

use super::{HubError, SkillMetadata};
use reqwest::Client;
use std::time::Duration;

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
        let base_url = std::env::var("CLAWHUB_URL")
            .unwrap_or_else(|_| "https://clawhub.ai/api/v1".to_string());
        
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
    /// 
    /// Uses two different endpoints depending on whether a query is provided:
    /// - Empty query: GET /skills → returns `{"items": [...], "nextCursor": null}`
    /// - Non-empty query: GET /search?q=... → returns `{"results": [...]}`
    pub async fn search_skills(&self, query: &str) -> Result<Vec<SkillMetadata>, HubError> {
        if query.is_empty() {
            self.list_skills().await
        } else {
            self.search_skills_query(query).await
        }
    }
    
    /// List all skills (empty search query)
    /// 
    /// Endpoint: GET /skills
    /// Response: `{"items": [...], "nextCursor": null}`
    async fn list_skills(&self) -> Result<Vec<SkillMetadata>, HubError> {
        let req = self.build_request(reqwest::Method::GET, "/skills?sort=downloads&limit=200");
        
        let resp = req
            .send()
            .await
            .map_err(|e| HubError::Network(e.to_string()))?;
        
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(vec![]);
        }
        
        if !resp.status().is_success() {
            return Err(HubError::InvalidResponse(
                format!("List skills failed: {}", resp.status())
            ));
        }
        
        #[derive(serde::Deserialize)]
        struct ClawHubListResponse {
            #[serde(default)]
            items: Vec<ClawHubListItem>,
            #[serde(default)]
            next_cursor: Option<String>,
        }
        
        #[derive(serde::Deserialize)]
        struct ClawHubListItem {
            slug: String,
            #[serde(default)]
            display_name: Option<String>,
            #[serde(rename = "displayName", default)]
            display_name_alt: Option<String>,
            #[serde(default)]
            summary: Option<String>,
            #[serde(default)]
            version: Option<String>,
            #[serde(default)]
            updated_at: Option<i64>,
            #[serde(rename = "updatedAt", default)]
            updated_at_alt: Option<i64>,
        }
        
        let body: ClawHubListResponse = resp
            .json()
            .await
            .map_err(|e| HubError::InvalidResponse(format!("JSON parse error: {}", e)))?;
        
        let skills: Vec<SkillMetadata> = body.items.into_iter().map(|r| {
            let name = r.display_name.or(r.display_name_alt).unwrap_or_else(|| r.slug.clone());
            SkillMetadata {
                id: r.slug.clone(),
                name,
                version: r.version.unwrap_or_else(|| "1.0.0".to_string()),
                description: r.summary.unwrap_or_default(),
                author: "ClawHub".to_string(),
                license: "MIT".to_string(),
                repository: None,
                hash: String::new(),
                downloads: 0,
                rating: 0.0,
                capabilities: vec![],
                tags: vec![],
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            }
        }).collect();
        
        Ok(skills)
    }
    
    /// Search skills with a query string
    /// 
    /// Endpoint: GET /search?q=...
    /// Response: `{"results": [...]}`
    async fn search_skills_query(&self, query: &str) -> Result<Vec<SkillMetadata>, HubError> {
        let path = format!("/search?q={}", urlencoding::encode(query));
        let req = self.build_request(reqwest::Method::GET, &path);
        
        let resp = req
            .send()
            .await
            .map_err(|e| HubError::Network(e.to_string()))?;
        
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(vec![]);
        }
        
        if !resp.status().is_success() {
            return Err(HubError::InvalidResponse(
                format!("Search failed: {}", resp.status())
            ));
        }
        
        #[derive(serde::Deserialize)]
        struct ClawHubSearchResponse {
            #[serde(default)]
            results: Vec<ClawHubSearchResult>,
        }
        
        #[derive(serde::Deserialize)]
        struct ClawHubSearchResult {
            slug: String,
            #[serde(default)]
            display_name: Option<String>,
            #[serde(rename = "displayName", default)]
            display_name_alt: Option<String>,
            #[serde(default)]
            summary: Option<String>,
            #[serde(default)]
            version: Option<String>,
            #[serde(default)]
            updated_at: Option<i64>,
            #[serde(rename = "updatedAt", default)]
            updated_at_alt: Option<i64>,
        }
        
        let body: ClawHubSearchResponse = resp
            .json()
            .await
            .map_err(|e| HubError::InvalidResponse(format!("JSON parse error: {}", e)))?;
        
        let skills: Vec<SkillMetadata> = body.results.into_iter().map(|r| {
            let name = r.display_name.or(r.display_name_alt).unwrap_or_else(|| r.slug.clone());
            SkillMetadata {
                id: r.slug.clone(),
                name,
                version: r.version.unwrap_or_else(|| "1.0.0".to_string()),
                description: r.summary.unwrap_or_default(),
                author: "ClawHub".to_string(),
                license: "MIT".to_string(),
                repository: None,
                hash: String::new(),
                downloads: 0,
                rating: 0.0,
                capabilities: vec![],
                tags: vec![],
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            }
        }).collect();
        
        Ok(skills)
    }
    
    /// Get skill metadata
    pub async fn get_skill(&self, id: &str) -> Result<SkillMetadata, HubError> {
        let req = self.build_request(
            reqwest::Method::GET,
            &format!("/skills/{}", id),
        );
        
        let resp = req
            .send()
            .await
            .map_err(|e| HubError::Network(e.to_string()))?;
        
        match resp.status() {
            reqwest::StatusCode::OK => {
                #[derive(serde::Deserialize)]
                struct ClawHubSkillDetail {
                    skill: ClawHubSkillInfo,
                    #[serde(default)]
                    latest_version: Option<ClawHubVersionInfo>,
                    #[serde(rename = "latestVersion", default)]
                    latest_version_alt: Option<ClawHubVersionInfo>,
                    #[serde(default)]
                    owner: Option<ClawHubOwner>,
                }
                
                #[derive(serde::Deserialize)]
                struct ClawHubSkillInfo {
                    slug: String,
                    #[serde(default)]
                    display_name: Option<String>,
                    #[serde(rename = "displayName", default)]
                    display_name_alt: Option<String>,
                    #[serde(default)]
                    summary: Option<String>,
                    #[serde(default)]
                    tags: Option<serde_json::Value>,
                    #[serde(default)]
                    stats: Option<ClawHubStats>,
                }
                
                #[derive(serde::Deserialize)]
                struct ClawHubVersionInfo {
                    #[serde(default)]
                    version: Option<String>,
                    #[serde(default)]
                    license: Option<String>,
                }
                
                #[derive(serde::Deserialize)]
                struct ClawHubStats {
                    #[serde(default)]
                    downloads: u64,
                    #[serde(default)]
                    stars: u64,
                }
                
                #[derive(serde::Deserialize)]
                struct ClawHubOwner {
                    #[serde(default)]
                    handle: Option<String>,
                    #[serde(default)]
                    display_name: Option<String>,
                    #[serde(rename = "displayName", default)]
                    display_name_alt: Option<String>,
                }
                
                let detail: ClawHubSkillDetail = resp
                    .json()
                    .await
                    .map_err(|e| HubError::InvalidResponse(format!("JSON parse error: {}", e)))?;
                
                let skill = detail.skill;
                let version_info = detail.latest_version.or(detail.latest_version_alt);
                let owner = detail.owner;
                let stats = skill.stats.unwrap_or(ClawHubStats { downloads: 0, stars: 0 });
                
                let name = skill.display_name.or(skill.display_name_alt).unwrap_or_else(|| skill.slug.clone());
                let author = owner.as_ref()
                    .and_then(|o| o.handle.clone().or(o.display_name.clone().or(o.display_name_alt.clone())))
                    .unwrap_or_else(|| "ClawHub".to_string());
                let version = version_info.as_ref().and_then(|v| v.version.clone()).unwrap_or_else(|| "1.0.0".to_string());
                let license = version_info.as_ref().and_then(|v| v.license.clone()).unwrap_or_else(|| "MIT".to_string());
                
                Ok(SkillMetadata {
                    id: skill.slug,
                    name,
                    version,
                    description: skill.summary.unwrap_or_default(),
                    author,
                    license,
                    repository: None,
                    hash: String::new(),
                    downloads: stats.downloads,
                    rating: stats.stars as f32,
                    capabilities: vec![],
                    tags: vec![],
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                })
            }
            reqwest::StatusCode::NOT_FOUND => {
                Err(HubError::DownloadNotSupported)
            }
            status => {
                Err(HubError::InvalidResponse(
                    format!("Get skill failed: {}", status)
                ))
            }
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
            reqwest::StatusCode::NOT_FOUND => {
                Err(HubError::DownloadNotSupported)
            }
            reqwest::StatusCode::UNAUTHORIZED => {
                Err(HubError::AuthFailed("Invalid API key".to_string()))
            }
            status => {
                Err(HubError::DownloadFailed(
                    format!("Download failed: {}", status)
                ))
            }
        }
    }
    
    /// Get skill versions
    pub async fn get_versions(&self, id: &str) -> Result<Vec<String>, HubError> {
        let req = self.build_request(
            reqwest::Method::GET,
            &format!("/skills/{}/versions", id),
        );
        
        let resp = req
            .send()
            .await
            .map_err(|e| HubError::Network(e.to_string()))?;
        
        if !resp.status().is_success() {
            return Err(HubError::InvalidResponse(
                format!("Get versions failed: {}", resp.status())
            ));
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


