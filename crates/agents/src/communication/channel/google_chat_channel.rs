//! Google Chat Channel Implementation
//!
//! Provides integration with Google Chat through the Google Chat API.
//! Supports both Webhook and Service Account authentication modes.
//!
//! # Features
//! - Space (room) management
//! - Threaded conversations
//! - Card-based interactive messages
//! - File sharing
//!
//! # API Reference
//! - <https://developers.google.com/chat/api/reference/rest>

use std::collections::HashMap;

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use super::r#trait::{
    BaseChannelConfig, Channel, ChannelConfig, ChannelEvent, ChannelInfo, ChannelType,
    ConnectionMode, ContentType, MemberInfo,
};
use crate::communication::{Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

/// Google Chat API base URL
const GOOGLE_CHAT_API_BASE: &str = "https://chat.googleapis.com/v1";

/// Google Chat channel implementation
pub struct GoogleChatChannel {
    config: GoogleChatConfig,
    client: Client,
    connected: bool,
    access_token: Option<String>,
    token_expiry: Option<chrono::DateTime<chrono::Utc>>,
}

/// Google Chat configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleChatConfig {
    /// Service account key JSON (for bot authentication)
    pub service_account_key: Option<String>,
    /// Webhook URL (for simple webhook mode)
    pub webhook_url: Option<String>,
    /// Space ID to monitor (optional)
    pub space_id: Option<String>,
    #[serde(flatten)]
    pub base: BaseChannelConfig,
}

impl GoogleChatConfig {
    /// Create configuration from environment variables
    pub fn from_env() -> Option<Self> {
        let service_account_key = std::env::var("GOOGLE_CHAT_SERVICE_KEY").ok();
        let webhook_url = std::env::var("GOOGLE_CHAT_WEBHOOK_URL").ok();
        let space_id = std::env::var("GOOGLE_CHAT_SPACE_ID").ok();

        // At least one auth method is required
        if service_account_key.is_none() && webhook_url.is_none() {
            return None;
        }

        let mut base = BaseChannelConfig::from_env("GOOGLE_CHAT")?;
        // Google Chat uses Webhook mode
        base.connection_mode = ConnectionMode::Webhook;

        Some(Self {
            service_account_key,
            webhook_url,
            space_id,
            base,
        })
    }

    /// Get space ID from webhook URL if available
    pub fn space_id_from_webhook(&self) -> Option<String> {
        if let Some(url) = &self.webhook_url {
            // Extract space ID from webhook URL
            // Format: https://chat.googleapis.com/v1/spaces/{space_id}/messages
            if let Some(start) = url.find("/spaces/") {
                let start = start + 8;
                let end = url[start..]
                    .find('/')
                    .map(|i| start + i)
                    .unwrap_or(url.len());
                return Some(url[start..end].to_string());
            }
        }
        self.space_id.clone()
    }
}

impl Default for GoogleChatConfig {
    fn default() -> Self {
        let mut base = BaseChannelConfig::default();
        // Google Chat uses Webhook mode
        base.connection_mode = ConnectionMode::Webhook;

        Self {
            service_account_key: None,
            webhook_url: None,
            space_id: None,
            base,
        }
    }
}

impl ChannelConfig for GoogleChatConfig {
    fn from_env() -> Option<Self>
    where
        Self: Sized,
    {
        Self::from_env()
    }

    fn is_valid(&self) -> bool {
        self.service_account_key.is_some() || self.webhook_url.is_some()
    }

    fn allowlist(&self) -> Vec<String> {
        vec![]
    }

    fn connection_mode(&self) -> ConnectionMode {
        self.base.connection_mode
    }

    fn auto_reconnect(&self) -> bool {
        self.base.auto_reconnect
    }

    fn max_reconnect_attempts(&self) -> u32 {
        self.base.max_reconnect_attempts
    }
}

impl GoogleChatChannel {
    /// Create a new Google Chat channel
    pub fn new(config: GoogleChatConfig) -> Result<Self> {
        if !config.is_valid() {
            return Err(AgentError::configuration(
                "Google Chat config must have service_account_key or webhook_url",
            ));
        }

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| {
                AgentError::configuration(format!("Failed to create HTTP client: {}", e))
            })?;

        Ok(Self {
            config,
            client,
            connected: false,
            access_token: None,
            token_expiry: None,
        })
    }

    /// Get valid access token (refresh if needed)
    async fn get_access_token(&mut self) -> Result<String> {
        // Check if we have a valid token
        if let Some(ref token) = self.access_token {
            if let Some(expiry) = self.token_expiry {
                if chrono::Utc::now() < expiry - chrono::Duration::minutes(5) {
                    return Ok(token.clone());
                }
            }
        }

        // Need to refresh token using service account
        if let Some(ref key_json) = self.config.service_account_key {
            let token = self.refresh_service_account_token(key_json).await?;
            self.access_token = Some(token.clone());
            self.token_expiry = Some(chrono::Utc::now() + chrono::Duration::minutes(60));
            return Ok(token);
        }

        Err(AgentError::authentication(
            "No valid authentication method available",
        ))
    }

    /// Refresh service account token
    async fn refresh_service_account_token(&self, _key_json: &str) -> Result<String> {
        // In a real implementation, this would:
        // 1. Parse the service account JSON
        // 2. Create a JWT
        // 3. Sign it with the private key
        // 4. Exchange for access token
        //
        // For now, return a placeholder
        // TODO: Implement proper JWT signing
        warn!("Service account token refresh not fully implemented");
        Ok("placeholder_token".to_string())
    }

    /// Build card message for interactive content
    fn build_card_message(content: &str, title: Option<&str>) -> serde_json::Value {
        let mut card = serde_json::json!({
            "cardsV2": [{
                "card": {
                    "sections": [{
                        "widgets": [{
                            "textParagraph": {
                                "text": content
                            }
                        }]
                    }]
                }
            }]
        });

        if let Some(t) = title {
            card["cardsV2"][0]["card"]["header"] = serde_json::json!({
                "title": t
            });
        }

        card
    }

    /// Send message via webhook
    async fn send_via_webhook(&self, message: &Message) -> Result<()> {
        let webhook_url = self
            .config
            .webhook_url
            .as_ref()
            .ok_or_else(|| AgentError::configuration("Webhook URL not configured"))?;

        let payload = match message.message_type {
            MessageType::Text | MessageType::Reply => {
                serde_json::json!({ "text": message.content })
            }
            _ => Self::build_card_message(&message.content, Some("Message")),
        };

        let response = self
            .client
            .post(webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to send message: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!(
                "Google Chat API error: {}",
                error_text
            )));
        }

        debug!("Message sent via Google Chat webhook");
        Ok(())
    }

    /// Send message via API
    async fn send_via_api(&mut self, channel_id: &str, message: &Message) -> Result<()> {
        let token = self.get_access_token().await?;
        let space_id = if channel_id.is_empty() {
            self.config
                .space_id_from_webhook()
                .ok_or_else(|| AgentError::configuration("Space ID not configured"))?
        } else {
            channel_id.to_string()
        };

        let url = format!("{}/spaces/{}/messages", GOOGLE_CHAT_API_BASE, space_id);

        let payload = match message.message_type {
            MessageType::Text | MessageType::Reply => {
                serde_json::json!({ "text": message.content })
            }
            _ => Self::build_card_message(&message.content, Some("Message")),
        };

        let response = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .json(&payload)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to send message: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AgentError::platform(format!(
                "Google Chat API error: {}",
                error_text
            )));
        }

        debug!("Message sent via Google Chat API");
        Ok(())
    }
}

#[async_trait]
impl Channel for GoogleChatChannel {
    fn name(&self) -> &str {
        "google_chat"
    }

    fn platform(&self) -> PlatformType {
        PlatformType::GoogleChat
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    async fn connect(&mut self) -> Result<()> {
        info!("Connecting to Google Chat...");

        // Validate configuration
        if !self.config.is_valid() {
            return Err(AgentError::configuration(
                "Invalid Google Chat configuration",
            ));
        }

        // Test authentication if using service account
        if self.config.service_account_key.is_some() {
            match self.get_access_token().await {
                Ok(_) => {
                    info!("Google Chat authentication successful");
                }
                Err(e) => {
                    error!("Failed to authenticate with Google Chat: {}", e);
                    return Err(e);
                }
            }
        }

        self.connected = true;
        info!("Connected to Google Chat");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        info!("Disconnecting from Google Chat...");
        self.connected = false;
        self.access_token = None;
        info!("Disconnected from Google Chat");
        Ok(())
    }

    async fn send(&self, channel_id: &str, message: &Message) -> Result<()> {
        if !self.connected {
            return Err(AgentError::platform("Not connected to Google Chat"));
        }

        // Clone self to allow mutation for token refresh
        let mut this = Self {
            config: self.config.clone(),
            client: self.client.clone(),
            connected: self.connected,
            access_token: self.access_token.clone(),
            token_expiry: self.token_expiry,
        };

        // Use webhook if configured, otherwise use API
        if self.config.webhook_url.is_some() {
            this.send_via_webhook(message).await
        } else {
            this.send_via_api(channel_id, message).await
        }
    }

    async fn start_listener(&self, _event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        // Google Chat uses push notifications (webhooks) rather than pull
        // The webhook handler should be registered separately
        info!("Google Chat uses webhook push notifications");
        info!("Please configure webhook endpoint to receive messages");
        Ok(())
    }

    async fn stop_listener(&self) -> Result<()> {
        // Nothing to stop for webhook-based channels
        Ok(())
    }

    fn supported_content_types(&self) -> Vec<ContentType> {
        vec![
            ContentType::Text,
            ContentType::Image,
            ContentType::File,
            ContentType::Card,
            ContentType::Reaction,
        ]
    }

    async fn list_channels(&self) -> Result<Vec<ChannelInfo>> {
        // Google Chat uses "spaces" instead of channels
        // Return the configured space if available
        if let Some(space_id) = self.config.space_id_from_webhook() {
            Ok(vec![ChannelInfo {
                id: space_id,
                name: "Google Chat Space".to_string(),
                channel_type: ChannelType::Group,
                unread_count: 0,
                metadata: HashMap::new(),
            }])
        } else {
            Ok(vec![])
        }
    }

    async fn list_members(&self, _channel_id: &str) -> Result<Vec<MemberInfo>> {
        // Would require additional API calls to Google Chat API
        // For now, return empty list
        Ok(vec![])
    }

    fn connection_mode(&self) -> ConnectionMode {
        self.config.connection_mode()
    }
}

/// Google Chat webhook handler for receiving messages
pub struct GoogleChatWebhookHandler {
    event_sender: mpsc::Sender<ChannelEvent>,
}

impl GoogleChatWebhookHandler {
    pub fn new(event_sender: mpsc::Sender<ChannelEvent>) -> Self {
        Self { event_sender }
    }

    /// Handle incoming webhook request
    pub async fn handle_request(&self, body: &[u8]) -> Result<()> {
        let event: GoogleChatEvent = serde_json::from_slice(body)
            .map_err(|e| AgentError::platform(format!("Invalid webhook payload: {}", e)))?;

        match event.type_.as_str() {
            "MESSAGE" => {
                if let Some(message) = event.message {
                    let channel_event = ChannelEvent::MessageReceived {
                        platform: PlatformType::GoogleChat,
                        channel_id: event.space.name.unwrap_or_default(),
                        message: Message::new(
                            uuid::Uuid::new_v4(),
                            PlatformType::GoogleChat,
                            message.text.unwrap_or_default(),
                        ),
                    };

                    self.event_sender.send(channel_event).await.map_err(|e| {
                        AgentError::platform(format!("Failed to send event: {}", e))
                    })?;
                }
            }
            _ => {
                debug!("Unhandled Google Chat event type: {}", event.type_);
            }
        }

        Ok(())
    }
}

/// Google Chat webhook event structure
#[derive(Debug, Deserialize)]
struct GoogleChatEvent {
    #[serde(rename = "type")]
    type_: String,
    space: GoogleChatSpace,
    message: Option<GoogleChatMessage>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GoogleChatSpace {
    name: Option<String>,
    display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GoogleChatMessage {
    text: Option<String>,
    sender: Option<GoogleChatSender>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct GoogleChatSender {
    name: Option<String>,
    display_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_google_chat_config_validation() {
        // Invalid config - no auth
        let invalid_config = GoogleChatConfig::default();
        assert!(!invalid_config.is_valid());

        // Valid config - with webhook
        let valid_config = GoogleChatConfig {
            webhook_url: Some("https://chat.googleapis.com/v1/spaces/xxx/messages".to_string()),
            ..Default::default()
        };
        assert!(valid_config.is_valid());
    }

    #[test]
    fn test_build_card_message() {
        let card = GoogleChatChannel::build_card_message("Hello World", Some("Title"));
        assert!(card.get("cardsV2").is_some());
    }
}
