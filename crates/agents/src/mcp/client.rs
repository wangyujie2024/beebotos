//! MCP Client
//!
//! Client implementation for MCP protocol.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{mpsc, Mutex, RwLock};

use super::types::*;
use super::MCPError;

/// MCP Client configuration
#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub server_url: String,
    pub timeout_ms: u64,
    pub retry_count: u32,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            server_url: "http://localhost:3000".to_string(),
            timeout_ms: 30000,
            retry_count: 3,
        }
    }
}

/// MCP Client
///
/// ARCHITECTURE FIX: Now implements proper request-response matching using
/// request IDs. Each request gets a unique ID, and responses are matched to
/// their corresponding requests.
pub struct MCPClient {
    config: ClientConfig,
    initialized: AtomicBool,
    request_counter: Mutex<u64>,
    server_capabilities: RwLock<Option<ServerCapabilities>>,
    request_tx: mpsc::UnboundedSender<JsonRpcRequest>,
    /// ARCHITECTURE FIX: Map of pending requests (request_id -> response
    /// channel)
    pending_requests: Arc<Mutex<HashMap<RequestId, tokio::sync::oneshot::Sender<JsonRpcResponse>>>>,
}

impl MCPClient {
    /// Create new MCP client
    ///
    /// ARCHITECTURE FIX: Returns a client along with channels for
    /// request/response handling. Use `start_response_handler` to spawn a
    /// task that handles incoming responses.
    pub fn new(
        config: ClientConfig,
    ) -> (
        Self,
        mpsc::UnboundedReceiver<JsonRpcRequest>,
        mpsc::UnboundedSender<JsonRpcResponse>,
    ) {
        let (request_tx, request_rx) = mpsc::unbounded_channel();
        let (response_tx, response_rx) = mpsc::unbounded_channel();

        let client = Self {
            config,
            initialized: AtomicBool::new(false),
            request_counter: Mutex::new(0),
            server_capabilities: RwLock::new(None),
            request_tx,
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
        };

        // ARCHITECTURE FIX: Start response handler task
        client.start_response_handler(response_rx);

        (client, request_rx, response_tx)
    }

    /// ARCHITECTURE FIX: Start background task to handle responses and match to
    /// requests
    fn start_response_handler(&self, mut response_rx: mpsc::UnboundedReceiver<JsonRpcResponse>) {
        let pending = self.pending_requests.clone();

        tokio::spawn(async move {
            while let Some(response) = response_rx.recv().await {
                let request_id = response.id.clone();

                // Find and remove the pending request
                let sender = {
                    let mut pending_guard = pending.lock().await;
                    pending_guard.remove(&request_id)
                };

                if let Some(sender) = sender {
                    // Send response to the waiting request
                    if sender.send(response).is_err() {
                        tracing::warn!("Request {} receiver dropped", request_id);
                    }
                } else {
                    tracing::warn!("Received response for unknown request: {}", request_id);
                }
            }

            tracing::info!("MCP response handler stopped");
        });
    }

    /// Initialize connection
    pub async fn initialize(&self) -> Result<InitializeResult, MCPError> {
        if self.initialized.load(Ordering::SeqCst) {
            return Err(MCPError::InitializationFailed(
                "Already initialized".to_string(),
            ));
        }

        let params = InitializeParams {
            protocol_version: MCPVersion::current().protocol_version,
            capabilities: ClientCapabilities::default(),
            client_info: super::types::Implementation {
                name: "BeeBotOS MCP".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        };

        let response = self
            .request(
                "initialize",
                Some(
                    serde_json::to_value(params)
                        .map_err(|e| MCPError::SerializationFailed(e.to_string()))?,
                ),
            )
            .await?;

        let result: InitializeResult = serde_json::from_value(
            response
                .result
                .ok_or_else(|| MCPError::InitializationFailed("No result".to_string()))?,
        )
        .map_err(|e| MCPError::InitializationFailed(e.to_string()))?;

        *self.server_capabilities.write().await = Some(result.capabilities.clone());
        self.initialized.store(true, Ordering::SeqCst);

        // Send initialized notification
        let _ = self.notify("notifications/initialized", None).await;

        Ok(result)
    }

    /// Check if initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::SeqCst)
    }

    /// List available tools
    pub async fn list_tools(&self, cursor: Option<&str>) -> Result<ListToolsResult, MCPError> {
        self.ensure_initialized()?;

        let params = cursor.map(|c| serde_json::json!({ "cursor": c }));
        let response = self.request("tools/list", params).await?;

        serde_json::from_value(
            response
                .result
                .ok_or_else(|| MCPError::RequestFailed("No result".to_string()))?,
        )
        .map_err(|e| MCPError::RequestFailed(e.to_string()))
    }

    /// Call a tool
    pub async fn call_tool(
        &self,
        name: impl Into<String>,
        arguments: Option<serde_json::Map<String, Value>>,
    ) -> Result<CallToolResult, MCPError> {
        self.ensure_initialized()?;

        let params = CallToolParams {
            name: name.into(),
            arguments: arguments.map(|m| m.into_iter().collect()),
        };

        let response = self
            .request(
                "tools/call",
                Some(
                    serde_json::to_value(params)
                        .map_err(|e| MCPError::SerializationFailed(e.to_string()))?,
                ),
            )
            .await?;

        serde_json::from_value(
            response
                .result
                .ok_or_else(|| MCPError::RequestFailed("No result".to_string()))?,
        )
        .map_err(|e| MCPError::RequestFailed(e.to_string()))
    }

    /// List resources
    pub async fn list_resources(
        &self,
        cursor: Option<&str>,
    ) -> Result<ListResourcesResult, MCPError> {
        self.ensure_initialized()?;

        let params = cursor.map(|c| serde_json::json!({ "cursor": c }));
        let response = self.request("resources/list", params).await?;

        serde_json::from_value(
            response
                .result
                .ok_or_else(|| MCPError::RequestFailed("No result".to_string()))?,
        )
        .map_err(|e| MCPError::RequestFailed(e.to_string()))
    }

    /// Read resource
    pub async fn read_resource(
        &self,
        uri: impl Into<String>,
    ) -> Result<ReadResourceResult, MCPError> {
        self.ensure_initialized()?;

        let params = ReadResourceParams { uri: uri.into() };
        let response = self
            .request(
                "resources/read",
                Some(
                    serde_json::to_value(params)
                        .map_err(|e| MCPError::SerializationFailed(e.to_string()))?,
                ),
            )
            .await?;

        serde_json::from_value(
            response
                .result
                .ok_or_else(|| MCPError::RequestFailed("No result".to_string()))?,
        )
        .map_err(|e| MCPError::RequestFailed(e.to_string()))
    }

    /// List prompts
    pub async fn list_prompts(&self, cursor: Option<&str>) -> Result<ListPromptsResult, MCPError> {
        self.ensure_initialized()?;

        let params = cursor.map(|c| serde_json::json!({ "cursor": c }));
        let response = self.request("prompts/list", params).await?;

        serde_json::from_value(
            response
                .result
                .ok_or_else(|| MCPError::RequestFailed("No result".to_string()))?,
        )
        .map_err(|e| MCPError::RequestFailed(e.to_string()))
    }

    /// Get prompt
    pub async fn get_prompt(
        &self,
        name: impl Into<String>,
        arguments: Option<std::collections::HashMap<String, String>>,
    ) -> Result<GetPromptResult, MCPError> {
        self.ensure_initialized()?;

        let params = GetPromptParams {
            name: name.into(),
            arguments,
        };

        let response = self
            .request(
                "prompts/get",
                Some(
                    serde_json::to_value(params)
                        .map_err(|e| MCPError::SerializationFailed(e.to_string()))?,
                ),
            )
            .await?;

        serde_json::from_value(
            response
                .result
                .ok_or_else(|| MCPError::RequestFailed("No result".to_string()))?,
        )
        .map_err(|e| MCPError::RequestFailed(e.to_string()))
    }

    /// Send ping
    pub async fn ping(&self) -> Result<(), MCPError> {
        self.request(
            "ping",
            Some(
                serde_json::to_value(PingParams::default())
                    .map_err(|e| MCPError::SerializationFailed(e.to_string()))?,
            ),
        )
        .await?;
        Ok(())
    }

    /// Close connection
    pub async fn close(&self) -> Result<(), MCPError> {
        if self.initialized.load(Ordering::SeqCst) {
            let _ = self.notify("notifications/cancelled", None).await;
            self.initialized.store(false, Ordering::SeqCst);
        }
        Ok(())
    }

    /// Send request
    ///
    /// ARCHITECTURE FIX: Implements proper request-response matching using
    /// request IDs. Each request is assigned a unique ID and registered in
    /// pending_requests. The response handler task routes responses back to
    /// the correct request.
    async fn request(
        &self,
        method: impl Into<String>,
        params: Option<Value>,
    ) -> Result<JsonRpcResponse, MCPError> {
        let id = {
            let mut counter = self.request_counter.lock().await;
            *counter += 1;
            *counter
        };

        let request_id = RequestId::Number(id as i64);
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: request_id.clone(),
            method: method.into(),
            params,
        };

        // Create oneshot channel for this specific request
        let (tx, rx) = tokio::sync::oneshot::channel();

        // Register pending request
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(request_id.clone(), tx);
        }

        // Send request
        self.request_tx
            .send(request)
            .map_err(|_| MCPError::ConnectionFailed("Channel closed".to_string()))?;

        // Wait for matching response with timeout
        match tokio::time::timeout(
            tokio::time::Duration::from_millis(self.config.timeout_ms),
            rx,
        )
        .await
        {
            Ok(Ok(response)) => {
                if let Some(error) = response.error {
                    Err(MCPError::RequestFailed(format!(
                        "{}: {}",
                        error.code, error.message
                    )))
                } else {
                    Ok(response)
                }
            }
            Ok(Err(_)) => {
                // Response handler dropped - clean up pending request
                let mut pending = self.pending_requests.lock().await;
                pending.remove(&request_id);
                Err(MCPError::ConnectionFailed(
                    "Response channel closed".to_string(),
                ))
            }
            Err(_) => {
                // Timeout - clean up pending request
                let mut pending = self.pending_requests.lock().await;
                pending.remove(&request_id);
                Err(MCPError::Timeout)
            }
        }
    }

    /// Send notification (no response expected)
    async fn notify(
        &self,
        method: impl Into<String>,
        params: Option<Value>,
    ) -> Result<(), MCPError> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: RequestId::Null,
            method: method.into(),
            params,
        };

        self.request_tx
            .send(request)
            .map_err(|_| MCPError::ConnectionFailed("Channel closed".to_string()))?;
        Ok(())
    }

    fn ensure_initialized(&self) -> Result<(), MCPError> {
        if !self.initialized.load(Ordering::SeqCst) {
            return Err(MCPError::NotInitialized);
        }
        Ok(())
    }
}

/// ARCHITECTURE FIX: Implement Drop for graceful cleanup
///
/// Note: This uses try_lock to avoid blocking in drop.
/// In production, prefer explicit close() calls for proper async cleanup.
impl Drop for MCPClient {
    fn drop(&mut self) {
        // Mark as not initialized to signal shutdown
        self.initialized.store(false, Ordering::SeqCst);

        // Try to clear pending requests without blocking
        if let Ok(mut pending) = self.pending_requests.try_lock() {
            pending.clear();
        }

        // Note: We cannot send async close notification in Drop
        // The application should call close().await before dropping
        tracing::debug!("MCPClient dropped, pending requests cleared");
    }
}
