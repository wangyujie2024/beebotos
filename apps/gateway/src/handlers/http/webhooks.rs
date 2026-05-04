//! Webhook Handlers
//!
//! HTTP handlers for receiving webhooks from various messaging platforms.
//! Integrates with beebotos-agents webhook handlers for processing.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use beebotos_agents::communication::webhook::{
    DingTalkWebhookHandler, DiscordWebhookHandler, IMessageWebhookHandler, LarkWebhookHandler,
    MatrixWebhookHandler, SignalWebhookHandler, SlackWebhookHandler, TeamsWebhookHandler,
    TelegramWebhookConfig, TelegramWebhookHandler, TwitterWebhookHandler, WeChatWebhookHandler,
    WebhookConfig, WebhookHandler, WebhookManager, WhatsAppWebhookHandler,
};
use beebotos_agents::communication::channel::ChannelEvent;
use beebotos_agents::communication::{AgentMessageDispatcher, PlatformType};
use serde_json::json;
use tracing::{debug, error, info, warn};

use crate::error::GatewayError;
use crate::AppState;

// WeChat signature verification
use sha1::{Digest, Sha1};

/// Helper function to create internal error with correlation_id
fn internal_error(message: impl Into<String>) -> GatewayError {
    GatewayError::Internal {
        message: message.into(),
        correlation_id: uuid::Uuid::new_v4().to_string(),
    }
}

/// Webhook handler state
#[derive(Clone)]
pub struct WebhookHandlerState {
    /// Webhook manager
    pub manager: Arc<WebhookManager>,
    /// Platform configurations
    pub configs: Arc<std::collections::HashMap<String, WebhookConfig>>,
}

impl WebhookHandlerState {
    /// Create new webhook handler state
    pub fn new() -> Self {
        let manager = Arc::new(WebhookManager::new());
        let configs = Arc::new(std::collections::HashMap::new());

        Self { manager, configs }
    }

    /// Register all platform handlers with an optional dispatcher.
    pub async fn register_handlers(
        &self,
        dispatcher: Option<Arc<AgentMessageDispatcher>>,
    ) -> Result<(), GatewayError> {
        // Register Lark handler
        if let Ok((verification_token, encrypt_key)) = load_lark_config() {
            let mut handler = LarkWebhookHandler::new(verification_token, encrypt_key);
            if let Some(ref d) = dispatcher {
                handler = handler.with_dispatcher(d.clone());
            }
            self.manager
                .register_handler(Arc::new(handler))
                .await
                .map_err(|e| internal_error(format!("Failed to register Lark handler: {}", e)))?;
            info!("Registered Lark webhook handler at /webhook/lark");
        }

        // Register DingTalk handler
        if let Ok((app_key, app_secret)) = load_dingtalk_config() {
            let mut handler = DingTalkWebhookHandler::new(app_key, app_secret, None);
            if let Some(ref d) = dispatcher {
                handler = handler.with_dispatcher(d.clone());
            }
            self.manager.register_handler(Arc::new(handler)).await.map_err(|e| {
                internal_error(format!("Failed to register DingTalk handler: {}", e))
            })?;
            info!("Registered DingTalk webhook handler at /webhook/dingtalk");
        }

        // Register Telegram handler
        if let Ok(telegram_config) = load_telegram_config() {
            let mut handler = TelegramWebhookHandler::new(telegram_config.clone());
            if let Some(ref d) = dispatcher {
                handler = handler.with_dispatcher(d.clone());
            }
            self.manager.register_handler(Arc::new(handler)).await.map_err(|e| {
                internal_error(format!("Failed to register Telegram handler: {}", e))
            })?;
            info!(
                "Registered Telegram webhook handler at {}",
                telegram_config.endpoint_path
            );
        }

        // Register Discord handler
        if let Ok(public_key) = load_discord_config() {
            let mut handler = DiscordWebhookHandler::new(public_key);
            if let Some(ref d) = dispatcher {
                handler = handler.with_dispatcher(d.clone());
            }
            self.manager.register_handler(Arc::new(handler)).await.map_err(|e| {
                internal_error(format!("Failed to register Discord handler: {}", e))
            })?;
            info!("Registered Discord webhook handler at /webhook/discord");
        }

        // Register Slack handler
        if let Ok(signing_secret) = load_slack_config() {
            let mut handler = SlackWebhookHandler::new(signing_secret);
            if let Some(ref d) = dispatcher {
                handler = handler.with_dispatcher(d.clone());
            }
            self.manager
                .register_handler(Arc::new(handler))
                .await
                .map_err(|e| internal_error(format!("Failed to register Slack handler: {}", e)))?;
            info!("Registered Slack webhook handler at /webhook/slack");
        }

        // Register WeChat handler
        if let Ok((corp_id, token, encoding_aes_key)) = load_wechat_config() {
            let mut handler = WeChatWebhookHandler::new(corp_id, token, encoding_aes_key);
            if let Some(ref d) = dispatcher {
                handler = handler.with_dispatcher(d.clone());
            }
            self.manager
                .register_handler(Arc::new(handler))
                .await
                .map_err(|e| internal_error(format!("Failed to register WeChat handler: {}", e)))?;
            info!("Registered WeChat webhook handler at /webhook/wechat");
        }

        // Register Teams handler
        if let Ok((app_id, app_password)) = load_teams_config() {
            let handler = Arc::new(TeamsWebhookHandler::new(app_id, app_password));
            self.manager
                .register_handler(handler)
                .await
                .map_err(|e| internal_error(format!("Failed to register Teams handler: {}", e)))?;
            info!("Registered Teams webhook handler at /webhook/teams");
        }

        // Register Twitter handler
        if let Ok((consumer_key, api_secret)) = load_twitter_config() {
            let handler = Arc::new(TwitterWebhookHandler::new(consumer_key, api_secret));
            self.manager.register_handler(handler).await.map_err(|e| {
                internal_error(format!("Failed to register Twitter handler: {}", e))
            })?;
            info!("Registered Twitter webhook handler at /webhook/twitter");
        }

        // Register WhatsApp handler
        if let Ok(config) = load_whatsapp_config() {
            let handler = Arc::new(WhatsAppWebhookHandler::new(config));
            self.manager.register_handler(handler).await.map_err(|e| {
                internal_error(format!("Failed to register WhatsApp handler: {}", e))
            })?;
            info!("Registered WhatsApp webhook handler at /webhook/whatsapp");
        }

        // Register Signal handler
        if let Ok(config) = load_signal_config() {
            let handler = Arc::new(SignalWebhookHandler::new(config));
            self.manager
                .register_handler(handler)
                .await
                .map_err(|e| internal_error(format!("Failed to register Signal handler: {}", e)))?;
            info!("Registered Signal webhook handler at /webhook/signal");
        }

        // Register Matrix handler
        if let Ok(config) = load_matrix_config() {
            let handler = Arc::new(MatrixWebhookHandler::new(config));
            self.manager
                .register_handler(handler)
                .await
                .map_err(|e| internal_error(format!("Failed to register Matrix handler: {}", e)))?;
            info!("Registered Matrix webhook handler at /_matrix/app/v1/transactions");
        }

        // Register iMessage handler
        if let Ok(config) = load_imessage_config() {
            let handler = Arc::new(IMessageWebhookHandler::new(config));
            self.manager.register_handler(handler).await.map_err(|e| {
                internal_error(format!("Failed to register iMessage handler: {}", e))
            })?;
            info!("Registered iMessage webhook handler at /webhook/imessage");
        }

        Ok(())
    }
}

impl Default for WebhookHandlerState {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract webhook signature headers based on platform
fn extract_signature_headers<'a>(platform: &'a str, headers: &'a HeaderMap) -> (Option<&'a str>, Option<&'a str>) {
    let platform_lower = platform.to_lowercase();
    
    match platform_lower.as_str() {
        // Slack: X-Slack-Signature, X-Slack-Request-Timestamp
        "slack" => {
            let sig = headers
                .get("X-Slack-Signature")
                .or_else(|| headers.get("x-slack-signature"))
                .and_then(|v| v.to_str().ok());
            let ts = headers
                .get("X-Slack-Request-Timestamp")
                .or_else(|| headers.get("x-slack-request-timestamp"))
                .and_then(|v| v.to_str().ok());
            (sig, ts)
        }
        // Discord: X-Signature-Ed25519, X-Signature-Timestamp
        "discord" => {
            let sig = headers
                .get("X-Signature-Ed25519")
                .or_else(|| headers.get("x-signature-ed25519"))
                .and_then(|v| v.to_str().ok());
            let ts = headers
                .get("X-Signature-Timestamp")
                .or_else(|| headers.get("x-signature-timestamp"))
                .and_then(|v| v.to_str().ok());
            (sig, ts)
        }
        // Telegram: X-Telegram-Bot-Api-Secret-Token (not signature, but secret token)
        "telegram" => {
            let token = headers
                .get("X-Telegram-Bot-Api-Secret-Token")
                .or_else(|| headers.get("x-telegram-bot-api-secret-token"))
                .and_then(|v| v.to_str().ok());
            (token, None)
        }
        // DingTalk: timestamp header
        "dingtalk" => {
            let ts = headers
                .get("timestamp")
                .and_then(|v| v.to_str().ok());
            // Signature in query or body for DingTalk
            (None, ts)
        }
        // Generic fallback
        _ => {
            let sig = headers
                .get("X-Signature")
                .or_else(|| headers.get("x-signature"))
                .and_then(|v| v.to_str().ok());
            let ts = headers
                .get("X-Timestamp")
                .or_else(|| headers.get("x-timestamp"))
                .and_then(|v| v.to_str().ok());
            (sig, ts)
        }
    }
}

/// Generic webhook handler endpoint
pub async fn webhook_handler(
    State(state): State<Arc<AppState>>,
    Path(platform): Path<String>,
    headers: HeaderMap,
    body: String,
) -> Result<impl IntoResponse, GatewayError> {
    let path = format!("/webhook/{}", platform);

    debug!("Received webhook for platform: {}", platform);

    // Get signature from headers (platform-specific)
    let (signature, timestamp) = extract_signature_headers(&platform, &headers);

    // Get webhook state from app state - handlers are registered in AppState::new()
    let webhook_state = state.webhook_state.read().await;

    // Handle the request
    match webhook_state
        .manager
        .handle_request(&path, body.as_bytes(), signature, timestamp)
        .await
    {
        Ok(events) => {
            info!("Processed {} webhook events for {}", events.len(), platform);

            // Process events through the agent system
            for event in events {
                if let Some(message) = event.message {
                    // Send to agent for processing
                    if let Err(e) = process_message(state.clone(), message).await {
                        error!("Failed to process message: {}", e);
                    }
                }
            }

            Ok((StatusCode::OK, Json(json!({ "status": "ok" }))))
        }
        Err(e) => {
            error!("Webhook processing error: {}", e);
            Err(GatewayError::BadRequest {
                message: format!("Webhook error: {}", e),
                field: None,
            })
        }
    }
}

/// Lark-specific webhook handler (for URL verification challenge)
pub async fn lark_webhook_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: String,
) -> Result<(StatusCode, Json<serde_json::Value>), GatewayError> {
    debug!("Received Lark webhook");

    // Check if this is a URL verification challenge
    if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&body) {
        if let Some(challenge) = payload.get("challenge").and_then(|v| v.as_str()) {
            // Return the challenge for URL verification
            return Ok((StatusCode::OK, Json(json!({ "challenge": challenge }))));
        }
    }

    // Otherwise, process as normal webhook
    webhook_handler(State(state), Path("lark".to_string()), headers, body)
        .await
        .map(|_response| (StatusCode::OK, Json(json!({ "status": "ok" }))))
}

/// WeChat GET handler for URL verification (echostr)
pub async fn wechat_get_handler(
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<(StatusCode, String), GatewayError> {
    use beebotos_agents::communication::webhook::WeChatWebhookHandler;

    debug!("Received WeChat GET request, params: {:?}", params);

    // URL verification request must have echostr
    let echostr = params.get("echostr").ok_or_else(|| GatewayError::BadRequest {
        message: "Missing echostr parameter".to_string(),
        field: Some("echostr".to_string()),
    })?;

    let msg_signature = params.get("msg_signature").ok_or_else(|| GatewayError::BadRequest {
        message: "Missing msg_signature parameter".to_string(),
        field: Some("msg_signature".to_string()),
    })?;

    let timestamp = params.get("timestamp").ok_or_else(|| GatewayError::BadRequest {
        message: "Missing timestamp parameter".to_string(),
        field: Some("timestamp".to_string()),
    })?;

    let nonce = params.get("nonce").ok_or_else(|| GatewayError::BadRequest {
        message: "Missing nonce parameter".to_string(),
        field: Some("nonce".to_string()),
    })?;

    info!("Handling WeChat URL verification request");

    // Load WeChat config from environment
    if let Ok((corp_id, token, encoding_aes_key)) = load_wechat_config() {
        let handler = WeChatWebhookHandler::new(corp_id, token, encoding_aes_key);

        match handler.handle_verification(msg_signature, timestamp, nonce, echostr) {
            Ok(plain_text) => {
                info!("WeChat URL verification successful, returning plaintext");
                Ok((StatusCode::OK, plain_text))
            }
            Err(e) => {
                error!("WeChat URL verification failed: {}", e);
                Err(GatewayError::BadRequest {
                    message: format!("Verification failed: {}", e),
                    field: None,
                })
            }
        }
    } else {
        warn!("WeChat config not available for URL verification");
        Err(GatewayError::service_unavailable("WeChat", "WeChat not configured"))
    }
}

/// WeChat POST handler for message push
/// WeChat POST handler - internal implementation
/// This is called from a closure in main.rs that has access to app_state
pub async fn wechat_post_handler_impl(
    state: Arc<AppState>,
    msg_signature: &str,
    timestamp: &str,
    nonce: &str,
    body: &str,
) -> &'static str {
    info!("Processing WeChat message webhook");

    debug!(
        "WeChat signature params: sig={}, ts={}, nonce={}",
        msg_signature, timestamp, nonce
    );

    // Load WeChat config and verify signature
    use beebotos_agents::communication::webhook::WeChatWebhookHandler;

    if let Ok((corp_id, token, encoding_aes_key)) = load_wechat_config() {
        // Extract Encrypt field from XML body
        let msg_encrypt = extract_xml_value(body, "Encrypt").unwrap_or_default();

        // Compute expected signature
        let expected_sig = compute_wechat_signature(&token, timestamp, nonce, &msg_encrypt);

        if msg_signature != expected_sig {
            warn!("WeChat signature mismatch: expected={}, got={}", expected_sig, msg_signature);
            return "success"; // Return success to avoid retries
        }

        // Parse and process message
        let handler = WeChatWebhookHandler::new(corp_id, token, encoding_aes_key);

        match handler.parse_payload(body.as_bytes()).await {
            Ok(events) => {
                info!("Received {} WeChat events, processing async", events.len());

                // Process messages asynchronously to avoid blocking webhook response
                // WeChat requires quick response (< 5s), otherwise it will retry
                let state_clone = state.clone();
                tokio::spawn(async move {
                    for event in events {
                        if let Some(message) = event.message {
                            if let Err(e) = process_message_async(state_clone.clone(), message).await {
                                error!("Failed to process message: {}", e);
                            }
                        }
                    }
                });
            }
            Err(e) => {
                error!("Failed to parse WeChat payload: {}", e);
            }
        }
    }

    // Return success immediately to prevent WeChat retries
    "success"
}

/// Helper function to extract value from XML
/// Handles both <tag>value</tag> and <tag><![CDATA[value]]></tag> formats
fn extract_xml_value(xml: &str, tag: &str) -> Option<String> {
    let start_tag = format!("<{}>", tag);
    let end_tag = format!("</{}>", tag);

    if let Some(start) = xml.find(&start_tag) {
        if let Some(end) = xml.find(&end_tag) {
            let start_idx = start + start_tag.len();
            if start_idx < end {
                let value = &xml[start_idx..end];
                // Strip CDATA wrapper if present
                if value.starts_with("<![CDATA[") && value.ends_with("]]>") {
                    return Some(value[9..value.len()-3].to_string());
                }
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Helper function to compute WeChat signature
fn compute_wechat_signature(token: &str, timestamp: &str, nonce: &str, msg_encrypt: &str) -> String {
    let mut params = vec![
        token.to_string(),
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

/// Telegram webhook setup handler
#[allow(dead_code)]
pub async fn telegram_setup_handler(
    State(_state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, GatewayError> {
    // TODO: Implement Telegram webhook setup via Bot API
    Ok((StatusCode::OK, Json(json!({ "status": "ok" }))))
}

/// Process incoming message through the agent system
async fn process_message(
    state: Arc<AppState>,
    message: beebotos_agents::communication::Message,
) -> Result<(), GatewayError> {
    debug!(
        "Processing message on platform {:?}: content length {}",
        message.platform,
        message.content.len()
    );

    // Extract channel_id from metadata for reply
    // P1 FIX: Support platform-specific equivalents (chat_id, room_id, conversation_id)
    let channel_id = message
        .metadata
        .get("channel_id")
        .or_else(|| message.metadata.get("chat_id"))
        .or_else(|| message.metadata.get("room_id"))
        .or_else(|| message.metadata.get("conversation_id"))
        .or_else(|| message.metadata.get("sender_id")) // DingTalk / WeChat fallback
        .cloned()
        .unwrap_or_default();

    if channel_id.is_empty() {
        warn!("No channel_id found in message metadata, cannot send reply");
        return Ok(());
    }

    // Get sender_id for reply (the user who sent the message)
    let sender_id = message
        .metadata
        .get("sender_id")
        .or_else(|| message.metadata.get("from_user"))
        .cloned()
        .unwrap_or_else(|| channel_id.clone());

    // 🟢 P0 FIX: Route through new architecture if channel event bus is available
    if let Some(ref event_bus) = state.channel_event_bus {
        info!(
            "Routing webhook message from {} on {:?} through channel_event_bus (new architecture)",
            sender_id, message.platform
        );
        let event = ChannelEvent::MessageReceived {
            platform: message.platform,
            channel_id: channel_id.clone(),
            message: message.clone(),
        };
        if let Err(e) = event_bus.send(event).await {
            error!("Failed to send message to channel event bus: {}", e);
            return Err(GatewayError::internal(format!(
                "Failed to route message to agent system: {}",
                e
            )));
        }
        return Ok(());
    }

    // Fallback: direct LLM processing (legacy path)
    warn!(
        "channel_event_bus not available, falling back to direct LLM processing for {:?}",
        message.platform
    );

    // Call LLM to generate response
    let reply_content = match state.llm_service.process_message(&message).await {
        Ok(response) => {
            info!(
                "Generated response for {:?} message: content_length={}",
                message.platform, response.len()
            );
            response
        }
        Err(e) => {
            error!("Failed to process message with LLM: {}", e);
            "抱歉，我暂时无法处理您的消息。请稍后再试。\nSorry, I'm unable to process your message at the moment. Please try again later.".to_string()
        }
    };

    // Send reply via ChannelRegistry
    info!("Attempting to send reply via ChannelRegistry, platform: {:?}", message.platform);
    if let Some(ref registry) = state.channel_registry {
        info!("ChannelRegistry is available, looking up channel...");
        match registry.get_channel_by_platform(message.platform).await {
            Some(channel) => {
                info!("Found channel for platform {:?}, sending reply...", message.platform);
                // Create reply message
                let reply = beebotos_agents::communication::Message {
                    id: uuid::Uuid::new_v4(),
                    thread_id: message.thread_id,
                    platform: message.platform,
                    message_type: beebotos_agents::communication::MessageType::Text,
                    content: reply_content,
                    metadata: {
                        let mut meta = std::collections::HashMap::new();
                        meta.insert("to_user".to_string(), sender_id.clone());
                        meta.insert("reply_to".to_string(), message.id.to_string());
                        meta
                    },
                    timestamp: chrono::Utc::now(),
                };

                match channel.read().await.send(&sender_id, &reply).await {
                    Ok(_) => {
                        info!("Reply sent successfully to {} on {:?}", sender_id, message.platform);
                    }
                    Err(e) => {
                        error!("Failed to send reply via channel: {}", e);
                    }
                }
            }
            None => {
                error!("No channel found for platform {:?}", message.platform);
            }
        }
    } else {
        warn!("Channel registry not available, cannot send reply");
    }

    Ok(())
}

// Configuration loaders

fn load_lark_config() -> Result<(String, Option<String>), Box<dyn std::error::Error>> {
    let verification_token =
        std::env::var("LARK_VERIFICATION_TOKEN").map_err(|_| "LARK_VERIFICATION_TOKEN not set")?;
    let encrypt_key = std::env::var("LARK_ENCRYPT_KEY").ok();

    Ok((verification_token, encrypt_key))
}

fn load_dingtalk_config() -> Result<(String, String), Box<dyn std::error::Error>> {
    let app_key = std::env::var("DINGTALK_APP_KEY").map_err(|_| "DINGTALK_APP_KEY not set")?;
    let app_secret =
        std::env::var("DINGTALK_APP_SECRET").map_err(|_| "DINGTALK_APP_SECRET not set")?;

    Ok((app_key, app_secret))
}

fn load_telegram_config() -> Result<TelegramWebhookConfig, Box<dyn std::error::Error>> {
    let bot_token =
        std::env::var("TELEGRAM_BOT_TOKEN").map_err(|_| "TELEGRAM_BOT_TOKEN not set")?;
    let secret_token = std::env::var("TELEGRAM_SECRET_TOKEN").ok();

    Ok(TelegramWebhookConfig {
        bot_token,
        secret_token,
        endpoint_path: "/webhook/telegram".to_string(),
        timeout_secs: 30,
        max_body_size: 10 * 1024 * 1024,
    })
}

fn load_discord_config() -> Result<String, Box<dyn std::error::Error>> {
    let public_key =
        std::env::var("DISCORD_PUBLIC_KEY").map_err(|_| "DISCORD_PUBLIC_KEY not set")?;

    Ok(public_key)
}

fn load_slack_config() -> Result<String, Box<dyn std::error::Error>> {
    let signing_secret =
        std::env::var("SLACK_SIGNING_SECRET").map_err(|_| "SLACK_SIGNING_SECRET not set")?;

    Ok(signing_secret)
}

fn load_wechat_config() -> Result<(String, String, Option<String>), Box<dyn std::error::Error + Send + Sync>> {
    let corp_id = std::env::var("WECHAT_CORP_ID").map_err(|_| "WECHAT_CORP_ID not set")?;
    let token = std::env::var("WECHAT_TOKEN").map_err(|_| "WECHAT_TOKEN not set")?;
    let encoding_aes_key = std::env::var("WECHAT_ENCODING_AES_KEY").ok();

    Ok((corp_id, token, encoding_aes_key))
}

fn load_teams_config() -> Result<(String, String), Box<dyn std::error::Error>> {
    let app_id = std::env::var("TEAMS_APP_ID").map_err(|_| "TEAMS_APP_ID not set")?;
    let app_password =
        std::env::var("TEAMS_APP_PASSWORD").map_err(|_| "TEAMS_APP_PASSWORD not set")?;

    Ok((app_id, app_password))
}

fn load_twitter_config() -> Result<(String, String), Box<dyn std::error::Error>> {
    let consumer_key =
        std::env::var("TWITTER_CONSUMER_KEY").map_err(|_| "TWITTER_CONSUMER_KEY not set")?;
    let api_secret =
        std::env::var("TWITTER_API_SECRET").map_err(|_| "TWITTER_API_SECRET not set")?;

    Ok((consumer_key, api_secret))
}

fn load_whatsapp_config() -> Result<WebhookConfig, Box<dyn std::error::Error>> {
    // WhatsApp Baileys bridge doesn't require configuration by default
    // It uses a local bridge
    if std::env::var("WHATSAPP_ENABLED").unwrap_or_default() != "true" {
        return Err("WhatsApp not enabled".into());
    }

    Ok(WebhookConfig {
        platform: PlatformType::WhatsApp,
        endpoint_path: "/webhook/whatsapp".to_string(),
        secret: std::env::var("WHATSAPP_WEBHOOK_SECRET").ok(),
        encryption_key: None,
        verify_signatures: std::env::var("WHATSAPP_WEBHOOK_SECRET").is_ok(),
        decrypt_messages: false,
        allowed_ips: vec!["127.0.0.1".to_string()], // Only accept from localhost
        timeout_secs: 30,
        max_body_size: 50 * 1024 * 1024, // 50MB for media
    })
}

fn load_signal_config() -> Result<WebhookConfig, Box<dyn std::error::Error>> {
    if std::env::var("SIGNAL_ENABLED").unwrap_or_default() != "true" {
        return Err("Signal not enabled".into());
    }

    Ok(WebhookConfig {
        platform: PlatformType::Signal,
        endpoint_path: "/webhook/signal".to_string(),
        secret: None,
        encryption_key: None,
        verify_signatures: false,
        decrypt_messages: false,
        allowed_ips: vec!["127.0.0.1".to_string()],
        timeout_secs: 30,
        max_body_size: 10 * 1024 * 1024,
    })
}

fn load_matrix_config() -> Result<WebhookConfig, Box<dyn std::error::Error>> {
    let hs_token = std::env::var("MATRIX_HS_TOKEN").ok();

    if hs_token.is_none() {
        return Err("Matrix configuration not found".into());
    }

    Ok(WebhookConfig {
        platform: PlatformType::Matrix,
        endpoint_path: "/_matrix/app/v1/transactions".to_string(),
        secret: hs_token,
        encryption_key: None,
        verify_signatures: false, // Token is in query param
        decrypt_messages: false,
        allowed_ips: vec![],
        timeout_secs: 30,
        max_body_size: 10 * 1024 * 1024,
    })
}

fn load_imessage_config() -> Result<WebhookConfig, Box<dyn std::error::Error>> {
    if std::env::var("IMESSAGE_ENABLED").unwrap_or_default() != "true" {
        return Err("iMessage not enabled".into());
    }

    Ok(WebhookConfig {
        platform: PlatformType::IMessage,
        endpoint_path: "/webhook/imessage".to_string(),
        secret: std::env::var("BLUEBUBBLES_PASSWORD").ok(),
        encryption_key: None,
        verify_signatures: false,
        decrypt_messages: false,
        allowed_ips: vec![], // BlueBubbles can be on any IP
        timeout_secs: 30,
        max_body_size: 50 * 1024 * 1024,
    })
}

/// Message deduplication cache
/// Stores recently processed message IDs to prevent duplicate processing
use std::time::{Duration, Instant};
use std::sync::LazyLock;
use tokio::sync::Mutex;

/// Entry in the deduplication cache
struct DedupEntry {
    id: String,
    timestamp: Instant,
}

impl DedupEntry {
    fn new(id: String) -> Self {
        Self {
            id,
            timestamp: Instant::now(),
        }
    }
}

/// Global message deduplication cache
/// Messages are cached for 5 minutes to handle WeChat retries
static PROCESSED_MESSAGES: LazyLock<Mutex<Vec<DedupEntry>>> =
    LazyLock::new(|| Mutex::new(Vec::with_capacity(1000)));

/// Check if a message has been processed recently
async fn is_message_processed(msg_id: &str) -> bool {
    let mut cache = PROCESSED_MESSAGES.lock().await;

    // Clean up old entries (older than 5 minutes)
    let now = Instant::now();
    let timeout = Duration::from_secs(300);
    cache.retain(|entry| now.duration_since(entry.timestamp) < timeout);

    // Check if message is in cache
    if cache.iter().any(|entry| entry.id == msg_id) {
        return true;
    }

    // Add to cache
    cache.push(DedupEntry::new(msg_id.to_string()));

    // Limit cache size
    if cache.len() > 1000 {
        cache.remove(0);
    }

    false
}

/// Process message asynchronously with deduplication
async fn process_message_async(
    state: Arc<AppState>,
    message: beebotos_agents::communication::Message,
) -> Result<(), GatewayError> {
    // Use WeChat msg_id for deduplication if available
    let dedup_key = message
        .metadata
        .get("msg_id")
        .cloned()
        .unwrap_or_else(|| message.id.to_string());

    // Check for duplicates
    if is_message_processed(&dedup_key).await {
        info!("Skipping duplicate message: {}", dedup_key);
        return Ok(());
    }

    // Process the message
    process_message(state, message).await
}
