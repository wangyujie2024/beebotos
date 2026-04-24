//! Gateway Application Message Bus Integration
//!
//! Provides integration between Gateway application and the unified Message
//! Bus.

use std::sync::Arc;

use beebotos_message_bus::gateway::GatewayEventAdapter;
use beebotos_message_bus::{DefaultMessageBus, JsonCodec, MemoryTransport};

/// Message Bus configuration
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MessageBusConfig {
    /// Transport type: "memory", "grpc"
    pub transport: String,
    /// gRPC bind address (for grpc transport)
    pub grpc_bind_addr: String,
}

impl Default for MessageBusConfig {
    fn default() -> Self {
        Self {
            transport: "memory".to_string(),
            grpc_bind_addr: "0.0.0.0:50051".to_string(),
        }
    }
}

/// Type alias for the gateway message bus
type MessageBusType = DefaultMessageBus<MemoryTransport>;

/// Gateway Message Bus handle
#[allow(dead_code)]
pub struct GatewayMessageBus {
    /// Event adapter
    pub adapter: GatewayEventAdapter<MessageBusType>,
    /// The underlying message bus
    bus: Arc<MessageBusType>,
}

impl GatewayMessageBus {
    /// Create a new Gateway Message Bus
    pub fn new() -> Self {
        let transport = MemoryTransport::new();
        let bus: Arc<MessageBusType> =
            Arc::new(DefaultMessageBus::new(transport, Box::new(JsonCodec), None));
        let adapter = GatewayEventAdapter::new(Arc::clone(&bus));

        Self { adapter, bus }
    }

    /// Create with custom bus
    #[allow(dead_code)]
    pub fn with_bus(bus: Arc<MessageBusType>) -> Self {
        let adapter = GatewayEventAdapter::new(Arc::clone(&bus));
        Self { adapter, bus }
    }

    /// Get the underlying bus
    #[allow(dead_code)]
    pub fn bus(&self) -> Arc<MessageBusType> {
        Arc::clone(&self.bus)
    }
}

impl Default for GatewayMessageBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Global Message Bus instance
use once_cell::sync::OnceCell;
static GLOBAL_MESSAGE_BUS: OnceCell<Arc<GatewayMessageBus>> = OnceCell::new();

/// Initialize global Message Bus
pub fn init_global_message_bus(bus: GatewayMessageBus) {
    let _ = GLOBAL_MESSAGE_BUS.set(Arc::new(bus));
}

/// Get global Message Bus
#[allow(dead_code)]
pub fn global_message_bus() -> Option<Arc<GatewayMessageBus>> {
    GLOBAL_MESSAGE_BUS.get().cloned()
}

/// Initialize Message Bus from config
#[allow(dead_code)]
pub async fn init_from_config(config: &MessageBusConfig) -> anyhow::Result<GatewayMessageBus> {
    match config.transport.as_str() {
        "memory" => Ok(GatewayMessageBus::new()),
        "grpc" => {
            // gRPC transport is not fully implemented yet
            // For now, fall back to memory transport
            tracing::warn!("gRPC transport not fully implemented, using memory transport");
            Ok(GatewayMessageBus::new())
        }
        _ => {
            anyhow::bail!("Unsupported transport: {}", config.transport)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_gateway_message_bus() {
        let bus = GatewayMessageBus::new();

        // Publish a WebSocket event
        bus.adapter
            .publish_ws_connected("conn-123", "192.168.1.1:12345")
            .await
            .unwrap();
    }
}
