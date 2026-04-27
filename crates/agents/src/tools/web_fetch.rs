//! Tool: web_fetch
//!
//! Fetches web page content (text extraction).

use crate::llm::types::{FunctionDefinition, Tool};
use crate::llm::ToolHandler;

/// Web fetch tool
pub struct WebFetchTool;

impl WebFetchTool {
    /// Create new web fetch tool
    pub fn new() -> Self {
        Self
    }
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ToolHandler for WebFetchTool {
    fn definition(&self) -> Tool {
        Tool {
            r#type: "function".to_string(),
            function: FunctionDefinition {
                name: "web_fetch".to_string(),
                description: Some("获取网页内容".to_string()),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "网页 URL"
                        }
                    },
                    "required": ["url"]
                }),
            },
        }
    }

    async fn execute(&self, arguments: &str) -> Result<String, String> {
        let args: serde_json::Value = serde_json::from_str(arguments)
            .map_err(|e| format!("Invalid arguments: {}", e))?;

        let url = args["url"].as_str().ok_or("Missing url")?;

        let response = reqwest::get(url)
            .await
            .map_err(|e| format!("Failed to fetch URL: {}", e))?;

        let text = response
            .text()
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        // Simple HTML to text extraction
        let plain_text = html_to_text(&text);

        // Truncate if too long (max 32KB)
        const MAX_LEN: usize = 32 * 1024;
        if plain_text.len() > MAX_LEN {
            let end = plain_text[..MAX_LEN]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(MAX_LEN);
            Ok(format!(
                "{}\n\n[... {} more characters truncated ...]",
                &plain_text[..end],
                plain_text.len() - end
            ))
        } else {
            Ok(plain_text)
        }
    }
}

/// Very basic HTML tag stripping
fn html_to_text(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script = false;
    let mut prev_char = ' ';

    for ch in html.chars() {
        if in_script {
            if ch == '<' {
                // Check for </script>
                if html[result.len().saturating_sub(1)..].starts_with("</script>") {
                    in_script = false;
                }
            }
            continue;
        }

        if ch == '<' {
            in_tag = true;
            // Check for <script
            if html[result.len().saturating_sub(1)..].to_lowercase().starts_with("<script") {
                in_script = true;
            }
            continue;
        }

        if ch == '>' && in_tag {
            in_tag = false;
            continue;
        }

        if !in_tag {
            if ch.is_whitespace() {
                if prev_char != ' ' {
                    result.push(' ');
                    prev_char = ' ';
                }
            } else {
                result.push(ch);
                prev_char = ch;
            }
        }
    }

    result.trim().to_string()
}
