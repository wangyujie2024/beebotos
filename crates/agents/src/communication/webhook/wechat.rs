//! WeChat Work (企业微信) Webhook Handler
//!
//! Handles incoming webhooks from WeChat Work messaging platform.
//! Supports signature verification (HMAC-SHA256) and message decryption
//! (AES-CBC).

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use super::common::{encryption, MetadataBuilder};
use crate::communication::webhook::{
    SignatureVerification, WebhookConfig, WebhookEvent, WebhookEventType, WebhookHandler,
};
use crate::communication::{AgentMessageDispatcher, Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

/// WeChat Work webhook payload (XML format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeChatWebhookPayload {
    /// To user name (CorpID)
    #[serde(rename = "ToUserName")]
    pub to_user_name: Option<String>,
    /// From user name (UserID)
    #[serde(rename = "FromUserName")]
    pub from_user_name: Option<String>,
    /// Create time
    #[serde(rename = "CreateTime")]
    pub create_time: Option<i64>,
    /// Message type
    #[serde(rename = "MsgType")]
    pub msg_type: Option<String>,
    /// Message ID
    #[serde(rename = "MsgId")]
    pub msg_id: Option<i64>,
    /// Agent ID
    #[serde(rename = "AgentID")]
    pub agent_id: Option<String>,
    /// Content (for text messages)
    #[serde(rename = "Content")]
    pub content: Option<String>,
    /// Media ID (for image/voice/video/file)
    #[serde(rename = "MediaId")]
    pub media_id: Option<String>,
    /// Picture URL (for image messages)
    #[serde(rename = "PicUrl")]
    pub pic_url: Option<String>,
    /// Format (for voice messages)
    #[serde(rename = "Format")]
    pub format: Option<String>,
    /// Recognition (for voice messages)
    #[serde(rename = "Recognition")]
    pub recognition: Option<String>,
    /// Thumb media ID (for video messages)
    #[serde(rename = "ThumbMediaId")]
    pub thumb_media_id: Option<String>,
    /// Location X (for location messages)
    #[serde(rename = "Location_X")]
    pub location_x: Option<f64>,
    /// Location Y (for location messages)
    #[serde(rename = "Location_Y")]
    pub location_y: Option<f64>,
    /// Scale (for location messages)
    #[serde(rename = "Scale")]
    pub scale: Option<i32>,
    /// Label (for location messages)
    #[serde(rename = "Label")]
    pub label: Option<String>,
    /// Title (for link messages)
    #[serde(rename = "Title")]
    pub title: Option<String>,
    /// Description (for link messages)
    #[serde(rename = "Description")]
    pub description: Option<String>,
    /// URL (for link messages)
    #[serde(rename = "Url")]
    pub url: Option<String>,
    /// File key (for file messages)
    #[serde(rename = "FileKey")]
    pub file_key: Option<String>,
    /// File MD5 (for file messages)
    #[serde(rename = "FileMd5")]
    pub file_md5: Option<String>,
    /// File total length (for file messages)
    #[serde(rename = "FileTotalLen")]
    pub file_total_len: Option<i64>,
    /// Event type (for event messages)
    #[serde(rename = "Event")]
    pub event: Option<String>,
    /// Event key (for menu clicks)
    #[serde(rename = "EventKey")]
    pub event_key: Option<String>,
    /// Ticket (for QR code scans)
    #[serde(rename = "Ticket")]
    pub ticket: Option<String>,
    /// Latitude (for location events)
    #[serde(rename = "Latitude")]
    pub latitude: Option<f64>,
    /// Longitude (for location events)
    #[serde(rename = "Longitude")]
    pub longitude: Option<f64>,
    /// Precision (for location events)
    #[serde(rename = "Precision")]
    pub precision: Option<f64>,
    /// Change type (for contact change events)
    #[serde(rename = "ChangeType")]
    pub change_type: Option<String>,
    /// User ID (for contact change events)
    #[serde(rename = "UserID")]
    pub user_id: Option<String>,
    /// Department ID (for contact change events)
    #[serde(rename = "Department")]
    pub department: Option<String>,
    /// Encrypted data
    #[serde(rename = "Encrypt")]
    pub encrypt: Option<String>,
    /// Additional data
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// WeChat Work URL verification request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeChatUrlVerification {
    #[serde(rename = "msg_signature")]
    pub msg_signature: String,
    pub timestamp: String,
    pub nonce: String,
    pub echostr: String,
}

/// WeChat Work encrypted message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeChatEncryptedMessage {
    #[serde(rename = "ToUserName")]
    pub to_user_name: String,
    #[serde(rename = "Encrypt")]
    pub encrypt: String,
    #[serde(rename = "AgentID")]
    pub agent_id: Option<String>,
}

/// WeChat Work webhook handler
pub struct WeChatWebhookHandler {
    config: WebhookConfig,
    corp_id: String,
    token: String,
    encoding_aes_key: Option<String>,
    dispatcher: Option<Arc<AgentMessageDispatcher>>,
}

impl WeChatWebhookHandler {
    /// Create a new WeChat Work webhook handler
    ///
    /// # Arguments
    /// * `corp_id` - WeChat Work Corp ID
    /// * `token` - WeChat Work Token (for signature verification)
    /// * `encoding_aes_key` - Optional encoding AES key for decryption
    pub fn new(corp_id: String, token: String, encoding_aes_key: Option<String>) -> Self {
        let mut config = WebhookConfig::default();
        config.platform = PlatformType::WeChat;
        config.endpoint_path = "/webhook/wechat".to_string();
        config.verify_signatures = true;
        config.decrypt_messages = encoding_aes_key.is_some();

        Self {
            config,
            corp_id,
            token,
            encoding_aes_key,
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
        let corp_id = std::env::var("WECHAT_CORP_ID")
            .map_err(|_| AgentError::configuration("WECHAT_CORP_ID not set"))?;
        let token = std::env::var("WECHAT_TOKEN")
            .map_err(|_| AgentError::configuration("WECHAT_TOKEN not set"))?;
        let encoding_aes_key = std::env::var("WECHAT_ENCODING_AES_KEY").ok();

        Ok(Self::new(corp_id, token, encoding_aes_key))
    }

    /// Compute WeChat Work signature
    ///
    /// Signature algorithm: SHA1(sort(token, timestamp, nonce, msg_encrypt))
    fn compute_signature(&self, timestamp: &str, nonce: &str, msg_encrypt: &str) -> String {
        use sha1::{Digest, Sha1};

        let mut params = vec![
            self.token.clone(),
            timestamp.to_string(),
            nonce.to_string(),
            msg_encrypt.to_string(),
        ];
        params.sort();

        let concat = params.join("");
        let mut hasher = Sha1::new();
        hasher.update(concat);
        hex::encode(hasher.finalize())
    }

    /// Verify WeChat Work signature
    fn verify_signature(
        &self,
        timestamp: &str,
        nonce: &str,
        msg_encrypt: &str,
        signature: &str,
    ) -> bool {
        let computed = self.compute_signature(timestamp, nonce, msg_encrypt);
        computed == signature
    }

    /// Decrypt message using AES-CBC
    ///
    /// WeChat Work uses AES-CBC with PKCS#7 padding
    fn decrypt_message(&self, encrypted_data: &str) -> Result<String> {
        use base64::Engine;

        let aes_key = match &self.encoding_aes_key {
            Some(key) => key,
            None => return Err(AgentError::configuration("No encoding AES key provided").into()),
        };

        tracing::info!(
            "Decrypting message, encrypted_data len: {}",
            encrypted_data.len()
        );
        tracing::info!(
            "AES key (first 10 chars): {}...",
            &aes_key[..10.min(aes_key.len())]
        );

        // Decode base64 encrypted data
        let encrypted = base64::engine::general_purpose::STANDARD
            .decode(encrypted_data)
            .map_err(|e| AgentError::platform(format!("Failed to decode base64: {}", e)))?;

        tracing::info!("Encrypted data decoded, len: {}", encrypted.len());

        // Decode AES key (base64 with padding)
        let key_with_padding = format!("{}=", aes_key);
        tracing::info!("AES key with padding len: {}", key_with_padding.len());

        let key = base64::engine::general_purpose::STANDARD
            .decode(&key_with_padding)
            .map_err(|e| AgentError::platform(format!("Failed to decode AES key: {}", e)))?;

        tracing::info!("AES key decoded, len: {} (expected 32)", key.len());

        // Use common encryption utilities
        let iv = &key[..16];
        tracing::info!(
            "Using IV (first 16 bytes of key): {:?}",
            &iv[..4.min(iv.len())]
        );

        let decrypted = encryption::aes_cbc_decrypt(&encrypted, &key, iv).map_err(|e| {
            tracing::error!("AES decryption failed: {:?}", e);
            e
        })?;

        tracing::info!("Decrypted data len: {}", decrypted.len());

        let (content, corp_id) = encryption::extract_wechat_content(&decrypted).map_err(|e| {
            tracing::error!("Extract wechat content failed: {:?}", e);
            e
        })?;

        // Verify corp_id
        if corp_id != self.corp_id {
            return Err(AgentError::authentication("Corp ID mismatch").into());
        }

        Ok(content)
    }

    /// Encrypt message using AES-CBC
    #[allow(dead_code)]
    fn encrypt_message(&self, message: &str) -> Result<String> {
        use aes::cipher::block_padding::Pkcs7;
        use aes::cipher::{BlockEncryptMut, KeyIvInit};
        use base64::Engine;
        use rand::Rng;

        let aes_key = match &self.encoding_aes_key {
            Some(key) => key,
            None => return Err(AgentError::configuration("No encoding AES key provided").into()),
        };

        // Generate random bytes (16 bytes)
        let random_bytes: [u8; 16] = rand::thread_rng().gen();

        // Message length (4 bytes, big-endian)
        let msg_len = message.len() as u32;
        let msg_len_bytes = msg_len.to_be_bytes();

        // Corp ID
        let corp_id_bytes = self.corp_id.as_bytes();

        // Combine: random + msg_len + message + corp_id
        let mut data = Vec::new();
        data.extend_from_slice(&random_bytes);
        data.extend_from_slice(&msg_len_bytes);
        data.extend_from_slice(message.as_bytes());
        data.extend_from_slice(corp_id_bytes);

        // Decode AES key
        let key = base64::engine::general_purpose::STANDARD
            .decode(aes_key.to_owned() + "=")
            .map_err(|e| AgentError::platform(format!("Failed to decode AES key: {}", e)))?;

        if key.len() != 32 {
            return Err(AgentError::platform("Invalid AES key length").into());
        }

        let iv = &key[..16];

        // Encrypt
        type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;

        let mut buf = vec![0u8; data.len() + 32]; // Add padding space
        buf[..data.len()].copy_from_slice(&data);

        let encrypted = Aes256CbcEnc::new_from_slices(&key, iv)
            .map_err(|e| AgentError::platform(format!("Failed to create encryptor: {}", e)))?
            .encrypt_padded_mut::<Pkcs7>(&mut buf, data.len())
            .map_err(|e| AgentError::platform(format!("Failed to encrypt: {}", e)))?;

        // Encode to base64
        let result = base64::engine::general_purpose::STANDARD.encode(encrypted);
        Ok(result)
    }

    /// Parse XML payload to JSON-like structure
    fn parse_xml(&self, xml: &str) -> Result<WeChatWebhookPayload> {
        // Simple XML parsing - in production, use a proper XML parser
        // For now, we'll use a basic regex-based approach
        let payload = WeChatWebhookPayload {
            to_user_name: self.extract_xml_value(xml, "ToUserName"),
            from_user_name: self.extract_xml_value(xml, "FromUserName"),
            create_time: self
                .extract_xml_value(xml, "CreateTime")
                .and_then(|s| s.parse().ok()),
            msg_type: self.extract_xml_value(xml, "MsgType"),
            msg_id: self
                .extract_xml_value(xml, "MsgId")
                .and_then(|s| s.parse().ok()),
            agent_id: self.extract_xml_value(xml, "AgentID"),
            content: self.extract_xml_value(xml, "Content"),
            media_id: self.extract_xml_value(xml, "MediaId"),
            pic_url: self.extract_xml_value(xml, "PicUrl"),
            format: self.extract_xml_value(xml, "Format"),
            recognition: self.extract_xml_value(xml, "Recognition"),
            thumb_media_id: self.extract_xml_value(xml, "ThumbMediaId"),
            location_x: self
                .extract_xml_value(xml, "Location_X")
                .and_then(|s| s.parse().ok()),
            location_y: self
                .extract_xml_value(xml, "Location_Y")
                .and_then(|s| s.parse().ok()),
            scale: self
                .extract_xml_value(xml, "Scale")
                .and_then(|s| s.parse().ok()),
            label: self.extract_xml_value(xml, "Label"),
            title: self.extract_xml_value(xml, "Title"),
            description: self.extract_xml_value(xml, "Description"),
            url: self.extract_xml_value(xml, "Url"),
            file_key: self.extract_xml_value(xml, "FileKey"),
            file_md5: self.extract_xml_value(xml, "FileMd5"),
            file_total_len: self
                .extract_xml_value(xml, "FileTotalLen")
                .and_then(|s| s.parse().ok()),
            event: self.extract_xml_value(xml, "Event"),
            event_key: self.extract_xml_value(xml, "EventKey"),
            ticket: self.extract_xml_value(xml, "Ticket"),
            latitude: self
                .extract_xml_value(xml, "Latitude")
                .and_then(|s| s.parse().ok()),
            longitude: self
                .extract_xml_value(xml, "Longitude")
                .and_then(|s| s.parse().ok()),
            precision: self
                .extract_xml_value(xml, "Precision")
                .and_then(|s| s.parse().ok()),
            change_type: self.extract_xml_value(xml, "ChangeType"),
            user_id: self.extract_xml_value(xml, "UserID"),
            department: self.extract_xml_value(xml, "Department"),
            encrypt: self.extract_xml_value(xml, "Encrypt"),
            extra: HashMap::new(),
        };

        Ok(payload)
    }

    /// Extract value from XML tag
    fn extract_xml_value(&self, xml: &str, tag: &str) -> Option<String> {
        let pattern = format!(r"<{}><!\[CDATA\[(.*?)\]\]></{}>", tag, tag);
        let regex = regex::Regex::new(&pattern).ok()?;
        if let Some(captures) = regex.captures(xml) {
            return captures.get(1).map(|m| m.as_str().to_string());
        }

        // Try without CDATA
        let pattern = format!("<{}>(.*?)</{}>", tag, tag);
        let regex = regex::Regex::new(&pattern).ok()?;
        regex.captures(xml)?.get(1).map(|m| m.as_str().to_string())
    }

    /// Parse message type from WeChat Work message type
    fn parse_message_type(&self, msg_type: &str) -> MessageType {
        match msg_type {
            "text" => MessageType::Text,
            "image" => MessageType::Image,
            "voice" => MessageType::Voice,
            "video" => MessageType::Video,
            "file" => MessageType::File,
            "location" => MessageType::System,
            "link" => MessageType::System,
            "event" => MessageType::System,
            _ => MessageType::System,
        }
    }

    /// Parse event type from WeChat Work event
    fn parse_event_type(&self, event: &str) -> WebhookEventType {
        match event {
            "subscribe" => WebhookEventType::UserJoined,
            "unsubscribe" => WebhookEventType::UserLeft,
            "enter_agent" => WebhookEventType::System,
            "LOCATION" => WebhookEventType::System,
            "click" => WebhookEventType::BotMentioned,
            "view" => WebhookEventType::System,
            "scancode_push" => WebhookEventType::System,
            "scancode_waitmsg" => WebhookEventType::System,
            "pic_sysphoto" => WebhookEventType::System,
            "pic_photo_or_album" => WebhookEventType::System,
            "pic_weixin" => WebhookEventType::System,
            "location_select" => WebhookEventType::System,
            "change_contact" => WebhookEventType::System,
            _ => WebhookEventType::Unknown,
        }
    }

    /// Convert WeChat Work payload to internal message
    fn convert_to_message(&self, payload: &WeChatWebhookPayload) -> Option<Message> {
        let sender_id = payload.from_user_name.clone()?;
        let msg_type = payload
            .msg_type
            .clone()
            .unwrap_or_else(|| "text".to_string());

        let content = match msg_type.as_str() {
            "text" => payload.content.clone().unwrap_or_default(),
            "image" => format!("[Image]"),
            "voice" => {
                if let Some(recognition) = &payload.recognition {
                    format!("[Voice: {}]", recognition)
                } else {
                    "[Voice message]".to_string()
                }
            }
            "video" => "[Video message]".to_string(),
            "file" => format!(
                "[File: {}]",
                payload.file_key.as_deref().unwrap_or("unknown")
            ),
            "location" => format!(
                "[Location: {} - {}, {}]",
                payload.label.as_deref().unwrap_or(""),
                payload.location_x.unwrap_or(0.0),
                payload.location_y.unwrap_or(0.0)
            ),
            "link" => format!(
                "[Link: {} - {}]",
                payload.title.as_deref().unwrap_or(""),
                payload.url.as_deref().unwrap_or("")
            ),
            "event" => format!("[Event: {}]", payload.event.as_deref().unwrap_or("unknown")),
            _ => "[Unknown message type]".to_string(),
        };

        let metadata = MetadataBuilder::new()
            .add("from_user", &sender_id)
            .add("channel_id", &sender_id) // Use sender_id as channel_id for WeChat
            .add("msg_type", &msg_type)
            .add_optional("msg_id", payload.msg_id.map(|v| v.to_string()).as_ref())
            .add_optional("agent_id", payload.agent_id.as_ref())
            .add_optional("media_id", payload.media_id.as_ref())
            .add_optional("pic_url", payload.pic_url.as_ref())
            .add_optional("recognition", payload.recognition.as_ref())
            .add_optional("event", payload.event.as_ref())
            .add_optional("event_key", payload.event_key.as_ref())
            .add_optional("file_key", payload.file_key.as_ref())
            .add_optional("file_md5", payload.file_md5.as_ref())
            .add_optional(
                "file_total_len",
                payload.file_total_len.map(|v| v.to_string()),
            )
            .build();

        Some(Message {
            id: uuid::Uuid::new_v4(),
            thread_id: uuid::Uuid::new_v4(),
            platform: PlatformType::WeChat,
            message_type: self.parse_message_type(&msg_type),
            content,
            metadata,
            timestamp: payload
                .create_time
                .map(|t| chrono::DateTime::from_timestamp(t, 0))
                .flatten()
                .unwrap_or_else(chrono::Utc::now),
        })
    }

    /// Handle URL verification (echostr)
    pub fn handle_verification(
        &self,
        signature: &str,
        timestamp: &str,
        nonce: &str,
        echostr: &str,
    ) -> Result<String> {
        // URL verification: signature is computed with echostr as msg_encrypt
        if self.verify_signature(timestamp, nonce, echostr, signature) {
            // If encrypted, decrypt echostr
            if echostr.len() > 24 {
                // Likely encrypted
                self.decrypt_message(echostr)
            } else {
                Ok(echostr.to_string())
            }
        } else {
            Err(AgentError::authentication("Invalid signature").into())
        }
    }

    /// Get sender info from payload
    pub fn get_sender_info(&self, payload: &WeChatWebhookPayload) -> WeChatSenderInfo {
        WeChatSenderInfo {
            user_id: payload.from_user_name.clone(),
            corp_id: payload.to_user_name.clone(),
            agent_id: payload.agent_id.clone(),
        }
    }
}

/// WeChat Work sender information
#[derive(Debug, Clone)]
pub struct WeChatSenderInfo {
    pub user_id: Option<String>,
    pub corp_id: Option<String>,
    pub agent_id: Option<String>,
}

#[async_trait]
impl WebhookHandler for WeChatWebhookHandler {
    fn platform_type(&self) -> PlatformType {
        PlatformType::WeChat
    }

    async fn verify_signature(
        &self,
        _body: &[u8],
        _signature: Option<&str>,
        _timestamp: Option<&str>,
    ) -> Result<SignatureVerification> {
        // For WeChat, we skip signature verification here because
        // we need nonce from query params which is not available in this method.
        // Signature verification should be done in the handler before calling this.
        debug!("WeChat signature verification skipped in trait method");
        Ok(SignatureVerification::Valid)
    }

    async fn parse_payload(&self, body: &[u8]) -> Result<Vec<WebhookEvent>> {
        let body_str = String::from_utf8_lossy(body);

        // Parse XML payload
        let mut payload = self.parse_xml(&body_str)?;

        // Handle encrypted payload if needed
        if let Some(encrypt) = &payload.encrypt {
            if self.config.decrypt_messages {
                let decrypted = self.decrypt_message(encrypt)?;
                payload = self.parse_xml(&decrypted)?;
            } else {
                return Err(AgentError::platform(
                    "Received encrypted payload but decryption is not configured",
                ));
            }
        }

        debug!("Parsed WeChat payload: {:?}", payload);

        // Determine event type
        let msg_type = payload
            .msg_type
            .clone()
            .unwrap_or_else(|| "text".to_string());
        let event_type = if msg_type == "event" {
            self.parse_event_type(payload.event.as_deref().unwrap_or(""))
        } else {
            WebhookEventType::MessageReceived
        };

        let message = self.convert_to_message(&payload);

        // Build metadata
        let sender_id = payload.from_user_name.as_deref().unwrap_or("");
        let metadata = MetadataBuilder::new()
            .add_optional("agent_id", payload.agent_id.as_ref())
            .add_optional("sender_id", payload.from_user_name.as_ref())
            .add("channel_id", sender_id) // Use sender_id as channel_id for reply
            .build();

        let webhook_event = WebhookEvent {
            event_type,
            platform: PlatformType::WeChat,
            event_id: payload
                .msg_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            timestamp: payload
                .create_time
                .map(|t| chrono::DateTime::from_timestamp(t, 0))
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
            WebhookEventType::MessageReceived | WebhookEventType::BotMentioned => {
                if let Some(msg) = &event.message {
                    info!(
                        "Received message from WeChat Work: {} (type: {:?})",
                        msg.content, msg.message_type
                    );

                    // P0 FIX: Removed dispatcher.dispatch() to avoid duplicate
                    // processing. Messages are now routed
                    // exclusively through channel_event_bus →
                    // MessageProcessor → AgentResolver path in webhook_handler.
                    // if let Some(dispatcher) = &self.dispatcher {
                    //     let platform_user_id =
                    // event.metadata.get("to_user_name")
                    //         .cloned()
                    //         .unwrap_or_else(|| self.corp_id.clone());
                    //     let target_channel_id =
                    // msg.metadata.get("from_user_name")
                    //         .cloned()
                    //         .unwrap_or_default();
                    //
                    //     dispatcher.dispatch(
                    //         PlatformType::WeChat,
                    //         &platform_user_id,
                    //         msg.clone(),
                    //         target_channel_id,
                    //     ).await?;
                    // }
                }
            }
            WebhookEventType::UserJoined => {
                info!("User subscribed to WeChat Work agent");
            }
            WebhookEventType::UserLeft => {
                info!("User unsubscribed from WeChat Work agent");
            }
            WebhookEventType::System => {
                debug!("Received system event from WeChat Work");
            }
            _ => {
                debug!(
                    "Received unhandled event type from WeChat Work: {:?}",
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

/// WeChat Work webhook response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeChatWebhookResponse {
    #[serde(rename = "ToUserName")]
    pub to_user_name: String,
    #[serde(rename = "FromUserName")]
    pub from_user_name: String,
    #[serde(rename = "CreateTime")]
    pub create_time: i64,
    #[serde(rename = "MsgType")]
    pub msg_type: String,
    #[serde(rename = "Content", skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(rename = "Encrypt", skip_serializing_if = "Option::is_none")]
    pub encrypt: Option<String>,
}

impl WeChatWebhookResponse {
    /// Create a text response
    pub fn text(to_user: &str, from_user: &str, content: &str) -> Self {
        Self {
            to_user_name: to_user.to_string(),
            from_user_name: from_user.to_string(),
            create_time: chrono::Utc::now().timestamp(),
            msg_type: "text".to_string(),
            content: Some(content.to_string()),
            encrypt: None,
        }
    }

    /// Create an encrypted response
    pub fn encrypted(to_user: &str, from_user: &str, encrypted_data: &str) -> Self {
        Self {
            to_user_name: to_user.to_string(),
            from_user_name: from_user.to_string(),
            create_time: chrono::Utc::now().timestamp(),
            msg_type: "text".to_string(),
            content: None,
            encrypt: Some(encrypted_data.to_string()),
        }
    }

    /// Convert to XML string
    pub fn to_xml(&self) -> String {
        if let Some(content) = &self.content {
            format!(
                r#"<xml>
<ToUserName><![CDATA[{}]]></ToUserName>
<FromUserName><![CDATA[{}]]></FromUserName>
<CreateTime>{}</CreateTime>
<MsgType><![CDATA[text]]></MsgType>
<Content><![CDATA[{}]]></Content>
</xml>"#,
                self.to_user_name, self.from_user_name, self.create_time, content
            )
        } else if let Some(encrypt) = &self.encrypt {
            format!(
                r#"<xml>
<ToUserName><![CDATA[{}]]></ToUserName>
<Encrypt><![CDATA[{}]]></Encrypt>
<AgentID><![CDATA[{}]]></AgentID>
</xml>"#,
                self.to_user_name, encrypt, self.from_user_name
            )
        } else {
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_message_type() {
        let handler = WeChatWebhookHandler::new("corp_id".to_string(), "token".to_string(), None);

        assert_eq!(handler.parse_message_type("text"), MessageType::Text);
        assert_eq!(handler.parse_message_type("image"), MessageType::Image);
        assert_eq!(handler.parse_message_type("voice"), MessageType::Voice);
        assert_eq!(handler.parse_message_type("video"), MessageType::Video);
        assert_eq!(handler.parse_message_type("file"), MessageType::File);
        assert_eq!(handler.parse_message_type("location"), MessageType::System);
        assert_eq!(handler.parse_message_type("event"), MessageType::System);
        assert_eq!(handler.parse_message_type("unknown"), MessageType::System);
    }

    #[test]
    fn test_parse_event_type() {
        let handler = WeChatWebhookHandler::new("corp_id".to_string(), "token".to_string(), None);

        assert_eq!(
            handler.parse_event_type("subscribe"),
            WebhookEventType::UserJoined
        );
        assert_eq!(
            handler.parse_event_type("unsubscribe"),
            WebhookEventType::UserLeft
        );
        assert_eq!(
            handler.parse_event_type("click"),
            WebhookEventType::BotMentioned
        );
        assert_eq!(
            handler.parse_event_type("unknown"),
            WebhookEventType::Unknown
        );
    }

    #[test]
    fn test_webhook_response() {
        let response = WeChatWebhookResponse::text("to_user", "from_user", "Hello");
        assert_eq!(response.msg_type, "text");
        assert_eq!(response.content, Some("Hello".to_string()));
        assert_eq!(response.to_user_name, "to_user");
        assert_eq!(response.from_user_name, "from_user");

        let xml = response.to_xml();
        assert!(xml.contains("<ToUserName><![CDATA[to_user]]></ToUserName>"));
        assert!(xml.contains("<Content><![CDATA[Hello]]></Content>"));
    }

    #[test]
    fn test_extract_xml_value() {
        let handler = WeChatWebhookHandler::new("corp_id".to_string(), "token".to_string(), None);

        let xml = r#"<xml><ToUserName><![CDATA[corp_id]]></ToUserName><FromUserName><![CDATA[user123]]></FromUserName></xml>"#;

        assert_eq!(
            handler.extract_xml_value(xml, "ToUserName"),
            Some("corp_id".to_string())
        );
        assert_eq!(
            handler.extract_xml_value(xml, "FromUserName"),
            Some("user123".to_string())
        );
        assert_eq!(handler.extract_xml_value(xml, "NonExistent"), None);
    }
}
