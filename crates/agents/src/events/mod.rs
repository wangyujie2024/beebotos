//! Events Module
//!
//! Agent 事件系统 - 复用 `beebotos_core::event::EventBus` 实现统一事件总线。
//!
//! # 架构变更
//!
//! 之前：agents 模块自己实现 EventBus
//! 现在：统一复用 core::EventBus，实现跨模块事件通信
//!
//! # 使用示例
//!
//! ```ignore
//! use beebotos_agents::events::AgentEventBus;
//! use beebotos_agents::events::AgentLifecycleEvent;
//!
//! # tokio::runtime::Runtime::new().unwrap().block_on(async {
//! // 创建事件总线
//! let event_bus = AgentEventBus::new();
//!
//! // 订阅事件
//! let mut rx = event_bus.subscribe("handler-1").await;
//!
//! // 发布事件
//! event_bus.emit(AgentLifecycleEvent::AgentStarted { agent_id: "agent-1".into() }.into()).await;
//! # });
//! ```

pub mod types;

use std::sync::Arc;

use beebotos_core::event::{Event, EventBus as CoreEventBus};
// 重新导出 core 的事件类型
pub use beebotos_core::event::{Event as CoreEvent, TxStatus};
use tokio::sync::mpsc;
// Re-export new SystemEventBus compatible types
pub use types::{AgentLifecycleEvent, AgentStateEvent, AgentTaskEvent, TaskEventType};

/// Agent 事件总线
///
/// 包装 `beebotos_core::event::EventBus`，提供 Agent 特定的事件功能。
/// 这是统一事件系统的核心组件，替代了之前独立实现的 EventBus。
#[derive(Debug, Clone)]
pub struct AgentEventBus {
    inner: Arc<CoreEventBus>,
}

impl AgentEventBus {
    /// 创建新的事件总线
    pub fn new() -> Self {
        Self {
            inner: Arc::new(CoreEventBus::new()),
        }
    }

    /// 从现有的 core EventBus 创建
    pub fn from_core(core_bus: Arc<CoreEventBus>) -> Self {
        Self { inner: core_bus }
    }

    /// 订阅事件
    ///
    /// 返回一个接收器，可以接收发布到该总线的事件
    pub async fn subscribe(&self, name: &str) -> mpsc::UnboundedReceiver<Event> {
        self.inner.subscribe(name).await
    }

    /// 取消订阅
    pub async fn unsubscribe(&self, name: &str) {
        self.inner.unsubscribe(name).await;
    }

    /// 发布事件到所有订阅者
    pub async fn emit(&self, event: Event) {
        self.inner.emit(event).await;
    }

    /// 发布过滤后的事件
    pub async fn emit_filtered<F>(&self, event: Event, filter: F)
    where
        F: Fn(&Event) -> bool,
    {
        self.inner.emit_filtered(event, filter).await;
    }

    /// 获取内部 EventBus 引用
    pub fn inner(&self) -> &Arc<CoreEventBus> {
        &self.inner
    }

    /// 转换为 Arc<CoreEventBus>
    pub fn into_core(self) -> Arc<CoreEventBus> {
        self.inner
    }
}

impl Default for AgentEventBus {
    fn default() -> Self {
        Self::new()
    }
}

// 为了保持向后兼容，保留 AgentEvent 类型别名
/// Agent 事件类型（兼容旧代码）
pub type AgentEvent = Event;

/// 事件处理器 trait
#[async_trait::async_trait]
pub trait EventHandler: Send + Sync {
    /// 处理事件
    async fn handle(&self, event: &Event);
    /// 获取处理器名称
    fn name(&self) -> &str;
}

/// 事件订阅管理器
///
/// 管理多个事件处理器，自动分发事件。
pub struct EventSubscriber {
    event_bus: AgentEventBus,
    handlers: Vec<Box<dyn EventHandler>>,
}

impl EventSubscriber {
    /// 创建新的事件订阅管理器
    pub fn new(event_bus: AgentEventBus) -> Self {
        Self {
            event_bus,
            handlers: vec![],
        }
    }

    /// 添加事件处理器
    pub fn add_handler(&mut self, handler: Box<dyn EventHandler>) {
        self.handlers.push(handler);
    }

    /// 启动事件分发
    pub fn start(self) {
        let event_bus = self.event_bus;

        tokio::spawn(async move {
            // 为每个处理器创建一个订阅
            for handler in self.handlers {
                let name = handler.name().to_string();
                let mut rx = event_bus.subscribe(&name).await;

                tokio::spawn(async move {
                    while let Some(event) = rx.recv().await {
                        handler.handle(&event).await;
                    }
                });
            }
        });
    }
}

/// 遗留代码兼容层
///
/// 为了保持与旧代码的兼容性，保留旧的事件类型定义。
/// 新项目应直接使用 `beebotos_core::event::Event`。
#[deprecated(since = "1.1.0", note = "Use beebotos_core::event::Event instead")]
pub mod legacy {
    use serde::{Deserialize, Serialize};
    use uuid::Uuid;

    /// 遗留事件类型
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum LegacyEvent {
        AgentStarted {
            agent_id: Uuid,
        },
        AgentStopped {
            agent_id: Uuid,
        },
        TaskCompleted {
            task_id: Uuid,
        },
        Error {
            source: String,
            message: String,
        },
        Custom {
            name: String,
            data: serde_json::Value,
        },
    }
}

#[cfg(test)]
mod tests {
    use beebotos_core::types::{AgentId, AgentStatus, Timestamp};

    use super::*;

    #[tokio::test]
    async fn test_agent_event_bus_new() {
        let bus = AgentEventBus::new();
        let mut rx = bus.subscribe("test").await;

        let event = Event::AgentLifecycle {
            agent_id: AgentId::new(),
            from: AgentStatus::Idle,
            to: AgentStatus::Running,
            timestamp: Timestamp::now(),
        };

        bus.emit(event.clone()).await;

        let received = rx.try_recv();
        assert!(received.is_ok());
    }

    #[tokio::test]
    async fn test_agent_event_bus_from_core() {
        let core_bus = Arc::new(CoreEventBus::new());
        let bus = AgentEventBus::from_core(core_bus.clone());

        let mut rx = bus.subscribe("test").await;

        let event = Event::Metric {
            name: "test_metric".to_string(),
            value: 1.0,
            labels: std::collections::HashMap::new(),
            timestamp: Timestamp::now(),
        };

        bus.emit(event).await;
        assert!(rx.try_recv().is_ok());
    }

    #[tokio::test]
    async fn test_event_filtering() {
        let bus = AgentEventBus::new();
        let mut rx = bus.subscribe("test").await;

        // 发布一个被过滤的事件
        let event = Event::AgentLifecycle {
            agent_id: AgentId::new(),
            from: AgentStatus::Idle,
            to: AgentStatus::Running,
            timestamp: Timestamp::now(),
        };

        // 使用过滤 - 只允许 Metric 事件
        bus.emit_filtered(event.clone(), |e| matches!(e, Event::Metric { .. }))
            .await;

        // 应该收不到事件，因为被过滤了
        assert!(rx.try_recv().is_err());

        // 发布一个 Metric 事件
        let metric_event = Event::Metric {
            name: "test".to_string(),
            value: 1.0,
            labels: std::collections::HashMap::new(),
            timestamp: Timestamp::now(),
        };

        bus.emit_filtered(metric_event, |e| matches!(e, Event::Metric { .. }))
            .await;

        assert!(rx.try_recv().is_ok());
    }
}
