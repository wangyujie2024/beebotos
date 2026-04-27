//! 会话管理模块
//!
//! 提供会话创建、持久化、历史记录管理等功能

use std::collections::HashMap;

use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};

use super::{ChatSession, SessionContext, SessionFilter, TokenUsage};

/// 会话存储键
const SESSION_STORAGE_KEY: &str = "beebotos_webchat_sessions";
#[allow(dead_code)]
const CURRENT_SESSION_KEY: &str = "beebotos_webchat_current_session";

/// 会话管理器
#[derive(Clone, Debug)]
pub struct SessionManager {
    sessions: HashMap<String, ChatSession>,
    current_session_id: Option<String>,
    persistence_enabled: bool,
}

/// 序列化的会话存储
#[derive(Clone, Debug, Serialize, Deserialize)]
struct SessionStorage {
    version: String,
    sessions: Vec<ChatSession>,
    current_session_id: Option<String>,
    last_sync: String,
}

impl SessionManager {
    /// 创建新的会话管理器
    pub fn new() -> Self {
        let mut manager = Self {
            sessions: HashMap::new(),
            current_session_id: None,
            persistence_enabled: true,
        };

        // 尝试从本地存储加载
        if let Err(e) = manager.load_from_storage() {
            #[cfg(target_arch = "wasm32")]
            web_sys::console::warn_1(&format!("Failed to load sessions: {:?}", e).into());
            #[cfg(not(target_arch = "wasm32"))]
            eprintln!("Failed to load sessions: {:?}", e);
        }

        manager
    }

    /// 创建新会话
    pub fn create_session(&mut self, title: impl Into<String>) -> ChatSession {
        let session = ChatSession::new(title);
        let id = session.id.clone();

        self.sessions.insert(id.clone(), session.clone());
        self.current_session_id = Some(id);

        if self.persistence_enabled {
            let _ = self.save_to_storage();
        }

        session
    }

    /// 获取当前会话
    pub fn current_session(&self) -> Option<&ChatSession> {
        self.current_session_id
            .as_ref()
            .and_then(|id| self.sessions.get(id))
    }

    /// 获取当前会话（可变）
    pub fn current_session_mut(&mut self) -> Option<&mut ChatSession> {
        self.current_session_id
            .as_ref()
            .and_then(|id| self.sessions.get_mut(id))
    }

    /// 设置当前会话
    pub fn set_current_session(&mut self, session_id: &str) -> Result<(), SessionError> {
        if !self.sessions.contains_key(session_id) {
            return Err(SessionError::NotFound(session_id.to_string()));
        }

        self.current_session_id = Some(session_id.to_string());

        if self.persistence_enabled {
            let _ = self.save_to_storage();
        }

        Ok(())
    }

    /// 获取会话
    pub fn get_session(&self, id: &str) -> Option<&ChatSession> {
        self.sessions.get(id)
    }

    /// 获取会话（可变）
    pub fn get_session_mut(&mut self, id: &str) -> Option<&mut ChatSession> {
        self.sessions.get_mut(id)
    }

    /// 删除会话
    pub fn delete_session(&mut self, id: &str) -> Result<(), SessionError> {
        if !self.sessions.contains_key(id) {
            return Err(SessionError::NotFound(id.to_string()));
        }

        self.sessions.remove(id);

        // 如果删除的是当前会话，切换到第一个可用会话
        if self.current_session_id.as_deref() == Some(id) {
            self.current_session_id = self.sessions.keys().next().cloned();
        }

        if self.persistence_enabled {
            let _ = self.save_to_storage();
        }

        Ok(())
    }

    /// 列出所有会话
    pub fn list_sessions(&self) -> Vec<&ChatSession> {
        let mut sessions: Vec<_> = self.sessions.values().collect();

        // 排序：固定的在前，然后按更新时间倒序
        sessions.sort_by(|a, b| {
            if a.is_pinned && !b.is_pinned {
                std::cmp::Ordering::Less
            } else if !a.is_pinned && b.is_pinned {
                std::cmp::Ordering::Greater
            } else {
                b.updated_at.cmp(&a.updated_at)
            }
        });

        sessions
    }

    /// 过滤会话
    pub fn filter_sessions(&self, filter: &SessionFilter) -> Vec<&ChatSession> {
        let mut sessions: Vec<_> = self
            .sessions
            .values()
            .filter(|s| {
                // 归档过滤
                if !filter.include_archived && s.is_archived {
                    return false;
                }

                // 固定过滤
                if filter.only_pinned && !s.is_pinned {
                    return false;
                }

                // 搜索过滤
                if let Some(query) = &filter.search_query {
                    let query = query.to_lowercase();
                    let title_match = s.title.to_lowercase().contains(&query);
                    let content_match = s
                        .messages
                        .iter()
                        .any(|m| m.content.to_lowercase().contains(&query));
                    if !title_match && !content_match {
                        return false;
                    }
                }

                // 日期范围过滤
                if let Some(date_from) = &filter.date_from {
                    if s.created_at < *date_from {
                        return false;
                    }
                }
                if let Some(date_to) = &filter.date_to {
                    if s.created_at > *date_to {
                        return false;
                    }
                }

                true
            })
            .collect();

        // 排序
        sessions.sort_by(|a, b| {
            if a.is_pinned && !b.is_pinned {
                std::cmp::Ordering::Less
            } else if !a.is_pinned && b.is_pinned {
                std::cmp::Ordering::Greater
            } else {
                b.updated_at.cmp(&a.updated_at)
            }
        });

        sessions
    }

    /// 固定/取消固定会话
    pub fn toggle_pin(&mut self, id: &str) -> Result<bool, SessionError> {
        let session = self
            .sessions
            .get_mut(id)
            .ok_or_else(|| SessionError::NotFound(id.to_string()))?;

        session.toggle_pin();
        let is_pinned = session.is_pinned;

        if self.persistence_enabled {
            let _ = self.save_to_storage();
        }

        Ok(is_pinned)
    }

    /// 归档会话
    pub fn archive_session(&mut self, id: &str) -> Result<(), SessionError> {
        let session = self
            .sessions
            .get_mut(id)
            .ok_or_else(|| SessionError::NotFound(id.to_string()))?;

        session.is_archived = true;
        session.updated_at = chrono::Utc::now().to_rfc3339();

        if self.persistence_enabled {
            let _ = self.save_to_storage();
        }

        Ok(())
    }

    /// 恢复归档会话
    pub fn unarchive_session(&mut self, id: &str) -> Result<(), SessionError> {
        let session = self
            .sessions
            .get_mut(id)
            .ok_or_else(|| SessionError::NotFound(id.to_string()))?;

        session.is_archived = false;
        session.updated_at = chrono::Utc::now().to_rfc3339();

        if self.persistence_enabled {
            let _ = self.save_to_storage();
        }

        Ok(())
    }

    /// 更新会话上下文
    pub fn update_context(
        &mut self,
        id: &str,
        context: SessionContext,
    ) -> Result<(), SessionError> {
        let session = self
            .sessions
            .get_mut(id)
            .ok_or_else(|| SessionError::NotFound(id.to_string()))?;

        session.context = context;
        session.updated_at = chrono::Utc::now().to_rfc3339();

        if self.persistence_enabled {
            let _ = self.save_to_storage();
        }

        Ok(())
    }

    /// 获取会话数量
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// 获取活跃（未归档）会话数量
    pub fn active_session_count(&self) -> usize {
        self.sessions.values().filter(|s| !s.is_archived).count()
    }

    /// 清空所有会话
    pub fn clear_all(&mut self) {
        self.sessions.clear();
        self.current_session_id = None;

        if self.persistence_enabled {
            let _ = self.save_to_storage();
        }
    }

    /// 启用持久化
    pub fn enable_persistence(&mut self) {
        self.persistence_enabled = true;
        let _ = self.save_to_storage();
    }

    /// 禁用持久化
    pub fn disable_persistence(&mut self) {
        self.persistence_enabled = false;
    }

    /// 保存到本地存储
    fn save_to_storage(&self) -> Result<(), SessionError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            // In non-wasm environment, just return Ok (no persistent storage)
            return Ok(());
        }

        #[cfg(target_arch = "wasm32")]
        {
            let storage_data = SessionStorage {
                version: "1.0".to_string(),
                sessions: self.sessions.values().cloned().collect(),
                current_session_id: self.current_session_id.clone(),
                last_sync: chrono::Utc::now().to_rfc3339(),
            };

            let json = serde_json::to_string(&storage_data)
                .map_err(|e| SessionError::Serialization(e.to_string()))?;

            LocalStorage::set(SESSION_STORAGE_KEY, json)
                .map_err(|e| SessionError::Storage(e.to_string()))?;

            Ok(())
        }
    }

    /// 从本地存储加载
    fn load_from_storage(&mut self) -> Result<(), SessionError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            // In non-wasm environment, just return Ok (no persistent storage)
            return Ok(());
        }

        #[cfg(target_arch = "wasm32")]
        {
            let json: String = LocalStorage::get(SESSION_STORAGE_KEY)
                .map_err(|e| SessionError::Storage(e.to_string()))?;

            let storage_data: SessionStorage = serde_json::from_str(&json)
                .map_err(|e| SessionError::Serialization(e.to_string()))?;

            self.sessions = storage_data
                .sessions
                .into_iter()
                .map(|s| (s.id.clone(), s))
                .collect();
            self.current_session_id = storage_data.current_session_id;

            Ok(())
        }
    }

    /// 导出所有会话
    pub fn export_sessions(&self) -> Result<String, SessionError> {
        let storage_data = SessionStorage {
            version: "1.0".to_string(),
            sessions: self.sessions.values().cloned().collect(),
            current_session_id: self.current_session_id.clone(),
            last_sync: chrono::Utc::now().to_rfc3339(),
        };

        serde_json::to_string_pretty(&storage_data)
            .map_err(|e| SessionError::Serialization(e.to_string()))
    }

    /// 导入会话
    pub fn import_sessions(&mut self, json: &str) -> Result<usize, SessionError> {
        let storage_data: SessionStorage =
            serde_json::from_str(json).map_err(|e| SessionError::Serialization(e.to_string()))?;

        let count = storage_data.sessions.len();

        for session in storage_data.sessions {
            self.sessions.insert(session.id.clone(), session);
        }

        if self.persistence_enabled {
            let _ = self.save_to_storage();
        }

        Ok(count)
    }

    /// 计算所有会话的总 token 用量
    pub fn calculate_total_usage(&self) -> TokenUsage {
        let mut total = TokenUsage::new("all");

        for session in self.sessions.values() {
            total.prompt_tokens += session.total_token_usage.prompt_tokens;
            total.completion_tokens += session.total_token_usage.completion_tokens;
            total.total_tokens += session.total_token_usage.total_tokens;
            total.estimated_cost += session.total_token_usage.estimated_cost;
        }

        total
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 会话错误
#[derive(Clone, Debug)]
pub enum SessionError {
    NotFound(String),
    Storage(String),
    Serialization(String),
    ImportFailed(String),
    MaxSessionsReached(usize),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionError::NotFound(id) => write!(f, "Session not found: {}", id),
            SessionError::Storage(msg) => write!(f, "Storage error: {}", msg),
            SessionError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            SessionError::ImportFailed(msg) => write!(f, "Import failed: {}", msg),
            SessionError::MaxSessionsReached(max) => {
                write!(f, "Maximum number of sessions reached: {}", max)
            }
        }
    }
}

/// 会话持久化 trait
pub trait SessionPersistence {
    fn save(&self, session: &ChatSession) -> Result<(), SessionError>;
    fn load(&self, id: &str) -> Result<Option<ChatSession>, SessionError>;
    fn delete(&self, id: &str) -> Result<(), SessionError>;
    fn list(&self) -> Result<Vec<ChatSession>, SessionError>;
}

/// 本地存储持久化实现
pub struct LocalStoragePersistence;

impl LocalStoragePersistence {
    fn key_for(id: &str) -> String {
        format!("{}_{}", SESSION_STORAGE_KEY, id)
    }
}

impl SessionPersistence for LocalStoragePersistence {
    fn save(&self, session: &ChatSession) -> Result<(), SessionError> {
        let key = Self::key_for(&session.id);
        let json = serde_json::to_string(session)
            .map_err(|e| SessionError::Serialization(e.to_string()))?;

        LocalStorage::set(&key, json).map_err(|e| SessionError::Storage(e.to_string()))?;

        Ok(())
    }

    fn load(&self, id: &str) -> Result<Option<ChatSession>, SessionError> {
        let key = Self::key_for(id);

        let json: Result<String, _> = LocalStorage::get(&key);
        match json {
            Ok(json) => {
                let session = serde_json::from_str(&json)
                    .map_err(|e| SessionError::Serialization(e.to_string()))?;
                Ok(Some(session))
            }
            Err(_) => Ok(None),
        }
    }

    fn delete(&self, id: &str) -> Result<(), SessionError> {
        let key = Self::key_for(id);
        LocalStorage::delete(&key);
        Ok(())
    }

    fn list(&self) -> Result<Vec<ChatSession>, SessionError> {
        // 从聚合存储中读取
        let json: Result<String, _> = LocalStorage::get(SESSION_STORAGE_KEY);
        match json {
            Ok(json) => {
                let storage: SessionStorage = serde_json::from_str(&json)
                    .map_err(|e| SessionError::Serialization(e.to_string()))?;
                Ok(storage.sessions)
            }
            Err(_) => Ok(vec![]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_manager_creation() {
        let manager = SessionManager::new();
        assert_eq!(manager.session_count(), 0);
        assert!(manager.current_session().is_none());
    }

    #[test]
    fn test_create_session() {
        let mut manager = SessionManager::new();
        manager.disable_persistence(); // 避免测试时写入存储

        let session = manager.create_session("Test Session");
        assert_eq!(session.title, "Test Session");
        assert_eq!(manager.session_count(), 1);
        assert!(manager.current_session().is_some());
    }

    #[test]
    fn test_delete_session() {
        let mut manager = SessionManager::new();
        manager.disable_persistence();

        let session = manager.create_session("To Delete");
        let id = session.id.clone();

        assert!(manager.delete_session(&id).is_ok());
        assert_eq!(manager.session_count(), 0);
        assert!(manager.delete_session(&id).is_err()); // 再次删除应该失败
    }

    #[test]
    fn test_toggle_pin() {
        let mut manager = SessionManager::new();
        manager.disable_persistence();

        let session = manager.create_session("Pinned Session");
        let id = session.id.clone();

        assert!(!session.is_pinned);
        let result = manager.toggle_pin(&id).unwrap();
        assert!(result);

        let session = manager.get_session(&id).unwrap();
        assert!(session.is_pinned);
    }
}
