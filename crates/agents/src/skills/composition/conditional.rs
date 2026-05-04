//! Skill Conditional — Branch execution based on conditions

use tracing::info;

use crate::agent_impl::Agent;
use crate::error::AgentError;

/// Conditional skill execution
#[derive(Debug)]
pub struct SkillConditional {
    pub condition: Condition,
    pub then_branch: Box<dyn super::CompositionNode>,
    pub else_branch: Option<Box<dyn super::CompositionNode>>,
}

/// Condition evaluation types
#[derive(Debug, Clone)]
pub enum Condition {
    /// Check if output contains a substring
    OutputContains(String),
    /// Check if output equals a value
    OutputEquals(String),
    /// Check JSON field by pointer
    JsonFieldEquals { path: String, expected: String },
    /// Check exit code (for subprocess-based skills)
    ExitCode(i32),
    /// Custom expression evaluated by WorkflowEngine
    Expression(String),
    /// LLM-based judgment (requires Agent/LLM context for full evaluation)
    /// 🚧 Placeholder: returns false when evaluated without LLM context
    LlmJudge { prompt: String },
}

impl SkillConditional {
    /// Create new conditional
    pub fn new(
        condition: Condition,
        then_branch: Box<dyn super::CompositionNode>,
        else_branch: Option<Box<dyn super::CompositionNode>>,
    ) -> Self {
        Self {
            condition,
            then_branch,
            else_branch,
        }
    }

    /// Evaluate condition and execute appropriate branch
    pub async fn execute(&self, input: &str, agent: &Agent) -> Result<String, AgentError> {
        let condition_met = self.condition.evaluate_async(input, agent).await;

        info!(
            "Conditional evaluation: condition={:?}, result={}",
            self.condition, condition_met
        );

        if condition_met {
            info!("Executing THEN branch");
            self.then_branch.execute(input, agent).await
        } else if let Some(else_branch) = &self.else_branch {
            info!("Executing ELSE branch");
            else_branch.execute(input, agent).await
        } else {
            Ok("[Conditional: no ELSE branch, condition not met]".to_string())
        }
    }
}

impl Condition {
    /// Evaluate condition against skill output (sync fallback, LlmJudge returns false)
    pub fn evaluate(&self, output: &str) -> bool {
        match self {
            Condition::OutputContains(substr) => output.contains(substr),
            Condition::OutputEquals(expected) => output.trim() == expected.trim(),
            Condition::JsonFieldEquals { path, expected } => {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(output) {
                    value
                        .pointer(path)
                        .map(|v| v.to_string().trim_matches('"').to_string() == *expected)
                        .unwrap_or(false)
                } else {
                    false
                }
            }
            Condition::ExitCode(expected_code) => {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(output) {
                    value
                        .get("exit_code")
                        .and_then(|v| v.as_i64())
                        .map(|code| code == *expected_code as i64)
                        .unwrap_or(false)
                } else {
                    false
                }
            }
            Condition::Expression(expr) => {
                crate::workflow::engine::WorkflowEngine::evaluate_condition_expression(expr)
            }
            Condition::LlmJudge { prompt } => {
                tracing::warn!(
                    "LlmJudge condition requires LLM context to evaluate; returning false. Prompt: {}",
                    prompt
                );
                false
            }
        }
    }

    /// Evaluate condition with Agent context (supports LlmJudge via LLM)
    pub async fn evaluate_async(&self, output: &str, agent: &Agent) -> bool {
        match self {
            Condition::LlmJudge { prompt } => {
                match agent.judge_condition(prompt, output).await {
                    Ok(result) => {
                        tracing::info!("LlmJudge evaluated to {} for prompt: {}", result, prompt);
                        result
                    }
                    Err(e) => {
                        tracing::warn!("LlmJudge evaluation failed: {}, defaulting to false", e);
                        false
                    }
                }
            }
            _ => self.evaluate(output),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock composition node for testing nested execution
    #[derive(Debug)]
    struct MockNode {
        output: String,
    }

    #[async_trait::async_trait]
    impl super::super::CompositionNode for MockNode {
        async fn execute(&self, _input: &str, _agent: &Agent) -> Result<String, AgentError> {
            Ok(self.output.clone())
        }
    }

    #[tokio::test]
    async fn test_conditional_nested_then_branch() {
        let then_node = MockNode { output: "then_result".to_string() };
        let else_node = MockNode { output: "else_result".to_string() };
        let conditional = SkillConditional::new(
            Condition::OutputContains("trigger".to_string()),
            Box::new(then_node),
            Some(Box::new(else_node)),
        );
        let agent = crate::AgentBuilder::new("test").build();
        let result = conditional.execute("trigger word", &agent).await.unwrap();
        assert_eq!(result, "then_result");
    }

    #[tokio::test]
    async fn test_conditional_nested_else_branch() {
        let then_node = MockNode { output: "then_result".to_string() };
        let else_node = MockNode { output: "else_result".to_string() };
        let conditional = SkillConditional::new(
            Condition::OutputContains("trigger".to_string()),
            Box::new(then_node),
            Some(Box::new(else_node)),
        );
        let agent = crate::AgentBuilder::new("test").build();
        let result = conditional.execute("no match here", &agent).await.unwrap();
        assert_eq!(result, "else_result");
    }

    #[test]
    fn test_condition_contains() {
        let cond = Condition::OutputContains("success".to_string());
        assert!(cond.evaluate("Operation completed with success"));
        assert!(!cond.evaluate("Operation failed"));
    }

    #[test]
    fn test_condition_json_field() {
        let cond = Condition::JsonFieldEquals {
            path: "/status".to_string(),
            expected: "ok".to_string(),
        };
        assert!(cond.evaluate(r#"{"status": "ok"}"#));
        assert!(!cond.evaluate(r#"{"status": "error"}"#));
    }

    #[test]
    fn test_condition_exit_code() {
        let cond = Condition::ExitCode(0);
        assert!(cond.evaluate(r#"{"exit_code": 0, "stdout": "done"}"#));
        assert!(!cond.evaluate(r#"{"exit_code": 1, "stderr": "error"}"#));
    }
}
