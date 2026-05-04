//! Agent implementation
//!
//! Core Agent struct and task execution logic.
//! 
//! 🆕 PLANNING FIX: Integrated planning module for autonomous task planning and execution.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use tokio::time::timeout;
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
use crate::skills::composition::{InputMapping, PipelineStep, SkillPipeline};

pub struct Agent {
    pub(crate) config: AgentConfig,
    pub(crate) a2a_client: Option<a2a::A2AClient>,
    pub(crate) mcp_manager: Option<mcp::MCPManager>,
    pub(crate) outbound_router: Option<Arc<communication::OutboundMessageRouter>>,
    pub(crate) message_rx: Option<tokio::sync::mpsc::Receiver<communication::UserMessageContext>>,
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
    // 🟢 P2 FIX: LLM response cache to reduce latency for repeated queries
    pub(crate) llm_response_cache: Arc<RwLock<HashMap<String, (String, Instant)>>>,
    // 🆕 FIX: Hold the current plan's original user goal so skill matching can use
    // domain keywords (e.g. "旅游") even when step descriptions are generic English.
    pub(crate) current_plan_goal: Arc<RwLock<Option<String>>>,
    // 🆕 FIX: Global skill catalog injected into LLM system prompt
    pub(crate) skill_catalog: Option<String>,
    // 🟢 P1 FIX: Workflow registry for workflow execution tasks
    pub(crate) workflow_registry: Option<Arc<crate::workflow::WorkflowRegistry>>,
}

impl Agent {
    pub fn new(config: AgentConfig) -> Self {
        Self {
            config,
            a2a_client: None,
            mcp_manager: None,
            outbound_router: None,
            message_rx: None,
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
            // 🟢 P2 FIX: Initialize LLM response cache
            llm_response_cache: Arc::new(RwLock::new(HashMap::new())),
            // 🆕 FIX: Initialize current plan goal
            current_plan_goal: Arc::new(RwLock::new(None)),
            // 🆕 FIX: Initialize skill catalog
            skill_catalog: None,
            // 🟢 P1 FIX: Initialize workflow registry as None
            workflow_registry: None,
        }
    }

    /// Spawn a sub-agent with shared infrastructure.
    /// The sub-agent inherits kernel, LLM, skill registry, wallet, memory from the parent.
    pub fn spawn_sub_agent(&self, mut config: AgentConfig) -> Result<Agent, AgentError> {
        config.id = format!("{}-sub-{}", self.config.id, uuid::Uuid::new_v4());
        let mut child = Agent::new(config);
        // Share parent's infrastructure
        child.kernel = self.kernel.clone();
        child.llm_interface = self.llm_interface.clone();
        child.skill_registry = self.skill_registry.clone();
        child.wallet = self.wallet.clone();
        child.memory_system = self.memory_system.clone();
        child.event_bus = self.event_bus.clone();
        child.outbound_router = self.outbound_router.clone();
        child.queue_manager = self.queue_manager.clone();
        child.workflow_registry = self.workflow_registry.clone();
        info!("Spawned sub-agent {} from parent {}", child.config.id, self.config.id);
        Ok(child)
    }

    pub fn with_a2a(mut self, client: a2a::A2AClient) -> Self {
        self.a2a_client = Some(client);
        self
    }

    pub fn with_mcp(mut self, manager: mcp::MCPManager) -> Self {
        self.mcp_manager = Some(manager);
        self
    }

    pub fn with_outbound_router(mut self, router: Arc<communication::OutboundMessageRouter>) -> Self {
        self.outbound_router = Some(router);
        self
    }

    pub fn with_message_rx(mut self, rx: tokio::sync::mpsc::Receiver<communication::UserMessageContext>) -> Self {
        self.message_rx = Some(rx);
        self
    }

    pub fn outbound_router(&self) -> Option<&Arc<communication::OutboundMessageRouter>> {
        self.outbound_router.as_ref()
    }

    pub fn has_outbound_router(&self) -> bool {
        self.outbound_router.is_some()
    }

    /// Takes ownership of the message receiver (can only be called once).
    pub fn take_message_rx(&mut self) -> Option<tokio::sync::mpsc::Receiver<communication::UserMessageContext>> {
        self.message_rx.take()
    }

    pub fn message_rx_ref(&self) -> Option<&tokio::sync::mpsc::Receiver<communication::UserMessageContext>> {
        self.message_rx.as_ref()
    }

    pub fn has_message_rx(&self) -> bool {
        self.message_rx.is_some()
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

    /// 🆕 P2 FIX: Auto-detect multi-step intent and build a SkillPipeline
    ///
    /// Scans the user message for sequencing keywords (先/再/然后/first/then)
    /// and explicit skill references. If 2+ known skills are found in sequence,
    /// returns a `SkillPipeline` that chains them with PassThrough mapping.
    pub async fn try_build_auto_pipeline(&self, message: &str) -> Option<SkillPipeline> {
        let registry = self.skill_registry.as_ref()?;
        let skills = registry.list_enabled().await;
        if skills.len() < 2 {
            return None;
        }

        let lower_msg = message.to_lowercase();

        // Sequencing keywords: Chinese and English
        let has_sequence_indicator =
            (lower_msg.contains("先") || lower_msg.contains("首先") || lower_msg.contains("first"))
            && (lower_msg.contains("再") || lower_msg.contains("然后") || lower_msg.contains("接着") || lower_msg.contains("最后")
            || lower_msg.contains("then") || lower_msg.contains("next") || lower_msg.contains("after"));

        let has_pipeline_keywords = lower_msg.contains("pipeline")
            || lower_msg.contains("chain")
            || lower_msg.contains("流水线")
            || lower_msg.contains("串联");

        if !has_sequence_indicator && !has_pipeline_keywords {
            return None;
        }

        // Find all skill references in the message, in order of appearance
        let mut matched_skills: Vec<(usize, String)> = Vec::new();
        for skill in &skills {
            let skill_name_lower = skill.skill.name.to_lowercase();
            let skill_id_lower = skill.skill.id.to_lowercase();

            // Search for skill name or ID in message
            if let Some(pos) = lower_msg.find(&skill_name_lower) {
                matched_skills.push((pos, skill.skill.id.clone()));
            } else if let Some(pos) = lower_msg.find(&skill_id_lower) {
                matched_skills.push((pos, skill.skill.id.clone()));
            }
        }

        // Deduplicate and sort by position in message
        matched_skills.sort_by_key(|(pos, _id)| *pos);
        matched_skills.dedup_by(|a, b| a.1 == b.1);

        if matched_skills.len() < 2 {
            return None;
        }

        let steps: Vec<PipelineStep> = matched_skills
            .into_iter()
            .map(|(_pos, skill_id)| PipelineStep {
                skill_id,
                input_mapping: InputMapping::PassThrough,
                output_schema: None,
            })
            .collect();

        info!(
            "Auto-pipeline built with {} skills for message: {}",
            steps.len(),
            message.chars().take(60).collect::<String>()
        );
        Some(SkillPipeline::new(steps))
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

    /// 🆕 FIX: Set skill catalog for global LLM context injection
    pub fn with_skill_catalog(mut self, catalog: impl Into<String>) -> Self {
        self.skill_catalog = Some(catalog.into());
        self
    }

    /// 🟢 P1 FIX: Attach a workflow registry for workflow execution tasks
    pub fn with_workflow_registry(mut self, registry: Arc<crate::workflow::WorkflowRegistry>) -> Self {
        self.workflow_registry = Some(registry);
        self
    }

    /// 🆕 FIX: Inject skill catalog into message list if configured.
    /// Avoids duplicate injection if the first message already looks like a catalog.
    fn inject_skill_catalog(&self, messages: Vec<communication::Message>) -> Vec<communication::Message> {
        if let Some(ref catalog) = self.skill_catalog {
            // Check if catalog is already present (first message contains the catalog header)
            if messages.first().map(|m| m.content.contains("You have access to the following skills")).unwrap_or(false) {
                return messages;
            }
            let mut result = vec![communication::Message::new(
                uuid::Uuid::new_v4(),
                communication::PlatformType::Custom,
                format!(
                    "[System Context] You have access to the following skills. \
RULES: (1) If a skill matches the user request, reply ONLY with SKILL:<skill_id> and nothing else. \
(2) If NO skill matches, answer directly from general knowledge. \
(3) NEVER analyze, list, or mention available skills in your reply. \
(4) NEVER start with '用户问的是', '查看可用的skills', '这是一个关于', '但是' or similar meta-commentary.\n\n{}",
                    catalog
                ),
            )];
            result.extend(messages);
            result
        } else {
            messages
        }
    }

    /// Clean up LLM responses that contain thinking/process analysis instead of direct answers.
    fn cleanup_thinking_process(response: &str) -> String {
        let response = response.trim();
        if response.is_empty() {
            return response.to_string();
        }
        // If response starts with known thinking prefixes, try to extract actual answer
        let thinking_prefixes = [
            "用户问的是",
            "用户询问的是",
            "用户想知道",
            "用户的问题是",
            "查看可用的skills",
            "让我看看可用的",
            "看看可用的技能",
            "查看可用技能",
            "这是一个关于",
            "这是关于",
            "但是，",
            "但是 ",
            "不过，",
            "系统提示我",
            "我需要",
            "我来分析",
            "让我分析一下",
            "首先，",
            "第一步",
            "根据系统提示",
            "根据要求",
            "根据可用技能",
        ];
        let thinking_keywords = [
            "用户问的是", "用户询问的是", "用户想知道", "查看可用的skills",
            "让我看看可用的", "看看可用的技能", "查看可用技能", "可用的技能列表",
            "skill 列表", "技能列表", "不属于需要调用专门skill",
        ];
        // Detect if response is mostly analysis: starts with thinking prefix OR contains multiple thinking keywords
        let starts_with_thinking = thinking_prefixes.iter().any(|p| response.starts_with(p));
        let thinking_keyword_count = thinking_keywords.iter().filter(|k| response.contains(**k)).count();
        let is_pure_analysis = starts_with_thinking
            || thinking_keyword_count >= 2
            || (response.contains("用户") && response.contains("skill") && response.len() > 200);
        if is_pure_analysis {
            // Try to find any sentence that looks like an actual answer (not starting with thinking prefixes)
            for line in response.lines() {
                let trimmed = line.trim();
                if trimmed.len() > 10
                    && !thinking_prefixes.iter().any(|p| trimmed.starts_with(p))
                    && !trimmed.starts_with("-")
                    && !trimmed.starts_with("•")
                    && !trimmed.starts_with("【")
                    && !trimmed.starts_with("[")
                {
                    // Verify this line doesn't contain heavy thinking keywords
                    let has_thinking = thinking_keywords.iter().any(|k| trimmed.contains(*k));
                    if !has_thinking {
                        return trimmed.to_string();
                    }
                }
            }
            // Fallback: return a generic message encouraging rephrasing
            return "抱歉，我暂时无法准确回答这个问题。您可以换个方式描述您的需求，我会尽力帮助您。".to_string();
        }
        response.to_string()
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

        // Channel lifecycle is managed globally by ChannelInstanceManager, not per-agent.

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
            // 🟢 P1 FIX: Handle workflow execution tasks
            TaskType::WorkflowExecution => self.handle_workflow_task(&task).await,
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
        let (message_text, skill_hint) = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&task.input) {
            let msg = json.get("message")
                .and_then(|m| m.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| task.input.clone());
            let hint = json.get("skill_hint").cloned();
            (msg, hint)
        } else {
            (task.input.clone(), None)
        };

        let char_count = message_text.chars().count();
        let has_explicit_planning_param = task.parameters.contains_key("multi_step")
            || task.parameters.contains_key("dependencies")
            || task.parameters.contains_key("plan");

        // 🆕 FIX: 优化 planning 触发条件，适配中文场景
        // 1. 明确标记的参数总是触发
        // 2. 中等文本(>50字)且含规划关键词，或含多步骤连接词(先...再...然后)
        // 3. 较长文本(>120字)默认触发
        // 4. 英文场景保持原阈值(>200 chars)
        let has_planning_keywords = message_text.contains("计划")
            || message_text.contains("步骤")
            || message_text.contains("安排")
            || message_text.contains("规划")
            || message_text.contains("方案")
            || message_text.contains("攻略")
            || message_text.contains("流程");
        let has_multi_step_indicators =
            (message_text.contains("先") || message_text.contains("首先"))
            && (message_text.contains("再") || message_text.contains("然后") || message_text.contains("最后") || message_text.contains("接着"));

        // 中文文本密度高，适当降低阈值
        let is_chinese = message_text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));
        let planning_threshold = if is_chinese { 50 } else { 120 };
        let long_threshold = if is_chinese { 120 } else { 300 };

        // 🆕 FIX: 强规划关键词（如"计划"/"规划"/"攻略"）即使短文本也应触发 planning，
        // 避免"去汕头市旅游五天的计划"（14字）因低于50字阈值而被误判为简单查询。
        // 设置 6 字符下限防止单字误触发（如"计"）。
        // 🆕 FIX: 但生成类 skill（travel/writer 等）不走 planning，一次性生成更高效。
        let is_generative_skill = skill_hint.as_ref().map_or(false, |hint| {
            let name = hint.get("name").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
            name.contains("travel") || name.contains("planner")
                || name.contains("writer") || name.contains("creator")
                || name.contains("story") || name.contains("email")
                || name.contains("master") || name.contains("game")
        });

        let is_complex = has_explicit_planning_param
            || (has_planning_keywords && char_count >= 6 && !is_generative_skill)
            || has_multi_step_indicators
            || (char_count > planning_threshold && (has_planning_keywords || has_multi_step_indicators))
            || char_count > long_threshold;

        if self.is_planning_ready() && is_complex {
            info!("🧠 Complex LLM task detected (message length: {}), using planning for task {}", message_text.len(), task.id);
            return self.execute_with_planning(task.clone()).await;
        }

        // 🆕 P2 FIX: Auto-pipeline detection for multi-step skill chaining
        if let Some(pipeline) = self.try_build_auto_pipeline(&message_text).await {
            info!("🔄 Auto-pipeline detected for task {}, executing {} steps", task.id, pipeline.steps.len());
            match pipeline.execute(&message_text, self).await {
                Ok(result) => {
                    return Ok((result.clone(), vec![Artifact {
                        id: task.id.clone(),
                        artifact_type: "pipeline_result".to_string(),
                        content: result.as_bytes().to_vec(),
                        mime_type: "text/plain".to_string(),
                    }]));
                }
                Err(e) => {
                    warn!("Auto-pipeline execution failed for task {}: {}, falling back to LLM", task.id, e);
                }
            }
        }

        let llm = self
            .llm_interface
            .as_ref()
            .ok_or_else(|| AgentError::InvalidConfig("LLM interface not configured".into()))?;

        // Parse structured input JSON to extract current message and context metadata
        let (input_text, mut extra_params, image_urls, history, gateway_memory_context, weather_data) = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&task.input) {
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
            let memory_context = json.get("memory_context")
                .and_then(|m| m.as_str())
                .map(|s| s.to_string());
            let weather_data = json.get("weather_data")
                .and_then(|m| m.as_str())
                .map(|s| s.to_string());
            (message, params, images, history, memory_context, weather_data)
        } else {
            (task.input.clone(), task.parameters.clone(), Vec::new(), Vec::new(), None, None)
        };

        let mut metadata = std::collections::HashMap::new();
        if !image_urls.is_empty() {
            metadata.insert("image_urls".to_string(), serde_json::to_string(&image_urls).unwrap_or_default());
        }

        // Build message list with memory context, history, and current message
        let mut messages: Vec<communication::Message> = Vec::new();

        // 🆕 FIX: 当 gateway 传入 skill_hint 时，使用其 prompt_template 作为核心 persona。
        // 若 gateway 已通过 memory_context 注入 skill prompt（当前标准行为），则 persona 只做轻量标识，
        // 避免同一份 prompt_template 在 persona message 和 memory message 中重复出现，浪费 token。
        let persona = if let Some(ref hint) = skill_hint {
            let name = hint.get("name").and_then(|v| v.as_str()).unwrap_or(&self.config.name);
            let prompt_template = hint.get("prompt_template").and_then(|v| v.as_str()).unwrap_or("");
            if !prompt_template.is_empty() {
                // 检查 gateway 是否已把 skill prompt 注入 memory_context
                let gateway_has_skill_prompt = gateway_memory_context.as_ref()
                    .map_or(false, |m| m.contains(prompt_template.trim().split('\n').next().unwrap_or("")));
                if gateway_has_skill_prompt {
                    // Gateway 已注入完整 skill prompt，persona 只做轻量标识
                    format!("你是 {}。请保持友好、专业、有帮助的态度回答问题。", name)
                } else {
                    // Gateway 未注入，使用 skill prompt_template 作为 persona
                    format!("[角色] {}\n\n{}", name, prompt_template)
                }
            } else {
                let desc = hint.get("description").and_then(|v| v.as_str()).unwrap_or(&self.config.description);
                format!("你是 {}（{}）。请保持友好、专业、有帮助的态度回答问题。", name, desc)
            }
        } else {
            format!(
                "你是 {}（{}）。请保持友好、专业、有帮助的态度回答问题。",
                self.config.name,
                self.config.description
            )
        };

        // 🆕 FIX: Append skill-catalog trigger instruction to persona so the LLM
        // knows to emit SKILL:<id> when the user request matches a registered skill.
        // Only add this instruction when NO skill has been matched yet (skill_hint is None).
        // If skill_hint already exists, the gateway has already matched a skill; the agent
        // should execute it directly without asking the LLM to emit SKILL:<id>.
        let is_generative_skill = skill_hint.as_ref().map_or(false, |hint| {
            let name = hint.get("name").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
            name.contains("travel") && name.contains("planner")
        });
        let persona = if self.skill_catalog.is_some() && skill_hint.is_none() && !is_generative_skill {
            format!(
                "{}\n\n[系统指令] 当用户请求与某个 skill 匹配时，请只回复 SKILL:<skill_id>，不要提供其他解释。",
                persona
            )
        } else {
            persona
        };
        // 🆕 FIX: Force direct answer — Kimi k2.6 tends to explain system instructions
        let persona = format!("{}\n\n[强制规则] 直接回答用户问题，不要解释你收到了什么数据、什么技能指引或系统指令。禁止以\"用户问的是...\"、\"系统提示我...\"、\"我需要...\"开头。", persona);
        messages.push(communication::Message::new(
            uuid::Uuid::new_v4(),
            communication::PlatformType::Custom,
            persona,
        ));

        // 🟢 P1 FIX: Use gateway-provided memory_context if available (avoids redundant search + dirty query)
        if let Some(ref gateway_memory) = gateway_memory_context {
            if !gateway_memory.is_empty() {
                info!("Using gateway-provided memory context ({} chars) for agent {}", gateway_memory.len(), self.config.id);
                messages.push(communication::Message::new(
                    uuid::Uuid::new_v4(),
                    communication::PlatformType::Custom,
                    format!("[系统提示：以下是该用户的历史记忆，回答时必须结合这些信息]\n{}", gateway_memory),
                ));
            }
        }
        // Weather data will be appended to the user message below instead of a separate system message
        if let Some(ref memory) = self.memory_system {
            // Fallback: local search using ONLY input_text (never concatenate history — prevents self-referential duplication)
            let query = input_text.clone();

            // 🆕 FIX (方案B): fallback 记忆检索也采用独立预算
            let char_count = query.chars().count();
            let is_simple = char_count <= 10;
            let is_complex = char_count > 30
                || query.contains("计划") || query.contains("规划") || query.contains("步骤")
                || query.contains("安排") || query.contains("攻略") || query.contains("对比")
                || query.contains("分析") || query.contains("总结");
            let search_limit = if is_complex { 6 } else if char_count > 15 { 4 } else { 2 };
            let max_memory_chars = if is_simple { 400 } else if is_complex { 1200 } else { 800 };

            match memory.search(&query).await {
                Ok(results) => {
                    info!("Agent {} local memory search returned {} results (limit={}) for query '{}'..", self.config.id, results.len(), search_limit, query.chars().take(40).collect::<String>());
                    let input_lower = input_text.to_lowercase();
                    let mut total_chars = 0;
                    let memory_context: String = results.iter()
                        .filter(|r| {
                            // Skip memories that are essentially the current query being repeated
                            let is_self_referential = r.content.to_lowercase().contains(&input_lower);
                            if is_self_referential {
                                info!("Filtering out self-referential memory: {}", r.content.chars().take(40).collect::<String>());
                            }
                            !is_self_referential
                        })
                        .take(search_limit)
                        .filter_map(|r| {
                            let entry = format!("- {}", r.content);
                            if total_chars + entry.len() > max_memory_chars {
                                if total_chars == 0 {
                                    // First entry already too long, truncate it
                                    let truncated = format!("- {}...", &r.content[..r.content.len().min(max_memory_chars - 4)]);
                                    total_chars += truncated.len();
                                    return Some(truncated);
                                }
                                return Some("- ...（更多记忆已省略）".to_string());
                            }
                            total_chars += entry.len();
                            Some(entry)
                        })
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

        // Add current user message (with weather data appended if available)
        let user_message = if let Some(ref weather) = weather_data {
            if !weather.is_empty() {
                format!("用户: {}\n\n[参考数据] 实时天气：{}\n请基于以上数据回答。", input_text, weather)
            } else {
                format!("用户: {}", input_text)
            }
        } else {
            format!("用户: {}", input_text)
        };
        messages.push(communication::Message::with_metadata(
            uuid::Uuid::new_v4(),
            communication::PlatformType::Custom,
            user_message,
            metadata,
        ));

        // 🟢 P2 FIX: Dynamic max_tokens based on message complexity and skill type
        // Generative skills (travel_planner, etc.) need more tokens for rich output,
        // but we cap at 1200 to keep response times under ~60s.
        let dynamic_max_tokens = if is_generative_skill {
            "1200".to_string()
        } else if input_text.chars().count() < 30 {
            "300".to_string()
        } else if input_text.chars().count() < 100 {
            "600".to_string()
        } else {
            "1200".to_string()
        };
        extra_params.insert("max_tokens".to_string(), dynamic_max_tokens);

        // 🟢 P2 FIX: Check LLM response cache for simple text queries (no images, < 500 chars)
        let cache_key = if image_urls.is_empty() && input_text.len() < 500 {
            let memory_hash = gateway_memory_context.as_ref().map(|m| m.len().to_string()).unwrap_or_else(|| "0".to_string());
            Some(format!("{}|{}|{}", self.config.id, input_text.trim(), memory_hash))
        } else {
            None
        };

        if let Some(ref key) = cache_key {
            let cache = self.llm_response_cache.read().await;
            if let Some((cached_response, timestamp)) = cache.get(key) {
                if timestamp.elapsed() < Duration::from_secs(300) {
                    info!("P2 CACHE HIT: agent {} returning cached response for '{}' (age {:?})", self.config.id, input_text.chars().take(40).collect::<String>(), timestamp.elapsed());
                    return Ok((cached_response.clone(), vec![]));
                }
            }
            drop(cache);
        }

        let messages = self.inject_skill_catalog(messages);
        info!("handle_llm_task: messages count after inject = {}, skill_catalog set = {}", messages.len(), self.skill_catalog.is_some());

        let response = llm
            .call_llm(messages, Some(extra_params))
            .await
            .map_err(|e| AgentError::Execution(format!("LLM call failed: {}", e)))?;

        info!("handle_llm_task: LLM raw response (first 200 chars) = {}", &response.chars().take(200).collect::<String>());

        // 🆕 FIX: Guard against empty LLM responses to avoid corrupting conversation history
        if response.trim().is_empty() {
            warn!("LLM returned empty response; skipping cache/history storage");
            return Ok(("抱歉，AI 暂时无法生成回复，请稍后再试。".to_string(), vec![]));
        }

        // 🆕 FIX: If the LLM response is a skill trigger (e.g. "SKILL:hello_world"),
        // look up the skill in the registry and execute it instead of returning raw text.
        if let Some(skill_id) = response.trim().strip_prefix("SKILL:") {
            // 🛡️ FIX: Parse only the skill ID before any parameters (| or whitespace)
            let skill_id = skill_id.trim().split(|c: char| c == '|' || c.is_whitespace()).next().unwrap_or("").trim();
            if skill_id.is_empty() {
                warn!("LLM returned empty skill ID after parsing: {}", response.trim());
            } else {
                info!("LLM requested skill execution: {}", skill_id);
                if let Some(ref registry) = self.skill_registry {
                    if let Some(registered) = registry.get(skill_id).await {
                        let skill_result = self.execute_registered_skill(&registered, &input_text, None).await;
                        match skill_result {
                            Ok(result) => {
                                let _ = registry.record_usage(skill_id).await;
                                return Ok((result.output, vec![]));
                            }
                            Err(e) => {
                                warn!("Skill execution for '{}' failed: {}", skill_id, e);
                                return Ok((format!("执行 skill '{}' 时出错: {}", skill_id, e), vec![]));
                            }
                        }
                    } else {
                        warn!("LLM requested unknown skill: {}", skill_id);
                        return Ok((format!("抱歉，找不到 skill '{}'。", skill_id), vec![]));
                    }
                }
            }
        }

        // 🆕 FIX: Clean up thinking process from LLM response
        let response = Self::cleanup_thinking_process(&response);

        // 🟢 P2 FIX: Store response in cache
        if let Some(ref key) = cache_key {
            let mut cache = self.llm_response_cache.write().await;
            cache.insert(key.clone(), (response.clone(), Instant::now()));
            // Simple eviction: if cache grows beyond 100 entries, clear oldest half
            if cache.len() > 100 {
                let mut entries: Vec<_> = cache.drain().collect();
                entries.sort_by(|a, b| b.1.1.cmp(&a.1.1)); // newest first
                let keep = entries.len() / 2;
                for (k, v) in entries.into_iter().take(keep) {
                    cache.insert(k, v);
                }
            }
            drop(cache);
            info!("P2 CACHE STORE: agent {} cached response for '{}'", self.config.id, input_text.chars().take(40).collect::<String>());
        }

        Ok((response, vec![]))
    }

    /// Check whether a skill directory contains executable scripts
    async fn has_scripts_in_dir(&self, dir: &std::path::Path) -> bool {
        let mut entries = match tokio::fs::read_dir(dir).await {
            Ok(e) => e,
            Err(_) => return false,
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                if matches!(ext, "py" | "js" | "sh" | "ts") {
                    return true;
                }
            }
        }
        false
    }

    /// 🟢 P1 FIX: Public API to execute a skill by ID (used by composition, SkillCallTool, and external callers)
    pub async fn execute_skill_by_id(
        &self,
        skill_id: &str,
        input: &str,
        parameters: Option<HashMap<String, String>>,
    ) -> Result<skills::executor::SkillExecutionResult, AgentError> {
        let registry = self
            .skill_registry
            .as_ref()
            .ok_or_else(|| AgentError::InvalidConfig("Skill registry not configured".into()))?;

        let registered_skill = registry
            .get(skill_id).await
            .ok_or_else(|| AgentError::SkillNotFound(skill_id.to_string()))?;

        let result = self.execute_registered_skill(&registered_skill, input, parameters).await?;
        let _ = registry.record_usage(skill_id).await;
        Ok(result)
    }

    /// 🟢 P1 FIX: Internal helper for composition modules to call LLM with a simple prompt
    pub(crate) async fn call_llm_prompt(
        &self,
        prompt: impl Into<String>,
        system: Option<impl Into<String>>,
    ) -> Result<String, AgentError> {
        let llm = self.llm_interface.as_ref()
            .ok_or_else(|| AgentError::InvalidConfig("LLM interface not configured".into()))?;
        let mut messages: Vec<communication::Message> = Vec::new();
        if let Some(sys) = system {
            messages.push(communication::Message::new(
                uuid::Uuid::new_v4(),
                communication::PlatformType::Custom,
                sys.into(),
            ));
        }
        messages.push(communication::Message::new(
            uuid::Uuid::new_v4(),
            communication::PlatformType::Custom,
            prompt.into(),
        ));
        llm.call_llm(self.inject_skill_catalog(messages), None).await
            .map_err(|e| AgentError::Execution(format!("LLM call failed: {}", e)))
    }

    /// 🟢 P2 FIX: Judge a condition using LLM (used by LlmJudge in conditional/loop)
    pub(crate) async fn judge_condition(&self, prompt: &str, output: &str) -> Result<bool, AgentError> {
        let full_prompt = format!(
            "请根据以下条件判断给定的输出是否满足要求。\n\n条件: {}\n输出: {}\n\n如果满足条件，只回答 'true'；如果不满足，只回答 'false'。不要解释。",
            prompt, output
        );
        let result = self.call_llm_prompt(
            full_prompt,
            Some::<String>("你是一个严谨的条件判断助手。只输出 true 或 false。".into())
        ).await?;
        let trimmed = result.trim().to_lowercase();
        Ok(trimmed.contains("true") || trimmed.starts_with("是") || trimmed.starts_with("yes"))
    }

    /// 🟢 P2 FIX: Helper to execute a registered skill (shared by handle_skill_task and planning)
    async fn execute_registered_skill(
        &self,
        registered_skill: &skills::RegisteredSkill,
        input: &str,
        parameters: Option<HashMap<String, String>>,
    ) -> Result<skills::executor::SkillExecutionResult, AgentError> {
        let context = skills::executor::SkillContext {
            input: input.to_string(),
            parameters: parameters.unwrap_or_default(),
        };

        let start_time = std::time::Instant::now();
        
        // 🆕 FIX: Skip WASM attempt for markdown-based builtin skills that have no WASM binary.
        let wasm_path_empty = registered_skill.skill.wasm_path.as_os_str().is_empty();
        
        // 1. Try WASM execution if kernel and wasm_engine are available
        if !wasm_path_empty {
            if let Some(kernel) = self.kernel.as_ref() {
                if let Some(engine) = kernel.wasm_engine() {
                    let wasm_bytes = tokio::fs::read(&registered_skill.skill.wasm_path).await;
                    if let Ok(bytes) = wasm_bytes {
                        let module = engine.compile_cached(&registered_skill.skill.id, &bytes);
                        if let Ok(m) = module {
                            let instance = engine.instantiate_with_host(&m, &self.config.id);
                            if let Ok(mut inst) = instance {
                                let input_bytes = context.input.as_bytes();
                                if inst.write_memory(0, input_bytes).is_ok() {
                                    const MAX_OUTPUT_SIZE: usize = 65536;
                                    let call_result = inst.call_typed::<(i32, i32), i32>(
                                        &registered_skill.skill.manifest.entry_point,
                                        (0i32, input_bytes.len() as i32),
                                    );
                                    if let Ok(output_ptr) = call_result {
                                        let output_addr = output_ptr as usize;
                                        if let Ok(len_bytes) = inst.read_memory(output_addr, 4) {
                                            let output_len = u32::from_le_bytes([
                                                len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3],
                                            ]) as usize;
                                            if output_len <= MAX_OUTPUT_SIZE {
                                                if let Ok(output_bytes) = inst.read_memory(output_addr + 4, output_len) {
                                                    if let Ok(output) = String::from_utf8(output_bytes) {
                                                        return Ok(skills::executor::SkillExecutionResult {
                                                            task_id: registered_skill.skill.id.clone(),
                                                            success: true,
                                                            output,
                                                            structured_output: None,
                                                            execution_time_ms: start_time.elapsed().as_millis() as u64,
                                                        });
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            info!("Skill '{}' WASM unavailable or failed, falling back to LLM execution", registered_skill.skill.name);
        }
        
        // 2. Knowledge / Code skill execution via ReAct executor
        // 🆕 FIX: Generative skills (e.g. travel_planner) do not need ReAct tools;
        // direct LLM generation is faster and sufficient.
        let is_generative_skill = registered_skill.skill.name.to_lowercase().contains("travel")
            && registered_skill.skill.name.to_lowercase().contains("planner");

        let source = &registered_skill.skill.source_path;
        if !is_generative_skill && !source.as_os_str().is_empty() {
            if let Some(llm) = &self.llm_interface {
                let has_scripts = if source.is_dir() {
                    self.has_scripts_in_dir(source).await
                } else {
                    false
                };

                let result = if has_scripts {
                    info!("Executing code skill '{}' via ReAct with tools", registered_skill.skill.name);
                    let executor = skills::CodeSkillExecutor::new(llm.clone());
                    executor.execute(source, input).await
                } else {
                    info!("Executing knowledge skill '{}' via ReAct with tools", registered_skill.skill.name);
                    let executor = skills::KnowledgeSkillExecutor::new(llm.clone());
                    executor.execute(source, input).await
                };

                return match result {
                    Ok(output) => Ok(skills::executor::SkillExecutionResult {
                        task_id: registered_skill.skill.id.clone(),
                        success: true,
                        output,
                        structured_output: None,
                        execution_time_ms: start_time.elapsed().as_millis() as u64,
                    }),
                    Err(e) => {
                        warn!("ReAct execution for skill '{}' failed: {}", registered_skill.skill.name, e);
                        Err(e)
                    }
                };
            }
        }
        
        // 3. Legacy LLM fallback (source_path empty or no llm_interface)
        info!("Skill '{}' using legacy LLM fallback", registered_skill.skill.name);
        if let Some(llm) = &self.llm_interface {
            let manifest = &registered_skill.skill.manifest;
            let system_prompt = if !manifest.prompt_template.is_empty() {
                let mut prompt = manifest.prompt_template.clone();
                if !manifest.description.is_empty() && !prompt.contains(&manifest.description) {
                    prompt.push_str(&format!("\n\nAbout this skill: {}", manifest.description));
                }
                if !manifest.examples.is_empty() {
                    prompt.push_str(&format!("\n\nExamples:\n{}", manifest.examples));
                }
                prompt
            } else {
                format!(
                    "You are acting as the skill '{}'. {}\n\nSkill capabilities:\n{}\n\nExecute the following task using this skill persona.",
                    registered_skill.skill.name,
                    manifest.description,
                    manifest.capabilities.iter().map(|c| format!("- {}", c)).collect::<Vec<_>>().join("\n")
                )
            };
            let messages = vec![
                communication::Message::new(
                    uuid::Uuid::new_v4(),
                    communication::PlatformType::Custom,
                    system_prompt,
                ),
                communication::Message::new(
                    uuid::Uuid::new_v4(),
                    communication::PlatformType::Custom,
                    context.input.clone(),
                ),
            ];
            
            match llm.call_llm(self.inject_skill_catalog(messages), None).await {
                Ok(response) => {
                    return Ok(skills::executor::SkillExecutionResult {
                        task_id: registered_skill.skill.id.clone(),
                        success: true,
                        output: response,
                        structured_output: None,
                        execution_time_ms: start_time.elapsed().as_millis() as u64,
                    });
                }
                Err(e) => {
                    warn!("LLM fallback for skill '{}' also failed: {}", registered_skill.skill.name, e);
                }
            }
        }
        
        // Last resort: try legacy skill executor
        let executor = skills::SkillExecutor::new().map_err(|e| {
            AgentError::Execution(format!("Failed to create skill executor: {}", e))
        })?;
        executor.execute(&registered_skill.skill, context).await
            .map_err(|e| AgentError::Execution(format!("Skill execution failed: {}", e)))
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

        let result = self.execute_registered_skill(&registered_skill, &task.input, Some(task.parameters.clone())).await?;
        let execution_time_ms = result.execution_time_ms;

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

            llm.call_llm(self.inject_skill_catalog(messages), None)
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

        // Channels are disconnected globally; agent shutdown only cleans its own queue.

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

        // 🆕 FIX: Store the original user goal so skill matching can use domain keywords
        // without polluting every step description (which bloats LLM prompts).
        {
            let mut g = self.current_plan_goal.write().await;
            *g = Some(goal.clone());
        }

        info!("Created plan {} with {} steps for task {}", plan.id, plan.steps.len(), task.id);

        // 4. Store active plan
        {
            let mut active = self.active_plans.write().await;
            active.insert(plan.id.clone(), plan.clone());
        }

        // 5. Execute plan with timeout protection
        // 🆕 FIX: 动态计算超时：每步 30s + 15s 缓冲，上限 180s
        let step_count = plan.steps.len().max(1);
        let plan_timeout_secs = task.parameters.get("timeout_secs")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or_else(|| (step_count as u64 * 30 + 15).min(180));
        let plan_timeout = Duration::from_secs(plan_timeout_secs);
        
        let result = match timeout(plan_timeout, self.execute_plan_internal(&plan)).await {
            Ok(r) => r,
            Err(_) => {
                error!("⏱️ Plan execution timed out after {}s for task {}", plan_timeout.as_secs(), task.id);
                return Err(AgentError::Execution(
                    format!("Plan execution timed out after {}s", plan_timeout.as_secs())
                ));
            }
        };

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
    /// 🆕 FIX: Hard cap on step count to prevent LLM storm and timeout
    async fn execute_plan_internal(&self, plan: &Plan) -> Result<ExecutionResult, AgentError> {
        const MAX_PLAN_STEPS: usize = 5;  // 🆕 FIX: 从 3 提升到 5，减少暴力截断
        
        // 🆕 FIX: Reject or truncate plans with too many steps
        if plan.steps.len() > MAX_PLAN_STEPS {
            warn!("Plan {} has {} steps, exceeding max {}. Truncating to first {} steps.", 
                  plan.id, plan.steps.len(), MAX_PLAN_STEPS, MAX_PLAN_STEPS);
            // Note: Truncation requires rebuilding dependencies, so we fall back to sequential
            // execution on a truncated view. For safety, we execute sequentially.
            let mut truncated_plan = plan.clone();
            truncated_plan.steps.truncate(MAX_PLAN_STEPS);
            truncated_plan.dependencies.clear();
            for i in 1..truncated_plan.steps.len() {
                let _ = truncated_plan.add_step_with_deps(truncated_plan.steps[i].clone(), vec![i - 1]);
            }
            return self.execute_plan_sequential_or_dependency_aware(&truncated_plan).await;
        }
        
        // Check if parallel execution is enabled via plan metadata
        let enable_parallel = plan.metadata.get("enable_parallel")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        
        let max_concurrency = plan.metadata.get("max_concurrency")
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as usize; // 🆕 FIX: Reduced default from 5 to 3

        // 🆕 FIX: Only run truly independent steps in parallel.
        // Simple chain dependencies (0->1->2->...) MUST run sequentially to avoid
        // wasting LLM calls on steps that depend on prior outputs.
        let has_simple_chain_deps = plan.dependencies.len() + 1 == plan.steps.len()
            && plan.dependencies.iter().all(|(k, v)| v.len() == 1 && v[0] + 1 == *k);

        if enable_parallel && plan.dependencies.is_empty() {
            // Parallel execution only for truly independent steps
            self.execute_plan_parallel(plan, max_concurrency).await
        } else if enable_parallel && has_simple_chain_deps {
            // 🆕 FIX: Chain dependencies → sequential execution, NOT parallel
            info!("Plan {} has chain dependencies ({} steps), executing sequentially to avoid LLM waste", 
                  plan.id, plan.steps.len());
            self.execute_plan_sequential_or_dependency_aware(plan).await
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

    /// 🆕 FIX: Smart skill search for plan steps using keyword domain mapping.
    /// Maps step descriptions to relevant skills based on semantic keyword overlap
    /// rather than simple string containment.
    async fn search_skills_for_step(
        &self,
        registry: &Arc<skills::SkillRegistry>,
        step_description: &str,
    ) -> Vec<skills::RegisteredSkill> {
        let desc_lower = step_description.to_lowercase();
        
        // Domain keyword → skill name/tag mappings
        let domain_keywords: &[(&[&str], &str)] = &[
            (&["travel", "tour", "trip", "itinerary", "旅游", "旅行", "行程", "攻略", "景点", "酒店"], "travel_planner"),
            (&["code", "program", "develop", "debug", "coding", "编程", "代码", "开发"], "python_developer"),
            (&["code", "rust", "cargo", "编程", "代码"], "rust_developer"),
            (&["contract", "solidity", "smart contract", "合约", "区块链"], "solidity_developer"),
            (&["write", "email", "draft", "邮件", "写信"], "email_writer"),
            (&["story", "novel", "fiction", "write", "故事", "小说"], "story_writer"),
            (&["game", "gaming", "游戏", "玩家"], "game_master"),
            (&["data", "analyze", "analysis", "数据", "分析", "统计"], "data_analyst"),
            (&["image", "photo", "picture", "图", "照片"], "image_analyst"),
            (&["calendar", "schedule", "meeting", "日历", "会议", "安排"], "calendar_assistant"),
            (&["task", "todo", "plan", "任务", "待办"], "task_manager"),
            (&["defi", "yield", "liquidity", "farm", "挖矿", "流动性"], "yield_farmer"),
            (&["nft", "mint", "token", "数字藏品"], "nft_minter"),
            (&["health", "medical", "doctor", "健康", "医疗", "医生"], "health_advisor"),
            (&["learn", "study", "tutor", "lesson", "学习", "课程", "辅导"], "tutor"),
            (&["research", "paper", "survey", "研究", "论文", "调查"], "code_researcher"),
            (&["dao", "governance", "proposal", "vote", "治理", "提案", "投票"], "governance_analyst"),
            (&["finance", "portfolio", "invest", "理财", "投资", "组合"], "portfolio_manager"),
            (&["social", "community", "content", "社媒", "社群", "内容"], "content_creator"),
            (&["security", "audit", "vulnerability", "安全", "审计", "漏洞"], "auditor"),
        ];
        
        let mut matched_skill_ids = std::collections::HashSet::new();
        let mut all_candidates = Vec::new();
        
        // 1. Try domain keyword mapping against step description
        for (keywords, skill_id) in domain_keywords {
            if keywords.iter().any(|kw| desc_lower.contains(kw)) {
                if let Some(skill) = registry.get(skill_id).await {
                    if matched_skill_ids.insert(skill_id.to_string()) {
                        all_candidates.push(skill);
                    }
                }
            }
        }
        
        // 🆕 FIX: 优先用原始用户目标匹配 skill（planning steps 常为英文 generic 描述，
        // 而 goal 包含中文领域关键词，匹配成功率更高）。不再要求 all_candidates 为空，
        // 而是总是把 goal 匹配的 skill 加入候选池。
        if let Some(ref goal) = *self.current_plan_goal.read().await {
            let goal_lower = goal.to_lowercase();
            for (keywords, skill_id) in domain_keywords {
                if keywords.iter().any(|kw| goal_lower.contains(kw)) {
                    if let Some(skill) = registry.get(skill_id).await {
                        if matched_skill_ids.insert(skill_id.to_string()) {
                            info!("P2 PLANNING: skill '{}' matched via goal '{}' for step '{}'", 
                                  skill_id, goal.chars().take(30).collect::<String>(), 
                                  step_description.chars().take(40).collect::<String>());
                            all_candidates.push(skill);
                        }
                    } else {
                        warn!("P2 PLANNING: skill '{}' not found in registry (goal match)", skill_id);
                    }
                }
            }
        }
        
        // 2. Fallback to registry semantic search (name/description)
        let registry_candidates = registry.search(step_description).await;
        for skill in registry_candidates {
            if matched_skill_ids.insert(skill.skill.id.clone()) {
                all_candidates.push(skill);
            }
        }
        
        // 3. Tag-based search with keywords extracted from description
        let extracted_keywords: Vec<&str> = desc_lower
            .split_whitespace()
            .filter(|w| w.len() >= 3)
            .collect();
        for keyword in extracted_keywords.iter().take(5) {
            let tagged = registry.by_tag(keyword).await;
            for skill in tagged {
                if matched_skill_ids.insert(skill.skill.id.clone()) {
                    all_candidates.push(skill);
                }
            }
        }
        
        all_candidates
    }

    /// Execute action step
    ///
    /// 🟢 P2 FIX: Before falling back to LLM, attempts to match and execute a registered skill.
    /// This makes planning actually invoke tools instead of just chaining LLM calls.
    async fn execute_action_step(&self, step: &PlanStep) -> Result<ExecutionResult, AgentError> {
        let start_time = std::time::Instant::now();

        // 🟢 P2 FIX: Try skill registry first with semantic keyword matching
        if let Some(ref registry) = self.skill_registry {
            let enabled_count = registry.list_enabled().await.len();
            info!("P2 PLANNING: skill registry has {} enabled skills for step '{}'", enabled_count, step.description.chars().take(40).collect::<String>());
            let candidates = self.search_skills_for_step(registry, &step.description).await;
            info!("P2 PLANNING: found {} candidates for step '{}'", candidates.len(), step.description.chars().take(40).collect::<String>());

            if let Some(skill) = candidates.into_iter().find(|s| s.enabled) {
                info!("P2 PLANNING: matched skill '{}' for step '{}', executing...", skill.skill.name, step.description.chars().take(40).collect::<String>());
                match self.execute_registered_skill(&skill, &step.description, None).await {
                    Ok(result) => {
                        let _ = registry.record_usage(&skill.skill.id).await;
                        return Ok(ExecutionResult {
                            success: result.success,
                            data: Some(serde_json::json!({ "output": result.output })),
                            error: if result.success { None } else { Some(result.output.clone()) },
                            duration_ms: start_time.elapsed().as_millis() as u64,
                            attempts: 1,
                        });
                    }
                    Err(e) => {
                        warn!("P2 PLANNING: skill execution failed for step '{}', falling back to LLM: {}", step.description.chars().take(40).collect::<String>(), e);
                    }
                }
            } else {
                warn!("P2 PLANNING: no enabled skill matched for step '{}'", step.description.chars().take(40).collect::<String>());
            }
        } else {
            warn!("P2 PLANNING: skill registry is None, cannot match skills for step '{}'", step.description.chars().take(40).collect::<String>());
        }

        // Fallback: use LLM if available
        if let Some(llm) = &self.llm_interface {
            // 🆕 FIX: Planning 步骤调用 LLM 时携带原始用户目标，避免 LLM "盲打" 导致输出质量差、耗时长。
            let mut messages: Vec<communication::Message> = Vec::new();
            if let Some(ref goal) = *self.current_plan_goal.read().await {
                messages.push(communication::Message::new(
                    uuid::Uuid::new_v4(),
                    communication::PlatformType::Custom,
                    format!("[原始用户请求] {}\n\n请基于以上请求完成以下步骤。", goal),
                ));
            }
            messages.push(communication::Message::new(
                uuid::Uuid::new_v4(),
                communication::PlatformType::Custom,
                step.description.clone(),
            ));

            match llm.call_llm(self.inject_skill_catalog(messages), None).await {
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
                data: Some(serde_json::json!({ "output": format!("Step executed successfully: {}", step.description) })),
                error: None,
                duration_ms: start_time.elapsed().as_millis() as u64,
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

    /// 🟢 P1 FIX: Handle workflow execution tasks
    pub async fn handle_workflow_task(&self, task: &Task) -> Result<(String, Vec<Artifact>), AgentError> {
        let workflow_id = task
            .parameters
            .get("workflow_id")
            .ok_or_else(|| AgentError::InvalidConfig("Missing 'workflow_id' parameter".into()))?;

        let registry = self
            .workflow_registry
            .as_ref()
            .ok_or_else(|| AgentError::InvalidConfig("Workflow registry not configured".into()))?;

        let definition = registry
            .get(workflow_id)
            .ok_or_else(|| AgentError::SkillNotFound(format!("Workflow '{}' not found", workflow_id)))?
            .clone();

        let engine = crate::workflow::WorkflowEngine::new();
        let instance = engine.execute(&definition, self, serde_json::Value::Null, None).await?;

        let mut notification = String::new();
        if definition.config.notify_on_complete {
            let notify_prompt = format!(
                "请生成一条简洁的工作流完成通知：工作流 '{}' 已执行完毕，状态：{}，共 {} 个步骤，完成度 {}%，耗时 {} 秒。",
                workflow_id,
                instance.status,
                instance.step_states.len(),
                instance.completion_pct(),
                instance.duration_secs()
            );
            match self.call_llm_prompt(notify_prompt, Some::<String>(
                "你是一个工作流通知助手，只生成简洁的完成通知消息，不超过两句话。".into()
            )).await {
                Ok(notify_text) => {
                    info!("Workflow {} notification generated: {}", workflow_id, notify_text);
                    notification = format!("\n\n📢 通知: {}", notify_text);
                }
                Err(e) => {
                    warn!("Failed to generate notification for workflow {}: {}", workflow_id, e);
                }
            }
        }

        let result = format!(
            "Workflow '{}' executed with status: {} ({} steps, {}% complete, {}s){}",
            workflow_id,
            instance.status,
            instance.step_states.len(),
            instance.completion_pct(),
            instance.duration_secs(),
            notification
        );

        let artifacts = vec![Artifact {
            id: instance.id.clone(),
            artifact_type: "workflow_instance".to_string(),
            content: serde_json::to_vec_pretty(&instance).unwrap_or_default(),
            mime_type: "application/json".to_string(),
        }];

        Ok((result, artifacts))
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
