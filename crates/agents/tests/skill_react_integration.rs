//! Integration tests for the new Skill ReAct execution system

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use beebotos_agents::skills::{
    CodeSkillExecutor, KnowledgeSkillExecutor, SkillDiscovery, SkillKind,
};
use beebotos_agents::skills::tool_set::{FileReadTool, ProcessExecTool, SkillTool};

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// Mock LLM interface that simulates tool usage for testing
struct MockLLM {
    /// Absolute path to the skill directory (used to construct commands)
    skill_dir: PathBuf,
}

#[async_trait::async_trait]
impl beebotos_agents::communication::LLMCallInterface for MockLLM {
    async fn call_llm(
        &self,
        messages: Vec<beebotos_agents::communication::Message>,
        _context: Option<HashMap<String, String>>,
    ) -> beebotos_agents::error::Result<String> {
        let prompt = messages
            .into_iter()
            .map(|m| m.content)
            .collect::<Vec<_>>()
            .join("\n");

        let hello_py = self.skill_dir.join("hello.py").to_string_lossy().to_string();

        // Simulate LLM deciding to use process_exec on first call
        if prompt.contains("hello") && prompt.contains("process_exec") && !prompt.contains("Observation") {
            Ok(format!(
                r#"
ACTION: process_exec
PARAMETERS: {{"command": "python3 {} --name Alice", "working_dir": "."}}
"#,
                hello_py
            ))
        } else if prompt.contains("Observation") {
            // After receiving tool result, provide final answer
            Ok("The script says: Hello, Alice! Welcome to BeeBotOS Skill system.".to_string())
        } else {
            Ok("I don't know what to do.".to_string())
        }
    }

    async fn call_llm_stream(
        &self,
        _messages: Vec<beebotos_agents::communication::Message>,
        _context: Option<HashMap<String, String>>,
    ) -> beebotos_agents::error::Result<tokio::sync::mpsc::Receiver<String>> {
        unimplemented!()
    }
}

#[tokio::test]
async fn test_skill_discovery_finds_directory_skills() {
    let mut discovery = SkillDiscovery::new();
    discovery.add_path(project_root().join("skills"));

    let metas = discovery.scan().await;

    // Should find the hello-world directory skill
    let hello = metas.iter().find(|m| m.id == "hello_world");
    assert!(
        hello.is_some(),
        "Expected to find hello-world skill. Found: {:?}",
        metas.iter().map(|m| &m.id).collect::<Vec<_>>()
    );

    let hello = hello.unwrap();
    assert_eq!(hello.kind, SkillKind::Code);
    assert_eq!(hello.category, "daily");
}

#[tokio::test]
async fn test_skill_discovery_finds_legacy_flat_md() {
    let mut discovery = SkillDiscovery::new();
    discovery.add_path(project_root().join("skills"));

    let metas = discovery.scan().await;

    // Should find legacy flat-file skills like python_developer
    let python = metas.iter().find(|m| m.id == "python_developer");
    assert!(
        python.is_some(),
        "Expected to find python_developer skill. Found: {:?}",
        metas.iter().map(|m| &m.id).collect::<Vec<_>>()
    );

    let python = python.unwrap();
    assert_eq!(python.kind, SkillKind::Knowledge);
}

#[tokio::test]
async fn test_process_exec_tool_runs_script() {
    let skill_dir = project_root().join("skills/daily/hello_world");
    let tool = ProcessExecTool::new(vec![skill_dir.clone()]);
    let hello_py = skill_dir.join("hello.py").to_string_lossy().to_string();
    let params = serde_json::json!({
        "command": format!("python3 {} --name Test", hello_py),
        "timeout_ms": 10000
    });

    let result = tool.execute(&params).await;
    assert!(result.is_ok(), "Tool execution failed: {:?}", result);
    let output = result.unwrap();
    assert!(
        output.contains("Hello, Test!"),
        "Expected greeting in output, got: {}",
        output
    );
}

#[tokio::test]
async fn test_process_exec_tool_blocks_dangerous_command() {
    let tool = ProcessExecTool::new(vec![PathBuf::from(".")]);
    let params = serde_json::json!({
        "command": "rm -rf /",
        "timeout_ms": 10000
    });

    let result = tool.execute(&params).await;
    assert!(result.is_err(), "Expected dangerous command to be blocked");
    let err = result.unwrap_err();
    assert!(err.contains("blocked"), "Expected 'blocked' in error: {}", err);
}

#[tokio::test]
async fn test_file_read_tool() {
    let tool = FileReadTool;
    let skill_md = project_root().join("skills/daily/hello_world/SKILL.md");
    let params = serde_json::json!({
        "path": skill_md.to_string_lossy().to_string()
    });

    let result = tool.execute(&params).await;
    assert!(result.is_ok(), "File read failed: {:?}", result);
    let output = result.unwrap();
    assert!(output.contains("Hello World"), "Expected skill content, got: {}", output);
}

#[tokio::test]
async fn test_code_skill_executor_runs_react_loop() {
    let skill_dir = project_root().join("skills/daily/hello_world");
    let llm = Arc::new(MockLLM { skill_dir: skill_dir.clone() });
    let executor = CodeSkillExecutor::new(llm);

    let result = executor
        .execute(&skill_dir, "Say hello to Alice")
        .await;

    assert!(result.is_ok(), "Code skill execution failed: {:?}", result);
    let output = result.unwrap();
    assert!(
        output.contains("Hello, Alice"),
        "Expected 'Hello, Alice' in output, got: {}",
        output
    );
}

#[tokio::test]
async fn test_knowledge_skill_executor_runs_react_loop() {
    // Use a legacy flat-file skill as knowledge skill
    let llm = Arc::new(MockLLM { skill_dir: PathBuf::new() });
    let executor = KnowledgeSkillExecutor::new(llm);

    let skill_md = project_root().join("skills/coding/python_developer.md");
    let result = executor
        .execute(&skill_md, "How do I write a Python function?")
        .await;

    assert!(result.is_ok(), "Knowledge skill execution failed: {:?}", result);
    let output = result.unwrap();
    assert!(
        !output.is_empty(),
        "Expected non-empty output from knowledge skill, got empty string"
    );
}
