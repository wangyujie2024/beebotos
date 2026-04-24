//! Mattermost Content Parser
//!
//! Parses and formats content for Mattermost platform.

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Mattermost content parser
#[derive(Debug, Clone)]
pub struct MattermostContentParser;

impl MattermostContentParser {
    /// Create a new parser
    pub fn new() -> Self {
        Self
    }

    /// Extract mentions from content
    pub fn extract_mentions(&self, content: &str) -> Vec<String> {
        // Mattermost mentions: @username
        let mut mentions = Vec::new();
        for word in content.split_whitespace() {
            if word.starts_with('@') {
                let mention = word
                    .trim_start_matches('@')
                    .trim_matches(|c: char| !c.is_alphanumeric());
                if !mention.is_empty() {
                    mentions.push(mention.to_string());
                }
            }
        }
        mentions
    }

    /// Extract channels from content
    pub fn extract_channels(&self, content: &str) -> Vec<String> {
        // Mattermost channel references: ~channel-name
        let mut channels = Vec::new();
        for word in content.split_whitespace() {
            if word.starts_with('~') {
                let channel = word
                    .trim_start_matches('~')
                    .trim_matches(|c: char| !c.is_alphanumeric());
                if !channel.is_empty() {
                    channels.push(channel.to_string());
                }
            }
        }
        channels
    }

    /// Format message for Mattermost
    pub fn format_message(&self, content: &str) -> String {
        content.to_string()
    }

    /// Parse incoming Mattermost message
    pub fn parse_message(&self, content: &str, author_id: &str) -> Result<ParsedMattermostMessage> {
        let mentions = self.extract_mentions(content);
        let channels = self.extract_channels(content);

        Ok(ParsedMattermostMessage {
            content: content.to_string(),
            author_id: author_id.to_string(),
            mentions,
            channel_refs: channels,
            has_attachments: false, // Would be set based on message data
        })
    }

    /// Convert content to Mattermost markdown
    pub fn to_markdown(&self, content: &str) -> String {
        content.to_string()
    }

    /// Parse markdown from Mattermost
    pub fn parse_markdown(&self, content: &str) -> String {
        content.to_string()
    }
}

impl Default for MattermostContentParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Parsed Mattermost message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedMattermostMessage {
    pub content: String,
    pub author_id: String,
    pub mentions: Vec<String>,
    pub channel_refs: Vec<String>,
    pub has_attachments: bool,
}

/// Mattermost-specific content types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MattermostContent {
    /// Standard text message
    Text(String),
    /// Markdown formatted message
    Markdown(String),
    /// Message with attachments
    WithAttachments {
        text: String,
        attachments: Vec<MattermostAttachment>,
    },
    /// Interactive message with buttons
    Interactive {
        text: String,
        actions: Vec<MattermostAction>,
    },
}

/// Mattermost message attachment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MattermostAttachment {
    pub fallback: String,
    pub color: Option<String>,
    pub pretext: Option<String>,
    pub author_name: Option<String>,
    pub author_link: Option<String>,
    pub author_icon: Option<String>,
    pub title: Option<String>,
    pub title_link: Option<String>,
    pub text: Option<String>,
    pub fields: Vec<MattermostAttachmentField>,
    pub image_url: Option<String>,
    pub thumb_url: Option<String>,
    pub footer: Option<String>,
    pub footer_icon: Option<String>,
    pub timestamp: Option<i64>,
}

/// Mattermost attachment field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MattermostAttachmentField {
    pub title: String,
    pub value: String,
    pub short: bool,
}

/// Mattermost interactive action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MattermostAction {
    pub id: String,
    pub name: String,
    pub integration: ActionIntegration,
}

/// Action integration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionIntegration {
    pub url: String,
    pub context: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_mentions() {
        let parser = MattermostContentParser::new();
        let mentions = parser.extract_mentions("Hello @alice and @bob!");

        assert_eq!(mentions.len(), 2);
        assert!(mentions.contains(&"alice".to_string()));
        assert!(mentions.contains(&"bob".to_string()));
    }

    #[test]
    fn test_extract_channels() {
        let parser = MattermostContentParser::new();
        let channels = parser.extract_channels("Check out ~general and ~random");

        assert_eq!(channels.len(), 2);
        assert!(channels.contains(&"general".to_string()));
        assert!(channels.contains(&"random".to_string()));
    }

    #[test]
    fn test_parse_message() {
        let parser = MattermostContentParser::new();
        let parsed = parser.parse_message("Hello @alice!", "user-1").unwrap();

        assert_eq!(parsed.content, "Hello @alice!");
        assert_eq!(parsed.author_id, "user-1");
        assert_eq!(parsed.mentions.len(), 1);
    }
}
