//! Workflow Orchestration Integration Tests
//!
//! End-to-end tests covering: YAML loading → trigger matching → execution → status query.

use std::collections::HashMap;

use beebotos_agents::workflow::{
    WorkflowDefinition, WorkflowEngine, WorkflowRegistry, WorkflowStatus, StepStatus,
    TriggerEngine, TriggerType, definition::WorkflowStep,
};
use beebotos_agents::error::AgentError;

/// Mock step executor for testing without real skills
struct MockStepExecutor {
    responses: HashMap<String, String>,
}

impl MockStepExecutor {
    fn new(responses: HashMap<String, String>) -> Self {
        Self { responses }
    }
}

#[async_trait::async_trait]
impl beebotos_agents::workflow::StepExecutor for MockStepExecutor {
    async fn execute_skill(
        &self,
        skill_id: &str,
        input: &str,
        _params: HashMap<String, String>,
    ) -> Result<beebotos_agents::workflow::SkillStepResult, AgentError> {
        let output = self.responses.get(skill_id)
            .cloned()
            .unwrap_or_else(|| format!("mock:{}:input={}", skill_id, input));
        Ok(beebotos_agents::workflow::SkillStepResult {
            output,
            execution_time_ms: 10,
        })
    }
}

#[tokio::test]
async fn test_yaml_load_and_parse() {
    let yaml = r#"
id: test_pipeline
name: "Test Pipeline"
description: "Integration test pipeline"
version: "1.0.0"
tags: ["test"]
triggers:
  - type: manual
  - type: cron
    schedule: "0 9 * * *"
    timezone: "Asia/Shanghai"
config:
  timeout_sec: 60
  continue_on_failure: false
steps:
  - id: fetch_data
    name: "Fetch Data"
    skill: http_request
    params:
      url: "https://example.com/api"
    timeout_sec: 10
    retries: 1
  - id: process_data
    name: "Process Data"
    skill: data_processor
    depends_on: [fetch_data]
    params:
      input: "{{steps.fetch_data.output}}"
"#;

    let def: WorkflowDefinition = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(def.id, "test_pipeline");
    assert_eq!(def.steps.len(), 2);
    assert_eq!(def.steps[1].depends_on.as_ref().unwrap()[0], "fetch_data");
    assert!(matches!(&def.triggers[1].trigger_type, TriggerType::Cron { .. }));
}

#[tokio::test]
async fn test_workflow_registry_load_from_dir() {
    let temp_dir = tempfile::tempdir().unwrap();
    let workflow_path = temp_dir.path().join("test_workflow.yaml");
    std::fs::write(&workflow_path, r#"
id: registry_test
name: "Registry Test"
description: "Test"
steps:
  - id: step1
    name: "Step 1"
    skill: echo
    params:
      input: "hello"
"#).unwrap();

    let mut registry = WorkflowRegistry::new();
    registry.load_from_dir(temp_dir.path()).await.unwrap();

    let wf = registry.get("registry_test").expect("Workflow should be loaded");
    assert_eq!(wf.name, "Registry Test");
}

#[tokio::test]
async fn test_trigger_engine_match() {
    let def = WorkflowDefinition {
        id: "trigger_test".to_string(),
        name: "Trigger Test".to_string(),
        description: "Test".to_string(),
        version: "1.0.0".to_string(),
        author: None,
        tags: vec![],
        triggers: vec![
            beebotos_agents::workflow::TriggerDefinition {
                trigger_type: TriggerType::Manual,
            },
            beebotos_agents::workflow::TriggerDefinition {
                trigger_type: TriggerType::Webhook {
                    path: "/webhook/test".to_string(),
                    method: "POST".to_string(),
                    auth: None,
                },
            },
        ],
        config: Default::default(),
        steps: vec![],
    };

    let mut engine = TriggerEngine::new();
    engine.register(&def);

    assert!(engine.match_manual("Trigger Test").is_some());
    assert!(engine.match_manual("trigger_test").is_some());
    assert!(engine.match_webhook("/webhook/test", "POST").is_some());
    assert!(engine.match_webhook("/webhook/test", "GET").is_none());
}

#[tokio::test]
async fn test_workflow_engine_end_to_end() {
    let mut responses = HashMap::new();
    responses.insert("fetch_data".to_string(), r#"{"items": [1, 2, 3]}"#.to_string());
    responses.insert("process_data".to_string(), "processed: 3 items".to_string());

    let executor = MockStepExecutor::new(responses);
    let engine = WorkflowEngine::new();

    let definition = WorkflowDefinition {
        id: "e2e_test".to_string(),
        name: "E2E Test".to_string(),
        description: "Test".to_string(),
        version: "1.0".to_string(),
        author: None,
        tags: vec![],
        triggers: vec![],
        config: Default::default(),
        steps: vec![
            WorkflowStep {
                id: "step1".to_string(),
                name: "Fetch".to_string(),
                skill: "fetch_data".to_string(),
                params: serde_json::json!({"url": "http://example.com"}),
                depends_on: None,
                condition: None,
                timeout_sec: None,
                retries: None,
            },
            WorkflowStep {
                id: "step2".to_string(),
                name: "Process".to_string(),
                skill: "process_data".to_string(),
                params: serde_json::json!({"input": "{{steps.step1.output}}"}),
                depends_on: Some(vec!["step1".to_string()]),
                condition: None,
                timeout_sec: None,
                retries: None,
            },
        ],
    };

    let instance = engine.execute(&definition, &executor, serde_json::Value::Null, None).await.unwrap();

    assert_eq!(instance.status, WorkflowStatus::Completed);
    assert_eq!(instance.step_states.len(), 2);
    assert_eq!(instance.step_states.get("step1").unwrap().status, StepStatus::Completed);
    assert_eq!(instance.step_states.get("step2").unwrap().status, StepStatus::Completed);
}

#[tokio::test]
async fn test_workflow_with_condition_and_retry() {
    let mut responses = HashMap::new();
    responses.insert("check_status".to_string(), r#"{"status": "ok"}"#.to_string());
    responses.insert("notify".to_string(), "notification sent".to_string());

    let executor = MockStepExecutor::new(responses);
    let engine = WorkflowEngine::new();

    let definition = WorkflowDefinition {
        id: "conditional_test".to_string(),
        name: "Conditional Test".to_string(),
        description: "Test".to_string(),
        version: "1.0".to_string(),
        author: None,
        tags: vec![],
        triggers: vec![],
        config: Default::default(),
        steps: vec![
            WorkflowStep {
                id: "check".to_string(),
                name: "Check".to_string(),
                skill: "check_status".to_string(),
                params: serde_json::Value::Null,
                depends_on: None,
                condition: None,
                timeout_sec: None,
                retries: Some(2),
            },
            WorkflowStep {
                id: "notify".to_string(),
                name: "Notify".to_string(),
                skill: "notify".to_string(),
                params: serde_json::json!({"message": "{{steps.check.output}}"}),
                depends_on: Some(vec!["check".to_string()]),
                condition: Some("true".to_string()),
                timeout_sec: None,
                retries: None,
            },
        ],
    };

    let instance = engine.execute(&definition, &executor, serde_json::Value::Null, None).await.unwrap();
    assert_eq!(instance.status, WorkflowStatus::Completed);
    assert_eq!(instance.step_states.get("check").unwrap().retry_count, 0);
}

#[tokio::test]
async fn test_workflow_template_resolution() {
    let mut responses = HashMap::new();
    responses.insert("fetch_news".to_string(), r#"{"articles": [{"title": "Hello"}]}"#.to_string());
    responses.insert("summarize".to_string(), "Summary: Hello".to_string());

    let executor = MockStepExecutor::new(responses);
    let engine = WorkflowEngine::new();

    let definition = WorkflowDefinition {
        id: "template_test".to_string(),
        name: "Template Test".to_string(),
        description: "Test".to_string(),
        version: "1.0".to_string(),
        author: None,
        tags: vec![],
        triggers: vec![],
        config: Default::default(),
        steps: vec![
            WorkflowStep {
                id: "fetch_news".to_string(),
                name: "Fetch".to_string(),
                skill: "fetch_news".to_string(),
                params: serde_json::Value::Null,
                depends_on: None,
                condition: None,
                timeout_sec: None,
                retries: None,
            },
            WorkflowStep {
                id: "summarize".to_string(),
                name: "Summarize".to_string(),
                skill: "summarize".to_string(),
                params: serde_json::json!({"input": "{{steps.fetch_news.output.articles.0.title}}"}),
                depends_on: Some(vec!["fetch_news".to_string()]),
                condition: None,
                timeout_sec: None,
                retries: None,
            },
        ],
    };

    let instance = engine.execute(&definition, &executor, serde_json::Value::Null, None).await.unwrap();
    assert_eq!(instance.status, WorkflowStatus::Completed);
}
