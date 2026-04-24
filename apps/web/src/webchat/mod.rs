//! WebChat 聊天界面模块
//!
//! 提供会话管理、侧边提问、Token用量统计等功能
//! 兼容 OpenClaw V2026.3.13 Webchat 规格

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub mod chat;
pub mod mobile;
pub mod session;
pub mod sidebar;

pub use chat::{ChatInterface, MessageComposer, MessageList};
pub use mobile::MobileAdapter;
pub use session::{SessionManager, SessionPersistence};
pub use sidebar::{SideQuestion, SideQuestionManager, SideQuestionStatus};

/// 聊天消息
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    pub timestamp: String,
    pub attachments: Vec<Attachment>,
    pub metadata: MessageMetadata,
    pub token_usage: Option<TokenUsage>,
}

/// 消息角色
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

impl Default for MessageRole {
    fn default() -> Self {
        MessageRole::User
    }
}

/// 消息元数据
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct MessageMetadata {
    #[serde(default)]
    pub is_error: bool,
    #[serde(default)]
    pub is_streaming: bool,
    pub model: Option<String>,
    pub latency_ms: Option<u64>,
    #[serde(default)]
    pub edits: Vec<MessageEdit>,
}

/// 消息编辑记录
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MessageEdit {
    pub timestamp: String,
    pub previous_content: String,
}

/// 附件
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Attachment {
    pub id: String,
    pub file_name: String,
    pub file_type: String,
    pub file_size: u64,
    pub url: Option<String>,
    pub thumbnail_url: Option<String>,
    pub is_image: bool,
}

/// Token 用量
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub estimated_cost: f64,
    pub model: String,
}

impl Default for TokenUsage {
    fn default() -> Self {
        Self {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            estimated_cost: 0.0,
            model: "default".to_string(),
        }
    }
}

impl TokenUsage {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            ..Default::default()
        }
    }

    pub fn with_tokens(mut self, prompt: u64, completion: u64) -> Self {
        self.prompt_tokens = prompt;
        self.completion_tokens = completion;
        self.total_tokens = prompt + completion;
        self
    }

    /// 格式化显示
    pub fn format(&self) -> String {
        format!(
            "{} tokens (~${:.4})",
            self.total_tokens, self.estimated_cost
        )
    }
}

/// 聊天会话
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub context: SessionContext,
    pub is_pinned: bool,
    pub is_archived: bool,
    #[serde(default)]
    pub total_token_usage: TokenUsage,
}

impl ChatSession {
    pub fn new(title: impl Into<String>) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: title.into(),
            created_at: now.clone(),
            updated_at: now,
            messages: Vec::new(),
            context: SessionContext::default(),
            is_pinned: false,
            is_archived: false,
            total_token_usage: TokenUsage::new("default"),
        }
    }

    /// 添加消息
    pub fn add_message(&mut self, message: ChatMessage) {
        self.messages.push(message);
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }

    /// 获取最后一条消息
    pub fn last_message(&self) -> Option<&ChatMessage> {
        self.messages.last()
    }

    /// 获取消息数量
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// 固定/取消固定
    pub fn toggle_pin(&mut self) {
        self.is_pinned = !self.is_pinned;
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }

    /// 更新标题
    pub fn update_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }

    /// 计算总 token 用量
    pub fn calculate_total_usage(&mut self) {
        let mut total = TokenUsage::new(&self.total_token_usage.model);

        for msg in &self.messages {
            if let Some(usage) = &msg.token_usage {
                total.prompt_tokens += usage.prompt_tokens;
                total.completion_tokens += usage.completion_tokens;
                total.total_tokens += usage.total_tokens;
                total.estimated_cost += usage.estimated_cost;
            }
        }

        self.total_token_usage = total;
    }
}

/// 会话上下文
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SessionContext {
    pub agent_id: Option<String>,
    pub browser_instance: Option<String>,
    pub skill_context: Option<String>,
    pub system_prompt: Option<String>,
    pub custom_params: HashMap<String, String>,
}

/// 会话过滤器
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SessionFilter {
    pub include_archived: bool,
    pub only_pinned: bool,
    pub search_query: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
}

/// 用量面板
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UsagePanel {
    pub session_usage: TokenUsage,
    pub daily_usage: TokenUsage,
    pub monthly_usage: TokenUsage,
    pub limit_status: LimitStatus,
}

/// 限制状态
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LimitStatus {
    pub has_limit: bool,
    pub daily_limit: Option<u64>,
    pub monthly_limit: Option<u64>,
    pub daily_remaining: Option<u64>,
    pub monthly_remaining: Option<u64>,
    pub is_near_limit: bool,
}

impl Default for LimitStatus {
    fn default() -> Self {
        Self {
            has_limit: false,
            daily_limit: None,
            monthly_limit: None,
            daily_remaining: None,
            monthly_remaining: None,
            is_near_limit: false,
        }
    }
}

/// 快捷指令
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SlashCommand {
    pub command: String,
    pub description: String,
    pub args: Vec<CommandArg>,
    /// 处理器动作标识，序列化时只保存动作名称
    #[serde(skip)]
    pub handler: CommandHandler,
    /// 处理器类型标识，用于序列化
    pub handler_type: String,
}

/// 命令参数
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommandArg {
    pub name: String,
    pub description: String,
    pub required: bool,
    pub default_value: Option<String>,
}

/// 命令处理器类型
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandHandler {
    Builtin { action: String },
    Custom { script: String },
    Api { endpoint: String },
}

impl Default for CommandHandler {
    fn default() -> Self {
        CommandHandler::Builtin {
            action: "default".to_string(),
        }
    }
}

/// 快捷指令管理器
pub struct SlashCommandManager {
    commands: HashMap<String, SlashCommand>,
}

impl CommandHandler {
    /// 获取处理器类型标识
    pub fn handler_type(&self) -> String {
        match self {
            CommandHandler::Builtin { .. } => "builtin".to_string(),
            CommandHandler::Custom { .. } => "custom".to_string(),
            CommandHandler::Api { .. } => "api".to_string(),
        }
    }
}

impl SlashCommandManager {
    pub fn new() -> Self {
        let mut manager = Self {
            commands: HashMap::new(),
        };
        manager.register_default_commands();
        manager
    }

    fn register_default_commands(&mut self) {
        // /btw - 侧边提问
        let handler = CommandHandler::Builtin {
            action: "side_question".to_string(),
        };
        self.register(SlashCommand {
            command: "/btw".to_string(),
            description: "在主会话线程旁进行快速侧提问".to_string(),
            args: vec![CommandArg {
                name: "question".to_string(),
                description: "要问的问题".to_string(),
                required: true,
                default_value: None,
            }],
            handler_type: handler.handler_type(),
            handler,
        });

        // /clear - 清空会话
        let handler = CommandHandler::Builtin {
            action: "clear_session".to_string(),
        };
        self.register(SlashCommand {
            command: "/clear".to_string(),
            description: "清空当前会话的消息历史".to_string(),
            args: vec![],
            handler_type: handler.handler_type(),
            handler,
        });

        // /help - 显示帮助
        let handler = CommandHandler::Builtin {
            action: "show_help".to_string(),
        };
        self.register(SlashCommand {
            command: "/help".to_string(),
            description: "显示所有可用的快捷指令".to_string(),
            args: vec![],
            handler_type: handler.handler_type(),
            handler,
        });

        // /new - 新建会话
        let handler = CommandHandler::Builtin {
            action: "new_session".to_string(),
        };
        self.register(SlashCommand {
            command: "/new".to_string(),
            description: "创建新的聊天会话".to_string(),
            args: vec![CommandArg {
                name: "title".to_string(),
                description: "会话标题（可选）".to_string(),
                required: false,
                default_value: Some("New Chat".to_string()),
            }],
            handler_type: handler.handler_type(),
            handler,
        });
    }

    pub fn register(&mut self, command: SlashCommand) {
        self.commands.insert(command.command.clone(), command);
    }

    pub fn get(&self, command: &str) -> Option<&SlashCommand> {
        self.commands.get(command)
    }

    pub fn list_all(&self) -> Vec<&SlashCommand> {
        self.commands.values().collect()
    }

    /// 解析命令
    pub fn parse(&self, input: &str) -> Option<ParsedCommand> {
        let parts: Vec<&str> = input.trim().split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }

        let cmd = parts[0];
        let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

        self.commands.get(cmd).map(|command| ParsedCommand {
            command: command.clone(),
            args,
        })
    }
}

/// 解析后的命令
#[derive(Clone, Debug)]
pub struct ParsedCommand {
    pub command: SlashCommand,
    pub args: Vec<String>,
}

/// WebChat 配置
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WebchatConfig {
    pub enable_persistence: bool,
    pub max_sessions: usize,
    pub max_messages_per_session: usize,
    pub enable_attachments: bool,
    pub max_attachment_size_mb: u32,
    pub enable_slash_commands: bool,
    pub show_token_usage: bool,
}

impl Default for WebchatConfig {
    fn default() -> Self {
        Self {
            enable_persistence: true,
            max_sessions: 100,
            max_messages_per_session: 1000,
            enable_attachments: true,
            max_attachment_size_mb: 50,
            enable_slash_commands: true,
            show_token_usage: true,
        }
    }
}

/// 复制管理器
pub struct ClipboardManager;

impl ClipboardManager {
    /// 复制文本到剪贴板
    pub async fn copy_text(text: &str) -> Result<(), ClipboardError> {
        if let Some(window) = web_sys::window() {
            let navigator = window.navigator();
            let clipboard = navigator.clipboard();

            let promise = clipboard.write_text(text);

            // 使用 wasm_bindgen_futures 等待 Promise
            match wasm_bindgen_futures::JsFuture::from(promise).await {
                Ok(_) => Ok(()),
                Err(e) => Err(ClipboardError::WriteFailed(format!("{:?}", e))),
            }
        } else {
            Err(ClipboardError::NotAvailable)
        }
    }

    /// 复制代码块
    pub async fn copy_code(code: &str, language: Option<&str>) -> Result<(), ClipboardError> {
        let formatted = if let Some(lang) = language {
            format!("```{lang}\n{}\n```", code)
        } else {
            code.to_string()
        };
        Self::copy_text(&formatted).await
    }

    /// 复制消息为 Markdown
    pub async fn copy_as_markdown(message: &ChatMessage) -> Result<(), ClipboardError> {
        let role_prefix = match message.role {
            MessageRole::User => "**User**: ",
            MessageRole::Assistant => "**Assistant**: ",
            MessageRole::System => "**System**: ",
        };

        let markdown = format!("{}\n\n{}\n\n---\n", role_prefix, message.content);

        Self::copy_text(&markdown).await
    }
}

/// 剪贴板错误
#[derive(Clone, Debug)]
pub enum ClipboardError {
    NotAvailable,
    WriteFailed(String),
    ReadFailed(String),
}

impl std::fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClipboardError::NotAvailable => write!(f, "Clipboard API not available"),
            ClipboardError::WriteFailed(msg) => write!(f, "Failed to write to clipboard: {}", msg),
            ClipboardError::ReadFailed(msg) => write!(f, "Failed to read from clipboard: {}", msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_session() {
        let mut session = ChatSession::new("Test Session");
        assert_eq!(session.title, "Test Session");
        assert_eq!(session.messages.len(), 0);

        let message = ChatMessage {
            id: "1".to_string(),
            role: MessageRole::User,
            content: "Hello".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            attachments: vec![],
            metadata: MessageMetadata::default(),
            token_usage: None,
        };

        session.add_message(message);
        assert_eq!(session.messages.len(), 1);
    }

    #[test]
    fn test_token_usage() {
        let usage = TokenUsage::new("gpt-4").with_tokens(100, 50);
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
    }

    #[test]
    fn test_slash_command_manager() {
        let manager = SlashCommandManager::new();

        assert!(manager.get("/btw").is_some());
        assert!(manager.get("/clear").is_some());
        assert!(manager.get("/help").is_some());
        assert!(manager.get("/new").is_some());

        let parsed = manager.parse("/btw what is this?");
        assert!(parsed.is_some());
        assert_eq!(parsed.unwrap().args, vec!["what", "is", "this?"]);
    }
}
