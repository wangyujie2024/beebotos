//! Skill Prompt Builder
//!
//! Constructs the `<available_skills>` XML prompt segment for the Agent's
//! system message. Fully compatible with OpenClaw-style skill discovery.

use std::sync::Arc;

use crate::skills::registry::SkillRegistry;

/// Maximum number of skills to include in the prompt (token budget).
const MAX_SKILLS_IN_PROMPT: usize = 50;

/// Build the OpenClaw-style `<available_skills>` XML prompt.
///
/// Output format:
/// ```text
/// The following skills provide specialized instructions for specific tasks.
/// Use the read_skill tool to load a skill's file when the task matches its description.
///
/// <available_skills>
///   <skill>
///     <name>crypto-trading-bot</name>
///     <description>加密货币交易机器人开发...</description>
///     <location>skills/coding/crypto-trading-bot/SKILL.md</location>
///   </skill>
/// </available_skills>
/// ```
pub async fn build_skills_prompt(registry: &Arc<SkillRegistry>) -> String {
    let skills = registry.list_enabled().await;

    if skills.is_empty() {
        return String::new();
    }

    // Sort by usage_count descending (most-used first), then take top N
    let mut skills = skills;
    skills.sort_by(|a, b| b.usage_count.cmp(&a.usage_count));
    let skills = &skills[..skills.len().min(MAX_SKILLS_IN_PROMPT)];

    let mut xml = String::new();
    xml.push_str(
        "The following skills provide specialized instructions for specific tasks.\n");
    xml.push_str("Use the read_skill tool to load a skill's file when the task matches its description.\n\n");
    xml.push_str("<available_skills>\n");

    for skill in skills {
        let location = skill
            .skill
            .skill_md_path
            .to_string_lossy()
            .replace('\\', "/");
        xml.push_str("  <skill>\n");
        xml.push_str(&format!("    <name>{}</name>\n", skill.skill.id));
        xml.push_str(&format!(
            "    <description>{}</description>\n",
            escape_xml(&skill.skill.manifest.description)
        ));
        xml.push_str(&format!("    <location>{}</location>\n", location));
        xml.push_str("  </skill>\n");
    }

    xml.push_str("</available_skills>");

    xml
}

/// Escape XML special characters in text.
fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_xml() {
        assert_eq!(
            escape_xml("foo & bar <script>"),
            "foo &amp; bar &lt;script&gt;"
        );
    }
}
