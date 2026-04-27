//! 侧边提问模块
//!
//! 实现 `/btw` 快捷指令，支持在主会话线程旁进行快速侧提问

use serde::{Deserialize, Serialize};

use super::{ChatMessage, MessageMetadata, MessageRole};

/// 侧边提问
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SideQuestion {
    pub id: String,
    pub parent_session_id: String,
    pub question: String,
    pub response: Option<String>,
    pub status: SideQuestionStatus,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub context: SideQuestionContext,
}

/// 侧边提问状态
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SideQuestionStatus {
    Pending,
    Processing,
    Completed,
    Failed,
}

/// 侧边提问上下文
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SideQuestionContext {
    pub referenced_message_id: Option<String>,
    pub additional_context: Option<String>,
    pub browser_context: Option<String>,
}

impl SideQuestion {
    /// 创建新的侧边提问
    pub fn new(parent_session_id: impl Into<String>, question: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            parent_session_id: parent_session_id.into(),
            question: question.into(),
            response: None,
            status: SideQuestionStatus::Pending,
            created_at: chrono::Utc::now().to_rfc3339(),
            completed_at: None,
            context: SideQuestionContext::default(),
        }
    }

    /// 设置上下文
    pub fn with_context(mut self, context: SideQuestionContext) -> Self {
        self.context = context;
        self
    }

    /// 标记为处理中
    pub fn mark_processing(&mut self) {
        self.status = SideQuestionStatus::Processing;
    }

    /// 设置响应
    pub fn set_response(&mut self, response: impl Into<String>) {
        self.response = Some(response.into());
        self.status = SideQuestionStatus::Completed;
        self.completed_at = Some(chrono::Utc::now().to_rfc3339());
    }

    /// 标记为失败
    pub fn mark_failed(&mut self) {
        self.status = SideQuestionStatus::Failed;
        self.completed_at = Some(chrono::Utc::now().to_rfc3339());
    }

    /// 转换为消息
    pub fn to_message(&self) -> ChatMessage {
        let content = if let Some(response) = &self.response {
            format!("**Q**: {}\n\n**A**: {}", self.question, response)
        } else {
            self.question.clone()
        };

        ChatMessage {
            id: self.id.clone(),
            role: MessageRole::User,
            content,
            timestamp: self.created_at.clone(),
            attachments: vec![],
            metadata: MessageMetadata {
                is_error: self.status == SideQuestionStatus::Failed,
                is_streaming: self.status == SideQuestionStatus::Processing,
                model: None,
                latency_ms: None,
                edits: vec![],
            },
            token_usage: None,
        }
    }
}

/// 侧边提问管理器
#[derive(Clone, Debug)]
pub struct SideQuestionManager {
    questions: Vec<SideQuestion>,
    max_concurrent: usize,
}

impl SideQuestionManager {
    /// 创建新的管理器
    pub fn new() -> Self {
        Self {
            questions: Vec::new(),
            max_concurrent: 3,
        }
    }

    /// 创建侧边提问
    pub fn create_question(
        &mut self,
        parent_session_id: impl Into<String>,
        question: impl Into<String>,
    ) -> SideQuestion {
        let sq = SideQuestion::new(parent_session_id, question);
        self.questions.push(sq.clone());
        sq
    }

    /// 获取提问
    pub fn get_question(&self, id: &str) -> Option<&SideQuestion> {
        self.questions.iter().find(|q| q.id == id)
    }

    /// 获取提问（可变）
    pub fn get_question_mut(&mut self, id: &str) -> Option<&mut SideQuestion> {
        self.questions.iter_mut().find(|q| q.id == id)
    }

    /// 获取指定会话的所有侧边提问
    pub fn get_by_session(&self, session_id: &str) -> Vec<&SideQuestion> {
        self.questions
            .iter()
            .filter(|q| q.parent_session_id == session_id)
            .collect()
    }

    /// 获取进行中的提问数量
    pub fn processing_count(&self) -> usize {
        self.questions
            .iter()
            .filter(|q| q.status == SideQuestionStatus::Processing)
            .count()
    }

    /// 检查是否可以创建新的提问
    pub fn can_create(&self) -> bool {
        self.processing_count() < self.max_concurrent
    }

    /// 更新提问响应
    pub fn update_response(
        &mut self,
        id: &str,
        response: impl Into<String>,
    ) -> Option<&SideQuestion> {
        if let Some(q) = self.get_question_mut(id) {
            q.set_response(response);
            return Some(q);
        }
        None
    }

    /// 标记提问失败
    pub fn mark_failed(&mut self, id: &str) -> Option<&SideQuestion> {
        if let Some(q) = self.get_question_mut(id) {
            q.mark_failed();
            return Some(q);
        }
        None
    }

    /// 删除提问
    pub fn delete_question(&mut self, id: &str) -> bool {
        let initial_len = self.questions.len();
        self.questions.retain(|q| q.id != id);
        self.questions.len() < initial_len
    }

    /// 清理已完成的提问
    pub fn cleanup_completed(&mut self, keep_count: usize) {
        let completed: Vec<_> = self
            .questions
            .iter()
            .enumerate()
            .filter(|(_, q)| {
                q.status == SideQuestionStatus::Completed || q.status == SideQuestionStatus::Failed
            })
            .map(|(i, _)| i)
            .collect();

        // 保留最新的 keep_count 个
        let to_remove: Vec<_> = completed.iter().rev().skip(keep_count).collect();

        for &idx in to_remove {
            if idx < self.questions.len() {
                self.questions.remove(idx);
            }
        }
    }

    /// 获取所有提问
    pub fn list_all(&self) -> &[SideQuestion] {
        &self.questions
    }

    /// 清空所有提问
    pub fn clear_all(&mut self) {
        self.questions.clear();
    }
}

impl Default for SideQuestionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 侧边提问 UI 状态
#[derive(Clone, Debug)]
pub struct SideQuestionUIState {
    pub is_open: bool,
    pub current_question: Option<String>,
    pub questions: Vec<SideQuestion>,
    pub selected_question_id: Option<String>,
}

impl SideQuestionUIState {
    pub fn new() -> Self {
        Self {
            is_open: false,
            current_question: None,
            questions: Vec::new(),
            selected_question_id: None,
        }
    }

    pub fn open(&mut self) {
        self.is_open = true;
    }

    pub fn close(&mut self) {
        self.is_open = false;
    }

    pub fn toggle(&mut self) {
        self.is_open = !self.is_open;
    }

    pub fn set_question(&mut self, question: impl Into<String>) {
        self.current_question = Some(question.into());
    }

    pub fn clear_question(&mut self) {
        self.current_question = None;
    }

    pub fn add_question(&mut self, question: SideQuestion) {
        self.questions.push(question);
    }

    pub fn select_question(&mut self, id: impl Into<String>) {
        self.selected_question_id = Some(id.into());
    }
}

impl Default for SideQuestionUIState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_side_question_creation() {
        let sq = SideQuestion::new("session-1", "What is this?");
        assert_eq!(sq.parent_session_id, "session-1");
        assert_eq!(sq.question, "What is this?");
        assert_eq!(sq.status, SideQuestionStatus::Pending);
        assert!(sq.response.is_none());
    }

    #[test]
    fn test_side_question_lifecycle() {
        let mut sq = SideQuestion::new("session-1", "Test question");

        sq.mark_processing();
        assert_eq!(sq.status, SideQuestionStatus::Processing);

        sq.set_response("Test answer");
        assert_eq!(sq.status, SideQuestionStatus::Completed);
        assert_eq!(sq.response, Some("Test answer".to_string()));
        assert!(sq.completed_at.is_some());
    }

    #[test]
    fn test_side_question_manager() {
        let mut manager = SideQuestionManager::new();

        let sq = manager.create_question("session-1", "Q1");
        assert_eq!(manager.list_all().len(), 1);

        manager.update_response(&sq.id, "A1");
        let updated = manager.get_question(&sq.id).unwrap();
        assert_eq!(updated.response, Some("A1".to_string()));
    }

    #[test]
    fn test_concurrent_limit() {
        let mut manager = SideQuestionManager::new();

        // 创建3个处理中的问题
        for i in 0..3 {
            let sq = manager.create_question("session-1", format!("Q{}", i));
            if let Some(q) = manager.get_question_mut(&sq.id) {
                q.mark_processing();
            }
        }

        assert!(!manager.can_create()); // 已达上限
    }
}
