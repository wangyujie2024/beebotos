//! Template Resolution Engine
//!
//! Resolves `{{steps.<id>.output}}`, `{{workflow.any_failed}}`,
//! `{{now}}`, `{{env.VAR_NAME}}`, and `${ENV_VAR}` placeholders
//! within workflow step parameters.

use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;

/// Context for template variable resolution
#[derive(Debug, Clone, Default)]
pub struct TemplateContext {
    /// Outputs from completed steps
    pub step_outputs: HashMap<String, Value>,
    /// Status strings from steps
    pub step_statuses: HashMap<String, String>,
    /// Whether any step has failed
    pub workflow_failed: bool,
    /// Collected error messages
    pub error_log: Vec<String>,
    /// Trigger context (e.g. webhook payload, cron time)
    pub trigger_context: Value,
    /// Workflow-level input parameters
    pub input: Value,
    /// Workflow execution duration in seconds (updated as execution progresses)
    pub duration_secs: u64,
}

impl TemplateContext {
    /// Create a new template context
    pub fn new() -> Self {
        Self::default()
    }

    /// Create context with trigger data
    pub fn with_trigger(trigger_context: Value) -> Self {
        Self {
            trigger_context,
            ..Default::default()
        }
    }

    /// Update the workflow duration in seconds
    pub fn set_duration_secs(&mut self, secs: u64) {
        self.duration_secs = secs;
    }

    /// Add a step's output to the context
    pub fn add_step_output(&mut self, step_id: &str, output: Value) {
        self.step_outputs.insert(step_id.to_string(), output);
    }

    /// Add a step's status to the context
    pub fn add_step_status(&mut self, step_id: &str, status: &str) {
        self.step_statuses.insert(step_id.to_string(), status.to_string());
    }
}

/// Template resolution error
#[derive(Debug, Clone)]
pub enum TemplateError {
    InvalidSyntax(String),
    UnknownVariable(String),
    JsonPathNotFound(String),
    EnvVarNotFound(String),
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemplateError::InvalidSyntax(s) => write!(f, "Invalid template syntax: {}", s),
            TemplateError::UnknownVariable(s) => write!(f, "Unknown template variable: {}", s),
            TemplateError::JsonPathNotFound(s) => write!(f, "JSON path not found: {}", s),
            TemplateError::EnvVarNotFound(s) => write!(f, "Environment variable not found: {}", s),
        }
    }
}

impl std::error::Error for TemplateError {}

/// Resolve all template variables in a string
pub fn resolve_template(template: &str, context: &TemplateContext) -> Result<String, TemplateError> {
    // Match {{ ... }} with optional whitespace
    let re = Regex::new(r"\{\{\s*([^}]+)\s*\}\}").unwrap();
    let mut result = template.to_string();

    for cap in re.captures_iter(template) {
        let full_match = cap.get(0).unwrap().as_str();
        let var_path = cap.get(1).unwrap().as_str().trim();

        let resolved = resolve_variable(var_path, context)?;
        result = result.replace(full_match, &resolved);
    }

    // Also resolve ${ENV_VAR}
    let env_re = Regex::new(r"\$\{([^}]+)\}").unwrap();
    let template_clone = result.clone();
    for cap in env_re.captures_iter(&template_clone) {
        let full_match = cap.get(0).unwrap().as_str();
        let var_name = cap.get(1).unwrap().as_str().trim();

        let resolved = std::env::var(var_name)
            .map_err(|_| TemplateError::EnvVarNotFound(var_name.to_string()))?;
        result = result.replace(full_match, &resolved);
    }

    Ok(result)
}

/// Recursively resolve templates inside a JSON Value
pub fn resolve_value_templates(value: &mut Value, context: &TemplateContext) -> Result<(), TemplateError> {
    match value {
        Value::String(s) => {
            let resolved = resolve_template(s, context)?;
            *s = resolved;
        }
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                resolve_value_templates(item, context)?;
            }
        }
        Value::Object(map) => {
            for (_, v) in map.iter_mut() {
                resolve_value_templates(v, context)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Resolve a single variable path like "steps.fetch_news.output"
fn resolve_variable(path: &str, context: &TemplateContext) -> Result<String, TemplateError> {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        return Err(TemplateError::InvalidSyntax(path.to_string()));
    }

    match parts[0] {
        "steps" => {
            if parts.len() < 3 {
                return Err(TemplateError::InvalidSyntax(format!(
                    "steps variable must have form steps.<id>.<field>, got: {}",
                    path
                )));
            }
            let step_id = parts[1];
            let field = parts[2];

            match field {
                "output" => {
                    let output = context
                        .step_outputs
                        .get(step_id)
                        .ok_or_else(|| TemplateError::UnknownVariable(format!("steps.{}.output", step_id)))?;

                    // If there's a deeper path like steps.x.output.summary
                    if parts.len() > 3 {
                        let json_path = parts[3..].join(".");
                        resolve_json_path(output, &json_path)
                    } else {
                        Ok(match output {
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                    }
                }
                "status" => {
                    let status = context
                        .step_statuses
                        .get(step_id)
                        .ok_or_else(|| TemplateError::UnknownVariable(format!("steps.{}.status", step_id)))?;
                    Ok(status.clone())
                }
                other => Err(TemplateError::UnknownVariable(format!(
                    "Unknown step field: {}",
                    other
                ))),
            }
        }
        "workflow" => {
            if parts.len() < 2 {
                return Err(TemplateError::InvalidSyntax(path.to_string()));
            }
            match parts[1] {
                "any_failed" => Ok(context.workflow_failed.to_string()),
                "error_log" => Ok(context.error_log.join("\n")),
                "duration" => Ok(context.duration_secs.to_string()),
                other => Err(TemplateError::UnknownVariable(format!(
                    "Unknown workflow field: {}",
                    other
                ))),
            }
        }
        "input" => {
            // Workflow input parameters
            if parts.len() > 1 {
                let json_path = parts[1..].join(".");
                resolve_json_path(&context.input, &json_path)
            } else {
                Ok(context.input.to_string())
            }
        }
        // OpenClaw compatibility: {{ now }} → current ISO 8601 timestamp
        "now" => {
            Ok(chrono::Utc::now().to_rfc3339())
        }
        // OpenClaw compatibility: {{ env.VAR_NAME }} → environment variable
        "env" => {
            if parts.len() < 2 {
                return Err(TemplateError::InvalidSyntax(
                    "env variable must have form env.VAR_NAME".to_string()
                ));
            }
            let var_name = parts[1];
            std::env::var(var_name)
                .map_err(|_| TemplateError::EnvVarNotFound(var_name.to_string()))
        }
        other => Err(TemplateError::UnknownVariable(other.to_string())),
    }
}

/// Resolve a path within a JSON value.
/// Supports:
/// - Dot notation: `foo.bar.0.baz`
/// - JSON Pointer (RFC 6901): `/foo/bar/baz` (use when keys contain dots)
fn resolve_json_path(value: &Value, path: &str) -> Result<String, TemplateError> {
    resolve_json_path_internal(value, path)
}

/// Internal JSON path resolver (pub(crate) for trigger event filtering)
pub(crate) fn resolve_json_path_internal(value: &Value, path: &str) -> Result<String, TemplateError> {
    let parts: Vec<String> = if path.starts_with('/') {
        // JSON Pointer: split by '/', skip empty leading segment
        path.split('/').skip(1).map(decode_json_pointer).collect()
    } else {
        // Dot notation
        path.split('.').map(String::from).collect()
    };
    let mut current = value;

    for part in &parts {
        // Try as object key first
        if let Some(next) = current.get(part) {
            current = next;
            continue;
        }
        // Try as array index
        if let Ok(idx) = part.parse::<usize>() {
            if let Some(next) = current.get(idx) {
                current = next;
                continue;
            }
        }
        return Err(TemplateError::JsonPathNotFound(path.to_string()));
    }

    Ok(match current {
        Value::String(s) => s.clone(),
        _ => current.to_string(),
    })
}

/// Decode a JSON Pointer segment (~1 → /, ~0 → ~)
fn decode_json_pointer(segment: &str) -> String {
    segment.replace("~1", "/").replace("~0", "~")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_step_output() {
        let mut ctx = TemplateContext::new();
        ctx.add_step_output(
            "fetch_news",
            serde_json::json!({"articles": [{"title": "Hello"}]}),
        );

        let result = resolve_template("{{steps.fetch_news.output}}", &ctx).unwrap();
        assert!(result.contains("Hello"));
    }

    #[test]
    fn test_resolve_json_path() {
        let mut ctx = TemplateContext::new();
        ctx.add_step_output(
            "fetch_news",
            serde_json::json!({"articles": [{"title": "Hello"}]}),
        );

        let result = resolve_template("{{steps.fetch_news.output.articles.0.title}}", &ctx).unwrap();
        assert_eq!(result, "Hello");
    }

    #[test]
    fn test_resolve_workflow_fields() {
        let mut ctx = TemplateContext::new();
        ctx.workflow_failed = true;
        ctx.error_log.push("Step X failed".to_string());

        let result = resolve_template("{{workflow.any_failed}}", &ctx).unwrap();
        assert_eq!(result, "true");
    }

    #[test]
    fn test_resolve_step_status() {
        let mut ctx = TemplateContext::new();
        ctx.add_step_status("fetch_news", "completed");

        let result = resolve_template("{{steps.fetch_news.status}}", &ctx).unwrap();
        assert_eq!(result, "completed");
    }

    #[test]
    fn test_resolve_value_templates() {
        let mut ctx = TemplateContext::new();
        ctx.add_step_output("fetch_news", serde_json::json!("news content"));

        let mut value = serde_json::json!({
            "message": "Result: {{steps.fetch_news.output}}",
            "nested": {
                "arr": ["{{steps.fetch_news.output}}"]
            }
        });

        resolve_value_templates(&mut value, &ctx).unwrap();
        assert_eq!(value["message"], "Result: news content");
        assert_eq!(value["nested"]["arr"][0], "news content");
    }

    #[test]
    fn test_resolve_json_pointer_with_dots() {
        let mut ctx = TemplateContext::new();
        ctx.add_step_output(
            "fetch_news",
            serde_json::json!({"foo.bar": {"baz": "qux"}}),
        );

        // JSON Pointer syntax to access key containing dot
        let result = resolve_template("{{steps.fetch_news.output./foo.bar/baz}}", &ctx).unwrap();
        assert_eq!(result, "qux");
    }

    #[test]
    fn test_decode_json_pointer() {
        assert_eq!(decode_json_pointer("foo~1bar"), "foo/bar");
        assert_eq!(decode_json_pointer("foo~0bar"), "foo~bar");
    }

    // ============================================================================
    // OpenClaw compatibility tests
    // ============================================================================

    #[test]
    fn test_resolve_now() {
        let ctx = TemplateContext::new();
        let result = resolve_template("{{now}}", &ctx).unwrap();
        // Should be a valid ISO 8601 timestamp
        assert!(result.contains("T"));
        assert!(result.contains("+"));
    }

    #[test]
    fn test_resolve_env_var() {
        // Set a test environment variable
        std::env::set_var("TEST_WORKFLOW_VAR", "test_value");
        let ctx = TemplateContext::new();
        let result = resolve_template("{{env.TEST_WORKFLOW_VAR}}", &ctx).unwrap();
        assert_eq!(result, "test_value");
    }

    #[test]
    fn test_resolve_env_var_not_found() {
        let ctx = TemplateContext::new();
        let result = resolve_template("{{env.NONEXISTENT_VAR_12345}}", &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn test_legacy_env_syntax_still_works() {
        std::env::set_var("LEGACY_VAR", "legacy_value");
        let ctx = TemplateContext::new();
        let result = resolve_template("${LEGACY_VAR}", &ctx).unwrap();
        assert_eq!(result, "legacy_value");
    }
}
