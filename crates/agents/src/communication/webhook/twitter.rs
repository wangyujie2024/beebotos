//! Twitter/X Webhook Handler
//!
//! Handles incoming webhooks from Twitter/X API for account activity events.
//! Supports CRC (Challenge-Response Check) for webhook validation.

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::communication::webhook::common::{compute_hmac_sha256, MetadataBuilder};
use crate::communication::webhook::{
    SignatureVerification, WebhookConfig, WebhookEvent, WebhookEventType, WebhookHandler,
};
use crate::communication::{Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

/// Twitter CRC challenge payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterCrcChallenge {
    #[serde(rename = "crc_token")]
    pub crc_token: String,
}

/// Twitter CRC response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterCrcResponse {
    #[serde(rename = "response_token")]
    pub response_token: String,
}

/// Twitter Account Activity webhook payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterWebhookPayload {
    #[serde(rename = "for_user_id")]
    pub for_user_id: String,
    #[serde(flatten)]
    pub event: TwitterEvent,
}

/// Twitter event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TwitterEvent {
    /// Tweet events
    TweetEvents {
        #[serde(rename = "tweet_create_events")]
        tweet_create_events: Option<Vec<TwitterTweet>>,
        #[serde(rename = "tweet_delete_events")]
        tweet_delete_events: Option<Vec<TwitterDeleteEvent>>,
    },
    /// Direct message events
    DirectMessageEvents {
        #[serde(rename = "direct_message_events")]
        direct_message_events: Option<Vec<TwitterDirectMessageEvent>>,
        #[serde(rename = "direct_message_indicate_typing_events")]
        typing_events: Option<Vec<serde_json::Value>>,
        #[serde(rename = "direct_message_mark_read_events")]
        read_events: Option<Vec<serde_json::Value>>,
        users: Option<HashMap<String, TwitterUser>>,
    },
    /// Follow events
    FollowEvents {
        #[serde(rename = "follow_events")]
        follow_events: Option<Vec<TwitterFollowEvent>>,
    },
    /// Favorite events
    FavoriteEvents {
        #[serde(rename = "favorite_events")]
        favorite_events: Option<Vec<TwitterFavoriteEvent>>,
    },
    /// Retweet events
    RetweetEvents {
        #[serde(rename = "retweeted_status")]
        retweet_events: Option<Vec<TwitterRetweetEvent>>,
    },
    /// Mention events
    MentionEvents {
        #[serde(rename = "tweet_create_events")]
        mention_events: Option<Vec<TwitterTweet>>,
    },
}

/// Twitter tweet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterTweet {
    #[serde(rename = "id")]
    pub id: u64,
    #[serde(rename = "id_str")]
    pub id_str: String,
    #[serde(rename = "created_at")]
    pub created_at: String,
    pub text: String,
    pub user: TwitterUser,
    pub entities: Option<TwitterEntities>,
    #[serde(rename = "extended_tweet")]
    pub extended_tweet: Option<TwitterExtendedTweet>,
    #[serde(rename = "in_reply_to_status_id")]
    pub in_reply_to_status_id: Option<u64>,
    #[serde(rename = "in_reply_to_user_id")]
    pub in_reply_to_user_id: Option<u64>,
    #[serde(rename = "quoted_status_id")]
    pub quoted_status_id: Option<u64>,
    #[serde(rename = "retweeted_status")]
    pub retweeted_status: Option<Box<TwitterTweet>>,
    #[serde(default, rename = "is_quote_status")]
    pub is_quote_status: bool,
}

/// Twitter user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterUser {
    pub id: u64,
    #[serde(rename = "id_str")]
    pub id_str: String,
    pub name: String,
    #[serde(rename = "screen_name")]
    pub screen_name: String,
    pub location: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "profile_image_url")]
    pub profile_image_url: Option<String>,
    #[serde(rename = "profile_image_url_https")]
    pub profile_image_url_https: Option<String>,
    pub verified: Option<bool>,
    #[serde(rename = "followers_count")]
    pub followers_count: Option<i32>,
    #[serde(rename = "friends_count")]
    pub friends_count: Option<i32>,
}

/// Twitter entities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterEntities {
    pub hashtags: Option<Vec<TwitterHashtag>>,
    pub urls: Option<Vec<TwitterUrl>>,
    #[serde(rename = "user_mentions")]
    pub user_mentions: Option<Vec<TwitterUserMention>>,
    pub media: Option<Vec<TwitterMedia>>,
}

/// Twitter hashtag
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterHashtag {
    pub text: String,
    pub indices: Vec<i32>,
}

/// Twitter URL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterUrl {
    pub url: String,
    #[serde(rename = "expanded_url")]
    pub expanded_url: String,
    #[serde(rename = "display_url")]
    pub display_url: String,
    pub indices: Vec<i32>,
}

/// Twitter user mention
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterUserMention {
    #[serde(rename = "screen_name")]
    pub screen_name: String,
    pub name: String,
    pub id: u64,
    #[serde(rename = "id_str")]
    pub id_str: String,
    pub indices: Vec<i32>,
}

/// Twitter media
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterMedia {
    pub id: u64,
    #[serde(rename = "id_str")]
    pub id_str: String,
    #[serde(rename = "media_url")]
    pub media_url: String,
    #[serde(rename = "media_url_https")]
    pub media_url_https: String,
    pub url: String,
    #[serde(rename = "display_url")]
    pub display_url: String,
    #[serde(rename = "expanded_url")]
    pub expanded_url: String,
    #[serde(rename = "type")]
    pub media_type: String,
}

/// Twitter extended tweet (for long tweets)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterExtendedTweet {
    #[serde(rename = "full_text")]
    pub full_text: String,
    pub entities: Option<TwitterEntities>,
    #[serde(rename = "extended_entities")]
    pub extended_entities: Option<TwitterExtendedEntities>,
}

/// Twitter extended entities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterExtendedEntities {
    pub media: Option<Vec<TwitterMedia>>,
}

/// Twitter delete event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterDeleteEvent {
    pub status: TwitterDeleteStatus,
    #[serde(rename = "timestamp_ms")]
    pub timestamp_ms: String,
}

/// Twitter delete status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterDeleteStatus {
    #[serde(rename = "user_id")]
    pub user_id: String,
    #[serde(rename = "id")]
    pub id: String,
}

/// Twitter direct message event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterDirectMessageEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub id: String,
    #[serde(rename = "created_timestamp")]
    pub created_timestamp: String,
    #[serde(rename = "message_create")]
    pub message_create: TwitterMessageCreate,
}

/// Twitter message create
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterMessageCreate {
    #[serde(rename = "target")]
    pub target: TwitterMessageTarget,
    #[serde(rename = "sender_id")]
    pub sender_id: String,
    #[serde(rename = "message_data")]
    pub message_data: TwitterMessageData,
}

/// Twitter message target
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterMessageTarget {
    #[serde(rename = "recipient_id")]
    pub recipient_id: String,
}

/// Twitter message data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterMessageData {
    pub text: String,
    pub entities: Option<TwitterEntities>,
    pub attachment: Option<TwitterAttachment>,
}

/// Twitter attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterAttachment {
    #[serde(rename = "type")]
    pub attachment_type: String,
    pub media: Option<TwitterMedia>,
}

/// Twitter follow event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterFollowEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(rename = "created_timestamp")]
    pub created_timestamp: String,
    pub target: TwitterUser,
    pub source: TwitterUser,
}

/// Twitter favorite event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterFavoriteEvent {
    #[serde(rename = "id")]
    pub id: String,
    #[serde(rename = "created_at")]
    pub created_at: String,
    #[serde(rename = "timestamp_ms")]
    pub timestamp_ms: String,
    pub favorited_status: TwitterTweet,
    pub user: TwitterUser,
}

/// Twitter retweet event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterRetweetEvent {
    #[serde(rename = "created_at")]
    pub created_at: String,
    pub id: u64,
    pub user: TwitterUser,
    #[serde(rename = "retweeted_status")]
    pub retweeted_status: TwitterTweet,
}

/// Twitter webhook handler
#[allow(dead_code)]
pub struct TwitterWebhookHandler {
    config: WebhookConfig,
    api_secret: String,
    consumer_key: String,
}

impl TwitterWebhookHandler {
    /// Create a new Twitter webhook handler
    ///
    /// # Arguments
    /// * `consumer_key` - Twitter API consumer key
    /// * `api_secret` - Twitter API consumer secret for CRC validation
    pub fn new(consumer_key: String, api_secret: String) -> Self {
        let mut config = WebhookConfig::default();
        config.platform = PlatformType::Twitter;
        config.endpoint_path = "/webhook/twitter".to_string();
        config.verify_signatures = true;

        Self {
            config,
            api_secret,
            consumer_key,
        }
    }

    /// Create handler from environment variables
    pub fn from_env() -> Result<Self> {
        let consumer_key = std::env::var("TWITTER_API_KEY")
            .map_err(|_| AgentError::configuration("TWITTER_API_KEY not set"))?;
        let api_secret = std::env::var("TWITTER_API_SECRET")
            .map_err(|_| AgentError::configuration("TWITTER_API_SECRET not set"))?;

        Ok(Self::new(consumer_key, api_secret))
    }

    /// Generate CRC response token
    ///
    /// Twitter CRC validation: response_token = sha256(crc_token +
    /// consumer_secret)
    pub fn generate_crc_response(&self, crc_token: &str) -> String {
        let computed = compute_hmac_sha256(&self.api_secret, crc_token.as_bytes(), "");
        format!("sha256={}", computed)
    }

    /// Parse Twitter event type
    #[allow(dead_code)]
    fn parse_event_type(&self, event: &TwitterEvent) -> WebhookEventType {
        match event {
            TwitterEvent::TweetEvents { .. } => WebhookEventType::MessageReceived,
            TwitterEvent::DirectMessageEvents { .. } => WebhookEventType::MessageReceived,
            TwitterEvent::FollowEvents { .. } => WebhookEventType::UserJoined,
            TwitterEvent::FavoriteEvents { .. } => WebhookEventType::MessageReceived,
            TwitterEvent::RetweetEvents { .. } => WebhookEventType::MessageReceived,
            TwitterEvent::MentionEvents { .. } => WebhookEventType::BotMentioned,
        }
    }

    /// Convert Twitter tweet to internal message
    fn convert_tweet_to_message(&self, tweet: &TwitterTweet, for_user_id: &str) -> Option<Message> {
        // Skip our own tweets
        if tweet.user.id_str == for_user_id {
            return None;
        }

        let content = tweet
            .extended_tweet
            .as_ref()
            .map(|et| et.full_text.clone())
            .unwrap_or_else(|| tweet.text.clone());

        let metadata = MetadataBuilder::new()
            .add("tweet_id", &tweet.id_str)
            .add("user_id", &tweet.user.id_str)
            .add("user_name", &tweet.user.name)
            .add("screen_name", &tweet.user.screen_name)
            .add_optional(
                "in_reply_to_status_id",
                tweet.in_reply_to_status_id.map(|id| id.to_string()),
            )
            .add_optional(
                "in_reply_to_user_id",
                tweet.in_reply_to_user_id.map(|id| id.to_string()),
            )
            .add_optional(
                "quoted_status_id",
                tweet.quoted_status_id.map(|id| id.to_string()),
            )
            .add_optional(
                "is_quote_status",
                if tweet.is_quote_status {
                    Some("true")
                } else {
                    None
                },
            )
            .add_optional(
                "is_retweet",
                if tweet.retweeted_status.is_some() {
                    Some("true")
                } else {
                    None
                },
            )
            .add_optional(
                "mentions",
                tweet.entities.as_ref().and_then(|e| {
                    e.user_mentions.as_ref().map(|mentions| {
                        mentions
                            .iter()
                            .map(|m| format!("@{}", m.screen_name))
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                }),
            )
            .build();

        Some(Message {
            id: uuid::Uuid::new_v4(),
            thread_id: uuid::Uuid::new_v4(),
            platform: PlatformType::Twitter,
            message_type: if tweet.in_reply_to_status_id.is_some() {
                MessageType::Reply
            } else {
                MessageType::Text
            },
            content,
            metadata,
            timestamp: chrono::Utc::now(),
        })
    }

    /// Convert Twitter DM to internal message
    fn convert_dm_to_message(
        &self,
        dm: &TwitterDirectMessageEvent,
        users: &Option<HashMap<String, TwitterUser>>,
        for_user_id: &str,
    ) -> Option<Message> {
        // Skip our own DMs
        if dm.message_create.sender_id == for_user_id {
            return None;
        }

        let sender = users.as_ref()?.get(&dm.message_create.sender_id)?;

        let metadata = MetadataBuilder::new()
            .add("dm_id", &dm.id)
            .add("sender_id", &dm.message_create.sender_id)
            .add("recipient_id", &dm.message_create.target.recipient_id)
            .add("sender_name", &sender.name)
            .add("sender_screen_name", &sender.screen_name)
            .add("is_dm", "true")
            .add_optional(
                "attachment_type",
                dm.message_create
                    .message_data
                    .attachment
                    .as_ref()
                    .map(|a| &a.attachment_type),
            )
            .build();

        Some(Message {
            id: uuid::Uuid::new_v4(),
            thread_id: uuid::Uuid::new_v4(),
            platform: PlatformType::Twitter,
            message_type: MessageType::Text,
            content: dm.message_create.message_data.text.clone(),
            metadata,
            timestamp: chrono::Utc::now(),
        })
    }
}

#[async_trait]
impl WebhookHandler for TwitterWebhookHandler {
    fn platform_type(&self) -> PlatformType {
        PlatformType::Twitter
    }

    async fn verify_signature(
        &self,
        _body: &[u8],
        _signature: Option<&str>,
        _timestamp: Option<&str>,
    ) -> Result<SignatureVerification> {
        // Twitter uses CRC challenge-response for webhook validation
        // Signature verification is done via the CRC mechanism
        Ok(SignatureVerification::Skipped)
    }

    async fn parse_payload(&self, body: &[u8]) -> Result<Vec<WebhookEvent>> {
        // First, try to parse as CRC challenge
        if let Ok(crc) = serde_json::from_slice::<TwitterCrcChallenge>(body) {
            debug!("Received Twitter CRC challenge");
            let response_token = self.generate_crc_response(&crc.crc_token);
            return Ok(vec![WebhookEvent {
                event_type: WebhookEventType::System,
                platform: PlatformType::Twitter,
                event_id: "crc_challenge".to_string(),
                timestamp: chrono::Utc::now(),
                payload: serde_json::json!({
                    "crc_token": crc.crc_token,
                    "response_token": response_token
                }),
                message: None,
                metadata: MetadataBuilder::new()
                    .add("crc_token", &crc.crc_token)
                    .add("response_token", &response_token)
                    .build(),
            }]);
        }

        // Parse as regular webhook payload
        let payload: TwitterWebhookPayload = serde_json::from_slice(body)
            .map_err(|e| AgentError::platform(format!("Failed to parse Twitter payload: {}", e)))?;

        debug!("Parsed Twitter payload for user: {}", payload.for_user_id);

        let mut events = Vec::new();

        match &payload.event {
            TwitterEvent::TweetEvents {
                tweet_create_events,
                ..
            } => {
                if let Some(tweets) = tweet_create_events {
                    for tweet in tweets {
                        if let Some(message) =
                            self.convert_tweet_to_message(tweet, &payload.for_user_id)
                        {
                            let event_type = if tweet
                                .entities
                                .as_ref()
                                .and_then(|e| e.user_mentions.as_ref())
                                .map(|m| m.iter().any(|um| um.id_str == payload.for_user_id))
                                .unwrap_or(false)
                            {
                                WebhookEventType::BotMentioned
                            } else {
                                WebhookEventType::MessageReceived
                            };

                            events.push(WebhookEvent {
                                event_type,
                                platform: PlatformType::Twitter,
                                event_id: tweet.id_str.clone(),
                                timestamp: chrono::Utc::now(),
                                payload: serde_json::to_value(tweet).unwrap_or_default(),
                                message: Some(message),
                                metadata: MetadataBuilder::new()
                                    .add("for_user_id", &payload.for_user_id)
                                    .build(),
                            });
                        }
                    }
                }
            }
            TwitterEvent::DirectMessageEvents {
                direct_message_events,
                users,
                ..
            } => {
                if let Some(dms) = direct_message_events {
                    for dm in dms {
                        if let Some(message) =
                            self.convert_dm_to_message(dm, users, &payload.for_user_id)
                        {
                            events.push(WebhookEvent {
                                event_type: WebhookEventType::MessageReceived,
                                platform: PlatformType::Twitter,
                                event_id: dm.id.clone(),
                                timestamp: chrono::DateTime::from_timestamp(
                                    dm.created_timestamp.parse::<i64>().unwrap_or(0) / 1000,
                                    0,
                                )
                                .unwrap_or_else(chrono::Utc::now),
                                payload: serde_json::to_value(dm).unwrap_or_default(),
                                message: Some(message),
                                metadata: MetadataBuilder::new()
                                    .add("for_user_id", &payload.for_user_id)
                                    .build(),
                            });
                        }
                    }
                }
            }
            TwitterEvent::FollowEvents { follow_events } => {
                if let Some(follows) = follow_events {
                    for follow in follows {
                        events.push(WebhookEvent {
                            event_type: WebhookEventType::UserJoined,
                            platform: PlatformType::Twitter,
                            event_id: format!("follow_{}", follow.created_timestamp),
                            timestamp: chrono::DateTime::from_timestamp(
                                follow.created_timestamp.parse::<i64>().unwrap_or(0) / 1000,
                                0,
                            )
                            .unwrap_or_else(chrono::Utc::now),
                            payload: serde_json::to_value(follow).unwrap_or_default(),
                            message: None,
                            metadata: MetadataBuilder::new()
                                .add("for_user_id", &payload.for_user_id)
                                .add("follower_id", &follow.source.id_str)
                                .build(),
                        });
                    }
                }
            }
            TwitterEvent::FavoriteEvents { favorite_events } => {
                if let Some(favorites) = favorite_events {
                    for favorite in favorites {
                        events.push(WebhookEvent {
                            event_type: WebhookEventType::MessageReceived,
                            platform: PlatformType::Twitter,
                            event_id: favorite.id.clone(),
                            timestamp: chrono::DateTime::from_timestamp(
                                favorite.timestamp_ms.parse::<i64>().unwrap_or(0) / 1000,
                                0,
                            )
                            .unwrap_or_else(chrono::Utc::now),
                            payload: serde_json::to_value(favorite).unwrap_or_default(),
                            message: None,
                            metadata: MetadataBuilder::new()
                                .add("for_user_id", &payload.for_user_id)
                                .add("favorited_by", &favorite.user.id_str)
                                .build(),
                        });
                    }
                }
            }
            _ => {
                debug!("Received unhandled Twitter event type");
            }
        }

        Ok(events)
    }

    async fn handle_event(&self, event: WebhookEvent) -> Result<()> {
        match event.event_type {
            WebhookEventType::MessageReceived => {
                if let Some(msg) = &event.message {
                    info!(
                        "Received message from Twitter: {} (type: {:?})",
                        msg.content, msg.message_type
                    );
                }
            }
            WebhookEventType::BotMentioned => {
                if let Some(msg) = &event.message {
                    info!("Bot mentioned on Twitter: {}", msg.content);
                }
            }
            WebhookEventType::UserJoined => {
                info!("New follower on Twitter");
            }
            WebhookEventType::System => {
                debug!("Received Twitter system event (CRC)");
            }
            _ => {
                debug!(
                    "Received unhandled event type from Twitter: {:?}",
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

/// Twitter webhook response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwitterWebhookResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "response_token")]
    pub response_token: Option<String>,
}

impl TwitterWebhookResponse {
    /// Create a CRC response
    pub fn crc_response(token: impl Into<String>) -> Self {
        Self {
            response_token: Some(token.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_twitter_webhook_response() {
        let crc_resp = TwitterWebhookResponse::crc_response("sha256=abc123");
        assert_eq!(crc_resp.response_token, Some("sha256=abc123".to_string()));
    }

    #[test]
    fn test_parse_crc_challenge() {
        let json = r#"{"crc_token":"test_token_123"}"#;
        let crc: TwitterCrcChallenge = serde_json::from_str(json).unwrap();
        assert_eq!(crc.crc_token, "test_token_123");
    }

    #[test]
    fn test_generate_crc_response() {
        let handler = TwitterWebhookHandler::new(
            "test_consumer_key".to_string(),
            "test_consumer_secret".to_string(),
        );

        let response = handler.generate_crc_response("test_token");
        assert!(response.starts_with("sha256="));
        assert_eq!(response.len(), 71); // "sha256=" + 64 hex chars
    }

    #[test]
    fn test_parse_tweet() {
        let json = r#"{
            "id": 1234567890,
            "id_str": "1234567890",
            "created_at": "Mon Jan 01 00:00:00 +0000 2024",
            "text": "Hello Twitter!",
            "user": {
                "id": 9876543210,
                "id_str": "9876543210",
                "name": "Test User",
                "screen_name": "testuser"
            },
            "entities": {
                "hashtags": [],
                "urls": [],
                "user_mentions": []
            }
        }"#;

        let tweet: TwitterTweet = serde_json::from_str(json).unwrap();
        assert_eq!(tweet.id_str, "1234567890");
        assert_eq!(tweet.text, "Hello Twitter!");
        assert_eq!(tweet.user.screen_name, "testuser");
    }

    #[test]
    fn test_parse_direct_message() {
        let json = r#"{
            "type": "message_create",
            "id": "1234567890",
            "created_timestamp": "1704067200000",
            "message_create": {
                "target": {
                    "recipient_id": "12345"
                },
                "sender_id": "67890",
                "message_data": {
                    "text": "Hello DM!"
                }
            }
        }"#;

        let dm: TwitterDirectMessageEvent = serde_json::from_str(json).unwrap();
        assert_eq!(dm.id, "1234567890");
        assert_eq!(dm.message_create.message_data.text, "Hello DM!");
    }
}
