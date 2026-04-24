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

            // Parse skill name from first heading
            let first_line = content.lines().next().unwrap_or("").trim();
            let (skill_name, skill_id) = if first_line.starts_with("# ") {
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
            };

            // 🆕 FIX: Parse deep markdown sections
            let sections = parse_markdown_sections(&content);

            // Description: first paragraph after any heading, fallback to Description
            // section
            let description = sections.get("description").cloned().unwrap_or_else(|| {
                content
                    .lines()
                    .skip(1)
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

            // 🆕 FIX: Parse capabilities from bullet points in Capabilities section
            let capabilities = sections
                .get("capabilities")
                .map(|text| parse_capabilities(text))
                .unwrap_or_default();

            // Build tags from full content
            let tags = build_tags(&content);

            let skill = LoadedSkill {
                id: skill_id.clone(),
                name: skill_name.clone(),
                version: Version::new(1, 0, 0),
                wasm_path: PathBuf::new(), // No WASM — execution falls back to LLM
                manifest: SkillManifest {
                    id: skill_id.clone(),
                    name: skill_name.clone(),
                    version: Version::new(1, 0, 0),
                    description: description.clone(),
                    author: "BeeBotOS".to_string(),
                    capabilities: if capabilities.is_empty() {
                        tags.clone()
                    } else {
                        capabilities
                    },
                    permissions: vec!["llm:chat".to_string()],
                    entry_point: "run".to_string(),
                    license: "MIT".to_string(),
                    functions: vec![],
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
