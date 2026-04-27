//! iLink Protocol Client for WeChat Personal Account
//!
//! Direct implementation of Tencent's iLink Bot API for WeChat personal
//! accounts. Based on the official OpenClaw/iLink protocol.
//!
//! API Base: https://ilinkai.weixin.qq.com

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use crate::error::{AgentError, Result};

/// iLink API base URL
const ILINK_API_BASE: &str = "https://ilinkai.weixin.qq.com";

/// Default session duration (24 hours)
pub const SESSION_DURATION_SECS: u64 = 24 * 3600;

/// Long-polling timeout (35 seconds, matches server behavior)
pub const POLLING_TIMEOUT_SECS: u64 = 35;

/// Request timeout for long-polling
pub const REQUEST_TIMEOUT_SECS: u64 = 45;

/// iLink client configuration
#[derive(Debug, Clone)]
pub struct ILinkConfig {
    pub base_url: String,
    pub timeout_secs: u64,
    pub max_retries: u32,
}

impl Default for ILinkConfig {
    fn default() -> Self {
        Self {
            base_url: ILINK_API_BASE.to_string(),
            timeout_secs: REQUEST_TIMEOUT_SECS,
            max_retries: 3,
        }
    }
}

/// Bot session info
#[derive(Debug, Clone)]
pub struct BotSession {
    pub bot_token: String,
    pub base_url: String,
    pub login_time: std::time::Instant,
    pub wxid: Option<String>,
    pub nickname: Option<String>,
}

impl BotSession {
    /// Check if session is still valid (not expired)
    pub fn is_valid(&self) -> bool {
        let elapsed = self.login_time.elapsed().as_secs();
        elapsed < SESSION_DURATION_SECS
    }

    /// Get remaining seconds before expiration
    pub fn remaining_secs(&self) -> u64 {
        let elapsed = self.login_time.elapsed().as_secs();
        if elapsed >= SESSION_DURATION_SECS {
            0
        } else {
            SESSION_DURATION_SECS - elapsed
        }
    }

    /// Format remaining time as human-readable string
    pub fn remaining_text(&self) -> String {
        let secs = self.remaining_secs();
        let hours = secs / 3600;
        let minutes = (secs % 3600) / 60;
        let seconds = secs % 60;

        if hours > 0 {
            format!("{} 小时 {} 分钟", hours, minutes)
        } else if minutes > 0 {
            format!("{} 分钟 {} 秒", minutes, seconds)
        } else {
            format!("{} 秒", seconds)
        }
    }
}

/// QR Code response from get_bot_qrcode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrCodeResponse {
    pub qrcode: String,
    #[serde(rename = "qrcode_img_content")]
    pub qrcode_img_content: Option<String>,
}

/// QR Code status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrCodeStatusResponse {
    pub status: String,
    #[serde(rename = "bot_token")]
    pub bot_token: Option<String>,
    #[serde(rename = "baseurl")]
    pub base_url: Option<String>,
}

/// Message from WeChat (inbound)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeChatMessage {
    #[serde(rename = "message_id")]
    pub message_id: Option<i64>,
    #[serde(rename = "from_user_id")]
    pub from_user_id: String,
    #[serde(rename = "to_user_id")]
    pub to_user_id: String,
    #[serde(rename = "message_type")]
    pub message_type: i32,
    #[serde(rename = "message_state")]
    pub message_state: i32,
    #[serde(rename = "context_token")]
    pub context_token: String,
    #[serde(rename = "item_list")]
    pub item_list: Vec<MessageItem>,
    #[serde(rename = "seq", default)]
    pub seq: Option<i64>,
}

impl WeChatMessage {
    /// Get text content from message
    pub fn text(&self) -> Option<String> {
        for item in &self.item_list {
            if item.item_type == 1 {
                return item.text_item.as_ref().map(|t| t.text.clone());
            }
        }
        None
    }

    /// Get picture content from message
    pub fn picture(&self) -> Option<&PicItem> {
        for item in &self.item_list {
            if item.item_type == 2 {
                return item.pic_item.as_ref();
            }
        }
        None
    }

    /// Get voice content from message
    pub fn voice(&self) -> Option<&VoiceItem> {
        for item in &self.item_list {
            if item.item_type == 3 {
                return item.voice_item.as_ref();
            }
        }
        None
    }

    /// Get video content from message
    pub fn video(&self) -> Option<&VideoItem> {
        for item in &self.item_list {
            if item.item_type == 4 {
                return item.video_item.as_ref();
            }
        }
        None
    }

    /// Get message type name
    pub fn message_type_name(&self) -> &'static str {
        match self.message_type {
            1 => "text",
            2 => "image",
            3 => "voice",
            4 => "video",
            5 => "location",
            6 => "link",
            7 => "business_card",
            8 => "file",
            9 => "quote",
            10 => "system",
            _ => "unknown",
        }
    }
}

/// Message item in item_list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageItem {
    #[serde(rename = "type")]
    pub item_type: i32,
    #[serde(rename = "text_item")]
    pub text_item: Option<TextItem>,
    #[serde(rename = "pic_item")]
    pub pic_item: Option<PicItem>,
    #[serde(rename = "voice_item")]
    pub voice_item: Option<VoiceItem>,
    #[serde(rename = "video_item")]
    pub video_item: Option<VideoItem>,
}

/// Text item content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextItem {
    pub text: String,
}

/// Picture item content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PicItem {
    #[serde(rename = "file_name")]
    pub file_name: String,
    #[serde(rename = "pic_url")]
    pub pic_url: String,
    #[serde(rename = "thumb_url")]
    pub thumb_url: Option<String>,
    #[serde(rename = "pic_size")]
    pub pic_size: i32,
    #[serde(rename = "pic_width")]
    pub pic_width: i32,
    #[serde(rename = "pic_height")]
    pub pic_height: i32,
}

/// Voice item content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceItem {
    #[serde(rename = "file_name")]
    pub file_name: String,
    #[serde(rename = "voice_url")]
    pub voice_url: String,
    #[serde(rename = "voice_size")]
    pub voice_size: i32,
    #[serde(rename = "voice_duration")]
    pub voice_duration: i32,
}

/// Video item content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoItem {
    #[serde(rename = "file_name")]
    pub file_name: String,
    #[serde(rename = "video_url")]
    pub video_url: String,
    #[serde(rename = "thumb_url")]
    pub thumb_url: Option<String>,
    #[serde(rename = "video_size")]
    pub video_size: i32,
    #[serde(rename = "video_duration")]
    pub video_duration: i32,
}

/// GetUpdates response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetUpdatesResponse {
    #[serde(rename = "ret", default)]
    pub ret: i32,
    #[serde(rename = "msgs", default)]
    pub msgs: Option<Vec<WeChatMessage>>,
    #[serde(rename = "get_updates_buf", default)]
    pub get_updates_buf: Option<String>,
    #[serde(rename = "longpolling_timeout_ms", default)]
    pub longpolling_timeout_ms: Option<u64>,
}

/// GetConfig request/response
#[derive(Debug, Clone, Serialize)]
pub struct GetConfigRequest {
    #[serde(rename = "ilink_user_id")]
    pub ilink_user_id: String,
    #[serde(rename = "context_token")]
    pub context_token: String,
    #[serde(rename = "base_info")]
    pub base_info: BaseInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetConfigResponse {
    #[serde(rename = "typing_ticket")]
    pub typing_ticket: Option<String>,
}

/// SendTyping request
#[derive(Debug, Clone, Serialize)]
pub struct SendTypingRequest {
    #[serde(rename = "ilink_user_id")]
    pub ilink_user_id: String,
    #[serde(rename = "typing_ticket")]
    pub typing_ticket: String,
    pub status: i32, // 1 = typing, 2 = stop typing
}

/// SendMessage request
#[derive(Debug, Clone, Serialize)]
pub struct SendMessageRequest {
    pub msg: OutboundMessage,
    #[serde(rename = "base_info")]
    pub base_info: BaseInfo,
}

/// Outbound message structure
#[derive(Debug, Clone, Serialize)]
pub struct OutboundMessage {
    #[serde(rename = "from_user_id")]
    pub from_user_id: String,
    #[serde(rename = "to_user_id")]
    pub to_user_id: String,
    #[serde(rename = "client_id")]
    pub client_id: String,
    #[serde(rename = "message_type")]
    pub message_type: i32,
    #[serde(rename = "message_state")]
    pub message_state: i32,
    #[serde(rename = "context_token")]
    pub context_token: String,
    #[serde(rename = "item_list")]
    pub item_list: Vec<OutboundMessageItem>,
}

/// Outbound message item
#[derive(Debug, Clone, Serialize)]
pub struct OutboundMessageItem {
    #[serde(rename = "type")]
    pub item_type: i32,
    #[serde(rename = "text_item")]
    pub text_item: TextItemContent,
}

#[derive(Debug, Clone, Serialize)]
pub struct TextItemContent {
    pub text: String,
}

/// Base info required in all requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseInfo {
    #[serde(rename = "channel_version")]
    pub channel_version: String,
}

impl Default for BaseInfo {
    fn default() -> Self {
        Self {
            channel_version: "1.0.2".to_string(),
        }
    }
}

/// iLink HTTP client
pub struct ILinkClient {
    config: ILinkConfig,
    http_client: reqwest::Client,
}

impl ILinkClient {
    /// Create a new iLink client
    pub fn new(config: Option<ILinkConfig>) -> Self {
        let config = config.unwrap_or_default();
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(10)
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config,
            http_client,
        }
    }

    /// Generate random X-WECHAT-UIN header
    fn generate_uin_header(&self) -> String {
        let uin: u32 = rand::random();
        base64::encode(uin.to_string())
    }

    /// Create headers for iLink API requests
    fn make_headers(&self, token: Option<&str>) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        headers.insert("AuthorizationType", "ilink_bot_token".parse().unwrap());
        headers.insert("X-WECHAT-UIN", self.generate_uin_header().parse().unwrap());

        if let Some(t) = token {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", t).parse().unwrap(),
            );
        }

        headers
    }

    /// Get bot QR code for login
    pub async fn get_bot_qrcode(&self) -> Result<QrCodeResponse> {
        let url = format!(
            "{}/ilink/bot/get_bot_qrcode?bot_type=3",
            self.config.base_url
        );

        println!("[ILINK] Requesting QR code from: {}", url);
        info!("🌐 正在请求 iLink QR 码: {}", url);

        let response = match self
            .http_client
            .get(&url)
            .timeout(Duration::from_secs(10))
            .send()
            .await
        {
            Ok(resp) => {
                println!("[ILINK] HTTP response received: {}", resp.status());
                resp
            }
            Err(e) => {
                println!("[ILINK] HTTP request failed: {}", e);
                error!("❌ 请求 QR 码失败: {}", e);
                return Err(AgentError::platform(format!("Failed to get QR code: {}", e)).into());
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!("❌ 获取 QR 码失败 ({}): {}", status, error_text);
            return Err(AgentError::platform(format!(
                "Get QR code failed ({}): {}",
                status, error_text
            ))
            .into());
        }

        info!("✅ QR 码响应成功，正在解析...");

        let qr_resp: QrCodeResponse = match response.json().await {
            Ok(resp) => resp,
            Err(e) => {
                error!("❌ 解析 QR 码响应失败: {}", e);
                return Err(
                    AgentError::platform(format!("Failed to parse QR response: {}", e)).into(),
                );
            }
        };

        info!("✅ 成功获取 iLink QR 码");
        Ok(qr_resp)
    }

    /// Get QR code status
    pub async fn get_qrcode_status(&self, qrcode: &str) -> Result<QrCodeStatusResponse> {
        let url = format!(
            "{}/ilink/bot/get_qrcode_status?qrcode={}",
            self.config.base_url, qrcode
        );

        let response = self
            .http_client
            .get(&url)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to get QR status: {}", e)))?;

        let status: QrCodeStatusResponse = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse QR status: {}", e)))?;

        Ok(status)
    }

    /// Long-polling for messages
    pub async fn get_updates(
        &self,
        bot_token: &str,
        base_url: Option<&str>,
        updates_buf: &str,
    ) -> Result<GetUpdatesResponse> {
        let url = format!(
            "{}/ilink/bot/getupdates",
            base_url.unwrap_or(&self.config.base_url)
        );

        let body = serde_json::json!({
            "get_updates_buf": updates_buf,
            "base_info": BaseInfo::default()
        });

        let response = self
            .http_client
            .post(&url)
            .headers(self.make_headers(Some(bot_token)))
            .json(&body)
            .timeout(Duration::from_secs(POLLING_TIMEOUT_SECS + 10))
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AgentError::platform("GetUpdates timeout (normal for long-polling)")
                } else {
                    AgentError::platform(format!("GetUpdates failed: {}", e))
                }
            })?;

        if !response.status().is_success() {
            let status = response.status();
            if status.as_u16() == 401 {
                return Err(AgentError::platform("Session expired, need to re-login").into());
            }
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AgentError::platform(format!(
                "GetUpdates error ({}): {}",
                status, error_text
            ))
            .into());
        }

        let updates: GetUpdatesResponse = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse updates: {}", e)))?;

        Ok(updates)
    }

    /// Get config (typing_ticket)
    pub async fn get_config(
        &self,
        bot_token: &str,
        base_url: Option<&str>,
        user_id: &str,
        context_token: &str,
    ) -> Result<GetConfigResponse> {
        let url = format!(
            "{}/ilink/bot/getconfig",
            base_url.unwrap_or(&self.config.base_url)
        );

        let body = GetConfigRequest {
            ilink_user_id: user_id.to_string(),
            context_token: context_token.to_string(),
            base_info: BaseInfo::default(),
        };

        let response = self
            .http_client
            .post(&url)
            .headers(self.make_headers(Some(bot_token)))
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("GetConfig failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AgentError::platform(format!(
                "GetConfig error ({}): {}",
                status, error_text
            ))
            .into());
        }

        let config: GetConfigResponse = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse config: {}", e)))?;

        Ok(config)
    }

    /// Send typing status
    pub async fn send_typing(
        &self,
        bot_token: &str,
        base_url: Option<&str>,
        user_id: &str,
        typing_ticket: &str,
        status: i32,
    ) -> Result<()> {
        let url = format!(
            "{}/ilink/bot/sendtyping",
            base_url.unwrap_or(&self.config.base_url)
        );

        let body = SendTypingRequest {
            ilink_user_id: user_id.to_string(),
            typing_ticket: typing_ticket.to_string(),
            status,
        };

        let response = self
            .http_client
            .post(&url)
            .headers(self.make_headers(Some(bot_token)))
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("SendTyping failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            warn!("SendTyping error ({}): {}", status, error_text);
        }

        Ok(())
    }

    /// Send message
    pub async fn send_message(
        &self,
        bot_token: &str,
        base_url: Option<&str>,
        to_user_id: &str,
        context_token: &str,
        text: &str,
    ) -> Result<()> {
        let url = format!(
            "{}/ilink/bot/sendmessage",
            base_url.unwrap_or(&self.config.base_url)
        );

        // Generate random client_id
        let client_id = format!("openclaw-weixin-{:08x}", rand::random::<u32>());

        let body = SendMessageRequest {
            msg: OutboundMessage {
                from_user_id: "".to_string(),
                to_user_id: to_user_id.to_string(),
                client_id,
                message_type: 2,  // BOT message
                message_state: 2, // FINISH
                context_token: context_token.to_string(),
                item_list: vec![OutboundMessageItem {
                    item_type: 1, // text
                    text_item: TextItemContent {
                        text: text.to_string(),
                    },
                }],
            },
            base_info: BaseInfo::default(),
        };

        debug!("Sending message to {}: {}", to_user_id, text);

        let response = self
            .http_client
            .post(&url)
            .headers(self.make_headers(Some(bot_token)))
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("SendMessage failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AgentError::platform(format!(
                "SendMessage error ({}): {}",
                status, error_text
            ))
            .into());
        }

        debug!("Message sent successfully");
        Ok(())
    }

    /// Download media file (image, voice, video)
    pub async fn download_media(
        &self,
        bot_token: &str,
        _base_url: Option<&str>,
        media_url: &str,
    ) -> Result<Vec<u8>> {
        info!("🌐 下载媒体文件: {}", media_url);

        let response = self
            .http_client
            .get(media_url)
            .headers(self.make_headers(Some(bot_token)))
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to download media: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AgentError::platform(format!(
                "Download media error ({}): {}",
                status, error_text
            ))
            .into());
        }

        let data = response
            .bytes()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to read media data: {}", e)))?;

        info!("✅ 媒体文件下载成功: {} bytes", data.len());
        Ok(data.to_vec())
    }
}

// Simple base64 encode for generating X-WECHAT-UIN
mod base64 {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn encode(input: String) -> String {
        let bytes = input.as_bytes();
        let mut result = String::with_capacity((bytes.len() + 2) / 3 * 4);

        for chunk in bytes.chunks(3) {
            let b = match chunk.len() {
                1 => [chunk[0], 0, 0],
                2 => [chunk[0], chunk[1], 0],
                3 => [chunk[0], chunk[1], chunk[2]],
                _ => unreachable!(),
            };

            let idx0 = (b[0] >> 2) as usize;
            let idx1 = (((b[0] & 0b11) << 4) | (b[1] >> 4)) as usize;
            let idx2 = (((b[1] & 0b1111) << 2) | (b[2] >> 6)) as usize;
            let idx3 = (b[2] & 0b111111) as usize;

            result.push(ALPHABET[idx0] as char);
            result.push(ALPHABET[idx1] as char);

            if chunk.len() > 1 {
                result.push(ALPHABET[idx2] as char);
            } else {
                result.push('=');
            }

            if chunk.len() > 2 {
                result.push(ALPHABET[idx3] as char);
            } else {
                result.push('=');
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64::encode("1234567890".to_string()), "MTIzNDU2Nzg5MA==");
    }

    #[test]
    fn test_session_remaining() {
        let session = BotSession {
            bot_token: "test".to_string(),
            base_url: "https://test".to_string(),
            login_time: std::time::Instant::now(),
            wxid: None,
            nickname: None,
        };

        assert!(session.remaining_secs() > 0);
        assert!(!session.remaining_text().is_empty());
    }
}
