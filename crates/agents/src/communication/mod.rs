//! Agent Communication Module
//!
//! Handles multi-platform communication routing and message delivery.
//! Provides integration between messaging platforms (Slack, Telegram, Lark,
//! etc.) and the agent's LLM processing pipeline.

pub mod channel;
pub mod integration;
pub mod router;
pub mod thread;
pub mod voice;
pub mod webhook;

// 🟢 P0 FIX: Message history tracking
pub mod message_history;

// Multi-user multi-agent channel architecture
pub mod agent_channel;
pub mod channel_instance_manager;
pub mod message_router_v2;
pub mod offline_message_store;
pub mod offline_message_store_sqlite;
pub mod user_channel;

use std::collections::HashMap;
use std::sync::Arc;

pub use agent_channel::{AgentChannelBinding, RoutingRules};
use async_trait::async_trait;
pub use channel_instance_manager::{ChannelInstanceManager, ChannelInstanceStatus};
pub use message_history::{
    ChannelHistoryExport, HistoryQuery, HistoryQueryResult, MessageDeletionRecord,
    MessageEditRecord, MessageHistoryStats, MessageHistoryStore, MessagePinRecord, MessageSnapshot,
    OperationType, SearchResult,
};
pub use message_router_v2::{
    AgentMessageDispatcher, InboundMessageRouter, OutboundMessageRouter, ReplyRoute,
    RoutingDecision, UserMessageContext,
};
pub use offline_message_store::OfflineMessageStore;
pub use offline_message_store_sqlite::{MemoryOfflineMessageStore, SqliteOfflineMessageStore};
pub use router::{CommunicationRouter, RouteConfig};
use serde::{Deserialize, Serialize};
pub use thread::{Thread, ThreadManager};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
pub use user_channel::{
    ChannelBindingStatus, ChannelInstanceId, ChannelInstanceRef, PlatformCredentials,
    UserChannelBinding, UserChannelConfig,
};
use uuid::Uuid;
pub use voice::{VoiceConfig, VoiceHandler};

use crate::error::{AgentError, Result};

/// Communication message types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MessageType {
    Text,
    Voice,
    Image,
    File,
    System,
    Video,
    Reply,
    Sticker,
}

/// Communication message (platform message)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformMessage {
    pub id: Uuid,
    pub thread_id: Uuid,
    pub platform: PlatformType,
    pub message_type: MessageType,
    pub content: String,
    pub metadata: HashMap<String, String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Type alias for backward compatibility
pub type Message = PlatformMessage;

impl PlatformMessage {
    pub fn new(thread_id: Uuid, platform: PlatformType, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            thread_id,
            platform,
            message_type: MessageType::Text,
            content: content.into(),
            metadata: HashMap::new(),
            timestamp: chrono::Utc::now(),
        }
    }

    /// Create a message with metadata
    pub fn with_metadata(
        thread_id: Uuid,
        platform: PlatformType,
        content: impl Into<String>,
        metadata: HashMap<String, String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            thread_id,
            platform,
            message_type: MessageType::Text,
            content: content.into(),
            metadata,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Set message type
    pub fn with_message_type(mut self, message_type: MessageType) -> Self {
        self.message_type = message_type;
        self
    }
}

/// Platform types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlatformType {
    Slack,
    Telegram,
    Discord,
    WhatsApp,
    Signal,
    IMessage,
    WeChat,
    Teams,
    Twitter,
    Lark,
    DingTalk,
    Matrix,
    GoogleChat,
    Line,
    QQ,
    IRC,
    WebChat,
    Custom,
}

impl std::fmt::Display for PlatformType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlatformType::Slack => write!(f, "slack"),
            PlatformType::Telegram => write!(f, "telegram"),
            PlatformType::Discord => write!(f, "discord"),
            PlatformType::WhatsApp => write!(f, "whatsapp"),
            PlatformType::Signal => write!(f, "signal"),
            PlatformType::IMessage => write!(f, "imessage"),
            PlatformType::WeChat => write!(f, "wechat"),
            PlatformType::Teams => write!(f, "teams"),
            PlatformType::Twitter => write!(f, "twitter"),
            PlatformType::Lark => write!(f, "lark"),
            PlatformType::DingTalk => write!(f, "dingtalk"),
            PlatformType::Matrix => write!(f, "matrix"),
            PlatformType::GoogleChat => write!(f, "googlechat"),
            PlatformType::Line => write!(f, "line"),
            PlatformType::QQ => write!(f, "qq"),
            PlatformType::IRC => write!(f, "irc"),
            PlatformType::WebChat => write!(f, "webchat"),
            PlatformType::Custom => write!(f, "custom"),
        }
    }
}

/// Message handler trait for processing incoming messages
#[async_trait]
pub trait MessageHandler: Send + Sync {
    /// Handle an incoming message
    ///
    /// # Arguments
    /// * `message` - The incoming message
    ///
    /// # Returns
    /// Response message or error
    async fn handle_message(&self, message: Message) -> Result<Option<Message>>;

    /// Get the platform type this handler supports
    fn platform_type(&self) -> PlatformType;
}

/// Webhook handler registration trait
#[async_trait]
pub trait WebhookHandlerRegistry: Send + Sync {
    /// Register a webhook handler
    ///
    /// # Arguments
    /// * `path` - Webhook endpoint path
    /// * `handler` - Handler function
    ///
    /// # Returns
    /// Result indicating success or failure
    async fn register_webhook(
        &self,
        path: &str,
        handler: Box<dyn Fn(Vec<u8>) -> Result<Vec<u8>> + Send + Sync>,
    ) -> Result<()>;

    /// Unregister a webhook handler
    ///
    /// # Arguments
    /// * `path` - Webhook endpoint path
    async fn unregister_webhook(&self, path: &str) -> Result<()>;
}

/// LLM call interface for communication manager
#[async_trait]
pub trait LLMCallInterface: Send + Sync {
    /// Call LLM with messages
    ///
    /// # Arguments
    /// * `messages` - List of messages to send to LLM
    /// * `context` - Additional context for the LLM call
    ///
    /// # Returns
    /// LLM response text
    async fn call_llm(
        &self,
        messages: Vec<Message>,
        context: Option<HashMap<String, String>>,
    ) -> Result<String>;

    /// Call LLM with streaming response
    ///
    /// # Arguments
    /// * `messages` - List of messages to send to LLM
    /// * `context` - Additional context for the LLM call
    ///
    /// # Returns
    /// Stream of response chunks
    async fn call_llm_stream(
        &self,
        messages: Vec<Message>,
        context: Option<HashMap<String, String>>,
    ) -> Result<tokio::sync::mpsc::Receiver<String>>;
}

/// Communication manager
#[allow(dead_code)]
pub struct CommunicationManager {
    router: CommunicationRouter,
    thread_manager: ThreadManager,
    platforms: HashMap<PlatformType, Box<dyn PlatformAdapter>>,
    /// Message handlers for each platform
    message_handlers: HashMap<PlatformType, Vec<Arc<dyn MessageHandler>>>,
    /// Webhook manager
    webhook_manager: Arc<RwLock<webhook::WebhookManager>>,
    /// LLM call interface
    llm_interface: Option<Arc<dyn LLMCallInterface>>,
    /// Processing statistics
    stats: Arc<RwLock<CommunicationStats>>,
}

/// Communication statistics
#[derive(Debug, Clone, Default)]
pub struct CommunicationStats {
    /// Total messages received
    pub messages_received: u64,
    /// Total messages sent
    pub messages_sent: u64,
    /// Total errors
    pub errors: u64,
    /// Messages by platform
    pub messages_by_platform: HashMap<PlatformType, u64>,
    /// Average processing time in milliseconds
    pub avg_processing_time_ms: u64,
}

impl CommunicationManager {
    pub fn new() -> Self {
        Self {
            router: CommunicationRouter::new(),
            thread_manager: ThreadManager::new(),
            platforms: HashMap::new(),
            message_handlers: HashMap::new(),
            webhook_manager: Arc::new(RwLock::new(webhook::WebhookManager::new())),
            llm_interface: None,
            stats: Arc::new(RwLock::new(CommunicationStats::default())),
        }
    }

    /// Create with LLM interface
    pub fn with_llm_interface(mut self, interface: Arc<dyn LLMCallInterface>) -> Self {
        self.llm_interface = Some(interface);
        self
    }

    pub async fn send_message(&mut self, message: Message) -> Result<()> {
        let platform = self.platforms.get_mut(&message.platform).ok_or_else(|| {
            AgentError::communication(format!("Platform {:?} not configured", message.platform))
        })?;

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.messages_sent += 1;
        }

        platform.send(message).await
    }

    pub fn register_platform(
        &mut self,
        platform_type: PlatformType,
        adapter: Box<dyn PlatformAdapter>,
    ) {
        self.platforms.insert(platform_type, adapter);
    }

    /// Register a message handler for a platform
    pub fn register_message_handler(&mut self, handler: Arc<dyn MessageHandler>) {
        let platform = handler.platform_type();
        self.message_handlers
            .entry(platform)
            .or_default()
            .push(handler);
        debug!("Registered message handler for platform: {:?}", platform);
    }

    /// Handle incoming message with registered handlers
    pub async fn handle_incoming_message(&self, message: Message) -> Result<Option<Message>> {
        let platform = message.platform;
        let handlers = self.message_handlers.get(&platform);

        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.messages_received += 1;
            *stats.messages_by_platform.entry(platform).or_default() += 1;
        }

        if let Some(handlers) = handlers {
            for handler in handlers {
                match handler.handle_message(message.clone()).await {
                    Ok(response) => {
                        if response.is_some() {
                            return Ok(response);
                        }
                    }
                    Err(e) => {
                        warn!("Handler error for platform {:?}: {}", platform, e);
                    }
                }
            }
        }

        // If no handler processed the message, try LLM interface
        if let Some(llm) = &self.llm_interface {
            let start_time = std::time::Instant::now();

            let response_text = llm.call_llm(vec![message], None).await?;

            let processing_time = start_time.elapsed().as_millis() as u64;

            // Update stats
            {
                let mut stats = self.stats.write().await;
                // Update average processing time
                let total = stats.messages_received;
                stats.avg_processing_time_ms =
                    (stats.avg_processing_time_ms * (total - 1) + processing_time) / total;
            }

            return Ok(Some(Message::new(Uuid::new_v4(), platform, response_text)));
        }

        Ok(None)
    }

    /// Get webhook manager
    pub fn webhook_manager(&self) -> Arc<RwLock<webhook::WebhookManager>> {
        self.webhook_manager.clone()
    }

    /// Register webhook handler
    pub async fn register_webhook_handler(
        &self,
        handler: Arc<dyn webhook::WebhookHandler>,
    ) -> Result<()> {
        let manager = self.webhook_manager.read().await;
        manager.register_handler(handler).await
    }

    /// Handle webhook request
    pub async fn handle_webhook_request(
        &self,
        path: &str,
        body: &[u8],
        signature: Option<&str>,
        timestamp: Option<&str>,
    ) -> Result<Vec<webhook::WebhookEvent>> {
        let manager = self.webhook_manager.read().await;
        manager
            .handle_request(path, body, signature, timestamp)
            .await
    }

    /// Set LLM call interface
    pub fn set_llm_interface(&mut self, interface: Arc<dyn LLMCallInterface>) {
        self.llm_interface = Some(interface);
    }

    /// Get LLM call interface
    pub fn llm_interface(&self) -> Option<Arc<dyn LLMCallInterface>> {
        self.llm_interface.clone()
    }

    /// Get communication statistics
    pub async fn get_stats(&self) -> CommunicationStats {
        self.stats.read().await.clone()
    }

    /// Reset statistics
    pub async fn reset_stats(&self) {
        let mut stats = self.stats.write().await;
        *stats = CommunicationStats::default();
    }

    /// Connect all platforms
    pub async fn connect_all(&mut self) -> Vec<Result<()>> {
        let mut results = Vec::new();
        for (platform_type, adapter) in &mut self.platforms {
            info!("Connecting to platform: {:?}", platform_type);
            match adapter.connect().await {
                Ok(()) => {
                    info!("Successfully connected to {:?}", platform_type);
                    results.push(Ok(()));
                }
                Err(e) => {
                    error!("Failed to connect to {:?}: {}", platform_type, e);
                    results.push(Err(e));
                }
            }
        }
        results
    }

    /// Disconnect all platforms
    pub async fn disconnect_all(&mut self) -> Vec<Result<()>> {
        let mut results = Vec::new();
        for (platform_type, adapter) in &mut self.platforms {
            info!("Disconnecting from platform: {:?}", platform_type);
            match adapter.disconnect().await {
                Ok(()) => {
                    info!("Successfully disconnected from {:?}", platform_type);
                    results.push(Ok(()));
                }
                Err(e) => {
                    error!("Failed to disconnect from {:?}: {}", platform_type, e);
                    results.push(Err(e));
                }
            }
        }
        results
    }

    /// Check if platform is connected
    pub fn is_platform_connected(&self, platform: PlatformType) -> bool {
        self.platforms
            .get(&platform)
            .map(|a| a.is_connected())
            .unwrap_or(false)
    }

    /// Get list of configured platforms
    pub fn get_configured_platforms(&self) -> Vec<PlatformType> {
        self.platforms.keys().cloned().collect()
    }

    /// Get list of platforms with handlers
    pub fn get_platforms_with_handlers(&self) -> Vec<PlatformType> {
        self.message_handlers.keys().cloned().collect()
    }
}

impl Default for CommunicationManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Platform adapter trait
#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    async fn send(&mut self, message: Message) -> Result<()>;
    async fn receive(&mut self) -> Result<Option<Message>>;
    async fn connect(&mut self) -> Result<()>;
    async fn disconnect(&mut self) -> Result<()>;
    fn is_connected(&self) -> bool;
}

/// Default LLM call interface implementation
#[allow(dead_code)]
pub struct DefaultLLMInterface {
    /// Model router for LLM calls
    #[allow(dead_code)]
    model_router: Arc<RwLock<crate::models::router::ModelRouter>>,
}

impl DefaultLLMInterface {
    pub fn new(router: crate::models::router::ModelRouter) -> Self {
        Self {
            model_router: Arc::new(RwLock::new(router)),
        }
    }

    /// Create with default OpenAI provider from environment
    pub async fn from_env() -> Result<Self> {
        use crate::llm::providers::{OpenAIConfig, OpenAIProvider};

        let config = OpenAIConfig::from_env()
            .map_err(|e| AgentError::InvalidConfig(format!("OpenAI config error: {}", e)))?;

        let provider = OpenAIProvider::new(config)
            .map_err(|e| AgentError::InvalidConfig(format!("OpenAI provider error: {}", e)))?;

        let router = crate::models::router::ModelRouter::new("openai");
        router.register_provider("openai", Arc::new(provider)).await;

        Ok(Self {
            model_router: Arc::new(RwLock::new(router)),
        })
    }
}

#[async_trait]
impl LLMCallInterface for DefaultLLMInterface {
    async fn call_llm(
        &self,
        messages: Vec<Message>,
        context: Option<HashMap<String, String>>,
    ) -> Result<String> {
        // Convert messages to a single prompt
        let prompt = messages
            .iter()
            .map(|m| format!("[{:?}]: {}", m.platform, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        // Extract config from context
        let config = crate::models::ModelConfig {
            provider: context
                .as_ref()
                .and_then(|c| c.get("provider"))
                .cloned()
                .unwrap_or_default(),
            model: context
                .as_ref()
                .and_then(|c| c.get("model"))
                .cloned()
                .unwrap_or_else(|| "gpt-4o-mini".to_string()),
            temperature: context
                .as_ref()
                .and_then(|c| c.get("temperature"))
                .and_then(|t| t.parse().ok())
                .unwrap_or(0.7),
            max_tokens: context
                .as_ref()
                .and_then(|c| c.get("max_tokens"))
                .and_then(|t| t.parse().ok())
                .unwrap_or(2048),
            top_p: context
                .as_ref()
                .and_then(|c| c.get("top_p"))
                .and_then(|t| t.parse().ok())
                .unwrap_or(1.0),
        };

        let request = crate::models::CompletionRequest { prompt, config };

        // Call model router
        let response = self
            .model_router
            .read()
            .await
            .complete(request)
            .await
            .map_err(|e| AgentError::Execution(format!("LLM call failed: {}", e)))?;

        Ok(response.text)
    }

    async fn call_llm_stream(
        &self,
        messages: Vec<Message>,
        context: Option<HashMap<String, String>>,
    ) -> Result<tokio::sync::mpsc::Receiver<String>> {
        // Convert messages to a single prompt
        let prompt = messages
            .iter()
            .map(|m| format!("[{:?}]: {}", m.platform, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        // Extract config from context
        let config = crate::models::ModelConfig {
            provider: context
                .as_ref()
                .and_then(|c| c.get("provider"))
                .cloned()
                .unwrap_or_default(),
            model: context
                .as_ref()
                .and_then(|c| c.get("model"))
                .cloned()
                .unwrap_or_else(|| "gpt-4o-mini".to_string()),
            temperature: context
                .as_ref()
                .and_then(|c| c.get("temperature"))
                .and_then(|t| t.parse().ok())
                .unwrap_or(0.7),
            max_tokens: context
                .as_ref()
                .and_then(|c| c.get("max_tokens"))
                .and_then(|t| t.parse().ok())
                .unwrap_or(2048),
            top_p: context
                .as_ref()
                .and_then(|c| c.get("top_p"))
                .and_then(|t| t.parse().ok())
                .unwrap_or(1.0),
        };

        let request = crate::models::CompletionRequest { prompt, config };

        // Get stream from model router
        self.model_router
            .read()
            .await
            .complete_stream(request)
            .await
            .map_err(|e| AgentError::Execution(format!("LLM stream failed: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let thread_id = Uuid::new_v4();
        let msg = Message::new(thread_id, PlatformType::Slack, "Hello");
        assert_eq!(msg.content, "Hello");
        assert_eq!(msg.platform, PlatformType::Slack);
    }

    #[test]
    fn test_message_with_metadata() {
        let thread_id = Uuid::new_v4();
        let mut metadata = HashMap::new();
        metadata.insert("key".to_string(), "value".to_string());

        let msg = Message::with_metadata(thread_id, PlatformType::Lark, "Hello", metadata);
        assert_eq!(msg.metadata.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_platform_type_display() {
        assert_eq!(format!("{}", PlatformType::Lark), "lark");
        assert_eq!(format!("{}", PlatformType::Slack), "slack");
        assert_eq!(format!("{}", PlatformType::Telegram), "telegram");
    }

    #[test]
    fn test_communication_stats_default() {
        let stats = CommunicationStats::default();
        assert_eq!(stats.messages_received, 0);
        assert_eq!(stats.messages_sent, 0);
        assert_eq!(stats.errors, 0);
        assert!(stats.messages_by_platform.is_empty());
    }

    #[tokio::test]
    async fn test_communication_manager_new() {
        let manager = CommunicationManager::new();
        let stats = manager.get_stats().await;
        assert_eq!(stats.messages_received, 0);
    }
}
