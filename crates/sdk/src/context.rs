//! Agent Context
//!
//! Execution context for agents.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::{SessionId, TaskId};
use crate::AgentId;

/// Agent execution context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContext {
    pub agent_id: AgentId,
    pub session_id: SessionId,
    pub task_id: Option<TaskId>,
    pub parent_id: Option<AgentId>,
    pub depth: u8,
    pub capabilities: Vec<String>,
    pub metadata: HashMap<String, String>,
    pub quota: ResourceQuota,
}

/// Resource quota
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceQuota {
    pub max_tokens: u64,
    pub max_execution_time_ms: u64,
    pub max_memory_mb: u64,
    pub max_storage_mb: u64,
}

impl Default for ResourceQuota {
    fn default() -> Self {
        Self {
            max_tokens: 1_000_000,
            max_execution_time_ms: 300_000, // 5 minutes
            max_memory_mb: 512,
            max_storage_mb: 100,
        }
    }
}

impl AgentContext {
    /// Create new context
    pub fn new(agent_id: AgentId) -> Self {
        Self {
            agent_id,
            session_id: SessionId::new(),
            task_id: None,
            parent_id: None,
            depth: 0,
            capabilities: vec![],
            metadata: HashMap::new(),
            quota: ResourceQuota::default(),
        }
    }

    /// With parent context (for subagents)
    pub fn with_parent(mut self, parent: &AgentContext) -> Self {
        self.parent_id = Some(parent.agent_id.clone());
        self.depth = parent.depth + 1;
        self.capabilities = parent.capabilities.clone();
        self.quota = ResourceQuota {
            max_tokens: parent.quota.max_tokens / 2,
            max_execution_time_ms: parent.quota.max_execution_time_ms / 2,
            max_memory_mb: parent.quota.max_memory_mb / 2,
            max_storage_mb: parent.quota.max_storage_mb / 2,
        };
        self
    }

    /// With task ID
    pub fn with_task(mut self, task_id: TaskId) -> Self {
        self.task_id = Some(task_id);
        self
    }

    /// With capability
    pub fn with_capability(mut self, capability: impl Into<String>) -> Self {
        self.capabilities.push(capability.into());
        self
    }

    /// With metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Check if has capability
    pub fn has_capability(&self, cap: &str) -> bool {
        self.capabilities.contains(&cap.to_string()) || self.capabilities.contains(&"*".to_string())
    }

    /// Get effective quota (accounting for parent)
    pub fn effective_quota(&self) -> ResourceQuota {
        self.quota.clone()
    }

    /// Check if this is a root agent
    pub fn is_root(&self) -> bool {
        self.parent_id.is_none()
    }

    /// Check if this is a subagent
    pub fn is_subagent(&self) -> bool {
        self.parent_id.is_some()
    }

    /// Get session key in OpenClaw format
    pub fn session_key(&self) -> String {
        format!("agent:{}:session:{}", self.agent_id, self.session_id)
    }

    /// Get transcript path
    pub fn transcript_path(&self) -> String {
        format!("data/transcripts/{}/{}", self.agent_id, self.session_id)
    }

    /// Get workspace path
    pub fn workspace_path(&self) -> String {
        format!("data/workspaces/{}/{}", self.agent_id, self.session_id)
    }
}

impl Default for AgentContext {
    fn default() -> Self {
        Self::new(AgentId::new())
    }
}

/// Execution scope
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionScope {
    /// Sandbox with restricted permissions
    Sandbox,
    /// Normal execution
    Normal,
    /// Elevated permissions
    Privileged,
}

/// Context builder
pub struct ContextBuilder {
    agent_id: AgentId,
    parent: Option<AgentContext>,
    capabilities: Vec<String>,
    quota: ResourceQuota,
    scope: ExecutionScope,
}

impl ContextBuilder {
    pub fn new(agent_id: AgentId) -> Self {
        Self {
            agent_id,
            parent: None,
            capabilities: vec![],
            quota: ResourceQuota::default(),
            scope: ExecutionScope::Normal,
        }
    }

    pub fn with_parent(mut self, parent: AgentContext) -> Self {
        self.parent = Some(parent);
        self
    }

    pub fn with_capability(mut self, cap: impl Into<String>) -> Self {
        self.capabilities.push(cap.into());
        self
    }

    pub fn with_quota(mut self, quota: ResourceQuota) -> Self {
        self.quota = quota;
        self
    }

    pub fn with_scope(mut self, scope: ExecutionScope) -> Self {
        self.scope = scope;
        self
    }

    pub fn build(self) -> AgentContext {
        let mut ctx = AgentContext::new(self.agent_id);
        ctx.capabilities = self.capabilities;
        ctx.quota = self.quota;

        if let Some(parent) = self.parent {
            ctx = ctx.with_parent(&parent);
        }

        // Add scope to metadata
        ctx.metadata.insert(
            "scope".to_string(),
            match self.scope {
                ExecutionScope::Sandbox => "sandbox".to_string(),
                ExecutionScope::Normal => "normal".to_string(),
                ExecutionScope::Privileged => "privileged".to_string(),
            },
        );

        ctx
    }
}
