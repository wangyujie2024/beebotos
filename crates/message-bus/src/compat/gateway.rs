//! Gateway Module Message Bus Adapter
//!
//! Provides integration between the Gateway module and the unified message bus.

use std::sync::Arc;

use serde_json::json;
use tracing::{info, warn};

use crate::{Message, MessageBus, MessageStream, Result, SubscriptionId};

/// Gateway event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayEventType {
    /// WebSocket connection established
    WsConnected,
    /// WebSocket connection closed
    WsDisconnected,
    /// WebSocket message received
    WsMessageReceived,
    /// WebSocket message sent
    WsMessageSent,
    /// HTTP request received
    HttpRequestReceived,
    /// HTTP response sent
    HttpResponseSent,
    /// Rate limit exceeded
    RateLimitExceeded,
    /// Authentication succeeded
    AuthSuccess,
    /// Authentication failed
    AuthFailed,
    /// Service discovery update
    ServiceDiscovery,
    /// Circuit breaker state change
    CircuitBreaker,
    /// Health check
    HealthCheck,
}

impl GatewayEventType {
    /// Get the topic name for this event type
    pub fn topic(&self) -> &'static str {
        match self {
            Self::WsConnected => "gateway/websocket/connected",
            Self::WsDisconnected => "gateway/websocket/disconnected",
            Self::WsMessageReceived => "gateway/websocket/message/received",
            Self::WsMessageSent => "gateway/websocket/message/sent",
            Self::HttpRequestReceived => "gateway/http/request/received",
            Self::HttpResponseSent => "gateway/http/response/sent",
            Self::RateLimitExceeded => "gateway/ratelimit/exceeded",
            Self::AuthSuccess => "gateway/auth/success",
            Self::AuthFailed => "gateway/auth/failed",
            Self::ServiceDiscovery => "gateway/discovery/update",
            Self::CircuitBreaker => "gateway/circuitbreaker/state",
            Self::HealthCheck => "gateway/health/check",
        }
    }
}

/// Gateway event adapter for message bus integration
pub struct GatewayEventAdapter<B: MessageBus> {
    bus: Arc<B>,
    topic_prefix: String,
}

impl<B: MessageBus> GatewayEventAdapter<B> {
    /// Create a new Gateway event adapter
    pub fn new(bus: Arc<B>) -> Self {
        Self {
            bus,
            topic_prefix: "gateway".to_string(),
        }
    }

    /// Create with custom topic prefix
    pub fn with_prefix(bus: Arc<B>, prefix: String) -> Self {
        Self {
            bus,
            topic_prefix: prefix,
        }
    }

    fn create_message(&self, event_type: GatewayEventType, event: serde_json::Value) -> Message {
        let payload = serde_json::to_vec(&event).unwrap_or_default();
        Message::new(event_type.topic(), payload)
    }

    // ==================== WebSocket Events ====================

    /// Publish WebSocket connection established event
    pub async fn publish_ws_connected(&self, connection_id: &str, remote_addr: &str) -> Result<()> {
        let event = json!({
            "event_type": "ws_connected",
            "connection_id": connection_id,
            "remote_addr": remote_addr,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        let msg = self.create_message(GatewayEventType::WsConnected, event);
        self.bus
            .publish(GatewayEventType::WsConnected.topic(), msg)
            .await
    }

    /// Publish WebSocket connection closed event
    pub async fn publish_ws_disconnected(
        &self,
        connection_id: &str,
        remote_addr: &str,
        reason: &str,
        duration_secs: u64,
    ) -> Result<()> {
        let event = json!({
            "event_type": "ws_disconnected",
            "connection_id": connection_id,
            "remote_addr": remote_addr,
            "reason": reason,
            "duration_secs": duration_secs,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        let msg = self.create_message(GatewayEventType::WsDisconnected, event);
        self.bus
            .publish(GatewayEventType::WsDisconnected.topic(), msg)
            .await
    }

    /// Publish WebSocket message received event
    pub async fn publish_ws_message_received(
        &self,
        connection_id: &str,
        message_type: &str,
        payload_size: usize,
    ) -> Result<()> {
        let event = json!({
            "event_type": "ws_message_received",
            "connection_id": connection_id,
            "message_type": message_type,
            "payload_size": payload_size,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        let msg = self.create_message(GatewayEventType::WsMessageReceived, event);
        self.bus
            .publish(GatewayEventType::WsMessageReceived.topic(), msg)
            .await
    }

    /// Publish WebSocket message sent event
    pub async fn publish_ws_message_sent(
        &self,
        connection_id: &str,
        message_type: &str,
        payload_size: usize,
    ) -> Result<()> {
        let event = json!({
            "event_type": "ws_message_sent",
            "connection_id": connection_id,
            "message_type": message_type,
            "payload_size": payload_size,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        let msg = self.create_message(GatewayEventType::WsMessageSent, event);
        self.bus
            .publish(GatewayEventType::WsMessageSent.topic(), msg)
            .await
    }

    // ==================== HTTP Events ====================

    /// Publish HTTP request received event
    pub async fn publish_http_request(
        &self,
        request_id: &str,
        method: &str,
        path: &str,
        remote_addr: &str,
        headers: serde_json::Value,
    ) -> Result<()> {
        let event = json!({
            "event_type": "http_request",
            "request_id": request_id,
            "method": method,
            "path": path,
            "remote_addr": remote_addr,
            "headers": headers,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        let msg = self.create_message(GatewayEventType::HttpRequestReceived, event);
        self.bus
            .publish(GatewayEventType::HttpRequestReceived.topic(), msg)
            .await
    }

    /// Publish HTTP response sent event
    pub async fn publish_http_response(
        &self,
        request_id: &str,
        status_code: u16,
        duration_ms: u64,
        response_size: usize,
    ) -> Result<()> {
        let event = json!({
            "event_type": "http_response",
            "request_id": request_id,
            "status_code": status_code,
            "duration_ms": duration_ms,
            "response_size": response_size,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        let msg = self.create_message(GatewayEventType::HttpResponseSent, event);
        self.bus
            .publish(GatewayEventType::HttpResponseSent.topic(), msg)
            .await
    }

    // ==================== Security Events ====================

    /// Publish rate limit exceeded event
    pub async fn publish_rate_limit_exceeded(
        &self,
        client_id: &str,
        endpoint: &str,
        limit: u32,
        window_secs: u64,
    ) -> Result<()> {
        let event = json!({
            "event_type": "rate_limit_exceeded",
            "client_id": client_id,
            "endpoint": endpoint,
            "limit": limit,
            "window_secs": window_secs,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        let msg = self.create_message(GatewayEventType::RateLimitExceeded, event);
        self.bus
            .publish(GatewayEventType::RateLimitExceeded.topic(), msg)
            .await
    }

    /// Publish authentication success event
    pub async fn publish_auth_success(
        &self,
        user_id: &str,
        client_id: &str,
        auth_method: &str,
    ) -> Result<()> {
        let event = json!({
            "event_type": "auth_success",
            "user_id": user_id,
            "client_id": client_id,
            "auth_method": auth_method,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        let msg = self.create_message(GatewayEventType::AuthSuccess, event);
        self.bus
            .publish(GatewayEventType::AuthSuccess.topic(), msg)
            .await
    }

    /// Publish authentication failed event
    pub async fn publish_auth_failed(
        &self,
        client_id: &str,
        auth_method: &str,
        reason: &str,
    ) -> Result<()> {
        let event = json!({
            "event_type": "auth_failed",
            "client_id": client_id,
            "auth_method": auth_method,
            "reason": reason,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        let msg = self.create_message(GatewayEventType::AuthFailed, event);
        self.bus
            .publish(GatewayEventType::AuthFailed.topic(), msg)
            .await
    }

    // ==================== Service Discovery Events ====================

    /// Publish service discovery update event
    pub async fn publish_service_discovery(
        &self,
        service_name: &str,
        instances: Vec<serde_json::Value>,
        operation: &str,
    ) -> Result<()> {
        let event = json!({
            "event_type": "service_discovery",
            "service_name": service_name,
            "instances": instances,
            "operation": operation,
            "instance_count": instances.len(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        let msg = self.create_message(GatewayEventType::ServiceDiscovery, event);
        self.bus
            .publish(GatewayEventType::ServiceDiscovery.topic(), msg)
            .await
    }

    // ==================== Circuit Breaker Events ====================

    /// Publish circuit breaker state change event
    pub async fn publish_circuit_breaker(
        &self,
        service_name: &str,
        previous_state: &str,
        current_state: &str,
        failure_count: u32,
    ) -> Result<()> {
        let event = json!({
            "event_type": "circuit_breaker",
            "service_name": service_name,
            "previous_state": previous_state,
            "current_state": current_state,
            "failure_count": failure_count,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        let msg = self.create_message(GatewayEventType::CircuitBreaker, event);
        self.bus
            .publish(GatewayEventType::CircuitBreaker.topic(), msg)
            .await
    }

    // ==================== Health Events ====================

    /// Publish health check event
    pub async fn publish_health_check(
        &self,
        component: &str,
        status: &str,
        details: serde_json::Value,
    ) -> Result<()> {
        let event = json!({
            "event_type": "health_check",
            "component": component,
            "status": status,
            "details": details,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });

        let msg = self.create_message(GatewayEventType::HealthCheck, event);
        self.bus
            .publish(GatewayEventType::HealthCheck.topic(), msg)
            .await
    }

    // ==================== Subscription Helpers ====================

    /// Subscribe to all Gateway events
    pub async fn subscribe_all(&self) -> Result<(SubscriptionId, MessageStream)> {
        self.bus.subscribe("gateway/#").await
    }

    /// Subscribe to WebSocket events
    pub async fn subscribe_websocket(&self) -> Result<(SubscriptionId, MessageStream)> {
        self.bus.subscribe("gateway/websocket/#").await
    }

    /// Subscribe to HTTP events
    pub async fn subscribe_http(&self) -> Result<(SubscriptionId, MessageStream)> {
        self.bus.subscribe("gateway/http/#").await
    }

    /// Subscribe to security events
    pub async fn subscribe_security(&self) -> Result<(SubscriptionId, MessageStream)> {
        self.bus.subscribe("gateway/+/+").await
    }

    /// Get reference to underlying message bus
    pub fn message_bus(&self) -> &Arc<B> {
        &self.bus
    }
}

impl<B: MessageBus> Clone for GatewayEventAdapter<B> {
    fn clone(&self) -> Self {
        Self {
            bus: Arc::clone(&self.bus),
            topic_prefix: self.topic_prefix.clone(),
        }
    }
}

/// Gateway message bus bridge for bidirectional communication
pub struct GatewayMessageBridge<B: MessageBus> {
    adapter: GatewayEventAdapter<B>,
    command_sub: Option<SubscriptionId>,
}

impl<B: MessageBus + 'static> GatewayMessageBridge<B> {
    /// Create a new message bridge
    pub fn new(bus: Arc<B>) -> Self {
        Self {
            adapter: GatewayEventAdapter::new(bus),
            command_sub: None,
        }
    }

    /// Start the bridge and listen for commands
    pub async fn start(&mut self) -> Result<()> {
        let (sub_id, mut stream) = self.adapter.bus.subscribe("gateway/commands/#").await?;
        self.command_sub = Some(sub_id);

        let adapter = self.adapter.clone();
        tokio::spawn(async move {
            while let Some(msg) = stream.recv().await {
                if let Err(e) = Self::handle_command(&adapter, &msg).await {
                    warn!("Failed to handle gateway command: {}", e);
                }
            }
        });

        info!("Gateway message bridge started");
        Ok(())
    }

    /// Handle incoming commands
    async fn handle_command(_adapter: &GatewayEventAdapter<B>, msg: &Message) -> Result<()> {
        let payload: serde_json::Value = msg.decode_payload()?;
        let cmd = payload.get("command").and_then(|v| v.as_str());

        match cmd {
            Some("broadcast_ws") => {
                info!("Received broadcast command via message bus");
            }
            Some("disconnect_client") => {
                if let Some(client_id) = payload.get("client_id").and_then(|v| v.as_str()) {
                    info!("Received disconnect command for client: {}", client_id);
                }
            }
            Some("update_rate_limit") => {
                info!("Received rate limit update command");
            }
            _ => {
                warn!("Unknown gateway command: {:?}", cmd);
            }
        }

        Ok(())
    }

    /// Stop the bridge
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(sub_id) = self.command_sub.take() {
            self.adapter.bus.unsubscribe(sub_id).await?;
        }
        info!("Gateway message bridge stopped");
        Ok(())
    }

    /// Get the event adapter
    pub fn adapter(&self) -> &GatewayEventAdapter<B> {
        &self.adapter
    }
}

/// Metrics collector for Gateway events
pub struct GatewayMetricsCollector<B: MessageBus> {
    #[allow(dead_code)]
    adapter: GatewayEventAdapter<B>,
    _sub_handle: Option<tokio::task::JoinHandle<()>>,
}

impl<B: MessageBus + 'static> GatewayMetricsCollector<B> {
    /// Create and start metrics collector
    pub async fn new(bus: Arc<B>) -> Result<Self> {
        let adapter = GatewayEventAdapter::new(bus);
        let (_sub_id, mut stream) = adapter.subscribe_all().await?;

        let handle = tokio::spawn(async move {
            let mut ws_connections = 0u64;
            let mut http_requests = 0u64;
            let mut auth_failures = 0u64;

            while let Some(msg) = stream.recv().await {
                match msg.metadata.topic.as_str() {
                    "gateway/websocket/connected" => ws_connections += 1,
                    "gateway/websocket/disconnected" => {
                        ws_connections = ws_connections.saturating_sub(1)
                    }
                    "gateway/http/request/received" => http_requests += 1,
                    "gateway/auth/failed" => auth_failures += 1,
                    _ => {}
                }

                tracing::debug!(
                    "Gateway metrics - WS: {}, HTTP: {}, Auth Failures: {}",
                    ws_connections,
                    http_requests,
                    auth_failures
                );
            }
        });

        Ok(Self {
            adapter,
            _sub_handle: Some(handle),
        })
    }

    /// Stop metrics collection
    pub async fn stop(self) {
        if let Some(handle) = self._sub_handle {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DefaultMessageBus, JsonCodec, MemoryTransport};

    #[tokio::test]
    async fn test_gateway_event_adapter() {
        let bus = Arc::new(DefaultMessageBus::new(
            MemoryTransport::new(),
            Box::new(JsonCodec::new()),
            None,
        ));
        let adapter = GatewayEventAdapter::new(bus.clone());

        // Subscribe to WebSocket events
        let (_sub_id, mut stream) = adapter.subscribe_websocket().await.unwrap();

        // Publish events
        adapter
            .publish_ws_connected("conn-123", "192.168.1.1:12345")
            .await
            .unwrap();
        adapter
            .publish_ws_disconnected("conn-123", "192.168.1.1:12345", "client_close", 60)
            .await
            .unwrap();

        // Receive events
        let msg1 = stream.recv().await.unwrap();
        assert!(msg1.metadata.topic.contains("websocket/connected"));

        let msg2 = stream.recv().await.unwrap();
        assert!(msg2.metadata.topic.contains("websocket/disconnected"));

        bus.unsubscribe(_sub_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_http_events() {
        let bus = Arc::new(DefaultMessageBus::new(
            MemoryTransport::new(),
            Box::new(JsonCodec::new()),
            None,
        ));
        let adapter = GatewayEventAdapter::new(bus.clone());

        let (_sub_id, mut stream) = adapter.subscribe_http().await.unwrap();

        adapter
            .publish_http_request(
                "req-456",
                "POST",
                "/api/agents",
                "10.0.0.1:56789",
                json!({"content-type": "application/json"}),
            )
            .await
            .unwrap();

        let msg = stream.recv().await.unwrap();
        assert!(msg.metadata.topic.contains("http/request"));

        bus.unsubscribe(_sub_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_security_events() {
        let bus = Arc::new(DefaultMessageBus::new(
            MemoryTransport::new(),
            Box::new(JsonCodec::new()),
            None,
        ));
        let adapter = GatewayEventAdapter::new(bus);

        adapter
            .publish_auth_success("user-789", "client-abc", "jwt")
            .await
            .unwrap();
        adapter
            .publish_auth_failed("client-def", "api_key", "invalid_key")
            .await
            .unwrap();
        adapter
            .publish_rate_limit_exceeded("client-ghi", "/api/expensive", 100, 60)
            .await
            .unwrap();
    }
}
