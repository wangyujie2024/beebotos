//! Agents Message Bus Integration
//!
//! Provides integration with the unified Message Bus for inter-agent
//! communication.
//!
//! 🔒 P0 FIX: Implementation now delegates to beebotos_message_bus crate.

use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use beebotos_message_bus::{DefaultMessageBus, JsonCodec, MemoryTransport};

/// Type alias for the message bus
type MessageBusType = DefaultMessageBus<MemoryTransport>;

/// Agents Message Bus handle
#[allow(dead_code)]
pub struct AgentsMessageBus {
    bus: Arc<MessageBusType>,
}

impl AgentsMessageBus {
    /// Create new Agents Message Bus
    pub fn new<B: MessageBus + 'static>(_bus: Arc<B>) -> Self {
        Self::create_internal()
    }

    /// Create new Agents Message Bus with default transport
    pub fn create() -> Self {
        Self::create_internal()
    }

    fn create_internal() -> Self {
        Self {
            bus: Arc::new(DefaultMessageBus::new(
                MemoryTransport::new(),
                Box::new(JsonCodec),
                None,
            )),
        }
    }

    /// Get the underlying bus
    #[allow(dead_code)]
    pub fn bus(&self) -> Arc<MessageBusType> {
        Arc::clone(&self.bus)
    }

    /// Publish a message to a topic
    #[allow(dead_code)]
    pub async fn publish(&self, topic: &str, message: &[u8]) -> crate::error::Result<()> {
        self.bus
            .publish(topic, message)
            .await
            .map_err(|e| crate::error::AgentError::CommunicationFailed(e.to_string()))
    }

    /// Subscribe to a topic
    #[allow(dead_code)]
    pub async fn subscribe(
        &self,
        topic: &str,
    ) -> crate::error::Result<Box<dyn MessageBusSubscriber>> {
        self.bus
            .subscribe(topic)
            .await
            .map_err(|e| crate::error::AgentError::CommunicationFailed(e.to_string()))
    }
}

impl Clone for AgentsMessageBus {
    fn clone(&self) -> Self {
        Self {
            bus: Arc::clone(&self.bus),
        }
    }
}

/// MessageBus trait for abstraction
///
/// ARCHITECTURE FIX: Changed to use Pin<Box<dyn Future>> for dyn compatibility
pub trait MessageBus: Send + Sync {
    /// Publish a message to a topic
    fn publish(
        &self,
        topic: &str,
        message: &[u8],
    ) -> Pin<
        Box<dyn std::future::Future<Output = Result<(), Box<dyn std::error::Error>>> + Send + '_>,
    >;
    /// Subscribe to a topic
    fn subscribe(
        &self,
        topic: &str,
    ) -> Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<Box<dyn MessageBusSubscriber>, Box<dyn std::error::Error>>,
                > + Send
                + '_,
        >,
    >;
}

/// MessageBus subscriber trait
///
/// 🔒 P0 FIX: Uses async_trait for cleaner async trait definitions.
#[async_trait]
pub trait MessageBusSubscriber: Send + Sync {
    /// Receive next message
    async fn recv(&mut self) -> Option<Vec<u8>>;
}

/// 🔒 P0 FIX: Wrapper subscriber for MessageStream
struct MessageStreamSubscriber {
    receiver: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
}

#[async_trait]
impl MessageBusSubscriber for MessageStreamSubscriber {
    async fn recv(&mut self) -> Option<Vec<u8>> {
        self.receiver.recv().await
    }
}

impl MessageBus for DefaultMessageBus<MemoryTransport> {
    fn publish(
        &self,
        topic: &str,
        message: &[u8],
    ) -> Pin<
        Box<dyn std::future::Future<Output = Result<(), Box<dyn std::error::Error>>> + Send + '_>,
    > {
        let topic = topic.to_string();
        let message = message.to_vec();
        let bus = self;

        Box::pin(async move {
            use beebotos_message_bus::{Message, MessageBus as ExternalMessageBus};

            let msg = Message::new(&topic, message);
            ExternalMessageBus::publish(bus, &topic, msg)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
        })
    }

    fn subscribe(
        &self,
        topic: &str,
    ) -> Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<Box<dyn MessageBusSubscriber>, Box<dyn std::error::Error>>,
                > + Send
                + '_,
        >,
    > {
        let topic = topic.to_string();
        let bus = self;

        Box::pin(async move {
            use beebotos_message_bus::MessageBus as ExternalMessageBus;

            let (_sub_id, mut stream) = ExternalMessageBus::subscribe(bus, &topic)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

            // Create a channel to bridge between MessageStream and our trait
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

            // Spawn a task to forward messages
            tokio::spawn(async move {
                while let Some(msg) = stream.recv().await {
                    let payload = msg.payload.to_vec();
                    if tx.send(payload).is_err() {
                        break; // Receiver dropped
                    }
                }
            });

            let subscriber = MessageStreamSubscriber { receiver: rx };
            Ok(Box::new(subscriber) as Box<dyn MessageBusSubscriber>)
        })
    }
}

/// Global Message Bus handle using OnceLock for thread-safe initialization
///
/// SECURITY FIX: Replaced `static mut` with `OnceLock` to eliminate data race
/// risks and unsafe code blocks.
///
/// ARCHITECTURE FIX: Use concrete type instead of dyn trait to avoid dyn
/// compatibility issues
static GLOBAL_MESSAGE_BUS: OnceLock<Arc<MessageBusType>> = OnceLock::new();

/// Initialize Agents Message Bus
///
/// This function is thread-safe and can only be called once.
/// Subsequent calls will return an error.
pub fn init_message_bus<B: MessageBus + 'static>(_bus: Arc<B>) -> Result<(), &'static str> {
    let default_bus = Arc::new(DefaultMessageBus::new(
        MemoryTransport::new(),
        Box::new(JsonCodec),
        None,
    ));
    GLOBAL_MESSAGE_BUS
        .set(default_bus)
        .map_err(|_| "Message bus already initialized")
}

/// Get global Message Bus
///
/// Returns None if the message bus hasn't been initialized yet.
#[allow(dead_code)]
pub fn message_bus() -> Option<Arc<MessageBusType>> {
    GLOBAL_MESSAGE_BUS.get().cloned()
}

/// Get or initialize global Message Bus with a default
///
/// SECURITY FIX: Provides safe access to the global message bus without unsafe
/// blocks.
#[allow(dead_code)]
pub fn get_or_init_message_bus() -> Arc<MessageBusType> {
    GLOBAL_MESSAGE_BUS
        .get_or_init(|| {
            let transport = MemoryTransport::new();
            let bus: Arc<DefaultMessageBus<MemoryTransport>> =
                Arc::new(DefaultMessageBus::new(transport, Box::new(JsonCodec), None));
            bus
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_agents_message_bus() {
        // Create a mock message bus using the internal trait
        // Since AgentsMessageBus::new creates its own internal bus, we just test it
        // works
        let agents_bus = AgentsMessageBus::create();

        // Test that we can access the bus
        let _ = agents_bus.bus();
    }
}
