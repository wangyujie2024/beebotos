//! DAG (Directed Acyclic Graph) Task Scheduler
//!
//! Provides explicit task dependency management with topological ordering,
//! parallel execution of independent tasks, and dynamic task replanning.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    DAG Task Scheduler                           │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
//! │  │   Workflow   │  │  Dependency  │  │   Dynamic    │          │
//! │  │   Builder    │  │    Graph     │  │   Planner    │          │
//! │  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘          │
//! │         │                 │                 │                  │
//! │         └─────────────────┴─────────────────┘                  │
//! │                           │                                     │
//! │                    ┌──────▼───────┐                            │
//! │                    │   Scheduler  │                            │
//! │                    │   Engine     │                            │
//! │                    └──────┬───────┘                            │
//! │                           │                                     │
//! │         ┌─────────────────┼─────────────────┐                  │
//! │         │                 │                 │                  │
//! │    ┌────▼────┐       ┌────▼────┐       ┌────▼────┐            │
//! │    │ Ready   │──────▶│Running  │──────▶│Completed│            │
//! │    │ Queue   │       │ Tasks   │       │/Failed  │            │
//! │    └─────────┘       └─────────┘       └─────────┘            │
//! │                                                                 │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex, RwLock, Semaphore};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::error::AgentError;
use crate::session::SessionKey;
use crate::task::{TaskResult, TaskType};

/// DAG-based workflow definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagWorkflow {
    /// Workflow ID
    pub id: String,
    /// Workflow name
    pub name: String,
    /// Workflow description
    pub description: String,
    /// Workflow tasks
    pub tasks: Vec<DagTask>,
    /// Task dependencies (task_id -> [dependency_task_ids])
    pub dependencies: HashMap<String, Vec<String>>,
    /// Workflow configuration
    pub config: WorkflowConfig,
    /// Created at
    pub created_at: DateTime<Utc>,
    /// Session key for context
    pub session_key: Option<SessionKey>,
}

/// Configuration for DAG workflow execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConfig {
    /// Maximum concurrent tasks
    pub max_concurrency: usize,
    /// Task timeout in seconds
    pub task_timeout_sec: u64,
    /// Workflow timeout in seconds
    pub workflow_timeout_sec: u64,
    /// Retry policy
    pub retry_policy: TaskRetryPolicy,
    /// Enable dynamic replanning
    pub enable_replanning: bool,
    /// Continue on task failure
    pub continue_on_failure: bool,
}

impl Default for WorkflowConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 5,
            task_timeout_sec: 300,
            workflow_timeout_sec: 3600,
            retry_policy: TaskRetryPolicy::default(),
            enable_replanning: true,
            continue_on_failure: false,
        }
    }
}

/// Task retry policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRetryPolicy {
    /// Maximum retry attempts
    pub max_retries: u32,
    /// Initial backoff in milliseconds
    pub initial_backoff_ms: u64,
    /// Maximum backoff in milliseconds
    pub max_backoff_ms: u64,
    /// Backoff multiplier
    pub backoff_multiplier: f64,
    /// Retryable error codes
    pub retryable_errors: Vec<String>,
}

impl Default for TaskRetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff_ms: 1000,
            max_backoff_ms: 30000,
            backoff_multiplier: 2.0,
            retryable_errors: vec!["timeout".to_string(), "transient".to_string()],
        }
    }
}

/// Task definition in a DAG workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagTask {
    /// Task ID
    pub id: String,
    /// Task name
    pub name: String,
    /// Task description
    pub description: String,
    /// Task type
    pub task_type: TaskType,
    /// Task input parameters
    pub parameters: HashMap<String, serde_json::Value>,
    /// Task priority
    pub priority: TaskPriority,
    /// Estimated execution time (seconds)
    pub estimated_duration_sec: Option<u64>,
    /// Required capabilities
    pub required_capabilities: Vec<String>,
    /// Resource requirements
    pub resource_requirements: ResourceRequirements,
}

/// Task priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    Lowest = 0,
    Low = 1,
    Normal = 2,
    High = 3,
    Critical = 4,
}

/// Resource requirements for a task
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceRequirements {
    /// Minimum memory in MB
    pub min_memory_mb: Option<u64>,
    /// Minimum CPU cores
    pub min_cpu_cores: Option<u32>,
    /// GPU required
    pub requires_gpu: bool,
}

/// Runtime state of a task
#[derive(Debug, Clone)]
pub struct TaskRuntimeState {
    /// Task reference
    pub task: DagTask,
    /// Current status
    pub status: TaskExecutionStatus,
    /// Retry count
    pub retry_count: u32,
    /// Start time
    pub started_at: Option<DateTime<Utc>>,
    /// End time
    pub completed_at: Option<DateTime<Utc>>,
    /// Execution result
    pub result: Option<TaskResult>,
    /// Error message
    pub error: Option<String>,
    /// Assigned executor
    pub assigned_executor: Option<String>,
}

/// Task execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskExecutionStatus {
    /// Task created but not ready (dependencies pending)
    Pending,
    /// Ready to execute
    Ready,
    /// Currently executing
    Running,
    /// Completed successfully
    Completed,
    /// Failed
    Failed,
    /// Cancelled
    Cancelled,
    /// Waiting for retry
    WaitingRetry,
}

/// Running workflow instance
#[derive(Debug, Clone)]
pub struct WorkflowInstance {
    /// Instance ID
    pub instance_id: String,
    /// Workflow definition
    pub workflow: DagWorkflow,
    /// Task states
    pub task_states: HashMap<String, TaskRuntimeState>,
    /// Current status
    pub status: WorkflowStatus,
    /// Start time
    pub started_at: Option<DateTime<Utc>>,
    /// End time
    pub completed_at: Option<DateTime<Utc>>,
    /// Execution graph (internal)
    pub execution_graph: Option<ExecutionGraph>,
}

/// Workflow status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    Pending,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

/// Execution graph for efficient dependency tracking
#[derive(Debug, Clone)]
pub struct ExecutionGraph {
    /// Graph structure
    pub graph: DiGraph<String, ()>,
    /// Node index to task ID mapping
    pub node_to_task: HashMap<NodeIndex, String>,
    /// Task ID to node index mapping
    pub task_to_node: HashMap<String, NodeIndex>,
    /// Topological order (cached)
    pub topological_order: Vec<String>,
}

/// Task execution request
#[derive(Debug, Clone)]
pub struct TaskExecutionRequest {
    /// Instance ID
    pub instance_id: String,
    /// Task ID
    pub task_id: String,
    /// Task definition
    pub task: DagTask,
    /// Session key
    pub session_key: Option<SessionKey>,
}

/// Task executor trait
#[async_trait::async_trait]
pub trait TaskExecutor: Send + Sync {
    /// Execute a task
    async fn execute(&self, request: TaskExecutionRequest) -> Result<TaskResult, AgentError>;
    
    /// Check if executor can handle a task
    fn can_execute(&self, task: &DagTask) -> bool;
    
    /// Get executor ID
    fn executor_id(&self) -> &str;
}

/// DAG Scheduler
pub struct DagScheduler {
    /// Configuration
    config: SchedulerConfig,
    /// Registered executors
    executors: Arc<RwLock<Vec<Arc<dyn TaskExecutor>>>>,
    /// Running workflow instances
    instances: Arc<RwLock<HashMap<String, WorkflowInstance>>>,
    /// Task queue (ready tasks)
    ready_queue: Arc<Mutex<VecDeque<TaskExecutionRequest>>>,
    /// Semaphore for concurrency control
    concurrency_semaphore: Arc<Semaphore>,
    /// Event sender
    event_sender: mpsc::Sender<SchedulerEvent>,
    /// Shutdown signal
    shutdown: Arc<tokio::sync::Notify>,
    /// Background tasks
    background_tasks: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

/// Scheduler configuration
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Maximum concurrent tasks
    pub max_concurrency: usize,
    /// Queue capacity
    pub queue_capacity: usize,
    /// Enable work stealing
    pub enable_work_stealing: bool,
    /// Health check interval (seconds)
    pub health_check_interval_sec: u64,
    /// Enable metrics collection
    pub enable_metrics: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 10,
            queue_capacity: 1000,
            enable_work_stealing: true,
            health_check_interval_sec: 30,
            enable_metrics: true,
        }
    }
}

/// Scheduler events
#[derive(Debug, Clone)]
pub enum SchedulerEvent {
    /// Workflow started
    WorkflowStarted { instance_id: String, workflow_id: String },
    /// Workflow completed
    WorkflowCompleted { instance_id: String, workflow_id: String, success: bool },
    /// Task status changed
    TaskStatusChanged { instance_id: String, task_id: String, from: TaskExecutionStatus, to: TaskExecutionStatus },
    /// Task completed
    TaskCompleted { instance_id: String, task_id: String, success: bool, duration_ms: u64 },
    /// Task failed (after retries)
    TaskFailed { instance_id: String, task_id: String, error: String },
    /// Dynamic replanning triggered
    ReplanningTriggered { instance_id: String, reason: String },
    /// Error occurred
    Error { instance_id: Option<String>, error: String },
}

/// Scheduler metrics
#[derive(Debug, Clone, Default)]
pub struct SchedulerMetrics {
    /// Total workflows submitted
    pub workflows_submitted: u64,
    /// Total workflows completed
    pub workflows_completed: u64,
    /// Total workflows failed
    pub workflows_failed: u64,
    /// Total tasks executed
    pub tasks_executed: u64,
    /// Total tasks failed
    pub tasks_failed: u64,
    /// Average workflow duration (ms)
    pub avg_workflow_duration_ms: f64,
    /// Average task duration (ms)
    pub avg_task_duration_ms: f64,
    /// Current running workflows
    pub running_workflows: usize,
    /// Current running tasks
    pub running_tasks: usize,
    /// Queue depth
    pub queue_depth: usize,
}

impl DagScheduler {
    /// Create new DAG scheduler
    pub fn new(config: SchedulerConfig) -> (Self, mpsc::Receiver<SchedulerEvent>) {
        let (event_sender, event_receiver) = mpsc::channel(10000);
        
        let scheduler = Self {
            config: config.clone(),
            executors: Arc::new(RwLock::new(Vec::new())),
            instances: Arc::new(RwLock::new(HashMap::new())),
            ready_queue: Arc::new(Mutex::new(VecDeque::with_capacity(config.queue_capacity))),
            concurrency_semaphore: Arc::new(Semaphore::new(config.max_concurrency)),
            event_sender,
            shutdown: Arc::new(tokio::sync::Notify::new()),
            background_tasks: Arc::new(Mutex::new(Vec::new())),
        };

        (scheduler, event_receiver)
    }

    /// Register a task executor
    pub async fn register_executor(&self, executor: Arc<dyn TaskExecutor>) {
        let mut executors = self.executors.write().await;
        executors.push(executor);
        info!("Registered task executor");
    }

    /// Start the scheduler
    pub async fn start(&self) {
        info!("Starting DAG scheduler...");
        
        // Start worker tasks
        for i in 0..self.config.max_concurrency {
            self.start_worker(i).await;
        }
        
        // Start health monitor
        self.start_health_monitor().await;
        
        info!("DAG scheduler started with {} workers", self.config.max_concurrency);
    }

    /// Start a worker task
    async fn start_worker(&self, worker_id: usize) {
        let ready_queue = self.ready_queue.clone();
        let executors = self.executors.clone();
        let instances = self.instances.clone();
        let semaphore = self.concurrency_semaphore.clone();
        let event_sender = self.event_sender.clone();
        let shutdown = self.shutdown.clone();

        let handle = tokio::spawn(async move {
            info!("Worker {} started", worker_id);
            
            loop {
                tokio::select! {
                    _ = shutdown.notified() => {
                        info!("Worker {} shutting down", worker_id);
                        break;
                    }
                    _ = async {
                        // Acquire semaphore permit
                        let _permit = semaphore.acquire().await.unwrap();
                        
                        // Get next ready task
                        let request = {
                            let mut queue = ready_queue.lock().await;
                            queue.pop_front()
                        };
                        
                        if let Some(request) = request {
                            debug!("Worker {} executing task {}", worker_id, request.task_id);
                            
                            // Find suitable executor
                            let executor = {
                                let executors = executors.read().await;
                                executors.iter()
                                    .find(|e| e.can_execute(&request.task))
                                    .cloned()
                            };
                            
                            if let Some(executor) = executor {
                                // Update task status to running
                                Self::update_task_status(
                                    &instances,
                                    &request.instance_id,
                                    &request.task_id,
                                    TaskExecutionStatus::Running,
                                    Some(executor.executor_id().to_string()),
                                    &event_sender,
                                ).await;
                                
                                let start_time = std::time::Instant::now();
                                
                                // Execute task
                                let result = executor.execute(request.clone()).await;
                                let duration_ms = start_time.elapsed().as_millis() as u64;
                                
                                match result {
                                    Ok(task_result) => {
                                        Self::complete_task(
                                            &instances,
                                            &ready_queue,
                                            &request,
                                            task_result,
                                            duration_ms,
                                            &event_sender,
                                        ).await;
                                    }
                                    Err(e) => {
                                        Self::fail_task(
                                            &instances,
                                            &ready_queue,
                                            &request,
                                            e.to_string(),
                                            duration_ms,
                                            &event_sender,
                                        ).await;
                                    }
                                }
                            } else {
                                warn!("No executor found for task {}", request.task_id);
                                Self::fail_task(
                                    &instances,
                                    &ready_queue,
                                    &request,
                                    "No suitable executor found".to_string(),
                                    0,
                                    &event_sender,
                                ).await;
                            }
                        } else {
                            // No tasks available, sleep briefly
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                    } => {}
                }
            }
        });

        self.background_tasks.lock().await.push(handle);
    }

    /// Start health monitor
    async fn start_health_monitor(&self) {
        let instances = self.instances.clone();
        let event_sender = self.event_sender.clone();
        let shutdown = self.shutdown.clone();
        let interval = self.config.health_check_interval_sec;

        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(interval));
            
            loop {
                tokio::select! {
                    _ = shutdown.notified() => break,
                    _ = ticker.tick() => {
                        Self::check_workflow_health(&instances, &event_sender).await;
                    }
                }
            }
        });

        self.background_tasks.lock().await.push(handle);
    }

    /// Check workflow health
    async fn check_workflow_health(
        instances: &Arc<RwLock<HashMap<String, WorkflowInstance>>>,
        event_sender: &mpsc::Sender<SchedulerEvent>,
    ) {
        let now = Utc::now();
        let mut instances_guard = instances.write().await;
        
        for (instance_id, instance) in instances_guard.iter_mut() {
            if instance.status != WorkflowStatus::Running {
                continue;
            }
            
            // Check for timed out tasks
            for (task_id, state) in &mut instance.task_states {
                if state.status == TaskExecutionStatus::Running {
                    if let Some(started_at) = state.started_at {
                        let elapsed = now.signed_duration_since(started_at).num_seconds() as u64;
                        let timeout = instance.workflow.config.task_timeout_sec;
                        
                        if elapsed > timeout {
                            warn!(
                                "Task {} in workflow {} timed out after {}s",
                                task_id, instance_id, elapsed
                            );
                            
                            state.status = TaskExecutionStatus::Failed;
                            state.error = Some(format!("Task timed out after {}s", elapsed));
                            state.completed_at = Some(now);
                            
                            let _ = event_sender.send(SchedulerEvent::TaskFailed {
                                instance_id: instance_id.clone(),
                                task_id: task_id.clone(),
                                error: "Task timeout".to_string(),
                            }).await;
                        }
                    }
                }
            }
        }
    }

    /// Submit a workflow for execution
    pub async fn submit_workflow(&self, workflow: DagWorkflow) -> Result<WorkflowInstance, SchedulerError> {
        // Validate workflow (check for cycles)
        self.validate_workflow(&workflow)?;
        
        let instance_id = Uuid::new_v4().to_string();
        
        // Build execution graph
        let execution_graph = self.build_execution_graph(&workflow);
        
        // Initialize task states
        let mut task_states = HashMap::new();
        for task in &workflow.tasks {
            task_states.insert(
                task.id.clone(),
                TaskRuntimeState {
                    task: task.clone(),
                    status: TaskExecutionStatus::Pending,
                    retry_count: 0,
                    started_at: None,
                    completed_at: None,
                    result: None,
                    error: None,
                    assigned_executor: None,
                },
            );
        }
        
        // Mark tasks with no dependencies as ready
        let mut ready_tasks = Vec::new();
        for task in &workflow.tasks {
            let deps = workflow.dependencies.get(&task.id).cloned().unwrap_or_default();
            if deps.is_empty() {
                task_states.get_mut(&task.id).unwrap().status = TaskExecutionStatus::Ready;
                ready_tasks.push(task.id.clone());
            }
        }
        
        let instance = WorkflowInstance {
            instance_id: instance_id.clone(),
            workflow: workflow.clone(),
            task_states,
            status: WorkflowStatus::Pending,
            started_at: None,
            completed_at: None,
            execution_graph: Some(execution_graph),
        };
        
        // Store instance
        {
            let mut instances = self.instances.write().await;
            instances.insert(instance_id.clone(), instance.clone());
        }
        
        // Emit event
        let _ = self.event_sender.send(SchedulerEvent::WorkflowStarted {
            instance_id: instance_id.clone(),
            workflow_id: workflow.id.clone(),
        }).await;
        
        info!("Submitted workflow {} as instance {}", workflow.id, instance_id);
        
        // Start execution
        self.start_workflow_execution(&instance_id).await?;
        
        Ok(instance)
    }

    /// Validate workflow (check for cycles and missing dependencies)
    fn validate_workflow(&self, workflow: &DagWorkflow) -> Result<(), SchedulerError> {
        let mut graph = DiGraph::<&str, ()>::new();
        let mut node_indices = HashMap::new();
        
        // Add nodes
        for task in &workflow.tasks {
            let idx = graph.add_node(task.id.as_str());
            node_indices.insert(task.id.as_str(), idx);
        }
        
        // Add edges
        for (task_id, deps) in &workflow.dependencies {
            let task_idx = node_indices.get(task_id.as_str())
                .ok_or_else(|| SchedulerError::InvalidWorkflow(
                    format!("Task {} not found", task_id)
                ))?;
            
            for dep in deps {
                let dep_idx = node_indices.get(dep.as_str())
                    .ok_or_else(|| SchedulerError::InvalidWorkflow(
                        format!("Dependency {} not found for task {}", dep, task_id)
                    ))?;
                
                graph.add_edge(*dep_idx, *task_idx, ());
            }
        }
        
        // Check for cycles using topological sort
        if petgraph::algo::toposort(&graph, None).is_err() {
            return Err(SchedulerError::InvalidWorkflow(
                "Workflow contains cycles".to_string()
            ));
        }
        
        Ok(())
    }

    /// Build execution graph for efficient dependency tracking
    fn build_execution_graph(&self, workflow: &DagWorkflow) -> ExecutionGraph {
        let mut graph = DiGraph::<String, ()>::new();
        let mut task_to_node = HashMap::new();
        let mut node_to_task = HashMap::new();
        
        // Add nodes
        for task in &workflow.tasks {
            let idx = graph.add_node(task.id.clone());
            task_to_node.insert(task.id.clone(), idx);
            node_to_task.insert(idx, task.id.clone());
        }
        
        // Add edges (dependency -> task)
        for (task_id, deps) in &workflow.dependencies {
            let task_idx = task_to_node[task_id];
            for dep in deps {
                let dep_idx = task_to_node[dep];
                graph.add_edge(dep_idx, task_idx, ());
            }
        }
        
        // Compute topological order
        let topological_order = match toposort(&graph, None) {
            Ok(order) => order.iter().map(|node| node_to_task[node].clone()).collect(),
            Err(_) => Vec::new(), // Cycle detected, will be caught in validation
        };
        
        ExecutionGraph {
            graph,
            node_to_task,
            task_to_node,
            topological_order,
        }
    }

    /// Start workflow execution
    async fn start_workflow_execution(&self, instance_id: &str) -> Result<(), SchedulerError> {
        let mut instances = self.instances.write().await;
        
        let instance = instances.get_mut(instance_id)
            .ok_or_else(|| SchedulerError::InstanceNotFound(instance_id.to_string()))?;
        
        instance.status = WorkflowStatus::Running;
        instance.started_at = Some(Utc::now());
        
        // Queue ready tasks
        drop(instances); // Release lock before async operations
        
        self.queue_ready_tasks(instance_id).await?;
        
        Ok(())
    }

    /// Queue all ready tasks for execution
    async fn queue_ready_tasks(&self, instance_id: &str) -> Result<(), SchedulerError> {
        let instances = self.instances.read().await;
        
        let instance = instances.get(instance_id)
            .ok_or_else(|| SchedulerError::InstanceNotFound(instance_id.to_string()))?;
        
        let ready_tasks: Vec<_> = instance.task_states.iter()
            .filter(|(_, state)| state.status == TaskExecutionStatus::Ready)
            .map(|(id, state)| (id.clone(), state.task.clone()))
            .collect();
        
        drop(instances);
        
        for (task_id, task) in ready_tasks {
            let request = TaskExecutionRequest {
                instance_id: instance_id.to_string(),
                task_id: task_id.clone(),
                task,
                session_key: None, // Could be passed from workflow
            };
            
            let mut queue = self.ready_queue.lock().await;
            queue.push_back(request);
            
            debug!("Queued task {} for execution", task_id);
        }
        
        Ok(())
    }

    /// Update task status
    async fn update_task_status(
        instances: &Arc<RwLock<HashMap<String, WorkflowInstance>>>,
        instance_id: &str,
        task_id: &str,
        status: TaskExecutionStatus,
        assigned_executor: Option<String>,
        event_sender: &mpsc::Sender<SchedulerEvent>,
    ) {
        let mut instances_guard = instances.write().await;
        
        if let Some(instance) = instances_guard.get_mut(instance_id) {
            if let Some(state) = instance.task_states.get_mut(task_id) {
                let old_status = state.status;
                state.status = status;
                state.assigned_executor = assigned_executor;
                
                if status == TaskExecutionStatus::Running {
                    state.started_at = Some(Utc::now());
                }
                
                let _ = event_sender.send(SchedulerEvent::TaskStatusChanged {
                    instance_id: instance_id.to_string(),
                    task_id: task_id.to_string(),
                    from: old_status,
                    to: status,
                }).await;
            }
        }
    }

    /// Complete a task
    async fn complete_task(
        instances: &Arc<RwLock<HashMap<String, WorkflowInstance>>>,
        ready_queue: &Arc<Mutex<VecDeque<TaskExecutionRequest>>>,
        request: &TaskExecutionRequest,
        result: TaskResult,
        duration_ms: u64,
        event_sender: &mpsc::Sender<SchedulerEvent>,
    ) {
        let mut instances_guard = instances.write().await;
        
        if let Some(instance) = instances_guard.get_mut(&request.instance_id) {
            if let Some(state) = instance.task_states.get_mut(&request.task_id) {
                state.status = TaskExecutionStatus::Completed;
                state.completed_at = Some(Utc::now());
                state.result = Some(result);
                
                let _ = event_sender.send(SchedulerEvent::TaskCompleted {
                    instance_id: request.instance_id.clone(),
                    task_id: request.task_id.clone(),
                    success: true,
                    duration_ms,
                }).await;
            }
        }
        
        drop(instances_guard);
        
        // Update downstream tasks
        Self::update_downstream_tasks(instances, ready_queue, &request.instance_id, &request.task_id, event_sender).await;
        
        // Check if workflow is complete
        Self::check_workflow_completion(instances, &request.instance_id, event_sender).await;
    }

    /// Fail a task
    async fn fail_task(
        instances: &Arc<RwLock<HashMap<String, WorkflowInstance>>>,
        ready_queue: &Arc<Mutex<VecDeque<TaskExecutionRequest>>>,
        request: &TaskExecutionRequest,
        error: String,
        _duration_ms: u64,
        event_sender: &mpsc::Sender<SchedulerEvent>,
    ) {
        let mut instances_guard = instances.write().await;
        
        let (should_replan, continue_on_failure) = if let Some(instance) = instances_guard.get_mut(&request.instance_id) {
            if let Some(state) = instance.task_states.get_mut(&request.task_id) {
                // Check if we should retry
                if state.retry_count < instance.workflow.config.retry_policy.max_retries {
                    state.retry_count += 1;
                    state.status = TaskExecutionStatus::WaitingRetry;
                    
                    // Schedule retry with backoff
                    let backoff = instance.workflow.config.retry_policy.initial_backoff_ms 
                        * (instance.workflow.config.retry_policy.backoff_multiplier.powi(state.retry_count as i32) as u64);
                    
                    info!("Scheduling retry {} for task {} in {}ms", state.retry_count, request.task_id, backoff);
                    
                    // For simplicity, just mark as ready again
                    state.status = TaskExecutionStatus::Ready;
                    (true, false)
                } else {
                    state.status = TaskExecutionStatus::Failed;
                    state.completed_at = Some(Utc::now());
                    state.error = Some(error.clone());
                    
                    let _ = event_sender.send(SchedulerEvent::TaskFailed {
                        instance_id: request.instance_id.clone(),
                        task_id: request.task_id.clone(),
                        error: error.clone(),
                    }).await;
                    
                    let should_replan = instance.workflow.config.enable_replanning && !instance.workflow.config.continue_on_failure;
                    (should_replan, instance.workflow.config.continue_on_failure)
                }
            } else {
                (false, false)
            }
        } else {
            (false, false)
        };
        
        drop(instances_guard);
        
        if should_replan {
            // Trigger dynamic replanning
            let _ = event_sender.send(SchedulerEvent::ReplanningTriggered {
                instance_id: request.instance_id.clone(),
                reason: format!("Task {} failed: {}", request.task_id, error),
            }).await;
        }
        
        // If continue_on_failure is enabled, allow downstream tasks to proceed
        if continue_on_failure {
            Self::update_downstream_tasks(instances, ready_queue, &request.instance_id, &request.task_id, event_sender).await;
        }
        
        // Check if workflow should fail
        Self::check_workflow_completion(instances, &request.instance_id, event_sender).await;
    }

    /// Update downstream tasks after a task completes or fails with continue_on_failure
    async fn update_downstream_tasks(
        instances: &Arc<RwLock<HashMap<String, WorkflowInstance>>>,
        ready_queue: &Arc<Mutex<VecDeque<TaskExecutionRequest>>>,
        instance_id: &str,
        completed_task_id: &str,
        _event_sender: &mpsc::Sender<SchedulerEvent>,
    ) {
        let instances_guard = instances.read().await;
        
        let downstream_tasks = if let Some(instance) = instances_guard.get(instance_id) {
            if let Some(ref graph) = instance.execution_graph {
                let completed_node = graph.task_to_node.get(completed_task_id);
                
                if let Some(&node) = completed_node {
                    // Get neighbors (dependent tasks)
                    graph.graph.neighbors(node)
                        .filter_map(|n| graph.node_to_task.get(&n).cloned())
                        .collect::<Vec<_>>()
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
        } else {
            vec![]
        };
        
        let ready_tasks: Vec<_> = downstream_tasks.iter()
            .filter_map(|task_id| {
                if let Some(instance) = instances_guard.get(instance_id) {
                    if let Some(state) = instance.task_states.get(task_id) {
                        if state.status == TaskExecutionStatus::Pending {
                            // Check if all dependencies are completed (or failed with continue_on_failure)
                            let deps = instance.workflow.dependencies.get(task_id)
                                .cloned()
                                .unwrap_or_default();
                            
                            let all_deps_ready = deps.iter().all(|dep| {
                                instance.task_states.get(dep)
                                    .map(|s| {
                                        matches!(s.status, TaskExecutionStatus::Completed | TaskExecutionStatus::Failed)
                                    })
                                    .unwrap_or(false)
                            });
                            
                            if all_deps_ready {
                                return Some(task_id.clone());
                            }
                        }
                    }
                }
                None
            })
            .collect();
        
        // Collect task clones for newly ready tasks before dropping read lock
        let ready_task_clones: Vec<DagTask> = ready_tasks.iter()
            .filter_map(|task_id| {
                instances_guard.get(instance_id)
                    .and_then(|i| i.task_states.get(task_id))
                    .map(|s| s.task.clone())
            })
            .collect();
        
        drop(instances_guard);
        
        // Mark ready tasks
        let mut instances_guard = instances.write().await;
        for task_id in &ready_tasks {
            if let Some(instance) = instances_guard.get_mut(instance_id) {
                if let Some(state) = instance.task_states.get_mut(task_id) {
                    state.status = TaskExecutionStatus::Ready;
                }
            }
        }
        
        drop(instances_guard);
        
        // Queue ready tasks into the ready queue
        let mut queue = ready_queue.lock().await;
        for (task_id, task) in ready_tasks.into_iter().zip(ready_task_clones.into_iter()) {
            let request = TaskExecutionRequest {
                instance_id: instance_id.to_string(),
                task_id: task_id.clone(),
                task,
                session_key: None,
            };
            queue.push_back(request);
            debug!("Queued downstream task {} for instance {}", task_id, instance_id);
        }
    }

    /// Check if workflow is complete
    async fn check_workflow_completion(
        instances: &Arc<RwLock<HashMap<String, WorkflowInstance>>>,
        instance_id: &str,
        event_sender: &mpsc::Sender<SchedulerEvent>,
    ) {
        let mut instances_guard = instances.write().await;
        
        if let Some(instance) = instances_guard.get_mut(instance_id) {
            let all_completed = instance.task_states.values().all(|s| {
                matches!(s.status, TaskExecutionStatus::Completed | TaskExecutionStatus::Failed | TaskExecutionStatus::Cancelled)
            });
            
            if all_completed {
                let any_failed = instance.task_states.values()
                    .any(|s| s.status == TaskExecutionStatus::Failed);
                
                instance.status = if any_failed {
                    WorkflowStatus::Failed
                } else {
                    WorkflowStatus::Completed
                };
                instance.completed_at = Some(Utc::now());
                
                let _ = event_sender.send(SchedulerEvent::WorkflowCompleted {
                    instance_id: instance_id.to_string(),
                    workflow_id: instance.workflow.id.clone(),
                    success: !any_failed,
                }).await;
            }
        }
    }

    /// Get workflow instance
    pub async fn get_instance(&self, instance_id: &str) -> Option<WorkflowInstance> {
        let instances = self.instances.read().await;
        instances.get(instance_id).cloned()
    }

    /// Get task result from a workflow instance
    pub async fn get_task_result(&self, instance_id: &str, task_id: &str) -> Option<TaskResult> {
        let instances = self.instances.read().await;
        instances.get(instance_id)
            .and_then(|i| i.task_states.get(task_id))
            .and_then(|s| s.result.clone())
    }

    /// Get scheduler metrics
    pub async fn get_metrics(&self) -> SchedulerMetrics {
        let instances = self.instances.read().await;
        let queue = self.ready_queue.lock().await;
        
        let running_workflows = instances.values()
            .filter(|i| i.status == WorkflowStatus::Running)
            .count();
        
        let running_tasks = instances.values()
            .flat_map(|i| i.task_states.values())
            .filter(|s| s.status == TaskExecutionStatus::Running)
            .count();
        
        SchedulerMetrics {
            workflows_submitted: instances.len() as u64,
            running_workflows,
            running_tasks,
            queue_depth: queue.len(),
            ..Default::default()
        }
    }

    /// Graceful shutdown
    pub async fn shutdown(&self) {
        info!("Initiating DAG scheduler graceful shutdown...");
        
        self.shutdown.notify_waiters();
        
        let mut tasks = self.background_tasks.lock().await;
        for handle in tasks.drain(..) {
            if let Err(e) = handle.await {
                error!("Worker task panicked: {}", e);
            }
        }
        
        info!("DAG scheduler shutdown complete");
    }
}

/// Scheduler errors
#[derive(Debug, thiserror::Error, Clone)]
pub enum SchedulerError {
    #[error("Workflow validation failed: {0}")]
    InvalidWorkflow(String),
    #[error("Instance not found: {0}")]
    InstanceNotFound(String),
    #[error("Task not found: {0}")]
    TaskNotFound(String),
    #[error("No suitable executor found for task: {0}")]
    NoExecutor(String),
    #[error("Cycle detected in workflow dependencies")]
    CycleDetected,
    #[error("Scheduler error: {0}")]
    Other(String),
}

impl From<AgentError> for SchedulerError {
    fn from(e: AgentError) -> Self {
        SchedulerError::Other(e.to_string())
    }
}

/// DAG Workflow Builder
pub struct DagWorkflowBuilder {
    workflow: DagWorkflow,
    current_dependencies: Vec<String>,
}

impl DagWorkflowBuilder {
    /// Create new workflow builder
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            workflow: DagWorkflow {
                id: Uuid::new_v4().to_string(),
                name: name.into(),
                description: String::new(),
                tasks: Vec::new(),
                dependencies: HashMap::new(),
                config: WorkflowConfig::default(),
                created_at: Utc::now(),
                session_key: None,
            },
            current_dependencies: Vec::new(),
        }
    }

    /// Set description
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.workflow.description = desc.into();
        self
    }

    /// Set configuration
    pub fn config(mut self, config: WorkflowConfig) -> Self {
        self.workflow.config = config;
        self
    }

    /// Add a task
    pub fn add_task(mut self, task: DagTask) -> Self {
        let task_id = task.id.clone();
        self.workflow.tasks.push(task);
        
        // Set dependencies if any
        if !self.current_dependencies.is_empty() {
            self.workflow.dependencies.insert(task_id, self.current_dependencies.clone());
        }
        
        self
    }

    /// Set dependencies for subsequent tasks
    pub fn depends_on(mut self, task_ids: Vec<String>) -> Self {
        self.current_dependencies = task_ids;
        self
    }

    /// Build the workflow
    pub fn build(self) -> DagWorkflow {
        self.workflow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockExecutor;

    #[async_trait::async_trait]
    impl TaskExecutor for MockExecutor {
        async fn execute(&self, request: TaskExecutionRequest) -> Result<TaskResult, AgentError> {
            Ok(TaskResult {
                task_id: request.task_id,
                success: true,
                output: "Success".to_string(),
                artifacts: vec![],
                execution_time_ms: 100,
            })
        }

        fn can_execute(&self, _task: &DagTask) -> bool {
            true
        }

        fn executor_id(&self) -> &str {
            "mock"
        }
    }

    #[tokio::test]
    async fn test_dag_scheduler_creation() {
        use tokio::time::{timeout, Duration};
        
        let config = SchedulerConfig {
            max_concurrency: 1, // Use single worker for faster test
            health_check_interval_sec: 1, // Short health check interval
            ..SchedulerConfig::default()
        };
        let (scheduler, _receiver) = DagScheduler::new(config);
        
        scheduler.start().await;
        
        // Give workers time to start listening for shutdown
        tokio::time::sleep(Duration::from_millis(50)).await;
        
        // Shutdown with timeout
        let result = timeout(Duration::from_secs(5), scheduler.shutdown()).await;
        assert!(result.is_ok(), "shutdown() timed out");
    }

    #[tokio::test]
    async fn test_workflow_validation() {
        let config = SchedulerConfig::default();
        let (scheduler, _receiver) = DagScheduler::new(config);
        
        // Valid workflow
        let valid_workflow = DagWorkflow {
            id: "test".to_string(),
            name: "Test".to_string(),
            description: String::new(),
            tasks: vec![
                DagTask {
                    id: "task1".to_string(),
                    name: "Task 1".to_string(),
                    description: String::new(),
                    task_type: TaskType::LlmChat,
                    parameters: HashMap::new(),
                    priority: TaskPriority::Normal,
                    estimated_duration_sec: None,
                    required_capabilities: vec![],
                    resource_requirements: ResourceRequirements::default(),
                },
            ],
            dependencies: HashMap::new(),
            config: WorkflowConfig::default(),
            created_at: Utc::now(),
            session_key: None,
        };
        
        assert!(scheduler.validate_workflow(&valid_workflow).is_ok());
        
        // Invalid workflow (cycle)
        let cyclic_workflow = DagWorkflow {
            id: "test".to_string(),
            name: "Test".to_string(),
            description: String::new(),
            tasks: vec![
                DagTask {
                    id: "task1".to_string(),
                    name: "Task 1".to_string(),
                    description: String::new(),
                    task_type: TaskType::LlmChat,
                    parameters: HashMap::new(),
                    priority: TaskPriority::Normal,
                    estimated_duration_sec: None,
                    required_capabilities: vec![],
                    resource_requirements: ResourceRequirements::default(),
                },
                DagTask {
                    id: "task2".to_string(),
                    name: "Task 2".to_string(),
                    description: String::new(),
                    task_type: TaskType::LlmChat,
                    parameters: HashMap::new(),
                    priority: TaskPriority::Normal,
                    estimated_duration_sec: None,
                    required_capabilities: vec![],
                    resource_requirements: ResourceRequirements::default(),
                },
            ],
            dependencies: {
                let mut deps = HashMap::new();
                deps.insert("task1".to_string(), vec!["task2".to_string()]);
                deps.insert("task2".to_string(), vec!["task1".to_string()]);
                deps
            },
            config: WorkflowConfig::default(),
            created_at: Utc::now(),
            session_key: None,
        };
        
        assert!(scheduler.validate_workflow(&cyclic_workflow).is_err());
    }

    #[tokio::test]
    async fn test_workflow_builder() {
        let workflow = DagWorkflowBuilder::new("Test Workflow")
            .description("A test workflow")
            .add_task(DagTask {
                id: "task1".to_string(),
                name: "Task 1".to_string(),
                description: String::new(),
                task_type: TaskType::LlmChat,
                parameters: HashMap::new(),
                priority: TaskPriority::Normal,
                estimated_duration_sec: None,
                required_capabilities: vec![],
                resource_requirements: ResourceRequirements::default(),
            })
            .add_task(DagTask {
                id: "task2".to_string(),
                name: "Task 2".to_string(),
                description: String::new(),
                task_type: TaskType::LlmChat,
                parameters: HashMap::new(),
                priority: TaskPriority::Normal,
                estimated_duration_sec: None,
                required_capabilities: vec![],
                resource_requirements: ResourceRequirements::default(),
            })
            .build();
        
        assert_eq!(workflow.name, "Test Workflow");
        assert_eq!(workflow.tasks.len(), 2);
    }

    #[tokio::test]
    async fn test_multi_level_dag_execution() {
        use tokio::time::{timeout, Duration};

        let config = SchedulerConfig {
            max_concurrency: 2,
            health_check_interval_sec: 1,
            ..SchedulerConfig::default()
        };
        let (scheduler, mut receiver) = DagScheduler::new(config);

        scheduler.start().await;
        scheduler.register_executor(Arc::new(MockExecutor)).await;

        // Build a 3-level DAG:
        //       task1
        //       /   \
        //    task2  task3
        //       \   /
        //       task4
        let workflow = DagWorkflowBuilder::new("Multi-Level DAG")
            .description("Tests downstream task queuing")
            .add_task(DagTask {
                id: "task1".to_string(),
                name: "Root Task".to_string(),
                description: String::new(),
                task_type: TaskType::LlmChat,
                parameters: HashMap::new(),
                priority: TaskPriority::Normal,
                estimated_duration_sec: None,
                required_capabilities: vec![],
                resource_requirements: ResourceRequirements::default(),
            })
            .depends_on(vec!["task1".to_string()])
            .add_task(DagTask {
                id: "task2".to_string(),
                name: "Level 2 A".to_string(),
                description: String::new(),
                task_type: TaskType::LlmChat,
                parameters: HashMap::new(),
                priority: TaskPriority::Normal,
                estimated_duration_sec: None,
                required_capabilities: vec![],
                resource_requirements: ResourceRequirements::default(),
            })
            .depends_on(vec!["task1".to_string()])
            .add_task(DagTask {
                id: "task3".to_string(),
                name: "Level 2 B".to_string(),
                description: String::new(),
                task_type: TaskType::LlmChat,
                parameters: HashMap::new(),
                priority: TaskPriority::Normal,
                estimated_duration_sec: None,
                required_capabilities: vec![],
                resource_requirements: ResourceRequirements::default(),
            })
            .depends_on(vec!["task2".to_string(), "task3".to_string()])
            .add_task(DagTask {
                id: "task4".to_string(),
                name: "Leaf Task".to_string(),
                description: String::new(),
                task_type: TaskType::LlmChat,
                parameters: HashMap::new(),
                priority: TaskPriority::Normal,
                estimated_duration_sec: None,
                required_capabilities: vec![],
                resource_requirements: ResourceRequirements::default(),
            })
            .build();

        let instance = scheduler.submit_workflow(workflow).await.unwrap();
        let instance_id = instance.instance_id;

        // Wait for workflow completion event
        let completed = timeout(Duration::from_secs(5), async {
            while let Some(event) = receiver.recv().await {
                if let SchedulerEvent::WorkflowCompleted { instance_id: id, success, .. } = &event {
                    if id == &instance_id {
                        return *success;
                    }
                }
            }
            false
        }).await;

        assert!(completed.is_ok(), "Workflow timed out — downstream tasks were never queued");
        assert!(completed.unwrap(), "Workflow failed");

        // Verify all tasks completed
        let final_instance = scheduler.get_instance(&instance_id).await.unwrap();
        assert_eq!(final_instance.task_states.len(), 4);
        for (task_id, state) in &final_instance.task_states {
            assert_eq!(
                state.status,
                TaskExecutionStatus::Completed,
                "Task {} did not complete (status: {:?})",
                task_id, state.status
            );
        }

        scheduler.shutdown().await;
    }
}
