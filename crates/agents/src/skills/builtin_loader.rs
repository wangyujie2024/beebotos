//! Builtin Skill Loader
//!
//! Scans the project `skills/` directory and registers markdown-defined skills
//! as lightweight builtins. These skills have no WASM binary; execution falls
//! back to LLM with the skill description as a system prompt.
//!
//! 🆕 FIX: Now parses deep markdown sections (Prompt Template, Examples,
//! Capabilities) so high-quality skills actually deliver their full value.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::skills::loader::{LoadedSkill, SkillManifest};
use crate::skills::registry::{SkillRegistry, Version};

/// Parse a markdown file into structured sections.
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

/// Extract YAML frontmatter from markdown content.
/// Returns (frontmatter_map, markdown_body_without_frontmatter).
fn extract_frontmatter(content: &str) -> (Option<HashMap<String, String>>, String) {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return (None, content.to_string());
    }

    let lines: Vec<&str> = trimmed.lines().collect();
    if lines.len() < 3 {
        return (None, content.to_string());
    }

    // Find closing ---
    let mut end_idx = 0;
    for (i, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            end_idx = i;
            break;
        }
    }

    if end_idx == 0 {
        return (None, content.to_string());
    }

    let mut frontmatter = HashMap::new();
    let yaml_lines = &lines[1..end_idx];
    let mut i = 0;

    while i < yaml_lines.len() {
        let line = yaml_lines[i];
        if line.trim().is_empty() || line.trim().starts_with('#') {
            i += 1;
            continue;
        }

        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_string();
            let value = value.trim().to_string();

            // Collect multi-line indented values
            let mut full_value = value;
            let mut j = i + 1;
            while j < yaml_lines.len() {
                let next = yaml_lines[j];
                if !next.starts_with(' ') && !next.starts_with('\t') && !next.trim().is_empty() {
                    break;
                }
                if !next.trim().is_empty() {
                    if !full_value.is_empty() {
                        full_value.push('\n');
                    }
                    full_value.push_str(next);
                }
                j += 1;
            }
            i = j;

            if !key.is_empty() {
                frontmatter.insert(key, full_value);
            }
        } else {
            i += 1;
        }
    }

    let body = lines[end_idx + 1..].join("\n");
    (Some(frontmatter), body)
}

/// Extract bullet-point capabilities from a capabilities text block.
fn parse_capabilities(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                Some(trimmed[2..].trim().to_string())
            } else if trimmed.starts_with("• ") {
                Some(trimmed[2..].trim().to_string())
            } else {
                None
            }
        })
        .collect()
}

/// Build tags from content keywords.
fn build_tags(content: &str) -> Vec<String> {
    let mut tags = vec![];
    let lower = content.to_lowercase();
    if lower.contains("search") || lower.contains("搜索") {
        tags.push("search".to_string());
    }
    if lower.contains("analysis") || lower.contains("分析") {
        tags.push("analysis".to_string());
    }
    if lower.contains("plan") || lower.contains("计划") || lower.contains("规划") {
        tags.push("planning".to_string());
    }
    if lower.contains("code") || lower.contains("代码") || lower.contains("开发") {
        tags.push("coding".to_string());
    }
    if lower.contains("write") || lower.contains("写") || lower.contains("邮件") {
        tags.push("writing".to_string());
    }
    if lower.contains("data") || lower.contains("数据") {
        tags.push("data".to_string());
    }
    if lower.contains("travel")
        || lower.contains("tour")
        || lower.contains("旅游")
        || lower.contains("旅行")
    {
        tags.push("travel".to_string());
    }
    if lower.contains("translate") || lower.contains("翻译") || lower.contains("语言") {
        tags.push("translation".to_string());
    }
    if lower.contains("cook")
        || lower.contains("recipe")
        || lower.contains("食谱")
        || lower.contains("做菜")
    {
        tags.push("cooking".to_string());
    }
    if lower.contains("fitness")
        || lower.contains("workout")
        || lower.contains("健身")
        || lower.contains("运动")
    {
        tags.push("fitness".to_string());
    }
    if lower.contains("movie")
        || lower.contains("film")
        || lower.contains("电影")
        || lower.contains("剧集")
    {
        tags.push("entertainment".to_string());
    }
    if lower.contains("news") || lower.contains("新闻") || lower.contains("资讯") {
        tags.push("news".to_string());
    }
    if lower.contains("weather") || lower.contains("天气") || lower.contains("气温") {
        tags.push("weather".to_string());
    }
    if lower.contains("calculat")
        || lower.contains("math")
        || lower.contains("计算")
        || lower.contains("房贷")
        || lower.contains("投资")
    {
        tags.push("calculator".to_string());
    }
    tags
}

/// Scan `skills/` directory and register all `.md` skills into the given
/// registry.
pub async fn load_builtin_skills(registry: &Arc<SkillRegistry>) {
    let skills_dir = PathBuf::from("skills");
    if !skills_dir.exists() || !skills_dir.is_dir() {
        return;
    }

    let mut registered = 0;
    let mut entries = match tokio::fs::read_dir(&skills_dir).await {
        Ok(e) => e,
        Err(_) => return,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let category_dir = entry.path();
        if !category_dir.is_dir() {
            continue;
        }

        let mut md_entries = match tokio::fs::read_dir(&category_dir).await {
            Ok(e) => e,
            Err(_) => continue,
        };

        while let Ok(Some(md_entry)) = md_entries.next_entry().await {
            let path = md_entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }

            let content = match tokio::fs::read_to_string(&path).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            // 🆕 FIX: Parse YAML frontmatter if present
            let (frontmatter, body_content) = extract_frontmatter(&content);

            // Parse skill name from frontmatter, first heading, or file stem
            let (skill_name, skill_id) = if let Some(ref fm) = &frontmatter {
                if let Some(name) = fm.get("name") {
                    let id = name
                        .to_lowercase()
                        .replace(' ', "_")
                        .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
                    (name.clone(), id)
                } else {
                    let first_heading = body_content
                        .lines()
                        .find(|l| l.trim().starts_with("# "))
                        .unwrap_or("");
                    if !first_heading.is_empty() {
                        let name = first_heading[2..].trim().to_string();
                        let id = name
                            .to_lowercase()
                            .replace(' ', "_")
                            .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
                        (name, id)
                    } else {
                        let stem = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("unknown")
                            .to_string();
                        (stem.clone(), stem.to_lowercase().replace(' ', "_"))
                    }
                }
            } else {
                let first_line = content.lines().next().unwrap_or("").trim();
                if first_line.starts_with("# ") {
                    let name = first_line[2..].trim().to_string();
                    let id = name
                        .to_lowercase()
                        .replace(' ', "_")
                        .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
                    (name, id)
                } else {
                    let stem = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    (stem.clone(), stem.to_lowercase().replace(' ', "_"))
                }
            };

            // 🆕 FIX: Parse deep markdown sections from body (without frontmatter)
            let sections = parse_markdown_sections(&body_content);

            // Description: frontmatter first, then Description section, then first paragraph
            let description = frontmatter
                .as_ref()
                .and_then(|fm| fm.get("description").cloned())
                .or_else(|| sections.get("description").cloned())
                .unwrap_or_else(|| {
                    body_content
                        .lines()
                        .skip_while(|l| l.trim().is_empty() || l.trim().starts_with('#'))
                        .take_while(|l| !l.trim().starts_with('#') && !l.trim().starts_with("```"))
                        .collect::<Vec<_>>()
                        .join(" ")
                        .trim()
                        .to_string()
                });

            let description = if description.is_empty() {
                format!("Built-in skill: {}", skill_name)
            } else {
                description
            };

            // 🆕 FIX: Extract prompt template and examples from markdown sections
            let prompt_template = sections.get("prompt_template").cloned().unwrap_or_default();
            let examples = sections.get("examples").cloned().unwrap_or_default();

            // 🆕 FIX: Parse capabilities from frontmatter or markdown section
            let fm_capabilities = frontmatter
                .as_ref()
                .and_then(|fm| fm.get("capabilities"))
                .map(|text| parse_capabilities(text))
                .unwrap_or_default();
            let capabilities = if fm_capabilities.is_empty() {
                sections
                    .get("capabilities")
                    .map(|text| parse_capabilities(text))
                    .unwrap_or_default()
            } else {
                fm_capabilities
            };

            // Build tags from full content
            let tags = build_tags(&content);

            // 🆕 FIX: Parse version, author, license from frontmatter
            let version = frontmatter
                .as_ref()
                .and_then(|fm| fm.get("version"))
                .and_then(|v| {
                    let parts: Vec<&str> = v.split('.').collect();
                    if !parts.is_empty() {
                        let major = parts[0].parse().unwrap_or(1);
                        let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
                        let patch = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
                        Some(Version::new(major, minor, patch))
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| Version::new(1, 0, 0));

            let author = frontmatter
                .as_ref()
                .and_then(|fm| fm.get("author").cloned())
                .unwrap_or_else(|| "BeeBotOS".to_string());

            let license = frontmatter
                .as_ref()
                .and_then(|fm| fm.get("license").cloned())
                .unwrap_or_else(|| "MIT".to_string());

            let skill = LoadedSkill {
                id: skill_id.clone(),
                name: skill_name.clone(),
                version: version.clone(),
                skill_md_path: path.clone(),
                manifest: SkillManifest {
                    id: skill_id.clone(),
                    name: skill_name.clone(),
                    version,
                    description: description.clone(),
                    author,
                    capabilities: if capabilities.is_empty() {
                        tags.clone()
                    } else {
                        capabilities
                    },
                    license,
                    prompt_template,
                    examples,
                },
            };

            let category = category_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("general");
            registry.register(skill, category, tags).await;
            registered += 1;
        }
    }

    if registered > 0 {
        tracing::info!(
            "✅ Registered {} built-in skills from skills/ directory",
            registered
        );
    }
}
