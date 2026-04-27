//! Telegram Webhook Handler
//!
//! Handles incoming webhooks from Telegram Bot API.
//! Supports signature verification and various update types.
//!
//! Refactored to use common webhook utilities from the common module.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

// Import common webhook utilities
use super::common::{
    MetadataBuilder, SignatureVerification as CommonSigVerification, SignatureVerifier,
    TokenVerifier,
};
use crate::communication::channel::telegram_channel::{
    TelegramCallbackQuery, TelegramInlineQuery, TelegramMessage,
};
use crate::communication::webhook::{
    SignatureVerification, WebhookConfig, WebhookEvent, WebhookEventType, WebhookHandler,
};
use crate::communication::{AgentMessageDispatcher, Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

/// Telegram webhook payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramWebhookPayload {
    #[serde(rename = "update_id")]
    pub update_id: i64,
    pub message: Option<TelegramMessage>,
    #[serde(rename = "edited_message")]
    pub edited_message: Option<TelegramMessage>,
    #[serde(rename = "channel_post")]
    pub channel_post: Option<TelegramMessage>,
    #[serde(rename = "edited_channel_post")]
    pub edited_channel_post: Option<TelegramMessage>,
    #[serde(rename = "callback_query")]
    pub callback_query: Option<TelegramCallbackQuery>,
    #[serde(rename = "inline_query")]
    pub inline_query: Option<TelegramInlineQuery>,
}

/// Telegram webhook handler configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramWebhookConfig {
    pub bot_token: String,
    pub secret_token: Option<String>,
    pub endpoint_path: String,
    pub max_body_size: usize,
    pub timeout_secs: u64,
}

impl Default for TelegramWebhookConfig {
    fn default() -> Self {
        Self {
            bot_token: String::new(),
            secret_token: None,
            endpoint_path: "/webhook/telegram".to_string(),
            max_body_size: 10 * 1024 * 1024,
            timeout_secs: 30,
        }
    }
}

/// Telegram webhook handler
pub struct TelegramWebhookHandler {
    config: WebhookConfig,
    telegram_config: TelegramWebhookConfig,
    // Use common token verifier
    token_verifier: Option<TokenVerifier>,
    dispatcher: Option<Arc<AgentMessageDispatcher>>,
}

impl TelegramWebhookHandler {
    /// Create a new Telegram webhook handler
    pub fn new(telegram_config: TelegramWebhookConfig) -> Self {
        let mut config = WebhookConfig::default();
        config.platform = PlatformType::Telegram;
        config.endpoint_path = telegram_config.endpoint_path.clone();
        config.secret = telegram_config.secret_token.clone();
        config.verify_signatures = telegram_config.secret_token.is_some();
        config.max_body_size = telegram_config.max_body_size;
        config.timeout_secs = telegram_config.timeout_secs;

        // Create token verifier if secret token is provided
        let token_verifier = telegram_config
            .secret_token
            .as_ref()
            .map(|secret| TokenVerifier::new(secret));

        Self {
            config,
            telegram_config,
            token_verifier,
            dispatcher: None,
        }
    }

    /// Attach an agent message dispatcher.
    pub fn with_dispatcher(mut self, dispatcher: Arc<AgentMessageDispatcher>) -> Self {
        self.dispatcher = Some(dispatcher);
        self
    }

    /// Create handler from environment variables
    pub fn from_env() -> Result<Self> {
        let bot_token = std::env::var("TELEGRAM_BOT_TOKEN")
            .map_err(|_| AgentError::configuration("TELEGRAM_BOT_TOKEN not set"))?;
        let secret_token = std::env::var("TELEGRAM_WEBHOOK_SECRET").ok();
        let endpoint_path = std::env::var("TELEGRAM_WEBHOOK_PATH")
            .unwrap_or_else(|_| "/webhook/telegram".to_string());

        Ok(Self::new(TelegramWebhookConfig {
            bot_token,
            secret_token,
            endpoint_path,
            ..Default::default()
        }))
    }

    /// Parse update type using common patterns
    fn parse_update_type(&self, payload: &TelegramWebhookPayload) -> WebhookEventType {
        if payload.message.is_some() || payload.channel_post.is_some() {
            WebhookEventType::MessageReceived
        } else if payload.edited_message.is_some() || payload.edited_channel_post.is_some() {
            WebhookEventType::MessageEdited
        } else if payload.callback_query.is_some() {
            WebhookEventType::BotMentioned
        } else if payload.inline_query.is_some() {
            WebhookEventType::MessageReceived
        } else {
            WebhookEventType::Unknown
        }
    }

    /// Parse message type using common utility
    fn parse_message_type(&self, message: &TelegramMessage) -> MessageType {
        parse_message_type_from_telegram(message)
    }

    /// Extract content from Telegram message
    fn extract_message_content(&self, message: &TelegramMessage) -> String {
        if let Some(text) = &message.text {
            text.clone()
        } else if let Some(caption) = &message.caption {
            caption.clone()
        } else if let Some(photo) = &message.photo {
            let largest = photo.iter().max_by_key(|p| p.file_size.unwrap_or(0));
            format!(
                "[Photo] {}",
                largest.map(|p| p.file_id.clone()).unwrap_or_default()
            )
        } else if let Some(document) = &message.document {
            format!(
                "[File: {}]",
                document.file_name.as_deref().unwrap_or("unnamed")
            )
        } else if let Some(audio) = &message.audio {
            format!(
                "[Audio: {} - {}]",
                audio.performer.as_deref().unwrap_or("Unknown"),
                audio.title.as_deref().unwrap_or("Unknown")
            )
        } else if let Some(voice) = &message.voice {
            format!("[Voice: {}s]", voice.duration)
        } else if let Some(video) = &message.video {
            format!("[Video: {}s]", video.duration)
        } else if let Some(sticker) = &message.sticker {
            format!("[Sticker: {:?}]", sticker.emoji)
        } else if let Some(location) = &message.location {
            format!("[Location: {}, {}]", location.latitude, location.longitude)
        } else if let Some(contact) = &message.contact {
            format!(
                "[Contact: {} {}]",
                contact.first_name,
                contact.last_name.as_deref().unwrap_or("")
            )
        } else {
            "[Unknown message type]".to_string()
        }
    }

    /// Build metadata using common MetadataBuilder
    fn build_message_metadata(&self, message: &TelegramMessage) -> HashMap<String, String> {
        let mut builder = MetadataBuilder::new()
            .add("message_id", message.message_id.to_string())
            .add("chat_id", message.chat.id.to_string())
            .add("chat_type", &message.chat.chat_type);

        if let Some(from) = &message.from {
            builder = builder
                .add("sender_id", from.id.to_string())
                .add("sender_name", &from.first_name)
                .add("is_bot", from.is_bot.to_string())
                .add_optional("sender_username", from.username.as_ref());
        }

        builder = builder
            .add_optional("chat_title", message.chat.title.as_ref())
            .add_optional("chat_username", message.chat.username.as_ref());

        // Add file IDs for media messages
        if let Some(photo) = &message.photo {
            if let Some(largest) = photo.iter().max_by_key(|p| p.file_size.unwrap_or(0)) {
                builder = builder
                    .add("file_id", &largest.file_id)
                    .add("file_unique_id", &largest.file_unique_id);
            }
        } else if let Some(document) = &message.document {
            builder = builder
                .add("file_id", &document.file_id)
                .add("file_unique_id", &document.file_unique_id)
                .add_optional("mime_type", document.mime_type.as_ref());
        } else if let Some(audio) = &message.audio {
            builder = builder
                .add("file_id", &audio.file_id)
                .add("file_unique_id", &audio.file_unique_id);
        } else if let Some(voice) = &message.voice {
            builder = builder
                .add("file_id", &voice.file_id)
                .add("file_unique_id", &voice.file_unique_id)
                .add("duration", voice.duration.to_string());
        } else if let Some(video) = &message.video {
            builder = builder
                .add("file_id", &video.file_id)
                .add("file_unique_id", &video.file_unique_id)
                .add("duration", video.duration.to_string());
        } else if let Some(sticker) = &message.sticker {
            builder = builder
                .add("file_id", &sticker.file_id)
                .add("file_unique_id", &sticker.file_unique_id)
                .add_optional("sticker_emoji", sticker.emoji.as_ref());
        }

        if let Some(reply_to) = &message.reply_to_message {
            builder = builder.add("reply_to_message_id", reply_to.message_id.to_string());
        }

        builder.build()
    }

    /// Convert Telegram update to internal Message
    fn convert_to_message(&self, payload: &TelegramWebhookPayload) -> Option<Message> {
        let message = payload
            .message
            .as_ref()
            .or(payload.edited_message.as_ref())
            .or(payload.channel_post.as_ref())
            .or(payload.edited_channel_post.as_ref())?;

        let message_type = self.parse_message_type(message);
        let content = self.extract_message_content(message);
        let metadata = self.build_message_metadata(message);

        Some(Message {
            id: uuid::Uuid::new_v4(),
            thread_id: uuid::Uuid::new_v4(),
            platform: PlatformType::Telegram,
            message_type,
            content,
            metadata,
            timestamp: chrono::DateTime::from_timestamp(message.date, 0)
                .unwrap_or_else(chrono::Utc::now),
        })
    }

    /// Handle callback query
    fn handle_callback_query(&self, query: &TelegramCallbackQuery) -> WebhookEvent {
        let metadata = MetadataBuilder::new()
            .add("callback_query_id", &query.id)
            .add("user_id", query.from.id.to_string())
            .add("user_name", &query.from.first_name)
            .add("chat_instance", &query.chat_instance)
            .add_optional("callback_data", query.data.as_ref())
            .build();

        WebhookEvent {
            event_type: WebhookEventType::BotMentioned,
            platform: PlatformType::Telegram,
            event_id: query.id.clone(),
            timestamp: chrono::Utc::now(),
            payload: serde_json::to_value(query).unwrap_or_default(),
            message: None,
            metadata,
        }
    }

    /// Handle inline query
    fn handle_inline_query(&self, query: &TelegramInlineQuery) -> WebhookEvent {
        let metadata = MetadataBuilder::new()
            .add("inline_query_id", &query.id)
            .add("user_id", query.from.id.to_string())
            .add("user_name", &query.from.first_name)
            .add("query", &query.query)
            .add("offset", &query.offset)
            .build();

        WebhookEvent {
            event_type: WebhookEventType::MessageReceived,
            platform: PlatformType::Telegram,
            event_id: query.id.clone(),
            timestamp: chrono::Utc::now(),
            payload: serde_json::to_value(query).unwrap_or_default(),
            message: None,
            metadata,
        }
    }

    /// Get the bot token
    pub fn bot_token(&self) -> &str {
        &self.telegram_config.bot_token
    }

    /// Get the secret token
    pub fn secret_token(&self) -> Option<&str> {
        self.telegram_config.secret_token.as_deref()
    }
}

#[async_trait]
impl WebhookHandler for TelegramWebhookHandler {
    fn platform_type(&self) -> PlatformType {
        PlatformType::Telegram
    }

    async fn verify_signature(
        &self,
        _body: &[u8],
        signature: Option<&str>,
        _timestamp: Option<&str>,
    ) -> Result<SignatureVerification> {
        // Use common TokenVerifier if configured
        if let Some(verifier) = &self.token_verifier {
            match signature {
                Some(provided) => match verifier.verify(_body, Some(provided), None).await {
                    Ok(CommonSigVerification::Valid) => Ok(SignatureVerification::Valid),
                    Ok(CommonSigVerification::Invalid) => {
                        warn!("Invalid Telegram webhook secret token");
                        Ok(SignatureVerification::Invalid)
                    }
                    Ok(CommonSigVerification::Skipped) => {
                        // This shouldn't happen with token verifier, but handle it
                        warn!("Missing Telegram webhook secret token");
                        Ok(SignatureVerification::Invalid)
                    }
                    Err(e) => Err(e),
                },
                None => {
                    warn!("Missing Telegram webhook secret token");
                    Ok(SignatureVerification::Invalid)
                }
            }
        } else {
            Ok(SignatureVerification::Skipped)
        }
    }

    async fn parse_payload(&self, body: &[u8]) -> Result<Vec<WebhookEvent>> {
        let payload: TelegramWebhookPayload = serde_json::from_slice(body).map_err(|e| {
            AgentError::platform(format!("Failed to parse Telegram payload: {}", e))
        })?;

        debug!("Received Telegram update: {}", payload.update_id);

        let mut events = Vec::new();

        // Handle callback query
        if let Some(callback_query) = &payload.callback_query {
            events.push(self.handle_callback_query(callback_query));
            return Ok(events);
        }

        // Handle inline query
        if let Some(inline_query) = &payload.inline_query {
            events.push(self.handle_inline_query(inline_query));
            return Ok(events);
        }

        // Handle regular message
        let event_type = self.parse_update_type(&payload);
        let message = self.convert_to_message(&payload);

        let mut metadata = HashMap::new();
        metadata.insert("update_id".to_string(), payload.update_id.to_string());

        if payload.edited_message.is_some() {
            metadata.insert("is_edited".to_string(), "true".to_string());
        }
        if payload.channel_post.is_some() || payload.edited_channel_post.is_some() {
            metadata.insert("is_channel_post".to_string(), "true".to_string());
        }

        let webhook_event = WebhookEvent {
            event_type,
            platform: PlatformType::Telegram,
            event_id: payload.update_id.to_string(),
            timestamp: chrono::Utc::now(),
            payload: serde_json::to_value(&payload).unwrap_or_default(),
            message,
            metadata,
        };

        events.push(webhook_event);
        Ok(events)
    }

    async fn handle_event(&self, event: WebhookEvent) -> Result<()> {
        match event.event_type {
            WebhookEventType::MessageReceived
            | WebhookEventType::MessageEdited
            | WebhookEventType::BotMentioned => {
                if let Some(msg) = &event.message {
                    info!(
                        "Received message from Telegram: {} (type: {:?})",
                        msg.content, msg.message_type
                    );

                    // P0 FIX: Removed dispatcher.dispatch() to avoid duplicate
                    // processing. Messages are now routed
                    // exclusively through channel_event_bus →
                    // MessageProcessor → AgentResolver path in webhook_handler.
                    // if let Some(dispatcher) = &self.dispatcher {
                    //     // For Telegram, use bot token prefix as
                    // platform_user_id to support multi-bot.
                    //     let platform_user_id =
                    // self.telegram_config.bot_token.split(':').next()
                    //         .unwrap_or("telegram_default")
                    //         .to_string();
                    //     let target_channel_id = msg.metadata.get("chat_id")
                    //         .cloned()
                    //         .unwrap_or_default();
                    //
                    //     dispatcher.dispatch(
                    //         PlatformType::Telegram,
                    //         &platform_user_id,
                    //         msg.clone(),
                    //         target_channel_id,
                    //     ).await?;
                    // }
                }
            }
            _ => debug!("Received unhandled event type: {:?}", event.event_type),
        }

        Ok(())
    }

    fn get_config(&self) -> &WebhookConfig {
        &self.config
    }
}

/// Helper function to parse Telegram message type
fn parse_message_type_from_telegram(message: &TelegramMessage) -> MessageType {
    if message.text.is_some() {
        MessageType::Text
    } else if message.photo.is_some() {
        MessageType::Image
    } else if message.document.is_some() {
        MessageType::File
    } else if message.audio.is_some() || message.voice.is_some() {
        MessageType::Voice
    } else if message.video.is_some() || message.video_note.is_some() {
        MessageType::Video
    } else if message.sticker.is_some() {
        MessageType::Image
    } else if message.location.is_some() || message.contact.is_some() {
        MessageType::Text
    } else {
        MessageType::System
    }
}

/// Telegram webhook response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramWebhookResponse {
    pub method: String,
    #[serde(rename = "chat_id")]
    pub chat_id: Option<i64>,
    pub text: Option<String>,
    #[serde(rename = "parse_mode")]
    pub parse_mode: Option<String>,
    #[serde(rename = "reply_to_message_id")]
    pub reply_to_message_id: Option<i64>,
}

impl TelegramWebhookResponse {
    /// Create a text message response
    pub fn message(chat_id: i64, text: impl Into<String>) -> Self {
        Self {
            method: "sendMessage".to_string(),
            chat_id: Some(chat_id),
            text: Some(text.into()),
            parse_mode: None,
            reply_to_message_id: None,
        }
    }

    /// Create a text message response with HTML parsing
    pub fn message_html(chat_id: i64, text: impl Into<String>) -> Self {
        Self {
            method: "sendMessage".to_string(),
            chat_id: Some(chat_id),
            text: Some(text.into()),
            parse_mode: Some("HTML".to_string()),
            reply_to_message_id: None,
        }
    }

    /// Create a reply to a message
    pub fn reply(chat_id: i64, text: impl Into<String>, reply_to_message_id: i64) -> Self {
        Self {
            method: "sendMessage".to_string(),
            chat_id: Some(chat_id),
            text: Some(text.into()),
            parse_mode: None,
            reply_to_message_id: Some(reply_to_message_id),
        }
    }

    /// Create an answer callback query response
    pub fn answer_callback_query(
        callback_query_id: impl Into<String>,
        text: impl Into<String>,
    ) -> serde_json::Value {
        serde_json::json!({
            "method": "answerCallbackQuery",
            "callback_query_id": callback_query_id.into(),
            "text": text.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::communication::channel::telegram_channel::{TelegramChat, TelegramPhotoSize};

    #[test]
    fn test_parse_message_type() {
        let handler = TelegramWebhookHandler::new(TelegramWebhookConfig::default());

        let text_message = TelegramMessage {
            message_id: 1,
            from: None,
            date: 1234567890,
            chat: TelegramChat {
                id: 123,
                chat_type: "private".to_string(),
                title: None,
                username: None,
                first_name: None,
                last_name: None,
            },
            text: Some("Hello".to_string()),
            photo: None,
            document: None,
            audio: None,
            voice: None,
            video: None,
            video_note: None,
            sticker: None,
            location: None,
            contact: None,
            caption: None,
            ..Default::default()
        };

        assert_eq!(handler.parse_message_type(&text_message), MessageType::Text);

        let photo_message = TelegramMessage {
            text: None, // Clear text to ensure photo is detected
            photo: Some(vec![TelegramPhotoSize {
                file_id: "photo123".to_string(),
                file_unique_id: "unique123".to_string(),
                width: 100,
                height: 100,
                file_size: Some(1024),
            }]),
            ..text_message.clone()
        };

        assert_eq!(
            handler.parse_message_type(&photo_message),
            MessageType::Image
        );
    }

    #[test]
    fn test_extract_message_content() {
        let handler = TelegramWebhookHandler::new(TelegramWebhookConfig::default());

        let message = TelegramMessage {
            message_id: 1,
            from: None,
            date: 1234567890,
            chat: TelegramChat {
                id: 123,
                chat_type: "private".to_string(),
                title: None,
                username: None,
                first_name: None,
                last_name: None,
            },
            text: Some("Hello World".to_string()),
            photo: None,
            document: None,
            audio: None,
            voice: None,
            video: None,
            video_note: None,
            sticker: None,
            location: None,
            contact: None,
            caption: None,
            ..Default::default()
        };

        assert_eq!(handler.extract_message_content(&message), "Hello World");
    }

    #[test]
    fn test_telegram_webhook_response() {
        let response = TelegramWebhookResponse::message(123456, "Hello");
        assert_eq!(response.method, "sendMessage");
        assert_eq!(response.chat_id, Some(123456));
        assert_eq!(response.text, Some("Hello".to_string()));

        let response = TelegramWebhookResponse::message_html(123456, "<b>Hello</b>");
        assert_eq!(response.parse_mode, Some("HTML".to_string()));
    }

    #[test]
    fn test_answer_callback_query() {
        let response = TelegramWebhookResponse::answer_callback_query("query123", "Answer");
        assert_eq!(response["method"], "answerCallbackQuery");
        assert_eq!(response["callback_query_id"], "query123");
        assert_eq!(response["text"], "Answer");
    }

    #[test]
    fn test_metadata_builder_with_telegram() {
        let metadata = MetadataBuilder::new()
            .add("update_id", "12345")
            .add("chat_id", "67890")
            .add_optional("username", Some("testuser"))
            .build();

        assert_eq!(metadata.get("update_id"), Some(&"12345".to_string()));
        assert_eq!(metadata.get("chat_id"), Some(&"67890".to_string()));
        assert_eq!(metadata.get("username"), Some(&"testuser".to_string()));
    }
}
