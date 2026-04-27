//! Tool: search_files
//!
//! Searches file contents within a directory (grep-like).

use crate::llm::types::{FunctionDefinition, Tool};
use crate::llm::ToolHandler;

/// Search files tool
pub struct SearchFilesTool;

impl SearchFilesTool {
    /// Create new search files tool
    pub fn new() -> Self {
        Self
    }
}

impl Default for SearchFilesTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ToolHandler for SearchFilesTool {
    fn definition(&self) -> Tool {
        Tool {
            r#type: "function".to_string(),
            function: FunctionDefinition {
                name: "search_files".to_string(),
                description: Some("在指定目录下搜索文件内容".to_string()),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "搜索关键词"
                        },
                        "path": {
                            "type": "string",
                            "description": "搜索目录（可选，默认当前目录）"
                        },
                        "file_pattern": {
                            "type": "string",
                            "description": "文件匹配模式（可选，如 *.rs）"
                        }
                    },
                    "required": ["query"]
                }),
            },
        }
    }

    async fn execute(&self, arguments: &str) -> Result<String, String> {
        let args: serde_json::Value = serde_json::from_str(arguments)
            .map_err(|e| format!("Invalid arguments: {}", e))?;

        let query = args["query"].as_str().ok_or("Missing query")?;
        let path = args["path"].as_str().unwrap_or(".");
        let file_pattern = args["file_pattern"].as_str();

        let mut results = Vec::new();
        let mut entries = tokio::fs::read_dir(path)
            .await
            .map_err(|e| format!("Failed to read directory: {}", e))?;

        while let Ok(Some(entry)) = entries.next_entry().await {
            let file_path = entry.path();
            if !file_path.is_file() {
                continue;
            }

            // Check file pattern
            if let Some(pattern) = file_pattern {
                let name = file_path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                if !glob_match(name, pattern) {
                    continue;
                }
            }

            let content = match tokio::fs::read_to_string(&file_path).await {
                Ok(c) => c,
                Err(_) => continue, // Skip binary files
            };

            for (line_num, line) in content.lines().enumerate() {
                if line.contains(query) {
                    let file_name = file_path.display();
                    results.push(format!(
                        "{}:{}: {}",
                        file_name,
                        line_num + 1,
                        line.trim()
                    ));
                    if results.len() >= 50 {
                        break;
                    }
                }
            }
            if results.len() >= 50 {
                break;
            }
        }

        if results.is_empty() {
            Ok("No matches found.".to_string())
        } else {
            Ok(results.join("\n"))
        }
    }
}

/// Simple glob matching (supports * and ?)
fn glob_match(name: &str, pattern: &str) -> bool {
    let mut name_chars = name.chars().peekable();
    let mut pattern_chars = pattern.chars().peekable();

    while let Some(p) = pattern_chars.next() {
        match p {
            '*' => {
                let next_p = pattern_chars.peek().copied();
                if next_p.is_none() {
                    return true;
                }
                while let Some(n) = name_chars.next() {
                    if Some(n) == next_p {
                        break;
                    }
                }
            }
            '?' => {
                if name_chars.next().is_none() {
                    return false;
                }
            }
            c => {
                if name_chars.next() != Some(c) {
                    return false;
                }
            }
        }
    }

    name_chars.next().is_none()
}
