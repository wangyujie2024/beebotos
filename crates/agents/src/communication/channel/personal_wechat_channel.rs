//! Personal WeChat Channel Implementation (iLink Protocol)
//!
//! Direct implementation of Tencent's iLink Bot API for WeChat personal
//! accounts. Based on the official OpenClaw/iLink protocol without requiring a
//! middle-layer service.
//!
//! Features:
//! - Direct iLink API communication (https://ilinkai.weixin.qq.com)
//! - QR code login with 24h session
//! - Long-polling message reception (35s server hold)
//! - Auto-reconnection before session expiration
//! - "Typing" indicator support
//!
//! Protocol Reference:
//! - https://docs.openclaw.ai
//! - @tencent-weixin/openclaw-weixin npm package

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use super::ilink_client::{BotSession, ILinkClient, ILinkConfig, QrCodeResponse};
use super::r#trait::{BaseChannelConfig, ConnectionMode, ContentType};
use super::{Channel, ChannelConfig, ChannelEvent, ChannelInfo, MemberInfo};
use crate::communication::{Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

/// Personal WeChat channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalWeChatConfig {
    /// Base URL for iLink API (default: https://ilinkai.weixin.qq.com)
    #[serde(default = "default_ilink_base_url")]
    pub base_url: String,
    /// Bot token (obtained after QR login, optional for initial setup)
    pub bot_token: Option<String>,
    /// Bot base URL (may differ from base_url after login)
    pub bot_base_url: Option<String>,
    /// Auto-reconnect before session expires
    #[serde(default = "default_auto_reconnect")]
    pub auto_reconnect: bool,
    /// Reconnection check interval in seconds
    #[serde(default = "default_reconnect_interval")]
    pub reconnect_interval_secs: u64,
    /// Warning before expiration (seconds)
    #[serde(default = "default_warning_before")]
    pub warning_before_secs: u64,
    /// Force reconnection when remaining time is below this (seconds)
    #[serde(default = "default_force_before")]
    pub force_before_secs: u64,
    /// Base channel configuration
    #[serde(flatten)]
    pub base: BaseChannelConfig,
}

fn default_ilink_base_url() -> String {
    "https://ilinkai.weixin.qq.com".to_string()
}

fn default_auto_reconnect() -> bool {
    true
}

fn default_reconnect_interval() -> u64 {
    300 // 5 minutes
}

fn default_warning_before() -> u64 {
    7200 // 2 hours
}

fn default_force_before() -> u64 {
    1800 // 30 minutes
}

impl Default for PersonalWeChatConfig {
    fn default() -> Self {
        Self {
            base_url: default_ilink_base_url(),
            bot_token: None,
            bot_base_url: None,
            auto_reconnect: default_auto_reconnect(),
            reconnect_interval_secs: default_reconnect_interval(),
            warning_before_secs: default_warning_before(),
            force_before_secs: default_force_before(),
            base: BaseChannelConfig {
                connection_mode: ConnectionMode::Polling,
                auto_reconnect: true,
                max_reconnect_attempts: 10,
                ..Default::default()
            },
        }
    }
}

impl ChannelConfig for PersonalWeChatConfig {
    fn from_env() -> Option<Self>
    where
        Self: Sized,
    {
        let base_url =
            std::env::var("PERSONAL_WECHAT_BASE_URL").unwrap_or_else(|_| default_ilink_base_url());
        let bot_token = std::env::var("PERSONAL_WECHAT_BOT_TOKEN").ok();
        let bot_base_url = std::env::var("PERSONAL_WECHAT_BOT_BASE_URL").ok();

        let mut base = BaseChannelConfig::from_env("PERSONAL_WECHAT").unwrap_or_default();
        base.connection_mode = ConnectionMode::Polling;

        Some(Self {
            base_url,
            bot_token,
            bot_base_url,
            auto_reconnect: std::env::var("PERSONAL_WECHAT_AUTO_RECONNECT")
                .map(|v| v.parse().unwrap_or(true))
                .unwrap_or(true),
            reconnect_interval_secs: std::env::var("PERSONAL_WECHAT_RECONNECT_INTERVAL")
                .map(|v| v.parse().unwrap_or(300))
                .unwrap_or(300),
            warning_before_secs: std::env::var("PERSONAL_WECHAT_WARNING_BEFORE")
                .map(|v| v.parse().unwrap_or(7200))
                .unwrap_or(7200),
            force_before_secs: std::env::var("PERSONAL_WECHAT_FORCE_BEFORE")
                .map(|v| v.parse().unwrap_or(1800))
                .unwrap_or(1800),
            base,
        })
    }

    fn is_valid(&self) -> bool {
        // Config is valid if we have bot_token (already logged in) or we're ready for
        // QR login
        true
    }

    fn allowlist(&self) -> Vec<String> {
        vec![]
    }

    fn connection_mode(&self) -> ConnectionMode {
        ConnectionMode::Polling
    }

    fn auto_reconnect(&self) -> bool {
        self.auto_reconnect
    }

    fn max_reconnect_attempts(&self) -> u32 {
        self.base.max_reconnect_attempts
    }
}

/// Typing ticket cache per user
#[derive(Debug, Clone)]
struct TypingTicketInfo {
    ticket: String,
    cached_at: std::time::Instant,
}

impl TypingTicketInfo {
    fn is_valid(&self) -> bool {
        // Typing ticket valid for 24 hours
        self.cached_at.elapsed().as_secs() < 24 * 3600
    }
}

/// Internal QR code info for login
#[derive(Debug, Clone)]
struct QrLoginInfo {
    qrcode: String,
    qrcode_url: Option<String>,
}

/// Serializable session data for persistence across restarts
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedSession {
    bot_token: String,
    base_url: String,
    login_time: chrono::DateTime<chrono::Utc>,
    wxid: Option<String>,
    nickname: Option<String>,
}

/// Personal WeChat channel implementation using iLink protocol
pub struct PersonalWeChatChannel {
    config: PersonalWeChatConfig,
    ilink_client: ILinkClient,
    connected: Arc<RwLock<bool>>,
    session: Arc<RwLock<Option<BotSession>>>,
    /// QR code info for login
    qr_login_info: Arc<RwLock<Option<QrLoginInfo>>>,
    /// Typing ticket cache (user_id -> ticket info)
    typing_tickets: Arc<RwLock<HashMap<String, TypingTicketInfo>>>,
    /// Last contact for reconnection notifications
    last_contact: Arc<RwLock<Option<(String, String)>>>, // (user_id, context_token)
    /// Pending reconnection confirmation
    reconnect_pending: Arc<RwLock<bool>>,
    /// Listener task handle
    listener_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    /// Reconnection monitor task handle
    reconnect_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    /// Event sender
    event_sender: Arc<RwLock<Option<mpsc::Sender<ChannelEvent>>>>,
    /// Welcome messages sent to users
    welcomed_users: Arc<RwLock<HashMap<String, bool>>>,
    /// Session persistence file path
    session_store_path: PathBuf,
}

impl PersonalWeChatChannel {
    /// Create a new personal WeChat channel
    pub fn new(config: PersonalWeChatConfig) -> Self {
        let ilink_config = ILinkConfig {
            base_url: config.base_url.clone(),
            ..Default::default()
        };
        let ilink_client = ILinkClient::new(Some(ilink_config));

        let session_store_path = std::env::var("PERSONAL_WECHAT_SESSION_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("data/personal_wechat_session.json"));

        Self {
            config,
            ilink_client,
            connected: Arc::new(RwLock::new(false)),
            session: Arc::new(RwLock::new(None)),
            qr_login_info: Arc::new(RwLock::new(None)),
            typing_tickets: Arc::new(RwLock::new(HashMap::new())),
            last_contact: Arc::new(RwLock::new(None)),
            reconnect_pending: Arc::new(RwLock::new(false)),
            listener_handle: Arc::new(RwLock::new(None)),
            reconnect_handle: Arc::new(RwLock::new(None)),
            event_sender: Arc::new(RwLock::new(None)),
            welcomed_users: Arc::new(RwLock::new(HashMap::new())),
            session_store_path,
        }
    }

    /// Save current session to disk for persistence across restarts
    async fn save_session(&self) {
        if let Some(session) = self.session.read().await.as_ref() {
            let persisted = PersistedSession {
                bot_token: session.bot_token.clone(),
                base_url: session.base_url.clone(),
                login_time: chrono::Utc::now(),
                wxid: session.wxid.clone(),
                nickname: session.nickname.clone(),
            };
            if let Ok(json) = serde_json::to_string_pretty(&persisted) {
                if let Some(parent) = self.session_store_path.parent() {
                    let _ = tokio::fs::create_dir_all(parent).await;
                }
                match tokio::fs::write(&self.session_store_path, json).await {
                    Ok(_) => info!("个人微信 session 已持久化到 {:?}", self.session_store_path),
                    Err(e) => warn!("保存个人微信 session 失败: {}", e),
                }
            }
        }
    }

    /// Load session from disk if available
    async fn load_session(&self) -> Option<BotSession> {
        match tokio::fs::read_to_string(&self.session_store_path).await {
            Ok(content) => {
                match serde_json::from_str::<PersistedSession>(&content) {
                    Ok(persisted) => {
                        info!("从 {:?} 恢复个人微信 session", self.session_store_path);
                        Some(BotSession {
                            bot_token: persisted.bot_token,
                            base_url: persisted.base_url,
                            login_time: std::time::Instant::now(), // Reset timer on restore
                            wxid: persisted.wxid,
                            nickname: persisted.nickname,
                        })
                    }
                    Err(e) => {
                        warn!("解析个人微信 session 文件失败: {}", e);
                        None
                    }
                }
            }
            Err(e) => {
                debug!(
                    "未找到个人微信 session 文件 {:?}: {}",
                    self.session_store_path, e
                );
                None
            }
        }
    }

    /// Clear persisted session
    #[allow(dead_code)]
    async fn clear_session(&self) {
        let _ = tokio::fs::remove_file(&self.session_store_path).await;
    }

    /// Get QR code for login (to be displayed to user)
    pub async fn get_qr_code(&self) -> Result<QrCodeResponse> {
        println!("[PERSONAL_WECHAT] get_qr_code() called");
        println!("[PERSONAL_WECHAT] Calling ilink_client.get_bot_qrcode()...");
        let qr_resp = self.ilink_client.get_bot_qrcode().await?;
        println!("[PERSONAL_WECHAT] Got QR response: {}", qr_resp.qrcode);

        // Store QR info for status checking
        let login_info = QrLoginInfo {
            qrcode: qr_resp.qrcode.clone(),
            qrcode_url: qr_resp.qrcode_img_content.clone(),
        };
        println!("[PERSONAL_WECHAT] Storing QR info...");
        *self.qr_login_info.write().await = Some(login_info);
        println!("[PERSONAL_WECHAT] QR info stored successfully");

        println!("[PERSONAL_WECHAT] Logging QR code URL...");
        info!(
            "个人微信登录二维码已生成，请将以下链接在微信中打开并扫码:\n{}",
            qr_resp
                .qrcode_img_content
                .as_deref()
                .unwrap_or(&qr_resp.qrcode)
        );
        println!("[PERSONAL_WECHAT] get_qr_code() completed successfully");

        Ok(qr_resp)
    }

    /// Check QR code scan status and complete login
    pub async fn check_qr_status(&self) -> Result<bool> {
        let qrcode = match self.qr_login_info.read().await.as_ref() {
            Some(info) => info.qrcode.clone(),
            None => return Err(AgentError::platform("QR code not generated yet").into()),
        };

        let status = self.ilink_client.get_qrcode_status(&qrcode).await?;

        if status.status == "confirmed" {
            if let (Some(token), Some(base_url)) = (status.bot_token, status.base_url) {
                let session = BotSession {
                    bot_token: token,
                    base_url,
                    login_time: std::time::Instant::now(),
                    wxid: None,
                    nickname: None,
                };

                *self.session.write().await = Some(session);
                *self.connected.write().await = true;

                self.save_session().await;
                info!("个人微信登录成功！");

                // Start reconnection monitor
                self.start_reconnect_monitor();

                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Poll messages from iLink (long-polling)
    async fn poll_messages(&self, event_sender: mpsc::Sender<ChannelEvent>) -> Result<()> {
        let session = self
            .session
            .read()
            .await
            .as_ref()
            .cloned()
            .ok_or_else(|| AgentError::platform("Bot session not initialized"))?;

        let mut updates_buf = String::new();

        info!("开始 iLink 消息长轮询...");

        while *self.connected.read().await {
            match self
                .ilink_client
                .get_updates(&session.bot_token, Some(&session.base_url), &updates_buf)
                .await
            {
                Ok(updates) => {
                    // Update buffer for next poll
                    if let Some(new_buf) = updates.get_updates_buf {
                        updates_buf = new_buf;
                    }

                    // Process messages
                    if let Some(msgs) = updates.msgs {
                        for msg in msgs {
                            // Skip system messages (type 10) and unknown types
                            // Process user messages: text(1), image(2), voice(3), video(4), etc.
                            if msg.message_type == 10
                                || msg.message_type < 1
                                || msg.message_type > 9
                            {
                                debug!("跳过系统消息或未知类型: type={}", msg.message_type);
                                continue;
                            }

                            if let Err(e) = self.process_message(msg, &event_sender).await {
                                error!("处理消息失败: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    if error_msg.contains("expired") || error_msg.contains("401") {
                        error!("iLink 会话已过期，需要重新登录");
                        *self.connected.write().await = false;
                        break;
                    }
                    warn!("长轮询错误: {}，5秒后重试...", e);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }

        info!("消息轮询已停止");
        Ok(())
    }

    /// Process incoming message
    async fn process_message(
        &self,
        msg: super::ilink_client::WeChatMessage,
        event_sender: &mpsc::Sender<ChannelEvent>,
    ) -> Result<()> {
        let from_id = msg.from_user_id.clone();
        let context_token = msg.context_token.clone();
        let msg_type = msg.message_type;

        // Update last contact for reconnection notifications
        *self.last_contact.write().await = Some((from_id.clone(), context_token.clone()));

        // iLink may put the actual content type in item_list[0].item_type
        // rather than in message_type. Use item_type when available.
        let actual_type = msg
            .item_list
            .first()
            .map(|item| item.item_type)
            .unwrap_or(msg_type);

        // Log raw message structure for debugging
        if let Ok(json) = serde_json::to_string(&msg) {
            info!("📨 RAW iLink message: {}", json);
        }

        debug!(
            "收到个人微信消息 from={}, message_type={}, actual_item_type={}",
            from_id, msg_type, actual_type
        );

        // Create metadata
        let mut metadata = HashMap::new();
        metadata.insert("from_user_id".to_string(), from_id.clone());
        metadata.insert("sender_id".to_string(), from_id.clone());
        metadata.insert("channel_id".to_string(), from_id.clone());
        metadata.insert("to_user_id".to_string(), msg.to_user_id.clone());
        metadata.insert("context_token".to_string(), context_token.clone());
        if let Some(msg_id) = msg.message_id {
            metadata.insert("msg_id".to_string(), msg_id.to_string());
            metadata.insert("message_id".to_string(), msg_id.to_string());
        }

        // Handle different message types based on actual item type
        let (message_type, content) = match actual_type {
            1 => {
                // Text message
                let text = msg.text().unwrap_or_default();
                info!("📨 文本消息 from={}: {}", from_id, text);
                (MessageType::Text, text)
            }
            2 => {
                // Image message
                if let Some(pic) = msg.picture() {
                    info!("🖼️ 图片消息 from={}, url={}", from_id, pic.pic_url);
                    metadata.insert("image_url".to_string(), pic.pic_url.clone());
                    metadata.insert("image_width".to_string(), pic.pic_width.to_string());
                    metadata.insert("image_height".to_string(), pic.pic_height.to_string());
                    metadata.insert("image_size".to_string(), pic.pic_size.to_string());
                    metadata.insert("image_name".to_string(), pic.file_name.clone());
                    // Format required by multimodal processor to detect image_key
                    (
                        MessageType::Image,
                        format!("[图片] image_key: {}", pic.pic_url),
                    )
                } else {
                    // iLink personal WeChat protocol does not provide image URL in getupdates
                    info!("🖼️ 图片消息 from={} (iLink 协议限制，无图片 URL)", from_id);
                    (
                        MessageType::Image,
                        "[图片] 说明：由于个人微信 iLink 协议限制，Bot \
                         无法查看图片内容。请用文字描述图片，我会尽力帮您分析。"
                            .to_string(),
                    )
                }
            }
            3 => {
                // Voice message
                if let Some(voice) = msg.voice() {
                    info!("🎤 语音消息 from={}, url={}", from_id, voice.voice_url);
                    metadata.insert("voice_url".to_string(), voice.voice_url.clone());
                    metadata.insert(
                        "voice_duration".to_string(),
                        voice.voice_duration.to_string(),
                    );
                    metadata.insert("voice_size".to_string(), voice.voice_size.to_string());
                    metadata.insert("voice_name".to_string(), voice.file_name.clone());
                    (
                        MessageType::Voice,
                        format!("[语音 {}秒] {}", voice.voice_duration, voice.file_name),
                    )
                } else {
                    // iLink personal WeChat protocol does not provide voice URL in getupdates
                    info!("🎤 语音消息 from={} (iLink 协议限制，无语音 URL)", from_id);
                    (
                        MessageType::Voice,
                        "[语音] 说明：由于个人微信 iLink 协议限制，Bot \
                         无法收听语音消息。请将语音转换为文字发送，我会帮您分析。"
                            .to_string(),
                    )
                }
            }
            4 => {
                // Video message
                if let Some(video) = msg.video() {
                    info!("🎬 视频消息 from={}, url={}", from_id, video.video_url);
                    metadata.insert("video_url".to_string(), video.video_url.clone());
                    metadata.insert(
                        "video_duration".to_string(),
                        video.video_duration.to_string(),
                    );
                    metadata.insert("video_size".to_string(), video.video_size.to_string());
                    metadata.insert("video_name".to_string(), video.file_name.clone());
                    if let Some(thumb) = &video.thumb_url {
                        metadata.insert("video_thumb".to_string(), thumb.clone());
                    }
                    (
                        MessageType::Video,
                        format!("[视频 {}秒] {}", video.video_duration, video.file_name),
                    )
                } else {
                    // iLink personal WeChat protocol does not provide video URL in getupdates
                    info!("🎬 视频消息 from={} (iLink 协议限制，无视频 URL)", from_id);
                    (
                        MessageType::Video,
                        "[视频] 说明：由于个人微信 iLink 协议限制，Bot \
                         无法查看视频内容。请用文字描述视频内容，我会帮您分析。"
                            .to_string(),
                    )
                }
            }
            _ => {
                // Other message types
                info!(
                    "📨 其他类型消息 from={}, message_type={}, actual_type={}",
                    from_id, msg_type, actual_type
                );
                metadata.insert("raw_message_type".to_string(), msg_type.to_string());
                metadata.insert("raw_actual_type".to_string(), actual_type.to_string());
                (
                    MessageType::Text,
                    format!("[{}消息]", msg.message_type_name()),
                )
            }
        };

        let message = Message {
            id: Uuid::new_v4(),
            thread_id: Uuid::new_v4(),
            platform: PlatformType::WeChat,
            message_type,
            content,
            metadata,
            timestamp: chrono::Utc::now(),
        };

        let event = ChannelEvent::MessageReceived {
            platform: PlatformType::WeChat,
            channel_id: from_id.clone(),
            message,
        };

        if let Err(e) = event_sender.send(event).await {
            error!("发送事件失败: {}", e);
            return Err(AgentError::platform(format!("Event bus error: {}", e)).into());
        }

        Ok(())
    }

    /// Send message to user
    async fn send_message_internal(&self, to_user_id: &str, text: &str) -> Result<()> {
        let session = self
            .session
            .read()
            .await
            .as_ref()
            .cloned()
            .ok_or_else(|| AgentError::platform("未登录"))?;

        // Get or fetch typing ticket
        let typing_ticket = self.get_typing_ticket(to_user_id).await?;

        // Send "typing" status
        if let Err(e) = self
            .ilink_client
            .send_typing(
                &session.bot_token,
                Some(&session.base_url),
                to_user_id,
                &typing_ticket,
                1,
            )
            .await
        {
            warn!("发送 typing 状态失败: {}", e);
        }

        // Get context token from last contact or use empty
        let context_token = self
            .last_contact
            .read()
            .await
            .as_ref()
            .filter(|(id, _)| id == to_user_id)
            .map(|(_, ctx)| ctx.clone())
            .unwrap_or_default();

        // Send message
        self.ilink_client
            .send_message(
                &session.bot_token,
                Some(&session.base_url),
                to_user_id,
                &context_token,
                text,
            )
            .await?;

        // Stop "typing" status
        if let Err(e) = self
            .ilink_client
            .send_typing(
                &session.bot_token,
                Some(&session.base_url),
                to_user_id,
                &typing_ticket,
                2,
            )
            .await
        {
            warn!("停止 typing 状态失败: {}", e);
        }

        Ok(())
    }

    /// Get typing ticket (cached per user)
    async fn get_typing_ticket(&self, user_id: &str) -> Result<String> {
        // Check cache
        {
            let tickets = self.typing_tickets.read().await;
            if let Some(info) = tickets.get(user_id) {
                if info.is_valid() {
                    return Ok(info.ticket.clone());
                }
            }
        }

        // Fetch new ticket
        let session = self
            .session
            .read()
            .await
            .as_ref()
            .cloned()
            .ok_or_else(|| AgentError::platform("未登录"))?;

        let context_token = self
            .last_contact
            .read()
            .await
            .as_ref()
            .filter(|(id, _)| id == user_id)
            .map(|(_, ctx)| ctx.clone())
            .unwrap_or_default();

        let config_resp = self
            .ilink_client
            .get_config(
                &session.bot_token,
                Some(&session.base_url),
                user_id,
                &context_token,
            )
            .await?;

        let ticket = config_resp
            .typing_ticket
            .ok_or_else(|| AgentError::platform("无法获取 typing_ticket"))?;

        // Cache ticket
        let info = TypingTicketInfo {
            ticket: ticket.clone(),
            cached_at: std::time::Instant::now(),
        };
        self.typing_tickets
            .write()
            .await
            .insert(user_id.to_string(), info);

        Ok(ticket)
    }

    /// Start reconnection monitor task
    fn start_reconnect_monitor(&self) {
        if !self.config.auto_reconnect {
            return;
        }

        let channel = self.clone();
        let handle = tokio::spawn(async move {
            let mut check_interval =
                interval(Duration::from_secs(channel.config.reconnect_interval_secs));

            loop {
                check_interval.tick().await;

                let should_warn = {
                    if let Some(ref session) = *channel.session.read().await {
                        let remaining = session.remaining_secs();
                        let warning_threshold = channel.config.warning_before_secs;
                        remaining < warning_threshold
                            && remaining > channel.config.force_before_secs
                    } else {
                        false
                    }
                };

                if should_warn && !*channel.reconnect_pending.read().await {
                    if let Some((user_id, _context_token)) =
                        channel.last_contact.read().await.clone()
                    {
                        let remaining_text = channel
                            .session
                            .read()
                            .await
                            .as_ref()
                            .map(|s| s.remaining_text())
                            .unwrap_or_default();

                        warn!(
                            "个人微信会话即将过期 (剩余 {})，发送提醒消息...",
                            remaining_text
                        );

                        // Send warning message
                        let warning_msg = format!(
                            "[提醒] 微信 Bot 连接还剩 {}，即将需要重新扫码登录。\n回复 Y \
                             立即重连，N 稍后提醒。",
                            remaining_text
                        );

                        if let Err(e) = channel.send_message_internal(&user_id, &warning_msg).await
                        {
                            error!("发送重连提醒失败: {}", e);
                        } else {
                            *channel.reconnect_pending.write().await = true;
                        }
                    }
                }

                // Check if force reconnect needed
                let should_force = {
                    if let Some(ref session) = *channel.session.read().await {
                        session.remaining_secs() < channel.config.force_before_secs
                    } else {
                        false
                    }
                };

                if should_force {
                    error!("个人微信会话即将过期，强制断开连接!");
                    *channel.connected.write().await = false;
                    break;
                }
            }
        });

        // Spawn a task to set the handle since we can't hold the lock across await
        let reconnect_handle = self.reconnect_handle.clone();
        tokio::spawn(async move {
            *reconnect_handle.write().await = Some(handle);
        });
    }

    /// Get current session info
    pub async fn get_session_info(&self) -> Option<BotSession> {
        self.session.read().await.clone()
    }

    /// Check if session is valid
    pub async fn is_session_valid(&self) -> bool {
        if let Some(ref session) = *self.session.read().await {
            session.is_valid()
        } else {
            false
        }
    }

    /// Get QR code URL for display
    pub async fn get_qr_url(&self) -> Option<String> {
        let info = self.qr_login_info.read().await;
        info.as_ref()
            .and_then(|i| i.qrcode_url.clone())
            .or_else(|| info.as_ref().map(|i| i.qrcode.clone()))
    }

    /// Complete login with bot_token and start listener
    pub async fn complete_login(
        &self,
        bot_token: String,
        base_url: String,
        event_bus: mpsc::Sender<ChannelEvent>,
    ) -> Result<()> {
        let session = BotSession {
            bot_token,
            base_url,
            login_time: std::time::Instant::now(),
            wxid: None,
            nickname: None,
        };

        *self.session.write().await = Some(session);
        *self.connected.write().await = true;
        self.save_session().await;

        info!("========================================");
        info!("个人微信登录成功!");
        info!("========================================");

        self.start_reconnect_monitor();

        info!("🎧 启动个人微信消息监听...");
        if let Err(e) = self.start_listener(event_bus).await {
            error!("❌ 启动个人微信消息监听失败: {}", e);
            return Err(e);
        }

        info!("✅ 个人微信消息监听已启动");
        Ok(())
    }
}

#[async_trait]
impl Channel for PersonalWeChatChannel {
    fn name(&self) -> &str {
        "personal_wechat"
    }

    fn platform(&self) -> PlatformType {
        PlatformType::WeChat
    }

    fn is_connected(&self) -> bool {
        // Read the connected state without blocking the async runtime
        self.connected.try_read().map(|g| *g).unwrap_or(false)
    }

    async fn connect(&mut self) -> Result<()> {
        // Use println! for synchronous output to ensure we see this
        println!("[PERSONAL_WECHAT] connect() called");
        info!("🔌 PersonalWeChatChannel::connect() 被调用");

        // Check if already connected
        println!("[PERSONAL_WECHAT] Getting connected lock...");
        let already_connected = *self.connected.read().await;
        println!("[PERSONAL_WECHAT] Got connected={}", already_connected);

        println!("[PERSONAL_WECHAT] Checking session valid...");
        let session_valid = self.is_session_valid().await;
        println!("[PERSONAL_WECHAT] Session valid={}", session_valid);
        info!(
            "连接状态检查: connected={}, session_valid={}",
            already_connected, session_valid
        );

        if already_connected && session_valid {
            info!("个人微信已连接且会话有效");
            return Ok(());
        }

        // Try to restore persisted session first
        if !already_connected && !session_valid {
            if let Some(session) = self.load_session().await {
                info!("从持久化存储恢复个人微信 session");
                *self.session.write().await = Some(session);
                *self.connected.write().await = true;
                self.start_reconnect_monitor();
                info!("使用持久化 session 连接到个人微信");
                return Ok(());
            }
        }

        // If we have existing token in config (non-empty), try to use it
        if let Some(ref token) = self.config.bot_token {
            info!("检测到 bot_token 配置，长度={}", token.len());
            if !token.is_empty() {
                let session = BotSession {
                    bot_token: token.clone(),
                    base_url: self
                        .config
                        .bot_base_url
                        .clone()
                        .unwrap_or_else(|| self.config.base_url.clone()),
                    login_time: std::time::Instant::now(), // Assume just logged in
                    wxid: None,
                    nickname: None,
                };

                *self.session.write().await = Some(session);
                *self.connected.write().await = true;
                self.save_session().await;

                // Start reconnection monitor
                self.start_reconnect_monitor();

                info!("使用现有 bot_token 连接到个人微信");
                return Ok(());
            }
        } else {
            info!("没有配置 bot_token，需要 QR 码登录");
        }

        // Need QR login
        info!("🔄 开始获取 QR 码...");
        let qr_resp = match self.get_qr_code().await {
            Ok(resp) => {
                info!("✅ 成功获取 QR 码响应");
                resp
            }
            Err(e) => {
                error!("❌ 获取 QR 码失败: {}", e);
                return Err(e);
            }
        };

        println!("[PERSONAL_WECHAT] Printing QR code info...");
        info!("========================================");
        info!("个人微信登录");
        info!("========================================");
        info!("请使用微信扫描以下二维码或打开链接:");
        if let Some(ref url) = qr_resp.qrcode_img_content {
            info!("链接: {}", url);
            println!("[PERSONAL_WECHAT] QR URL: {}", url);
        }
        info!("QR Code: {}", qr_resp.qrcode);
        info!("========================================");

        info!("🕐 等待用户扫码（不阻塞 Gateway 启动）...");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        // Stop listener
        if let Some(handle) = self.listener_handle.write().await.take() {
            handle.abort();
        }

        // Stop reconnection monitor
        if let Some(handle) = self.reconnect_handle.write().await.take() {
            handle.abort();
        }

        *self.connected.write().await = false;
        info!("个人微信已断开连接");
        Ok(())
    }

    async fn send(&self, channel_id: &str, message: &Message) -> Result<()> {
        if !self.is_session_valid().await {
            return Err(AgentError::platform("会话已过期，请重新登录").into());
        }

        let text = match message.message_type {
            MessageType::Text => message.content.clone(),
            _ => {
                warn!("个人微信仅支持文本消息，已转换");
                message.content.clone()
            }
        };

        self.send_message_internal(channel_id, &text).await
    }

    async fn start_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        self.stop_listener().await?;

        if !*self.connected.read().await {
            // If we have QR login info, poll for scan status in background
            if self.qr_login_info.read().await.is_some() {
                let channel = self.clone();
                let handle = tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(Duration::from_secs(3)).await;
                        match channel.check_qr_status().await {
                            Ok(true) => {
                                info!("✅ 用户扫码成功，启动个人微信消息监听");
                                if let Err(e) = channel.start_listener(event_bus).await {
                                    error!("启动个人微信消息监听失败: {}", e);
                                }
                                break;
                            }
                            Ok(false) => {}
                            Err(e) => {
                                error!("QR 状态检查失败: {}", e);
                                break;
                            }
                        }
                    }
                });
                *self.listener_handle.write().await = Some(handle);
                info!("个人微信 QR 状态轮询已启动，等待扫码后自动开启消息监听...");
                return Ok(());
            }
            return Err(AgentError::platform("未连接").into());
        }

        let channel = self.clone();
        let handle = tokio::spawn(async move {
            if let Err(e) = channel.poll_messages(event_bus).await {
                error!("消息轮询错误: {}", e);
            }
        });

        *self.listener_handle.write().await = Some(handle);
        info!("个人微信消息监听已启动");

        Ok(())
    }

    async fn stop_listener(&self) -> Result<()> {
        if let Some(handle) = self.listener_handle.write().await.take() {
            handle.abort();
            info!("消息监听已停止");
        }
        Ok(())
    }

    fn supported_content_types(&self) -> Vec<ContentType> {
        vec![
            ContentType::Text,
            ContentType::Image,
            ContentType::Audio,
            ContentType::Video,
        ]
    }

    async fn download_image(&self, file_key: &str, _message_id: Option<&str>) -> Result<Vec<u8>> {
        // file_key is the image URL for iLink
        info!("🖼️ 下载图片: {}", file_key);

        let session = self.session.read().await.clone();
        if let Some(session) = session {
            match self
                .ilink_client
                .download_media(&session.bot_token, Some(&session.base_url), file_key)
                .await
            {
                Ok(data) => {
                    info!("✅ 图片下载成功: {} bytes", data.len());
                    Ok(data)
                }
                Err(e) => {
                    error!("❌ 图片下载失败: {}", e);
                    Err(e)
                }
            }
        } else {
            Err(AgentError::platform("未登录，无法下载图片").into())
        }
    }

    async fn list_channels(&self) -> Result<Vec<ChannelInfo>> {
        Ok(vec![])
    }

    async fn list_members(&self, _channel_id: &str) -> Result<Vec<MemberInfo>> {
        Ok(vec![])
    }

    fn connection_mode(&self) -> ConnectionMode {
        ConnectionMode::Polling
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Clone for PersonalWeChatChannel {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            ilink_client: ILinkClient::new(Some(ILinkConfig {
                base_url: self.config.base_url.clone(),
                ..Default::default()
            })),
            connected: self.connected.clone(),
            session: self.session.clone(),
            qr_login_info: self.qr_login_info.clone(),
            typing_tickets: self.typing_tickets.clone(),
            last_contact: self.last_contact.clone(),
            reconnect_pending: self.reconnect_pending.clone(),
            listener_handle: Arc::new(RwLock::new(None)),
            reconnect_handle: Arc::new(RwLock::new(None)),
            event_sender: self.event_sender.clone(),
            welcomed_users: self.welcomed_users.clone(),
            session_store_path: self.session_store_path.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = PersonalWeChatConfig::default();
        assert_eq!(config.base_url, "https://ilinkai.weixin.qq.com");
        assert!(config.auto_reconnect);
        assert_eq!(config.reconnect_interval_secs, 300);
    }
}
