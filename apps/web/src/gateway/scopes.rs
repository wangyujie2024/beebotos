//! 权限范围管理
//!
//! 实现 Gateway 的精细化权限控制

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// Gateway 权限范围
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum GatewayScope {
    // 浏览器权限
    BrowserRead,
    BrowserWrite,
    BrowserAdmin,

    // Agent 权限
    AgentRead,
    AgentWrite,
    AgentAdmin,

    // 聊天权限
    ChatRead,
    ChatWrite,

    // 设置权限
    SettingsRead,
    SettingsWrite,

    // 管理权限
    Admin,
}

impl GatewayScope {
    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            GatewayScope::BrowserRead => "browser:read",
            GatewayScope::BrowserWrite => "browser:write",
            GatewayScope::BrowserAdmin => "browser:admin",
            GatewayScope::AgentRead => "agent:read",
            GatewayScope::AgentWrite => "agent:write",
            GatewayScope::AgentAdmin => "agent:admin",
            GatewayScope::ChatRead => "chat:read",
            GatewayScope::ChatWrite => "chat:write",
            GatewayScope::SettingsRead => "settings:read",
            GatewayScope::SettingsWrite => "settings:write",
            GatewayScope::Admin => "admin",
        }
    }

    /// 从字符串解析
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "browser:read" => Some(GatewayScope::BrowserRead),
            "browser:write" => Some(GatewayScope::BrowserWrite),
            "browser:admin" => Some(GatewayScope::BrowserAdmin),
            "agent:read" => Some(GatewayScope::AgentRead),
            "agent:write" => Some(GatewayScope::AgentWrite),
            "agent:admin" => Some(GatewayScope::AgentAdmin),
            "chat:read" => Some(GatewayScope::ChatRead),
            "chat:write" => Some(GatewayScope::ChatWrite),
            "settings:read" => Some(GatewayScope::SettingsRead),
            "settings:write" => Some(GatewayScope::SettingsWrite),
            "admin" => Some(GatewayScope::Admin),
            _ => None,
        }
    }

    /// 获取相关读权限
    pub fn read_scope(&self) -> Option<GatewayScope> {
        match self {
            GatewayScope::BrowserWrite => Some(GatewayScope::BrowserRead),
            GatewayScope::AgentWrite => Some(GatewayScope::AgentRead),
            GatewayScope::ChatWrite => Some(GatewayScope::ChatRead),
            GatewayScope::SettingsWrite => Some(GatewayScope::SettingsRead),
            _ => None,
        }
    }

    /// 检查是否是写权限
    pub fn is_write(&self) -> bool {
        matches!(
            self,
            GatewayScope::BrowserWrite
                | GatewayScope::AgentWrite
                | GatewayScope::ChatWrite
                | GatewayScope::SettingsWrite
        )
    }

    /// 检查是否是管理权限
    pub fn is_admin(&self) -> bool {
        matches!(
            self,
            GatewayScope::BrowserAdmin | GatewayScope::AgentAdmin | GatewayScope::Admin
        )
    }
}

/// 权限范围管理器
#[derive(Clone, Debug)]
pub struct ScopeManager {
    scopes: HashSet<GatewayScope>,
}

impl ScopeManager {
    /// 创建新的管理器
    pub fn new(scopes: Vec<GatewayScope>) -> Self {
        Self {
            scopes: scopes.into_iter().collect(),
        }
    }

    /// 检查是否有指定权限
    pub fn has_scope(&self, scope: GatewayScope) -> bool {
        // Admin 拥有所有权限
        if self.scopes.contains(&GatewayScope::Admin) {
            return true;
        }

        // 检查具体权限
        if self.scopes.contains(&scope) {
            return true;
        }

        // 检查读权限：如果有对应的写权限，则也有读权限
        // 原来的 read_scope 逻辑已被下面的代码替代
        let _ = scope.read_scope();

        // 如果查询的是读权限，检查是否有对应的写权限
        if !scope.is_write() {
            // 找到对应的写权限
            let write_scope = match scope {
                GatewayScope::BrowserRead => Some(GatewayScope::BrowserWrite),
                GatewayScope::AgentRead => Some(GatewayScope::AgentWrite),
                GatewayScope::ChatRead => Some(GatewayScope::ChatWrite),
                GatewayScope::SettingsRead => Some(GatewayScope::SettingsWrite),
                _ => None,
            };
            if let Some(ws) = write_scope {
                if self.scopes.contains(&ws) {
                    return true;
                }
            }
        }

        false
    }

    /// 检查是否有所有指定权限
    pub fn has_all_scopes(&self, scopes: &[GatewayScope]) -> bool {
        scopes.iter().all(|s| self.has_scope(s.clone()))
    }

    /// 检查是否有任意指定权限
    pub fn has_any_scope(&self, scopes: &[GatewayScope]) -> bool {
        scopes.iter().any(|s| self.has_scope(s.clone()))
    }

    /// 添加权限
    pub fn add_scope(&mut self, scope: GatewayScope) {
        self.scopes.insert(scope);
    }

    /// 移除权限
    pub fn remove_scope(&mut self, scope: &GatewayScope) {
        self.scopes.remove(scope);
    }

    /// 获取所有权限
    pub fn get_scopes(&self) -> Vec<GatewayScope> {
        self.scopes.iter().cloned().collect()
    }

    /// 获取权限字符串列表
    pub fn get_scope_strings(&self) -> Vec<String> {
        self.scopes.iter().map(|s| s.as_str().to_string()).collect()
    }

    /// 清空所有权限
    pub fn clear(&mut self) {
        self.scopes.clear();
    }

    /// 从字符串列表解析
    pub fn from_strings(scopes: &[String]) -> Self {
        let parsed: Vec<_> = scopes
            .iter()
            .filter_map(|s| GatewayScope::from_str(s))
            .collect();
        Self::new(parsed)
    }
}

impl Default for ScopeManager {
    fn default() -> Self {
        Self::new(vec![])
    }
}

/// 权限检查守卫
pub struct ScopeGuard {
    required_scope: GatewayScope,
}

impl ScopeGuard {
    pub fn new(scope: GatewayScope) -> Self {
        Self {
            required_scope: scope,
        }
    }

    pub fn check(&self, manager: &ScopeManager) -> bool {
        manager.has_scope(self.required_scope.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_as_str() {
        assert_eq!(GatewayScope::BrowserRead.as_str(), "browser:read");
        assert_eq!(GatewayScope::Admin.as_str(), "admin");
    }

    #[test]
    fn test_scope_from_str() {
        assert_eq!(
            GatewayScope::from_str("browser:read"),
            Some(GatewayScope::BrowserRead)
        );
        assert_eq!(GatewayScope::from_str("unknown"), None);
    }

    #[test]
    fn test_scope_manager() {
        let manager = ScopeManager::new(vec![
            GatewayScope::BrowserRead,
            GatewayScope::ChatRead,
            GatewayScope::ChatWrite,
        ]);

        assert!(manager.has_scope(GatewayScope::BrowserRead));
        assert!(manager.has_scope(GatewayScope::ChatWrite));
        assert!(!manager.has_scope(GatewayScope::BrowserWrite));
        assert!(!manager.has_scope(GatewayScope::Admin));
    }

    #[test]
    fn test_admin_scope() {
        let manager = ScopeManager::new(vec![GatewayScope::Admin]);

        // Admin 应该拥有所有权限
        assert!(manager.has_scope(GatewayScope::BrowserRead));
        assert!(manager.has_scope(GatewayScope::BrowserWrite));
        assert!(manager.has_scope(GatewayScope::ChatRead));
        assert!(manager.has_scope(GatewayScope::AgentAdmin));
    }

    #[test]
    fn test_has_all_scopes() {
        let manager =
            ScopeManager::new(vec![GatewayScope::BrowserRead, GatewayScope::BrowserWrite]);

        assert!(manager.has_all_scopes(&[GatewayScope::BrowserRead, GatewayScope::BrowserWrite,]));

        assert!(!manager.has_all_scopes(&[GatewayScope::BrowserRead, GatewayScope::ChatRead,]));
    }

    #[test]
    fn test_has_any_scope() {
        let manager = ScopeManager::new(vec![GatewayScope::BrowserRead]);

        assert!(manager.has_any_scope(&[GatewayScope::BrowserRead, GatewayScope::ChatRead,]));

        assert!(!manager.has_any_scope(&[GatewayScope::ChatRead, GatewayScope::ChatWrite,]));
    }
}
