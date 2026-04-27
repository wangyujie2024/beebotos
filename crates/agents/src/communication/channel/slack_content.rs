//! Slack Message Content Parser
//!
//! Provides parsing and handling of various Slack message content types
//! including text, blocks, files, images, videos, and rich text.

use serde::{Deserialize, Serialize};

use crate::communication::channel::content::{
    ContentType as UnifiedContentType, MediaContent, PlatformContent,
};
use crate::error::{AgentError, Result};

/// Slack message content types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "content")]
pub enum SlackContent {
    /// Plain text message
    #[serde(rename = "text")]
    Text(SlackTextContent),
    /// Block Kit blocks
    #[serde(rename = "blocks")]
    Blocks(SlackBlocksContent),
    /// File attachment
    #[serde(rename = "file")]
    File(SlackFileContent),
    /// Image attachment
    #[serde(rename = "image")]
    Image(SlackImageContent),
    /// Video attachment
    #[serde(rename = "video")]
    Video(SlackVideoContent),
    /// Rich text (mrkdwn with formatting)
    #[serde(rename = "rich_text")]
    RichText(SlackRichTextContent),
    /// Interactive message
    #[serde(rename = "interactive")]
    Interactive(SlackInteractiveContent),
}

/// Text content structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SlackTextContent {
    /// The text content (supports mrkdwn)
    pub text: String,
    /// Whether to unfurl links
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "unfurl_links")]
    pub unfurl_links: Option<bool>,
    /// Whether to unfurl media
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "unfurl_media")]
    pub unfurl_media: Option<bool>,
    /// Parse mode (full, none, or client)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "parse")]
    pub parse: Option<String>,
    /// Link names
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "link_names")]
    pub link_names: Option<bool>,
    /// Reply broadcast (for thread replies)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "reply_broadcast")]
    pub reply_broadcast: Option<bool>,
}

/// Blocks content structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SlackBlocksContent {
    /// Block Kit blocks
    pub blocks: Vec<SlackBlock>,
    /// Fallback text for notifications
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "text")]
    pub fallback_text: Option<String>,
}

/// Slack Block Kit block
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlackBlock {
    /// Block type
    #[serde(rename = "type")]
    pub block_type: String,
    /// Block ID
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "block_id")]
    pub block_id: Option<String>,
    /// Text (for section and header blocks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<SlackTextObject>,
    /// Elements (for context and actions blocks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elements: Option<Vec<SlackBlockElement>>,
    /// Accessory (for section blocks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessory: Option<Box<SlackBlockElement>>,
    /// Fields (for section blocks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<SlackTextObject>>,
    /// Image URL (for image blocks)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "image_url")]
    pub image_url: Option<String>,
    /// Alt text (for image blocks)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "alt_text")]
    pub alt_text: Option<String>,
    /// Title (for image blocks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<SlackTextObject>,
    /// Label (for input blocks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<SlackTextObject>,
    /// Element (for input blocks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element: Option<Box<SlackBlockElement>>,
    /// Hint (for input blocks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<SlackTextObject>,
    /// Optional (for input blocks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optional: Option<bool>,
}

/// Slack text object
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlackTextObject {
    /// Text type (plain_text or mrkdwn)
    #[serde(rename = "type")]
    pub text_type: String,
    /// Text content
    pub text: String,
    /// Whether to enable emoji
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<bool>,
    /// Whether to escape characters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbatim: Option<bool>,
}

/// Slack block element
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlackBlockElement {
    /// Element type
    #[serde(rename = "type")]
    pub element_type: String,
    /// Action ID
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "action_id")]
    pub action_id: Option<String>,
    /// Text (for button and static_select)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<SlackTextObject>,
    /// URL (for button)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Value (for button and static_select)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Style (for button: primary, danger)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    /// Placeholder (for input elements)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<SlackTextObject>,
    /// Options (for static_select)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<SlackOptionObject>>,
    /// Initial options (for multi_static_select)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "initial_options")]
    pub initial_options: Option<Vec<SlackOptionObject>>,
    /// Initial option (for static_select)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "initial_option")]
    pub initial_option: Option<SlackOptionObject>,
    /// Multiline (for plain_text_input)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiline: Option<bool>,
    /// Min length (for plain_text_input)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "min_length")]
    pub min_length: Option<i32>,
    /// Max length (for plain_text_input)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "max_length")]
    pub max_length: Option<i32>,
    /// Initial value (for plain_text_input)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "initial_value")]
    pub initial_value: Option<String>,
    /// Image URL (for image element)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "image_url")]
    pub image_url: Option<String>,
    /// Alt text (for image element)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "alt_text")]
    pub alt_text: Option<String>,
}

/// Slack option object
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlackOptionObject {
    /// Option text
    pub text: SlackTextObject,
    /// Option value
    pub value: String,
    /// Description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<SlackTextObject>,
    /// URL (for overflow menu)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// File content structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SlackFileContent {
    /// File ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// File name
    pub filename: String,
    /// File URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// File URL for download
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "url_private")]
    pub url_private: Option<String>,
    /// Content type (MIME type)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "mimetype")]
    pub mime_type: Option<String>,
    /// File type
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "filetype")]
    pub file_type: Option<String>,
    /// File size in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    /// Title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Initial comment
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "initial_comment")]
    pub initial_comment: Option<String>,
    /// Channels to share to
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<Vec<String>>,
}

/// Image content structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SlackImageContent {
    /// Image URL
    pub url: String,
    /// Image width
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    /// Image height
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
    /// Image size in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    /// Alt text
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "alt_text")]
    pub alt_text: Option<String>,
    /// Title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Video content structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SlackVideoContent {
    /// Video URL
    pub url: String,
    /// Video width
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    /// Video height
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
    /// Video duration in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<i32>,
    /// Thumbnail URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    /// Title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Rich text content structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SlackRichTextContent {
    /// Rich text sections
    pub sections: Vec<SlackRichTextSection>,
}

/// Rich text section
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlackRichTextSection {
    /// Section type
    #[serde(rename = "type")]
    pub section_type: String,
    /// Elements in the section
    pub elements: Vec<SlackRichTextElement>,
}

/// Rich text element
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlackRichTextElement {
    /// Element type (text, user, channel, emoji, link)
    #[serde(rename = "type")]
    pub element_type: String,
    /// Text content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// User ID (for user mentions)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "user_id")]
    pub user_id: Option<String>,
    /// Channel ID (for channel mentions)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "channel_id")]
    pub channel_id: Option<String>,
    /// Emoji name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// URL (for links)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Style (bold, italic, strike, code)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<SlackTextStyle>,
}

/// Text style
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SlackTextStyle {
    /// Bold
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    /// Italic
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    /// Strikethrough
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strike: Option<bool>,
    /// Code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<bool>,
}

/// Interactive content structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SlackInteractiveContent {
    /// Callback ID
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "callback_id")]
    pub callback_id: Option<String>,
    /// Actions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actions: Option<Vec<SlackAction>>,
    /// Attachment actions (legacy)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "attachment_actions")]
    pub attachment_actions: Option<Vec<SlackAttachmentAction>>,
}

/// Slack action
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlackAction {
    /// Action ID
    #[serde(rename = "action_id")]
    pub action_id: String,
    /// Block ID
    #[serde(rename = "block_id")]
    pub block_id: String,
    /// Action type
    #[serde(rename = "type")]
    pub action_type: String,
    /// Selected option value
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "selected_option")]
    pub selected_option: Option<SlackOptionObject>,
    /// Selected options (for multi-select)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "selected_options")]
    pub selected_options: Option<Vec<SlackOptionObject>>,
    /// Value (for buttons)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Action timestamp
    #[serde(rename = "action_ts")]
    pub action_ts: String,
}

/// Slack attachment action (legacy)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlackAttachmentAction {
    /// Action name
    pub name: String,
    /// Action type
    #[serde(rename = "type")]
    pub action_type: String,
    /// Selected options
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "selected_options")]
    pub selected_options: Option<Vec<serde_json::Value>>,
    /// Value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// Slack content parser
#[derive(Debug, Clone, Default)]
pub struct SlackContentParser;

impl SlackContentParser {
    /// Create a new content parser
    pub fn new() -> Self {
        Self
    }

    /// Parse content from JSON value
    ///
    /// # Arguments
    /// * `content_type` - The content type (text, blocks, file, image, video,
    ///   rich_text, interactive)
    /// * `content` - The JSON content to parse
    ///
    /// # Returns
    /// Parsed SlackContent enum variant
    pub fn parse(content_type: &str, content: serde_json::Value) -> Result<SlackContent> {
        match content_type {
            "text" => {
                let text_content: SlackTextContent =
                    serde_json::from_value(content).map_err(|e| {
                        AgentError::platform(format!("Failed to parse text content: {}", e))
                    })?;
                Ok(SlackContent::Text(text_content))
            }
            "blocks" => {
                let blocks_content: SlackBlocksContent =
                    serde_json::from_value(content).map_err(|e| {
                        AgentError::platform(format!("Failed to parse blocks content: {}", e))
                    })?;
                Ok(SlackContent::Blocks(blocks_content))
            }
            "file" => {
                let file_content: SlackFileContent =
                    serde_json::from_value(content).map_err(|e| {
                        AgentError::platform(format!("Failed to parse file content: {}", e))
                    })?;
                Ok(SlackContent::File(file_content))
            }
            "image" => {
                let image_content: SlackImageContent =
                    serde_json::from_value(content).map_err(|e| {
                        AgentError::platform(format!("Failed to parse image content: {}", e))
                    })?;
                Ok(SlackContent::Image(image_content))
            }
            "video" => {
                let video_content: SlackVideoContent =
                    serde_json::from_value(content).map_err(|e| {
                        AgentError::platform(format!("Failed to parse video content: {}", e))
                    })?;
                Ok(SlackContent::Video(video_content))
            }
            "rich_text" => {
                let rich_text_content: SlackRichTextContent = serde_json::from_value(content)
                    .map_err(|e| {
                        AgentError::platform(format!("Failed to parse rich text content: {}", e))
                    })?;
                Ok(SlackContent::RichText(rich_text_content))
            }
            "interactive" => {
                let interactive_content: SlackInteractiveContent = serde_json::from_value(content)
                    .map_err(|e| {
                        AgentError::platform(format!("Failed to parse interactive content: {}", e))
                    })?;
                Ok(SlackContent::Interactive(interactive_content))
            }
            _ => {
                Err(AgentError::platform(format!("Unknown content type: {}", content_type)).into())
            }
        }
    }

    /// Parse content from JSON string
    ///
    /// # Arguments
    /// * `content_type` - The content type
    /// * `content_json` - JSON string to parse
    ///
    /// # Returns
    /// Parsed SlackContent enum variant
    pub fn parse_str(content_type: &str, content_json: &str) -> Result<SlackContent> {
        let content: serde_json::Value = serde_json::from_str(content_json)
            .map_err(|e| AgentError::platform(format!("Invalid JSON: {}", e)))?;
        Self::parse(content_type, content)
    }

    /// Extract plain text from any content type
    ///
    /// # Arguments
    /// * `content` - The SlackContent to extract text from
    ///
    /// # Returns
    /// Extracted plain text string
    pub fn extract_text(content: &SlackContent) -> String {
        match content {
            SlackContent::Text(text) => text.text.clone(),
            SlackContent::Blocks(blocks) => {
                // Extract text from blocks
                let mut texts = Vec::new();
                for block in &blocks.blocks {
                    if let Some(text) = &block.text {
                        texts.push(text.text.clone());
                    }
                    if let Some(fields) = &block.fields {
                        for field in fields {
                            texts.push(field.text.clone());
                        }
                    }
                }
                texts.join("\n")
            }
            SlackContent::File(file) => {
                format!("[File: {}]", file.filename)
            }
            SlackContent::Image(image) => {
                format!("[Image] {}", image.title.as_deref().unwrap_or("(no title)"))
            }
            SlackContent::Video(video) => {
                format!(
                    "[Video: {}s] {}",
                    video
                        .duration
                        .map(|d| d.to_string())
                        .unwrap_or_else(|| "?".to_string()),
                    video.title.as_deref().unwrap_or("(no title)")
                )
            }
            SlackContent::RichText(rich_text) => {
                let mut texts = Vec::new();
                for section in &rich_text.sections {
                    for element in &section.elements {
                        if let Some(text) = &element.text {
                            texts.push(text.clone());
                        }
                    }
                }
                texts.join("")
            }
            SlackContent::Interactive(_) => "[Interactive Message]".to_string(),
        }
    }

    /// Get content type string
    ///
    /// # Arguments
    /// * `content` - The SlackContent
    ///
    /// # Returns
    /// Content type string
    pub fn get_content_type(content: &SlackContent) -> &'static str {
        match content {
            SlackContent::Text(_) => "text",
            SlackContent::Blocks(_) => "blocks",
            SlackContent::File(_) => "file",
            SlackContent::Image(_) => "image",
            SlackContent::Video(_) => "video",
            SlackContent::RichText(_) => "rich_text",
            SlackContent::Interactive(_) => "interactive",
        }
    }

    /// Create text content
    ///
    /// # Arguments
    /// * `text` - The text content
    ///
    /// # Returns
    /// SlackContent::Text variant
    pub fn create_text(text: impl Into<String>) -> SlackContent {
        SlackContent::Text(SlackTextContent {
            text: text.into(),
            unfurl_links: None,
            unfurl_media: None,
            parse: None,
            link_names: None,
            reply_broadcast: None,
        })
    }

    /// Create blocks content
    ///
    /// # Arguments
    /// * `blocks` - The Block Kit blocks
    ///
    /// # Returns
    /// SlackContent::Blocks variant
    pub fn create_blocks(blocks: Vec<SlackBlock>) -> SlackContent {
        SlackContent::Blocks(SlackBlocksContent {
            blocks,
            fallback_text: None,
        })
    }

    /// Create file content
    ///
    /// # Arguments
    /// * `filename` - The file name
    /// * `url` - Optional file URL
    ///
    /// # Returns
    /// SlackContent::File variant
    pub fn create_file(filename: impl Into<String>, url: Option<String>) -> SlackContent {
        SlackContent::File(SlackFileContent {
            id: None,
            filename: filename.into(),
            url: url.clone(),
            url_private: url,
            mime_type: None,
            file_type: None,
            size: None,
            title: None,
            initial_comment: None,
            channels: None,
        })
    }

    /// Create image content
    ///
    /// # Arguments
    /// * `url` - The image URL
    ///
    /// # Returns
    /// SlackContent::Image variant
    pub fn create_image(url: impl Into<String>) -> SlackContent {
        SlackContent::Image(SlackImageContent {
            url: url.into(),
            width: None,
            height: None,
            size: None,
            alt_text: None,
            title: None,
        })
    }

    /// Create video content
    ///
    /// # Arguments
    /// * `url` - The video URL
    ///
    /// # Returns
    /// SlackContent::Video variant
    pub fn create_video(url: impl Into<String>) -> SlackContent {
        SlackContent::Video(SlackVideoContent {
            url: url.into(),
            width: None,
            height: None,
            duration: None,
            thumbnail: None,
            title: None,
        })
    }

    /// Create rich text content
    ///
    /// # Arguments
    /// * `sections` - Rich text sections
    ///
    /// # Returns
    /// SlackContent::RichText variant
    pub fn create_rich_text(sections: Vec<SlackRichTextSection>) -> SlackContent {
        SlackContent::RichText(SlackRichTextContent { sections })
    }

    /// Create interactive content
    ///
    /// # Arguments
    /// * `callback_id` - Callback ID
    ///
    /// # Returns
    /// SlackContent::Interactive variant
    pub fn create_interactive(callback_id: impl Into<String>) -> SlackContent {
        SlackContent::Interactive(SlackInteractiveContent {
            callback_id: Some(callback_id.into()),
            actions: None,
            attachment_actions: None,
        })
    }

    /// Serialize content to JSON string
    ///
    /// # Arguments
    /// * `content` - The SlackContent to serialize
    ///
    /// # Returns
    /// JSON string representation
    pub fn to_json(content: &SlackContent) -> Result<String> {
        serde_json::to_string(content)
            .map_err(|e| AgentError::platform(format!("Failed to serialize content: {}", e)).into())
    }

    /// Serialize content to JSON value
    ///
    /// # Arguments
    /// * `content` - The SlackContent to serialize
    ///
    /// # Returns
    /// JSON value representation
    pub fn to_json_value(content: &SlackContent) -> Result<serde_json::Value> {
        serde_json::to_value(content)
            .map_err(|e| AgentError::platform(format!("Failed to serialize content: {}", e)).into())
    }

    /// Convert mrkdwn to plain text
    ///
    /// # Arguments
    /// * `mrkdwn` - mrkdwn formatted text
    ///
    /// # Returns
    /// Plain text
    pub fn mrkdwn_to_plain_text(mrkdwn: &str) -> String {
        let mut text = mrkdwn.to_string();

        // Remove bold formatting
        text = text.replace("*", "");
        // Remove italic formatting
        text = text.replace("_", "");
        // Remove strikethrough formatting
        text = text.replace("~", "");
        // Remove code formatting
        text = text.replace("`", "");
        // Remove code block formatting
        text = text.replace("```", "");
        // Convert mentions
        text = text.replace("<@", "@");
        text = text.replace("<#", "#");
        text = text.replace("<!", "@");
        text = text.replace(">", "");
        // Convert links
        text = text.replace("<http", "http");
        text = text.replace("|", " (");

        text
    }

    /// Convert plain text to mrkdwn
    ///
    /// # Arguments
    /// * `text` - Plain text
    ///
    /// # Returns
    /// mrkdwn formatted text
    pub fn plain_text_to_mrkdwn(text: &str) -> String {
        // Escape special characters
        let mut mrkdwn = text.to_string();
        mrkdwn = mrkdwn.replace("&", "&amp;");
        mrkdwn = mrkdwn.replace("<", "&lt;");
        mrkdwn = mrkdwn.replace(">", "&gt;");
        mrkdwn
    }
}

/// Block Kit builder for easy block creation
pub struct BlockKitBuilder;

impl BlockKitBuilder {
    /// Create a header block
    pub fn header(text: impl Into<String>) -> SlackBlock {
        SlackBlock {
            block_type: "header".to_string(),
            block_id: None,
            text: Some(SlackTextObject {
                text_type: "plain_text".to_string(),
                text: text.into(),
                emoji: Some(true),
                verbatim: None,
            }),
            elements: None,
            accessory: None,
            fields: None,
            image_url: None,
            alt_text: None,
            title: None,
            label: None,
            element: None,
            hint: None,
            optional: None,
        }
    }

    /// Create a section block with text
    pub fn section(text: impl Into<String>) -> SlackBlock {
        SlackBlock {
            block_type: "section".to_string(),
            block_id: None,
            text: Some(SlackTextObject {
                text_type: "mrkdwn".to_string(),
                text: text.into(),
                emoji: None,
                verbatim: None,
            }),
            elements: None,
            accessory: None,
            fields: None,
            image_url: None,
            alt_text: None,
            title: None,
            label: None,
            element: None,
            hint: None,
            optional: None,
        }
    }

    /// Create a section block with fields
    pub fn section_fields(fields: Vec<(impl Into<String>, bool)>) -> SlackBlock {
        let text_objects: Vec<SlackTextObject> = fields
            .into_iter()
            .map(|(text, is_markdown)| SlackTextObject {
                text_type: if is_markdown {
                    "mrkdwn".to_string()
                } else {
                    "plain_text".to_string()
                },
                text: text.into(),
                emoji: None,
                verbatim: None,
            })
            .collect();

        SlackBlock {
            block_type: "section".to_string(),
            block_id: None,
            text: None,
            elements: None,
            accessory: None,
            fields: Some(text_objects),
            image_url: None,
            alt_text: None,
            title: None,
            label: None,
            element: None,
            hint: None,
            optional: None,
        }
    }

    /// Create a divider block
    pub fn divider() -> SlackBlock {
        SlackBlock {
            block_type: "divider".to_string(),
            block_id: None,
            text: None,
            elements: None,
            accessory: None,
            fields: None,
            image_url: None,
            alt_text: None,
            title: None,
            label: None,
            element: None,
            hint: None,
            optional: None,
        }
    }

    /// Create an image block
    pub fn image(image_url: impl Into<String>, alt_text: impl Into<String>) -> SlackBlock {
        SlackBlock {
            block_type: "image".to_string(),
            block_id: None,
            text: None,
            elements: None,
            accessory: None,
            fields: None,
            image_url: Some(image_url.into()),
            alt_text: Some(alt_text.into()),
            title: None,
            label: None,
            element: None,
            hint: None,
            optional: None,
        }
    }

    /// Create an image block with title
    pub fn image_with_title(
        image_url: impl Into<String>,
        alt_text: impl Into<String>,
        title: impl Into<String>,
    ) -> SlackBlock {
        SlackBlock {
            block_type: "image".to_string(),
            block_id: None,
            text: None,
            elements: None,
            accessory: None,
            fields: None,
            image_url: Some(image_url.into()),
            alt_text: Some(alt_text.into()),
            title: Some(SlackTextObject {
                text_type: "plain_text".to_string(),
                text: title.into(),
                emoji: Some(true),
                verbatim: None,
            }),
            label: None,
            element: None,
            hint: None,
            optional: None,
        }
    }

    /// Create a context block
    pub fn context(elements: Vec<SlackBlockElement>) -> SlackBlock {
        SlackBlock {
            block_type: "context".to_string(),
            block_id: None,
            text: None,
            elements: Some(elements),
            accessory: None,
            fields: None,
            image_url: None,
            alt_text: None,
            title: None,
            label: None,
            element: None,
            hint: None,
            optional: None,
        }
    }

    /// Create an actions block
    pub fn actions(elements: Vec<SlackBlockElement>) -> SlackBlock {
        SlackBlock {
            block_type: "actions".to_string(),
            block_id: None,
            text: None,
            elements: Some(elements),
            accessory: None,
            fields: None,
            image_url: None,
            alt_text: None,
            title: None,
            label: None,
            element: None,
            hint: None,
            optional: None,
        }
    }

    /// Create a button element
    pub fn button(
        text: impl Into<String>,
        action_id: impl Into<String>,
        value: Option<String>,
        style: Option<String>,
    ) -> SlackBlockElement {
        SlackBlockElement {
            element_type: "button".to_string(),
            action_id: Some(action_id.into()),
            text: Some(SlackTextObject {
                text_type: "plain_text".to_string(),
                text: text.into(),
                emoji: Some(true),
                verbatim: None,
            }),
            url: None,
            value,
            style,
            placeholder: None,
            options: None,
            initial_options: None,
            initial_option: None,
            multiline: None,
            min_length: None,
            max_length: None,
            initial_value: None,
            image_url: None,
            alt_text: None,
        }
    }

    /// Create an image element (for context blocks)
    pub fn image_element(
        image_url: impl Into<String>,
        alt_text: impl Into<String>,
    ) -> SlackBlockElement {
        SlackBlockElement {
            element_type: "image".to_string(),
            action_id: None,
            text: None,
            url: None,
            value: None,
            style: None,
            placeholder: None,
            options: None,
            initial_options: None,
            initial_option: None,
            multiline: None,
            min_length: None,
            max_length: None,
            initial_value: None,
            image_url: Some(image_url.into()),
            alt_text: Some(alt_text.into()),
        }
    }

    /// Create a text element (for context blocks)
    pub fn text_element(text: impl Into<String>) -> SlackBlockElement {
        SlackBlockElement {
            element_type: "mrkdwn".to_string(),
            action_id: None,
            text: Some(SlackTextObject {
                text_type: "mrkdwn".to_string(),
                text: text.into(),
                emoji: None,
                verbatim: None,
            }),
            url: None,
            value: None,
            style: None,
            placeholder: None,
            options: None,
            initial_options: None,
            initial_option: None,
            multiline: None,
            min_length: None,
            max_length: None,
            initial_value: None,
            image_url: None,
            alt_text: None,
        }
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
        let content = SlackContentParser::parse("text", json).unwrap();
        assert!(matches!(content, SlackContent::Text(_)));
        assert_eq!(SlackContentParser::extract_text(&content), "Hello, World!");
    }

    #[test]
    fn test_parse_blocks_content() {
        let json = serde_json::json!({
            "blocks": [
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": "Hello"
                    }
                }
            ]
        });
        let content = SlackContentParser::parse("blocks", json).unwrap();
        assert!(matches!(content, SlackContent::Blocks(_)));
    }

    #[test]
    fn test_block_kit_builder() {
        let header = BlockKitBuilder::header("Test Header");
        assert_eq!(header.block_type, "header");

        let section = BlockKitBuilder::section("Test Section");
        assert_eq!(section.block_type, "section");

        let divider = BlockKitBuilder::divider();
        assert_eq!(divider.block_type, "divider");

        let image = BlockKitBuilder::image("https://example.com/image.png", "Example");
        assert_eq!(image.block_type, "image");
    }

    #[test]
    fn test_section_fields() {
        let fields = vec![("Field 1", true), ("Field 2", false)];
        let section = BlockKitBuilder::section_fields(fields);
        assert_eq!(section.block_type, "section");
        assert_eq!(section.fields.as_ref().map(|f| f.len()), Some(2));
    }

    #[test]
    fn test_button_element() {
        let button = BlockKitBuilder::button(
            "Click me",
            "button_action",
            Some("button_value".to_string()),
            Some("primary".to_string()),
        );
        assert_eq!(button.element_type, "button");
        assert_eq!(button.action_id, Some("button_action".to_string()));
    }

    #[test]
    fn test_get_content_type() {
        let text = SlackContent::Text(SlackTextContent::default());
        assert_eq!(SlackContentParser::get_content_type(&text), "text");

        let blocks = SlackContent::Blocks(SlackBlocksContent::default());
        assert_eq!(SlackContentParser::get_content_type(&blocks), "blocks");

        let file = SlackContent::File(SlackFileContent::default());
        assert_eq!(SlackContentParser::get_content_type(&file), "file");
    }

    #[test]
    fn test_create_text() {
        let content = SlackContentParser::create_text("Test message");
        assert!(
            matches!(content, SlackContent::Text(SlackTextContent { text, .. }) if text == "Test message")
        );
    }

    #[test]
    fn test_create_blocks() {
        let blocks = vec![BlockKitBuilder::section("Test")];
        let content = SlackContentParser::create_blocks(blocks);
        assert!(matches!(content, SlackContent::Blocks(_)));
    }

    #[test]
    fn test_mrkdwn_to_plain_text() {
        let mrkdwn = "Hello *bold* _italic_ ~strike~ `code` <@U123> <#C456>";
        let plain = SlackContentParser::mrkdwn_to_plain_text(mrkdwn);
        assert!(plain.contains("Hello"));
        assert!(plain.contains("bold"));
        assert!(!plain.contains("*"));
    }

    #[test]
    fn test_plain_text_to_mrkdwn() {
        let text = "Hello <world> & test";
        let mrkdwn = SlackContentParser::plain_text_to_mrkdwn(text);
        assert!(mrkdwn.contains("&lt;"));
        assert!(mrkdwn.contains("&gt;"));
        assert!(mrkdwn.contains("&amp;"));
    }

    #[test]
    fn test_serialize_content() {
        let content = SlackContentParser::create_text("Test");
        let json = SlackContentParser::to_json(&content).unwrap();
        assert!(json.contains("Test"));
    }

    #[test]
    fn test_rich_text_extraction() {
        let rich_text = SlackRichTextContent {
            sections: vec![SlackRichTextSection {
                section_type: "rich_text_section".to_string(),
                elements: vec![
                    SlackRichTextElement {
                        element_type: "text".to_string(),
                        text: Some("Hello ".to_string()),
                        user_id: None,
                        channel_id: None,
                        name: None,
                        url: None,
                        style: None,
                    },
                    SlackRichTextElement {
                        element_type: "user".to_string(),
                        text: None,
                        user_id: Some("U123".to_string()),
                        channel_id: None,
                        name: None,
                        url: None,
                        style: None,
                    },
                ],
            }],
        };

        let content = SlackContent::RichText(rich_text);
        let text = SlackContentParser::extract_text(&content);
        assert!(text.contains("Hello"));
    }
}

// =============================================================================
// 🟢 P0 FIX: PlatformContent trait implementation for unified content framework
// =============================================================================

impl PlatformContent for SlackContent {
    fn content_type(&self) -> UnifiedContentType {
        match self {
            SlackContent::Text(_) => UnifiedContentType::Text,
            SlackContent::Blocks(_) => UnifiedContentType::Rich,
            SlackContent::File(_) => UnifiedContentType::File,
            SlackContent::Image(_) => UnifiedContentType::Image,
            SlackContent::Video(_) => UnifiedContentType::Video,
            SlackContent::RichText(_) => UnifiedContentType::Rich,
            SlackContent::Interactive(_) => UnifiedContentType::Card,
        }
    }

    fn extract_text(&self) -> String {
        SlackContentParser::extract_text(self)
    }

    fn as_media(&self) -> Option<&MediaContent> {
        // Slack content doesn't directly use MediaContent,
        // but we can convert for unified handling
        None
    }
}

impl SlackImageContent {
    /// Convert to unified MediaContent
    pub fn to_media_content(&self) -> MediaContent {
        MediaContent {
            url: self.url.clone(),
            width: self.width,
            height: self.height,
            size: self.size,
            mime_type: None,
            filename: self.title.clone(),
            caption: None,
            thumbnail: None,
            duration: None,
        }
    }
}

impl SlackVideoContent {
    /// Convert to unified MediaContent
    pub fn to_media_content(&self) -> MediaContent {
        MediaContent {
            url: self.url.clone(),
            width: self.width,
            height: self.height,
            duration: self.duration,
            thumbnail: self.thumbnail.clone(),
            mime_type: None,
            filename: self.title.clone(),
            caption: None,
            size: None,
        }
    }
}

impl SlackFileContent {
    /// Convert to unified MediaContent
    pub fn to_media_content(&self) -> MediaContent {
        MediaContent {
            url: self.url.clone().unwrap_or_default(),
            mime_type: self.mime_type.clone(),
            filename: Some(self.filename.clone()),
            size: self.size,
            width: None,
            height: None,
            duration: None,
            caption: self.initial_comment.clone(),
            thumbnail: None,
        }
    }
}
