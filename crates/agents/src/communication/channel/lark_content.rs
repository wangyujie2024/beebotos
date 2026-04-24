//! Lark Message Content Parser
//!
//! Provides parsing and handling of various Lark/Feishu message content types
//! including text, post (rich text), images, files, interactive cards, and
//! more.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::communication::channel::content::{
    ContentType as UnifiedContentType, MediaContent, PlatformContent,
};
use crate::error::{AgentError, Result};

/// Lark message content types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "msg_type", content = "content")]
pub enum LarkContent {
    /// Plain text message
    #[serde(rename = "text")]
    Text(LarkTextContent),
    /// Rich text/post message
    #[serde(rename = "post")]
    Post(LarkPostContent),
    /// Image message
    #[serde(rename = "image")]
    Image(LarkImageContent),
    /// File message
    #[serde(rename = "file")]
    File(LarkFileContent),
    /// Interactive card message
    #[serde(rename = "interactive")]
    Interactive(LarkInteractiveContent),
    /// Share chat message
    #[serde(rename = "share_chat")]
    ShareChat(LarkShareChatContent),
    /// Audio/voice message
    #[serde(rename = "audio")]
    Audio(LarkAudioContent),
    /// Sticker message
    #[serde(rename = "sticker")]
    Sticker(LarkStickerContent),
}

/// Text content structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LarkTextContent {
    /// The text content
    pub text: String,
}

/// Post (rich text) content structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LarkPostContent {
    /// Post content in JSON format
    pub content: serde_json::Value,
}

/// Post element representing a single element in rich text
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tag")]
pub enum PostElement {
    /// Plain text element
    #[serde(rename = "text")]
    Text {
        /// Text content
        text: String,
        /// Text style (bold, italic, etc.)
        #[serde(skip_serializing_if = "Option::is_none")]
        style: Option<HashMap<String, bool>>,
    },
    /// Link element
    #[serde(rename = "a")]
    Link {
        /// Link URL
        href: String,
        /// Link text
        text: String,
    },
    /// At mention element
    #[serde(rename = "at")]
    At {
        /// User ID
        user_id: String,
        /// User name (optional)
        #[serde(skip_serializing_if = "Option::is_none")]
        user_name: Option<String>,
    },
    /// Image element
    #[serde(rename = "img")]
    Image {
        /// Image key
        image_key: String,
        /// Image width (optional)
        #[serde(skip_serializing_if = "Option::is_none")]
        width: Option<i32>,
        /// Image height (optional)
        #[serde(skip_serializing_if = "Option::is_none")]
        height: Option<i32>,
    },
    /// Media element (video/audio)
    #[serde(rename = "media")]
    Media {
        /// File key
        file_key: String,
        /// Media type
        #[serde(skip_serializing_if = "Option::is_none")]
        media_type: Option<String>,
    },
    /// Emotion/emoji element
    #[serde(rename = "emotion")]
    Emotion {
        /// Emoji type
        emoji_type: String,
    },
}

/// Image content structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LarkImageContent {
    /// Image key for downloading the image
    pub image_key: String,
}

/// File content structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LarkFileContent {
    /// File key for downloading the file
    pub file_key: String,
    /// File name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
}

/// Interactive card content structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LarkInteractiveContent {
    /// Card configuration
    pub config: Option<serde_json::Value>,
    /// Card header
    pub header: Option<serde_json::Value>,
    /// Card elements
    pub elements: Vec<serde_json::Value>,
}

/// Share chat content structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LarkShareChatContent {
    /// Shared chat ID
    pub chat_id: String,
    /// Chat name (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_name: Option<String>,
}

/// Audio content structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LarkAudioContent {
    /// Audio file key
    pub file_key: String,
    /// Audio duration in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<i32>,
}

/// Sticker content structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LarkStickerContent {
    /// Sticker file key
    pub file_key: String,
}

/// Lark content parser for parsing message content from JSON
#[derive(Debug, Clone, Default)]
pub struct LarkContentParser;

impl LarkContentParser {
    /// Create a new content parser
    pub fn new() -> Self {
        Self
    }

    /// Parse content from JSON value
    ///
    /// # Arguments
    /// * `msg_type` - The message type (text, post, image, etc.)
    /// * `content` - The JSON content to parse
    ///
    /// # Returns
    /// Parsed LarkContent enum variant
    pub fn parse(msg_type: &str, content: serde_json::Value) -> Result<LarkContent> {
        match msg_type {
            "text" => {
                let text_content: LarkTextContent =
                    serde_json::from_value(content).map_err(|e| {
                        AgentError::platform(format!("Failed to parse text content: {}", e))
                    })?;
                Ok(LarkContent::Text(text_content))
            }
            "post" => {
                let post_content: LarkPostContent =
                    serde_json::from_value(content).map_err(|e| {
                        AgentError::platform(format!("Failed to parse post content: {}", e))
                    })?;
                Ok(LarkContent::Post(post_content))
            }
            "image" => {
                let image_content: LarkImageContent =
                    serde_json::from_value(content).map_err(|e| {
                        AgentError::platform(format!("Failed to parse image content: {}", e))
                    })?;
                Ok(LarkContent::Image(image_content))
            }
            "file" => {
                let file_content: LarkFileContent =
                    serde_json::from_value(content).map_err(|e| {
                        AgentError::platform(format!("Failed to parse file content: {}", e))
                    })?;
                Ok(LarkContent::File(file_content))
            }
            "interactive" => {
                let interactive_content: LarkInteractiveContent = serde_json::from_value(content)
                    .map_err(|e| {
                    AgentError::platform(format!("Failed to parse interactive content: {}", e))
                })?;
                Ok(LarkContent::Interactive(interactive_content))
            }
            "share_chat" => {
                let share_chat_content: LarkShareChatContent = serde_json::from_value(content)
                    .map_err(|e| {
                        AgentError::platform(format!("Failed to parse share_chat content: {}", e))
                    })?;
                Ok(LarkContent::ShareChat(share_chat_content))
            }
            "audio" => {
                let audio_content: LarkAudioContent =
                    serde_json::from_value(content).map_err(|e| {
                        AgentError::platform(format!("Failed to parse audio content: {}", e))
                    })?;
                Ok(LarkContent::Audio(audio_content))
            }
            "sticker" => {
                let sticker_content: LarkStickerContent =
                    serde_json::from_value(content).map_err(|e| {
                        AgentError::platform(format!("Failed to parse sticker content: {}", e))
                    })?;
                Ok(LarkContent::Sticker(sticker_content))
            }
            _ => Err(AgentError::platform(format!("Unknown message type: {}", msg_type)).into()),
        }
    }

    /// Parse content from JSON string
    ///
    /// # Arguments
    /// * `msg_type` - The message type
    /// * `content_json` - JSON string to parse
    ///
    /// # Returns
    /// Parsed LarkContent enum variant
    pub fn parse_str(msg_type: &str, content_json: &str) -> Result<LarkContent> {
        let content: serde_json::Value = serde_json::from_str(content_json)
            .map_err(|e| AgentError::platform(format!("Invalid JSON: {}", e)))?;
        Self::parse(msg_type, content)
    }

    /// Extract plain text from any content type
    ///
    /// # Arguments
    /// * `content` - The LarkContent to extract text from
    ///
    /// # Returns
    /// Extracted plain text string
    pub fn extract_text(content: &LarkContent) -> String {
        match content {
            LarkContent::Text(text) => text.text.clone(),
            LarkContent::Post(post) => Self::extract_text_from_post(post),
            LarkContent::Image(_) => "[Image]".to_string(),
            LarkContent::File(file) => {
                format!("[File: {}]", file.file_name.as_deref().unwrap_or("unnamed"))
            }
            LarkContent::Interactive(_) => "[Interactive Card]".to_string(),
            LarkContent::ShareChat(chat) => {
                format!(
                    "[Shared Chat: {}]",
                    chat.chat_name.as_deref().unwrap_or(&chat.chat_id)
                )
            }
            LarkContent::Audio(_) => "[Audio]".to_string(),
            LarkContent::Sticker(_) => "[Sticker]".to_string(),
        }
    }

    /// Extract text from post content
    fn extract_text_from_post(post: &LarkPostContent) -> String {
        let mut texts = Vec::new();

        // Parse post content structure
        if let Some(content) = post.content.get("content") {
            if let Some(lines) = content.as_array() {
                for line in lines {
                    if let Some(elements) = line.as_array() {
                        let line_text: Vec<String> = elements
                            .iter()
                            .filter_map(|elem| {
                                if let Some(tag) = elem.get("tag").and_then(|t| t.as_str()) {
                                    match tag {
                                        "text" => elem
                                            .get("text")
                                            .and_then(|t| t.as_str())
                                            .map(String::from),
                                        "a" => elem
                                            .get("text")
                                            .and_then(|t| t.as_str())
                                            .map(|text| format!("[{}]", text)),
                                        "at" => elem
                                            .get("user_name")
                                            .and_then(|n| n.as_str())
                                            .map(|name| format!("@{} ", name))
                                            .or_else(|| Some("@user ".to_string())),
                                        "img" => Some("[Image]".to_string()),
                                        "emotion" => elem
                                            .get("emoji_type")
                                            .and_then(|e| e.as_str())
                                            .map(|emoji| format!("[{}]", emoji)),
                                        _ => None,
                                    }
                                } else {
                                    None
                                }
                            })
                            .collect();
                        if !line_text.is_empty() {
                            texts.push(line_text.join(""));
                        }
                    }
                }
            }
        }

        texts.join("\n")
    }

    /// Get message type from content
    ///
    /// # Arguments
    /// * `content` - The LarkContent
    ///
    /// # Returns
    /// Message type string
    pub fn get_msg_type(content: &LarkContent) -> &'static str {
        match content {
            LarkContent::Text(_) => "text",
            LarkContent::Post(_) => "post",
            LarkContent::Image(_) => "image",
            LarkContent::File(_) => "file",
            LarkContent::Interactive(_) => "interactive",
            LarkContent::ShareChat(_) => "share_chat",
            LarkContent::Audio(_) => "audio",
            LarkContent::Sticker(_) => "sticker",
        }
    }

    /// Create text content
    ///
    /// # Arguments
    /// * `text` - The text content
    ///
    /// # Returns
    /// LarkContent::Text variant
    pub fn create_text(text: impl Into<String>) -> LarkContent {
        LarkContent::Text(LarkTextContent { text: text.into() })
    }

    /// Create image content
    ///
    /// # Arguments
    /// * `image_key` - The image key
    ///
    /// # Returns
    /// LarkContent::Image variant
    pub fn create_image(image_key: impl Into<String>) -> LarkContent {
        LarkContent::Image(LarkImageContent {
            image_key: image_key.into(),
        })
    }

    /// Create file content
    ///
    /// # Arguments
    /// * `file_key` - The file key
    /// * `file_name` - Optional file name
    ///
    /// # Returns
    /// LarkContent::File variant
    pub fn create_file(file_key: impl Into<String>, file_name: Option<String>) -> LarkContent {
        LarkContent::File(LarkFileContent {
            file_key: file_key.into(),
            file_name,
        })
    }

    /// Serialize content to JSON string
    ///
    /// # Arguments
    /// * `content` - The LarkContent to serialize
    ///
    /// # Returns
    /// JSON string representation
    pub fn to_json(content: &LarkContent) -> Result<String> {
        serde_json::to_string(content)
            .map_err(|e| AgentError::platform(format!("Failed to serialize content: {}", e)).into())
    }

    /// Serialize content to JSON value
    ///
    /// # Arguments
    /// * `content` - The LarkContent to serialize
    ///
    /// # Returns
    /// JSON value representation
    pub fn to_json_value(content: &LarkContent) -> Result<serde_json::Value> {
        serde_json::to_value(content)
            .map_err(|e| AgentError::platform(format!("Failed to serialize content: {}", e)).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_text_content() {
        let json = serde_json::json!({
            "text": "Hello, World!"
        });
        let content = LarkContentParser::parse("text", json).unwrap();
        assert!(matches!(content, LarkContent::Text(_)));
        assert_eq!(LarkContentParser::extract_text(&content), "Hello, World!");
    }

    #[test]
    fn test_parse_image_content() {
        let json = serde_json::json!({
            "image_key": "img_12345"
        });
        let content = LarkContentParser::parse("image", json).unwrap();
        assert!(matches!(content, LarkContent::Image(_)));
        assert_eq!(LarkContentParser::extract_text(&content), "[Image]");
    }

    #[test]
    fn test_parse_file_content() {
        let json = serde_json::json!({
            "file_key": "file_12345",
            "file_name": "document.pdf"
        });
        let content = LarkContentParser::parse("file", json).unwrap();
        assert!(matches!(content, LarkContent::File(_)));
        assert_eq!(
            LarkContentParser::extract_text(&content),
            "[File: document.pdf]"
        );
    }

    #[test]
    fn test_parse_post_content() {
        let json = serde_json::json!({
            "content": {
                "content": [
                    [
                        {"tag": "text", "text": "Hello "},
                        {"tag": "at", "user_id": "user123", "user_name": "John"},
                        {"tag": "text", "text": "!"}
                    ]
                ]
            }
        });
        let content = LarkContentParser::parse("post", json).unwrap();
        assert!(matches!(content, LarkContent::Post(_)));
        let text = LarkContentParser::extract_text(&content);
        assert!(text.contains("Hello"));
        assert!(text.contains("John"));
    }

    #[test]
    fn test_create_text() {
        let content = LarkContentParser::create_text("Test message");
        assert!(
            matches!(content, LarkContent::Text(LarkTextContent { text }) if text == "Test message")
        );
    }

    #[test]
    fn test_create_image() {
        let content = LarkContentParser::create_image("img_key_123");
        assert!(
            matches!(content, LarkContent::Image(LarkImageContent { image_key }) if image_key == "img_key_123")
        );
    }

    #[test]
    fn test_get_msg_type() {
        let text = LarkContent::Text(LarkTextContent::default());
        assert_eq!(LarkContentParser::get_msg_type(&text), "text");

        let image = LarkContent::Image(LarkImageContent::default());
        assert_eq!(LarkContentParser::get_msg_type(&image), "image");

        let file = LarkContent::File(LarkFileContent::default());
        assert_eq!(LarkContentParser::get_msg_type(&file), "file");
    }

    #[test]
    fn test_unknown_msg_type() {
        let json = serde_json::json!({});
        let result = LarkContentParser::parse("unknown", json);
        assert!(result.is_err());
    }

    #[test]
    fn test_serialize_content() {
        let content = LarkContentParser::create_text("Test");
        let json = LarkContentParser::to_json(&content).unwrap();
        assert!(json.contains("Test"));
    }
}

// =============================================================================
// 🟢 P0 FIX: PlatformContent trait implementation for unified content framework
// =============================================================================

impl PlatformContent for LarkContent {
    fn content_type(&self) -> UnifiedContentType {
        match self {
            LarkContent::Text(_) => UnifiedContentType::Text,
            LarkContent::Post(_) => UnifiedContentType::Rich,
            LarkContent::Image(_) => UnifiedContentType::Image,
            LarkContent::File(_) => UnifiedContentType::File,
            LarkContent::Interactive(_) => UnifiedContentType::Card,
            LarkContent::ShareChat(_) => UnifiedContentType::Rich,
            LarkContent::Audio(_) => UnifiedContentType::Audio,
            LarkContent::Sticker(_) => UnifiedContentType::Sticker,
        }
    }

    fn extract_text(&self) -> String {
        LarkContentParser::extract_text(self)
    }
}

impl LarkImageContent {
    /// Convert to unified MediaContent
    pub fn to_media_content(&self) -> MediaContent {
        MediaContent {
            url: self.image_key.clone(),
            mime_type: None,
            filename: None,
            size: None,
            width: None,
            height: None,
            duration: None,
            caption: None,
            thumbnail: None,
        }
    }
}

impl LarkFileContent {
    /// Convert to unified MediaContent
    pub fn to_media_content(&self) -> MediaContent {
        MediaContent {
            url: self.file_key.clone(),
            mime_type: None,
            filename: self.file_name.clone(),
            size: None,
            width: None,
            height: None,
            duration: None,
            caption: None,
            thumbnail: None,
        }
    }
}

impl LarkAudioContent {
    /// Convert to unified MediaContent
    pub fn to_media_content(&self) -> MediaContent {
        MediaContent {
            url: self.file_key.clone(),
            mime_type: None,
            filename: None,
            size: None,
            width: None,
            height: None,
            duration: self.duration,
            caption: None,
            thumbnail: None,
        }
    }
}
