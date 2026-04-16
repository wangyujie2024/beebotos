//! Agent implementation
//!
//! Core Agent struct and task execution logic.
//! 
//! 🆕 PLANNING FIX: Integrated planning module for autonomous task planning and execution.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::device::{Device, DeviceAutomation, AppLifecycle};
use crate::error::AgentError;
use crate::planning::{
    ExecutionResult, Plan, PlanContext, PlanExecutor, PlanId, PlanStatus, PlanStep, PlanStrategy,
    PlanningEngine, RePlanner, StepType,
};
use crate::task::{Artifact, Task, TaskResult, TaskType};
use crate::{
    a2a, communication, events, mcp, queue, skills, state_manager, types, wallet, AgentConfig,
};

pub struct Agent {
    pub(crate) config: AgentConfig,
    pub(crate) a2a_client: Option<a2a::A2AClient>,
    pub(crate) mcp_manager: Option<mcp::MCPManager>,
    pub(crate) platform_manager: Option<communication::channel::ChannelManager>,
    pub(crate) queue_manager: Option<Arc<queue::QueueManager>>,
    pub(crate) skill_registry: Option<Arc<skills::SkillRegistry>>,
    pub(crate) llm_interface: Option<Arc<dyn communication::LLMCallInterface>>,
    // 🔒 P0 FIX: Wallet integration for on-chain transactions
    pub(crate) wallet: Option<Arc<wallet::AgentWallet>>,
    // 🟢 P1 FIX: 统一事件总线 - 复用 core::EventBus
    pub(crate) event_bus: Option<events::AgentEventBus>,
    // 🔒 P0 FIX: Kernel integration for WASM sandbox execution
    pub(crate) kernel: Option<Arc<beebotos_kernel::Kernel>>,
    // 🔒 P0 FIX: Agent state (from state_manager)
    pub(crate) state: state_manager::AgentState,
    // 🆕 PLANNING FIX: Planning module integration
    pub(crate) planning_engine: Option<Arc<PlanningEngine>>,
    pub(crate) plan_executor: Option<Arc<PlanExecutor>>,
    pub(crate) replanner: Option<Arc<dyn RePlanner>>,
    // 🆕 PLANNING FIX: Active plans tracking
    pub(crate) active_plans: Arc<RwLock<HashMap<PlanId, Plan>>>,
    // 🆕 DEVICE FIX: Device automation integration
    pub(crate) device: Option<Device>,
    // 🟢 P1 FIX: Memory system for long-term memory retrieval
    pub(crate) memory_system: Option<Arc<dyn crate::memory::MemorySearch>>,
}

impl Agent {
    pub fn new(config: AgentConfig) -> Self {
        Self {
            config,
            a2a_client: None,
            mcp_manager: None,
            platform_manager: None,
            queue_manager: None,
            skill_registry: None,
            llm_interface: None,
            state: state_manager::AgentState::Registered,
            wallet: None,    // 🔒 P0 FIX: Initialize wallet as None
            event_bus: None, // 🟢 P1 FIX: Initialize event bus as None
            kernel: None,    // 🔒 P0 FIX: Initialize kernel as None
            // 🆕 PLANNING FIX: Initialize planning components as None
            planning_engine: None,
            plan_executor: None,
            replanner: None,
            active_plans: Arc::new(RwLock::new(HashMap::new())),
            // 🆕 DEVICE FIX: Initialize device as None
            device: None,
            // 🟢 P1 FIX: Initialize memory system as None
            memory_system: None,
        }
    }

    pub fn with_a2a(mut self, client: a2a::A2AClient) -> Self {
        self.a2a_client = Some(client);
        self
    }

    pub fn with_mcp(mut self, manager: mcp::MCPManager) -> Self {
        self.mcp_manager = Some(manager);
        self
    }

    pub fn with_platforms(mut self, manager: communication::channel::ChannelManager) -> Self {
        self.platform_manager = Some(manager);
        self
    }

    pub fn with_queue_manager(mut self, manager: Arc<queue::QueueManager>) -> Self {
        self.queue_manager = Some(manager);
        self
    }

    pub fn with_skill_registry(mut self, registry: Arc<skills::SkillRegistry>) -> Self {
        self.skill_registry = Some(registry);
        self
    }

    pub fn with_llm_interface(
        mut self,
        interface: Arc<dyn communication::LLMCallInterface>,
    ) -> Self {
        self.llm_interface = Some(interface);
        self
    }

    /// 🔒 P0 FIX: Set wallet for on-chain transactions
    pub fn with_wallet(mut self, wallet: Arc<wallet::AgentWallet>) -> Self {
        self.wallet = Some(wallet);
        info!("Wallet configured for agent {}", self.config.id);
        self
    }

    /// 🔒 P0 FIX: Get wallet reference
    pub fn wallet(&self) -> Option<&Arc<wallet::AgentWallet>> {
        self.wallet.as_ref()
    }

    /// 🔒 P0 FIX: Check if agent has wallet configured
    pub fn has_wallet(&self) -> bool {
        self.wallet.is_some()
    }

    /// 🟢 P1 FIX: Set event bus for unified event system
    pub fn with_event_bus(mut self, event_bus: events::AgentEventBus) -> Self {
        self.event_bus = Some(event_bus);
        info!("Event bus configured for agent {}", self.config.id);
        self
    }

    /// 🟢 P1 FIX: Get event bus reference
    pub fn event_bus(&self) -> Option<&events::AgentEventBus> {
        self.event_bus.as_ref()
    }

    /// 🟢 P1 FIX: Check if agent has event bus configured
    pub fn has_event_bus(&self) -> bool {
        self.event_bus.is_some()
    }

    /// 🔒 P0 FIX: Set kernel for WASM sandbox execution
    pub fn with_kernel(mut self, kernel: Arc<beebotos_kernel::Kernel>) -> Self {
        self.kernel = Some(kernel);
        info!("Kernel configured for agent {}", self.config.id);
        self
    }

    /// 🔒 P0 FIX: Get kernel reference
    pub fn kernel(&self) -> Option<&Arc<beebotos_kernel::Kernel>> {
        self.kernel.as_ref()
    }

    /// 🔒 P0 FIX: Check if agent has kernel configured
    pub fn has_kernel(&self) -> bool {
        self.kernel.is_some()
    }

    /// 🆕 PLANNING FIX: Set planning engine for autonomous planning
    pub fn with_planning_engine(mut self, engine: Arc<PlanningEngine>) -> Self {
        self.planning_engine = Some(engine);
        info!("Planning engine configured for agent {}", self.config.id);
        self
    }

    /// 🆕 PLANNING FIX: Get planning engine reference
    pub fn planning_engine(&self) -> Option<&Arc<PlanningEngine>> {
        self.planning_engine.as_ref()
    }

    /// 🆕 PLANNING FIX: Check if agent has planning engine configured
    pub fn has_planning_engine(&self) -> bool {
        self.planning_engine.is_some()
    }

    /// 🆕 PLANNING FIX: Set plan executor
    pub fn with_plan_executor(mut self, executor: Arc<PlanExecutor>) -> Self {
        self.plan_executor = Some(executor);
        info!("Plan executor configured for agent {}", self.config.id);
        self
    }

    /// 🆕 PLANNING FIX: Get plan executor reference
    pub fn plan_executor(&self) -> Option<&Arc<PlanExecutor>> {
        self.plan_executor.as_ref()
    }

    /// 🆕 PLANNING FIX: Check if agent has plan executor configured
    pub fn has_plan_executor(&self) -> bool {
        self.plan_executor.is_some()
    }

    /// 🆕 PLANNING FIX: Set replanner for dynamic replanning
    pub fn with_replanner(mut self, replanner: Arc<dyn RePlanner>) -> Self {
        self.replanner = Some(replanner);
        info!("RePlanner configured for agent {}", self.config.id);
        self
    }

    /// 🆕 PLANNING FIX: Get replanner reference
    pub fn replanner(&self) -> Option<&Arc<dyn RePlanner>> {
        self.replanner.as_ref()
    }

    /// 🆕 PLANNING FIX: Check if agent has replanner configured
    pub fn has_replanner(&self) -> bool {
        self.replanner.is_some()
    }

    /// 🆕 PLANNING FIX: Check if planning module is fully configured
    pub fn is_planning_ready(&self) -> bool {
        self.has_planning_engine() && self.has_plan_executor()
    }

    // 🆕 DEVICE FIX: Device automation methods

    /// Set device for automation
    pub fn with_device(mut self, device: Device) -> Self {
        self.device = Some(device);
        info!("Device configured for agent {}", self.config.id);
        self
    }

    /// Get device reference
    pub fn device(&self) -> Option<&Device> {
        self.device.as_ref()
    }

    /// Check if agent has device configured
    pub fn has_device(&self) -> bool {
        self.device.is_some()
    }

    /// 🟢 P1 FIX: Set memory system for long-term memory retrieval
    pub fn with_memory_system(mut self, memory: Arc<dyn crate::memory::MemorySearch>) -> Self {
        self.memory_system = Some(memory);
        info!("Memory system configured for agent {}", self.config.id);
        self
    }

    /// 🟢 P1 FIX: Get memory system reference
    pub fn memory_system(&self) -> Option<&Arc<dyn crate::memory::MemorySearch>> {
        self.memory_system.as_ref()
    }

    /// 🟢 P1 FIX: Check if agent has memory system configured
    pub fn has_memory_system(&self) -> bool {
        self.memory_system.is_some()
    }

    /// Connect to device (if configured)
    pub async fn connect_device(&self) -> crate::error::Result<()> {
        if let Some(ref device) = self.device {
            match device {
                Device::Node(d) => d.connect().await,
                Device::Ios(d) => d.connect().await,
                Device::Android(d) => d.connect().await,
            }
        } else {
            Err(AgentError::InvalidConfig("No device configured".to_string()))
        }
    }

    /// Disconnect from device
    pub async fn disconnect_device(&self) -> crate::error::Result<()> {
        if let Some(ref device) = self.device {
            match device {
                Device::Node(d) => d.disconnect().await,
                Device::Ios(d) => d.disconnect().await,
                Device::Android(d) => d.disconnect().await,
            }
        } else {
            Ok(())
        }
    }

    pub async fn initialize(&mut self) -> Result<(), AgentError> {
        if let Some(mcp) = self.mcp_manager.as_mut() {
            mcp.initialize_all().await?;
        }

        if let Some(platforms) = self.platform_manager.as_mut() {
            platforms.connect_all().await;
        }

        self.state = state_manager::AgentState::Idle;
        Ok(())
    }

    pub fn get_state(&self) -> &state_manager::AgentState {
        &self.state
    }

    pub fn get_config(&self) -> &AgentConfig {
        &self.config
    }

    pub async fn execute_task(&mut self, task: Task) -> Result<TaskResult, AgentError> {
        self.state = state_manager::AgentState::Working {
            task_id: task.id.clone(),
        };

        let result = self.process_task(task).await;

        self.state = state_manager::AgentState::Idle;
        result
    }

    /// 🟢 P1 FIX: 批量执行任务
    pub async fn execute_batch(&mut self, tasks: Vec<Task>) -> Vec<Result<TaskResult, AgentError>> {
        if tasks.is_empty() {
            return vec![];
        }

        info!("Starting batch execution of {} tasks", tasks.len());
        let start_time = std::time::Instant::now();

        let this = &*self;
        let futures = tasks.into_iter().map(|task| this.process_task(task));
        let results = futures::future::join_all(futures).await;

        let elapsed = start_time.elapsed();
        info!(
            "Batch execution completed: {} tasks in {:?}",
            results.len(),
            elapsed
        );

        results
    }

    /// Process a task with full implementation
    /// 
    /// 🆕 PLANNING FIX: Enhanced with automatic complexity detection and planning integration
    async fn process_task(&self, task: Task) -> Result<TaskResult, AgentError> {
        info!("Processing task {} of type {}", task.id, task.task_type);

        let start_time = std::time::Instant::now();

        // 🆕 PLANNING FIX: Check if this is a planning-related task or needs planning
        // Clone task ID before moving task
        let task_id = task.id.clone();
        
        let result = match &task.task_type {
            TaskType::LlmChat => self.handle_llm_task(&task).await,
            TaskType::SkillExecution => self.handle_skill_task(&task).await,
            TaskType::McpTool => self.handle_mcp_task(&task).await,
            TaskType::FileProcessing => self.handle_file_task(&task).await,
            TaskType::A2aSend => self.handle_a2a_task(&task).await,
            TaskType::ChainTransaction => self.handle_chain_transaction_task(&task).await,
            // 🆕 PLANNING FIX: Handle planning-specific task types
            TaskType::PlanCreation => self.handle_plan_creation_task(&task).await,
            TaskType::PlanExecution => self.handle_plan_execution_task(&task).await,
            TaskType::PlanAdaptation => self.handle_plan_adaptation_task(&task).await,
            // 🆕 DEVICE FIX: Handle device automation task types
            TaskType::DeviceAutomation => self.handle_device_automation_task(&task).await,
            TaskType::AppLifecycle => self.handle_app_lifecycle_task(&task).await,
            TaskType::Custom(type_name) => {
                // 🆕 PLANNING FIX: Check if complex task needs planning
                if self.is_planning_ready() && self.should_use_planning(&task).await {
                    self.execute_with_planning(task).await
                } else {
                    warn!("Unknown custom task type: {}", type_name);
                    Err(AgentError::InvalidConfig(format!(
                        "Unsupported task type: {}",
                        type_name
                    )))
                }
            }
        };

        let execution_time_ms = start_time.elapsed().as_millis() as u64;

        match result {
            Ok((output, artifacts)) => {
                info!(
                    "Task {} completed successfully in {}ms",
                    task_id, execution_time_ms
                );
                Ok(TaskResult {
                    task_id,
                    success: true,
                    output,
                    artifacts,
                    execution_time_ms,
                })
            }
            Err(e) => {
                error!(
                    "Task {} failed after {}ms: {}",
                    task_id, execution_time_ms, e
                );
                Err(e)
            }
        }
    }

    async fn handle_llm_task(&self, task: &Task) -> Result<(String, Vec<Artifact>), AgentError> {
        // 🆕 PLANNING FIX: 基于实际消息内容判断复杂度，复杂任务使用 planning 执行
        let message_text = serde_json::from_str::<serde_json::Value>(&task.input)
            .ok()
            .and_then(|json| json.get("message").and_then(|m| m.as_str()).map(|s| s.to_string()))
            .unwrap_or_else(|| task.input.clone());

        let is_complex = message_text.chars().count() > 200
            || task.parameters.contains_key("multi_step")
            || task.parameters.contains_key("dependencies")
            || task.parameters.contains_key("plan")
            || message_text.contains("计划")
            || message_text.contains("步骤")
            || message_text.contains("安排")
            || message_text.contains("规划");

        if self.is_planning_ready() && is_complex {
            info!("🧠 Complex LLM task detected (message length: {}), using planning for task {}", message_text.len(), task.id);
            return self.execute_with_planning(task.clone()).await;
        }

        let llm = self
            .llm_interface
            .as_ref()
            .ok_or_else(|| AgentError::InvalidConfig("LLM interface not configured".into()))?;

        // Parse structured input JSON to extract current message and context metadata
        let (input_text, extra_params, image_urls, history) = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&task.input) {
            let message = json.get("message")
                .and_then(|m| m.as_str())
                .unwrap_or(&task.input)
                .to_string();
            let mut params = task.parameters.clone();
            if let Some(platform) = json.get("platform").and_then(|p| p.as_str()) {
                params.insert("platform".to_string(), platform.to_string());
            }
            if let Some(channel_id) = json.get("channel_id").and_then(|c| c.as_str()) {
                params.insert("channel_id".to_string(), channel_id.to_string());
            }
            if let Some(user_id) = json.get("user_id").and_then(|u| u.as_str()) {
                params.insert("user_id".to_string(), user_id.to_string());
            }
            if let Some(session_id) = json.get("session_id").and_then(|s| s.as_str()) {
                params.insert("session_id".to_string(), session_id.to_string());
            }
            let images: Vec<String> = json.get("images")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            let history: Vec<(String, String)> = json.get("history")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| {
                            let role = v.get("role").and_then(|r| r.as_str())?;
                            let content = v.get("content").and_then(|c| c.as_str())?;
                            Some((role.to_string(), content.to_string()))
                        })
                        .collect()
                })
                .unwrap_or_default();
            (message, params, images, history)
        } else {
            (task.input.clone(), task.parameters.clone(), Vec::new(), Vec::new())
        };

        let mut metadata = std::collections::HashMap::new();
        if !image_urls.is_empty() {
            metadata.insert("image_urls".to_string(), serde_json::to_string(&image_urls).unwrap_or_default());
        }

        // Build message list with memory context, history, and current message
        let mut messages: Vec<communication::Message> = Vec::new();

        // 🟢 P1 FIX: Retrieve long-term memory and inject as context
        if let Some(ref memory) = self.memory_system {
            let query_parts: Vec<String> = std::iter::once(input_text.clone())
                .chain(history.iter().map(|(_, content)| content.clone()))
                .collect();
            let query = query_parts.join(" ");

            match memory.search(&query).await {
                Ok(results) => {
                    info!("Agent {} memory search returned {} results for query '{}..'", self.config.id, results.len(), query.chars().take(40).collect::<String>());
                    let input_lower = input_text.to_lowercase();
                    let memory_context: String = results.iter()
                        .filter(|r| {
                            // Skip memories that are essentially the current query being repeated
                            let is_self_referential = r.content.to_lowercase().contains(&input_lower);
                            if is_self_referential {
                                info!("Filtering out self-referential memory: {}", r.content.chars().take(40).collect::<String>());
                            }
                            !is_self_referential
                        })
                        .take(5)
                        .map(|r| format!("- {}", r.content))
                        .collect::<Vec<_>>()
                        .join("\n");
                    if !memory_context.is_empty() {
                        info!("Injecting memory context ({} chars) into agent LLM prompt", memory_context.len());
                        messages.push(communication::Message::new(
                            uuid::Uuid::new_v4(),
                            communication::PlatformType::Custom,
                            format!("[系统提示：以下是该用户的历史记忆，回答时必须结合这些信息]\n{}", memory_context),
                        ));
                    }
                }
                Err(e) => {
                    warn!("Memory search failed for agent {}: {}", self.config.id, e);
                }
            }
        }

        // Add history messages with role prefixes for clarity
        for (role, content) in history {
            let prefix = match role.as_str() {
                "user" => "用户",
                "assistant" => "助手",
                "system" => "系统",
                _ => &role,
            };
            messages.push(communication::Message::new(
                uuid::Uuid::new_v4(),
                communication::PlatformType::Custom,
                format!("{}: {}", prefix, content),
            ));
        }

        // Add current user message
        messages.push(communication::Message::with_metadata(
            uuid::Uuid::new_v4(),
            communication::PlatformType::Custom,
            format!("用户: {}", input_text),
            metadata,
        ));

        let response = llm
            .call_llm(messages, Some(extra_params))
            .await
            .map_err(|e| AgentError::Execution(format!("LLM call failed: {}", e)))?;

        Ok((response, vec![]))
    }

    async fn handle_skill_task(&self, task: &Task) -> Result<(String, Vec<Artifact>), AgentError> {
        let skill_name = task
            .parameters
            .get("skill")
            .ok_or_else(|| AgentError::InvalidConfig("Missing 'skill' parameter".into()))?;

        let registry = self
            .skill_registry
            .as_ref()
            .ok_or_else(|| AgentError::InvalidConfig("Skill registry not configured".into()))?;

        let registered_skill = registry
            .get(skill_name).await
            .ok_or_else(|| AgentError::SkillNotFound(skill_name.clone()))?;

        let context = skills::executor::SkillContext {
            input: task.input.clone(),
            parameters: task.parameters.clone(),
        };

        let (result, execution_time_ms) = if let Some(kernel) = self.kernel.as_ref() {
            if let Some(engine) = kernel.wasm_engine() {
                info!("Executing skill '{}' in kernel WASM sandbox", skill_name);
                let start_time = std::time::Instant::now();

                let wasm_bytes = tokio::fs::read(&registered_skill.skill.wasm_path)
                    .await
                    .map_err(|e| {
                        AgentError::Execution(format!("Failed to read WASM file: {}", e))
                    })?;

                let module = engine
                    .compile_cached(&registered_skill.skill.id, &wasm_bytes)
                    .map_err(|e| AgentError::Execution(format!("Failed to compile WASM: {}", e)))?;

                let mut instance = engine
                    .instantiate_with_host(&module, &self.config.id)
                    .map_err(|e| {
                        AgentError::Execution(format!("Failed to instantiate WASM: {}", e))
                    })?;

                let input_bytes = context.input.as_bytes();
                instance.write_memory(0, input_bytes).map_err(|e| {
                    AgentError::Execution(format!("Failed to write WASM memory: {}", e))
                })?;

                const _OUTPUT_OFFSET: usize = 65536;
                const MAX_OUTPUT_SIZE: usize = 65536;

                let output_ptr = instance
                    .call_typed::<(i32, i32), i32>(
                        &registered_skill.skill.manifest.entry_point,
                        (0i32, input_bytes.len() as i32),
                    )
                    .map_err(|e| AgentError::Execution(format!("WASM execution failed: {}", e)))?;

                let output_addr = output_ptr as usize;
                let len_bytes = instance.read_memory(output_addr, 4).map_err(|e| {
                    AgentError::Execution(format!("Failed to read output length: {}", e))
                })?;
                let output_len =
                    u32::from_le_bytes([len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3]])
                        as usize;

                if output_len > MAX_OUTPUT_SIZE {
                    return Err(AgentError::Execution(format!(
                        "WASM output too large: {} bytes (max {})",
                        output_len, MAX_OUTPUT_SIZE
                    )));
                }

                let output_bytes = instance
                    .read_memory(output_addr + 4, output_len)
                    .map_err(|e| AgentError::Execution(format!("Failed to read output: {}", e)))?;
                let output = String::from_utf8(output_bytes).map_err(|e| {
                    AgentError::Execution(format!("WASM output is not valid UTF-8: {}", e))
                })?;

                let execution_time_ms = start_time.elapsed().as_millis() as u64;
                info!("Skill '{}' executed in {}ms", skill_name, execution_time_ms);

                (
                    skills::executor::SkillExecutionResult {
                        task_id: registered_skill.skill.id.clone(),
                        success: true,
                        output,
                        execution_time_ms,
                    },
                    execution_time_ms,
                )
            } else {
                return Err(AgentError::Execution(
                    "WASM runtime not enabled in kernel".into(),
                ));
            }
        } else {
            let start_time = std::time::Instant::now();
            let executor = skills::SkillExecutor::new().map_err(|e| {
                AgentError::Execution(format!("Failed to create skill executor: {}", e))
            })?;

            let result = executor
                .execute(&registered_skill.skill, context)
                .await
                .map_err(|e| AgentError::Execution(format!("Skill execution failed: {}", e)))?;

            let execution_time_ms = start_time.elapsed().as_millis() as u64;
            (result, execution_time_ms)
        };

        let _ = registry.record_usage(skill_name).await;

        let artifacts = if !result.output.is_empty() {
            vec![Artifact {
                id: uuid::Uuid::new_v4().to_string(),
                artifact_type: "skill_output".to_string(),
                content: result.output.clone().into_bytes(),
                mime_type: "text/plain".to_string(),
            }]
        } else {
            vec![]
        };

        Ok((
            format!(
                "Skill '{}' executed successfully in {}ms",
                skill_name, execution_time_ms
            ),
            artifacts,
        ))
    }

    async fn handle_mcp_task(&self, task: &Task) -> Result<(String, Vec<Artifact>), AgentError> {
        let mcp = self
            .mcp_manager
            .as_ref()
            .ok_or_else(|| AgentError::InvalidConfig("MCP manager not configured".into()))?;

        let tool_name = task
            .parameters
            .get("tool")
            .ok_or_else(|| AgentError::InvalidConfig("Missing 'tool' parameter".into()))?;

        let client = mcp
            .get_client("default")
            .await
            .ok_or_else(|| AgentError::InvalidConfig("MCP client not found".into()))?;

        let arguments: Option<serde_json::Map<String, serde_json::Value>> = if task.input.is_empty()
        {
            None
        } else {
            serde_json::from_str(&task.input)
                .map_err(|e| AgentError::InvalidConfig(format!("Invalid tool arguments: {}", e)))?
        };

        let result = client
            .call_tool(tool_name, arguments)
            .await
            .map_err(|e| AgentError::Execution(format!("MCP tool call failed: {}", e)))?;

        if result.is_error {
            return Err(AgentError::Execution("MCP tool returned an error".into()));
        }

        let output = result
            .content
            .iter()
            .map(|c| format!("{:?}", c))
            .collect::<Vec<_>>()
            .join("\n");

        Ok((output, vec![]))
    }

    async fn handle_file_task(&self, task: &Task) -> Result<(String, Vec<Artifact>), AgentError> {
        let file_path = task
            .parameters
            .get("file_path")
            .ok_or_else(|| AgentError::InvalidConfig("Missing 'file_path' parameter".into()))?;

        let content = tokio::fs::read_to_string(file_path)
            .await
            .map_err(|e| AgentError::Execution(format!("Failed to read file: {}", e)))?;

        let output = if task.input.is_empty() {
            format!(
                "File content ({} bytes): {}",
                content.len(),
                &content[..content.len().min(100)]
            )
        } else {
            let llm = self
                .llm_interface
                .as_ref()
                .ok_or_else(|| AgentError::InvalidConfig("LLM interface not configured".into()))?;

            let prompt = format!("{}\n\nFile content:\n{}", task.input, content);
            let messages = vec![communication::Message::new(
                uuid::Uuid::new_v4(),
                communication::PlatformType::Custom,
                prompt,
            )];

            llm.call_llm(messages, None)
                .await
                .map_err(|e| AgentError::Execution(format!("LLM processing failed: {}", e)))?
        };

        Ok((output, vec![]))
    }

    async fn handle_a2a_task(&self, task: &Task) -> Result<(String, Vec<Artifact>), AgentError> {
        let a2a = self
            .a2a_client
            .as_ref()
            .ok_or_else(|| AgentError::InvalidConfig("A2A client not configured".into()))?;

        let target_agent = task
            .parameters
            .get("target_agent")
            .ok_or_else(|| AgentError::InvalidConfig("Missing 'target_agent' parameter".into()))?;

        let mut params = HashMap::new();
        params.insert(
            "content".to_string(),
            serde_json::Value::String(task.input.clone()),
        );

        let payload = a2a::message::MessagePayload::Request {
            action: "send_message".to_string(),
            params,
        };

        let message = a2a::message::A2AMessage::new(
            a2a::message::MessageType::Request,
            types::AgentId::from_string(&self.config.id),
            Some(types::AgentId::from_string(target_agent)),
            payload,
        );

        let _response = a2a
            .send_message(message, target_agent)
            .await
            .map_err(|e| AgentError::A2A(format!("Failed to send A2A message: {}", e)))?;

        let output = format!("A2A message sent to {}. Response received.", target_agent);

        Ok((output, vec![]))
    }

    async fn handle_chain_transaction_task(
        &self,
        task: &Task,
    ) -> Result<(String, Vec<Artifact>), AgentError> {
        let wallet = self.wallet.as_ref().ok_or_else(|| {
            AgentError::InvalidConfig(
                "Wallet not configured. Use Agent::with_wallet() to enable chain transactions."
                    .into(),
            )
        })?;

        let to = task.parameters.get("to").ok_or_else(|| {
            AgentError::InvalidConfig("Missing 'to' parameter for chain transaction".into())
        })?;

        let value = task
            .parameters
            .get("value")
            .and_then(|v| v.parse::<u128>().ok())
            .unwrap_or(0);

        let data = task.parameters.get("data").cloned().unwrap_or_default();

        info!("Executing chain transaction: to={}, value={}", to, value);

        let tx_hash = wallet
            .send_transaction(
                to.parse()
                    .map_err(|_| AgentError::InvalidConfig("Invalid 'to' address".into()))?,
                value,
                if data.is_empty() {
                    None
                } else {
                    Some(data.into_bytes())
                },
            )
            .await
            .map_err(|e| AgentError::Execution(format!("Chain transaction failed: {}", e)))?;

        let output = format!("Transaction sent successfully. Hash: {:?}", tx_hash);

        let artifact = Artifact {
            id: uuid::Uuid::new_v4().to_string(),
            artifact_type: "transaction_receipt".to_string(),
            content: serde_json::json!({
                "tx_hash": format!("{:?}", tx_hash),
                "to": to,
                "value": value,
            })
            .to_string()
            .into_bytes(),
            mime_type: "application/json".to_string(),
        };

        Ok((output, vec![artifact]))
    }

    pub async fn shutdown(&mut self) -> Result<(), AgentError> {
        info!("Agent {} initiating graceful shutdown", self.config.id);
        self.state = state_manager::AgentState::ShuttingDown;

        if let Some(queue) = self.queue_manager.take() {
            info!("Shutting down queue manager...");
            queue.shutdown().await;
        }

        if let Some(platforms) = self.platform_manager.take() {
            info!("Disconnecting from platforms...");
            let results = platforms.disconnect_all().await;
            for (_platform, result) in results {
                match result {
                    Ok(()) => info!("Platform disconnected successfully"),
                    Err(e) => warn!("Error disconnecting platform: {}", e),
                }
            }
        }

        if let Some(mcp) = self.mcp_manager.take() {
            info!("Closing MCP connections...");
            mcp.close_all().await;
        }

        self.a2a_client = None;
        self.skill_registry = None;
        self.llm_interface = None;

        info!("Agent {} shutdown complete", self.config.id);
        Ok(())
    }

    // ============================================================================
    // 🆕 PLANNING FIX: Planning Module Integration Methods
    // ============================================================================

    /// Analyze task complexity to determine execution strategy
    pub async fn analyze_task_complexity(&self, task: &Task) -> TaskComplexity {
        // Heuristic rules for complexity detection
        let is_complex = 
            task.input.len() > 200 ||                              // Long description
            task.parameters.contains_key("multi_step") ||          // Explicit multi-step flag
            task.parameters.contains_key("dependencies") ||        // Has dependencies
            task.parameters.contains_key("plan") ||                // Explicit planning request
            matches!(task.task_type, 
                TaskType::PlanCreation | 
                TaskType::PlanExecution |
                TaskType::PlanAdaptation
            );

        if is_complex {
            TaskComplexity::Complex
        } else {
            TaskComplexity::Simple
        }
    }

    /// Determine if task should use planning
    pub async fn should_use_planning(&self, task: &Task) -> bool {
        if !self.is_planning_ready() {
            return false;
        }

        match self.analyze_task_complexity(task).await {
            TaskComplexity::Complex => true,
            TaskComplexity::Simple => {
                // Check for explicit planning override
                task.parameters.get("use_planning")
                    .map(|v| v == "true" || v == "1")
                    .unwrap_or(false)
            }
        }
    }

    /// Execute task using planning
    pub async fn execute_with_planning(&self, task: Task) -> Result<(String, Vec<Artifact>), AgentError> {
        let planning_engine = self.planning_engine.as_ref().ok_or_else(|| {
            AgentError::InvalidConfig("Planning engine not configured".into())
        })?;

        info!("Using planning for task: {}", task.id);

        // 1. Create plan context
        let context = self.create_plan_context(&task).await?;

        // 2. Determine planning strategy
        let strategy = self.select_plan_strategy(&task);

        // 3. Create plan using the actual message text as goal
        let goal = serde_json::from_str::<serde_json::Value>(&task.input)
            .ok()
            .and_then(|json| json.get("message").and_then(|m| m.as_str()).map(|s| s.to_string()))
            .unwrap_or_else(|| task.input.clone());
        let plan = planning_engine
            .create_plan(&goal, &context, Some(strategy))
            .await
            .map_err(|e| AgentError::Planning(format!("Failed to create plan: {}", e)))?;

        info!("Created plan {} with {} steps for task {}", plan.id, plan.steps.len(), task.id);

        // 4. Store active plan
        {
            let mut active = self.active_plans.write().await;
            active.insert(plan.id.clone(), plan.clone());
        }

        // 5. Execute plan
        let result = self.execute_plan_internal(&plan).await;

        // 6. Cleanup
        {
            let mut active = self.active_plans.write().await;
            active.remove(&plan.id);
        }

        match result {
            Ok(exec_result) => {
                if exec_result.success {
                    let output = exec_result.data
                        .and_then(|d| d.get("output").cloned())
                        .and_then(|o| o.as_str().map(|s| s.to_string()))
                        .unwrap_or_else(|| "Plan executed successfully".to_string());
                    
                    Ok((output, vec![]))
                } else {
                    Err(AgentError::Execution(
                        exec_result.error.unwrap_or_else(|| "Plan execution failed".to_string())
                    ))
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Create plan context from task
    async fn create_plan_context(&self, task: &Task) -> Result<PlanContext, AgentError> {
        let mut context = PlanContext::new(&self.config.id);
        
        // Add available tools
        let tools = self.get_available_tools().await;
        context.available_tools = tools;

        // Add session info if present
        if let Some(session_id) = task.parameters.get("session_id") {
            context.session_id = Some(session_id.clone());
        }

        // Add constraints from task parameters
        if let Some(constraints) = task.parameters.get("constraints") {
            context.constraints = constraints.split(',').map(|s| s.trim().to_string()).collect();
        }

        Ok(context)
    }

    /// Get available tools for planning
    async fn get_available_tools(&self) -> Vec<String> {
        let mut tools = vec![];

        // Add LLM if available
        if self.llm_interface.is_some() {
            tools.push("llm".to_string());
        }

        // Add skills
        if self.skill_registry.is_some() {
            tools.push("skill".to_string());
        }

        // Add MCP tools
        if self.mcp_manager.is_some() {
            tools.push("mcp".to_string());
        }

        tools
    }

    /// Select planning strategy based on task
    pub fn select_plan_strategy(&self, task: &Task) -> PlanStrategy {
        if let Some(strategy_str) = task.parameters.get("strategy") {
            match strategy_str.as_str() {
                "react" => PlanStrategy::ReAct,
                "chain_of_thought" | "cot" => PlanStrategy::ChainOfThought,
                "goal_based" => PlanStrategy::GoalBased,
                "hybrid" => PlanStrategy::Hybrid,
                _ => PlanStrategy::Hybrid,
            }
        } else {
            PlanStrategy::Hybrid
        }
    }

    /// Execute plan with step handlers
    /// 
    /// 🆕 OPTIMIZATION: Supports both sequential and parallel execution
    /// 🆕 OPTIMIZATION: Respects step dependencies when executing in parallel
    async fn execute_plan_internal(&self, plan: &Plan) -> Result<ExecutionResult, AgentError> {
        // Check if parallel execution is enabled via plan metadata
        let enable_parallel = plan.metadata.get("enable_parallel")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        
        let max_concurrency = plan.metadata.get("max_concurrency")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;

        if enable_parallel && plan.dependencies.is_empty() {
            // Parallel execution for independent steps
            self.execute_plan_parallel(plan, max_concurrency).await
        } else {
            // Sequential execution (or dependency-aware)
            self.execute_plan_sequential_or_dependency_aware(plan).await
        }
    }
    
    /// Execute plan steps in parallel
    /// 
    /// 🆕 OPTIMIZATION: Uses futures::future::join_all for concurrent execution
    async fn execute_plan_parallel(
        &self, 
        plan: &Plan,
        max_concurrency: usize,
    ) -> Result<ExecutionResult, AgentError> {
        let start_time = std::time::Instant::now();
        use tokio::sync::Semaphore;
        use futures::future::join_all;
        
        let semaphore = Arc::new(Semaphore::new(max_concurrency));
        
        // Create futures for all steps
        let mut futures = Vec::new();
        for (step_idx, step) in plan.steps.iter().enumerate() {
            let step = step.clone();
            let semaphore = semaphore.clone();
            
            futures.push(async move {
                let _permit = semaphore.acquire().await.unwrap();
                let result = self.execute_step_by_type(&step).await;
                (step_idx, result)
            });
        }
        
        // Execute all futures concurrently
        let results = join_all(futures).await;
        
        // Check if all succeeded
        let all_success = results.iter().all(|(_, r)| matches!(r, Ok(r) if r.success));
        let any_failed = results.iter().any(|(_, r)| matches!(r, Ok(r) if !r.success));
        
        // Collect outputs from successful steps, and find first failure if needed
        let mut final_output = "Plan executed successfully".to_string();
        for (_, result) in &results {
            match result {
                Ok(exec_result) => {
                    if exec_result.success {
                        if let Some(data) = &exec_result.data {
                            if let Some(output) = data.get("output").and_then(|o| o.as_str()) {
                                final_output = output.to_string();
                            }
                        }
                    } else if any_failed && !self.should_continue_on_failure(plan) {
                        return Ok(exec_result.clone());
                    }
                }
                Err(_) => {}
            }
        }

        Ok(ExecutionResult {
            success: all_success,
            data: Some(serde_json::json!({ "output": final_output })),
            error: None,
            duration_ms: start_time.elapsed().as_millis() as u64,
            attempts: 1,
        })
    }
    
    /// Execute plan steps sequentially or with dependency awareness
    /// 
    /// 🆕 OPTIMIZATION: Supports dependency-aware execution
    async fn execute_plan_sequential_or_dependency_aware(
        &self, 
        plan: &Plan,
    ) -> Result<ExecutionResult, AgentError> {
        let start_time = std::time::Instant::now();
        
        // If no dependencies, execute sequentially
        if plan.dependencies.is_empty() {
            let mut final_output = "Plan executed successfully".to_string();
            for (step_idx, step) in plan.steps.iter().enumerate() {
                info!("Executing plan {} step {}: {}", plan.id, step_idx, step.description);

                let step_result = self.execute_step_by_type(step).await;

                match step_result {
                    Ok(result) => {
                        if result.success {
                            if let Some(data) = &result.data {
                                if let Some(output) = data.get("output").and_then(|o| o.as_str()) {
                                    final_output = output.to_string();
                                }
                            }
                        } else if !self.should_continue_on_failure(plan) {
                            return Ok(result);
                        }
                    }
                    Err(e) => {
                        error!("Step {} execution failed: {}", step_idx, e);
                        return Err(e);
                    }
                }
            }

            Ok(ExecutionResult {
                success: true,
                data: Some(serde_json::json!({ "output": final_output })),
                error: None,
                duration_ms: start_time.elapsed().as_millis() as u64,
                attempts: 1,
            })
        } else {
            // Dependency-aware execution
            return self.execute_plan_with_dependencies(plan).await;
        }
    }
    
    /// Execute step based on its type
    async fn execute_step_by_type(&self, step: &PlanStep) -> Result<ExecutionResult, AgentError> {
        match step.step_type {
            StepType::Action => self.execute_action_step(step).await,
            StepType::Decision => self.execute_decision_step(step).await,
            StepType::Reasoning => self.execute_reasoning_step(step).await,
            StepType::Information => self.execute_information_step(step).await,
            StepType::Validation => self.execute_validation_step(step).await,
        }
    }
    
    /// Execute plan with dependency awareness
    /// 
    /// 🆕 OPTIMIZATION: Executes steps in waves based on dependencies
    async fn execute_plan_with_dependencies(&self, plan: &Plan) -> Result<ExecutionResult, AgentError> {
        let start_time = std::time::Instant::now();

        
        let mut completed: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let total_steps = plan.steps.len();
        let mut last_output = "Plan executed successfully".to_string();
        
        while completed.len() < total_steps {
            // Find steps that are ready (all dependencies completed)
            let ready: Vec<usize> = plan.steps.iter().enumerate()
                .filter(|(i, _)| !completed.contains(i))
                .filter(|(i, _)| {
                    plan.dependencies.get(i)
                        .map(|deps| deps.iter().all(|d| completed.contains(d)))
                        .unwrap_or(true)
                })
                .map(|(i, _)| i)
                .collect();
            
            if ready.is_empty() {
                // Deadlock detected
                return Err(AgentError::Planning(
                    "Deadlock detected in plan dependencies".to_string()
                ));
            }
            
            // Execute ready steps in parallel using join_all
            use futures::future::join_all;
            
            let futures: Vec<_> = ready.into_iter().map(|step_idx| {
                let step = plan.steps[step_idx].clone();
                async move {
                    let result = self.execute_step_by_type(&step).await;
                    (step_idx, result)
                }
            }).collect();
            
            let results = join_all(futures).await;
            
            // Process results
            for (step_idx, result) in results {
                match result {
                    Ok(exec_result) => {
                        if exec_result.success {
                            completed.insert(step_idx);
                            if let Some(data) = &exec_result.data {
                                if let Some(output) = data.get("output").and_then(|o| o.as_str()) {
                                    last_output = output.to_string();
                                }
                            }
                        } else if !self.should_continue_on_failure(plan) {
                            return Ok(exec_result);
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
        }
        
        Ok(ExecutionResult {
            success: true,
            data: Some(serde_json::json!({ "output": last_output })),
            error: None,
            duration_ms: start_time.elapsed().as_millis() as u64,
            attempts: 1,
        })
    }

    /// Execute action step
    async fn execute_action_step(&self, step: &PlanStep) -> Result<ExecutionResult, AgentError> {
        // Default implementation uses LLM if available
        if let Some(llm) = &self.llm_interface {
            let messages = vec![communication::Message::new(
                uuid::Uuid::new_v4(),
                communication::PlatformType::Custom,
                step.description.clone(),
            )];

            let start_time = std::time::Instant::now();
            
            match llm.call_llm(messages, None).await {
                Ok(response) => Ok(ExecutionResult {
                    success: true,
                    data: Some(serde_json::json!({ "output": response })),
                    error: None,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                    attempts: 1,
                }),
                Err(e) => Ok(ExecutionResult {
                    success: false,
                    data: None,
                    error: Some(format!("Step execution failed: {}", e)),
                    duration_ms: start_time.elapsed().as_millis() as u64,
                    attempts: 1,
                }),
            }
        } else {
            Ok(ExecutionResult {
                success: true,
                data: Some(serde_json::json!({ "output": step.description.clone() })),
                error: None,
                duration_ms: 0,
                attempts: 1,
            })
        }
    }

    /// Execute decision step
    async fn execute_decision_step(&self, step: &PlanStep) -> Result<ExecutionResult, AgentError> {
        // Decision steps evaluate conditions
        self.execute_action_step(step).await
    }

    /// Execute reasoning step
    async fn execute_reasoning_step(&self, step: &PlanStep) -> Result<ExecutionResult, AgentError> {
        // Reasoning steps typically use LLM
        self.execute_action_step(step).await
    }

    /// Execute information gathering step
    async fn execute_information_step(&self, step: &PlanStep) -> Result<ExecutionResult, AgentError> {
        // Information steps gather data from various sources
        self.execute_action_step(step).await
    }

    /// Execute validation step
    async fn execute_validation_step(&self, step: &PlanStep) -> Result<ExecutionResult, AgentError> {
        // Validation steps verify results
        self.execute_action_step(step).await
    }

    /// Check if should continue on step failure
    fn should_continue_on_failure(&self, plan: &Plan) -> bool {
        plan.metadata.get("continue_on_failure")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    /// Handle plan creation task
    pub async fn handle_plan_creation_task(&self, task: &Task) -> Result<(String, Vec<Artifact>), AgentError> {
        let planning_engine = self.planning_engine.as_ref().ok_or_else(|| {
            AgentError::InvalidConfig("Planning engine not configured".into())
        })?;

        let context = self.create_plan_context(task).await?;
        let strategy = self.select_plan_strategy(task);

        let plan = planning_engine
            .create_plan(&task.input, &context, Some(strategy))
            .await
            .map_err(|e| AgentError::Planning(format!("Failed to create plan: {}", e)))?;

        // Store plan
        {
            let mut active = self.active_plans.write().await;
            active.insert(plan.id.clone(), plan.clone());
        }

        let output = format!(
            "Created plan '{}' (ID: {}) with {} steps using {:?} strategy",
            plan.name, plan.id, plan.steps.len(), strategy
        );

        Ok((output, vec![]))
    }

    /// Handle plan execution task
    pub async fn handle_plan_execution_task(&self, task: &Task) -> Result<(String, Vec<Artifact>), AgentError> {
        let plan_id_str = task.parameters.get("plan_id")
            .or_else(|| if task.input.is_empty() { None } else { Some(&task.input) })
            .ok_or_else(|| AgentError::InvalidConfig("Missing 'plan_id' parameter".into()))?;

        let plan_id = PlanId::from_string(plan_id_str);

        // Find plan
        let plan = {
            let active = self.active_plans.read().await;
            active.get(&plan_id).cloned()
        }.ok_or_else(|| AgentError::NotFound(format!("Plan not found: {}", plan_id)))?;

        // Execute plan
        let result = self.execute_plan_internal(&plan).await?;

        if result.success {
            let output = result.data
                .as_ref()
                .and_then(|d| d.get("output"))
                .and_then(|o| o.as_str())
                .unwrap_or("Plan executed successfully")
                .to_string();
            Ok((output, vec![]))
        } else {
            Err(AgentError::Execution(
                result.error.unwrap_or_else(|| "Plan execution failed".to_string())
            ))
        }
    }

    /// Handle plan adaptation task
    pub async fn handle_plan_adaptation_task(&self, task: &Task) -> Result<(String, Vec<Artifact>), AgentError> {
        let plan_id_str = task.parameters.get("plan_id")
            .ok_or_else(|| AgentError::InvalidConfig("Missing 'plan_id' parameter".into()))?;

        let plan_id = PlanId::from_string(plan_id_str);

        // Find plan
        let plan = {
            let active = self.active_plans.read().await;
            active.get(&plan_id).cloned()
        }.ok_or_else(|| AgentError::NotFound(format!("Plan not found: {}", plan_id)))?;

        // Attempt replanning if replanner is available
        if let Some(replanner) = &self.replanner {
            let mut adapted_plan = plan.clone();
            let trigger = crate::planning::RePlanTrigger::GoalChanged {
                new_goal: task.input.clone(),
                reason: "User requested plan adaptation".to_string(),
            };
            match replanner.replan(&mut adapted_plan, &trigger).await {
                Ok(()) => {
                    // Execute adapted plan
                    let result = self.execute_plan_internal(&adapted_plan).await?;
                    if result.success {
                        return Ok(("Plan adapted and executed successfully".to_string(), vec![]));
                    }
                }
                Err(e) => {
                    warn!("Replanning failed: {}", e);
                }
            }
        }

        Err(AgentError::Planning("Plan adaptation failed".to_string()))
    }

    /// Get active plan by ID
    pub async fn get_active_plan(&self, plan_id: &PlanId) -> Option<Plan> {
        let active = self.active_plans.read().await;
        active.get(plan_id).cloned()
    }

    /// List all active plans
    pub async fn list_active_plans(&self) -> Vec<Plan> {
        let active = self.active_plans.read().await;
        active.values().cloned().collect()
    }

    /// Cancel an active plan
    pub async fn cancel_plan(&self, plan_id: &PlanId) -> Result<(), AgentError> {
        let mut active = self.active_plans.write().await;
        if let Some(mut plan) = active.remove(plan_id) {
            plan.status = PlanStatus::Cancelled;
            info!("Cancelled plan: {}", plan_id);
            Ok(())
        } else {
            Err(AgentError::NotFound(format!("Plan not found: {}", plan_id)))
        }
    }

    /// Explicitly create a plan using planning engine
    pub async fn create_plan(&self, goal: &str, strategy: PlanStrategy) -> Result<Plan, AgentError> {
        let engine = self.planning_engine.as_ref().ok_or_else(|| {
            AgentError::InvalidConfig("Planning engine not configured".into())
        })?;

        let context = PlanContext::new(&self.config.id);
        
        let plan = engine.create_plan(goal, &context, Some(strategy))
            .await
            .map_err(|e| AgentError::Planning(format!("Failed to create plan: {}", e)))?;
        
        // Store plan in active_plans
        {
            let mut active = self.active_plans.write().await;
            active.insert(plan.id.clone(), plan.clone());
        }
        
        Ok(plan)
    }

    /// Explicitly execute a plan
    pub async fn execute_plan(&self, plan: &Plan) -> Result<ExecutionResult, AgentError> {
        // Store plan first
        {
            let mut active = self.active_plans.write().await;
            active.insert(plan.id.clone(), plan.clone());
        }

        let result = self.execute_plan_internal(plan).await;

        // Cleanup
        {
            let mut active = self.active_plans.write().await;
            active.remove(&plan.id);
        }

        result
    }

    // 🆕 DEVICE FIX: Device automation task handlers

    /// Handle device automation task
    pub async fn handle_device_automation_task(&self, task: &Task) -> Result<(String, Vec<Artifact>), AgentError> {
        let device = self.device.as_ref().ok_or_else(|| {
            AgentError::InvalidConfig("No device configured for automation".into())
        })?;

        let action = task.parameters.get("action")
            .ok_or_else(|| AgentError::InvalidConfig("Missing 'action' parameter for device automation".into()))?;

        let result = match action.as_str() {
            "tap" => {
                let x = task.parameters.get("x").and_then(|s| s.parse().ok()).unwrap_or(0);
                let y = task.parameters.get("y").and_then(|s| s.parse().ok()).unwrap_or(0);
                match device {
                    Device::Node(d) => d.tap(x, y).await,
                    Device::Ios(d) => d.tap(x, y).await,
                    Device::Android(d) => d.tap(x, y).await,
                }?;
                format!("Tapped at ({}, {})", x, y)
            }
            "swipe" => {
                let from_x = task.parameters.get("from_x").and_then(|s| s.parse().ok()).unwrap_or(0);
                let from_y = task.parameters.get("from_y").and_then(|s| s.parse().ok()).unwrap_or(0);
                let to_x = task.parameters.get("to_x").and_then(|s| s.parse().ok()).unwrap_or(0);
                let to_y = task.parameters.get("to_y").and_then(|s| s.parse().ok()).unwrap_or(0);
                let duration = task.parameters.get("duration").and_then(|s| s.parse().ok()).unwrap_or(500);
                match device {
                    Device::Node(d) => d.swipe(from_x, from_y, to_x, to_y, duration).await,
                    Device::Ios(d) => d.swipe(from_x, from_y, to_x, to_y, duration).await,
                    Device::Android(d) => d.swipe(from_x, from_y, to_x, to_y, duration).await,
                }?;
                format!("Swiped from ({}, {}) to ({}, {})", from_x, from_y, to_x, to_y)
            }
            "screenshot" => {
                let screenshot = match device {
                    Device::Node(d) => d.take_screenshot().await,
                    Device::Ios(d) => d.take_screenshot().await,
                    Device::Android(d) => d.take_screenshot().await,
                }?;
                format!("Screenshot captured: {} bytes", screenshot.len())
            }
            "press_button" => {
                let button_str = task.parameters.get("button").unwrap_or(&"home".to_string()).clone();
                let button = match button_str.as_str() {
                    "home" => crate::device::HardwareButton::Home,
                    "back" => crate::device::HardwareButton::Back,
                    "power" => crate::device::HardwareButton::Power,
                    "volume_up" => crate::device::HardwareButton::VolumeUp,
                    "volume_down" => crate::device::HardwareButton::VolumeDown,
                    _ => crate::device::HardwareButton::Home,
                };
                match device {
                    Device::Node(d) => d.press_button(button).await,
                    Device::Ios(d) => d.press_button(button).await,
                    Device::Android(d) => d.press_button(button).await,
                }?;
                format!("Pressed button: {:?}", button)
            }
            "type_text" => {
                let text = &task.input;
                match device {
                    Device::Node(d) => d.type_text(text).await,
                    Device::Ios(d) => d.type_text(text).await,
                    Device::Android(d) => d.type_text(text).await,
                }?;
                format!("Typed text: {}", text)
            }
            "find_element" => {
                let locator_type = task.parameters.get("locator_type").unwrap_or(&"id".to_string()).clone();
                let locator_value = &task.input;
                let locator = crate::device::ElementLocator::new(
                    match locator_type.as_str() {
                        "id" => crate::device::LocatorType::Id,
                        "xpath" => crate::device::LocatorType::XPath,
                        "accessibility_id" => crate::device::LocatorType::AccessibilityId,
                        "text" => crate::device::LocatorType::Text,
                        _ => crate::device::LocatorType::Id,
                    },
                    locator_value
                );
                let element = match device {
                    Device::Node(d) => d.find_element(&locator).await,
                    Device::Ios(d) => d.find_element(&locator).await,
                    Device::Android(d) => d.find_element(&locator).await,
                }?;
                format!("Found element: {:?}", element)
            }
            "tap_element" => {
                let locator_type = task.parameters.get("locator_type").unwrap_or(&"id".to_string()).clone();
                let locator_value = &task.input;
                let locator = crate::device::ElementLocator::new(
                    match locator_type.as_str() {
                        "id" => crate::device::LocatorType::Id,
                        "xpath" => crate::device::LocatorType::XPath,
                        "accessibility_id" => crate::device::LocatorType::AccessibilityId,
                        "text" => crate::device::LocatorType::Text,
                        _ => crate::device::LocatorType::Id,
                    },
                    locator_value
                );
                match device {
                    Device::Node(d) => d.tap_element(&locator).await,
                    Device::Ios(d) => d.tap_element(&locator).await,
                    Device::Android(d) => d.tap_element(&locator).await,
                }?;
                format!("Tapped element: {}", locator_value)
            }
            _ => {
                return Err(AgentError::InvalidConfig(format!(
                    "Unknown device action: {}", action
                )));
            }
        };

        Ok((result, vec![]))
    }

    /// Handle app lifecycle task
    pub async fn handle_app_lifecycle_task(&self, task: &Task) -> Result<(String, Vec<Artifact>), AgentError> {
        let device = self.device.as_ref().ok_or_else(|| {
            AgentError::InvalidConfig("No device configured for app lifecycle".into())
        })?;

        let operation = task.parameters.get("operation")
            .ok_or_else(|| AgentError::InvalidConfig("Missing 'operation' parameter".into()))?;

        let package = &task.input;

        let result = match operation.as_str() {
            "install" => {
                let app_path = task.parameters.get("app_path").unwrap_or(package);
                match device {
                    Device::Node(d) => d.install_app(app_path).await,
                    Device::Ios(d) => d.install_app(app_path).await,
                    Device::Android(d) => d.install_app(app_path).await,
                }?;
                format!("Installed app: {}", app_path)
            }
            "uninstall" => {
                match device {
                    Device::Node(d) => d.uninstall_app(package).await,
                    Device::Ios(d) => d.uninstall_app(package).await,
                    Device::Android(d) => d.uninstall_app(package).await,
                }?;
                format!("Uninstalled app: {}", package)
            }
            "launch" => {
                match device {
                    Device::Node(d) => d.launch_app(package).await,
                    Device::Ios(d) => d.launch_app(package).await,
                    Device::Android(d) => d.launch_app(package).await,
                }?;
                format!("Launched app: {}", package)
            }
            "close" => {
                match device {
                    Device::Node(d) => d.close_app(package).await,
                    Device::Ios(d) => d.close_app(package).await,
                    Device::Android(d) => d.close_app(package).await,
                }?;
                format!("Closed app: {}", package)
            }
            "is_installed" => {
                let installed = match device {
                    Device::Node(d) => d.is_app_installed(package).await,
                    Device::Ios(d) => d.is_app_installed(package).await,
                    Device::Android(d) => d.is_app_installed(package).await,
                }?;
                format!("App {} installed: {}", package, installed)
            }
            "clear_data" => {
                match device {
                    Device::Node(d) => d.clear_app_data(package).await,
                    Device::Ios(d) => d.clear_app_data(package).await,
                    Device::Android(d) => d.clear_app_data(package).await,
                }?;
                format!("Cleared app data: {}", package)
            }
            _ => {
                return Err(AgentError::InvalidConfig(format!(
                    "Unknown app lifecycle operation: {}", operation
                )));
            }
        };

        Ok((result, vec![]))
    }
}

/// Task complexity level for determining execution strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskComplexity {
    /// Simple task - can be executed directly
    Simple,
    /// Complex task - requires planning
    Complex,
}

// ============================================================================
// 🆕 OPTIMIZATION: Unit Tests for Planning Integration
// ============================================================================

#[cfg(test)]
mod planning_integration_tests {
    use super::*;
    use crate::planning::{PlanningEngine, PlanExecutor};

    /// Helper function to create a test agent without planning
    fn create_test_agent() -> Agent {
        Agent::new(AgentConfig::default())
    }

    /// Helper function to create a test agent with planning capabilities
    fn create_test_agent_with_planning() -> Agent {
        Agent::new(AgentConfig::default())
            .with_planning_engine(Arc::new(PlanningEngine::new()))
            .with_plan_executor(Arc::new(PlanExecutor::new()))
    }

    // ============================================================================
    // Task Complexity Analysis Tests
    // ============================================================================

    #[tokio::test]
    async fn test_analyze_task_complexity_simple() {
        let agent = create_test_agent();
        let task = Task {
            id: "test-1".to_string(),
            task_type: TaskType::LlmChat,
            input: "Hello".to_string(),
            parameters: HashMap::new(),
        };
        
        assert_eq!(
            agent.analyze_task_complexity(&task).await,
            TaskComplexity::Simple
        );
    }

    #[tokio::test]
    async fn test_analyze_task_complexity_long_input() {
        let agent = create_test_agent();
        let task = Task {
            id: "test-2".to_string(),
            task_type: TaskType::LlmChat,
            input: "x".repeat(201), // > 200 chars
            parameters: HashMap::new(),
        };
        
        assert_eq!(
            agent.analyze_task_complexity(&task).await,
            TaskComplexity::Complex
        );
    }

    #[tokio::test]
    async fn test_analyze_task_complexity_multi_step_flag() {
        let agent = create_test_agent();
        let mut params = HashMap::new();
        params.insert("multi_step".to_string(), "true".to_string());
        
        let task = Task {
            id: "test-3".to_string(),
            task_type: TaskType::SkillExecution,
            input: "Short".to_string(),
            parameters: params,
        };
        
        assert_eq!(
            agent.analyze_task_complexity(&task).await,
            TaskComplexity::Complex
        );
    }

    #[tokio::test]
    async fn test_analyze_task_complexity_planning_types() {
        let agent = create_test_agent();
        
        // PlanCreation should always be Complex
        let plan_task = Task {
            id: "test-4".to_string(),
            task_type: TaskType::PlanCreation,
            input: "Short".to_string(),
            parameters: HashMap::new(),
        };
        
        assert_eq!(
            agent.analyze_task_complexity(&plan_task).await,
            TaskComplexity::Complex
        );
        
        // PlanExecution should always be Complex
        let exec_task = Task {
            id: "test-5".to_string(),
            task_type: TaskType::PlanExecution,
            input: "".to_string(),
            parameters: HashMap::new(),
        };
        
        assert_eq!(
            agent.analyze_task_complexity(&exec_task).await,
            TaskComplexity::Complex
        );
        
        // PlanAdaptation should always be Complex
        let adapt_task = Task {
            id: "test-6".to_string(),
            task_type: TaskType::PlanAdaptation,
            input: "Adapt".to_string(),
            parameters: HashMap::new(),
        };
        
        assert_eq!(
            agent.analyze_task_complexity(&adapt_task).await,
            TaskComplexity::Complex
        );
    }

    // ============================================================================
    // Should Use Planning Tests
    // ============================================================================

    #[tokio::test]
    async fn test_should_use_planning_not_ready() {
        let agent = create_test_agent(); // No planning engine
        let task = Task {
            id: "test-7".to_string(),
            task_type: TaskType::LlmChat,
            input: "x".repeat(300), // Complex task
            parameters: HashMap::new(),
        };
        
        // Should not use planning if not configured
        assert!(!agent.should_use_planning(&task).await);
    }

    #[tokio::test]
    async fn test_should_use_planning_complex_task() {
        let agent = create_test_agent_with_planning();
        let task = Task {
            id: "test-8".to_string(),
            task_type: TaskType::LlmChat,
            input: "x".repeat(300), // Complex task
            parameters: HashMap::new(),
        };
        
        // Should use planning for complex tasks
        assert!(agent.should_use_planning(&task).await);
    }

    #[tokio::test]
    async fn test_should_use_planning_explicit_override() {
        let agent = create_test_agent_with_planning();
        let mut params = HashMap::new();
        params.insert("use_planning".to_string(), "true".to_string());
        
        let task = Task {
            id: "test-9".to_string(),
            task_type: TaskType::LlmChat,
            input: "Short".to_string(), // Simple task
            parameters: params,
        };
        
        // Should use planning if explicitly requested
        assert!(agent.should_use_planning(&task).await);
    }

    // ============================================================================
    // Agent Configuration Tests
    // ============================================================================

    #[test]
    fn test_is_planning_ready_without_components() {
        let agent = create_test_agent();
        assert!(!agent.is_planning_ready());
        assert!(!agent.has_planning_engine());
        assert!(!agent.has_plan_executor());
        assert!(!agent.has_replanner());
    }

    #[test]
    fn test_is_planning_ready_with_components() {
        let agent = Agent::new(AgentConfig::default())
            .with_planning_engine(Arc::new(PlanningEngine::new()))
            .with_plan_executor(Arc::new(PlanExecutor::new()));
        
        assert!(agent.is_planning_ready());
        assert!(agent.has_planning_engine());
        assert!(agent.has_plan_executor());
    }

    // ============================================================================
    // Plan Management Tests
    // ============================================================================

    #[tokio::test]
    async fn test_create_and_get_plan() {
        let agent = create_test_agent_with_planning();
        
        let plan = agent
            .create_plan("Test goal", PlanStrategy::ReAct)
            .await
            .expect("Failed to create plan");
        
        // Verify plan exists
        let retrieved = agent.get_active_plan(&plan.id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, plan.id);
    }

    #[tokio::test]
    async fn test_list_active_plans() {
        let agent = create_test_agent_with_planning();
        
        // Initially empty
        let plans = agent.list_active_plans().await;
        assert!(plans.is_empty());
        
        // Create some plans
        let _plan1 = agent.create_plan("Goal 1", PlanStrategy::ReAct).await.unwrap();
        let _plan2 = agent.create_plan("Goal 2", PlanStrategy::Hybrid).await.unwrap();
        
        let plans = agent.list_active_plans().await;
        assert_eq!(plans.len(), 2);
    }

    #[tokio::test]
    async fn test_cancel_plan() {
        let agent = create_test_agent_with_planning();
        
        let plan = agent.create_plan("Test goal", PlanStrategy::ReAct).await.unwrap();
        
        // Cancel plan
        agent.cancel_plan(&plan.id).await.expect("Failed to cancel plan");
        
        // Verify removed
        assert!(agent.get_active_plan(&plan.id).await.is_none());
    }

    #[tokio::test]
    async fn test_cancel_nonexistent_plan() {
        let agent = create_test_agent_with_planning();
        
        let fake_id = PlanId::new();
        let result = agent.cancel_plan(&fake_id).await;
        
        assert!(result.is_err());
        match result.unwrap_err() {
            AgentError::NotFound(_) => (), // Expected
            other => panic!("Expected NotFound error, got {:?}", other),
        }
    }

    // ============================================================================
    // Error Handling Tests
    // ============================================================================

    #[tokio::test]
    async fn test_create_plan_without_engine() {
        let agent = create_test_agent(); // No planning engine
        
        let result = agent.create_plan("Test goal", PlanStrategy::ReAct).await;
        
        assert!(result.is_err());
        match result.unwrap_err() {
            AgentError::InvalidConfig(msg) => {
                assert!(msg.contains("Planning engine not configured"));
            }
            other => panic!("Expected InvalidConfig error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_get_active_plan_nonexistent() {
        let agent = create_test_agent_with_planning();
        
        let fake_id = PlanId::new();
        let plan = agent.get_active_plan(&fake_id).await;
        
        assert!(plan.is_none());
    }

    // ============================================================================
    // Plan Strategy Selection Tests
    // ============================================================================

    #[test]
    fn test_select_plan_strategy_react() {
        let agent = create_test_agent();
        let mut params = HashMap::new();
        params.insert("strategy".to_string(), "react".to_string());
        
        let task = Task {
            id: "test-strategy".to_string(),
            task_type: TaskType::LlmChat,
            input: "Test".to_string(),
            parameters: params,
        };
        
        assert_eq!(agent.select_plan_strategy(&task), PlanStrategy::ReAct);
    }

    #[test]
    fn test_select_plan_strategy_cot() {
        let agent = create_test_agent();
        let mut params = HashMap::new();
        params.insert("strategy".to_string(), "cot".to_string());
        
        let task = Task {
            id: "test-strategy".to_string(),
            task_type: TaskType::LlmChat,
            input: "Test".to_string(),
            parameters: params,
        };
        
        assert_eq!(agent.select_plan_strategy(&task), PlanStrategy::ChainOfThought);
    }

    #[test]
    fn test_select_plan_strategy_default() {
        let agent = create_test_agent();
        let task = Task {
            id: "test-strategy".to_string(),
            task_type: TaskType::LlmChat,
            input: "Test".to_string(),
            parameters: HashMap::new(),
        };
        
        assert_eq!(agent.select_plan_strategy(&task), PlanStrategy::Hybrid);
    }
}
