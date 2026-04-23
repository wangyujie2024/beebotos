//! WebChat 状态管理

use crate::webchat::{ChatMessage, ChatSession, SideQuestion, TokenUsage, UsagePanel};
use leptos::prelude::*;

/// WebChat 状态
#[derive(Clone, Debug)]
pub struct WebchatState {
    /// 当前选中的会话 ID
    pub current_session_id: RwSignal<Option<String>>,
    /// 所有会话
    pub sessions: RwSignal<Vec<ChatSession>>,
    /// 当前会话的消息
    pub current_messages: RwSignal<Vec<ChatMessage>>,
    /// 输入框内容
    pub input_content: RwSignal<String>,
    /// 是否正在发送
    pub is_sending: RwSignal<bool>,
    /// 是否正在流式接收
    pub is_streaming: RwSignal<bool>,
    /// 流式内容缓冲区
    pub streaming_content: RwSignal<String>,
    /// 用量统计
    pub usage: RwSignal<UsagePanel>,
    /// 侧边提问列表
    pub side_questions: RwSignal<Vec<SideQuestion>>
    ,
    /// 消息缓存（按会话 ID）
    pub message_cache: RwSignal<std::collections::HashMap<String, Vec<ChatMessage>>>,
    /// 当前错误
    pub error: RwSignal<Option<String>>,
}

impl WebchatState {
    pub fn new() -> Self {
        Self {
            current_session_id: RwSignal::new(None),
            sessions: RwSignal::new(Vec::new()),
            current_messages: RwSignal::new(Vec::new()),
            input_content: RwSignal::new(String::new()),
            is_sending: RwSignal::new(false),
            is_streaming: RwSignal::new(false),
            streaming_content: RwSignal::new(String::new()),
            usage: RwSignal::new(UsagePanel {
                session_usage: TokenUsage::new("default"),
                daily_usage: TokenUsage::new("default"),
                monthly_usage: TokenUsage::new("default"),
                limit_status: Default::default(),
            }),
            side_questions: RwSignal::new(Vec::new()),
            message_cache: RwSignal::new(std::collections::HashMap::new()),
            error: RwSignal::new(None),
        }
    }

    /// 选中会话
    pub fn select_session(&self, id: impl Into<String>) {
        self.current_session_id.set(Some(id.into()));
        self.current_messages.set(Vec::new());
    }

    /// 清除选中的会话
    pub fn clear_session(&self) {
        self.current_session_id.set(None);
        self.current_messages.set(Vec::new());
    }

    /// 设置输入内容
    pub fn set_input(&self, content: impl Into<String>) {
        self.input_content.set(content.into());
    }

    /// 清空输入
    pub fn clear_input(&self) {
        self.input_content.set(String::new());
    }

    /// 添加消息
    pub fn add_message(&self, message: ChatMessage) {
        self.current_messages.update(|msgs| msgs.push(message.clone()));
        if let Some(session_id) = self.current_session_id.get() {
            self.message_cache.update(|cache| {
                cache.entry(session_id).or_default().push(message);
            });
        }
    }

    /// 开始流式接收
    pub fn start_streaming(&self) {
        self.is_streaming.set(true);
        self.streaming_content.set(String::new());
    }

    /// 追加流式内容
    pub fn append_streaming_content(&self, chunk: impl Into<String>) {
        self.streaming_content.update(|content| {
            content.push_str(&chunk.into());
        });
    }

    /// 结束流式接收
    pub fn finish_streaming(&self) {
        self.is_streaming.set(false);
        // 将流式内容转为正式消息
        let content = self.streaming_content.get();
        if !content.is_empty() {
            let message = ChatMessage {
                id: uuid::Uuid::new_v4().to_string(),
                role: crate::webchat::MessageRole::Assistant,
                content,
                timestamp: chrono::Utc::now().to_rfc3339(),
                attachments: vec![],
                metadata: Default::default(),
                token_usage: None,
            };
            self.add_message(message);
        }
        self.streaming_content.set(String::new());
    }

    /// 设置错误
    pub fn set_error(&self, error: Option<String>) {
        self.error.set(error);
    }

    /// 添加侧边提问
    pub fn add_side_question(&self, question: SideQuestion) {
        self.side_questions.update(|qs| qs.push(question));
    }

    /// 更新侧边提问响应
    pub fn update_side_question(&self, id: &str, response: impl Into<String>) {
        let response = response.into();
        self.side_questions.update(|qs| {
            for q in qs.iter_mut() {
                if q.id == id {
                    q.set_response(response.clone());
                    break;
                }
            }
        });
    }
}

impl Default for WebchatState {
    fn default() -> Self {
        Self::new()
    }
}

/// WebChat UI 状态
#[derive(Clone, Debug)]
pub struct ChatUIState {
    /// 是否显示会话列表面板
    pub show_sessions_panel: RwSignal<bool>,
    /// 是否显示用量面板
    pub show_usage_panel: RwSignal<bool>,
    /// 是否显示侧边提问面板
    pub show_side_panel: RwSignal<bool>,
    /// 是否显示新建会话弹窗
    pub show_new_session_modal: RwSignal<bool>,
    /// 是否显示设置弹窗
    pub show_settings_modal: RwSignal<bool>,
    /// 搜索查询
    pub search_query: RwSignal<String>,
    /// 滚动到底部标志
    pub scroll_to_bottom: RwSignal<bool>,
}

impl ChatUIState {
    pub fn new() -> Self {
        Self {
            show_sessions_panel: RwSignal::new(true),
            show_usage_panel: RwSignal::new(false),
            show_side_panel: RwSignal::new(false),
            show_new_session_modal: RwSignal::new(false),
            show_settings_modal: RwSignal::new(false),
            search_query: RwSignal::new(String::new()),
            scroll_to_bottom: RwSignal::new(false),
        }
    }

    pub fn toggle_sessions_panel(&self) {
        self.show_sessions_panel.update(|v| *v = !*v);
    }

    pub fn toggle_usage_panel(&self) {
        self.show_usage_panel.update(|v| *v = !*v);
    }

    pub fn toggle_side_panel(&self) {
        self.show_side_panel.update(|v| *v = !*v);
    }

    pub fn trigger_scroll_to_bottom(&self) {
        self.scroll_to_bottom.set(true);
    }

    pub fn clear_scroll_to_bottom(&self) {
        self.scroll_to_bottom.set(false);
    }
}

impl Default for ChatUIState {
    fn default() -> Self {
        Self::new()
    }
}

/// 会话 UI 状态
#[derive(Clone, Debug)]
pub struct SessionUIState {
    /// 是否正在编辑标题
    pub is_editing_title: RwSignal<bool>,
    /// 编辑中的标题
    pub editing_title: RwSignal<String>,
    /// 选中的消息 ID（用于复制等操作）
    pub selected_message_id: RwSignal<Option<String>>,
    /// 是否显示附件上传
    pub show_attachment_upload: RwSignal<bool>,
    /// 上传中的文件
    pub uploading_files: RwSignal<Vec<String>>,
}

impl SessionUIState {
    pub fn new() -> Self {
        Self {
            is_editing_title: RwSignal::new(false),
            editing_title: RwSignal::new(String::new()),
            selected_message_id: RwSignal::new(None),
            show_attachment_upload: RwSignal::new(false),
            uploading_files: RwSignal::new(Vec::new()),
        }
    }

    pub fn start_editing_title(&self, current_title: &str) {
        self.editing_title.set(current_title.to_string());
        self.is_editing_title.set(true);
    }

    pub fn finish_editing_title(&self) {
        self.is_editing_title.set(false);
    }

    pub fn cancel_editing_title(&self) {
        self.is_editing_title.set(false);
        self.editing_title.set(String::new());
    }
}

impl Default for SessionUIState {
    fn default() -> Self {
        Self::new()
    }
}

/// 提供 WebChat 状态到上下文
pub fn provide_webchat_state() {
    provide_context(WebchatState::new());
    provide_context(ChatUIState::new());
    provide_context(SessionUIState::new());
}

/// 使用 WebChat 状态
pub fn use_webchat_state() -> WebchatState {
    use_context::<WebchatState>().expect("WebchatState not provided")
}

/// 使用 WebChat UI 状态
pub fn use_chat_ui_state() -> ChatUIState {
    use_context::<ChatUIState>().expect("ChatUIState not provided")
}

/// 使用会话 UI 状态
pub fn use_session_ui_state() -> SessionUIState {
    use_context::<SessionUIState>().expect("SessionUIState not provided")
}
