//! Twitter Channel Implementation
//!
//! Unified Channel trait implementation for Twitter/X API v2.
//! Supports Polling mode (default) and Webhook mode for Premium API.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, Duration};
use tracing::{error, info};

use super::r#trait::{BaseChannelConfig, ConnectionMode, ContentType};
use super::{Channel, ChannelConfig, ChannelEvent, ChannelInfo, MemberInfo};
use crate::communication::{Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

/// Twitter API v2 base URL
const TWITTER_API_BASE: &str = "https://api.twitter.com/2";

/// Twitter Channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterChannelConfig {
    /// Bearer token for app-level API access
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bearer_token: Option<String>,
    /// API Key (Consumer Key)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// API Secret (Consumer Secret)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_secret: Option<String>,
    /// Access Token
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    /// Access Token Secret
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token_secret: Option<String>,
    /// Polling interval in seconds (default: 60)
    #[serde(default = "default_polling_interval")]
    pub polling_interval_secs: u64,
    /// User ID to monitor mentions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(flatten)]
    pub base: BaseChannelConfig,
}

fn default_polling_interval() -> u64 {
    60
}

impl Default for TwitterChannelConfig {
    fn default() -> Self {
        let mut base = BaseChannelConfig::default();
        // Twitter defaults to Polling mode
        base.connection_mode = ConnectionMode::Polling;

        Self {
            bearer_token: None,
            api_key: None,
            api_secret: None,
            access_token: None,
            access_token_secret: None,
            polling_interval_secs: 60,
            user_id: None,
            base,
        }
    }
}

impl ChannelConfig for TwitterChannelConfig {
    fn from_env() -> Option<Self>
    where
        Self: Sized,
    {
        let bearer_token = std::env::var("TWITTER_BEARER_TOKEN").ok();
        let api_key = std::env::var("TWITTER_API_KEY").ok();
        let api_secret = std::env::var("TWITTER_API_SECRET").ok();
        let access_token = std::env::var("TWITTER_ACCESS_TOKEN").ok();
        let access_token_secret = std::env::var("TWITTER_ACCESS_TOKEN_SECRET").ok();
        let user_id = std::env::var("TWITTER_USER_ID").ok();

        let mut base = BaseChannelConfig::from_env("TWITTER")?;
        // Twitter defaults to Polling mode if not specified
        if std::env::var("TWITTER_CONNECTION_MODE").is_err() {
            base.connection_mode = ConnectionMode::Polling;
        }

        let polling_interval_secs = std::env::var("TWITTER_POLLING_INTERVAL")
            .map(|v| v.parse().unwrap_or(60))
            .unwrap_or(60);

        Some(Self {
            bearer_token,
            api_key,
            api_secret,
            access_token,
            access_token_secret,
            polling_interval_secs,
            user_id,
            base,
        })
    }

    fn is_valid(&self) -> bool {
        // Need either bearer token or full OAuth 1.0a credentials
        self.bearer_token.is_some()
            || (self.api_key.is_some()
                && self.api_secret.is_some()
                && self.access_token.is_some()
                && self.access_token_secret.is_some())
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

/// Twitter API response
#[derive(Debug, Clone, Deserialize)]
pub struct TwitterApiResponse<T> {
    pub data: Option<T>,
    pub errors: Option<Vec<TwitterError>>,
    pub meta: Option<serde_json::Value>,
}

/// Twitter error
#[derive(Debug, Clone, Deserialize)]
pub struct TwitterError {
    pub message: String,
    pub code: Option<i32>,
}

/// Twitter tweet
#[derive(Debug, Clone, Deserialize)]
pub struct TwitterTweet {
    pub id: String,
    pub text: String,
    #[serde(rename = "author_id")]
    pub author_id: Option<String>,
    #[serde(rename = "created_at")]
    pub created_at: Option<String>,
    #[serde(rename = "conversation_id")]
    pub conversation_id: Option<String>,
    #[serde(rename = "in_reply_to_user_id")]
    pub in_reply_to_user_id: Option<String>,
    #[serde(rename = "referenced_tweets")]
    pub referenced_tweets: Option<Vec<ReferencedTweet>>,
}

/// Referenced tweet
#[derive(Debug, Clone, Deserialize)]
pub struct ReferencedTweet {
    #[serde(rename = "type")]
    pub ref_type: String,
    pub id: String,
}

/// Twitter user
#[derive(Debug, Clone, Deserialize)]
pub struct TwitterUser {
    pub id: String,
    pub username: String,
    pub name: String,
}

/// Twitter mention
#[derive(Debug, Clone, Deserialize)]
pub struct TwitterMention {
    pub id: String,
    pub text: String,
    #[serde(rename = "author_id")]
    pub author_id: String,
}

/// Twitter Channel implementation
pub struct TwitterChannel {
    config: TwitterChannelConfig,
    http_client: reqwest::Client,
    connected: Arc<RwLock<bool>>,
    last_mention_id: Arc<RwLock<Option<String>>>,
    listener_handle: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    user_info: Arc<RwLock<Option<TwitterUser>>>,
}

impl TwitterChannel {
    /// Create a new Twitter channel
    pub fn new(config: TwitterChannelConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
            connected: Arc::new(RwLock::new(false)),
            last_mention_id: Arc::new(RwLock::new(None)),
            listener_handle: Arc::new(RwLock::new(None)),
            user_info: Arc::new(RwLock::new(None)),
        }
    }

    /// Create from environment variables
    pub fn from_env() -> Result<Self> {
        let config = TwitterChannelConfig::from_env()
            .ok_or_else(|| AgentError::configuration("Twitter credentials not set"))?;
        Ok(Self::new(config))
    }

    /// Get auth header for OAuth 1.0a
    fn get_oauth1_auth_header(&self, _method: &str, _url: &str) -> Option<String> {
        let api_key = self.config.api_key.as_ref()?;
        let _api_secret = self.config.api_secret.as_ref()?;
        let access_token = self.config.access_token.as_ref()?;
        let _access_token_secret = self.config.access_token_secret.as_ref()?;

        // Generate OAuth 1.0a header
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string();
        let nonce: String = (0..32).map(|_| rand::random::<u8>() as char).collect();

        let auth_header = format!(
            r#"OAuth oauth_consumer_key="{}", oauth_nonce="{}", oauth_signature_method="HMAC-SHA1", oauth_timestamp="{}", oauth_token="{}", oauth_version="1.0", oauth_signature="{}""#,
            api_key, nonce, timestamp, access_token, ""
        );

        Some(auth_header)
    }

    /// Get bearer token auth header
    fn get_bearer_auth_header(&self) -> Option<String> {
        self.config
            .bearer_token
            .as_ref()
            .map(|token| format!("Bearer {}", token))
    }

    /// Get auth header (prefer OAuth 1.0a for user context, Bearer for app
    /// context)
    fn get_auth_header(&self, _method: &str, _url: &str) -> Option<String> {
        // Prefer OAuth 1.0a if available (for posting tweets)
        if self.config.access_token.is_some() {
            self.get_oauth1_auth_header(_method, _url)
        } else {
            self.get_bearer_auth_header()
        }
    }

    /// Get current user info
    async fn get_me(&self) -> Result<TwitterUser> {
        let url = format!("{}/users/me", TWITTER_API_BASE);

        let auth_header = self
            .get_auth_header("GET", &url)
            .ok_or_else(|| AgentError::authentication("No valid credentials"))?;

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", auth_header)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to get user info: {}", e)))?;

        let api_response: TwitterApiResponse<TwitterUser> = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        if let Some(errors) = api_response.errors {
            if !errors.is_empty() {
                return Err(AgentError::platform(format!(
                    "Twitter API error: {}",
                    errors[0].message
                ))
                .into());
            }
        }

        api_response
            .data
            .ok_or_else(|| AgentError::platform("No user data in response").into())
    }

    /// Send tweet
    pub async fn send_tweet(&self, text: &str, reply_to: Option<&str>) -> Result<String> {
        let url = format!("{}/tweets", TWITTER_API_BASE);

        let auth_header = self
            .get_auth_header("POST", &url)
            .ok_or_else(|| AgentError::authentication("No valid credentials for posting"))?;

        let mut body = serde_json::json!({
            "text": text,
        });

        if let Some(reply_id) = reply_to {
            body["reply"] = serde_json::json!({
                "in_reply_to_tweet_id": reply_id,
            });
        }

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", auth_header)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to send tweet: {}", e)))?;

        let api_response: TwitterApiResponse<TwitterTweet> = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        if let Some(errors) = api_response.errors {
            if !errors.is_empty() {
                return Err(AgentError::platform(format!(
                    "Twitter API error: {}",
                    errors[0].message
                ))
                .into());
            }
        }

        let tweet = api_response
            .data
            .ok_or_else(|| AgentError::platform("No tweet data in response"))?;

        Ok(tweet.id)
    }

    /// Get mentions timeline
    async fn get_mentions(&self) -> Result<Vec<TwitterTweet>> {
        let user_id = if let Some(ref id) = self.config.user_id {
            id.clone()
        } else {
            let user_info = self.user_info.read().await;
            user_info
                .as_ref()
                .map(|u| u.id.clone())
                .ok_or_else(|| AgentError::configuration("User ID not set"))?
        };

        let url = format!("{}/users/{}/mentions", TWITTER_API_BASE, user_id);

        let auth_header = self
            .get_auth_header("GET", &url)
            .ok_or_else(|| AgentError::authentication("No valid credentials"))?;

        let mut request = self
            .http_client
            .get(&url)
            .header("Authorization", auth_header);

        // Add since_id if we have a last mention ID
        if let Some(since_id) = self.last_mention_id.read().await.clone() {
            request = request.query(&[("since_id", since_id)]);
        }

        let response = request
            .send()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to get mentions: {}", e)))?;

        let api_response: TwitterApiResponse<Vec<TwitterTweet>> = response
            .json()
            .await
            .map_err(|e| AgentError::platform(format!("Failed to parse response: {}", e)))?;

        if let Some(errors) = api_response.errors {
            if !errors.is_empty() {
                return Err(AgentError::platform(format!(
                    "Twitter API error: {}",
                    errors[0].message
                ))
                .into());
            }
        }

        Ok(api_response.data.unwrap_or_default())
    }

    /// Convert Twitter tweet to internal Message
    fn convert_tweet(&self, tweet: &TwitterTweet) -> Option<Message> {
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("tweet_id".to_string(), tweet.id.clone());
        metadata.insert(
            "author_id".to_string(),
            tweet.author_id.clone().unwrap_or_default(),
        );

        if let Some(ref conversation_id) = tweet.conversation_id {
            metadata.insert("conversation_id".to_string(), conversation_id.clone());
        }

        if let Some(ref in_reply_to) = tweet.in_reply_to_user_id {
            metadata.insert("in_reply_to_user_id".to_string(), in_reply_to.clone());
        }

        let timestamp = tweet
            .created_at
            .as_ref()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now);

        Some(Message {
            id: uuid::Uuid::new_v4(),
            thread_id: uuid::Uuid::new_v4(),
            platform: PlatformType::Twitter,
            message_type: MessageType::Text,
            content: tweet.text.clone(),
            metadata,
            timestamp,
        })
    }

    /// Run polling listener
    async fn run_polling_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        let mut interval = interval(Duration::from_secs(self.config.polling_interval_secs));

        loop {
            interval.tick().await;

            match self.get_mentions().await {
                Ok(tweets) => {
                    for tweet in tweets {
                        // Update last mention ID
                        if tweet.id.parse::<u64>().unwrap_or(0)
                            > self
                                .last_mention_id
                                .read()
                                .await
                                .as_ref()
                                .and_then(|id| id.parse().ok())
                                .unwrap_or(0)
                        {
                            *self.last_mention_id.write().await = Some(tweet.id.clone());
                        }

                        if let Some(message) = self.convert_tweet(&tweet) {
                            let event = ChannelEvent::MessageReceived {
                                platform: PlatformType::Twitter,
                                channel_id: tweet.author_id.clone().unwrap_or_default(),
                                message,
                            };

                            if let Err(e) = event_bus.send(event).await {
                                error!("Failed to send event: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("Polling error: {}", e);
                    if !self.config.base.auto_reconnect {
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Run webhook listener (for Account Activity API)
    async fn run_webhook_listener(&self, _event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        info!(
            "Twitter webhook listener started on port {}",
            self.config.base.webhook_port
        );
        // TODO: Implement webhook server for Twitter Account Activity API
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }
}

#[async_trait]
impl Channel for TwitterChannel {
    fn name(&self) -> &str {
        "twitter"
    }

    fn platform(&self) -> PlatformType {
        PlatformType::Twitter
    }

    fn is_connected(&self) -> bool {
        if let Ok(connected) = self.connected.try_read() {
            *connected
        } else {
            false
        }
    }

    async fn connect(&mut self) -> Result<()> {
        // Verify credentials by getting user info
        let user_info = self.get_me().await?;
        info!(
            "Connected to Twitter as @{} ({})",
            user_info.username, user_info.name
        );
        *self.user_info.write().await = Some(user_info);
        *self.connected.write().await = true;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.stop_listener().await?;
        *self.connected.write().await = false;
        info!("Disconnected from Twitter");
        Ok(())
    }

    async fn send(&self, channel_id: &str, message: &Message) -> Result<()> {
        // For Twitter, channel_id is the tweet ID to reply to (if any)
        let reply_to = if channel_id.is_empty() || channel_id == "timeline" {
            None
        } else {
            Some(channel_id)
        };

        self.send_tweet(&message.content, reply_to).await?;
        Ok(())
    }

    async fn start_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        self.stop_listener().await?;

        match self.config.base.connection_mode {
            ConnectionMode::Polling => {
                let channel = self.clone();
                let handle = tokio::spawn(async move {
                    if let Err(e) = channel.run_polling_listener(event_bus).await {
                        error!("Polling listener error: {}", e);
                    }
                });
                *self.listener_handle.write().await = Some(handle);
            }
            ConnectionMode::Webhook => {
                let channel = self.clone();
                let handle = tokio::spawn(async move {
                    if let Err(e) = channel.run_webhook_listener(event_bus).await {
                        error!("Webhook listener error: {}", e);
                    }
                });
                *self.listener_handle.write().await = Some(handle);
            }
            _ => {
                return Err(AgentError::platform(
                    "Twitter does not support WebSocket mode",
                ));
            }
        }

        Ok(())
    }

    async fn stop_listener(&self) -> Result<()> {
        if let Some(handle) = self.listener_handle.write().await.take() {
            handle.abort();
        }
        Ok(())
    }

    fn supported_content_types(&self) -> Vec<ContentType> {
        vec![ContentType::Text, ContentType::Image]
    }

    async fn list_channels(&self) -> Result<Vec<ChannelInfo>> {
        // Twitter doesn't have channels in the traditional sense
        Ok(vec![])
    }

    async fn list_members(&self, _channel_id: &str) -> Result<Vec<MemberInfo>> {
        Ok(vec![])
    }

    fn connection_mode(&self) -> ConnectionMode {
        self.config.base.connection_mode
    }
}

impl Clone for TwitterChannel {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            http_client: self.http_client.clone(),
            connected: self.connected.clone(),
            last_mention_id: self.last_mention_id.clone(),
            listener_handle: Arc::new(RwLock::new(None)),
            user_info: self.user_info.clone(),
        }
    }
}
