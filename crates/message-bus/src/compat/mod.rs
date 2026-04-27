//! Compatibility layer for migrating existing modules to MessageBus
//!
//! This module provides adapters that allow existing code to work with
//! the new MessageBus without requiring immediate rewrites.

pub mod gateway;

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::{Message, MessageBus, SubscriptionId};

/// Adapter for Core EventBus (simplified version without beebotos_core
/// dependency)
///
/// This is a placeholder adapter for demonstration. The full implementation
/// would integrate with beebotos_core::event::EventBus
pub struct CoreEventBusAdapter<B: MessageBus> {
    inner: Arc<B>,
}

impl<B: MessageBus> CoreEventBusAdapter<B> {
    /// Create a new adapter
    pub fn new(inner: Arc<B>) -> Self {
        Self { inner }
    }

    /// Emit an event (legacy API)
    pub async fn emit(&self, event_type: &str, payload: serde_json::Value) -> crate::Result<()> {
        let topic = format!("core/event/{}", event_type);
        let message = Message::with_payload(&topic, &payload)?;
        self.inner.publish(&topic, message).await
    }

    /// Subscribe to events (legacy API)
    pub async fn subscribe(&self, _name: &str) -> crate::Result<mpsc::UnboundedReceiver<Message>> {
        let topic_pattern = "core/event/+";
        let (_sub_id, mut stream) = self.inner.subscribe(topic_pattern).await?;

        let (tx, rx) = mpsc::unbounded_channel();

        // Spawn forwarding task
        tokio::spawn(async move {
            while let Some(message) = stream.recv().await {
                if tx.send(message).is_err() {
                    break;
                }
            }
        });

        Ok(rx)
    }
}

/// Adapter for agents module events
pub struct AgentsEventAdapter<B: MessageBus> {
    inner: Arc<B>,
}

impl<B: MessageBus> AgentsEventAdapter<B> {
    pub fn new(inner: Arc<B>) -> Self {
        Self { inner }
    }

    /// Publish agent lifecycle event
    pub async fn publish_agent_lifecycle(
        &self,
        agent_id: &str,
        old_state: &str,
        new_state: &str,
    ) -> crate::Result<()> {
        let event = serde_json::json!({
            "agent_id": agent_id,
            "old_state": old_state,
            "new_state": new_state,
        });

        let topic = format!("agent/{}/lifecycle/state_change", agent_id);
        let message = Message::with_payload(&topic, &event)?;

        self.inner.publish(&topic, message).await
    }

    /// Publish task event
    pub async fn publish_task_event(
        &self,
        agent_id: &str,
        task_id: &str,
        event_type: &str,
    ) -> crate::Result<()> {
        let topic = format!("agent/{}/task/{}/{}", agent_id, task_id, event_type);
        let message = Message::new(&topic, vec![]);

        self.inner.publish(&topic, message).await
    }
}

/// Migration helper for kernel events
pub struct KernelEventAdapter<B: MessageBus> {
    inner: Arc<B>,
}

impl<B: MessageBus> KernelEventAdapter<B> {
    pub fn new(inner: Arc<B>) -> Self {
        Self { inner }
    }

    /// Publish kernel task event
    pub async fn publish_task_event(
        &self,
        task_id: u64,
        event_type: &str,
        data: HashMap<String, String>,
    ) -> crate::Result<()> {
        let topic = format!("kernel/task/{}/{}", task_id, event_type);
        let message = Message::with_payload(&topic, &data)?;

        self.inner.publish(&topic, message).await
    }

    /// Publish resource event
    pub async fn publish_resource_event(
        &self,
        resource_type: &str,
        event_data: HashMap<String, String>,
    ) -> crate::Result<()> {
        let topic = format!("kernel/resource/{}", resource_type);
        let message = Message::with_payload(&topic, &event_data)?;

        self.inner.publish(&topic, message).await
    }
}

/// Migration helper for chain events
pub struct ChainEventAdapter<B: MessageBus> {
    inner: Arc<B>,
}

impl<B: MessageBus> ChainEventAdapter<B> {
    pub fn new(inner: Arc<B>) -> Self {
        Self { inner }
    }

    /// Publish transaction event
    pub async fn publish_transaction_event(
        &self,
        tx_hash: &str,
        status: &str,
        block_number: Option<u64>,
    ) -> crate::Result<()> {
        let topic = format!("chain/tx/{}", status);
        let event = serde_json::json!({
            "tx_hash": tx_hash,
            "status": status,
            "block_number": block_number,
        });

        let message = Message::with_payload(&topic, &event)?;
        self.inner.publish(&topic, message).await
    }

    /// Publish contract event
    pub async fn publish_contract_event(
        &self,
        contract: &str,
        event_name: &str,
        data: serde_json::Value,
    ) -> crate::Result<()> {
        let topic = format!("chain/contract/{}/{}", contract, event_name);
        let message = Message::with_payload(&topic, &data)?;

        self.inner.publish(&topic, message).await
    }
}

/// Bridge between old and new event systems
pub struct EventBridge<B: MessageBus> {
    bus: Arc<B>,
    #[allow(dead_code)]
    subscriptions: HashMap<String, SubscriptionId>,
}

impl<B: MessageBus> EventBridge<B> {
    /// Create a new event bridge
    pub fn new(bus: Arc<B>) -> Self {
        Self {
            bus,
            subscriptions: HashMap::new(),
        }
    }

    /// Get reference to message bus
    pub fn bus(&self) -> &Arc<B> {
        &self.bus
    }
}

/// Migration guide helper
pub struct MigrationGuide;

impl MigrationGuide {
    /// Print migration steps for a module
    pub fn print_migration_steps(module_name: &str) {
        println!("\n=== Migration Guide for {} ===\n", module_name);

        match module_name {
            "agents" => {
                println!("1. Replace AgentEventBus with MessageBus:");
                println!("   OLD: let event_bus = AgentEventBus::new();");
                println!("   NEW: let bus = Arc::new(MemoryTransport::new());");
                println!();
                println!("2. Update event publishing:");
                println!("   OLD: event_bus.emit(Event::AgentLifecycle {{ ... }}).await;");
                println!("   NEW: bus.publish(\"agent/123/lifecycle\", message).await?;");
                println!();
                println!("3. Update subscriptions:");
                println!("   OLD: let mut rx = event_bus.subscribe(\"handler\").await;");
                println!("   NEW: let (sub_id, mut rx) = bus.subscribe(\"agent/+/+\").await?;");
            }
            "gateway" => {
                println!("1. Use GatewayEventAdapter:");
                println!("   let adapter = GatewayEventAdapter::new(bus);");
                println!();
                println!("2. Publish WebSocket events:");
                println!(
                    "   adapter.publish_ws_connected(\"conn-123\", \"192.168.1.1:12345\").await?;"
                );
                println!();
                println!("3. Subscribe to events:");
                println!("   let (sub_id, rx) = adapter.subscribe_websocket().await?;");
            }
            "kernel" => {
                println!("1. Replace KernelTaskEvent publishing:");
                println!("   OLD: emit_event(KernelTaskEvent::Spawned {{ ... }}).await;");
                println!("   NEW: bus.publish(\"kernel/task/spawned\", message).await?;");
                println!();
                println!("2. Update event types to use Message payloads");
            }
            _ => {
                println!("General migration steps:");
                println!("1. Identify all event publishing points");
                println!("2. Replace with MessageBus::publish() calls");
                println!("3. Identify all event subscriptions");
                println!("4. Replace with MessageBus::subscribe() calls");
                println!("5. Update event types to be serializable");
                println!("6. Add correlation IDs for request-reply patterns");
            }
        }

        println!();
    }

    /// Get API mapping for a module
    pub fn get_api_mapping(module_name: &str) -> HashMap<String, String> {
        let mut mapping = HashMap::new();

        match module_name {
            "agents" => {
                mapping.insert(
                    "AgentEventBus::emit".to_string(),
                    "MessageBus::publish".to_string(),
                );
                mapping.insert(
                    "AgentEventBus::subscribe".to_string(),
                    "MessageBus::subscribe".to_string(),
                );
                mapping.insert(
                    "Event::AgentLifecycle".to_string(),
                    "agent/{id}/lifecycle".to_string(),
                );
            }
            "gateway" => {
                mapping.insert(
                    "Gateway::on_ws_connect".to_string(),
                    "GatewayEventAdapter::publish_ws_connected".to_string(),
                );
                mapping.insert(
                    "Gateway::on_http_request".to_string(),
                    "GatewayEventAdapter::publish_http_request".to_string(),
                );
                mapping.insert(
                    "Gateway::on_auth".to_string(),
                    "GatewayEventAdapter::publish_auth_success/failed".to_string(),
                );
            }
            "kernel" => {
                mapping.insert(
                    "emit_event(KernelTaskEvent::...)".to_string(),
                    "MessageBus::publish".to_string(),
                );
            }
            _ => {}
        }

        mapping
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DefaultMessageBus, JsonCodec, MemoryTransport};

    #[tokio::test]
    async fn test_core_event_bus_adapter() {
        let bus = Arc::new(DefaultMessageBus::new(
            MemoryTransport::new(),
            Box::new(JsonCodec::new()),
            None,
        ));
        let adapter = CoreEventBusAdapter::new(bus.clone());

        // Subscribe through adapter
        let mut rx = adapter.subscribe("test").await.unwrap();

        // Emit event
        adapter
            .emit(
                "task_started",
                serde_json::json!({
                    "task_id": "task-123",
                }),
            )
            .await
            .unwrap();

        // Should receive through the bus
        let received = rx.recv().await;
        assert!(received.is_some());
    }

    #[test]
    fn test_migration_guide_print() {
        MigrationGuide::print_migration_steps("agents");
        MigrationGuide::print_migration_steps("gateway");
        MigrationGuide::print_migration_steps("kernel");
    }

    #[test]
    fn test_api_mapping() {
        let mapping = MigrationGuide::get_api_mapping("agents");
        assert!(mapping.contains_key("AgentEventBus::emit"));
        assert!(mapping.contains_key("AgentEventBus::subscribe"));

        let gateway_mapping = MigrationGuide::get_api_mapping("gateway");
        assert!(gateway_mapping.contains_key("Gateway::on_ws_connect"));
    }
}
