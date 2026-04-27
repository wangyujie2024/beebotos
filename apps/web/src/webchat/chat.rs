//! 聊天界面组件
//!
//! 提供消息列表、消息输入、消息操作等功能

use super::{ChatMessage, MessageRole};

/// 聊天界面控制器
pub struct ChatInterface;

impl ChatInterface {
    /// 格式化消息时间
    pub fn format_timestamp(timestamp: &str) -> String {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(timestamp) {
            let local = dt.with_timezone(&chrono::Local);
            local.format("%H:%M").to_string()
        } else {
            timestamp.to_string()
        }
    }

    /// 格式化消息日期
    pub fn format_date(timestamp: &str) -> String {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(timestamp) {
            let local = dt.with_timezone(&chrono::Local);
            let today = chrono::Local::now().date_naive();
            let msg_date = local.date_naive();

            if msg_date == today {
                "Today".to_string()
            } else if msg_date == today.pred_opt().unwrap_or(today) {
                "Yesterday".to_string()
            } else {
                local.format("%Y-%m-%d").to_string()
            }
        } else {
            timestamp.to_string()
        }
    }

    /// 截断长消息
    pub fn truncate_message(content: &str, max_length: usize) -> String {
        if content.len() > max_length {
            format!("{}...", &content[..max_length])
        } else {
            content.to_string()
        }
    }

    /// 检测快捷指令
    pub fn detect_slash_command(content: &str) -> Option<(&str, &str)> {
        if content.starts_with('/') {
            let parts: Vec<&str> = content.splitn(2, ' ').collect();
            if parts.len() >= 1 {
                let command = parts[0];
                let args = parts.get(1).unwrap_or(&"");
                return Some((command, args));
            }
        }
        None
    }
}

/// 消息列表控制器
pub struct MessageList;

impl MessageList {
    /// 分组消息（按日期）
    pub fn group_by_date(messages: &[ChatMessage]) -> Vec<(String, Vec<&ChatMessage>)> {
        use std::collections::BTreeMap;

        let mut groups: BTreeMap<String, Vec<&ChatMessage>> = BTreeMap::new();

        for msg in messages {
            let date = ChatInterface::format_date(&msg.timestamp);
            groups.entry(date).or_default().push(msg);
        }

        groups.into_iter().collect()
    }

    /// 查找消息索引
    pub fn find_message_index(messages: &[ChatMessage], id: &str) -> Option<usize> {
        messages.iter().position(|m| m.id == id)
    }
}

/// 消息输入控制器
pub struct MessageComposer;

impl MessageComposer {
    /// 计算输入字符数
    pub fn count_characters(content: &str) -> usize {
        content.chars().count()
    }

    /// 检查是否是有效输入
    pub fn is_valid_input(content: &str) -> bool {
        let trimmed = content.trim();
        !trimmed.is_empty() && trimmed.len() <= 10000
    }

    /// 处理输入快捷键
    pub fn handle_shortcut(content: &str, key: &str, shift: bool) -> InputAction {
        match key {
            "Enter" => {
                if shift {
                    InputAction::InsertNewLine
                } else {
                    InputAction::SendMessage
                }
            }
            "Escape" => InputAction::ClearInput,
            "/" => {
                if content.is_empty() {
                    InputAction::ShowSlashCommands
                } else {
                    InputAction::InsertCharacter('/')
                }
            }
            _ => InputAction::InsertCharacter(key.chars().next().unwrap_or(' ')),
        }
    }

    /// 预处理消息内容
    pub fn preprocess_content(content: &str) -> String {
        content
            .lines()
            .map(|line| line.trim_end())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    }
}

/// 输入动作
#[derive(Clone, Debug, PartialEq)]
pub enum InputAction {
    SendMessage,
    InsertNewLine,
    ClearInput,
    ShowSlashCommands,
    InsertCharacter(char),
    None,
}

/// 消息操作
pub struct MessageActions;

impl MessageActions {
    /// 复制消息内容
    pub async fn copy_content(content: &str) -> Result<(), super::ClipboardError> {
        super::ClipboardManager::copy_text(content).await
    }

    /// 编辑消息
    pub fn edit_message(messages: &mut [ChatMessage], id: &str, new_content: &str) -> bool {
        if let Some(msg) = messages.iter_mut().find(|m| m.id == id) {
            // 保存编辑历史
            let edit = super::MessageEdit {
                timestamp: chrono::Utc::now().to_rfc3339(),
                previous_content: msg.content.clone(),
            };
            msg.metadata.edits.push(edit);

            // 更新内容
            msg.content = new_content.to_string();
            true
        } else {
            false
        }
    }

    /// 删除消息
    pub fn delete_message(messages: &mut Vec<ChatMessage>, id: &str) -> bool {
        let initial_len = messages.len();
        messages.retain(|m| m.id != id);
        messages.len() < initial_len
    }

    /// 重新生成回复
    pub fn regenerate_response(
        messages: &mut Vec<ChatMessage>,
        user_message_id: &str,
    ) -> Option<String> {
        // 找到用户消息后的助手消息并删除
        if let Some(idx) = messages.iter().position(|m| m.id == user_message_id) {
            // 收集要删除的索引
            let to_remove: Vec<usize> = messages
                .iter()
                .enumerate()
                .filter(|(i, m)| *i > idx && m.role == MessageRole::Assistant)
                .map(|(i, _)| i)
                .collect();

            // 从后往前删除
            for i in to_remove.iter().rev() {
                messages.remove(*i);
            }

            if !to_remove.is_empty() {
                return Some(user_message_id.to_string());
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_timestamp() {
        let ts = "2026-03-14T10:30:00Z";
        let formatted = ChatInterface::format_timestamp(ts);
        // 格式化结果取决于时区，但至少应该包含时间
        assert!(!formatted.is_empty());
    }

    #[test]
    fn test_detect_slash_command() {
        assert_eq!(
            ChatInterface::detect_slash_command("/btw what is this?"),
            Some(("/btw", "what is this?"))
        );

        assert_eq!(
            ChatInterface::detect_slash_command("/clear"),
            Some(("/clear", ""))
        );

        assert_eq!(ChatInterface::detect_slash_command("Hello world"), None);
    }

    #[test]
    fn test_input_validation() {
        assert!(MessageComposer::is_valid_input("Hello"));
        assert!(!MessageComposer::is_valid_input(""));
        assert!(!MessageComposer::is_valid_input("   "));
    }

    #[test]
    fn test_input_shortcuts() {
        assert_eq!(
            MessageComposer::handle_shortcut("", "Enter", false),
            InputAction::SendMessage
        );

        assert_eq!(
            MessageComposer::handle_shortcut("", "Enter", true),
            InputAction::InsertNewLine
        );

        assert_eq!(
            MessageComposer::handle_shortcut("", "/", false),
            InputAction::ShowSlashCommands
        );
    }

    #[test]
    fn test_preprocess_content() {
        let input = "  Line 1  \nLine 2  \n  \n";
        let output = MessageComposer::preprocess_content(input);
        assert_eq!(output, "Line 1\nLine 2");
    }
}
