//! Task Executor Module
//!
//! 提供批量任务执行功能，支持并行处理和结果聚合。

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, Semaphore};
use tracing::{error, info};

use crate::error::{AgentError, Result};
use crate::{Artifact, Task, TaskResult, TaskType};

/// 批量执行结果
#[derive(Debug, Clone)]
pub struct BatchResult {
    /// 成功结果
    pub successes: Vec<TaskResult>,
    /// 失败结果
    pub failures: Vec<(String, AgentError)>, // (task_id, error)
    /// 总执行时间
    pub total_duration: Duration,
    /// 成功数量
    pub success_count: usize,
    /// 失败数量
    pub failure_count: usize,
}

impl BatchResult {
    /// 创建新的批量结果
    pub fn new() -> Self {
        Self {
            successes: vec![],
            failures: vec![],
            total_duration: Duration::default(),
            success_count: 0,
            failure_count: 0,
        }
    }

    /// 添加成功结果
    pub fn add_success(&mut self, result: TaskResult) {
        self.successes.push(result);
        self.success_count += 1;
    }

    /// 添加失败结果
    pub fn add_failure(&mut self, task_id: String, error: AgentError) {
        self.failures.push((task_id, error));
        self.failure_count += 1;
    }

    /// 是否全部成功
    pub fn all_succeeded(&self) -> bool {
        self.failure_count == 0
    }

    /// 成功率
    pub fn success_rate(&self) -> f64 {
        let total = self.success_count + self.failure_count;
        if total == 0 {
            0.0
        } else {
            self.success_count as f64 / total as f64
        }
    }

    /// 获取按任务类型分组的统计
    pub fn stats_by_type(&self) -> HashMap<String, (usize, usize)> {
        let mut stats: HashMap<String, (usize, usize)> = HashMap::new();

        for success in &self.successes {
            let entry = stats.entry(success.task_id.clone()).or_insert((0, 0));
            entry.0 += 1;
        }

        for (task_id, _) in &self.failures {
            let entry = stats.entry(task_id.clone()).or_insert((0, 0));
            entry.1 += 1;
        }

        stats
    }
}

impl Default for BatchResult {
    fn default() -> Self {
        Self::new()
    }
}

/// 任务执行器 trait
#[async_trait::async_trait]
pub trait TaskExecutor: Send + Sync {
    /// 执行单个任务
    async fn execute(&self, task: Task) -> Result<TaskResult>;

    /// 批量执行任务
    async fn execute_batch(&self, tasks: Vec<Task>) -> BatchResult {
        let start = Instant::now();
        let mut result = BatchResult::new();

        for task in tasks {
            let task_id = task.id.clone();
            match self.execute(task).await {
                Ok(task_result) => result.add_success(task_result),
                Err(e) => result.add_failure(task_id, e),
            }
        }

        result.total_duration = start.elapsed();
        result
    }
}

/// 批量执行器 - 支持并行执行
pub struct BatchExecutor {
    /// 最大并发数
    max_concurrency: usize,
    /// 批次超时时间
    batch_timeout: Duration,
    /// 任务超时时间
    task_timeout: Duration,
}

impl BatchExecutor {
    /// 创建新的批量执行器
    pub fn new(max_concurrency: usize) -> Self {
        Self {
            max_concurrency,
            batch_timeout: Duration::from_secs(300),
            task_timeout: Duration::from_secs(180),
        }
    }

    /// 设置批次超时
    pub fn with_batch_timeout(mut self, timeout: Duration) -> Self {
        self.batch_timeout = timeout;
        self
    }

    /// 设置任务超时
    pub fn with_task_timeout(mut self, timeout: Duration) -> Self {
        self.task_timeout = timeout;
        self
    }

    /// 并行执行批量任务
    ///
    /// # Type Parameters
    /// * `F` - 任务执行函数类型
    /// * `Fut` - 异步返回类型
    ///
    /// # Arguments
    /// * `tasks` - 任务列表
    /// * `executor` - 任务执行函数
    ///
    /// # Returns
    /// 批量执行结果
    pub async fn execute_parallel<F, Fut>(&self, tasks: Vec<Task>, executor: F) -> BatchResult
    where
        F: Fn(Task) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future<Output = Result<TaskResult>> + Send,
    {
        let start = Instant::now();
        let semaphore = Arc::new(Semaphore::new(self.max_concurrency));
        let task_timeout = self.task_timeout;

        info!(
            "Starting parallel execution of {} tasks (max_concurrency: {})",
            tasks.len(),
            self.max_concurrency
        );

        // 创建所有任务的 Future
        let mut handles = Vec::with_capacity(tasks.len());

        for task in tasks {
            let permit = semaphore.clone().acquire_owned().await.ok();
            let exec = executor.clone();

            let handle = tokio::spawn(async move {
                let task_id = task.id.clone();
                let result = tokio::time::timeout(task_timeout, exec(task)).await;

                drop(permit);

                match result {
                    Ok(Ok(task_result)) => Ok(task_result),
                    Ok(Err(e)) => Err((task_id, e)),
                    Err(_) => Err((task_id, AgentError::Timeout("Task timed out".to_string()))),
                }
            });

            handles.push(handle);
        }

        // 收集结果
        let mut batch_result = BatchResult::new();

        for handle in handles {
            match handle.await {
                Ok(Ok(result)) => batch_result.add_success(result),
                Ok(Err((task_id, error))) => batch_result.add_failure(task_id, error),
                Err(e) => {
                    error!("Task panicked: {}", e);
                    batch_result.add_failure(
                        "unknown".to_string(),
                        AgentError::Execution(format!("Task panicked: {}", e)),
                    );
                }
            }
        }

        batch_result.total_duration = start.elapsed();

        info!(
            "Batch execution completed: {}/{} succeeded in {:?}",
            batch_result.success_count,
            batch_result.success_count + batch_result.failure_count,
            batch_result.total_duration
        );

        batch_result
    }

    /// 流式批量执行
    ///
    /// 边执行边返回结果，适用于大批量任务
    pub async fn execute_streaming<F, Fut>(
        &self,
        tasks: Vec<Task>,
        executor: F,
    ) -> mpsc::Receiver<Result<TaskResult>>
    where
        F: Fn(Task) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future<Output = Result<TaskResult>> + Send,
    {
        let (tx, rx) = mpsc::channel(tasks.len());
        let semaphore = Arc::new(Semaphore::new(self.max_concurrency));

        info!("Starting streaming execution of {} tasks", tasks.len());

        tokio::spawn(async move {
            for task in tasks {
                let tx = tx.clone();
                let permit = semaphore.clone().acquire_owned().await.ok();
                let exec = executor.clone();

                tokio::spawn(async move {
                    let result = tokio::time::timeout(Duration::from_secs(180), exec(task)).await;

                    let task_result = match result {
                        Ok(r) => r,
                        Err(_) => Err(AgentError::Timeout("Task timed out".to_string())),
                    };

                    let _ = tx.send(task_result).await;
                    drop(permit);
                });
            }
        });

        rx
    }

    /// 分组批量执行
    ///
    /// 按任务类型分组，同类型任务一起执行（可能获得额外优化）
    pub async fn execute_grouped<F, Fut>(
        &self,
        tasks: Vec<Task>,
        executor: F,
    ) -> HashMap<TaskType, BatchResult>
    where
        F: Fn(Task) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future<Output = Result<TaskResult>> + Send,
    {
        // 按任务类型分组
        let mut groups: HashMap<TaskType, Vec<Task>> = HashMap::new();
        for task in tasks {
            groups.entry(task.task_type.clone()).or_default().push(task);
        }

        info!("Executing {} task groups", groups.len());

        // 顺序执行各组（避免生命周期问题）
        let mut results = HashMap::new();
        for (task_type, group_tasks) in groups {
            let exec = executor.clone();
            let batch_result = self.execute_parallel(group_tasks, exec).await;
            results.insert(task_type, batch_result);
        }

        results
    }
}

impl Default for BatchExecutor {
    fn default() -> Self {
        Self::new(10)
    }
}

/// 任务处理器 trait - 用于自定义任务处理逻辑
#[async_trait::async_trait]
pub trait TaskHandler: Send + Sync {
    /// 处理任务
    async fn handle(&self, task: &Task) -> Result<(String, Vec<Artifact>)>;

    /// 是否支持该任务类型
    fn supports(&self, task_type: &TaskType) -> bool;
}

/// 处理器注册表
pub struct HandlerRegistry {
    handlers: Vec<Box<dyn TaskHandler>>,
}

impl HandlerRegistry {
    /// 创建新的注册表
    pub fn new() -> Self {
        Self { handlers: vec![] }
    }

    /// 注册处理器
    pub fn register(&mut self, handler: Box<dyn TaskHandler>) {
        self.handlers.push(handler);
    }

    /// 查找支持该任务类型的处理器
    pub fn find_handler(&self, task_type: &TaskType) -> Option<&dyn TaskHandler> {
        self.handlers
            .iter()
            .find(|h| h.supports(task_type))
            .map(|h| h.as_ref())
    }

    /// 处理任务
    pub async fn process(&self, task: &Task) -> Result<(String, Vec<Artifact>)> {
        let handler = self
            .find_handler(&task.task_type)
            .ok_or_else(|| AgentError::UnsupportedTaskType(task.task_type.to_string()))?;

        handler.handle(task).await
    }
}

impl Default for HandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 批量任务构建器
pub struct BatchTaskBuilder {
    tasks: Vec<Task>,
}

impl BatchTaskBuilder {
    /// 创建新的批量任务构建器
    pub fn new() -> Self {
        Self { tasks: vec![] }
    }

    /// 添加任务
    pub fn add_task(mut self, task: Task) -> Self {
        self.tasks.push(task);
        self
    }

    /// 添加多个任务
    pub fn add_tasks(mut self, tasks: Vec<Task>) -> Self {
        self.tasks.extend(tasks);
        self
    }

    /// 创建同类型批量任务
    pub fn add_homogeneous_tasks(mut self, task_type: TaskType, inputs: Vec<String>) -> Self {
        for (i, input) in inputs.into_iter().enumerate() {
            let task = Task {
                id: format!("batch-task-{}", i),
                task_type: task_type.clone(),
                input,
                parameters: HashMap::new(),
            };
            self.tasks.push(task);
        }
        self
    }

    /// 构建任务列表
    pub fn build(self) -> Vec<Task> {
        self.tasks
    }

    /// 获取任务数量
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }
}

impl Default for BatchTaskBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_result() {
        let mut result = BatchResult::new();

        result.add_success(TaskResult {
            task_id: "task-1".to_string(),
            success: true,
            output: "success".to_string(),
            artifacts: vec![],
            execution_time_ms: 100,
        });

        result.add_failure(
            "task-2".to_string(),
            AgentError::Timeout("test timeout".to_string()),
        );

        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 1);
        assert!(!result.all_succeeded());
        assert_eq!(result.success_rate(), 0.5);
    }

    #[test]
    fn test_batch_result_empty() {
        let result = BatchResult::new();
        assert!(result.all_succeeded()); // 空列表认为全部成功
        assert_eq!(result.success_rate(), 0.0);
    }

    #[test]
    fn test_batch_executor_new() {
        let executor = BatchExecutor::new(20)
            .with_batch_timeout(Duration::from_secs(600))
            .with_task_timeout(Duration::from_secs(120));

        assert_eq!(executor.max_concurrency, 20);
        assert_eq!(executor.batch_timeout, Duration::from_secs(600));
        assert_eq!(executor.task_timeout, Duration::from_secs(120));
    }

    #[test]
    fn test_batch_task_builder() {
        let tasks = BatchTaskBuilder::new()
            .add_homogeneous_tasks(
                TaskType::LlmChat,
                vec!["Hello".to_string(), "World".to_string()],
            )
            .build();

        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].task_type, TaskType::LlmChat);
        assert_eq!(tasks[1].task_type, TaskType::LlmChat);
    }

    #[tokio::test]
    async fn test_batch_executor_parallel() {
        let executor = BatchExecutor::new(5);

        // 创建测试任务
        let tasks: Vec<Task> = (0..10)
            .map(|i| Task {
                id: format!("task-{}", i),
                task_type: TaskType::LlmChat,
                input: format!("input-{}", i),
                parameters: HashMap::new(),
            })
            .collect();

        // 模拟执行函数
        let exec = |task: Task| async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(TaskResult {
                task_id: task.id,
                success: true,
                output: "done".to_string(),
                artifacts: vec![],
                execution_time_ms: 10,
            })
        };

        let result = executor.execute_parallel(tasks, exec).await;

        assert_eq!(result.success_count, 10);
        assert_eq!(result.failure_count, 0);
        assert!(result.all_succeeded());
    }
}
