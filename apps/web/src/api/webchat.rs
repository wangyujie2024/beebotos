//! WebChat API 服务
//!
//! 与 Gateway 的 WebChat API 对接

use super::client::{ApiClient, ApiError};
use crate::webchat::{ChatMessage, ChatSession, UsagePanel};
use serde::{Deserialize, Serialize};

/// 后端消息响应（用于兼容后端 JSON 格式）
#[derive(Clone, Debug, Deserialize)]
struct BackendMessageResponse {
    id: String,
    role: String,
    content: String,
    #[serde(alias = "created_at")]
    timestamp: String,
    metadata: serde_json::Value,
    token_usage: Option<serde_json::Value>,
}

impl From<BackendMessageResponse> for ChatMessage {
    fn from(resp: BackendMessageResponse) -> Self {
        let role = match resp.role.as_str() {
            "user" => crate::webchat::MessageRole::User,
            "assistant" => crate::webchat::MessageRole::Assistant,
            "system" => crate::webchat::MessageRole::System,
            _ => crate::webchat::MessageRole::Assistant,
        };

        let metadata = serde_json::from_value::<crate::webchat::MessageMetadata>(resp.metadata)
            .unwrap_or_default();

        let token_usage = resp
            .token_usage
            .and_then(|v| serde_json::from_value::<crate::webchat::TokenUsage>(v).ok());

        Self {
            id: resp.id,
            role,
            content: resp.content,
            timestamp: resp.timestamp,
            attachments: vec![],
            metadata,
            token_usage,
        }
    }
}

/// WebChat API 服务
#[derive(Clone)]
pub struct WebchatApiService {
    client: ApiClient,
}

impl WebchatApiService {
    pub fn new(client: ApiClient) -> Self {
        Self { client }
    }

    /// 列出会话
    pub async fn list_sessions(&self) -> Result<Vec<ChatSession>, ApiError> {
        self.client.get("/webchat/sessions").await
    }

    /// 创建新会话
    pub async fn create_session(&self, title: &str) -> Result<ChatSession, ApiError> {
        let request = CreateSessionRequest {
            title: title.to_string(),
        };
        self.client.post("/webchat/sessions", &request).await
    }

    /// 获取会话详情
    pub async fn get_session(&self, id: &str) -> Result<ChatSession, ApiError> {
        self.client.get(&format!("/webchat/sessions/{}", js_sys::encode_uri_component(id))).await
    }

    /// 删除会话
    pub async fn delete_session(&self, id: &str) -> Result<(), ApiError> {
        self.client.delete(&format!("/webchat/sessions/{}", js_sys::encode_uri_component(id))).await
    }

    /// 固定/取消固定会话
    pub async fn toggle_pin(&self, id: &str) -> Result<ChatSession, ApiError> {
        self.client
            .post(&format!("/webchat/sessions/{}/pin", js_sys::encode_uri_component(id)), &serde_json::json!({}))
            .await
    }

    /// 归档会话
    pub async fn archive_session(&self, id: &str) -> Result<serde_json::Value, ApiError> {
        self.client
            .post(&format!("/webchat/sessions/{}/archive", js_sys::encode_uri_component(id)), &serde_json::json!({}))
            .await
    }

    /// 获取会话消息
    pub async fn get_messages(&self, session_id: &str) -> Result<Vec<ChatMessage>, ApiError> {
        let responses: Vec<BackendMessageResponse> = self.client
            .get(&format!("/webchat/sessions/{}/messages", js_sys::encode_uri_component(session_id)))
            .await?;
        Ok(responses.into_iter().map(Into::into).collect())
    }

    /// 发送消息到 WebChat Channel
    pub async fn send_message(
        &self,
        session_id: &str,
        content: &str,
        user_id: &str,
    ) -> Result<serde_json::Value, ApiError> {
        let request = serde_json::json!({
            "user_id": user_id,
            "content": content,
            "session_id": session_id,
        });

        self.client
            .post("/channels/webchat/messages", &request)
            .await
    }

    /// 发送流式消息（返回消息流）
    pub async fn send_message_streaming(
        &self,
        session_id: &str,
        content: &str,
    ) -> Result<StreamingResponse, ApiError> {
        let request = SendMessageRequest {
            session_id: session_id.to_string(),
            content: content.to_string(),
        };

        self.client
            .post(
                &format!("/webchat/sessions/{}/messages/stream", js_sys::encode_uri_component(session_id)),
                &request,
            )
            .await
    }

    /// 更新会话标题
    pub async fn update_title(&self, id: &str, title: &str) -> Result<ChatSession, ApiError> {
        let request = UpdateTitleRequest {
            title: title.to_string(),
        };

        self.client
            .put(&format!("/webchat/sessions/{}/title", js_sys::encode_uri_component(id)), &request)
            .await
    }

    /// 清空会话消息
    pub async fn clear_messages(&self, id: &str) -> Result<(), ApiError> {
        self.client
            .post(&format!("/webchat/sessions/{}/clear", js_sys::encode_uri_component(id)), &serde_json::json!({}))
            .await
    }

    /// 获取用量统计
    pub async fn get_usage(&self) -> Result<UsagePanel, ApiError> {
        self.client.get("/webchat/usage").await
    }

    /// 创建侧边提问
    pub async fn create_side_question(
        &self,
        session_id: &str,
        question: &str,
    ) -> Result<SideQuestionResponse, ApiError> {
        let request = SideQuestionRequest {
            session_id: session_id.to_string(),
            question: question.to_string(),
        };

        self.client.post("/webchat/side-questions", &request).await
    }

    /// 导出会话
    pub async fn export_session(&self, id: &str) -> Result<String, ApiError> {
        let response: ExportResponse = self
            .client
            .get(&format!("/webchat/sessions/{}/export", js_sys::encode_uri_component(id)))
            .await?;

        Ok(response.data)
    }

    /// 导入会话
    pub async fn import_session(&self, data: &str) -> Result<ChatSession, ApiError> {
        let request = ImportRequest {
            data: data.to_string(),
        };

        self.client.post("/webchat/sessions/import", &request).await
    }
}

/// 创建会话请求
#[derive(Clone, Debug, Serialize, Deserialize)]
struct CreateSessionRequest {
    title: String,
}

/// 发送消息请求
#[derive(Clone, Debug, Serialize, Deserialize)]
struct SendMessageRequest {
    session_id: String,
    content: String,
}

/// 更新标题请求
#[derive(Clone, Debug, Serialize, Deserialize)]
struct UpdateTitleRequest {
    title: String,
}

/// 流式响应
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StreamingResponse {
    pub stream_id: String,
    pub status: String,
}

/// 侧边提问请求
#[derive(Clone, Debug, Serialize, Deserialize)]
struct SideQuestionRequest {
    session_id: String,
    question: String,
}

/// 侧边提问响应
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SideQuestionResponse {
    pub id: String,
    pub question: String,
    pub status: String,
}

/// 导出响应
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExportResponse {
    data: String,
}

/// 导入请求
#[derive(Clone, Debug, Serialize, Deserialize)]
struct ImportRequest {
    data: String,
}

/// 快捷指令请求
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SlashCommandRequest {
    pub session_id: String,
    pub command: String,
    pub args: Vec<String>,
}

/// 快捷指令响应
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SlashCommandResponse {
    pub success: bool,
    pub message: String,
    pub action: Option<String>,
}

/// Token 用量请求
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenUsageRequest {
    pub session_id: Option<String>,
    pub period: Option<String>, // "day", "month", "all"
}

/// 消息编辑请求
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditMessageRequest {
    pub message_id: String,
    pub new_content: String,
}

/// 附件上传请求
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UploadAttachmentRequest {
    pub session_id: String,
    pub file_name: String,
    pub file_type: String,
    pub file_data: String, // base64
}

/// 附件上传响应
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UploadAttachmentResponse {
    pub attachment_id: String,
    pub url: String,
    pub thumbnail_url: Option<String>,
}
