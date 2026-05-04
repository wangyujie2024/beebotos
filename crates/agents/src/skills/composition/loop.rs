//! Skill Loop — Repeat execution with condition checking

use std::time::Duration;
use tracing::{info, warn};

use crate::agent_impl::Agent;
use crate::error::AgentError;

/// Loop skill execution
#[derive(Debug)]
pub struct SkillLoop {
    pub body: Box<dyn super::CompositionNode>,
    pub until: LoopCondition,
    pub max_iterations: usize,
    pub backoff_ms: u64,
}

/// Condition to stop the loop
#[derive(Debug, Clone)]
pub enum LoopCondition {
    /// Stop when output contains substring
    OutputContains(String),
    /// Stop when output equals value
    OutputEquals(String),
    /// Stop when exit code matches
    ExitCode(i32),
    /// Stop when JSON field matches
    JsonFieldEquals { path: String, expected: String },
    /// Stop when LLM judges the output meets the prompt criteria
    /// 🚧 Placeholder: requires Agent/LLM context for full evaluation
    LlmJudge { prompt: String },
    /// Retry up to N times regardless of output
    MaxAttempts(usize),
}

impl SkillLoop {
    /// Create new loop
    pub fn new(
        body: Box<dyn super::CompositionNode>,
        until: LoopCondition,
        max_iterations: usize,
        backoff_ms: u64,
    ) -> Self {
        Self {
            body,
            until,
            max_iterations,
            backoff_ms,
        }
    }

    /// Execute body in a loop until condition is met or max iterations reached
    pub async fn execute(&self, input: &str, agent: &Agent) -> Result<String, AgentError> {
        let mut last_output = input.to_string();
        let mut iteration = 0;

        loop {
            iteration += 1;
            if iteration > self.max_iterations {
                warn!(
                    "Loop max iterations ({}) reached",
                    self.max_iterations
                );
                return Ok(format!(
                    "[Loop exceeded max iterations ({})] Last output: {}",
                    self.max_iterations, last_output
                ));
            }

            info!(
                "Loop iteration {}/{}",
                iteration, self.max_iterations
            );

            // Execute body and update output for next iteration's condition check
            match self.body.execute(&last_output, agent).await {
                Ok(result) => {
                    last_output = result;
                }
                Err(e) => {
                    warn!(
                        "Loop iteration {} failed: {}",
                        iteration, e
                    );
                    last_output = format!("[ERROR: {}]", e);
                }
            }

            // Check termination condition
            if self.until.is_met_async(&last_output, iteration, agent).await {
                info!(
                    "Loop condition met at iteration {}",
                    iteration
                );
                return Ok(format!(
                    "[Loop completed after {} iterations] Output: {}",
                    iteration, last_output
                ));
            }

            // Exponential backoff before next iteration (cap at 30s)
            if self.backoff_ms > 0 {
                let delay = self.backoff_ms.saturating_mul(1u64 << (iteration - 1).min(10));
                let capped = delay.min(30000);
                tokio::time::sleep(Duration::from_millis(capped)).await;
            }
        }
    }
}

impl LoopCondition {
    /// Check if the loop should stop (sync fallback, LlmJudge returns false)
    pub fn is_met(&self, output: &str, iteration: usize) -> bool {
        match self {
            LoopCondition::OutputContains(substr) => output.contains(substr),
            LoopCondition::OutputEquals(expected) => output.trim() == expected.trim(),
            LoopCondition::ExitCode(expected) => {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(output) {
                    value
                        .get("exit_code")
                        .and_then(|v| v.as_i64())
                        .map(|code| code == *expected as i64)
                        .unwrap_or(false)
                } else {
                    false
                }
            }
            LoopCondition::JsonFieldEquals { path, expected } => {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(output) {
                    value
                        .pointer(path)
                        .map(|v| v.to_string().trim_matches('"').to_string() == *expected)
                        .unwrap_or(false)
                } else {
                    false
                }
            }
            LoopCondition::LlmJudge { prompt } => {
                warn!(
                    "LlmJudge loop condition requires LLM context to evaluate; returning false. Prompt: {}",
                    prompt
                );
                false
            }
            LoopCondition::MaxAttempts(max) => iteration >= *max,
        }
    }

    /// Check if the loop should stop with Agent context (supports LlmJudge via LLM)
    pub async fn is_met_async(&self, output: &str, iteration: usize, agent: &Agent) -> bool {
        match self {
            LoopCondition::LlmJudge { prompt } => {
                match agent.judge_condition(prompt, output).await {
                    Ok(result) => {
                        info!("LlmJudge loop condition evaluated to {} for prompt: {}", result, prompt);
                        result
                    }
                    Err(e) => {
                        warn!("LlmJudge loop evaluation failed: {}, defaulting to false", e);
                        false
                    }
                }
            }
            _ => self.is_met(output, iteration),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Stateful mock composition node that returns different output on each call
    #[derive(Debug)]
    struct CountingMockNode {
        counter: AtomicUsize,
        outputs: Vec<String>,
    }

    #[async_trait::async_trait]
    impl super::super::CompositionNode for CountingMockNode {
        async fn execute(&self, _input: &str, _agent: &Agent) -> Result<String, AgentError> {
            let idx = self.counter.fetch_add(1, Ordering::SeqCst);
            Ok(self.outputs.get(idx).cloned().unwrap_or_else(|| "default".to_string()))
        }
    }

    #[tokio::test]
    async fn test_loop_nested_body() {
        let body = CountingMockNode {
            counter: AtomicUsize::new(0),
            outputs: vec!["running".to_string(), "done".to_string()],
        };
        let loop_node = SkillLoop::new(
            Box::new(body),
            LoopCondition::OutputContains("done".to_string()),
            5,
            0,
        );
        let agent = crate::AgentBuilder::new("test").build();
        let result = loop_node.execute("start", &agent).await.unwrap();
        assert!(result.contains("done"));
        assert!(result.contains("2 iterations"));
    }

    #[tokio::test]
    async fn test_loop_nested_max_iterations() {
        let body = CountingMockNode {
            counter: AtomicUsize::new(0),
            outputs: vec!["a".to_string(), "b".to_string()],
        };
        let loop_node = SkillLoop::new(
            Box::new(body),
            LoopCondition::OutputContains("never".to_string()),
            2,
            0,
        );
        let agent = crate::AgentBuilder::new("test").build();
        let result = loop_node.execute("start", &agent).await.unwrap();
        assert!(result.contains("exceeded max iterations (2)"));
    }

    #[test]
    fn test_loop_condition_max_attempts() {
        let cond = LoopCondition::MaxAttempts(3);
        assert!(!cond.is_met("anything", 1));
        assert!(!cond.is_met("anything", 2));
        assert!(cond.is_met("anything", 3));
    }

    #[test]
    fn test_loop_condition_contains() {
        let cond = LoopCondition::OutputContains("done".to_string());
        assert!(cond.is_met("Process is done", 1));
        assert!(!cond.is_met("Still running", 1));
    }

    #[test]
    fn test_loop_condition_exit_code() {
        let cond = LoopCondition::ExitCode(0);
        assert!(cond.is_met(r#"{"exit_code": 0}"#, 1));
        assert!(!cond.is_met(r#"{"exit_code": 1}"#, 1));
    }
}
