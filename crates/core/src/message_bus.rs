//! Core Module Message Bus Integration
//!
//! Provides integration between Core module events and the unified Message Bus.

use std::sync::Arc;

use beebotos_message_bus::{Message, MessageBus, Result as BusResult};

use crate::event::Event;
// Unused imports removed

/// Core module event adapter for Message Bus
pub struct CoreMessageBus<B: MessageBus> {
    bus: Arc<B>,
}

impl<B: MessageBus> CoreMessageBus<B> {
    /// Create a new Core Message Bus adapter
    pub fn new(bus: Arc<B>) -> Self {
        Self { bus }
    }

    /// Publish an event to the message bus
    pub async fn publish(&self, event: Event) -> BusResult<()> {
        let (topic, payload) = Self::event_to_message(event);
        let message = Message::new(&topic, payload);
        self.bus.publish(&topic, message).await
    }

    /// Subscribe to Core events
    pub async fn subscribe(
        &self,
    ) -> BusResult<(
        beebotos_message_bus::SubscriptionId,
        beebotos_message_bus::MessageStream,
    )> {
        self.bus.subscribe("core/#").await
    }

    /// Convert Core Event to Message Bus topic and payload
    fn event_to_message(event: Event) -> (String, Vec<u8>) {
        let topic = match &event {
            Event::AgentLifecycle { .. } => "core/agent/lifecycle",
            Event::AgentSpawned { .. } => "core/agent/spawned",
            Event::MemoryConsolidated { .. } => "core/memory/consolidated",
            Event::BlockchainTx { .. } => "core/blockchain/tx",
            Event::DaoProposalCreated { .. } => "core/dao/proposal",
            Event::DaoVoteCast { .. } => "core/dao/vote",
            Event::SkillExecuted { .. } => "core/skill/executed",
            Event::Metric { .. } => "core/metric",
            Event::TaskStarted { .. } => "core/task/started",
            Event::TaskCompleted { .. } => "core/task/completed",
            // Note: TaskFailed variant doesn't exist in Event enum
            // Task failure is indicated by TaskCompleted with success=false
        };

        let payload = serde_json::to_vec(&event).unwrap_or_default();
        (topic.to_string(), payload)
    }
}

impl<B: MessageBus> Clone for CoreMessageBus<B> {
    fn clone(&self) -> Self {
        Self {
            bus: Arc::clone(&self.bus),
        }
    }
}

use std::sync::OnceLock;

/// Global Message Bus handle for Core module
///
/// SECURITY FIX: Replaced `static mut` with `OnceLock` for thread-safe
/// initialization
static CORE_MESSAGE_BUS: OnceLock<Arc<dyn MessageBus>> = OnceLock::new();

/// Initialize Core Message Bus
///
/// This function is thread-safe and can only be called once.
pub fn init_message_bus<B: MessageBus + 'static>(bus: Arc<B>) -> Result<(), &'static str> {
    CORE_MESSAGE_BUS
        .set(bus)
        .map_err(|_| "Core message bus already initialized")
}

/// Get global Message Bus
pub fn message_bus() -> Option<Arc<dyn MessageBus>> {
    CORE_MESSAGE_BUS.get().cloned()
}

#[cfg(test)]
mod tests {
    use beebotos_message_bus::{DefaultMessageBus, JsonCodec, MemoryTransport};

    use super::*;
    use crate::types::{AgentId, AgentStatus, Timestamp};

    #[tokio::test]
    async fn test_core_message_bus() {
        let bus = Arc::new(DefaultMessageBus::new(
            MemoryTransport::new(),
            Box::new(JsonCodec::new()),
            None,
        ));
        let core_bus = CoreMessageBus::new(bus);

        let event = Event::AgentLifecycle {
            agent_id: AgentId::new(),
            from: AgentStatus::Idle,
            to: AgentStatus::Running,
            timestamp: Timestamp::now(),
        };

        assert!(core_bus.publish(event).await.is_ok());
    }
}
