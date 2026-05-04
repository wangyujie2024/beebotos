//! Skill Discovery
//!
//! Scans skill directories and discovers skills in both directory-based
//! (SKILL.md) and legacy flat-file (.md) formats.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::skills::registry::Version;

/// Discovered skill metadata (lightweight — suitable for lazy loading)
#[derive(Debug, Clone)]
pub struct SkillMetadata {
    pub id: String,
    pub name: String,
    pub version: Version,
    pub description: String,
    pub category: String,
    pub tags: Vec<String>,
    /// Absolute path to the skill directory or .md file
    pub path: PathBuf,
    pub kind: SkillKind,
}

/// Classification of skill implementation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillKind {
    /// Pure documentation skill (SKILL.md only)
    Knowledge,
    /// Documentation + executable scripts (.py/.js/.sh)
    Code,
    /// WASM binary skill
    Wasm,
}

/// Skill directory scanner
pub struct SkillDiscovery {
    paths: Vec<PathBuf>,
}

impl SkillDiscovery {
    pub fn new() -> Self {
        Self { paths: vec![] }
    }

    pub fn add_path(&mut self, path: impl AsRef<Path>) {
        self.paths.push(path.as_ref().to_path_buf());
    }

    /// Scan all configured paths and return discovered skill metadata.
    ///
    /// Priority: paths earlier in the list take precedence.
    /// Duplicate skill IDs from later paths are skipped.
    pub async fn scan(&self) -> Vec<SkillMetadata> {
        let mut seen = HashMap::new();
        let mut results = Vec::new();

        for base in &self.paths {
            let base_meta = match tokio::fs::metadata(base).await {
                Ok(m) if m.is_dir() => m,
                _ => continue,
            };
            let _ = base_meta;

            let entries = match tokio::fs::read_dir(base).await {
                Ok(e) => e,
                Err(_) => continue,
            };

            let mut entries_vec = Vec::new();
            let mut entries_stream = entries;
            while let Ok(Some(entry)) = entries_stream.next_entry().await {
                entries_vec.push(entry.path());
            }

            for path in entries_vec {
                let is_dir = match tokio::fs::metadata(&path).await {
                    Ok(m) => m.is_dir(),
                    Err(_) => continue,
                };

                if is_dir {
                    // 1. Try directory-based skill (SKILL.md)
                    if let Some(meta) = Self::inspect_directory(&path).await {
                        if !seen.contains_key(&meta.id) {
                            seen.insert(meta.id.clone(), meta.path.clone());
                            results.push(meta);
                        }
                    } else {
                        // 2. Scan contents of this subdirectory
                        let mut entries = match tokio::fs::read_dir(&path).await {
                            Ok(e) => e,
                            Err(_) => continue,
                        };
                        while let Ok(Some(entry)) = entries.next_entry().await {
                            let child = entry.path();
                            let child_is_dir = match tokio::fs::metadata(&child).await {
                                Ok(m) => m.is_dir(),
                                Err(_) => continue,
                            };
                            if child_is_dir {
                                // Nested directory skill (e.g. skills/daily/hello_world/)
                                if let Some(meta) = Self::inspect_directory(&child).await {
                                    if !seen.contains_key(&meta.id) {
                                        seen.insert(meta.id.clone(), meta.path.clone());
                                        results.push(meta);
                                    }
                                }
                            } else if is_md_file(&child).await {
                                // Legacy flat .md file
                                if let Some(meta) = Self::inspect_flat_md(&child).await {
                                    if !seen.contains_key(&meta.id) {
                                        seen.insert(meta.id.clone(), meta.path.clone());
                                        results.push(meta);
                                    }
                                }
                            }
                        }
                    }
                } else if is_md_file(&path).await {
                    if let Some(meta) = Self::inspect_flat_md(&path).await {
                        if !seen.contains_key(&meta.id) {
                            seen.insert(meta.id.clone(), meta.path.clone());
                            results.push(meta);
                        }
                    }
                }
            }
        }

        results
    }

    /// Inspect a directory that may contain a SKILL.md or skill.yaml
    async fn inspect_directory(path: &Path) -> Option<SkillMetadata> {
        let skill_md = path.join("SKILL.md");
        let skill_yaml = path.join("skill.yaml");
        let skill_wasm = path.join("skill.wasm");

        if tokio::fs::metadata(&skill_md).await.map(|m| m.is_file()).unwrap_or(false) {
            let content = tokio::fs::read_to_string(&skill_md).await.ok()?;
            let (front_matter, body) = parse_front_matter(&content);

            let id = front_matter
                .get("name")
                .cloned()
                .unwrap_or_else(|| path.file_name().unwrap().to_string_lossy().to_string());
            let name = front_matter.get("name").cloned().unwrap_or_else(|| id.clone());
            let description = front_matter
                .get("description")
                .cloned()
                .unwrap_or_else(|| extract_first_paragraph(&body));
            let category = front_matter.get("category").cloned().unwrap_or_default();
            let tags = parse_tags(front_matter.get("tags"));
            let version = parse_version(front_matter.get("version"));

            let kind = if tokio::fs::metadata(&skill_wasm).await.map(|m| m.is_file()).unwrap_or(false) {
                SkillKind::Wasm
            } else if has_executable_scripts(path).await {
                SkillKind::Code
            } else {
                SkillKind::Knowledge
            };

            return Some(SkillMetadata {
                id: sanitize_id(&id),
                name,
                version,
                description,
                category,
                tags,
                path: path.to_path_buf(),
                kind,
            });
        }

        if tokio::fs::metadata(&skill_yaml).await.map(|m| m.is_file()).unwrap_or(false)
            && tokio::fs::metadata(&skill_wasm).await.map(|m| m.is_file()).unwrap_or(false)
        {
            // WASM skill without SKILL.md — fall back to legacy loader
            let content = tokio::fs::read_to_string(&skill_yaml).await.ok()?;
            let manifest: crate::skills::loader::SkillManifest =
                serde_yaml::from_str(&content).ok()?;
            return Some(SkillMetadata {
                id: sanitize_id(&manifest.id),
                name: manifest.name.clone(),
                version: manifest.version.clone(),
                description: manifest.description.clone(),
                category: String::new(),
                tags: vec![],
                path: path.to_path_buf(),
                kind: SkillKind::Wasm,
            });
        }

        None
    }

    /// Inspect a legacy flat .md file
    async fn inspect_flat_md(path: &Path) -> Option<SkillMetadata> {
        let content = tokio::fs::read_to_string(path).await.ok()?;
        let first_line = content.lines().next()?.trim();

        let name = if first_line.starts_with("# ") {
            first_line[2..].trim().to_string()
        } else {
            path.file_stem()?.to_string_lossy().to_string()
        };

        let id = sanitize_id(&name);
        let description = extract_first_paragraph(&content);
        let parent = path.parent()?;
        let category = parent
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        Some(SkillMetadata {
            id,
            name,
            version: Version::new(1, 0, 0),
            description,
            category,
            tags: build_tags_from_content(&content),
            path: path.to_path_buf(),
            kind: SkillKind::Knowledge,
        })
    }
}

impl Default for SkillDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

/// Check whether a path has the `.md` extension
async fn is_md_file(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("md")
}

/// Check whether a directory contains executable scripts
async fn has_executable_scripts(path: &Path) -> bool {
    let mut entries = match tokio::fs::read_dir(path).await {
        Ok(e) => e,
        Err(_) => return false,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let p = entry.path();
        if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
            if matches!(ext, "py" | "js" | "sh" | "ts") {
                return true;
            }
        }
    }
    false
}

/// Parse YAML front matter (--- ... ---) from markdown content.
/// Uses a proper YAML parser for robustness.
fn parse_front_matter(content: &str) -> (HashMap<String, String>, String) {
    let mut map = HashMap::new();
    let trimmed = content.trim_start();

    if !trimmed.starts_with("---") {
        return (map, content.to_string());
    }

    let after_first = &trimmed[3..];
    if let Some(end_idx) = after_first.find("---") {
        let yaml_part = after_first[..end_idx].trim();
        let body = after_first[end_idx + 3..].trim_start().to_string();

        // Use serde_yaml for proper parsing (supports quoted strings, lists, nested objects)
        if let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(yaml_part) {
            if let serde_yaml::Value::Mapping(m) = value {
                for (k, v) in m {
                    if let (Some(key), Some(val_str)) = (
                        k.as_str(),
                        match v {
                            serde_yaml::Value::String(s) => Some(s),
                            serde_yaml::Value::Number(n) => Some(n.to_string()),
                            serde_yaml::Value::Bool(b) => Some(b.to_string()),
                            _ => None,
                        },
                    ) {
                        map.insert(key.to_string(), val_str);
                    }
                }
                return (map, body);
            }
        }

        // Fallback to simple line parser if serde_yaml fails or returns non-mapping
        for line in yaml_part.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once(':') {
                map.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
        return (map, body);
    }

    (map, content.to_string())
}

/// Extract the first non-empty, non-heading paragraph
fn extract_first_paragraph(content: &str) -> String {
    content
        .lines()
        .skip_while(|l| l.trim().is_empty() || l.trim().starts_with('#') || l.trim() == "---")
        .take_while(|l| !l.trim().is_empty() && !l.trim().starts_with("##"))
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn parse_version(v: Option<&String>) -> Version {
    v.and_then(|s| {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() >= 3 {
            let major = parts[0].parse().ok()?;
            let minor = parts[1].parse().ok()?;
            let patch = parts[2].parse().ok()?;
            Some(Version::new(major, minor, patch))
        } else {
            None
        }
    })
    .unwrap_or_else(|| Version::new(1, 0, 0))
}

fn parse_tags(v: Option<&String>) -> Vec<String> {
    v.map(|s| {
        s.trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .map(|t| t.trim().trim_matches('"').to_string())
            .filter(|t| !t.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

fn sanitize_id(name: &str) -> String {
    name.to_lowercase()
        .replace(' ', "_")
        .replace('-', "_")
        .replace(|c: char| !c.is_alphanumeric() && c != '_', "")
}

fn build_tags_from_content(content: &str) -> Vec<String> {
    let mut tags = vec![];
    let lower = content.to_lowercase();
    if lower.contains("code") || lower.contains("python") || lower.contains("rust") {
        tags.push("coding".to_string());
    }
    if lower.contains("data") || lower.contains("analysis") {
        tags.push("data".to_string());
    }
    if lower.contains("write") || lower.contains("email") {
        tags.push("writing".to_string());
    }
    tags
}
