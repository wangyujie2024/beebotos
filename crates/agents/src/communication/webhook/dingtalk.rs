//! DingTalk (钉钉) Webhook Handler
//!
//! Handles incoming webhooks from DingTalk messaging platform.
//! Supports signature verification (HMAC-SHA256) and message decryption.

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use super::common::{compute_hmac_sha256, MetadataBuilder};
use crate::communication::webhook::{
    SignatureVerification, WebhookConfig, WebhookEvent, WebhookEventType, WebhookHandler,
};
use crate::communication::{AgentMessageDispatcher, Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

/// DingTalk webhook payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DingTalkWebhookPayload {
    /// Event type (for stream mode)
    #[serde(rename = "EventType")]
    pub event_type: Option<String>,
    /// Timestamp for signature verification
    #[serde(rename = "TimeStamp")]
    pub timestamp: Option<String>,
    /// Signature
    #[serde(rename = "Sign")]
    pub sign: Option<String>,
    /// Chatbot webhook payload
    #[serde(rename = "conversationType")]
    pub conversation_type: Option<String>,
    #[serde(rename = "chatbotCorpId")]
    pub chatbot_corp_id: Option<String>,
    #[serde(rename = "chatbotUserId")]
    pub chatbot_user_id: Option<String>,
    /// Sender info
    #[serde(rename = "senderStaffId")]
    pub sender_staff_id: Option<String>,
    #[serde(rename = "senderNick")]
    pub sender_nick: Option<String>,
    #[serde(rename = "senderCorpId")]
    pub sender_corp_id: Option<String>,
    #[serde(rename = "senderUserId")]
    pub sender_user_id: Option<String>,
    /// Session webhook
    #[serde(rename = "sessionWebhook")]
    pub session_webhook: Option<String>,
    #[serde(rename = "sessionWebhookExpiredTime")]
    pub session_webhook_expired_time: Option<i64>,
    /// Message info
    #[serde(rename = "createAt")]
    pub create_at: Option<i64>,
    #[serde(rename = "msgtype")]
    pub msg_type: Option<String>,
    #[serde(rename = "content")]
    pub content: Option<String>,
    #[serde(rename = "text")]
    pub text: Option<DingTalkTextContent>,
    #[serde(rename = "markdown")]
    pub markdown: Option<DingTalkMarkdownContent>,
    #[serde(rename = "actionCard")]
    pub action_card: Option<DingTalkActionCardContent>,
    #[serde(rename = "image")]
    pub image: Option<DingTalkImageContent>,
    #[serde(rename = "voice")]
    pub voice: Option<DingTalkVoiceContent>,
    #[serde(rename = "file")]
    pub file: Option<DingTalkFileContent>,
    #[serde(rename = "link")]
    pub link: Option<DingTalkLinkContent>,
    /// Encrypted data
    #[serde(rename = "encrypt")]
    pub encrypt: Option<String>,
    /// Additional data
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// DingTalk text content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DingTalkTextContent {
    pub content: String,
}

/// DingTalk markdown content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DingTalkMarkdownContent {
    pub title: String,
    pub text: String,
}

/// DingTalk action card content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DingTalkActionCardContent {
    pub title: String,
    #[serde(rename = "markdown")]
    pub content: String,
    #[serde(rename = "singleTitle")]
    pub single_title: Option<String>,
    #[serde(rename = "singleURL")]
    pub single_url: Option<String>,
    #[serde(rename = "btnOrientation")]
    pub btn_orientation: Option<String>,
    #[serde(rename = "btns")]
    pub buttons: Option<Vec<DingTalkActionCardButton>>,
}

/// DingTalk action card button
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DingTalkActionCardButton {
    pub title: String,
    #[serde(rename = "actionURL")]
    pub action_url: String,
}

/// DingTalk image content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DingTalkImageContent {
    #[serde(rename = "picUrl")]
    pub pic_url: Option<String>,
    #[serde(rename = "downloadCode")]
    pub download_code: Option<String>,
}

/// DingTalk voice content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DingTalkVoiceContent {
    #[serde(rename = "downloadCode")]
    pub download_code: String,
    pub duration: Option<i32>,
    #[serde(rename = "recognition")]
    pub recognition: Option<String>,
}

/// DingTalk file content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DingTalkFileContent {
    #[serde(rename = "downloadCode")]
    pub download_code: String,
    #[serde(rename = "fileName")]
    pub file_name: String,
}

/// DingTalk link content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DingTalkLinkContent {
    pub title: String,
    pub text: String,
    #[serde(rename = "picUrl")]
    pub pic_url: Option<String>,
    #[serde(rename = "messageUrl")]
    pub message_url: String,
}

/// DingTalk webhook handler
#[allow(dead_code)]
pub struct DingTalkWebhookHandler {
    config: WebhookConfig,
    app_secret: String,
    app_key: String,
    encrypt_key: Option<String>,
    dispatcher: Option<std::sync::Arc<AgentMessageDispatcher>>,
}

impl DingTalkWebhookHandler {
    /// Create a new DingTalk webhook handler
    ///
    /// # Arguments
    /// * `app_key` - DingTalk app key
    /// * `app_secret` - DingTalk app secret for signature verification
    /// * `encrypt_key` - Optional encryption key for decrypting messages
    pub fn new(app_key: String, app_secret: String, encrypt_key: Option<String>) -> Self {
        let mut config = WebhookConfig::default();
        config.platform = PlatformType::DingTalk;
        config.endpoint_path = "/webhook/dingtalk".to_string();
        config.verify_signatures = true;
        config.decrypt_messages = encrypt_key.is_some();

        Self {
            config,
            app_key,
            app_secret,
            encrypt_key,
            dispatcher: None,
        }
    }

    pub fn with_dispatcher(mut self, dispatcher: std::sync::Arc<AgentMessageDispatcher>) -> Self {
        self.dispatcher = Some(dispatcher);
        self
    }

    /// Create handler from environment variables
    pub fn from_env() -> Result<Self> {
        let app_key = std::env::var("DINGTALK_APP_KEY")
            .map_err(|_| AgentError::configuration("DINGTALK_APP_KEY not set"))?;
        let app_secret = std::env::var("DINGTALK_APP_SECRET")
            .map_err(|_| AgentError::configuration("DINGTALK_APP_SECRET not set"))?;
        let encrypt_key = std::env::var("DINGTALK_ENCRYPT_KEY").ok();

        Ok(Self::new(app_key, app_secret, encrypt_key))
    }

    /// Compute DingTalk signature
    ///
    /// DingTalk uses HMAC-SHA256 with format: BASE64(HMAC-SHA256(timestamp +
    /// "\n" + app_secret))
    fn compute_signature(&self, timestamp: &str) -> String {
        use base64::Engine;
        let message = format!("{}\n{}", timestamp, self.app_secret);
        let computed = compute_hmac_sha256(&self.app_secret, message.as_bytes(), "");
        base64::engine::general_purpose::STANDARD.encode(computed.as_bytes())
    }

    /// Verify DingTalk signature
    fn verify_signature(&self, timestamp: &str, signature: &str) -> bool {
        let computed = self.compute_signature(timestamp);
        computed == signature
    }

    /// Parse DingTalk event type
    fn parse_event_type(&self, event_type: &str) -> WebhookEventType {
        match event_type {
            "chat_update_title" => WebhookEventType::MessageReceived,
            "chat_disband" => WebhookEventType::UserLeft,
            "chat_add_member" => WebhookEventType::UserJoined,
            "chat_remove_member" => WebhookEventType::UserLeft,
            "chat_quit" => WebhookEventType::UserLeft,
            "chat_change_owner" => WebhookEventType::System,
            _ => {
                if event_type.contains("message") {
                    WebhookEventType::MessageReceived
                } else {
                    WebhookEventType::Unknown
                }
            }
        }
    }

    /// Parse message type from DingTalk message type
    fn parse_message_type(&self, msg_type: &str) -> MessageType {
        match msg_type {
            "text" => MessageType::Text,
            "markdown" => MessageType::Text,
            "image" => MessageType::Image,
            "voice" => MessageType::Voice,
            "file" => MessageType::File,
            "link" => MessageType::Text,
            "action_card" => MessageType::Text,
            "rich" => MessageType::Text,
            _ => MessageType::System,
        }
    }

    /// Extract content from payload based on message type
    fn extract_content(&self, payload: &DingTalkWebhookPayload) -> (String, String) {
        let msg_type = payload
            .msg_type
            .clone()
            .unwrap_or_else(|| "text".to_string());

        let content = match msg_type.as_str() {
            "text" => payload
                .text
                .as_ref()
                .map(|t| t.content.clone())
                .unwrap_or_default(),
            "markdown" => payload
                .markdown
                .as_ref()
                .map(|m| format!("{}\n{}", m.title, m.text))
                .unwrap_or_default(),
            "image" => payload
                .image
                .as_ref()
                .map(|i| {
                    if let Some(url) = &i.pic_url {
                        format!("[Image] {}", url)
                    } else if let Some(code) = &i.download_code {
                        format!("[Image with download code: {}]", code)
                    } else {
                        "[Image]".to_string()
                    }
                })
                .unwrap_or_else(|| "[Image]".to_string()),
            "voice" => payload
                .voice
                .as_ref()
                .map(|v| {
                    let recognition = v.recognition.as_deref().unwrap_or("");
                    format!("[Voice: {}]", recognition)
                })
                .unwrap_or_else(|| "[Voice]".to_string()),
            "file" => payload
                .file
                .as_ref()
                .map(|f| format!("[File: {}]", f.file_name))
                .unwrap_or_else(|| "[File]".to_string()),
            "link" => payload
                .link
                .as_ref()
                .map(|l| format!("[Link: {} - {}]", l.title, l.message_url))
                .unwrap_or_else(|| "[Link]".to_string()),
            "action_card" => payload
                .action_card
                .as_ref()
                .map(|a| format!("[Action Card: {}]\n{}", a.title, a.content))
                .unwrap_or_else(|| "[Action Card]".to_string()),
            _ => payload.content.clone().unwrap_or_default(),
        };

        (msg_type, content)
    }

    /// Convert DingTalk payload to internal message
    fn convert_to_message(&self, payload: &DingTalkWebhookPayload) -> Option<Message> {
        let sender_id = payload
            .sender_user_id
            .clone()
            .or_else(|| payload.sender_staff_id.clone())?;
        let sender_nick = payload
            .sender_nick
            .clone()
            .unwrap_or_else(|| "Unknown".to_string());

        let (msg_type, content) = self.extract_content(payload);

        let mut metadata = MetadataBuilder::new()
            .add("sender_id", sender_id.clone())
            .add("sender_nick", sender_nick)
            .add("message_type", msg_type.clone());

        if let Some(corp_id) = &payload.sender_corp_id {
            metadata = metadata.add("corp_id", corp_id.clone());
        }
        if let Some(webhook) = &payload.session_webhook {
            metadata = metadata.add("session_webhook", webhook.clone());
        }
        if let Some(conversation_type) = &payload.conversation_type {
            metadata = metadata.add("conversation_type", conversation_type.clone());
        }

        // Store download codes for media files
        match msg_type.as_str() {
            "image" => {
                if let Some(image) = &payload.image {
                    if let Some(code) = &image.download_code {
                        metadata = metadata.add("download_code", code.clone());
                    }
                    if let Some(url) = &image.pic_url {
                        metadata = metadata.add("pic_url", url.clone());
                    }
                }
            }
            "voice" => {
                if let Some(voice) = &payload.voice {
                    metadata = metadata.add("download_code", voice.download_code.clone());
                    if let Some(duration) = voice.duration {
                        metadata = metadata.add("duration", duration.to_string());
                    }
                }
            }
            "file" => {
                if let Some(file) = &payload.file {
                    metadata = metadata.add("download_code", file.download_code.clone());
                    metadata = metadata.add("file_name", file.file_name.clone());
                }
            }
            _ => {}
        }

        let metadata = metadata.build();

        Some(Message {
            id: uuid::Uuid::new_v4(),
            thread_id: uuid::Uuid::new_v4(),
            platform: PlatformType::DingTalk,
            message_type: self.parse_message_type(&msg_type),
            content,
            metadata,
            timestamp: payload
                .create_at
                .map(|t| chrono::DateTime::from_timestamp(t / 1000, 0))
                .flatten()
                .unwrap_or_else(chrono::Utc::now),
        })
    }

    /// Get session webhook from payload
    pub fn get_session_webhook(&self, payload: &DingTalkWebhookPayload) -> Option<String> {
        payload.session_webhook.clone()
    }

    /// Get sender info from payload
    pub fn get_sender_info(&self, payload: &DingTalkWebhookPayload) -> DingTalkSenderInfo {
        DingTalkSenderInfo {
            user_id: payload
                .sender_user_id
                .clone()
                .or_else(|| payload.sender_staff_id.clone()),
            nick: payload.sender_nick.clone(),
            corp_id: payload.sender_corp_id.clone(),
        }
    }
}

/// DingTalk sender information
#[derive(Debug, Clone)]
pub struct DingTalkSenderInfo {
    pub user_id: Option<String>,
    pub nick: Option<String>,
    pub corp_id: Option<String>,
}

#[async_trait]
impl WebhookHandler for DingTalkWebhookHandler {
    fn platform_type(&self) -> PlatformType {
        PlatformType::DingTalk
    }

    async fn verify_signature(
        &self,
        _body: &[u8],
        signature: Option<&str>,
        timestamp: Option<&str>,
    ) -> Result<SignatureVerification> {
        // DingTalk signature verification requires timestamp from header
        let timestamp = match timestamp {
            Some(t) => t,
            None => {
                warn!("No timestamp provided for DingTalk signature verification");
                return Ok(SignatureVerification::Skipped);
            }
        };

        let signature = match signature {
            Some(s) => s,
            None => {
                warn!("No signature provided for DingTalk signature verification");
                return Ok(SignatureVerification::Skipped);
            }
        };

        if self.verify_signature(timestamp, signature) {
            debug!("DingTalk signature verified successfully");
            Ok(SignatureVerification::Valid)
        } else {
            error!("DingTalk signature verification failed");
            Ok(SignatureVerification::Invalid)
        }
    }

    async fn parse_payload(&self, body: &[u8]) -> Result<Vec<WebhookEvent>> {
        let payload: DingTalkWebhookPayload = serde_json::from_slice(body).map_err(|e| {
            AgentError::platform(format!("Failed to parse DingTalk payload: {}", e))
        })?;

        debug!("Parsed DingTalk payload: {:?}", payload);

        // Handle encrypted payload if needed
        if let Some(_encrypt) = &payload.encrypt {
            if self.config.decrypt_messages {
                // TODO: Implement decryption if needed
                warn!("Encrypted payload received but decryption not yet implemented");
            } else {
                return Err(AgentError::platform(
                    "Received encrypted payload but decryption is not configured",
                ));
            }
        }

        // Determine event type
        let event_type = payload
            .event_type
            .clone()
            .or_else(|| payload.msg_type.clone())
            .unwrap_or_else(|| "message".to_string());

        let webhook_event_type = self.parse_event_type(&event_type);
        let message = self.convert_to_message(&payload);

        // Build metadata
        let metadata = MetadataBuilder::new()
            .add_optional("session_webhook", payload.session_webhook.as_ref())
            .add_optional(
                "webhook_expired_time",
                payload.session_webhook_expired_time.map(|t| t.to_string()),
            )
            .add_optional("chatbot_user_id", payload.chatbot_user_id.as_ref())
            .build();

        let webhook_event = WebhookEvent {
            event_type: webhook_event_type,
            platform: PlatformType::DingTalk,
            event_id: payload
                .create_at
                .map(|t| t.to_string())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            timestamp: payload
                .create_at
                .map(|t| chrono::DateTime::from_timestamp(t / 1000, 0))
                .flatten()
                .unwrap_or_else(chrono::Utc::now),
            payload: serde_json::to_value(&payload).unwrap_or_default(),
            message,
            metadata,
        };

        Ok(vec![webhook_event])
    }

    async fn handle_event(&self, event: WebhookEvent) -> Result<()> {
        match event.event_type {
            WebhookEventType::MessageReceived => {
                if let Some(msg) = &event.message {
                    info!(
                        "Received message from DingTalk: {} (type: {:?})",
                        msg.content, msg.message_type
                    );

                    // P0 FIX: Removed dispatcher.dispatch() to avoid duplicate
                    // processing. Messages are now routed
                    // exclusively through channel_event_bus →
                    // MessageProcessor → AgentResolver path in webhook_handler.
                    // if let Some(dispatcher) = &self.dispatcher {
                    //     let platform_user_id = event.metadata.get("corp_id")
                    //         .or_else(||
                    // event.metadata.get("chatbot_corp_id"))
                    //         .cloned()
                    //         .unwrap_or_default();
                    //     let target_channel_id = msg.metadata.get("sender_id")
                    //         .cloned()
                    //         .unwrap_or_default();
                    //
                    //     dispatcher.dispatch(
                    //         PlatformType::DingTalk,
                    //         &platform_user_id,
                    //         msg.clone(),
                    //         target_channel_id,
                    //     ).await?;
                    // }
                }
            }
            WebhookEventType::UserJoined => {
                info!("User joined DingTalk chat");
            }
            WebhookEventType::UserLeft => {
                info!("User left DingTalk chat");
            }
            WebhookEventType::System => {
                debug!("Received system event from DingTalk");
            }
            _ => {
                debug!(
                    "Received unhandled event type from DingTalk: {:?}",
                    event.event_type
                );
            }
        }

        Ok(())
    }

    fn get_config(&self) -> &WebhookConfig {
        &self.config
    }
}

/// DingTalk webhook response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DingTalkWebhookResponse {
    /// Response message (for text response)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msgtype: Option<String>,
    /// Text content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<DingTalkTextContent>,
    /// Markdown content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markdown: Option<DingTalkMarkdownContent>,
    /// Action card content
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "actionCard")]
    pub action_card: Option<DingTalkActionCardContent>,
}

impl DingTalkWebhookResponse {
    /// Create a text response
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            msgtype: Some("text".to_string()),
            text: Some(DingTalkTextContent {
                content: content.into(),
            }),
            markdown: None,
            action_card: None,
        }
    }

    /// Create a markdown response
    pub fn markdown(title: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            msgtype: Some("markdown".to_string()),
            text: None,
            markdown: Some(DingTalkMarkdownContent {
                title: title.into(),
                text: text.into(),
            }),
            action_card: None,
        }
    }

    /// Create an action card response
    pub fn action_card(
        title: impl Into<String>,
        markdown: impl Into<String>,
        single_title: impl Into<String>,
        single_url: impl Into<String>,
    ) -> Self {
        Self {
            msgtype: Some("action_card".to_string()),
            text: None,
            markdown: None,
            action_card: Some(DingTalkActionCardContent {
                title: title.into(),
                content: markdown.into(),
                single_title: Some(single_title.into()),
                single_url: Some(single_url.into()),
                btn_orientation: None,
                buttons: None,
            }),
        }
    }

    /// Create an empty success response
    pub fn success() -> Self {
        Self {
            msgtype: None,
            text: None,
            markdown: None,
            action_card: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_message_type() {
        let handler =
            DingTalkWebhookHandler::new("test_key".to_string(), "test_secret".to_string(), None);

        assert_eq!(handler.parse_message_type("text"), MessageType::Text);
        assert_eq!(handler.parse_message_type("image"), MessageType::Image);
        assert_eq!(handler.parse_message_type("voice"), MessageType::Voice);
        assert_eq!(handler.parse_message_type("file"), MessageType::File);
        assert_eq!(handler.parse_message_type("unknown"), MessageType::System);
    }

    #[test]
    fn test_parse_event_type() {
        let handler =
            DingTalkWebhookHandler::new("test_key".to_string(), "test_secret".to_string(), None);

        assert_eq!(
            handler.parse_event_type("chat_add_member"),
            WebhookEventType::UserJoined
        );
        assert_eq!(
            handler.parse_event_type("chat_remove_member"),
            WebhookEventType::UserLeft
        );
        assert_eq!(
            handler.parse_event_type("unknown_event"),
            WebhookEventType::Unknown
        );
    }

    #[test]
    fn test_webhook_response() {
        let text_resp = DingTalkWebhookResponse::text("Hello");
        assert_eq!(text_resp.msgtype, Some("text".to_string()));
        assert!(text_resp.text.is_some());

        let markdown_resp = DingTalkWebhookResponse::markdown("Title", "Content");
        assert_eq!(markdown_resp.msgtype, Some("markdown".to_string()));
        assert!(markdown_resp.markdown.is_some());

        let action_card_resp = DingTalkWebhookResponse::action_card(
            "Title",
            "Markdown content",
            "Click me",
            "https://example.com",
        );
        assert_eq!(action_card_resp.msgtype, Some("action_card".to_string()));
        assert!(action_card_resp.action_card.is_some());
    }

    #[test]
    fn test_extract_content() {
        let handler =
            DingTalkWebhookHandler::new("test_key".to_string(), "test_secret".to_string(), None);

        let payload = DingTalkWebhookPayload {
            msg_type: Some("text".to_string()),
            text: Some(DingTalkTextContent {
                content: "Hello World".to_string(),
            }),
            ..Default::default()
        };

        let (msg_type, content) = handler.extract_content(&payload);
        assert_eq!(msg_type, "text");
        assert_eq!(content, "Hello World");
    }
}

// Default implementation for DingTalkWebhookPayload
impl Default for DingTalkWebhookPayload {
    fn default() -> Self {
        Self {
            event_type: None,
            timestamp: None,
            sign: None,
            conversation_type: None,
            chatbot_corp_id: None,
            chatbot_user_id: None,
            sender_staff_id: None,
            sender_nick: None,
            sender_corp_id: None,
            sender_user_id: None,
            session_webhook: None,
            session_webhook_expired_time: None,
            create_at: None,
            msg_type: None,
            content: None,
            text: None,
            markdown: None,
            action_card: None,
            image: None,
            voice: None,
            file: None,
            link: None,
            encrypt: None,
            extra: HashMap::new(),
        }
    }
}
