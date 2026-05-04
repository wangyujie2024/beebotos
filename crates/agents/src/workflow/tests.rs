//! Workflow Integration Tests
//!
//! Tests the full pipeline: definition parsing → template resolution →
//! engine execution → trigger matching → cancellation.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::workflow::{
    definition::{WorkflowDefinition, WorkflowStep},
    engine::{SkillStepResult, StepExecutor, WorkflowEngine},
    state::{StepStatus, WorkflowStatus},
    template::{resolve_template, TemplateContext},
    trigger::TriggerEngine,
    WorkflowRegistry,
};

/// Mock step executor for integration tests
struct TestStepExecutor {
    outputs: HashMap<String, String>,
}

#[async_trait::async_trait]
impl StepExecutor for TestStepExecutor {
    async fn execute_skill(
        &self,
        skill_id: &str,
        _input: &str,
        _params: HashMap<String, String>,
    ) -> Result<SkillStepResult, crate::error::AgentError> {
        let output = self
            .outputs
            .get(skill_id)
            .cloned()
            .unwrap_or_else(|| format!("default-output-{}", skill_id));
        Ok(SkillStepResult {
            output,
            execution_time_ms: 10,
        })
    }
}

#[test]
fn test_parse_and_validate_workflow_yaml() {
    let yaml = r#"
id: "test_pipeline"
name: "Test Pipeline"
description: "A test workflow"
version: "1.0.0"
tags: ["test", "integration"]
triggers:
  - type: manual
  - type: cron
    schedule: "0 9 * * *"
    timezone: "UTC"
config:
  timeout_sec: 120
  continue_on_failure: false
  notify_on_complete: true
steps:
  - id: fetch_data
    name: "Fetch Data"
    skill: http_request
    params:
      url: "https://example.com/api"
    timeout_sec: 30
    retries: 2

  - id: process_data
    name: "Process Data"
    skill: data_processor
    depends_on: ["fetch_data"]
    params:
      input: "{{steps.fetch_data.output}}"

  - id: notify
    name: "Notify"
    skill: email_sender
    depends_on: ["process_data"]
    condition: "{{workflow.any_failed}} == false"
    params:
      to: "admin@example.com"
      body: "Pipeline completed"
"#;

    let def: WorkflowDefinition = serde_yaml::from_str(yaml).expect("Should parse valid YAML");
    assert_eq!(def.id, "test_pipeline");
    assert_eq!(def.steps.len(), 3);
    assert_eq!(def.triggers.len(), 2);
    assert!(def.config.notify_on_complete);

    let step2 = &def.steps[1];
    assert_eq!(step2.depends_on.as_ref().unwrap()[0], "fetch_data");

    let step3 = &def.steps[2];
    assert_eq!(step3.condition.as_ref().unwrap(), "{{workflow.any_failed}} == false");
}

#[test]
fn test_template_resolution_pipeline() {
    let mut ctx = TemplateContext::new();
    ctx.add_step_output(
        "fetch_data",
        serde_json::json!({"items": [{"name": "item1"}, {"name": "item2"}]}),
    );
    ctx.add_step_status("fetch_data", "completed");
    ctx.workflow_failed = false;

    let resolved = resolve_template("{{steps.fetch_data.output.items.0.name}}", &ctx).unwrap();
    assert_eq!(resolved, "item1");

    let resolved = resolve_template("{{workflow.any_failed}}", &ctx).unwrap();
    assert_eq!(resolved, "false");

    std::env::set_var("TEST_VAR", "hello");
    let resolved = resolve_template("${TEST_VAR}", &ctx).unwrap();
    assert_eq!(resolved, "hello");
}

#[tokio::test]
async fn test_engine_executes_dag_with_dependencies() {
    let mut outputs = HashMap::new();
    outputs.insert("fetch_data".to_string(), r#"{"items": [1, 2, 3]}"#.to_string());
    outputs.insert("process_data".to_string(), r#"{"result": "ok"}"#.to_string());
    outputs.insert("notify".to_string(), "sent".to_string());

    let executor = TestStepExecutor { outputs };
    let engine = WorkflowEngine::new();

    let definition = WorkflowDefinition {
        id: "dag_test".to_string(),
        name: "DAG Test".to_string(),
        description: "Test".to_string(),
        version: "1.0".to_string(),
        author: None,
        tags: vec![],
        triggers: vec![],
        config: Default::default(),
        steps: vec![
            WorkflowStep {
                id: "fetch_data".to_string(),
                name: "Fetch".to_string(),
                skill: "fetch_data".to_string(),
                params: serde_json::Value::Null,
                depends_on: None,
                condition: None,
                timeout_sec: None,
                retries: None,
            },
            WorkflowStep {
                id: "process_data".to_string(),
                name: "Process".to_string(),
                skill: "process_data".to_string(),
                params: serde_json::json!({"input": "{{steps.fetch_data.output}}"}),
                depends_on: Some(vec!["fetch_data".to_string()]),
                condition: None,
                timeout_sec: None,
                retries: None,
            },
            WorkflowStep {
                id: "notify".to_string(),
                name: "Notify".to_string(),
                skill: "notify".to_string(),
                params: serde_json::Value::Null,
                depends_on: Some(vec!["process_data".to_string()]),
                condition: None,
                timeout_sec: None,
                retries: None,
            },
        ],
    };

    let instance = engine
        .execute(&definition, &executor, serde_json::Value::Null, None)
        .await
        .expect("Execution should succeed");

    assert_eq!(instance.status, WorkflowStatus::Completed);
    assert_eq!(instance.step_states.len(), 3);
    assert_eq!(
        instance.step_states.get("fetch_data").unwrap().status,
        StepStatus::Completed
    );
    assert_eq!(
        instance.step_states.get("process_data").unwrap().status,
        StepStatus::Completed
    );
    assert_eq!(
        instance.step_states.get("notify").unwrap().status,
        StepStatus::Completed
    );
}

#[tokio::test]
async fn test_engine_cancellation_stops_new_steps() {
    let mut outputs = HashMap::new();
    outputs.insert("step_a".to_string(), "ok".to_string());
    outputs.insert("step_b".to_string(), "ok".to_string());
    outputs.insert("step_c".to_string(), "ok".to_string());

    let executor = TestStepExecutor { outputs };
    let engine = WorkflowEngine::new();

    let definition = WorkflowDefinition {
        id: "cancel_test".to_string(),
        name: "Cancel Test".to_string(),
        description: "Test".to_string(),
        version: "1.0".to_string(),
        author: None,
        tags: vec![],
        triggers: vec![],
        config: Default::default(),
        steps: vec![
            WorkflowStep {
                id: "step_a".to_string(),
                name: "A".to_string(),
                skill: "step_a".to_string(),
                params: serde_json::Value::Null,
                depends_on: None,
                condition: None,
                timeout_sec: None,
                retries: None,
            },
            WorkflowStep {
                id: "step_b".to_string(),
                name: "B".to_string(),
                skill: "step_b".to_string(),
                params: serde_json::Value::Null,
                depends_on: Some(vec!["step_a".to_string()]),
                condition: None,
                timeout_sec: None,
                retries: None,
            },
            WorkflowStep {
                id: "step_c".to_string(),
                name: "C".to_string(),
                skill: "step_c".to_string(),
                params: serde_json::Value::Null,
                depends_on: Some(vec!["step_b".to_string()]),
                condition: None,
                timeout_sec: None,
                retries: None,
            },
        ],
    };

    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let cancel_clone = cancel.clone();

    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        cancel_clone.store(true, std::sync::atomic::Ordering::Relaxed);
    });

    let instance = engine
        .execute_with_cancel(
            &definition,
            &executor,
            serde_json::Value::Null,
            None,
            Some(cancel),
        )
        .await
        .expect("Execution should return instance");

    assert!(instance.status.is_terminal());
}

#[test]
fn test_trigger_engine_event_filtering() {
    let mut engine = TriggerEngine::new();

    let def = WorkflowDefinition {
        id: "event_wf".to_string(),
        name: "Event Workflow".to_string(),
        description: "Test".to_string(),
        version: "1.0".to_string(),
        author: None,
        tags: vec![],
        triggers: vec![crate::workflow::definition::TriggerDefinition {
            trigger_type: crate::workflow::definition::TriggerType::Event {
                source: "price_feed".to_string(),
                filter: Some("$.btc_usd_change > 0.02".to_string()),
            },
        }],
        config: Default::default(),
        steps: vec![WorkflowStep {
            id: "s1".to_string(),
            name: "S1".to_string(),
            skill: "skill".to_string(),
            params: serde_json::Value::Null,
            depends_on: None,
            condition: None,
            timeout_sec: None,
            retries: None,
        }],
    };

    engine.register(&def);

    let payload = serde_json::json!({"btc_usd_change": 0.05});
    let matches = engine.match_event("price_feed", &payload);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].workflow_id, "event_wf");

    let payload = serde_json::json!({"btc_usd_change": 0.01});
    let matches = engine.match_event("price_feed", &payload);
    assert!(matches.is_empty());

    let payload = serde_json::json!({"btc_usd_change": 0.05});
    let matches = engine.match_event("other_source", &payload);
    assert!(matches.is_empty());
}

#[test]
fn test_trigger_engine_jsonpath_truthiness() {
    let mut engine = TriggerEngine::new();

    let def = WorkflowDefinition {
        id: "truthy_wf".to_string(),
        name: "Truthy Workflow".to_string(),
        description: "Test".to_string(),
        version: "1.0".to_string(),
        author: None,
        tags: vec![],
        triggers: vec![crate::workflow::definition::TriggerDefinition {
            trigger_type: crate::workflow::definition::TriggerType::Event {
                source: "system".to_string(),
                filter: Some("$.enabled".to_string()),
            },
        }],
        config: Default::default(),
        steps: vec![WorkflowStep {
            id: "s1".to_string(),
            name: "S1".to_string(),
            skill: "skill".to_string(),
            params: serde_json::Value::Null,
            depends_on: None,
            condition: None,
            timeout_sec: None,
            retries: None,
        }],
    };

    engine.register(&def);

    let matches = engine.match_event("system", &serde_json::json!({"enabled": true}));
    assert_eq!(matches.len(), 1);

    let matches = engine.match_event("system", &serde_json::json!({"enabled": false}));
    assert!(matches.is_empty());
}

#[tokio::test]
async fn test_workflow_registry_and_execution() {
    let mut registry = WorkflowRegistry::new();

    let def = WorkflowDefinition {
        id: "registry_test".to_string(),
        name: "Registry Test".to_string(),
        description: "Test".to_string(),
        version: "1.0".to_string(),
        author: None,
        tags: vec!["test".to_string()],
        triggers: vec![],
        config: Default::default(),
        steps: vec![WorkflowStep {
            id: "s1".to_string(),
            name: "S1".to_string(),
            skill: "s1".to_string(),
            params: serde_json::Value::Null,
            depends_on: None,
            condition: None,
            timeout_sec: None,
            retries: None,
        }],
    };

    registry.register(def.clone());
    assert!(registry.get("registry_test").is_some());

    let executor = TestStepExecutor {
        outputs: HashMap::new(),
    };
    let engine = WorkflowEngine::new();

    let instance = engine
        .execute(&def, &executor, serde_json::Value::Null, None)
        .await
        .expect("Should execute");

    assert_eq!(instance.status, WorkflowStatus::Completed);
}


// ============================================================================
// YAML Example Workflow Validation Tests
// ============================================================================

/// Helper: collect all `{{steps.<id>.*}}` template references from a string
fn collect_step_references(template: &str) -> Vec<String> {
    let re = regex::Regex::new(r"\{\{\s*steps\.([a-zA-Z0-9_]+)\.(?:output|status)").unwrap();
    re.captures_iter(template)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

/// Helper: recursively collect step references from a JSON Value
fn collect_step_refs_from_value(value: &serde_json::Value) -> Vec<String> {
    let mut refs = Vec::new();
    match value {
        serde_json::Value::String(s) => {
            refs.extend(collect_step_references(s));
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                refs.extend(collect_step_refs_from_value(item));
            }
        }
        serde_json::Value::Object(map) => {
            for (_, v) in map.iter() {
                refs.extend(collect_step_refs_from_value(v));
            }
        }
        _ => {}
    }
    refs
}

/// Helper: validate a workflow definition's structural integrity
fn validate_workflow(def: &WorkflowDefinition) -> Result<(), String> {
    // 1. Check for duplicate step IDs
    let mut seen_ids = HashSet::new();
    for step in &def.steps {
        if !seen_ids.insert(step.id.clone()) {
            return Err(format!("Duplicate step ID: {}", step.id));
        }
    }

    // 2. Validate DAG (no cycles)
    let sorted = WorkflowEngine::topological_sort(&def.steps)
        .map_err(|e| format!("DAG validation failed: {}", e))?;

    // 3. Validate all depends_on references point to existing steps
    let step_id_set: HashSet<String> = def.steps.iter().map(|s| s.id.clone()).collect();
    for step in &def.steps {
        if let Some(deps) = &step.depends_on {
            for dep in deps {
                if !step_id_set.contains(dep) {
                    return Err(format!(
                        "Step '{}' depends_on unknown step '{}'",
                        step.id, dep
                    ));
                }
            }
        }
    }

    // 4. Validate all {{steps.x.*}} references point to existing step IDs
    for step in &def.steps {
        let refs = collect_step_refs_from_value(&step.params);
        for ref_id in &refs {
            if !step_id_set.contains(ref_id) {
                return Err(format!(
                    "Step '{}' references unknown step '{}' in params",
                    step.id, ref_id
                ));
            }
        }

        // Also check condition expressions
        if let Some(condition) = &step.condition {
            let cond_refs = collect_step_references(condition);
            for ref_id in &cond_refs {
                if !step_id_set.contains(ref_id) {
                    return Err(format!(
                        "Step '{}' references unknown step '{}' in condition",
                        step.id, ref_id
                    ));
                }
            }
        }
    }

    // 5. Validate that step references only point to earlier steps in topological order
    let step_order: std::collections::HashMap<String, usize> = sorted
        .iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), i))
        .collect();

    for step in &def.steps {
        let step_idx = step_order.get(&step.id).copied().unwrap_or(0);

        let refs = collect_step_refs_from_value(&step.params);
        for ref_id in &refs {
            let ref_idx = step_order.get(ref_id).copied().unwrap_or(0);
            if ref_idx >= step_idx {
                return Err(format!(
                    "Step '{}' (order {}) references step '{}' (order {}) which is not before it in DAG",
                    step.id, step_idx, ref_id, ref_idx
                ));
            }
        }

        if let Some(condition) = &step.condition {
            let cond_refs = collect_step_references(condition);
            for ref_id in &cond_refs {
                let ref_idx = step_order.get(ref_id).copied().unwrap_or(0);
                if ref_idx >= step_idx {
                    return Err(format!(
                        "Step '{}' (order {}) condition references step '{}' (order {}) which is not before it",
                        step.id, step_idx, ref_id, ref_idx
                    ));
                }
            }
        }
    }

    // 6. Validate trigger configurations
    for trigger in &def.triggers {
        match &trigger.trigger_type {
            crate::workflow::definition::TriggerType::Cron { schedule, .. } => {
                // Basic cron validation: should have 5 fields
                let parts: Vec<&str> = schedule.split_whitespace().collect();
                if parts.len() != 5 {
                    return Err(format!(
                        "Invalid cron schedule '{}': expected 5 fields, got {}",
                        schedule, parts.len()
                    ));
                }
            }
            crate::workflow::definition::TriggerType::Webhook { path, .. } => {
                if !path.starts_with('/') {
                    return Err(format!(
                        "Webhook path '{}' must start with '/'",
                        path
                    ));
                }
            }
            _ => {}
        }
    }

    Ok(())
}

#[test]
fn test_content_factory_yaml_valid() {
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = workspace.join("examples/workflows/content_factory.yaml");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));

    let def: WorkflowDefinition = serde_yaml::from_str(&content)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {}", path.display(), e));

    // Basic assertions
    assert_eq!(def.id, "content_factory");
    assert_eq!(def.name, "multi_agent_content_factory");
    assert_eq!(def.steps.len(), 5);
    assert_eq!(def.triggers.len(), 2);

    // Trigger checks
    let has_manual = def.triggers.iter().any(|t| matches!(t.trigger_type, crate::workflow::definition::TriggerType::Manual));
    let has_webhook = def.triggers.iter().any(|t| matches!(t.trigger_type, crate::workflow::definition::TriggerType::Webhook { path: ref p, .. } if p == "/webhook/content-factory"));
    assert!(has_manual, "Should have manual trigger");
    assert!(has_webhook, "Should have webhook trigger at /webhook/content-factory");

    // Step structure checks
    let plan_step = def.steps.iter().find(|s| s.id == "plan_tasks").expect("plan_tasks step exists");
    assert!(plan_step.depends_on.is_none(), "plan_tasks should have no dependencies");

    let research_step = def.steps.iter().find(|s| s.id == "research_parallel").expect("research_parallel step exists");
    assert_eq!(research_step.depends_on.as_ref().unwrap(), &["plan_tasks"]);
    assert_eq!(research_step.skill, "parallel_delegate");

    let draft_step = def.steps.iter().find(|s| s.id == "draft_content").expect("draft_content step exists");
    assert_eq!(draft_step.depends_on.as_ref().unwrap(), &["research_parallel"]);

    let review_step = def.steps.iter().find(|s| s.id == "review_content").expect("review_content step exists");
    assert_eq!(review_step.depends_on.as_ref().unwrap(), &["draft_content"]);

    let publish_step = def.steps.iter().find(|s| s.id == "publish_or_revise").expect("publish_or_revise step exists");
    assert_eq!(publish_step.depends_on.as_ref().unwrap(), &["review_content"]);
    assert_eq!(publish_step.condition.as_ref().unwrap(), "{{steps.review_content.output.score}} >= 80");

    // Full structural validation
    validate_workflow(&def).expect("content_factory workflow should be structurally valid");
}

#[test]
fn test_manga_pipeline_yaml_valid() {
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = workspace.join("examples/workflows/manga_pipeline.yaml");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));

    let def: WorkflowDefinition = serde_yaml::from_str(&content)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {}", path.display(), e));

    // Basic assertions
    assert_eq!(def.id, "manga_video_pipeline");
    assert_eq!(def.name, "manga_video_pipeline");
    assert_eq!(def.steps.len(), 8);
    assert_eq!(def.triggers.len(), 2);

    // Trigger checks
    let has_cron = def.triggers.iter().any(|t| matches!(t.trigger_type, crate::workflow::definition::TriggerType::Cron { schedule: ref s, .. } if s == "0 2 * * *"));
    let has_manual = def.triggers.iter().any(|t| matches!(t.trigger_type, crate::workflow::definition::TriggerType::Manual));
    assert!(has_cron, "Should have cron trigger at 0 2 * * *");
    assert!(has_manual, "Should have manual trigger");

    // Linear pipeline: each step depends on the previous
    let expected_deps: Vec<(&str, Option<Vec<String>>)> = vec![
        ("generate_idea", None),
        ("generate_script", Some(vec!["generate_idea".to_string()])),
        ("storyboard_design", Some(vec!["generate_script".to_string()])),
        ("generate_assets", Some(vec!["storyboard_design".to_string()])),
        ("video_compose", Some(vec!["generate_assets".to_string()])),
        ("post_process", Some(vec!["video_compose".to_string()])),
        ("publish", Some(vec!["post_process".to_string()])),
        ("notify_complete", Some(vec!["publish".to_string()])),
    ];

    for (step_id, expected_dep) in expected_deps {
        let step = def.steps.iter().find(|s| s.id == step_id).expect(&format!("{} step exists", step_id));
        assert_eq!(step.depends_on, expected_dep, "Step {} dependencies mismatch", step_id);
    }

    // Template reference checks
    let script_step = def.steps.iter().find(|s| s.id == "generate_script").unwrap();
    let refs = collect_step_refs_from_value(&script_step.params);
    assert!(refs.contains(&"generate_idea".to_string()), "generate_script should reference generate_idea");

    let storyboard_step = def.steps.iter().find(|s| s.id == "storyboard_design").unwrap();
    let refs = collect_step_refs_from_value(&storyboard_step.params);
    assert!(refs.contains(&"generate_script".to_string()));

    let notify_step = def.steps.iter().find(|s| s.id == "notify_complete").unwrap();
    let refs = collect_step_refs_from_value(&notify_step.params);
    assert!(refs.contains(&"generate_script".to_string()), "notify_complete should reference generate_script for title");
    assert!(refs.contains(&"publish".to_string()), "notify_complete should reference publish for urls");

    // Full structural validation
    validate_workflow(&def).expect("manga_pipeline workflow should be structurally valid");
}

#[test]
fn test_content_factory_dag_execution_order() {
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = workspace.join("examples/workflows/content_factory.yaml");
    let content = std::fs::read_to_string(&path).unwrap();
    let def: WorkflowDefinition = serde_yaml::from_str(&content).unwrap();

    let sorted = WorkflowEngine::topological_sort(&def.steps).unwrap();

    // plan_tasks must be first (no deps)
    assert_eq!(sorted[0], "plan_tasks");

    // research_parallel must come after plan_tasks
    let plan_idx = sorted.iter().position(|s| s == "plan_tasks").unwrap();
    let research_idx = sorted.iter().position(|s| s == "research_parallel").unwrap();
    assert!(research_idx > plan_idx, "research_parallel must come after plan_tasks");

    // draft_content must come after research_parallel
    let draft_idx = sorted.iter().position(|s| s == "draft_content").unwrap();
    assert!(draft_idx > research_idx, "draft_content must come after research_parallel");

    // review_content must come after draft_content
    let review_idx = sorted.iter().position(|s| s == "review_content").unwrap();
    assert!(review_idx > draft_idx, "review_content must come after draft_content");

    // publish_or_revise must come after review_content
    let publish_idx = sorted.iter().position(|s| s == "publish_or_revise").unwrap();
    assert!(publish_idx > review_idx, "publish_or_revise must come after review_content");
}

#[test]
fn test_manga_pipeline_dag_execution_order() {
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = workspace.join("examples/workflows/manga_pipeline.yaml");
    let content = std::fs::read_to_string(&path).unwrap();
    let def: WorkflowDefinition = serde_yaml::from_str(&content).unwrap();

    let sorted = WorkflowEngine::topological_sort(&def.steps).unwrap();

    // Linear pipeline: order must be exact
    assert_eq!(sorted, vec![
        "generate_idea",
        "generate_script",
        "storyboard_design",
        "generate_assets",
        "video_compose",
        "post_process",
        "publish",
        "notify_complete",
    ]);
}
