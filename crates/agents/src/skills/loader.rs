//! Skill Loader
//!
//! Loads and manages WASM-based skills from ClawHub.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::skills::registry::Version;

/// Skill loader
pub struct SkillLoader {
    skill_paths: Vec<PathBuf>,
    loaded_skills: HashMap<String, LoadedSkill>,
}

/// Loaded skill info
#[derive(Debug, Clone)]
pub struct LoadedSkill {
    pub id: String,
    pub name: String,
    pub version: Version,
    /// Path to the skill's SKILL.md file (replaces wasm_path)
    pub skill_md_path: PathBuf,
    pub manifest: SkillManifest,
}

/// Skill manifest (Markdown skill format)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkillManifest {
    pub id: String,
    pub name: String,
    pub version: Version,
    pub description: String,
    pub author: String,
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub license: String,
    /// Prompt template extracted from SKILL.md markdown body
    #[serde(default)]
    pub prompt_template: String,
    /// Few-shot examples extracted from SKILL.md markdown body
    #[serde(default)]
    pub examples: String,
}

impl SkillLoader {
    pub fn new() -> Self {
        Self {
            skill_paths: vec![],
            loaded_skills: HashMap::new(),
        }
    }

    /// Add skill search path
    pub fn add_path(&mut self, path: impl AsRef<Path>) {
        self.skill_paths.push(path.as_ref().to_path_buf());
    }

    /// Load skill from path (Markdown-based)
    pub async fn load_skill(&mut self, skill_id: &str) -> Result<LoadedSkill, SkillLoadError> {
        // Check if already loaded
        if let Some(skill) = self.loaded_skills.get(skill_id) {
            return Ok(skill.clone());
        }

        // Search for skill in paths
        for path in &self.skill_paths {
            let skill_dir = path.join(skill_id);
            if !skill_dir.exists() || !skill_dir.is_dir() {
                continue;
            }

            let skill_md_path = skill_dir.join("SKILL.md");
            if !skill_md_path.exists() {
                return Err(SkillLoadError::InvalidManifest(format!(
                    "SKILL.md not found at {:?}",
                    skill_md_path
                )));
            }

            let manifest = self.load_manifest(&skill_md_path).await?;

            let skill = LoadedSkill {
                id: skill_id.to_string(),
                name: manifest.name.clone(),
                version: manifest.version.clone(),
                skill_md_path,
                manifest,
            };

            self.loaded_skills
                .insert(skill_id.to_string(), skill.clone());
            return Ok(skill);
        }

        Err(SkillLoadError::SkillNotFound(skill_id.to_string()))
    }

    /// Load manifest from SKILL.md (YAML frontmatter + markdown body)
    async fn load_manifest(&self, skill_md_path: &Path) -> Result<SkillManifest, SkillLoadError> {
        let content = tokio::fs::read_to_string(skill_md_path)
            .await
            .map_err(|e| SkillLoadError::IoError(e.to_string()))?;

        // Parse YAML frontmatter between --- markers
        let (frontmatter, markdown_body) = parse_frontmatter(&content)
            .map_err(|e| SkillLoadError::ParseError(e))?;

        // Try to parse manifest from frontmatter
        let mut manifest: SkillManifest = serde_yaml::from_str(&frontmatter)
            .map_err(|e| SkillLoadError::ParseError(e.to_string()))?;

        // Extract prompt template and examples from markdown body
        let sections = parse_markdown_sections(&markdown_body);
        manifest.prompt_template = sections.get("prompt_template").cloned()
            .or_else(|| sections.get("core_function").cloned())
            .unwrap_or_default();
        manifest.examples = sections.get("examples").cloned().unwrap_or_default();

        // Ensure ID is set
        if manifest.id.is_empty() {
            manifest.id = skill_md_path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
        }

        Ok(manifest)
    }

    /// Get loaded skill
    pub fn get_skill(&self, skill_id: &str) -> Option<&LoadedSkill> {
        self.loaded_skills.get(skill_id)
    }

    /// List loaded skills
    pub fn list_skills(&self) -> Vec<&LoadedSkill> {
        self.loaded_skills.values().collect()
    }

    /// Unload skill
    pub fn unload_skill(&mut self, skill_id: &str) {
        self.loaded_skills.remove(skill_id);
    }
}

impl Default for SkillLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse YAML frontmatter from markdown content.
/// Returns (frontmatter_yaml, markdown_body).
fn parse_frontmatter(content: &str) -> Result<(String, String), String> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        // No frontmatter — return empty frontmatter and full content
        return Ok((String::new(), content.to_string()));
    }

    let after_first = &trimmed[3..];
    let Some(end_pos) = after_first.find("---") else {
        return Err("Unclosed frontmatter: missing closing ---".to_string());
    };

    let frontmatter = after_first[..end_pos].trim().to_string();
    let body = after_first[end_pos + 3..].trim_start().to_string();

    Ok((frontmatter, body))
}

/// Parse a markdown body into structured sections.
/// Returns a map: section_name -> content (without the ## heading line)
fn parse_markdown_sections(content: &str) -> HashMap<String, String> {
    let mut sections = HashMap::new();
    let mut current_section: Option<String> = None;
    let mut current_lines: Vec<String> = Vec::new();

    for line in content.lines() {
        if line.starts_with("## ") {
            // Save previous section
            if let Some(ref name) = current_section {
                let body = current_lines.join("\n").trim().to_string();
                if !body.is_empty() {
                    sections.insert(name.clone(), body);
                }
            }
            // Start new section
            current_section = Some(line[3..].trim().to_lowercase().replace(' ', "_"));
            current_lines.clear();
        } else if current_section.is_some() {
            current_lines.push(line.to_string());
        }
    }

    // Save final section
    if let Some(ref name) = current_section {
        let body = current_lines.join("\n").trim().to_string();
        if !body.is_empty() {
            sections.insert(name.clone(), body);
        }
    }

    sections
}

/// Skill load errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum SkillLoadError {
    #[error("Skill not found: {0}")]
    SkillNotFound(String),
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Invalid manifest: {0}")]
    InvalidManifest(String),
}
