//! iMessage Content Parser
//!
//! Parses iMessage content from BlueBubbles API format into internal Message
//! format. Handles all iMessage-specific features:
//! - Text messages with subject lines
//! - Attachments (images, videos, documents)
//! - Tapbacks (reactions)
//! - Rich links
//! - Location sharing
//! - Digital Touch
//! - Threaded replies

use std::collections::HashMap;
use std::path::PathBuf;

use crate::communication::channel::content::{
    ContentType as UnifiedContentType, MediaContent, PlatformContent,
};

/// iMessage attachment
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IMessageAttachment {
    pub guid: String,
    pub file_name: String,
    pub mime_type: String,
    pub file_size: i64,
    pub local_path: Option<String>,
    pub is_downloaded: bool,
}

/// iMessage message
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IMessageMessage {
    pub guid: String,
    pub chat_guid: String,
    pub text: Option<String>,
    pub attachments: Vec<IMessageAttachment>,
    pub associated_message_guid: Option<String>,
    pub associated_message_type: Option<i32>,
    pub date: i64,
    pub is_from_me: bool,
    pub handle: Option<String>,
    pub thread_originator_guid: Option<String>,
    pub thread_originator_part: Option<String>,
}

/// Tapback (reaction) type
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TapbackType {
    Heart = 0,
    ThumbsUp = 1,
    ThumbsDown = 2,
    HaHa = 3,
    Exclamation = 4,
    Question = 5,
}

impl TapbackType {
    pub fn from_i32(value: i32) -> Option<Self> {
        match value.abs() {
            0 => Some(TapbackType::Heart),
            1 => Some(TapbackType::ThumbsUp),
            2 => Some(TapbackType::ThumbsDown),
            3 => Some(TapbackType::HaHa),
            4 => Some(TapbackType::Exclamation),
            5 => Some(TapbackType::Question),
            _ => None,
        }
    }

    pub fn to_emoji(&self) -> &'static str {
        match self {
            TapbackType::Heart => "❤️",
            TapbackType::ThumbsUp => "👍",
            TapbackType::ThumbsDown => "👎",
            TapbackType::HaHa => "😂",
            TapbackType::Exclamation => "‼️",
            TapbackType::Question => "❓",
        }
    }
}
use crate::communication::{Message, MessageType, PlatformType};
use crate::error::{AgentError, Result};

/// Parsed iMessage content
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum IMessageContent {
    /// Plain text message
    Text {
        text: String,
        subject: Option<String>,
    },
    /// Message with attachments
    Attachment {
        text: Option<String>,
        attachments: Vec<AttachmentInfo>,
    },
    /// Rich link preview
    RichLink {
        url: String,
        title: Option<String>,
        description: Option<String>,
        image_url: Option<String>,
    },
    /// Location sharing
    Location {
        latitude: f64,
        longitude: f64,
        name: Option<String>,
        address: Option<String>,
    },
    /// Tapback (reaction)
    Tapback {
        original_message_guid: String,
        tapback_type: TapbackType,
        is_add: bool,
    },
    /// Digital Touch message
    DigitalTouch { item_type: String, data: Vec<u8> },
    /// Handwritten message
    Handwriting { image_data: Vec<u8> },
    /// Group action (added/removed participants)
    GroupAction {
        action: GroupActionType,
        participant: String,
    },
    /// Unknown/unsupported type
    Unknown { raw_data: serde_json::Value },
}

/// Attachment information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AttachmentInfo {
    pub guid: String,
    pub file_name: String,
    pub mime_type: String,
    pub file_size: i64,
    pub local_path: Option<PathBuf>,
    pub is_downloaded: bool,
}

impl From<&IMessageAttachment> for AttachmentInfo {
    fn from(attachment: &IMessageAttachment) -> Self {
        Self {
            guid: attachment.guid.clone(),
            file_name: attachment.file_name.clone(),
            mime_type: attachment.mime_type.clone(),
            file_size: attachment.file_size,
            local_path: attachment.local_path.as_ref().map(PathBuf::from),
            is_downloaded: attachment.is_downloaded,
        }
    }
}

/// Group action type
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GroupActionType {
    ParticipantAdded,
    ParticipantRemoved,
    GroupRenamed,
    GroupIconChanged,
}

/// iMessage content parser
pub struct IMessageContentParser;

impl IMessageContentParser {
    /// Parse iMessage content from BlueBubbles message
    pub fn parse(message: &IMessageMessage) -> Result<IMessageContent> {
        // Check for tapback first
        if let (Some(assoc_guid), Some(assoc_type)) = (
            &message.associated_message_guid,
            message.associated_message_type,
        ) {
            return Self::parse_tapback(assoc_guid, assoc_type);
        }

        // Check for attachments
        if !message.attachments.is_empty() {
            return Self::parse_with_attachments(message);
        }

        // Parse text content
        if let Some(text) = &message.text {
            // Check for rich links
            if let Some(rich_link) = Self::extract_rich_link(text) {
                return Ok(rich_link);
            }

            // Check for location sharing
            if let Some(location) = Self::extract_location(text) {
                return Ok(location);
            }

            // Regular text with optional subject
            let (subject, body) = Self::extract_subject(text);

            return Ok(IMessageContent::Text {
                text: body,
                subject,
            });
        }

        // Unknown/empty message
        Ok(IMessageContent::Unknown {
            raw_data: serde_json::to_value(message).unwrap_or_default(),
        })
    }

    /// Parse tapback (reaction)
    fn parse_tapback(original_guid: &str, tapback_type: i32) -> Result<IMessageContent> {
        let tapback = TapbackType::from_i32(tapback_type).ok_or_else(|| {
            AgentError::platform(format!("Unknown tapback type: {}", tapback_type))
        })?;

        // Check if it's an add or remove (negative values indicate removal)
        let is_add = tapback_type > 0;

        Ok(IMessageContent::Tapback {
            original_message_guid: original_guid.to_string(),
            tapback_type: tapback,
            is_add,
        })
    }

    /// Parse message with attachments
    fn parse_with_attachments(message: &IMessageMessage) -> Result<IMessageContent> {
        let attachments: Vec<AttachmentInfo> =
            message.attachments.iter().map(|a| a.into()).collect();

        Ok(IMessageContent::Attachment {
            text: message.text.clone(),
            attachments,
        })
    }

    /// Extract subject line from text
    fn extract_subject(text: &str) -> (Option<String>, String) {
        // iMessage uses a special character (U+FFFC or similar) to separate subject
        // from body Or uses newline for simple separation
        if let Some(pos) = text.find('\n') {
            let subject = text[..pos].trim().to_string();
            let body = text[pos + 1..].trim().to_string();

            if !subject.is_empty() && !body.is_empty() {
                return (Some(subject), body);
            }
        }

        (None, text.to_string())
    }

    /// Extract rich link from text
    fn extract_rich_link(text: &str) -> Option<IMessageContent> {
        // Look for URL patterns that might be rich links
        // iMessage rich links are usually just the URL in the text
        // The actual preview data is in the attachment/metadata

        // Simple URL detection
        let url_regex = regex::Regex::new(r"https?://[^\s]+").ok()?;

        if let Some(mat) = url_regex.find(text) {
            let url = mat.as_str().to_string();

            // If text is just the URL, it's likely a rich link
            if text.trim() == url {
                return Some(IMessageContent::RichLink {
                    url,
                    title: None,
                    description: None,
                    image_url: None,
                });
            }
        }

        None
    }

    /// Extract location from text
    fn extract_location(text: &str) -> Option<IMessageContent> {
        // iMessage location sharing uses a special format
        // Latitude and longitude are embedded in the text or metadata

        // Look for coordinates pattern: "latitude, longitude" or similar
        let coord_regex = regex::Regex::new(r"(-?\d+\.?\d*)\s*,\s*(-?\d+\.?\d*)").ok()?;

        if let Some(caps) = coord_regex.captures(text) {
            let lat: f64 = caps.get(1)?.as_str().parse().ok()?;
            let lon: f64 = caps.get(2)?.as_str().parse().ok()?;

            // Validate coordinates
            if lat.abs() <= 90.0 && lon.abs() <= 180.0 {
                return Some(IMessageContent::Location {
                    latitude: lat,
                    longitude: lon,
                    name: None,
                    address: None,
                });
            }
        }

        None
    }

    /// Convert iMessage to internal Message format
    pub fn to_message(
        imessage: &IMessageMessage,
        content: IMessageContent,
        platform_user_id: &str,
    ) -> Result<Message> {
        let message_type = match &content {
            IMessageContent::Text { .. } => MessageType::Text,
            IMessageContent::Attachment { attachments, .. } => {
                // Determine type based on first attachment
                if let Some(first) = attachments.first() {
                    if first.mime_type.starts_with("image/") {
                        MessageType::Image
                    } else if first.mime_type.starts_with("video/") {
                        MessageType::Video
                    } else if first.mime_type.starts_with("audio/") {
                        MessageType::Voice
                    } else {
                        MessageType::File
                    }
                } else {
                    MessageType::Text
                }
            }
            IMessageContent::RichLink { .. } => MessageType::Text,
            IMessageContent::Location { .. } => MessageType::Text,
            IMessageContent::Tapback { .. } => MessageType::Text,
            IMessageContent::DigitalTouch { .. } => MessageType::Image,
            IMessageContent::Handwriting { .. } => MessageType::Image,
            IMessageContent::GroupAction { .. } => MessageType::System,
            IMessageContent::Unknown { .. } => MessageType::Text,
        };

        let text_content = Self::extract_text(&content);

        let mut metadata = HashMap::new();
        metadata.insert("message_guid".to_string(), imessage.guid.clone());
        metadata.insert("chat_guid".to_string(), imessage.chat_guid.clone());
        metadata.insert("is_from_me".to_string(), imessage.is_from_me.to_string());
        metadata.insert("date".to_string(), imessage.date.to_string());

        // Add attachment info to metadata
        if let IMessageContent::Attachment { attachments, .. } = &content {
            if let Some(first) = attachments.first() {
                metadata.insert("attachment_guid".to_string(), first.guid.clone());
                metadata.insert("attachment_name".to_string(), first.file_name.clone());
                metadata.insert("attachment_mime".to_string(), first.mime_type.clone());
                if let Some(path) = &first.local_path {
                    metadata.insert(
                        "attachment_path".to_string(),
                        path.to_string_lossy().to_string(),
                    );
                }
            }
        }

        // Add tapback info
        if let IMessageContent::Tapback {
            original_message_guid,
            tapback_type,
            is_add,
        } = &content
        {
            metadata.insert(
                "original_message_guid".to_string(),
                original_message_guid.clone(),
            );
            metadata.insert(
                "tapback_type".to_string(),
                (*tapback_type as i32).to_string(),
            );
            metadata.insert(
                "tapback_emoji".to_string(),
                tapback_type.to_emoji().to_string(),
            );
            metadata.insert("is_add".to_string(), is_add.to_string());
        }

        // Add location info
        if let IMessageContent::Location {
            latitude,
            longitude,
            name,
            address,
        } = &content
        {
            metadata.insert("latitude".to_string(), latitude.to_string());
            metadata.insert("longitude".to_string(), longitude.to_string());
            if let Some(n) = name {
                metadata.insert("location_name".to_string(), n.clone());
            }
            if let Some(a) = address {
                metadata.insert("location_address".to_string(), a.clone());
            }
        }

        // Add thread reply info
        if let Some(originator) = &imessage.thread_originator_guid {
            metadata.insert("thread_originator_guid".to_string(), originator.clone());
        }
        if let Some(part) = &imessage.thread_originator_part {
            metadata.insert("thread_originator_part".to_string(), part.clone());
        }

        // Add extra fields to metadata since Message struct doesn't have them
        let mut metadata = metadata;
        metadata.insert("platform_user_id".to_string(), platform_user_id.to_string());
        metadata.insert(
            "sender_id".to_string(),
            imessage.handle.clone().unwrap_or_default(),
        );
        if let Some(sender) = &imessage.handle {
            metadata.insert("sender_name".to_string(), sender.clone());
        }
        if let Some(reply_to) = &imessage.thread_originator_guid {
            metadata.insert("reply_to".to_string(), reply_to.clone());
        }

        // Parse guid as UUID or generate new one
        let id = uuid::Uuid::parse_str(&imessage.guid).unwrap_or_else(|_| uuid::Uuid::new_v4());

        // Convert timestamp (i64 Unix timestamp) to DateTime<Utc>
        let timestamp =
            chrono::DateTime::from_timestamp(imessage.date, 0).unwrap_or_else(chrono::Utc::now);

        Ok(Message {
            id,
            thread_id: id, // Use same id as thread_id for now
            platform: PlatformType::IMessage,
            message_type,
            content: text_content,
            metadata,
            timestamp,
        })
    }

    /// Extract text content from parsed content
    pub fn extract_text(content: &IMessageContent) -> String {
        match content {
            IMessageContent::Text { text, subject } => {
                if let Some(subj) = subject {
                    format!("{}\n{}", subj, text)
                } else {
                    text.clone()
                }
            }
            IMessageContent::Attachment { text, attachments } => {
                let mut result = String::new();
                if let Some(t) = text {
                    result.push_str(t);
                    result.push('\n');
                }
                for att in attachments {
                    result.push_str(&format!("[Attachment: {}]", att.file_name));
                }
                result
            }
            IMessageContent::RichLink { url, title, .. } => {
                if let Some(t) = title {
                    format!("{}\n{}", t, url)
                } else {
                    url.clone()
                }
            }
            IMessageContent::Location {
                latitude,
                longitude,
                name,
                ..
            } => {
                if let Some(n) = name {
                    format!("{}: {}, {}", n, latitude, longitude)
                } else {
                    format!("Location: {}, {}", latitude, longitude)
                }
            }
            IMessageContent::Tapback { tapback_type, .. } => {
                format!("[{} Reaction]", tapback_type.to_emoji())
            }
            IMessageContent::DigitalTouch { item_type, .. } => {
                format!("[Digital Touch: {}]", item_type)
            }
            IMessageContent::Handwriting { .. } => "[Handwritten Message]".to_string(),
            IMessageContent::GroupAction {
                action,
                participant,
            } => {
                let action_str = match action {
                    GroupActionType::ParticipantAdded => "added",
                    GroupActionType::ParticipantRemoved => "removed",
                    GroupActionType::GroupRenamed => "renamed group",
                    GroupActionType::GroupIconChanged => "changed group icon",
                };
                format!("[Group: {} {}]", participant, action_str)
            }
            IMessageContent::Unknown { raw_data } => {
                format!("[Unknown message: {:?}]", raw_data)
            }
        }
    }

    /// Determine MIME type from file name
    pub fn mime_type_from_filename(filename: &str) -> &'static str {
        let lower = filename.to_lowercase();

        if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
            "image/jpeg"
        } else if lower.ends_with(".png") {
            "image/png"
        } else if lower.ends_with(".gif") {
            "image/gif"
        } else if lower.ends_with(".heic") {
            "image/heic"
        } else if lower.ends_with(".mov") || lower.ends_with(".qt") {
            "video/quicktime"
        } else if lower.ends_with(".mp4") {
            "video/mp4"
        } else if lower.ends_with(".m4v") {
            "video/x-m4v"
        } else if lower.ends_with(".caf") {
            "audio/x-caf"
        } else if lower.ends_with(".m4a") {
            "audio/m4a"
        } else if lower.ends_with(".mp3") {
            "audio/mpeg"
        } else if lower.ends_with(".pdf") {
            "application/pdf"
        } else if lower.ends_with(".vcf") {
            "text/vcard"
        } else if lower.ends_with(".pkpass") {
            "application/vnd.apple.pkpass"
        } else {
            "application/octet-stream"
        }
    }

    /// Check if MIME type is an image
    pub fn is_image_mime(mime: &str) -> bool {
        mime.starts_with("image/")
    }

    /// Check if MIME type is a video
    pub fn is_video_mime(mime: &str) -> bool {
        mime.starts_with("video/")
    }

    /// Check if MIME type is audio
    pub fn is_audio_mime(mime: &str) -> bool {
        mime.starts_with("audio/")
    }
}

/// iMessage outgoing content formatter
pub struct IMessageContentFormatter;

impl IMessageContentFormatter {
    /// Format text message for sending
    pub fn format_text(text: &str) -> String {
        text.to_string()
    }

    /// Format text with subject
    pub fn format_with_subject(subject: &str, body: &str) -> String {
        format!("{}\n{}", subject, body)
    }

    /// Format reply to message
    pub fn format_reply(_original_text: &str, reply_text: &str) -> String {
        // iMessage uses thread originator GUID for replies
        // The text can just be the reply content
        reply_text.to_string()
    }

    /// Format mention
    pub fn format_mention(_handle: &str, name: &str) -> String {
        // iMessage doesn't have explicit mentions
        // We just include the name
        format!("{} ", name)
    }

    /// Format location sharing
    pub fn format_location(latitude: f64, longitude: f64, name: Option<&str>) -> String {
        if let Some(n) = name {
            format!("{}: {}, {}", n, latitude, longitude)
        } else {
            format!("{}, {}", latitude, longitude)
        }
    }

    /// Truncate text to iMessage limit
    pub fn truncate_text(text: &str, max_length: usize) -> String {
        if text.len() <= max_length {
            text.to_string()
        } else {
            format!("{}...", &text[..max_length - 3])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_subject() {
        let text = "Subject Line\nThis is the body";
        let (subject, body) = IMessageContentParser::extract_subject(text);

        assert_eq!(subject, Some("Subject Line".to_string()));
        assert_eq!(body, "This is the body".to_string());
    }

    #[test]
    fn test_extract_subject_no_subject() {
        let text = "Just a simple message";
        let (subject, body) = IMessageContentParser::extract_subject(text);

        assert_eq!(subject, None);
        assert_eq!(body, "Just a simple message".to_string());
    }

    #[test]
    fn test_mime_type_from_filename() {
        assert_eq!(
            IMessageContentParser::mime_type_from_filename("photo.jpg"),
            "image/jpeg"
        );
        assert_eq!(
            IMessageContentParser::mime_type_from_filename("image.png"),
            "image/png"
        );
        assert_eq!(
            IMessageContentParser::mime_type_from_filename("video.mov"),
            "video/quicktime"
        );
        assert_eq!(
            IMessageContentParser::mime_type_from_filename("doc.pdf"),
            "application/pdf"
        );
        assert_eq!(
            IMessageContentParser::mime_type_from_filename("unknown.xyz"),
            "application/octet-stream"
        );
    }

    #[test]
    fn test_truncate_text() {
        let text = "This is a very long message that needs to be truncated";
        let truncated = IMessageContentFormatter::truncate_text(text, 20);

        assert_eq!(truncated, "This is a very lo...");
    }
}

// =============================================================================
// 🟢 P0 FIX: PlatformContent trait implementation for unified content framework
// =============================================================================

impl PlatformContent for IMessageContent {
    fn content_type(&self) -> UnifiedContentType {
        match self {
            IMessageContent::Text { .. } => UnifiedContentType::Text,
            IMessageContent::Attachment { .. } => UnifiedContentType::File,
            IMessageContent::RichLink { .. } => UnifiedContentType::Rich,
            IMessageContent::Location { .. } => UnifiedContentType::Location,
            IMessageContent::Tapback { .. } => UnifiedContentType::System,
            IMessageContent::DigitalTouch { .. } => UnifiedContentType::Image,
            IMessageContent::Handwriting { .. } => UnifiedContentType::Image,
            IMessageContent::GroupAction { .. } => UnifiedContentType::System,
            IMessageContent::Unknown { .. } => UnifiedContentType::Unknown,
        }
    }

    fn extract_text(&self) -> String {
        IMessageContentParser::extract_text(self)
    }
}

impl AttachmentInfo {
    /// Convert to unified MediaContent
    pub fn to_media_content(&self) -> MediaContent {
        MediaContent {
            url: self
                .local_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            mime_type: Some(self.mime_type.clone()),
            filename: Some(self.file_name.clone()),
            size: Some(self.file_size),
            width: None,
            height: None,
            duration: None,
            caption: None,
            thumbnail: None,
        }
    }
}
