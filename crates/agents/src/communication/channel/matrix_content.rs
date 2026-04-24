//! Matrix Message Content Parser
//!
//! Provides parsing and handling of various Matrix message content types
//! including text, HTML, images, files, audio, video, location, emote, and
//! notice.

#![allow(unreachable_patterns)]

use serde::{Deserialize, Serialize};

use crate::communication::channel::content::{
    ContentType as UnifiedContentType, MediaContent, PlatformContent,
};
use crate::error::{AgentError, Result};

/// Matrix message content types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "msgtype")]
#[allow(unreachable_patterns)]
pub enum MatrixContent {
    /// Plain text message
    #[serde(rename = "m.text")]
    Text(MatrixTextContent),
    /// HTML formatted message
    #[serde(rename = "m.text")]
    Html {
        #[serde(flatten)]
        content: MatrixTextContent,
        format: String,
        #[serde(rename = "formatted_body")]
        formatted_body: String,
    },
    /// Image message
    #[serde(rename = "m.image")]
    Image(MatrixImageContent),
    /// File message
    #[serde(rename = "m.file")]
    File(MatrixFileContent),
    /// Audio message
    #[serde(rename = "m.audio")]
    Audio(MatrixAudioContent),
    /// Video message
    #[serde(rename = "m.video")]
    Video(MatrixVideoContent),
    /// Location message
    #[serde(rename = "m.location")]
    Location(MatrixLocationContent),
    /// Emote message (/me style)
    #[serde(rename = "m.emote")]
    Emote(MatrixTextContent),
    /// Notice message (bot message)
    #[serde(rename = "m.notice")]
    Notice(MatrixTextContent),
}

/// Text content structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MatrixTextContent {
    /// The text content
    pub body: String,
}

/// Image content structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MatrixImageContent {
    /// Alt text for the image
    pub body: String,
    /// MXC URI of the image
    pub url: String,
    /// Image metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<MatrixImageInfo>,
}

/// Image info structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MatrixImageInfo {
    /// Image width in pixels
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<u32>,
    /// Image height in pixels
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h: Option<u32>,
    /// MIME type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mimetype: Option<String>,
    /// Size in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Thumbnail URL (MXC)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    /// Thumbnail info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_info: Option<MatrixThumbnailInfo>,
}

/// Thumbnail info structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MatrixThumbnailInfo {
    /// Thumbnail width
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<u32>,
    /// Thumbnail height
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h: Option<u32>,
    /// MIME type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mimetype: Option<String>,
    /// Size in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

/// File content structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MatrixFileContent {
    /// Display filename
    pub body: String,
    /// MXC URI of the file
    pub url: String,
    /// Original filename
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// File metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<MatrixFileInfo>,
}

/// File info structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MatrixFileInfo {
    /// MIME type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mimetype: Option<String>,
    /// Size in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Thumbnail URL (MXC) for documents
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    /// Thumbnail info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_info: Option<MatrixThumbnailInfo>,
}

/// Audio content structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MatrixAudioContent {
    /// Description of the audio
    pub body: String,
    /// MXC URI of the audio
    pub url: String,
    /// Audio metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<MatrixAudioInfo>,
}

/// Audio info structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MatrixAudioInfo {
    /// MIME type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mimetype: Option<String>,
    /// Size in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Duration in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<u32>,
}

/// Video content structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MatrixVideoContent {
    /// Description of the video
    pub body: String,
    /// MXC URI of the video
    pub url: String,
    /// Video metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<MatrixVideoInfo>,
}

/// Video info structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MatrixVideoInfo {
    /// MIME type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mimetype: Option<String>,
    /// Size in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Duration in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<u32>,
    /// Video width
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<u32>,
    /// Video height
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h: Option<u32>,
    /// Thumbnail URL (MXC)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    /// Thumbnail info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_info: Option<MatrixThumbnailInfo>,
}

/// Location content structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MatrixLocationContent {
    /// Location description
    pub body: String,
    /// Geo URI (e.g., geo:37.786971,-122.399677;u=35)
    pub geo_uri: String,
    /// Location metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<MatrixLocationInfo>,
}

/// Location info structure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MatrixLocationInfo {
    /// Thumbnail URL (MXC)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    /// Thumbnail info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_info: Option<MatrixThumbnailInfo>,
}

/// Matrix content parser for parsing message content from JSON
#[derive(Debug, Clone, Default)]
pub struct MatrixContentParser;

impl MatrixContentParser {
    /// Create a new content parser
    pub fn new() -> Self {
        Self
    }

    /// Parse content from JSON value
    ///
    /// # Arguments
    /// * `content` - The JSON content to parse
    ///
    /// # Returns
    /// Parsed MatrixContent enum variant
    pub fn parse(content: serde_json::Value) -> Result<MatrixContent> {
        let msgtype = content
            .get("msgtype")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AgentError::platform("Missing msgtype in content"))?;

        // Check if it's an HTML formatted message
        if msgtype == "m.text" && content.get("format").is_some() {
            let text_content: MatrixTextContent =
                serde_json::from_value(content.clone()).map_err(|e| {
                    AgentError::platform(format!("Failed to parse HTML content: {}", e))
                })?;
            let format = content
                .get("format")
                .and_then(|v| v.as_str())
                .unwrap_or("org.matrix.custom.html")
                .to_string();
            let formatted_body = content
                .get("formatted_body")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            return Ok(MatrixContent::Html {
                content: text_content,
                format,
                formatted_body,
            });
        }

        match msgtype {
            "m.text" => {
                let text_content: MatrixTextContent =
                    serde_json::from_value(content).map_err(|e| {
                        AgentError::platform(format!("Failed to parse text content: {}", e))
                    })?;
                Ok(MatrixContent::Text(text_content))
            }
            "m.image" => {
                let image_content: MatrixImageContent =
                    serde_json::from_value(content).map_err(|e| {
                        AgentError::platform(format!("Failed to parse image content: {}", e))
                    })?;
                Ok(MatrixContent::Image(image_content))
            }
            "m.file" => {
                let file_content: MatrixFileContent =
                    serde_json::from_value(content).map_err(|e| {
                        AgentError::platform(format!("Failed to parse file content: {}", e))
                    })?;
                Ok(MatrixContent::File(file_content))
            }
            "m.audio" => {
                let audio_content: MatrixAudioContent =
                    serde_json::from_value(content).map_err(|e| {
                        AgentError::platform(format!("Failed to parse audio content: {}", e))
                    })?;
                Ok(MatrixContent::Audio(audio_content))
            }
            "m.video" => {
                let video_content: MatrixVideoContent =
                    serde_json::from_value(content).map_err(|e| {
                        AgentError::platform(format!("Failed to parse video content: {}", e))
                    })?;
                Ok(MatrixContent::Video(video_content))
            }
            "m.location" => {
                let location_content: MatrixLocationContent = serde_json::from_value(content)
                    .map_err(|e| {
                        AgentError::platform(format!("Failed to parse location content: {}", e))
                    })?;
                Ok(MatrixContent::Location(location_content))
            }
            "m.emote" => {
                let text_content: MatrixTextContent =
                    serde_json::from_value(content).map_err(|e| {
                        AgentError::platform(format!("Failed to parse emote content: {}", e))
                    })?;
                Ok(MatrixContent::Emote(text_content))
            }
            "m.notice" => {
                let text_content: MatrixTextContent =
                    serde_json::from_value(content).map_err(|e| {
                        AgentError::platform(format!("Failed to parse notice content: {}", e))
                    })?;
                Ok(MatrixContent::Notice(text_content))
            }
            _ => Err(AgentError::platform(format!("Unknown message type: {}", msgtype)).into()),
        }
    }

    /// Parse content from JSON string
    ///
    /// # Arguments
    /// * `content_json` - JSON string to parse
    ///
    /// # Returns
    /// Parsed MatrixContent enum variant
    pub fn parse_str(content_json: &str) -> Result<MatrixContent> {
        let content: serde_json::Value = serde_json::from_str(content_json)
            .map_err(|e| AgentError::platform(format!("Invalid JSON: {}", e)))?;
        Self::parse(content)
    }

    /// Extract plain text from any content type
    ///
    /// # Arguments
    /// * `content` - The MatrixContent to extract text from
    ///
    /// # Returns
    /// Extracted plain text string
    pub fn extract_text(content: &MatrixContent) -> String {
        match content {
            MatrixContent::Text(text) => text.body.clone(),
            MatrixContent::Html {
                content,
                formatted_body: _,
                ..
            } => {
                // Return plain text body, but could also strip HTML from formatted_body
                content.body.clone()
            }
            MatrixContent::Image(image) => {
                format!("[Image: {}]", image.body)
            }
            MatrixContent::File(file) => {
                format!("[File: {}]", file.body)
            }
            MatrixContent::Audio(audio) => {
                format!("[Audio: {}]", audio.body)
            }
            MatrixContent::Video(video) => {
                format!("[Video: {}]", video.body)
            }
            MatrixContent::Location(location) => {
                format!("[Location: {} - {}]", location.body, location.geo_uri)
            }
            MatrixContent::Emote(emote) => {
                format!("* {} *", emote.body)
            }
            MatrixContent::Notice(notice) => {
                format!("[Notice: {}]", notice.body)
            }
        }
    }

    /// Get message type from content
    ///
    /// # Arguments
    /// * `content` - The MatrixContent
    ///
    /// # Returns
    /// Message type string
    pub fn get_msg_type(content: &MatrixContent) -> &'static str {
        match content {
            MatrixContent::Text(_) => "m.text",
            MatrixContent::Html { .. } => "m.text",
            MatrixContent::Image(_) => "m.image",
            MatrixContent::File(_) => "m.file",
            MatrixContent::Audio(_) => "m.audio",
            MatrixContent::Video(_) => "m.video",
            MatrixContent::Location(_) => "m.location",
            MatrixContent::Emote(_) => "m.emote",
            MatrixContent::Notice(_) => "m.notice",
        }
    }

    /// Get MXC URL from content (for media types)
    ///
    /// # Arguments
    /// * `content` - The MatrixContent
    ///
    /// # Returns
    /// MXC URL if available
    pub fn get_mxc_url(content: &MatrixContent) -> Option<&str> {
        match content {
            MatrixContent::Image(image) => Some(&image.url),
            MatrixContent::File(file) => Some(&file.url),
            MatrixContent::Audio(audio) => Some(&audio.url),
            MatrixContent::Video(video) => Some(&video.url),
            _ => None,
        }
    }

    /// Create text content
    ///
    /// # Arguments
    /// * `text` - The text content
    ///
    /// # Returns
    /// MatrixContent::Text variant
    pub fn create_text(text: impl Into<String>) -> MatrixContent {
        MatrixContent::Text(MatrixTextContent { body: text.into() })
    }

    /// Create HTML formatted content
    ///
    /// # Arguments
    /// * `body` - Plain text body
    /// * `formatted_body` - HTML formatted body
    ///
    /// # Returns
    /// MatrixContent::Html variant
    pub fn create_html(
        body: impl Into<String>,
        formatted_body: impl Into<String>,
    ) -> MatrixContent {
        MatrixContent::Html {
            content: MatrixTextContent { body: body.into() },
            format: "org.matrix.custom.html".to_string(),
            formatted_body: formatted_body.into(),
        }
    }

    /// Create image content
    ///
    /// # Arguments
    /// * `body` - Alt text
    /// * `url` - MXC URL
    /// * `info` - Optional image info
    ///
    /// # Returns
    /// MatrixContent::Image variant
    pub fn create_image(
        body: impl Into<String>,
        url: impl Into<String>,
        info: Option<MatrixImageInfo>,
    ) -> MatrixContent {
        MatrixContent::Image(MatrixImageContent {
            body: body.into(),
            url: url.into(),
            info,
        })
    }

    /// Create file content
    ///
    /// # Arguments
    /// * `body` - Display filename
    /// * `url` - MXC URL
    /// * `filename` - Original filename
    /// * `info` - Optional file info
    ///
    /// # Returns
    /// MatrixContent::File variant
    pub fn create_file(
        body: impl Into<String>,
        url: impl Into<String>,
        filename: impl Into<String>,
        info: Option<MatrixFileInfo>,
    ) -> MatrixContent {
        MatrixContent::File(MatrixFileContent {
            body: body.into(),
            url: url.into(),
            filename: Some(filename.into()),
            info,
        })
    }

    /// Create audio content
    ///
    /// # Arguments
    /// * `body` - Description
    /// * `url` - MXC URL
    /// * `info` - Optional audio info
    ///
    /// # Returns
    /// MatrixContent::Audio variant
    pub fn create_audio(
        body: impl Into<String>,
        url: impl Into<String>,
        info: Option<MatrixAudioInfo>,
    ) -> MatrixContent {
        MatrixContent::Audio(MatrixAudioContent {
            body: body.into(),
            url: url.into(),
            info,
        })
    }

    /// Create video content
    ///
    /// # Arguments
    /// * `body` - Description
    /// * `url` - MXC URL
    /// * `info` - Optional video info
    ///
    /// # Returns
    /// MatrixContent::Video variant
    pub fn create_video(
        body: impl Into<String>,
        url: impl Into<String>,
        info: Option<MatrixVideoInfo>,
    ) -> MatrixContent {
        MatrixContent::Video(MatrixVideoContent {
            body: body.into(),
            url: url.into(),
            info,
        })
    }

    /// Create location content
    ///
    /// # Arguments
    /// * `body` - Description
    /// * `geo_uri` - Geo URI
    ///
    /// # Returns
    /// MatrixContent::Location variant
    pub fn create_location(body: impl Into<String>, geo_uri: impl Into<String>) -> MatrixContent {
        MatrixContent::Location(MatrixLocationContent {
            body: body.into(),
            geo_uri: geo_uri.into(),
            info: None,
        })
    }

    /// Create emote content
    ///
    /// # Arguments
    /// * `body` - The emote text
    ///
    /// # Returns
    /// MatrixContent::Emote variant
    pub fn create_emote(body: impl Into<String>) -> MatrixContent {
        MatrixContent::Emote(MatrixTextContent { body: body.into() })
    }

    /// Create notice content
    ///
    /// # Arguments
    /// * `body` - The notice text
    ///
    /// # Returns
    /// MatrixContent::Notice variant
    pub fn create_notice(body: impl Into<String>) -> MatrixContent {
        MatrixContent::Notice(MatrixTextContent { body: body.into() })
    }

    /// Serialize content to JSON string
    ///
    /// # Arguments
    /// * `content` - The MatrixContent to serialize
    ///
    /// # Returns
    /// JSON string representation
    pub fn to_json(content: &MatrixContent) -> Result<String> {
        serde_json::to_string(content)
            .map_err(|e| AgentError::platform(format!("Failed to serialize content: {}", e)).into())
    }

    /// Serialize content to JSON value
    ///
    /// # Arguments
    /// * `content` - The MatrixContent to serialize
    ///
    /// # Returns
    /// JSON value representation
    pub fn to_json_value(content: &MatrixContent) -> Result<serde_json::Value> {
        serde_json::to_value(content)
            .map_err(|e| AgentError::platform(format!("Failed to serialize content: {}", e)).into())
    }

    /// Parse MXC URI into server name and media ID
    ///
    /// # Arguments
    /// * `mxc_uri` - MXC URI (e.g., mxc://example.com/media_id)
    ///
    /// # Returns
    /// Tuple of (server_name, media_id)
    pub fn parse_mxc_uri(mxc_uri: &str) -> Result<(String, String)> {
        if !mxc_uri.starts_with("mxc://") {
            return Err(AgentError::platform(format!(
                "Invalid MXC URI: {}",
                mxc_uri
            )));
        }

        let parts: Vec<&str> = mxc_uri[6..].split('/').collect();
        if parts.len() != 2 {
            return Err(AgentError::platform(format!(
                "Invalid MXC URI format: {}",
                mxc_uri
            )));
        }

        Ok((parts[0].to_string(), parts[1].to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_text_content() {
        let json = serde_json::json!({
            "msgtype": "m.text",
            "body": "Hello, World!"
        });
        let content = MatrixContentParser::parse(json).unwrap();
        assert!(matches!(content, MatrixContent::Text(_)));
        assert_eq!(MatrixContentParser::extract_text(&content), "Hello, World!");
    }

    #[test]
    fn test_parse_html_content() {
        let json = serde_json::json!({
            "msgtype": "m.text",
            "body": "Hello, World!",
            "format": "org.matrix.custom.html",
            "formatted_body": "<b>Hello, World!</b>"
        });
        let content = MatrixContentParser::parse(json).unwrap();
        assert!(matches!(content, MatrixContent::Html { .. }));
        assert_eq!(MatrixContentParser::extract_text(&content), "Hello, World!");
    }

    #[test]
    fn test_parse_image_content() {
        let json = serde_json::json!({
            "msgtype": "m.image",
            "body": "image.png",
            "url": "mxc://example.com/media123",
            "info": {
                "w": 1920,
                "h": 1080,
                "mimetype": "image/png",
                "size": 12345
            }
        });
        let content = MatrixContentParser::parse(json).unwrap();
        assert!(matches!(content, MatrixContent::Image(_)));
        assert_eq!(
            MatrixContentParser::extract_text(&content),
            "[Image: image.png]"
        );

        if let MatrixContent::Image(image) = content {
            assert_eq!(image.url, "mxc://example.com/media123");
            assert!(image.info.is_some());
            let info = image.info.unwrap();
            assert_eq!(info.w, Some(1920));
            assert_eq!(info.h, Some(1080));
        }
    }

    #[test]
    fn test_parse_file_content() {
        let json = serde_json::json!({
            "msgtype": "m.file",
            "body": "document.pdf",
            "url": "mxc://example.com/file456",
            "filename": "document.pdf",
            "info": {
                "mimetype": "application/pdf",
                "size": 98765
            }
        });
        let content = MatrixContentParser::parse(json).unwrap();
        assert!(matches!(content, MatrixContent::File(_)));
        assert_eq!(
            MatrixContentParser::extract_text(&content),
            "[File: document.pdf]"
        );
    }

    #[test]
    fn test_parse_audio_content() {
        let json = serde_json::json!({
            "msgtype": "m.audio",
            "body": "voice_message.ogg",
            "url": "mxc://example.com/audio789",
            "info": {
                "mimetype": "audio/ogg",
                "duration": 5000
            }
        });
        let content = MatrixContentParser::parse(json).unwrap();
        assert!(matches!(content, MatrixContent::Audio(_)));
        assert_eq!(
            MatrixContentParser::extract_text(&content),
            "[Audio: voice_message.ogg]"
        );
    }

    #[test]
    fn test_parse_video_content() {
        let json = serde_json::json!({
            "msgtype": "m.video",
            "body": "video.mp4",
            "url": "mxc://example.com/video012",
            "info": {
                "mimetype": "video/mp4",
                "duration": 60000,
                "w": 1920,
                "h": 1080
            }
        });
        let content = MatrixContentParser::parse(json).unwrap();
        assert!(matches!(content, MatrixContent::Video(_)));
        assert_eq!(
            MatrixContentParser::extract_text(&content),
            "[Video: video.mp4]"
        );
    }

    #[test]
    fn test_parse_location_content() {
        let json = serde_json::json!({
            "msgtype": "m.location",
            "body": "Golden Gate Bridge",
            "geo_uri": "geo:37.8199,-122.4783;u=35"
        });
        let content = MatrixContentParser::parse(json).unwrap();
        assert!(matches!(content, MatrixContent::Location(_)));
        let text = MatrixContentParser::extract_text(&content);
        assert!(text.contains("Golden Gate Bridge"));
        assert!(text.contains("geo:37.8199,-122.4783;u=35"));
    }

    #[test]
    fn test_parse_emote_content() {
        let json = serde_json::json!({
            "msgtype": "m.emote",
            "body": "dances happily"
        });
        let content = MatrixContentParser::parse(json).unwrap();
        assert!(matches!(content, MatrixContent::Emote(_)));
        assert_eq!(
            MatrixContentParser::extract_text(&content),
            "* dances happily *"
        );
    }

    #[test]
    fn test_parse_notice_content() {
        let json = serde_json::json!({
            "msgtype": "m.notice",
            "body": "Bot notification"
        });
        let content = MatrixContentParser::parse(json).unwrap();
        assert!(matches!(content, MatrixContent::Notice(_)));
        assert_eq!(
            MatrixContentParser::extract_text(&content),
            "[Notice: Bot notification]"
        );
    }

    #[test]
    fn test_get_msg_type() {
        let text = MatrixContent::Text(MatrixTextContent::default());
        assert_eq!(MatrixContentParser::get_msg_type(&text), "m.text");

        let image = MatrixContent::Image(MatrixImageContent::default());
        assert_eq!(MatrixContentParser::get_msg_type(&image), "m.image");

        let file = MatrixContent::File(MatrixFileContent::default());
        assert_eq!(MatrixContentParser::get_msg_type(&file), "m.file");
    }

    #[test]
    fn test_get_mxc_url() {
        let image = MatrixContent::Image(MatrixImageContent {
            body: "test".to_string(),
            url: "mxc://example.com/media123".to_string(),
            info: None,
        });
        assert_eq!(
            MatrixContentParser::get_mxc_url(&image),
            Some("mxc://example.com/media123")
        );

        let text = MatrixContent::Text(MatrixTextContent::default());
        assert_eq!(MatrixContentParser::get_mxc_url(&text), None);
    }

    #[test]
    fn test_parse_mxc_uri() {
        let result = MatrixContentParser::parse_mxc_uri("mxc://example.com/media123").unwrap();
        assert_eq!(result.0, "example.com");
        assert_eq!(result.1, "media123");

        let result = MatrixContentParser::parse_mxc_uri("invalid://example.com/media123");
        assert!(result.is_err());
    }

    #[test]
    fn test_create_text() {
        let content = MatrixContentParser::create_text("Test message");
        assert!(
            matches!(content, MatrixContent::Text(MatrixTextContent { body }) if body == "Test message")
        );
    }

    #[test]
    fn test_create_html() {
        let content = MatrixContentParser::create_html("Plain text", "<b>HTML</b>");
        assert!(
            matches!(content, MatrixContent::Html { format, .. } if format == "org.matrix.custom.html")
        );
    }

    #[test]
    fn test_create_image() {
        let info = MatrixImageInfo {
            w: Some(100),
            h: Some(100),
            mimetype: Some("image/png".to_string()),
            size: Some(1024),
            thumbnail_url: None,
            thumbnail_info: None,
        };
        let content =
            MatrixContentParser::create_image("image.png", "mxc://example.com/img", Some(info));
        assert!(matches!(content, MatrixContent::Image(_)));
    }

    #[test]
    fn test_create_location() {
        let content = MatrixContentParser::create_location("Home", "geo:40.7128,-74.0060;u=10");
        assert!(matches!(content, MatrixContent::Location(_)));
        if let MatrixContent::Location(loc) = content {
            assert_eq!(loc.body, "Home");
            assert_eq!(loc.geo_uri, "geo:40.7128,-74.0060;u=10");
        }
    }

    #[test]
    fn test_serialize_content() {
        let content = MatrixContentParser::create_text("Test");
        let json = MatrixContentParser::to_json(&content).unwrap();
        assert!(json.contains("Test"));
        assert!(json.contains("m.text"));
    }
}

// =============================================================================
// 🟢 P0 FIX: PlatformContent trait implementation for unified content framework
// =============================================================================

impl PlatformContent for MatrixContent {
    fn content_type(&self) -> UnifiedContentType {
        match self {
            MatrixContent::Text(_) => UnifiedContentType::Text,
            MatrixContent::Html { .. } => UnifiedContentType::Rich,
            MatrixContent::Image(_) => UnifiedContentType::Image,
            MatrixContent::File(_) => UnifiedContentType::File,
            MatrixContent::Audio(_) => UnifiedContentType::Audio,
            MatrixContent::Video(_) => UnifiedContentType::Video,
            MatrixContent::Location(_) => UnifiedContentType::Location,
            MatrixContent::Emote(_) => UnifiedContentType::Text,
            MatrixContent::Notice(_) => UnifiedContentType::System,
        }
    }

    fn extract_text(&self) -> String {
        MatrixContentParser::extract_text(self)
    }
}

impl MatrixImageContent {
    /// Convert to unified MediaContent
    pub fn to_media_content(&self) -> MediaContent {
        MediaContent {
            url: self.url.clone(),
            width: self.info.as_ref().and_then(|i| i.w.map(|w| w as i32)),
            height: self.info.as_ref().and_then(|i| i.h.map(|h| h as i32)),
            size: self.info.as_ref().and_then(|i| i.size.map(|s| s as i64)),
            mime_type: self.info.as_ref().and_then(|i| i.mimetype.clone()),
            filename: Some(self.body.clone()),
            caption: Some(self.body.clone()),
            thumbnail: self.info.as_ref().and_then(|i| i.thumbnail_url.clone()),
            duration: None,
        }
    }
}

impl MatrixVideoContent {
    /// Convert to unified MediaContent
    pub fn to_media_content(&self) -> MediaContent {
        MediaContent {
            url: self.url.clone(),
            width: self.info.as_ref().and_then(|i| i.w.map(|w| w as i32)),
            height: self.info.as_ref().and_then(|i| i.h.map(|h| h as i32)),
            duration: self
                .info
                .as_ref()
                .and_then(|i| i.duration.map(|d| d as i32)),
            mime_type: self.info.as_ref().and_then(|i| i.mimetype.clone()),
            filename: Some(self.body.clone()),
            caption: Some(self.body.clone()),
            size: self.info.as_ref().and_then(|i| i.size.map(|s| s as i64)),
            thumbnail: self.info.as_ref().and_then(|i| i.thumbnail_url.clone()),
        }
    }
}

impl MatrixFileContent {
    /// Convert to unified MediaContent
    pub fn to_media_content(&self) -> MediaContent {
        MediaContent {
            url: self.url.clone(),
            mime_type: self.info.as_ref().and_then(|i| i.mimetype.clone()),
            filename: self.filename.clone().or_else(|| Some(self.body.clone())),
            size: self.info.as_ref().and_then(|i| i.size.map(|s| s as i64)),
            width: None,
            height: None,
            duration: None,
            caption: None,
            thumbnail: self.info.as_ref().and_then(|i| i.thumbnail_url.clone()),
        }
    }
}

impl MatrixAudioContent {
    /// Convert to unified MediaContent
    pub fn to_media_content(&self) -> MediaContent {
        MediaContent {
            url: self.url.clone(),
            mime_type: self.info.as_ref().and_then(|i| i.mimetype.clone()),
            filename: Some(self.body.clone()),
            size: self.info.as_ref().and_then(|i| i.size.map(|s| s as i64)),
            duration: self
                .info
                .as_ref()
                .and_then(|i| i.duration.map(|d| d as i32)),
            width: None,
            height: None,
            caption: None,
            thumbnail: None,
        }
    }
}
