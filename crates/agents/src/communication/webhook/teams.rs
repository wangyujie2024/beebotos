//! Microsoft Teams Webhook Handler
//!
//! Handles incoming webhooks from Microsoft Teams Bot Framework.
//! Supports JWT token validation, Activity message parsing, and event
//! processing.

use std::collections::HashMap;

use async_trait::async_trait;
use base64::Engine;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use super::common::MetadataBuilder;
use crate::communication::webhook::{
    SignatureVerification, WebhookConfig, WebhookEvent, WebhookEventType, WebhookHandler,
};
use crate::communication::{Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

/// Teams webhook payload types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TeamsWebhookPayload {
    /// Activity payload (Bot Framework)
    Activity {
        #[serde(rename = "type")]
        activity_type: String,
        #[serde(rename = "channelId")]
        channel_id: String,
        conversation: TeamsConversation,
        from: TeamsChannelAccount,
        recipient: TeamsChannelAccount,
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(rename = "textFormat")]
        text_format: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        attachments: Option<Vec<TeamsAttachment>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        entities: Option<Vec<serde_json::Value>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(rename = "replyToId")]
        reply_to_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(rename = "serviceUrl")]
        service_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(rename = "channelData")]
        channel_data: Option<TeamsChannelData>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(rename = "value")]
        value: Option<serde_json::Value>,
        #[serde(flatten)]
        extra: HashMap<String, serde_json::Value>,
    },
    /// Generic JSON payload
    Generic(serde_json::Value),
}

/// Teams conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamsConversation {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_group: Option<bool>,
    #[serde(rename = "conversationType")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_type: Option<String>,
    #[serde(rename = "tenantId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

/// Teams channel account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamsChannelAccount {
    pub id: String,
    pub name: String,
    #[serde(rename = "aadObjectId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aad_object_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// Teams attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamsAttachment {
    #[serde(rename = "contentType")]
    pub content_type: String,
    #[serde(rename = "contentUrl")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "thumbnailUrl")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
}

/// Teams channel data (Teams-specific information)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamsChannelData {
    #[serde(rename = "tenant")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant: Option<TeamsTenantInfo>,
    #[serde(rename = "team")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team: Option<TeamsTeamInfo>,
    #[serde(rename = "channel")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<TeamsChannelInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "eventType")]
    pub event_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "messageReaction")]
    pub message_reaction: Option<TeamsMessageReaction>,
}

/// Teams tenant info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamsTenantInfo {
    pub id: String,
}

/// Teams team info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamsTeamInfo {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "aadGroupId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aad_group_id: Option<String>,
}

/// Teams channel info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamsChannelInfo {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Teams message reaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamsMessageReaction {
    #[serde(rename = "type")]
    pub reaction_type: String,
    #[serde(rename = "user")]
    pub user: TeamsChannelAccount,
}

/// Teams event types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeamsEventType {
    /// Message received
    Message,
    /// Conversation update
    ConversationUpdate,
    /// Message reaction
    MessageReaction,
    /// Message delete
    MessageDelete,
    /// Typing indicator
    Typing,
    /// End of conversation
    EndOfConversation,
    /// Event
    Event,
    /// Invoke (for task modules)
    Invoke,
    /// Installation update
    InstallationUpdate,
    /// Unknown
    Unknown(String),
}

impl From<&str> for TeamsEventType {
    fn from(s: &str) -> Self {
        match s {
            "message" => Self::Message,
            "conversationUpdate" => Self::ConversationUpdate,
            "messageReaction" => Self::MessageReaction,
            "messageDelete" => Self::MessageDelete,
            "typing" => Self::Typing,
            "endOfConversation" => Self::EndOfConversation,
            "event" => Self::Event,
            "invoke" => Self::Invoke,
            "installationUpdate" => Self::InstallationUpdate,
            _ => Self::Unknown(s.to_string()),
        }
    }
}

/// JWT token claims for Bot Framework validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamsJwtClaims {
    /// Issuer
    pub iss: String,
    /// Audience
    pub aud: String,
    /// Subject
    pub sub: String,
    /// Expiration time
    pub exp: i64,
    /// Not before
    pub nbf: i64,
    /// Issued at
    pub iat: i64,
    /// App ID
    #[serde(rename = "appid")]
    pub app_id: Option<String>,
    /// Service URL
    #[serde(rename = "serviceurl")]
    pub service_url: Option<String>,
}

/// Teams webhook handler
#[allow(dead_code)]
pub struct TeamsWebhookHandler {
    config: WebhookConfig,
    app_id: String,
    app_password: String,
    verify_jwt: bool,
}

impl TeamsWebhookHandler {
    /// Create a new Teams webhook handler
    ///
    /// # Arguments
    /// * `app_id` - Microsoft App ID
    /// * `app_password` - Microsoft App Password
    pub fn new(app_id: String, app_password: String) -> Self {
        let mut config = WebhookConfig::default();
        config.platform = PlatformType::Teams;
        config.endpoint_path = "/webhook/teams".to_string();
        config.verify_signatures = true;

        Self {
            config,
            app_id,
            app_password,
            verify_jwt: true,
        }
    }

    /// Create handler from environment variables
    pub fn from_env() -> Result<Self> {
        let app_id = std::env::var("MICROSOFT_APP_ID")
            .map_err(|_| AgentError::configuration("MICROSOFT_APP_ID not set"))?;
        let app_password = std::env::var("MICROSOFT_APP_PASSWORD")
            .map_err(|_| AgentError::configuration("MICROSOFT_APP_PASSWORD not set"))?;

        Ok(Self::new(app_id, app_password))
    }

    /// Disable JWT verification (for testing)
    pub fn disable_jwt_verification(mut self) -> Self {
        self.verify_jwt = false;
        self
    }

    /// Parse Teams event type
    fn parse_event_type(&self, activity_type: &str) -> WebhookEventType {
        match activity_type {
            "message" => WebhookEventType::MessageReceived,
            "conversationUpdate" => WebhookEventType::UserJoined,
            "messageReaction" => WebhookEventType::MessageReceived,
            "messageDelete" => WebhookEventType::MessageDeleted,
            "typing" => WebhookEventType::System,
            "endOfConversation" => WebhookEventType::System,
            "event" => WebhookEventType::System,
            "invoke" => WebhookEventType::MessageReceived,
            "installationUpdate" => WebhookEventType::System,
            _ => WebhookEventType::Unknown,
        }
    }

    /// Convert Teams activity to internal message
    fn convert_activity_to_message(&self, payload: &TeamsWebhookPayload) -> Option<Message> {
        match payload {
            TeamsWebhookPayload::Activity {
                activity_type,
                text,
                from,
                conversation,
                id,
                service_url,
                channel_data,
                attachments,
                reply_to_id,
                ..
            } => {
                // Skip bot's own messages
                if from.role.as_deref() == Some("bot") {
                    return None;
                }

                let content = text.clone().unwrap_or_default();

                let mut metadata_builder = MetadataBuilder::new()
                    .add("user_id", from.id.clone())
                    .add("user_name", from.name.clone())
                    .add("conversation_id", conversation.id.clone())
                    .add("activity_type", activity_type.clone())
                    .add_optional("aad_object_id", from.aad_object_id.clone())
                    .add_optional("service_url", service_url.clone())
                    .add_optional("activity_id", id.clone())
                    .add_optional("reply_to_id", reply_to_id.clone());

                // Extract team and channel info from channelData
                if let Some(channel_data) = channel_data {
                    if let Some(team) = &channel_data.team {
                        metadata_builder = metadata_builder.add("team_id", team.id.clone());
                        metadata_builder =
                            metadata_builder.add_optional("team_name", team.name.clone());
                    }
                    if let Some(channel) = &channel_data.channel {
                        metadata_builder = metadata_builder.add("channel_id", channel.id.clone());
                        metadata_builder =
                            metadata_builder.add_optional("channel_name", channel.name.clone());
                    }
                    if let Some(tenant) = &channel_data.tenant {
                        metadata_builder = metadata_builder.add("tenant_id", tenant.id.clone());
                    }
                    if let Some(event_type) = &channel_data.event_type {
                        metadata_builder =
                            metadata_builder.add("teams_event_type", event_type.clone());
                    }
                }

                // Handle attachments
                let mut metadata = metadata_builder.build();
                if let Some(attachs) = attachments {
                    let attachment_info: Vec<String> = attachs
                        .iter()
                        .map(|a| {
                            format!(
                                "{}:{}",
                                a.content_type,
                                a.name.as_deref().unwrap_or("unnamed")
                            )
                        })
                        .collect();
                    if !attachment_info.is_empty() {
                        metadata.insert("attachments".to_string(), attachment_info.join(","));
                    }
                }

                let message_type = if attachments.as_ref().map(|a| !a.is_empty()).unwrap_or(false) {
                    MessageType::File
                } else {
                    MessageType::Text
                };

                Some(Message {
                    id: uuid::Uuid::new_v4(),
                    thread_id: uuid::Uuid::new_v4(),
                    platform: PlatformType::Teams,
                    message_type,
                    content,
                    metadata,
                    timestamp: chrono::Utc::now(),
                })
            }
            _ => None,
        }
    }

    /// Extract conversation update info
    fn extract_conversation_update(
        &self,
        payload: &TeamsWebhookPayload,
    ) -> Option<(String, Vec<TeamsChannelAccount>)> {
        match payload {
            TeamsWebhookPayload::Activity {
                activity_type,
                channel_data,
                extra,
                ..
            } if activity_type == "conversationUpdate" => {
                // Extract members_added and members_removed from extra fields
                let members_added: Option<Vec<TeamsChannelAccount>> = extra
                    .get("membersAdded")
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
                let members_removed: Option<Vec<TeamsChannelAccount>> = extra
                    .get("membersRemoved")
                    .and_then(|v| serde_json::from_value(v.clone()).ok());

                let event_type = if members_added
                    .as_ref()
                    .map(|m| !m.is_empty())
                    .unwrap_or(false)
                {
                    "membersAdded"
                } else if members_removed
                    .as_ref()
                    .map(|m| !m.is_empty())
                    .unwrap_or(false)
                {
                    "membersRemoved"
                } else {
                    "conversationUpdate"
                };

                let members = members_added
                    .or_else(|| members_removed)
                    .unwrap_or_default();

                Some((event_type.to_string(), members))
            }
            _ => None,
        }
    }

    /// Extract message reaction info
    fn extract_message_reaction(
        &self,
        payload: &TeamsWebhookPayload,
    ) -> Option<TeamsMessageReaction> {
        match payload {
            TeamsWebhookPayload::Activity {
                activity_type,
                channel_data,
                reply_to_id,
                ..
            } if activity_type == "messageReaction" => {
                channel_data.as_ref()?.message_reaction.clone()
            }
            _ => None,
        }
    }

    /// Validate JWT token from Authorization header
    fn validate_jwt_token(&self, token: &str) -> Result<TeamsJwtClaims> {
        // Simple JWT validation - in production, use a proper JWT library
        // and validate against Microsoft's public keys

        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(AgentError::authentication("Invalid JWT token format").into());
        }

        // Decode payload (middle part)
        let payload = parts[1];
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .map_err(|e| AgentError::authentication(format!("Failed to decode JWT: {}", e)))?;

        let claims: TeamsJwtClaims = serde_json::from_slice(&decoded).map_err(|e| {
            AgentError::authentication(format!("Failed to parse JWT claims: {}", e))
        })?;

        // Validate expiration
        let now = chrono::Utc::now().timestamp();
        if claims.exp < now {
            return Err(AgentError::authentication("JWT token expired").into());
        }

        // Validate audience (should be our app ID)
        if claims.aud != self.app_id {
            return Err(AgentError::authentication("Invalid JWT audience").into());
        }

        Ok(claims)
    }
}

#[async_trait]
impl WebhookHandler for TeamsWebhookHandler {
    fn platform_type(&self) -> PlatformType {
        PlatformType::Teams
    }

    async fn verify_signature(
        &self,
        _body: &[u8],
        signature: Option<&str>,
        _timestamp: Option<&str>,
    ) -> Result<SignatureVerification> {
        if !self.verify_jwt {
            return Ok(SignatureVerification::Skipped);
        }

        let token = match signature {
            Some(t) => t.trim_start_matches("Bearer "),
            None => {
                warn!("No Authorization header provided for Teams webhook");
                return Ok(SignatureVerification::Skipped);
            }
        };

        match self.validate_jwt_token(token) {
            Ok(_) => {
                debug!("Teams JWT token validated successfully");
                Ok(SignatureVerification::Valid)
            }
            Err(e) => {
                error!("Teams JWT validation failed: {}", e);
                Ok(SignatureVerification::Invalid)
            }
        }
    }

    async fn parse_payload(&self, body: &[u8]) -> Result<Vec<WebhookEvent>> {
        let payload: TeamsWebhookPayload = serde_json::from_slice(body)
            .map_err(|e| AgentError::platform(format!("Failed to parse Teams payload: {}", e)))?;

        debug!("Parsed Teams payload");

        match &payload {
            TeamsWebhookPayload::Activity { activity_type, .. } => {
                let event_type = self.parse_event_type(activity_type);
                let message = self.convert_activity_to_message(&payload);

                // Extract additional metadata based on event type
                let metadata = if activity_type == "conversationUpdate" {
                    if let Some((update_type, members)) = self.extract_conversation_update(&payload)
                    {
                        MetadataBuilder::new()
                            .add("update_type", update_type)
                            .add("members_count", members.len().to_string())
                            .build()
                    } else {
                        HashMap::new()
                    }
                } else if activity_type == "messageReaction" {
                    if let Some(reaction) = self.extract_message_reaction(&payload) {
                        MetadataBuilder::new()
                            .add("reaction_type", reaction.reaction_type)
                            .add("reaction_user", reaction.user.id)
                            .build()
                    } else {
                        HashMap::new()
                    }
                } else {
                    HashMap::new()
                };

                let webhook_event = WebhookEvent {
                    event_type,
                    platform: PlatformType::Teams,
                    event_id: uuid::Uuid::new_v4().to_string(),
                    timestamp: chrono::Utc::now(),
                    payload: serde_json::to_value(&payload).unwrap_or_default(),
                    message,
                    metadata,
                };

                Ok(vec![webhook_event])
            }
            TeamsWebhookPayload::Generic(value) => {
                // Handle generic/unknown payload
                debug!("Received generic Teams payload: {:?}", value);

                let webhook_event = WebhookEvent {
                    event_type: WebhookEventType::Unknown,
                    platform: PlatformType::Teams,
                    event_id: uuid::Uuid::new_v4().to_string(),
                    timestamp: chrono::Utc::now(),
                    payload: value.clone(),
                    message: None,
                    metadata: MetadataBuilder::new().build(),
                };

                Ok(vec![webhook_event])
            }
        }
    }

    async fn handle_event(&self, event: WebhookEvent) -> Result<()> {
        match event.event_type {
            WebhookEventType::MessageReceived => {
                if let Some(msg) = &event.message {
                    info!(
                        "Received message from Teams: {} (user: {})",
                        msg.content,
                        msg.metadata
                            .get("user_name")
                            .unwrap_or(&"unknown".to_string())
                    );
                }
            }
            WebhookEventType::UserJoined => {
                if let Some(update_type) = event.metadata.get("update_type") {
                    info!("Teams conversation update: {}", update_type);
                }
            }
            WebhookEventType::MessageDeleted => {
                info!("Message deleted in Teams");
            }
            WebhookEventType::System => {
                debug!("Received Teams system event");
            }
            _ => {
                debug!(
                    "Received unhandled event type from Teams: {:?}",
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

/// Teams webhook response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamsWebhookResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub response_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "suggestedActions")]
    pub suggested_actions: Option<serde_json::Value>,
}

impl TeamsWebhookResponse {
    /// Create a text response
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            text: Some(content.into()),
            response_type: Some("message".to_string()),
            attachments: None,
            suggested_actions: None,
        }
    }

    /// Create an Adaptive Card response
    pub fn adaptive_card(card: serde_json::Value) -> Self {
        Self {
            text: None,
            response_type: Some("message".to_string()),
            attachments: Some(vec![serde_json::json!({
                "contentType": "application/vnd.microsoft.card.adaptive",
                "content": card,
            })]),
            suggested_actions: None,
        }
    }

    /// Create an empty success response
    pub fn success() -> Self {
        Self {
            text: None,
            response_type: None,
            attachments: None,
            suggested_actions: None,
        }
    }
}

/// Teams invoke response (for task modules and messaging extensions)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamsInvokeResponse {
    #[serde(rename = "task")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<serde_json::Value>,
    #[serde(rename = "composeExtension")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compose_extension: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
}

impl TeamsInvokeResponse {
    /// Create a task module response
    pub fn task_module(card: serde_json::Value) -> Self {
        Self {
            task: Some(serde_json::json!({
                "type": "continue",
                "value": card,
            })),
            compose_extension: None,
            config: None,
        }
    }

    /// Create a messaging extension response
    pub fn messaging_extension(attachments: Vec<serde_json::Value>) -> Self {
        Self {
            task: None,
            compose_extension: Some(serde_json::json!({
                "type": "result",
                "attachmentLayout": "list",
                "attachments": attachments,
            })),
            config: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_teams_webhook_response() {
        let text_resp = TeamsWebhookResponse::text("Hello");
        assert_eq!(text_resp.text, Some("Hello".to_string()));
        assert_eq!(text_resp.response_type, Some("message".to_string()));
    }

    #[test]
    fn test_teams_event_type() {
        assert_eq!(TeamsEventType::from("message"), TeamsEventType::Message);
        assert_eq!(
            TeamsEventType::from("conversationUpdate"),
            TeamsEventType::ConversationUpdate
        );
        assert_eq!(
            TeamsEventType::from("messageReaction"),
            TeamsEventType::MessageReaction
        );
        assert_eq!(
            TeamsEventType::from("unknown"),
            TeamsEventType::Unknown("unknown".to_string())
        );
    }

    #[test]
    fn test_parse_teams_activity() {
        let json = r#"{
            "type": "message",
            "channelId": "msteams",
            "conversation": {
                "id": "conversation_123",
                "isGroup": true,
                "conversationType": "channel"
            },
            "from": {
                "id": "user_123",
                "name": "Test User",
                "aadObjectId": "aad_123"
            },
            "recipient": {
                "id": "bot_123",
                "name": "Test Bot"
            },
            "text": "Hello bot!",
            "timestamp": "2024-01-01T00:00:00Z",
            "id": "activity_123",
            "serviceUrl": "https://smba.trafficmanager.net/"
        }"#;

        let payload: TeamsWebhookPayload = serde_json::from_str(json).unwrap();
        match payload {
            TeamsWebhookPayload::Activity {
                activity_type,
                text,
                ..
            } => {
                assert_eq!(activity_type, "message");
                assert_eq!(text, Some("Hello bot!".to_string()));
            }
            _ => panic!("Expected Activity payload"),
        }
    }

    #[test]
    fn test_teams_invoke_response() {
        let response = TeamsInvokeResponse::task_module(serde_json::json!({
            "title": "Task Module",
            "height": "medium",
        }));

        assert!(response.task.is_some());
        assert!(response.compose_extension.is_none());
    }
}
