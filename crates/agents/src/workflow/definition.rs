//! Workflow Definition
//!
//! YAML/JSON-parsable structures for declaring workflows.
//! Compatible with both BeeBotOS native format and OpenClaw format.

use serde::{Deserialize, Serialize};

/// Workflow definition parsed from YAML or JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    /// Unique workflow identifier (auto-populated from filename if empty)
    #[serde(default)]
    pub id: String,
    /// Human-readable name
    #[serde(default)]
    pub name: String,
    /// Description of what this workflow does
    pub description: String,
    /// Version string
    #[serde(default = "default_version")]
    pub version: String,
    /// Author information
    pub author: Option<String>,
    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
    /// Triggers that can start this workflow
    #[serde(default)]
    pub triggers: Vec<TriggerDefinition>,
    /// Global workflow configuration
    #[serde(default)]
    pub config: WorkflowGlobalConfig,
    /// Execution steps
    pub steps: Vec<WorkflowStep>,
    /// Global error handler (OpenClaw compatible)
    #[serde(default, rename = "error_handler")]
    pub error_handler: Option<ErrorHandler>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

/// Global configuration for a workflow
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkflowGlobalConfig {
    /// Overall workflow timeout in seconds
    pub timeout_sec: Option<u64>,
    /// Default max retries for steps
    pub max_retries: Option<u32>,
    /// Continue executing remaining steps if one fails
    #[serde(default)]
    pub continue_on_failure: bool,
    /// Notify when workflow completes
    #[serde(default)]
    pub notify_on_complete: bool,
}

/// Individual workflow step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    /// Step identifier (unique within workflow)
    pub id: String,
    /// Human-readable name (OpenClaw: optional, defaults to id)
    #[serde(default)]
    pub name: String,
    /// Skill ID to invoke (from SkillRegistry)
    pub skill: String,
    /// Parameters passed to the skill (supports template vars)
    /// OpenClaw compatibility: also accepts `input`
    #[serde(default, alias = "input")]
    pub params: serde_json::Value,
    /// IDs of steps that must complete before this one runs
    /// OpenClaw compatibility: also accepts `dependencies`
    #[serde(default, rename = "depends_on", alias = "dependencies")]
    pub depends_on: Option<Vec<String>>,
    /// Condition expression; step only runs if this evaluates to true
    pub condition: Option<String>,
    /// Step-specific timeout override
    pub timeout_sec: Option<u64>,
    /// Step-specific retry override
    pub retries: Option<u32>,
    /// Step-level error handler (OpenClaw compatible)
    #[serde(default, rename = "on_error")]
    pub on_error: Option<StepErrorHandler>,
}

/// Trigger definition for starting a workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerDefinition {
    #[serde(flatten)]
    pub trigger_type: TriggerType,
}

/// Types of workflow triggers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerType {
    /// Cron schedule trigger
    Cron {
        /// Cron expression (e.g. "0 9 * * *")
        schedule: String,
        /// Timezone (default UTC)
        timezone: Option<String>,
    },
    /// System event trigger
    /// OpenClaw compatibility: also accepts `channel` and `event_type`
    Event {
        /// Event source identifier (OpenClaw: `channel`)
        #[serde(alias = "channel")]
        source: String,
        /// Optional filter expression (OpenClaw: `event_type`)
        #[serde(alias = "event_type")]
        filter: Option<String>,
    },
    /// Webhook HTTP trigger
    Webhook {
        /// URL path (e.g. "/webhook/daily_news")
        path: String,
        /// HTTP method
        #[serde(default = "default_method")]
        method: String,
        /// Authentication type
        auth: Option<String>,
    },
    /// Manual trigger (via API or chat command)
    /// OpenClaw compatibility: supports `allowed_users`
    Manual {
        #[serde(default)]
        allowed_users: Vec<String>,
    },
}

fn default_method() -> String {
    "POST".to_string()
}

// ============================================================================
// OpenClaw-compatible error handling structures
// ============================================================================

/// Global error handler for a workflow (OpenClaw compatible)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorHandler {
    /// Which step's errors to capture ("any" for all steps)
    pub step: String,
    /// Action to take: "fail" | "retry" | "skip"
    pub action: String,
    /// Optional fallback skill to invoke
    pub fallback: Option<FallbackAction>,
}

/// Step-level error handler (OpenClaw compatible)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepErrorHandler {
    /// Action to take: "retry" | "skip" | "fail"
    pub action: String,
    /// Maximum retry attempts (for "retry" action)
    pub max_retries: Option<u32>,
    /// Delay between retries in seconds
    pub delay_seconds: Option<u64>,
    /// Optional fallback skill to invoke when all retries exhausted
    pub fallback: Option<FallbackAction>,
}

/// Fallback action to invoke on error (OpenClaw compatible)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackAction {
    /// Skill ID to invoke
    pub skill: String,
    /// Parameters passed to the fallback skill (OpenClaw: `input`)
    #[serde(default, alias = "input")]
    pub params: serde_json::Value,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_workflow_yaml() {
        let yaml = r#"
name: "daily_tech_news"
description: "Daily tech news briefing"
version: "1.0.0"
tags: ["daily", "news"]
triggers:
  - type: cron
    schedule: "0 9 * * *"
    timezone: "Asia/Shanghai"
  - type: manual
config:
  timeout_sec: 300
  continue_on_failure: false
steps:
  - id: fetch_news
    name: "Fetch News"
    skill: rss_reader
    params:
      url: "https://example.com/rss"
      limit: 10
    timeout_sec: 30
    retries: 2
  - id: summarize
    name: "Summarize"
    skill: llm_summarizer
    depends_on: ["fetch_news"]
    params:
      input: "{{steps.fetch_news.output}}"
"#;
        let def: WorkflowDefinition = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(def.name, "daily_tech_news");
        assert_eq!(def.steps.len(), 2);
        assert_eq!(def.steps[1].depends_on.as_ref().unwrap()[0], "fetch_news");
        assert!(matches!(
            &def.triggers[0].trigger_type,
            TriggerType::Cron { schedule, .. } if schedule == "0 9 * * *"
        ));
    }

    #[test]
    fn test_trigger_webhook() {
        let yaml = r#"
type: webhook
path: "/webhook/test"
method: "POST"
auth: "bearer"
"#;
        let trigger: TriggerType = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(
            trigger,
            TriggerType::Webhook { path, .. } if path == "/webhook/test"
        ));
    }

    #[test]
    fn test_load_example_workflows() {
        let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..");
        for path in &[
            "data/workflows/daily_news.yaml",
            "examples/workflows/data_processing.yaml",
            "examples/workflows/trading_bot.yaml",
            "examples/workflows/content_factory.yaml",
            "examples/workflows/manga_pipeline.yaml",
        ] {
            let full_path = workspace.join(path);
            let content = std::fs::read_to_string(&full_path)
                .unwrap_or_else(|e| panic!("Failed to read {}: {}", full_path.display(), e));
            let def: WorkflowDefinition = serde_yaml::from_str(&content)
                .unwrap_or_else(|e| panic!("Failed to parse {}: {}", full_path.display(), e));
            assert!(!def.id.is_empty(), "Workflow {} has empty ID", full_path.display());
            assert!(!def.steps.is_empty(), "Workflow {} has no steps", full_path.display());
        }
    }

    // ============================================================================
    // OpenClaw compatibility tests
    // ============================================================================

    #[test]
    fn test_openclaw_format_id_from_name() {
        // OpenClaw uses `name` as the workflow identifier
        let yaml = r#"
name: "openclaw-workflow"
description: "OpenClaw style workflow"
version: "1.0.0"
triggers:
  - type: manual
steps:
  - id: step1
    skill: test_skill
"#;
        let def: WorkflowDefinition = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(def.id, "openclaw-workflow");
        assert_eq!(def.name, "openclaw-workflow");
    }

    #[test]
    fn test_openclaw_input_alias() {
        // OpenClaw uses `input` instead of `params`
        let yaml = r#"
id: test
description: test
steps:
  - id: step1
    skill: test_skill
    input:
      url: "https://example.com"
"#;
        let def: WorkflowDefinition = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(def.steps[0].params["url"], "https://example.com");
    }

    #[test]
    fn test_openclaw_dependencies_alias() {
        // OpenClaw uses `dependencies` instead of `depends_on`
        let yaml = r#"
id: test
description: test
steps:
  - id: step1
    skill: a
  - id: step2
    skill: b
    dependencies: ["step1"]
"#;
        let def: WorkflowDefinition = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(def.steps[1].depends_on.as_ref().unwrap()[0], "step1");
    }

    #[test]
    fn test_openclaw_event_trigger_alias() {
        // OpenClaw uses `channel` and `event_type`
        let yaml = r#"
id: test
description: test
triggers:
  - type: event
    channel: "feishu"
    event_type: "message"
steps:
  - id: s1
    skill: test
"#;
        let def: WorkflowDefinition = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(
            &def.triggers[0].trigger_type,
            TriggerType::Event { source, filter } if source == "feishu" && filter.as_ref().unwrap() == "message"
        ));
    }

    #[test]
    fn test_openclaw_on_error_parsing() {
        let yaml = r#"
id: test
description: test
steps:
  - id: step1
    skill: test_skill
    on_error:
      action: retry
      max_retries: 3
      delay_seconds: 10
      fallback:
        skill: slack-notify
        input:
          text: "fallback message"
"#;
        let def: WorkflowDefinition = serde_yaml::from_str(yaml).unwrap();
        let on_error = def.steps[0].on_error.as_ref().unwrap();
        assert_eq!(on_error.action, "retry");
        assert_eq!(on_error.max_retries, Some(3));
        assert_eq!(on_error.delay_seconds, Some(10));
        let fallback = on_error.fallback.as_ref().unwrap();
        assert_eq!(fallback.skill, "slack-notify");
        assert_eq!(fallback.params["text"], "fallback message");
    }

    #[test]
    fn test_openclaw_error_handler_parsing() {
        let yaml = r#"
id: test
description: test
error_handler:
  step: "any"
  action: fail
  fallback:
    skill: log-error
    params:
      message: "global error"
steps:
  - id: s1
    skill: test
"#;
        let def: WorkflowDefinition = serde_yaml::from_str(yaml).unwrap();
        let eh = def.error_handler.as_ref().unwrap();
        assert_eq!(eh.step, "any");
        assert_eq!(eh.action, "fail");
        let fallback = eh.fallback.as_ref().unwrap();
        assert_eq!(fallback.skill, "log-error");
    }

    #[test]
    fn test_json_format_parsing() {
        // Test that JSON format workflows parse correctly
        let json = r#"{
            "id": "json-workflow",
            "name": "JSON Workflow",
            "description": "Test JSON format",
            "version": "1.0.0",
            "triggers": [{"type": "manual"}],
            "steps": [
                {"id": "s1", "skill": "test_skill", "params": {"key": "value"}}
            ]
        }"#;
        let def: WorkflowDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(def.id, "json-workflow");
        assert_eq!(def.steps[0].skill, "test_skill");
        assert_eq!(def.steps[0].params["key"], "value");
    }
}
