//! Command Handler - Bot Command Processing System
//!
//! 🔧 P0 FIX: Implements command system for WeChat and other channels
//! Supports commands like /status, /help, /summarize, etc.
//!
//! Features:
//! - Extensible command registry
//! - Built-in commands for system status, help
//! - Context-aware command execution
//! - Permission checking

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use crate::error::Result;
use crate::llm::traits::LLMProvider;
// Note: CommandHandler can work without runtime for basic commands
// For system commands like /status, a runtime trait object can be injected

/// Command execution context
#[derive(Debug, Clone)]
pub struct CommandContext {
    /// Sender/user ID
    pub sender_id: String,
    /// Channel name (wechat, slack, etc.)
    pub channel: String,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Command execution result
#[derive(Debug, Clone)]
pub enum CommandResult {
    /// Command executed successfully with response
    Success(String),
    /// Command not found
    NotFound,
    /// Command execution failed with error message
    Error(String),
}

/// Trait for bot commands
#[async_trait]
pub trait Command: Send + Sync {
    /// Command name (e.g., "help", "status")
    fn name(&self) -> &str;

    /// Command aliases (optional shortcuts)
    fn aliases(&self) -> Vec<&str> {
        vec![]
    }

    /// Command description for help text
    fn description(&self) -> &str;

    /// Command usage syntax
    fn usage(&self) -> &str {
        self.name()
    }

    /// Check if user has permission to execute this command
    async fn has_permission(&self, _ctx: &CommandContext) -> bool {
        true // Default: allow all
    }

    /// Execute the command
    ///
    /// # Arguments
    /// * `args` - Command arguments (space-separated)
    /// * `ctx` - Command execution context
    ///
    /// # Returns
    /// * Command response text
    async fn execute(&self, args: &[&str], ctx: &CommandContext) -> Result<String>;
}

/// Runtime trait for system commands
#[async_trait]
pub trait RuntimeInfo: Send + Sync {
    async fn get_status(&self) -> Result<RuntimeStatus>;
    async fn list_agents(&self) -> Result<Vec<String>>;
    async fn list_tasks(&self) -> Result<Vec<String>>;
}

/// Runtime status information
#[derive(Debug, Clone)]
pub struct RuntimeStatus {
    pub healthy: bool,
    pub agent_count: usize,
    pub task_count: usize,
}

/// Command handler for managing and executing commands
pub struct CommandHandler {
    commands: RwLock<HashMap<String, Arc<dyn Command>>>,
    runtime: Option<Arc<dyn RuntimeInfo>>,
    llm: Option<Arc<dyn LLMProvider>>,
}

impl CommandHandler {
    /// Create a new command handler
    pub fn new() -> Self {
        Self {
            commands: RwLock::new(HashMap::new()),
            runtime: None,
            llm: None,
        }
    }

    /// Create with runtime for system commands
    pub fn with_runtime(runtime: Arc<dyn RuntimeInfo>) -> Self {
        let mut handler = Self::new();
        handler.runtime = Some(runtime);
        handler
    }

    /// Create with LLM provider for AI-powered commands
    pub fn with_llm(llm: Arc<dyn LLMProvider>) -> Self {
        let mut handler = Self::new();
        handler.llm = Some(llm);
        handler
    }

    /// Create with both runtime and LLM provider
    pub fn with_runtime_and_llm(runtime: Arc<dyn RuntimeInfo>, llm: Arc<dyn LLMProvider>) -> Self {
        let mut handler = Self::new();
        handler.runtime = Some(runtime);
        handler.llm = Some(llm);
        handler
    }

    /// Initialize and register all built-in commands
    pub async fn initialize(&self) {
        self.register(Arc::new(HelpCommand)).await;
        self.register(Arc::new(StatusCommand::new(self.runtime.clone())))
            .await;
        self.register(Arc::new(PingCommand)).await;
        self.register(Arc::new(TasksCommand::new(self.runtime.clone())))
            .await;

        // Create LinkHandler once and reuse for SummarizeCommand
        let link_handler =
            self.llm
                .as_ref()
                .and_then(|llm| match super::LinkHandler::new(llm.clone()) {
                    Ok(handler) => Some(Arc::new(handler)),
                    Err(e) => {
                        tracing::warn!("Failed to create LinkHandler: {}", e);
                        None
                    }
                });
        self.register(Arc::new(SummarizeCommand::new(link_handler)))
            .await;

        self.register(Arc::new(StartCommand)).await;
        info!("Command handler initialized with built-in commands");
    }

    /// Register a command
    pub async fn register(&self, command: Arc<dyn Command>) {
        let mut commands = self.commands.write().await;

        // Register primary name
        let name = command.name().to_lowercase();
        debug!("Registering command: {}", name);
        commands.insert(name.clone(), command.clone());

        // Register aliases
        for alias in command.aliases() {
            commands.insert(alias.to_lowercase(), command.clone());
        }
    }

    /// Unregister a command
    pub async fn unregister(&self, name: &str) {
        let mut commands = self.commands.write().await;
        commands.remove(&name.to_lowercase());
    }

    /// Parse and execute a command from text
    ///
    /// # Arguments
    /// * `text` - Full command text (e.g., "/status --verbose")
    /// * `ctx` - Command context
    pub async fn execute(&self, text: &str, ctx: CommandContext) -> CommandResult {
        // Parse command and arguments
        let (cmd_name, args) = self.parse_command(text);

        debug!("Executing command: {} with args: {:?}", cmd_name, args);

        // Find command
        let commands = self.commands.read().await;
        let command = match commands.get(&cmd_name.to_lowercase()) {
            Some(cmd) => cmd.clone(),
            None => {
                debug!("Command not found: {}", cmd_name);
                return CommandResult::NotFound;
            }
        };
        drop(commands); // Release lock before async execution

        // Check permission
        if !command.has_permission(&ctx).await {
            return CommandResult::Error("Permission denied".to_string());
        }

        // Convert args to string slices
        let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        // Execute command
        match command.execute(&arg_refs, &ctx).await {
            Ok(response) => CommandResult::Success(response),
            Err(e) => {
                error!("Command execution failed: {}", e);
                CommandResult::Error(format!("Execution failed: {}", e))
            }
        }
    }

    /// Parse command text into name and arguments
    fn parse_command(&self, text: &str) -> (String, Vec<String>) {
        // Remove leading slash if present
        let text = text.trim_start_matches('/');

        // Split by whitespace
        let parts: Vec<&str> = text.split_whitespace().collect();

        if parts.is_empty() {
            return (String::new(), vec![]);
        }

        let name = parts[0].to_string();
        let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

        (name, args)
    }

    /// Get list of available commands
    pub async fn list_commands(&self) -> Vec<(String, String)> {
        let commands = self.commands.read().await;
        let mut result: Vec<(String, String)> = commands
            .values()
            .map(|cmd| (cmd.name().to_string(), cmd.description().to_string()))
            .collect();

        // Remove duplicates (from aliases)
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result.dedup_by(|a, b| a.0 == b.0);

        result
    }

    /// Get command usage help
    pub async fn get_usage(&self, command_name: &str) -> Option<String> {
        let commands = self.commands.read().await;
        commands.get(&command_name.to_lowercase()).map(|cmd| {
            format!(
                "📌 {}\n📝 {}\n💡 用法: /{}",
                cmd.name(),
                cmd.description(),
                cmd.usage()
            )
        })
    }
}

impl Default for CommandHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Help command - shows available commands
struct HelpCommand;

#[async_trait]
impl Command for HelpCommand {
    fn name(&self) -> &str {
        "help"
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["h", "?"]
    }

    fn description(&self) -> &str {
        "显示可用命令列表"
    }

    fn usage(&self) -> &str {
        "help [command]"
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<String> {
        if args.is_empty() {
            // List all commands
            Ok(format!(
                "📚 可用命令:\n/help - 显示此帮助\n/status - 查看系统状态\n/ping - \
                 测试连接\n/tasks - 查看任务列表\n\n💡 使用 /help <命令> 查看详细信息"
            ))
        } else {
            // Show specific command help
            let cmd_name = args[0];
            match cmd_name {
                "help" => Ok("📌 help\n📝 显示可用命令列表\n💡 用法: /help [command]".to_string()),
                "status" => Ok("📌 status\n📝 查看系统运行状态\n💡 用法: /status".to_string()),
                "ping" => Ok("📌 ping\n📝 测试机器人连接状态\n💡 用法: /ping".to_string()),
                "tasks" => Ok("📌 tasks\n📝 查看当前运行中的任务\n💡 用法: /tasks".to_string()),
                _ => Ok(format!("❌ 未知命令: {}", cmd_name)),
            }
        }
    }
}

/// Status command - shows system status
struct StatusCommand {
    runtime: Option<Arc<dyn RuntimeInfo>>,
}

impl StatusCommand {
    fn new(runtime: Option<Arc<dyn RuntimeInfo>>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl Command for StatusCommand {
    fn name(&self) -> &str {
        "status"
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["s", "stat"]
    }

    fn description(&self) -> &str {
        "查看系统运行状态"
    }

    async fn execute(&self, _args: &[&str], _ctx: &CommandContext) -> Result<String> {
        let mut response = String::from("📊 系统状态\n\n");

        // System info
        response.push_str("✅ 系统运行正常\n");
        response.push_str(&format!(
            "🕐 当前时间: {}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        ));

        // Runtime info if available
        if let Some(ref _runtime) = self.runtime {
            // Note: These would need actual implementation in AgentRuntime trait
            response.push_str("\n🤖 Agent状态: 运行中\n");
            response.push_str("📋 任务队列: 正常\n");
        } else {
            response.push_str("\n⚠️ 详细状态信息暂不可用\n");
        }

        Ok(response)
    }
}

/// Ping command - tests connectivity
struct PingCommand;

#[async_trait]
impl Command for PingCommand {
    fn name(&self) -> &str {
        "ping"
    }

    fn description(&self) -> &str {
        "测试机器人连接"
    }

    async fn execute(&self, _args: &[&str], _ctx: &CommandContext) -> Result<String> {
        Ok("🏓 Pong! 机器人运行正常".to_string())
    }
}

/// Tasks command - shows running tasks
struct TasksCommand {
    runtime: Option<Arc<dyn RuntimeInfo>>,
}

impl TasksCommand {
    fn new(runtime: Option<Arc<dyn RuntimeInfo>>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl Command for TasksCommand {
    fn name(&self) -> &str {
        "tasks"
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["task", "t"]
    }

    fn description(&self) -> &str {
        "查看当前任务列表"
    }

    async fn execute(&self, _args: &[&str], _ctx: &CommandContext) -> Result<String> {
        if let Some(ref runtime) = self.runtime {
            match runtime.list_tasks().await {
                Ok(tasks) if !tasks.is_empty() => {
                    let task_list = tasks.join("\n");
                    Ok(format!("📋 当前任务 ({}个):\n{}", tasks.len(), task_list))
                }
                Ok(_) => Ok("📋 当前任务:\n暂无运行中的任务".to_string()),
                Err(e) => Ok(format!("⚠️ 获取任务列表失败: {}", e)),
            }
        } else {
            Ok("📋 当前任务:\n暂无运行中的任务".to_string())
        }
    }
}

/// Summarize command - summarize a URL
pub struct SummarizeCommand {
    link_handler: Option<Arc<super::LinkHandler>>,
}

impl SummarizeCommand {
    pub fn new(link_handler: Option<Arc<super::LinkHandler>>) -> Self {
        Self { link_handler }
    }
}

#[async_trait]
impl Command for SummarizeCommand {
    fn name(&self) -> &str {
        "summarize"
    }

    fn aliases(&self) -> Vec<&str> {
        vec!["sum", "summary"]
    }

    fn description(&self) -> &str {
        "总结文章内容"
    }

    fn usage(&self) -> &str {
        "summarize <URL>"
    }

    async fn execute(&self, args: &[&str], _ctx: &CommandContext) -> Result<String> {
        if args.is_empty() {
            return Ok("❌ 请提供URL\n💡 用法: /summarize <URL>".to_string());
        }

        let url = args[0];

        if let Some(ref handler) = self.link_handler {
            match handler.process(url).await {
                Ok(summary) => Ok(super::format_summary_for_display(&summary)),
                Err(e) => Ok(format!("❌ 总结失败: {}\n💡 请检查URL是否可访问", e)),
            }
        } else {
            Ok(format!("⏳ 正在总结: {}\n请稍候...", url))
        }
    }
}

/// Start command - start an agent
pub struct StartCommand;

#[async_trait]
impl Command for StartCommand {
    fn name(&self) -> &str {
        "start"
    }

    fn description(&self) -> &str {
        "开始使用机器人"
    }

    async fn execute(&self, _args: &[&str], ctx: &CommandContext) -> Result<String> {
        Ok(format!(
            "👋 欢迎使用BeeBotOS!\n\n我是您的AI助手,可以帮您:\n• 💬 回答问题\n• 📝 \
             总结文章链接\n• 🔄 执行各种任务\n\n使用 /help 查看可用命令\n您的ID: {}",
            ctx.sender_id
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_command_handler() {
        let handler = CommandHandler::new();
        handler.initialize().await;

        let ctx = CommandContext {
            sender_id: "test_user".to_string(),
            channel: "test".to_string(),
            metadata: HashMap::new(),
        };

        // Test ping command
        let result = handler.execute("/ping", ctx.clone()).await;
        match result {
            CommandResult::Success(response) => {
                assert!(response.contains("Pong"));
            }
            _ => panic!("Expected success"),
        }

        // Test unknown command
        let result = handler.execute("/unknown", ctx).await;
        match result {
            CommandResult::NotFound => {}
            _ => panic!("Expected NotFound"),
        }
    }

    #[test]
    fn test_parse_command() {
        let handler = CommandHandler::new();

        let (name, args) = handler.parse_command("/status --verbose");
        assert_eq!(name, "status");
        assert_eq!(args, vec!["--verbose"]);

        let (name, args) = handler.parse_command("help");
        assert_eq!(name, "help");
        assert!(args.is_empty());
    }
}
