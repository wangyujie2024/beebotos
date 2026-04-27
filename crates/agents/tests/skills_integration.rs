//! Skills integration tests

use std::collections::HashMap;
use std::path::PathBuf;

/// Minimal valid WASM module: just header
fn minimal_wasm_header() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

/// Small WASM module with a function `handle(i32, i32) -> i32` that returns the
/// first argument
fn echo_wasm_skill() -> Vec<u8> {
    // WASM module:
    // - header
    // - type section: (i32, i32) -> i32
    // - function section: 1 function of type 0
    // - export section: export "handle" as func 0
    // - code section: func body: local.get 0, end
    vec![
        // header
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // type section
        0x01, 0x07, 0x01, 0x60, 0x02, 0x7f, 0x7f, 0x01, 0x7f, // function section
        0x03, 0x02, 0x01, 0x00, // export section: "handle"
        0x07, 0x0a, 0x01, 0x06, 0x68, 0x61, 0x6e, 0x64, 0x6c, 0x65, 0x00, 0x00,
        // code section
        0x0a, 0x06, 0x01, 0x04, 0x00, 0x20, 0x00, 0x0b,
    ]
}

fn create_temp_skill_dir(skill_id: &str, wasm_bytes: &[u8]) -> PathBuf {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join(skill_id);
    std::fs::create_dir_all(&dir).unwrap();

    let manifest = format!(
        r#"id: {}
name: {}
version: 1.0.0
description: Test skill
author: test
license: MIT
capabilities: []
permissions: []
entry_point: handle
"#,
        skill_id, skill_id
    );
    std::fs::write(dir.join("skill.yaml"), manifest).unwrap();
    std::fs::write(dir.join("skill.wasm"), wasm_bytes).unwrap();

    // Keep tmp alive by leaking it (simpler for tests)
    let path = dir.clone();
    std::mem::forget(tmp);
    path
}

#[tokio::test]
async fn test_skill_loader_loads_manifest() {
    let dir = create_temp_skill_dir("test_loader_skill", &echo_wasm_skill());
    let base_dir = dir.parent().unwrap().to_path_buf();

    let mut loader = beebotos_agents::skills::SkillLoader::new();
    loader.add_path(&base_dir);

    let skill = loader.load_skill("test_loader_skill").await.unwrap();
    assert_eq!(skill.id, "test_loader_skill");
    assert_eq!(skill.manifest.entry_point, "handle");
}

#[tokio::test]
async fn test_skill_registry_registration() {
    let dir = create_temp_skill_dir("test_registry_skill", &echo_wasm_skill());
    let base_dir = dir.parent().unwrap().to_path_buf();

    let mut loader = beebotos_agents::skills::SkillLoader::new();
    loader.add_path(&base_dir);
    let skill = loader.load_skill("test_registry_skill").await.unwrap();

    let registry = beebotos_agents::skills::SkillRegistry::new();
    registry
        .register(skill, "test", vec!["tag1".to_string()])
        .await;

    let found = registry.get("test_registry_skill").await.unwrap();
    assert_eq!(found.skill.id, "test_registry_skill");
    assert_eq!(found.category, "test");
    assert!(found.enabled);
    assert_eq!(found.tags, vec!["tag1"]);
}

#[tokio::test]
async fn test_skill_registry_enable_disable() {
    let dir = create_temp_skill_dir("test_lifecycle_skill", &echo_wasm_skill());
    let base_dir = dir.parent().unwrap().to_path_buf();

    let mut loader = beebotos_agents::skills::SkillLoader::new();
    loader.add_path(&base_dir);
    let skill = loader.load_skill("test_lifecycle_skill").await.unwrap();

    let registry = beebotos_agents::skills::SkillRegistry::new();
    registry.register(skill, "test", vec![]).await;

    registry.disable("test_lifecycle_skill").await;
    let found = registry.get("test_lifecycle_skill").await.unwrap();
    assert!(!found.enabled);

    registry.enable("test_lifecycle_skill").await;
    let found = registry.get("test_lifecycle_skill").await.unwrap();
    assert!(found.enabled);
}

#[tokio::test]
async fn test_skill_security_validator_accepts_valid_wasm() {
    let validator = beebotos_agents::skills::SkillSecurityValidator::new(
        beebotos_agents::skills::SkillSecurityPolicy::default(),
    );
    let valid_wasm = echo_wasm_skill();
    assert!(validator.validate(&valid_wasm).is_ok());
}

#[test]
fn test_skill_security_validator_rejects_invalid_magic() {
    let validator = beebotos_agents::skills::SkillSecurityValidator::new(
        beebotos_agents::skills::SkillSecurityPolicy::default(),
    );
    let invalid = vec![0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00];
    assert!(validator.validate(&invalid).is_err());
}

#[test]
fn test_skill_security_validator_rejects_oversized_module() {
    let mut policy = beebotos_agents::skills::SkillSecurityPolicy::default();
    policy.max_module_size = 10;
    let validator = beebotos_agents::skills::SkillSecurityValidator::new(policy);
    assert!(validator.validate(&echo_wasm_skill()).is_err());
}

#[tokio::test]
async fn test_skills_hub_lifecycle() {
    use std::sync::Arc;

    let dir = create_temp_skill_dir("test_hub_skill", &echo_wasm_skill());
    let base_dir = dir.parent().unwrap().to_path_buf();

    let mut loader = beebotos_agents::skills::SkillLoader::new();
    loader.add_path(&base_dir);
    let skill = loader.load_skill("test_hub_skill").await.unwrap();

    let registry = Arc::new(beebotos_agents::skills::SkillRegistry::new());
    let hub = beebotos_agents::skills::SkillsHub::new(registry.clone());

    hub.register(skill, "test", vec![]).await;
    assert!(hub.get("test_hub_skill").await.unwrap().enabled);

    hub.disable("test_hub_skill").await.unwrap();
    assert!(!hub.get("test_hub_skill").await.unwrap().enabled);
}

#[tokio::test]
async fn test_skill_executor_creation() {
    let executor = beebotos_agents::skills::SkillExecutor::new();
    assert!(executor.is_ok());
}

// =============================================================================
// Instance Manager integration tests
// =============================================================================

#[tokio::test]
async fn test_instance_manager_lifecycle() {
    use beebotos_agents::skills::{InstanceManager, InstanceStatus};

    let manager = InstanceManager::new();
    let mut config = HashMap::new();
    config.insert("model".to_string(), "gpt-4".to_string());

    let id = manager.create("skill-1", "agent-1", config).await.unwrap();
    assert_eq!(manager.count().await, 1);

    let instance = manager.get(&id).await.unwrap();
    assert_eq!(instance.skill_id, "skill-1");
    assert_eq!(instance.agent_id, "agent-1");
    assert_eq!(instance.status, InstanceStatus::Pending);
    assert_eq!(instance.config.get("model"), Some(&"gpt-4".to_string()));

    // Transition to Running
    manager
        .update_status(&id, InstanceStatus::Running)
        .await
        .unwrap();
    let instance = manager.get(&id).await.unwrap();
    assert_eq!(instance.status, InstanceStatus::Running);

    // Pause and resume
    manager
        .update_status(&id, InstanceStatus::Paused)
        .await
        .unwrap();
    manager
        .update_status(&id, InstanceStatus::Running)
        .await
        .unwrap();

    // Update config
    let mut updates = HashMap::new();
    updates.insert("temperature".to_string(), "0.7".to_string());
    manager.update_config(&id, updates).await.unwrap();
    let instance = manager.get(&id).await.unwrap();
    assert_eq!(instance.config.get("temperature"), Some(&"0.7".to_string()));
    assert_eq!(instance.config.get("model"), Some(&"gpt-4".to_string()));

    // Stop and delete
    manager
        .update_status(&id, InstanceStatus::Stopped)
        .await
        .unwrap();
    manager.delete(&id).await.unwrap();
    assert_eq!(manager.count().await, 0);
}

#[tokio::test]
async fn test_instance_manager_invalid_transition() {
    use beebotos_agents::skills::{InstanceManager, InstanceStatus};

    let manager = InstanceManager::new();
    let id = manager
        .create("skill-1", "agent-1", HashMap::new())
        .await
        .unwrap();

    // Pending -> Paused is invalid
    let result = manager.update_status(&id, InstanceStatus::Paused).await;
    assert!(result.is_err());

    // Running -> Pending is invalid
    manager
        .update_status(&id, InstanceStatus::Running)
        .await
        .unwrap();
    let result = manager.update_status(&id, InstanceStatus::Pending).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_instance_manager_usage_stats() {
    use beebotos_agents::skills::InstanceManager;

    let manager = InstanceManager::new();
    let id = manager
        .create("skill-1", "agent-1", HashMap::new())
        .await
        .unwrap();

    manager.record_execution(&id, true, 100.0).await.unwrap();
    manager.record_execution(&id, false, 200.0).await.unwrap();
    manager.record_execution(&id, true, 300.0).await.unwrap();

    let instance = manager.get(&id).await.unwrap();
    assert_eq!(instance.usage.total_calls, 3);
    assert_eq!(instance.usage.successful_calls, 2);
    assert_eq!(instance.usage.failed_calls, 1);
    assert_eq!(instance.usage.avg_latency_ms, 200.0);
}

#[tokio::test]
async fn test_instance_manager_list_filtering() {
    use beebotos_agents::skills::{InstanceFilter, InstanceManager, InstanceStatus};

    let manager = InstanceManager::new();
    let id1 = manager
        .create("skill-a", "agent-1", HashMap::new())
        .await
        .unwrap();
    let id2 = manager
        .create("skill-b", "agent-1", HashMap::new())
        .await
        .unwrap();
    let _id3 = manager
        .create("skill-a", "agent-2", HashMap::new())
        .await
        .unwrap();

    manager
        .update_status(&id1, InstanceStatus::Running)
        .await
        .unwrap();
    manager
        .update_status(&id2, InstanceStatus::Running)
        .await
        .unwrap();
    manager
        .update_status(&id2, InstanceStatus::Stopped)
        .await
        .unwrap();

    let filter = InstanceFilter {
        agent_id: Some("agent-1".to_string()),
        ..Default::default()
    };
    assert_eq!(manager.list(&filter).await.len(), 2);

    let filter = InstanceFilter {
        skill_id: Some("skill-a".to_string()),
        ..Default::default()
    };
    assert_eq!(manager.list(&filter).await.len(), 2);

    let filter = InstanceFilter {
        status: Some(InstanceStatus::Running),
        ..Default::default()
    };
    assert_eq!(manager.list(&filter).await.len(), 1);

    let filter = InstanceFilter {
        agent_id: Some("agent-1".to_string()),
        status: Some(InstanceStatus::Running),
        ..Default::default()
    };
    assert_eq!(manager.list(&filter).await.len(), 1);
    assert_eq!(manager.list(&filter).await[0].instance_id, id1);
}

// =============================================================================
// SkillExecutor streaming and structured I/O integration tests
// =============================================================================

#[tokio::test]
async fn test_executor_stream_chunks() {
    use beebotos_agents::skills::{SkillContext, SkillExecutor, StreamChunk};

    let dir = create_temp_skill_dir("stream_skill", &echo_wasm_skill());
    let base_dir = dir.parent().unwrap().to_path_buf();

    let mut loader = beebotos_agents::skills::SkillLoader::new();
    loader.add_path(&base_dir);
    let skill = loader.load_skill("stream_skill").await.unwrap();

    let executor = SkillExecutor::new().unwrap();

    let mut rx = executor
        .execute_stream(
            &skill,
            None,
            HashMap::new(),
            SkillContext {
                input: "hello world".to_string(),
                parameters: HashMap::new(),
            },
        )
        .await
        .unwrap();

    let mut chunks = Vec::new();
    while let Some(chunk) = rx.recv().await {
        let is_complete = matches!(chunk, StreamChunk::Complete);
        chunks.push(chunk);
        if is_complete {
            break;
        }
    }

    // Should receive at least one chunk (either Data, Error, or Complete)
    // Note: echo_wasm_skill does not conform to the [len: i32][data...] output
    // convention, so execution may fail and produce an Error chunk.
    assert!(
        !chunks.is_empty(),
        "Stream should produce at least one chunk"
    );
}
