//! LINE Content Parser
//!
//! Parses and formats content from LINE messages.
//! Supports various message types including text, stickers, images, and
//! templates.

use serde::{Deserialize, Serialize};

use crate::communication::channel::content::{ContentType as UnifiedContentType, PlatformContent};

/// LINE message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LineMessage {
    /// Text message
    Text {
        text: String,
        #[serde(rename = "emojis")]
        emojis: Option<Vec<LineEmoji>>,
        #[serde(rename = "mention")]
        mention: Option<LineMention>,
    },
    /// Sticker message
    Sticker {
        package_id: String,
        sticker_id: String,
        sticker_resource_type: Option<String>,
        keywords: Option<Vec<String>>,
    },
    /// Image message
    Image {
        original_content_url: String,
        preview_image_url: String,
        content_provider: Option<ContentProvider>,
    },
    /// Video message
    Video {
        original_content_url: String,
        preview_image_url: String,
        tracking_id: Option<String>,
        duration: Option<i32>,
        content_provider: Option<ContentProvider>,
    },
    /// Audio message
    Audio {
        original_content_url: String,
        duration: i32,
        content_provider: Option<ContentProvider>,
    },
    /// File message
    File { file_name: String, file_size: i64 },
    /// Location message
    Location {
        title: String,
        address: String,
        latitude: f64,
        longitude: f64,
    },
    /// Template message
    Template {
        alt_text: String,
        template: Template,
    },
    /// Flex message
    Flex {
        alt_text: String,
        contents: serde_json::Value,
    },
}

/// LINE emoji
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineEmoji {
    pub index: i32,
    pub length: i32,
    pub product_id: String,
    pub emoji_id: String,
}

/// LINE mention
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineMention {
    pub mentionees: Vec<Mentionee>,
}

/// Mentionee
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Mentionee {
    #[serde(rename = "user")]
    User {
        user_id: Option<String>,
        is_self: Option<bool>,
    },
    #[serde(rename = "all")]
    All,
}

/// Content provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentProvider {
    #[serde(rename = "type")]
    pub type_: String,
    pub original_content_url: Option<String>,
    pub preview_image_url: Option<String>,
}

/// Template types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Template {
    /// Buttons template
    Buttons {
        thumbnail_image_url: Option<String>,
        image_aspect_ratio: Option<String>,
        image_size: Option<String>,
        image_background_color: Option<String>,
        title: Option<String>,
        text: String,
        default_action: Option<Action>,
        actions: Vec<Action>,
    },
    /// Confirm template
    Confirm { text: String, actions: Vec<Action> },
    /// Carousel template
    Carousel { columns: Vec<CarouselColumn> },
    /// Image carousel template
    ImageCarousel { columns: Vec<ImageCarouselColumn> },
}

/// Carousel column
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarouselColumn {
    pub thumbnail_image_url: Option<String>,
    pub image_background_color: Option<String>,
    pub title: Option<String>,
    pub text: String,
    pub default_action: Option<Action>,
    pub actions: Vec<Action>,
}

/// Image carousel column
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageCarouselColumn {
    pub image_url: String,
    pub action: Action,
}

/// Action types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Action {
    /// Postback action
    Postback {
        label: String,
        data: String,
        display_text: Option<String>,
        input_option: Option<String>,
        fill_in_text: Option<String>,
    },
    /// Message action
    Message { label: String, text: String },
    /// URI action
    Uri {
        label: String,
        uri: String,
        alt_uri: Option<AltUri>,
    },
    /// Datetime picker action
    DatetimePicker {
        label: String,
        data: String,
        mode: String,
        initial: Option<String>,
        max: Option<String>,
        min: Option<String>,
    },
    /// Camera action
    Camera { label: String },
    /// Camera roll action
    CameraRoll { label: String },
    /// Location action
    Location { label: String },
    /// Rich menu switch action
    RichMenuSwitch {
        label: String,
        rich_menu_alias_id: String,
        data: String,
    },
}

/// Alt URI for URI action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AltUri {
    pub desktop: String,
}

/// Quick reply
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickReply {
    pub items: Vec<QuickReplyItem>,
}

/// Quick reply item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickReplyItem {
    #[serde(rename = "type")]
    pub type_: String,
    pub image_url: Option<String>,
    pub action: Action,
}

/// Source types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Source {
    #[serde(rename = "user")]
    User { user_id: String },
    #[serde(rename = "group")]
    Group {
        group_id: String,
        user_id: Option<String>,
    },
    #[serde(rename = "room")]
    Room {
        room_id: String,
        user_id: Option<String>,
    },
}

/// Parsed content result
#[derive(Debug, Clone, Default)]
pub struct ParsedContent {
    pub text: String,
    pub mentions: Vec<Mention>,
    pub stickers: Vec<StickerInfo>,
    pub media_urls: Vec<MediaUrl>,
    pub location: Option<LocationInfo>,
    pub commands: Vec<Command>,
}

/// Mention
#[derive(Debug, Clone)]
pub struct Mention {
    pub user_id: Option<String>,
    pub is_all: bool,
    pub index: i32,
    pub length: i32,
}

/// Sticker info
#[derive(Debug, Clone)]
pub struct StickerInfo {
    pub package_id: String,
    pub sticker_id: String,
}

/// Media URL
#[derive(Debug, Clone)]
pub struct MediaUrl {
    pub type_: String,
    pub url: String,
    pub preview_url: Option<String>,
}

/// Location info
#[derive(Debug, Clone)]
pub struct LocationInfo {
    pub title: String,
    pub address: String,
    pub latitude: f64,
    pub longitude: f64,
}

/// Command
#[derive(Debug, Clone)]
pub struct Command {
    pub name: String,
    pub args: Vec<String>,
}

/// LINE content parser
pub struct LineContentParser;

impl LineContentParser {
    /// Create a new parser
    pub fn new() -> Self {
        Self
    }

    /// Parse LINE message
    pub fn parse_message(&self, message: &LineMessage) -> ParsedContent {
        match message {
            LineMessage::Text {
                text,
                emojis: _,
                mention,
            } => self.parse_text_message(text, mention.as_ref()),
            LineMessage::Sticker {
                package_id,
                sticker_id,
                ..
            } => ParsedContent {
                stickers: vec![StickerInfo {
                    package_id: package_id.clone(),
                    sticker_id: sticker_id.clone(),
                }],
                ..Default::default()
            },
            LineMessage::Image {
                original_content_url,
                preview_image_url,
                ..
            } => ParsedContent {
                media_urls: vec![MediaUrl {
                    type_: "image".to_string(),
                    url: original_content_url.clone(),
                    preview_url: Some(preview_image_url.clone()),
                }],
                ..Default::default()
            },
            LineMessage::Video {
                original_content_url,
                preview_image_url,
                ..
            } => ParsedContent {
                media_urls: vec![MediaUrl {
                    type_: "video".to_string(),
                    url: original_content_url.clone(),
                    preview_url: Some(preview_image_url.clone()),
                }],
                ..Default::default()
            },
            LineMessage::Location {
                title,
                address,
                latitude,
                longitude,
            } => ParsedContent {
                location: Some(LocationInfo {
                    title: title.clone(),
                    address: address.clone(),
                    latitude: *latitude,
                    longitude: *longitude,
                }),
                ..Default::default()
            },
            _ => ParsedContent::default(),
        }
    }

    /// Parse text message
    fn parse_text_message(&self, text: &str, mention: Option<&LineMention>) -> ParsedContent {
        let mut mentions = Vec::new();
        let mut commands = Vec::new();

        // Extract mentions
        if let Some(mention_data) = mention {
            for mentionee in &mention_data.mentionees {
                match mentionee {
                    Mentionee::User { user_id, is_self } => {
                        mentions.push(Mention {
                            user_id: user_id.clone(),
                            is_all: is_self.unwrap_or(false),
                            index: 0,
                            length: 0,
                        });
                    }
                    Mentionee::All => {
                        mentions.push(Mention {
                            user_id: None,
                            is_all: true,
                            index: 0,
                            length: 0,
                        });
                    }
                }
            }
        }

        // Parse commands (format: /command arg1 arg2)
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

        ParsedContent {
            text: text.to_string(),
            mentions,
            stickers: vec![],
            media_urls: vec![],
            location: None,
            commands,
        }
    }

    /// Build text message
    pub fn build_text(&self, text: impl Into<String>) -> LineMessage {
        LineMessage::Text {
            text: text.into(),
            emojis: None,
            mention: None,
        }
    }

    /// Build text message with mention
    pub fn build_text_with_mention(
        &self,
        text: impl Into<String>,
        mention: LineMention,
    ) -> LineMessage {
        LineMessage::Text {
            text: text.into(),
            emojis: None,
            mention: Some(mention),
        }
    }

    /// Build sticker message
    pub fn build_sticker(
        &self,
        package_id: impl Into<String>,
        sticker_id: impl Into<String>,
    ) -> LineMessage {
        LineMessage::Sticker {
            package_id: package_id.into(),
            sticker_id: sticker_id.into(),
            sticker_resource_type: None,
            keywords: None,
        }
    }

    /// Build image message
    pub fn build_image(
        &self,
        original_url: impl Into<String>,
        preview_url: impl Into<String>,
    ) -> LineMessage {
        LineMessage::Image {
            original_content_url: original_url.into(),
            preview_image_url: preview_url.into(),
            content_provider: None,
        }
    }

    /// Build buttons template
    pub fn build_buttons_template(
        &self,
        alt_text: impl Into<String>,
        title: Option<&str>,
        text: impl Into<String>,
        actions: Vec<Action>,
    ) -> LineMessage {
        LineMessage::Template {
            alt_text: alt_text.into(),
            template: Template::Buttons {
                thumbnail_image_url: None,
                image_aspect_ratio: None,
                image_size: None,
                image_background_color: None,
                title: title.map(|s| s.to_string()),
                text: text.into(),
                default_action: None,
                actions,
            },
        }
    }

    /// Build confirm template
    pub fn build_confirm_template(
        &self,
        alt_text: impl Into<String>,
        text: impl Into<String>,
        yes_action: Action,
        no_action: Action,
    ) -> LineMessage {
        LineMessage::Template {
            alt_text: alt_text.into(),
            template: Template::Confirm {
                text: text.into(),
                actions: vec![yes_action, no_action],
            },
        }
    }

    /// Build quick reply
    pub fn build_quick_reply(&self, items: Vec<(String, Action)>) -> QuickReply {
        QuickReply {
            items: items
                .into_iter()
                .map(|(_label, action)| QuickReplyItem {
                    type_: "action".to_string(),
                    image_url: None,
                    action,
                })
                .collect(),
        }
    }

    /// Create postback action
    pub fn create_postback_action(
        &self,
        label: impl Into<String>,
        data: impl Into<String>,
        display_text: Option<impl Into<String>>,
    ) -> Action {
        Action::Postback {
            label: label.into(),
            data: data.into(),
            display_text: display_text.map(|s| s.into()),
            input_option: None,
            fill_in_text: None,
        }
    }

    /// Create message action
    pub fn create_message_action(
        &self,
        label: impl Into<String>,
        text: impl Into<String>,
    ) -> Action {
        Action::Message {
            label: label.into(),
            text: text.into(),
        }
    }

    /// Create URI action
    pub fn create_uri_action(&self, label: impl Into<String>, uri: impl Into<String>) -> Action {
        Action::Uri {
            label: label.into(),
            uri: uri.into(),
            alt_uri: None,
        }
    }

    /// Create mention
    pub fn create_mention(user_id: impl Into<String>, is_self: bool) -> Mentionee {
        Mentionee::User {
            user_id: Some(user_id.into()),
            is_self: Some(is_self),
        }
    }

    /// Create @all mention
    pub fn create_all_mention() -> Mentionee {
        Mentionee::All
    }
}

impl Default for LineContentParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_text_message() {
        let parser = LineContentParser::new();
        let message = LineMessage::Text {
            text: "Hello world".to_string(),
            emojis: None,
            mention: None,
        };

        let content = parser.parse_message(&message);
        assert_eq!(content.text, "Hello world");
    }

    #[test]
    fn test_parse_command() {
        let parser = LineContentParser::new();
        let message = LineMessage::Text {
            text: "/help general".to_string(),
            emojis: None,
            mention: None,
        };

        let content = parser.parse_message(&message);
        assert_eq!(content.commands.len(), 1);
        assert_eq!(content.commands[0].name, "help");
        assert_eq!(content.commands[0].args, vec!["general"]);
    }

    #[test]
    fn test_parse_sticker() {
        let parser = LineContentParser::new();
        let message = LineMessage::Sticker {
            package_id: "1".to_string(),
            sticker_id: "2".to_string(),
            sticker_resource_type: None,
            keywords: None,
        };

        let content = parser.parse_message(&message);
        assert_eq!(content.stickers.len(), 1);
        assert_eq!(content.stickers[0].package_id, "1");
        assert_eq!(content.stickers[0].sticker_id, "2");
    }

    #[test]
    fn test_build_text() {
        let parser = LineContentParser::new();
        let msg = parser.build_text("Hello");

        match msg {
            LineMessage::Text { text, .. } => assert_eq!(text, "Hello"),
            _ => panic!("Expected text message"),
        }
    }

    #[test]
    fn test_build_buttons_template() {
        let parser = LineContentParser::new();
        let action = parser.create_message_action("Click me", "clicked");
        let msg =
            parser.build_buttons_template("Alt text", Some("Title"), "Body text", vec![action]);

        match msg {
            LineMessage::Template { alt_text, .. } => assert_eq!(alt_text, "Alt text"),
            _ => panic!("Expected template message"),
        }
    }
}

// =============================================================================
// 🟢 P0 FIX: PlatformContent trait implementation for unified content framework
// =============================================================================

impl PlatformContent for LineMessage {
    fn content_type(&self) -> UnifiedContentType {
        match self {
            LineMessage::Text { .. } => UnifiedContentType::Text,
            LineMessage::Sticker { .. } => UnifiedContentType::Sticker,
            LineMessage::Image { .. } => UnifiedContentType::Image,
            LineMessage::Video { .. } => UnifiedContentType::Video,
            LineMessage::Audio { .. } => UnifiedContentType::Audio,
            LineMessage::File { .. } => UnifiedContentType::File,
            LineMessage::Location { .. } => UnifiedContentType::Location,
            LineMessage::Template { .. } => UnifiedContentType::Card,
            LineMessage::Flex { .. } => UnifiedContentType::Card,
        }
    }

    fn extract_text(&self) -> String {
        match self {
            LineMessage::Text { text, .. } => text.clone(),
            LineMessage::Sticker { .. } => "[Sticker]".to_string(),
            LineMessage::Image { .. } => "[Image]".to_string(),
            LineMessage::Video { .. } => "[Video]".to_string(),
            LineMessage::Audio { .. } => "[Audio]".to_string(),
            LineMessage::File { file_name, .. } => format!("[File: {}]", file_name),
            LineMessage::Location { title, .. } => format!("[Location: {}]", title),
            LineMessage::Template { alt_text, .. } => alt_text.clone(),
            LineMessage::Flex { alt_text, .. } => alt_text.clone(),
        }
    }
}
