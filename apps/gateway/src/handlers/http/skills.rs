//! Skills HTTP Handlers
//!
//! Handles skill installation, management from ClawHub/BeeHub.
//! Skills are Markdown-based (SKILL.md + YAML frontmatter) and used
//! by the Agent through tool-calling, not executed directly.

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

use crate::clients::{BeeHubClient, ClawHubClient, HubType, SkillMetadata};
use crate::error::GatewayError;
use crate::AppState;

/// Install skill request
#[derive(Debug, Deserialize)]
pub struct InstallSkillRequest {
    /// Skill source (ID or name)
    pub source: String,
    /// Target agent ID (optional)
    pub agent_id: Option<String>,
    /// Version constraint (optional)
    pub version: Option<String>,
    /// Hub to use (default: clawhub)
    pub hub: Option<String>,
}

/// Install skill response
#[derive(Debug, Serialize)]
pub struct InstallSkillResponse {
    pub success: bool,
    pub skill_id: String,
    pub name: String,
    pub version: String,
    pub message: String,
    pub installed_path: String,
}

/// List skills query parameters
#[derive(Debug, Deserialize)]
pub struct ListSkillsQuery {
    /// Filter by category
    pub category: Option<String>,
    /// Search query
    pub search: Option<String>,
    /// Hub to query (default: local)
    pub hub: Option<String>,
}

/// Skill info response
#[derive(Debug, Serialize)]
pub struct SkillInfoResponse {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub license: String,
    pub installed: bool,
    pub capabilities: Vec<String>,
    pub tags: Vec<String>,
    pub downloads: u64,
    pub rating: f32,
}

/// Install a skill from ClawHub or BeeHub
pub async fn install_skill(
    State(state): State<Arc<AppState>>,
    Json(req): Json<InstallSkillRequest>,
) -> Result<Json<InstallSkillResponse>, GatewayError> {
    info!("Installing skill: {} from hub: {:?}", req.source, req.hub);

    let hub_type = req
        .hub
        .as_deref()
        .and_then(|h| h.parse::<HubType>().ok())
        .unwrap_or_default();

    // Fetch skill metadata from hub
    let metadata = match hub_type {
        HubType::ClawHub => {
            let client = ClawHubClient::new().map_err(|e| GatewayError::Internal {
                message: format!("Failed to create ClawHub client: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            })?;

            client.get_skill(&req.source).await.map_err(|e| GatewayError::Internal {
                message: format!("Failed to get skill from ClawHub: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            })?
        }
        HubType::BeeHub => {
            let client = BeeHubClient::new().map_err(|e| GatewayError::Internal {
                message: format!("Failed to create BeeHub client: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            })?;

            client.get_skill(&req.source).await.map_err(|e| GatewayError::Internal {
                message: format!("Failed to get skill from BeeHub: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            })?
        }
    };

    info!(
        "Found skill: {} v{} from {}",
        metadata.name, metadata.version, metadata.author
    );

    // Check if already installed
    let skill_dir = get_skill_install_path(&metadata.id);
    if skill_dir.exists() {
        warn!("Skill {} is already installed at {:?}", metadata.id, skill_dir);
        return Ok(Json(InstallSkillResponse {
            success: true,
            skill_id: metadata.id,
            name: metadata.name,
            version: metadata.version,
            message: "Skill is already installed".to_string(),
            installed_path: skill_dir.to_string_lossy().to_string(),
        }));
    }

    // Download skill content
    let download_result = match hub_type {
        HubType::ClawHub => {
            let client = ClawHubClient::new().map_err(|e| GatewayError::Internal {
                message: format!("Failed to create ClawHub client: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            })?;
            client.download_skill(&req.source, req.version.as_deref()).await
        }
        HubType::BeeHub => {
            let client = BeeHubClient::new().map_err(|e| GatewayError::Internal {
                message: format!("Failed to create BeeHub client: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            })?;
            client.download_skill(&req.source, req.version.as_deref()).await
        }
    };

    match download_result {
        Ok(content_bytes) => {
            let content = String::from_utf8(content_bytes)
                .map_err(|e| GatewayError::Internal {
                    message: format!("Invalid UTF-8 in skill content: {}", e),
                    correlation_id: uuid::Uuid::new_v4().to_string(),
                })?;
            install_skill_content(&metadata, &content)
                .await
                .map_err(|e| GatewayError::Internal {
                    message: format!("Failed to install skill: {}", e),
                    correlation_id: uuid::Uuid::new_v4().to_string(),
                })?;
        }
        Err(crate::clients::HubError::DownloadNotSupported) => {
            info!(
                "Hub does not support downloads for {}; installing metadata-only stub",
                metadata.id
            );
            let hub_label = match hub_type {
                HubType::ClawHub => "clawhub",
                HubType::BeeHub => "beehub",
            };
            install_skill_metadata_only(&metadata, hub_label)
                .await
                .map_err(|e| GatewayError::Internal {
                    message: format!("Failed to install skill metadata: {}", e),
                    correlation_id: uuid::Uuid::new_v4().to_string(),
                })?;
        }
        Err(e) => {
            return Err(GatewayError::Internal {
                message: format!("Failed to download skill: {}", e),
                correlation_id: uuid::Uuid::new_v4().to_string(),
            });
        }
    }

    // Load and register to SkillRegistry if available
    if let Some(ref registry) = state.skill_registry {
        let mut loader = beebotos_agents::skills::SkillLoader::new();
        loader.add_path(get_skills_base_dir());
        match loader.load_skill(&metadata.id).await {
            Ok(skill) => {
                registry
                    .register(
                        skill,
                        metadata.tags.first().map(|s| s.as_str()).unwrap_or("general"),
                        metadata.tags.clone(),
                    )
                    .await;
                info!("Registered skill {} to registry", metadata.id);
            }
            Err(e) => {
                warn!("Failed to load skill into registry: {}", e);
            }
        }
    }

    info!("Successfully installed skill {} to {:?}", metadata.id, skill_dir);

    Ok(Json(InstallSkillResponse {
        success: true,
        skill_id: metadata.id.clone(),
        name: metadata.name,
        version: metadata.version,
        message: "Skill installed successfully".to_string(),
        installed_path: skill_dir.to_string_lossy().to_string(),
    }))
}

/// List installed skills or search from hub
pub async fn list_skills(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListSkillsQuery>,
) -> Result<Json<Vec<SkillInfoResponse>>, GatewayError> {
    let hub_type = query.hub.as_deref().and_then(|h| h.parse::<HubType>().ok());

    // If hub is specified, search from remote hub
    if let Some(hub) = hub_type {
        let search_query = query.search.as_deref().unwrap_or("");

        let skills = match hub {
            HubType::ClawHub => {
                let client = ClawHubClient::new().map_err(|e| {
                    GatewayError::service_unavailable(
                        "ClawHub",
                        format!("ClawHub client initialization failed: {}", e),
                    )
                })?;

                match client.search_skills(search_query).await {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("ClawHub search failed (query='{}'): {}", search_query, e);
                        return Ok(Json(vec![]));
                    }
                }
            }
            HubType::BeeHub => {
                let client = BeeHubClient::new().map_err(|e| {
                    GatewayError::service_unavailable(
                        "BeeHub",
                        format!("BeeHub client initialization failed: {}", e),
                    )
                })?;

                match client.search_skills(search_query).await {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("BeeHub search failed (query='{}'): {}", search_query, e);
                        return Ok(Json(vec![]));
                    }
                }
            }
        };

        let responses: Vec<SkillInfoResponse> = skills
            .into_iter()
            .map(|s| SkillInfoResponse {
                id: s.id.clone(),
                name: s.name,
                version: s.version,
                description: s.description,
                author: s.author,
                license: s.license,
                installed: is_skill_installed(&s.id),
                capabilities: s.capabilities,
                tags: s.tags,
                downloads: s.downloads,
                rating: s.rating,
            })
            .collect();

        return Ok(Json(responses));
    }

    // Otherwise, list locally installed skills
    let skills = list_installed_skills()
        .await
        .map_err(|e| GatewayError::Internal {
            message: format!("Failed to list installed skills: {}", e),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

    Ok(Json(skills))
}

/// Get skill details
pub async fn get_skill(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<SkillInfoResponse>, GatewayError> {
    let skill = get_skill_info(&id).await.map_err(|e| GatewayError::NotFound {
        resource: format!("Skill: {}", id),
        id: id.clone(),
    })?;

    Ok(Json(skill))
}

/// Uninstall a skill
pub async fn uninstall_skill(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    info!("Uninstalling skill: {}", id);

    let skill_dir = get_skill_install_path(&id);
    if !skill_dir.exists() {
        return Err(GatewayError::NotFound {
            resource: "Skill".to_string(),
            id: id.clone(),
        });
    }

    tokio::fs::remove_dir_all(&skill_dir)
        .await
        .map_err(|e| GatewayError::Internal {
            message: format!("Failed to uninstall skill: {}", e),
            correlation_id: uuid::Uuid::new_v4().to_string(),
        })?;

    info!("Successfully uninstalled skill: {}", id);

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Skill {} uninstalled", id),
    })))
}

/// Check hub health
pub async fn hub_health() -> Result<Json<serde_json::Value>, GatewayError> {
    let clawhub_client = ClawHubClient::new();
    let beehub_client = BeeHubClient::new();

    let clawhub_healthy = if let Ok(client) = clawhub_client {
        client.health_check().await.unwrap_or(false)
    } else {
        false
    };

    let beehub_healthy = if let Ok(client) = beehub_client {
        client.health_check().await.unwrap_or(false)
    } else {
        false
    };

    Ok(Json(serde_json::json!({
        "clawhub": {
            "status": if clawhub_healthy { "healthy" } else { "unhealthy" },
            "url": std::env::var("CLAWHUB_URL").unwrap_or_else(|_| "https://clawhub.ai/api/v1".to_string()),
        },
        "beehub": {
            "status": if beehub_healthy { "healthy" } else { "unhealthy" },
            "url": std::env::var("BEEHUB_URL").unwrap_or_else(|_| "http://localhost:3001".to_string()),
        },
    })))
}

// Helper functions

/// Get skill installation directory
pub fn get_skill_install_path(skill_id: &str) -> std::path::PathBuf {
    get_skills_base_dir().join(skill_id)
}

/// Get base skills directory
pub fn get_skills_base_dir() -> std::path::PathBuf {
    std::env::var("BEEBOTOS_SKILLS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("data/skills"))
}

/// Check if skill is installed
fn is_skill_installed(skill_id: &str) -> bool {
    get_skill_install_path(skill_id).exists()
}

/// Install skill metadata-only stub (no package available from hub)
async fn install_skill_metadata_only(
    metadata: &SkillMetadata,
    hub_label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let skill_dir = get_skill_install_path(&metadata.id);
    tokio::fs::create_dir_all(&skill_dir).await?;

    // Write SKILL.md with YAML frontmatter
    let skill_md = format!(
        "---\n\
         id: {}\n\
         name: {}\n\
         version: {}\n\
         description: {}\n\
         author: {}\n\
         license: {}\n\
         capabilities:\n\
         {}\n\
         ---\n\
         \n\
         # {}\n\
         \n\
         {}\n",
        metadata.id,
        metadata.name,
        metadata.version,
        metadata.description,
        metadata.author,
        metadata.license,
        metadata
            .capabilities
            .iter()
            .map(|c| format!("  - {}", c))
            .collect::<Vec<_>>()
            .join("\n"),
        metadata.name,
        metadata.description
    );

    let skill_md_path = skill_dir.join("SKILL.md");
    tokio::fs::write(&skill_md_path, skill_md).await?;

    // Write _meta.json
    let meta = serde_json::json!({
        "source_hub": hub_label,
        "installed_at": chrono::Utc::now().to_rfc3339(),
    });
    let meta_path = skill_dir.join("_meta.json");
    tokio::fs::write(&meta_path, serde_json::to_string_pretty(&meta)?).await?;

    info!("Installed metadata-only skill stub to {:?}", skill_dir);
    Ok(())
}

/// Install skill content (Markdown) to disk
async fn install_skill_content(
    metadata: &SkillMetadata,
    content: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let skill_dir = get_skill_install_path(&metadata.id);
    tokio::fs::create_dir_all(&skill_dir).await?;

    // Write SKILL.md
    let skill_md_path = skill_dir.join("SKILL.md");
    tokio::fs::write(&skill_md_path, content).await?;

    // Write _meta.json
    let meta = serde_json::json!({
        "id": metadata.id,
        "name": metadata.name,
        "version": metadata.version,
        "author": metadata.author,
        "installed_at": chrono::Utc::now().to_rfc3339(),
    });
    let meta_path = skill_dir.join("_meta.json");
    tokio::fs::write(&meta_path, serde_json::to_string_pretty(&meta)?).await?;

    info!("Installed skill to {:?}", skill_dir);
    Ok(())
}

/// Extract string array from yaml value
fn yaml_string_array(value: &serde_yaml::Value) -> Vec<String> {
    value
        .as_sequence()
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

async fn list_installed_skills() -> Result<Vec<SkillInfoResponse>, Box<dyn std::error::Error>> {
    let base_dir = get_skills_base_dir();
    let mut skills = Vec::new();

    if let Ok(mut entries) = tokio::fs::read_dir(&base_dir).await {
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                let skill_id = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                // Try to read SKILL.md
                let skill_md_path = path.join("SKILL.md");
                if let Ok(content) = tokio::fs::read_to_string(&skill_md_path).await {
                    // Parse YAML frontmatter
                    let (frontmatter, _) = parse_frontmatter(&content)?;
                    if let Ok(manifest) = serde_yaml::from_str::<serde_yaml::Value>(&frontmatter) {
                        skills.push(SkillInfoResponse {
                            id: skill_id.clone(),
                            name: manifest["name"]
                                .as_str()
                                .unwrap_or(&skill_id)
                                .to_string(),
                            version: manifest["version"]
                                .as_str()
                                .unwrap_or("1.0.0")
                                .to_string(),
                            description: manifest["description"]
                                .as_str()
                                .unwrap_or("")
                                .to_string(),
                            author: manifest["author"]
                                .as_str()
                                .unwrap_or("Unknown")
                                .to_string(),
                            license: manifest["license"]
                                .as_str()
                                .unwrap_or("MIT")
                                .to_string(),
                            installed: true,
                            capabilities: yaml_string_array(&manifest["capabilities"]),
                            tags: yaml_string_array(&manifest["tags"]),
                            downloads: 0,
                            rating: 0.0,
                        });
                    }
                }
            }
        }
    }

    Ok(skills)
}

/// Get skill info from local storage
async fn get_skill_info(skill_id: &str) -> Result<SkillInfoResponse, Box<dyn std::error::Error>> {
    let skill_dir = get_skill_install_path(skill_id);
    let skill_md_path = skill_dir.join("SKILL.md");

    let content = tokio::fs::read_to_string(&skill_md_path).await?;
    let (frontmatter, _) = parse_frontmatter(&content)?;
    let manifest: serde_yaml::Value = serde_yaml::from_str(&frontmatter)?;

    Ok(SkillInfoResponse {
        id: skill_id.to_string(),
        name: manifest["name"].as_str().unwrap_or(skill_id).to_string(),
        version: manifest["version"]
            .as_str()
            .unwrap_or("1.0.0")
            .to_string(),
        description: manifest["description"].as_str().unwrap_or("").to_string(),
        author: manifest["author"]
            .as_str()
            .unwrap_or("Unknown")
            .to_string(),
        license: manifest["license"].as_str().unwrap_or("MIT").to_string(),
        installed: true,
        capabilities: yaml_string_array(&manifest["capabilities"]),
        tags: yaml_string_array(&manifest["tags"]),
        downloads: 0,
        rating: 0.0,
    })
}

/// Parse YAML frontmatter from markdown content
fn parse_frontmatter(content: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Ok((String::new(), content.to_string()));
    }

    let after_first = &trimmed[3..];
    let Some(end_pos) = after_first.find("---") else {
        return Err("Unclosed frontmatter".into());
    };

    let frontmatter = after_first[..end_pos].trim().to_string();
    let body = after_first[end_pos + 3..].trim_start().to_string();

    Ok((frontmatter, body))
}
