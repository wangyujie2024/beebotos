//! ReAct (Reasoning + Acting) Framework
//!
//! Advanced task planning and execution framework that combines
//! reasoning capabilities with action execution in an iterative loop.
//!
//! # Features
//! - Chain-of-thought reasoning
//! - Tool use and action execution
//! - Observation processing
//! - Self-reflection and correction
//! - Multi-step task decomposition

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info};
use uuid::Uuid;

use crate::error::{AgentError, Result};

/// ReAct agent that performs reasoning and actions
pub struct ReActAgent {
    /// Agent configuration
    config: ReActConfig,
    /// Tool registry
    tools: Arc<RwLock<HashMap<String, Box<dyn Tool>>>>,
    /// Session memory
    memory: Arc<RwLock<Vec<ReActStep>>>,
    /// LLM interface for reasoning
    llm_interface: Option<Arc<dyn LLMInterface>>,
}

/// ReAct configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReActConfig {
    /// Maximum number of reasoning steps
    pub max_steps: usize,
    /// Whether to enable self-reflection
    pub enable_reflection: bool,
    /// Temperature for LLM reasoning
    pub reasoning_temperature: f32,
    /// Stop phrases that indicate completion
    pub stop_phrases: Vec<String>,
    /// Whether to allow tool retries
    pub allow_tool_retry: bool,
    /// Maximum tool retries
    pub max_tool_retries: u32,
}

impl Default for ReActConfig {
    fn default() -> Self {
        Self {
            max_steps: 10,
            enable_reflection: true,
            reasoning_temperature: 0.7,
            stop_phrases: vec![
                "I have completed the task".to_string(),
                "Task completed successfully".to_string(),
                "The answer is".to_string(),
            ],
            allow_tool_retry: true,
            max_tool_retries: 3,
        }
    }
}

/// A single step in the ReAct loop
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReActStep {
    /// Step number
    pub step_number: usize,
    /// Thought process
    pub thought: String,
    /// Action taken
    pub action: Option<Action>,
    /// Observation from action
    pub observation: Option<String>,
    /// Whether this step was successful
    pub success: bool,
    /// Reflection on the step
    pub reflection: Option<String>,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Action types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    /// Use a tool
    ToolUse {
        tool_name: String,
        parameters: HashMap<String, serde_json::Value>,
    },
    /// Final answer
    FinalAnswer { answer: String },
    /// Request more information
    RequestInfo { question: String },
    /// Delegate to another agent
    Delegate { agent_id: String, task: String },
}

/// Tool trait for extensible actions
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    /// Tool name
    fn name(&self) -> &str;
    /// Tool description for LLM
    fn description(&self) -> &str;
    /// Tool parameters schema
    fn parameters_schema(&self) -> serde_json::Value;
    /// Execute the tool
    async fn execute(&self, params: HashMap<String, serde_json::Value>) -> Result<ToolResult>;
}

/// Tool execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Success status
    pub success: bool,
    /// Result data
    pub data: serde_json::Value,
    /// Error message if failed
    pub error: Option<String>,
}

impl ToolResult {
    pub fn success(data: impl Serialize) -> Result<Self> {
        Ok(Self {
            success: true,
            data: serde_json::to_value(data)?,
            error: None,
        })
    }

    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            data: serde_json::Value::Null,
            error: Some(error.into()),
        }
    }
}

/// LLM interface for reasoning
#[async_trait::async_trait]
pub trait LLMInterface: Send + Sync {
    /// Generate reasoning and action
    async fn reason(&self, prompt: &str, context: &[ReActStep]) -> Result<String>;
    /// Parse action from LLM response
    async fn parse_action(&self, response: &str) -> Result<Action>;
}

/// ReAct execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReActResult {
    /// Task ID
    pub task_id: String,
    /// Final answer
    pub answer: Option<String>,
    /// All steps taken
    pub steps: Vec<ReActStep>,
    /// Number of steps used
    pub steps_used: usize,
    /// Whether task was completed
    pub completed: bool,
    /// Total execution time in seconds
    pub execution_time_secs: f64,
}

impl ReActAgent {
    /// Create a new ReAct agent
    pub fn new(config: ReActConfig) -> Self {
        Self {
            config,
            tools: Arc::new(RwLock::new(HashMap::new())),
            memory: Arc::new(RwLock::new(Vec::new())),
            llm_interface: None,
        }
    }

    /// Register a tool
    pub async fn register_tool(&self, tool: Box<dyn Tool>) {
        let tool_name = tool.name().to_string();
        let mut tools = self.tools.write().await;
        tools.insert(tool_name.clone(), tool);
        info!("Registered tool: {}", tool_name);
    }

    /// Set LLM interface
    pub fn set_llm_interface(&mut self, interface: Arc<dyn LLMInterface>) {
        self.llm_interface = Some(interface);
    }

    /// Execute a task using ReAct loop
    pub async fn execute(&self, task: &str) -> Result<ReActResult> {
        let task_id = Uuid::new_v4().to_string();
        let start_time = std::time::Instant::now();

        info!("Starting ReAct execution for task: {}", task_id);

        let llm = self
            .llm_interface
            .as_ref()
            .ok_or_else(|| AgentError::configuration("LLM interface not set"))?;

        let mut steps = Vec::new();
        let mut completed = false;
        let mut final_answer = None;

        for step_num in 1..=self.config.max_steps {
            debug!("ReAct step {}/{}", step_num, self.config.max_steps);

            // Generate reasoning prompt
            let prompt = self.build_reasoning_prompt(task, &steps);

            // Get reasoning from LLM
            let reasoning = llm.reason(&prompt, &steps).await?;

            // Check for completion phrases
            if self.should_complete(&reasoning) {
                final_answer = Some(self.extract_answer(&reasoning));
                completed = true;
                break;
            }

            // Parse action
            let action = llm.parse_action(&reasoning).await?;

            // Execute action
            let (observation, success) = self.execute_action(&action).await?;

            // Generate reflection if enabled
            let reflection = if self.config.enable_reflection {
                Some(
                    self.generate_reflection(&reasoning, &action, &observation, success)
                        .await?,
                )
            } else {
                None
            };

            // Record step
            let step = ReActStep {
                step_number: step_num,
                thought: reasoning,
                action: Some(action),
                observation: Some(observation),
                success,
                reflection,
                timestamp: chrono::Utc::now(),
            };

            steps.push(step.clone());

            // Update memory
            {
                let mut memory = self.memory.write().await;
                memory.push(step);
            }

            // Check if final answer
            if let Some(Action::FinalAnswer { answer }) =
                steps.last().and_then(|s| s.action.clone())
            {
                final_answer = Some(answer);
                completed = true;
                break;
            }
        }

        let execution_time = start_time.elapsed().as_secs_f64();

        info!(
            "ReAct execution completed in {:.2}s, steps: {}",
            execution_time,
            steps.len()
        );

        let steps_used = steps.len();
        Ok(ReActResult {
            task_id,
            answer: final_answer,
            steps,
            steps_used,
            completed,
            execution_time_secs: execution_time,
        })
    }

    /// Build reasoning prompt with context
    fn build_reasoning_prompt(&self, task: &str, steps: &[ReActStep]) -> String {
        let mut prompt = format!(
            "You are a helpful AI assistant that solves tasks by thinking step by step and taking \
             actions.\n\nTask: {}\n\nAvailable tools:\n",
            task
        );

        // Add tool descriptions (this would need to be implemented with proper tool
        // access) For now, using a simplified version
        prompt.push_str("\nThink step by step:\n");
        prompt.push_str("1. What is the current state?\n");
        prompt.push_str("2. What do I need to know or do?\n");
        prompt.push_str("3. What action should I take?\n\n");

        // Add previous steps
        if !steps.is_empty() {
            prompt.push_str("Previous steps:\n");
            for step in steps {
                prompt.push_str(&format!(
                    "Step {}: {}\nObservation: {}\n\n",
                    step.step_number,
                    step.thought,
                    step.observation.as_deref().unwrap_or("None")
                ));
            }
        }

        prompt.push_str("Now, provide your thought and action:");
        prompt
    }

    /// Execute an action
    async fn execute_action(&self, action: &Action) -> Result<(String, bool)> {
        match action {
            Action::ToolUse {
                tool_name,
                parameters,
            } => {
                let tools = self.tools.read().await;
                let tool = tools.get(tool_name).ok_or_else(|| {
                    AgentError::not_found(format!("Tool {} not found", tool_name))
                })?;

                let result = tool.execute(parameters.clone()).await?;

                let observation = if result.success {
                    format!(
                        "Tool '{}' executed successfully: {}",
                        tool_name, result.data
                    )
                } else {
                    format!(
                        "Tool '{}' failed: {}",
                        tool_name,
                        result.error.unwrap_or_default()
                    )
                };

                Ok((observation, result.success))
            }
            Action::FinalAnswer { answer } => Ok((format!("Final answer: {}", answer), true)),
            Action::RequestInfo { question } => {
                Ok((format!("Requesting information: {}", question), true))
            }
            Action::Delegate { agent_id, task } => Ok((
                format!("Delegating task '{}' to agent '{}'", task, agent_id),
                true,
            )),
        }
    }

    /// Check if reasoning indicates task completion
    fn should_complete(&self, reasoning: &str) -> bool {
        let reasoning_lower = reasoning.to_lowercase();
        self.config
            .stop_phrases
            .iter()
            .any(|phrase| reasoning_lower.contains(&phrase.to_lowercase()))
    }

    /// Extract final answer from reasoning
    fn extract_answer(&self, reasoning: &str) -> String {
        // Simple extraction - in practice would use more sophisticated parsing
        for phrase in &self.config.stop_phrases {
            if let Some(idx) = reasoning.to_lowercase().find(&phrase.to_lowercase()) {
                return reasoning[idx + phrase.len()..].trim().to_string();
            }
        }
        reasoning.to_string()
    }

    /// Generate reflection on a step
    async fn generate_reflection(
        &self,
        _thought: &str,
        action: &Action,
        observation: &str,
        success: bool,
    ) -> Result<String> {
        let reflection = if success {
            format!(
                "The action {:?} was successful. Observation: {}. I should continue building on \
                 this progress.",
                action, observation
            )
        } else {
            format!(
                "The action {:?} failed. Observation: {}. I should try a different approach or \
                 tool.",
                action, observation
            )
        };
        Ok(reflection)
    }

    /// Get execution history
    pub async fn get_history(&self) -> Vec<ReActStep> {
        self.memory.read().await.clone()
    }

    /// Clear memory
    pub async fn clear_memory(&self) {
        self.memory.write().await.clear();
    }
}

/// Common tools implementation
pub mod tools {
    use super::*;

    /// Calculator tool
    pub struct CalculatorTool;

    #[async_trait::async_trait]
    impl Tool for CalculatorTool {
        fn name(&self) -> &str {
            "calculator"
        }

        fn description(&self) -> &str {
            "Perform mathematical calculations. Parameters: expression (string)"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "expression": {
                        "type": "string",
                        "description": "Mathematical expression to evaluate"
                    }
                },
                "required": ["expression"]
            })
        }

        async fn execute(&self, params: HashMap<String, serde_json::Value>) -> Result<ToolResult> {
            let expression = params
                .get("expression")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AgentError::platform("Missing expression parameter"))?;

            // Simple evaluation (in production, use a proper math parser)
            match eval_expression(expression) {
                Ok(result) => ToolResult::success(result),
                Err(e) => Ok(ToolResult::failure(e)),
            }
        }
    }

    /// Search tool
    pub struct SearchTool {
        #[allow(dead_code)]
        client: reqwest::Client,
    }

    impl SearchTool {
        pub fn new() -> Self {
            Self {
                client: reqwest::Client::new(),
            }
        }
    }

    #[async_trait::async_trait]
    impl Tool for SearchTool {
        fn name(&self) -> &str {
            "search"
        }

        fn description(&self) -> &str {
            "Search for information. Parameters: query (string)"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query"
                    }
                },
                "required": ["query"]
            })
        }

        async fn execute(&self, params: HashMap<String, serde_json::Value>) -> Result<ToolResult> {
            let query = params
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AgentError::platform("Missing query parameter"))?;

            // Placeholder implementation
            ToolResult::success(format!("Search results for: {}", query))
        }
    }

    /// Simple expression evaluator
    fn eval_expression(expr: &str) -> std::result::Result<f64, String> {
        // Very basic implementation - use a proper parser in production
        expr.parse::<f64>()
            .map_err(|_| format!("Cannot evaluate expression: {}", expr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_react_config_default() {
        let config = ReActConfig::default();
        assert_eq!(config.max_steps, 10);
        assert!(config.enable_reflection);
        assert_eq!(config.max_tool_retries, 3);
    }

    #[test]
    fn test_tool_result_success() {
        let result = ToolResult::success(42).unwrap();
        assert!(result.success);
        assert_eq!(result.data, serde_json::json!(42));
        assert!(result.error.is_none());
    }

    #[test]
    fn test_tool_result_failure() {
        let result = ToolResult::failure("Something went wrong");
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[tokio::test]
    async fn test_react_agent_creation() {
        let config = ReActConfig::default();
        let agent = ReActAgent::new(config);

        // Register a tool
        agent.register_tool(Box::new(tools::CalculatorTool)).await;

        let tools = agent.tools.read().await;
        assert!(tools.contains_key("calculator"));
    }
}
