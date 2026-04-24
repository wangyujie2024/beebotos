//! Unified Content Framework
//!
//! Provides common traits and structures for all platform content types.
//! This module eliminates duplication across 16+ *_content.rs files.

use std::collections::HashMap;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::Result;

// =============================================================================
// Common Content Types
// =============================================================================

/// Content type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Text,
    Image,
    Video,
    Audio,
    File,
    Location,
    Sticker,
    Contact,
    Rich,
    Card,
    System,
    Unknown,
}

impl ContentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContentType::Text => "text",
            ContentType::Image => "image",
            ContentType::Video => "video",
            ContentType::Audio => "audio",
            ContentType::File => "file",
            ContentType::Location => "location",
            ContentType::Sticker => "sticker",
            ContentType::Contact => "contact",
            ContentType::Rich => "rich",
            ContentType::Card => "card",
            ContentType::System => "system",
            ContentType::Unknown => "unknown",
        }
    }
}

impl Default for ContentType {
    fn default() -> Self {
        ContentType::Text
    }
}

// =============================================================================
// Common Content Structures
// =============================================================================

/// Media content (Image, Video, Audio, File)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MediaContent {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
}

impl MediaContent {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ..Default::default()
        }
    }

    pub fn with_caption(mut self, caption: impl Into<String>) -> Self {
        self.caption = Some(caption.into());
        self
    }

    pub fn with_dimensions(mut self, width: i32, height: i32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }
}

/// Text content
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TextContent {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<TextFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entities: Option<Vec<TextEntity>>,
}

impl TextContent {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Default::default()
        }
    }
}

/// Text format enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextFormat {
    Plain,
    Markdown,
    Html,
}

/// Text entity (mention, hashtag, etc.)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TextEntity {
    pub offset: usize,
    pub length: usize,
    pub entity_type: EntityType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

/// Entity type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Mention,
    Hashtag,
    Cashtag,
    BotCommand,
    Url,
    Email,
    PhoneNumber,
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Code,
    Pre,
    TextLink,
    TextMention,
}

/// Location content
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocationContent {
    pub latitude: f64,
    pub longitude: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
}

/// Contact content
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ContactContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vcard: Option<String>,
}

/// Sticker content
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StickerContent {
    pub file_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_animated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_video: Option<bool>,
}

/// Rich content (HTML/markdown with formatting)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RichContent {
    pub text: String,
    pub format: TextFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<MediaContent>>,
}

/// Card content (structured message)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CardContent {
    pub title: Option<String>,
    pub description: Option<String>,
    pub image: Option<MediaContent>,
    pub buttons: Vec<CardButton>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

/// Card button
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CardButton {
    pub text: String,
    pub action: ButtonAction,
}

/// Button action
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "data")]
pub enum ButtonAction {
    Url(String),
    Postback(String),
    Callback(String),
}

// =============================================================================
// Content Trait
// =============================================================================

/// Core content trait implemented by all platform content types
///
/// Note: This trait is dyn-compatible (object-safe) for use with `&dyn
/// PlatformContent`. For serialization, use the `PlatformContentExt` trait
/// which requires `Serialize`.
pub trait PlatformContent: Send + Sync + 'static {
    /// Get the content type
    fn content_type(&self) -> ContentType;

    /// Extract plain text representation
    fn extract_text(&self) -> String;

    /// Get media content if this is a media type
    fn as_media(&self) -> Option<&MediaContent> {
        None
    }

    /// Get text content if this is a text type
    fn as_text(&self) -> Option<&TextContent> {
        None
    }
}

/// Extension trait for PlatformContent types that support serialization
///
/// This trait requires `Serialize` and `DeserializeOwned` and provides
/// JSON serialization methods. It is not dyn-compatible.
pub trait PlatformContentExt: PlatformContent + Serialize + DeserializeOwned + Clone {
    /// Convert to JSON string
    fn to_json(&self) -> Result<String> {
        serde_json::to_string(self)
            .map_err(|e| crate::error::AgentError::platform(format!("JSON serialize error: {}", e)))
    }

    /// Convert to JSON value
    fn to_json_value(&self) -> Result<serde_json::Value> {
        serde_json::to_value(self)
            .map_err(|e| crate::error::AgentError::platform(format!("JSON serialize error: {}", e)))
    }
}

// Blanket implementation for all types that implement both traits
impl<T> PlatformContentExt for T where T: PlatformContent + Serialize + DeserializeOwned + Clone {}

// =============================================================================
// Content Parser Trait
// =============================================================================

/// Parser for converting platform-specific formats to/from unified content
pub trait ContentParser<C: PlatformContent>: Clone + Send + Sync + 'static {
    /// Parse content from JSON value
    fn parse(&self, content_type: &str, content: serde_json::Value) -> Result<C>;

    /// Parse content from JSON string
    fn parse_str(&self, content_type: &str, json: &str) -> Result<C> {
        let value: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| crate::error::AgentError::platform(format!("JSON parse error: {}", e)))?;
        self.parse(content_type, value)
    }

    /// Create text content
    fn create_text(&self, text: impl Into<String>) -> C;

    /// Create image content
    fn create_image(&self, url: impl Into<String>) -> C;

    /// Create video content  
    fn create_video(&self, url: impl Into<String>) -> C;

    /// Create file content
    fn create_file(&self, url: impl Into<String>, filename: impl Into<String>) -> C;

    /// Get content type string for a content variant
    fn get_content_type(content: &C) -> &'static str;
}

// =============================================================================
// Content Builder
// =============================================================================

/// Builder for constructing complex content
pub struct ContentBuilder<C: PlatformContent> {
    content: Option<C>,
}

impl<C: PlatformContent> ContentBuilder<C> {
    pub fn new() -> Self {
        Self { content: None }
    }

    pub fn with_content(mut self, content: C) -> Self {
        self.content = Some(content);
        self
    }

    pub fn build(self) -> Option<C> {
        self.content
    }
}

impl<C: PlatformContent> Default for ContentBuilder<C> {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Utility Functions
// =============================================================================

/// Parse content type from string
pub fn parse_content_type(s: &str) -> ContentType {
    match s {
        "text" => ContentType::Text,
        "image" => ContentType::Image,
        "video" => ContentType::Video,
        "audio" => ContentType::Audio,
        "file" => ContentType::File,
        "location" => ContentType::Location,
        "sticker" => ContentType::Sticker,
        "contact" => ContentType::Contact,
        "rich" => ContentType::Rich,
        "card" => ContentType::Card,
        "system" => ContentType::System,
        _ => ContentType::Unknown,
    }
}

/// Extract text from any content type with formatting
pub fn extract_text_with_format(content: &dyn PlatformContent) -> String {
    let text = content.extract_text();
    if text.is_empty() {
        format!("[{}]", content.content_type().as_str())
    } else {
        text
    }
}

/// Create metadata map from key-value pairs
pub fn create_metadata(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}
