//! Kernel Module Message Bus Integration
//!
//! Provides integration between Kernel module and the unified Message Bus.

use std::sync::Arc;

use beebotos_message_bus::{Message, MessageBus, Result as BusResult};

use crate::capabilities::CapabilityLevel;
use crate::TaskId;

/// Kernel task events
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum KernelTaskEvent {
    /// Task spawned
    Spawned {
        /// Task ID
        task_id: TaskId,
        /// Agent ID that owns the task
        agent_id: String,
        /// Task priority
        priority: u8,
        /// Event timestamp
        timestamp: u64,
    },
    /// Task started execution
    Started {
        /// Task ID
        task_id: TaskId,
        /// Worker ID that started the task
        worker_id: String,
        /// Event timestamp
        timestamp: u64,
    },
    /// Task completed
    Completed {
        /// Task ID
        task_id: TaskId,
        /// Execution duration in milliseconds
        duration_ms: u64,
        /// Event timestamp
        timestamp: u64,
    },
    /// Task failed
    Failed {
        /// Task ID
        task_id: TaskId,
        /// Error message
        error: String,
        /// Event timestamp
        timestamp: u64,
    },
    /// Task cancelled
    Cancelled {
        /// Task ID
        task_id: TaskId,
        /// Cancellation reason
        reason: String,
        /// Event timestamp
        timestamp: u64,
    },
}

/// Kernel capability events
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum KernelCapabilityEvent {
    /// Capability granted
    Granted {
        /// Agent ID
        agent_id: String,
        /// Capability level granted
        level: CapabilityLevel,
        /// Event timestamp
        timestamp: u64,
    },
    /// Capability revoked
    Revoked {
        /// Agent ID
        agent_id: String,
        /// Capability level revoked
        level: CapabilityLevel,
        /// Event timestamp
        timestamp: u64,
    },
    /// Capability elevation requested
    ElevationRequested {
        /// Agent ID
        agent_id: String,
        /// Current capability level
        from: CapabilityLevel,
        /// Requested capability level
        to: CapabilityLevel,
        /// Event timestamp
        timestamp: u64,
    },
}

/// Kernel Message Bus adapter
pub struct KernelMessageBus<B: MessageBus> {
    bus: Arc<B>,
}

impl<B: MessageBus> KernelMessageBus<B> {
    /// Create a new Kernel Message Bus adapter
    pub fn new(bus: Arc<B>) -> Self {
        Self { bus }
    }

    /// Publish task event
    pub async fn publish_task_event(&self, event: KernelTaskEvent) -> BusResult<()> {
        let topic = format!(
            "kernel/task/{}",
            match &event {
                KernelTaskEvent::Spawned { task_id, .. } => task_id.to_string(),
                KernelTaskEvent::Started { task_id, .. } => task_id.to_string(),
                KernelTaskEvent::Completed { task_id, .. } => task_id.to_string(),
                KernelTaskEvent::Failed { task_id, .. } => task_id.to_string(),
                KernelTaskEvent::Cancelled { task_id, .. } => task_id.to_string(),
            }
        );

        let payload = serde_json::to_vec(&event).unwrap_or_default();
        let message = Message::new(&topic, payload);
        self.bus.publish(&topic, message).await
    }

    /// Publish capability event
    pub async fn publish_capability_event(&self, event: KernelCapabilityEvent) -> BusResult<()> {
        let topic = "kernel/capability/events".to_string();
        let payload = serde_json::to_vec(&event).unwrap_or_default();
        let message = Message::new(&topic, payload);
        self.bus.publish(&topic, message).await
    }

    /// Subscribe to task events
    pub async fn subscribe_task_events(
        &self,
    ) -> BusResult<(
        beebotos_message_bus::SubscriptionId,
        beebotos_message_bus::MessageStream,
    )> {
        self.bus.subscribe("kernel/task/+").await
    }

    /// Subscribe to capability events
    pub async fn subscribe_capability_events(
        &self,
    ) -> BusResult<(
        beebotos_message_bus::SubscriptionId,
        beebotos_message_bus::MessageStream,
    )> {
        self.bus.subscribe("kernel/capability/+").await
    }
}

impl<B: MessageBus> Clone for KernelMessageBus<B> {
    fn clone(&self) -> Self {
        Self {
            bus: Arc::clone(&self.bus),
        }
    }
}

use std::sync::OnceLock;

/// Global Message Bus handle
///
/// SECURITY FIX: Replaced `static mut` with `OnceLock` for thread-safe
/// initialization
static KERNEL_MESSAGE_BUS: OnceLock<Arc<dyn MessageBus>> = OnceLock::new();

/// Initialize Kernel Message Bus
///
/// This function is thread-safe and can only be called once.
pub fn init_message_bus<B: MessageBus + 'static>(bus: Arc<B>) -> Result<(), &'static str> {
    KERNEL_MESSAGE_BUS
        .set(bus)
        .map_err(|_| "Kernel message bus already initialized")
}

/// Get global Message Bus
pub fn message_bus() -> Option<Arc<dyn MessageBus>> {
    KERNEL_MESSAGE_BUS.get().cloned()
}

#[cfg(test)]
mod tests {
    use beebotos_message_bus::{DefaultMessageBus, JsonCodec, MemoryTransport};

    use super::*;

    #[tokio::test]
    async fn test_kernel_message_bus() {
        let bus = Arc::new(DefaultMessageBus::new(
            MemoryTransport::new(),
            Box::new(JsonCodec::new()),
            None,
        ));
        let kernel_bus = KernelMessageBus::new(bus);

        let event = KernelTaskEvent::Spawned {
            task_id: TaskId::new(1),
            agent_id: "agent-1".to_string(),
            priority: 5,
            timestamp: 0,
        };

        assert!(kernel_bus.publish_task_event(event).await.is_ok());
    }
}
