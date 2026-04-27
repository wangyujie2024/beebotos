//! Google Chat Content Parser
//!
//! Parses and formats content from Google Chat messages.
//! Supports cards, mentions, and interactive elements.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::communication::channel::content::{ContentType as UnifiedContentType, PlatformContent};
use crate::error::{AgentError, Result};

/// Google Chat message content types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GoogleChatContent {
    /// Plain text message
    Text {
        text: String,
        annotations: Option<Vec<TextAnnotation>>,
    },
    /// Card message
    Card { cards: Vec<Card> },
    /// Card with text
    CardWithText { text: String, cards: Vec<Card> },
    /// Slash command
    SlashCommand {
        command_id: String,
        command_name: String,
        arguments: Option<Vec<CommandArgument>>,
    },
    /// Interactive event (button click, etc.)
    InteractiveEvent {
        action: FormAction,
        parameters: HashMap<String, String>,
    },
}

/// Text annotation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextAnnotation {
    #[serde(rename = "type")]
    pub type_: String,
    pub start_index: i32,
    pub length: i32,
    pub user_mention: Option<UserMention>,
    pub slash_command: Option<SlashCommandInfo>,
}

/// User mention in text
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMention {
    pub user: User,
    pub type_: String,
}

/// User info
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub name: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
}

/// Slash command info
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlashCommandInfo {
    pub command_id: String,
    pub command_name: String,
    pub trigger_character: String,
    pub type_: String,
}

/// Command argument
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandArgument {
    pub name: String,
    pub value: String,
}

/// Card structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub header: Option<CardHeader>,
    pub sections: Vec<CardSection>,
}

/// Card header
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardHeader {
    pub title: String,
    pub subtitle: Option<String>,
    pub image_url: Option<String>,
    pub image_type: Option<String>,
    pub image_alt_text: Option<String>,
}

/// Card section
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardSection {
    pub header: Option<String>,
    pub widgets: Vec<Widget>,
    pub collapsible: Option<bool>,
    pub uncollapsible_widgets_count: Option<i32>,
}

/// Card widget
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Widget {
    pub text_paragraph: Option<TextParagraph>,
    pub image: Option<ImageWidget>,
    pub key_value: Option<KeyValue>,
    pub buttons: Option<ButtonGroup>,
}

/// Text paragraph widget
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextParagraph {
    pub text: String,
}

/// Image widget
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageWidget {
    pub image_url: String,
    pub on_click: Option<OnClick>,
    pub alt_text: Option<String>,
}

/// Key-value widget
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyValue {
    pub top_label: Option<String>,
    pub content: String,
    pub content_multiline: Option<bool>,
    pub bottom_label: Option<String>,
    pub on_click: Option<OnClick>,
    pub icon: Option<String>,
    pub button: Option<Button>,
}

/// Button group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ButtonGroup {
    pub buttons: Vec<Button>,
}

/// Button
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Button {
    pub text_button: Option<TextButton>,
    pub image_button: Option<ImageButton>,
}

/// Text button
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextButton {
    pub text: String,
    pub on_click: OnClick,
}

/// Image button
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageButton {
    pub icon: String,
    pub on_click: OnClick,
}

/// On click action
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnClick {
    pub action: Option<FormAction>,
    pub open_link: Option<OpenLink>,
}

/// Form action
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormAction {
    pub action_method_name: String,
    pub parameters: Vec<ActionParameter>,
}

/// Action parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionParameter {
    pub key: String,
    pub value: String,
}

/// Open link action
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenLink {
    pub url: String,
}

/// Parsed content result
#[derive(Debug, Clone)]
pub struct ParsedContent {
    pub text: String,
    pub mentions: Vec<Mention>,
    pub commands: Vec<Command>,
    pub attachments: Vec<Attachment>,
}

/// Mention
#[derive(Debug, Clone)]
pub struct Mention {
    pub user_id: String,
    pub user_name: String,
    pub start_index: usize,
    pub length: usize,
}

/// Command
#[derive(Debug, Clone)]
pub struct Command {
    pub name: String,
    pub args: Vec<String>,
}

/// Attachment
#[derive(Debug, Clone)]
pub struct Attachment {
    pub type_: String,
    pub url: Option<String>,
    pub name: Option<String>,
}

/// Google Chat content parser
pub struct GoogleChatContentParser;

impl GoogleChatContentParser {
    /// Create a new parser
    pub fn new() -> Self {
        Self
    }

    /// Parse text content
    pub fn parse_text(&self, text: &str) -> ParsedContent {
        let mut mentions = Vec::new();
        let mut commands = Vec::new();
        let attachments = Vec::new();

        // Parse @mentions (format: <users/{user_id}>{display_name}</users/{user_id}>)
        let mention_regex = regex::Regex::new(r"<users/([^>]+)>([^<]+)</users/[^>]+>").ok();
        if let Some(ref re) = mention_regex {
            for cap in re.captures_iter(text) {
                if let (Some(id), Some(name)) = (cap.get(1), cap.get(2)) {
                    mentions.push(Mention {
                        user_id: id.as_str().to_string(),
                        user_name: name.as_str().to_string(),
                        start_index: cap.get(0).map(|m| m.start()).unwrap_or(0),
                        length: cap.get(0).map(|m| m.len()).unwrap_or(0),
                    });
                }
            }
        }

        // Parse slash commands (format: /command arg1 arg2)
        if text.starts_with('/') {
            let parts: Vec<&str> = text.split_whitespace().collect();
            if !parts.is_empty() {
                let cmd_name = parts[0].trim_start_matches('/').to_string();
                let args = parts[1..].iter().map(|s| s.to_string()).collect();
                commands.push(Command {
                    name: cmd_name,
                    args,
                });
            }
        }

        // Clean text (remove mention tags)
        let clean_text = mention_regex
            .map(|re| re.replace_all(text, "$2").to_string())
            .unwrap_or_else(|| text.to_string());

        ParsedContent {
            text: clean_text,
            mentions,
            commands,
            attachments,
        }
    }

    /// Format text for Google Chat
    pub fn format_text(&self, text: &str, mentions: &[Mention]) -> String {
        let mut result = text.to_string();

        // Replace mentions with Google Chat format
        for mention in mentions.iter().rev() {
            let mention_tag = format!(
                "<users/{}>{}</users/{}>",
                mention.user_id, mention.user_name, mention.user_id
            );
            let start = mention.start_index;
            let end = start + mention.length;
            if start < result.len() && end <= result.len() {
                result.replace_range(start..end, &mention_tag);
            }
        }

        result
    }

    /// Parse card JSON
    pub fn parse_card(&self, json: &str) -> Result<Card> {
        serde_json::from_str(json)
            .map_err(|e| AgentError::platform(format!("Invalid card JSON: {}", e)))
    }

    /// Build a simple card
    pub fn build_card(&self, title: &str, subtitle: Option<&str>, text: &str) -> Card {
        Card {
            header: Some(CardHeader {
                title: title.to_string(),
                subtitle: subtitle.map(|s| s.to_string()),
                image_url: None,
                image_type: None,
                image_alt_text: None,
            }),
            sections: vec![CardSection {
                header: None,
                widgets: vec![Widget {
                    text_paragraph: Some(TextParagraph {
                        text: text.to_string(),
                    }),
                    image: None,
                    key_value: None,
                    buttons: None,
                }],
                collapsible: None,
                uncollapsible_widgets_count: None,
            }],
        }
    }

    /// Build a card with buttons
    pub fn build_card_with_buttons(
        &self,
        title: &str,
        text: &str,
        buttons: Vec<(String, String)>, // (label, action)
    ) -> Card {
        let button_widgets: Vec<Button> = buttons
            .into_iter()
            .map(|(label, action)| Button {
                text_button: Some(TextButton {
                    text: label,
                    on_click: OnClick {
                        action: Some(FormAction {
                            action_method_name: action,
                            parameters: vec![],
                        }),
                        open_link: None,
                    },
                }),
                image_button: None,
            })
            .collect();

        Card {
            header: Some(CardHeader {
                title: title.to_string(),
                subtitle: None,
                image_url: None,
                image_type: None,
                image_alt_text: None,
            }),
            sections: vec![
                CardSection {
                    header: None,
                    widgets: vec![Widget {
                        text_paragraph: Some(TextParagraph {
                            text: text.to_string(),
                        }),
                        image: None,
                        key_value: None,
                        buttons: None,
                    }],
                    collapsible: None,
                    uncollapsible_widgets_count: None,
                },
                CardSection {
                    header: None,
                    widgets: vec![Widget {
                        text_paragraph: None,
                        image: None,
                        key_value: None,
                        buttons: Some(ButtonGroup {
                            buttons: button_widgets,
                        }),
                    }],
                    collapsible: None,
                    uncollapsible_widgets_count: None,
                },
            ],
        }
    }

    /// Extract plain text from card
    pub fn extract_card_text(&self, card: &Card) -> String {
        let mut texts = Vec::new();

        if let Some(header) = &card.header {
            texts.push(header.title.clone());
            if let Some(subtitle) = &header.subtitle {
                texts.push(subtitle.clone());
            }
        }

        for section in &card.sections {
            for widget in &section.widgets {
                if let Some(paragraph) = &widget.text_paragraph {
                    texts.push(paragraph.text.clone());
                }
                if let Some(kv) = &widget.key_value {
                    if let Some(label) = &kv.top_label {
                        texts.push(label.clone());
                    }
                    texts.push(kv.content.clone());
                    if let Some(label) = &kv.bottom_label {
                        texts.push(label.clone());
                    }
                }
            }
        }

        texts.join(" ")
    }
}

impl Default for GoogleChatContentParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_text() {
        let parser = GoogleChatContentParser::new();
        let content = parser.parse_text("Hello <users/123>John</users/123>!");

        assert_eq!(content.text, "Hello John!");
        assert_eq!(content.mentions.len(), 1);
        assert_eq!(content.mentions[0].user_id, "123");
        assert_eq!(content.mentions[0].user_name, "John");
    }

    #[test]
    fn test_parse_command() {
        let parser = GoogleChatContentParser::new();
        let content = parser.parse_text("/help general");

        assert_eq!(content.commands.len(), 1);
        assert_eq!(content.commands[0].name, "help");
        assert_eq!(content.commands[0].args, vec!["general"]);
    }

    #[test]
    fn test_build_card() {
        let parser = GoogleChatContentParser::new();
        let card = parser.build_card("Title", Some("Subtitle"), "Body text");

        assert_eq!(card.header.as_ref().unwrap().title, "Title");
        assert_eq!(card.sections.len(), 1);
    }

    #[test]
    fn test_extract_card_text() {
        let parser = GoogleChatContentParser::new();
        let card = parser.build_card("Title", Some("Subtitle"), "Body");
        let text = parser.extract_card_text(&card);

        assert!(text.contains("Title"));
        assert!(text.contains("Subtitle"));
        assert!(text.contains("Body"));
    }
}

// =============================================================================
// 🟢 P0 FIX: PlatformContent trait implementation for unified content framework
// =============================================================================

impl PlatformContent for GoogleChatContent {
    fn content_type(&self) -> UnifiedContentType {
        match self {
            GoogleChatContent::Text { .. } => UnifiedContentType::Text,
            GoogleChatContent::Card { .. } => UnifiedContentType::Card,
            GoogleChatContent::CardWithText { .. } => UnifiedContentType::Card,
            GoogleChatContent::SlashCommand { .. } => UnifiedContentType::System,
            GoogleChatContent::InteractiveEvent { .. } => UnifiedContentType::System,
        }
    }

    fn extract_text(&self) -> String {
        match self {
            GoogleChatContent::Text { text, .. } => text.clone(),
            GoogleChatContent::Card { cards } => {
                let parser = GoogleChatContentParser::new();
                cards
                    .iter()
                    .map(|card| parser.extract_card_text(card))
                    .collect::<Vec<_>>()
                    .join(" ")
            }
            GoogleChatContent::CardWithText { text, cards } => {
                let parser = GoogleChatContentParser::new();
                let card_text = cards
                    .iter()
                    .map(|card| parser.extract_card_text(card))
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("{} {}", text, card_text)
            }
            GoogleChatContent::SlashCommand { command_name, .. } => {
                format!("[Command: {}]", command_name)
            }
            GoogleChatContent::InteractiveEvent { action, .. } => {
                format!("[Interactive: {}]", action.action_method_name)
            }
        }
    }
}
