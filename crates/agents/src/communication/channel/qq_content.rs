//! QQ Content Parser
//!
//! Parses and formats content from QQ messages.
//! Supports CQ codes, mentions, and OneBot message formats.

use serde::{Deserialize, Serialize};

use crate::communication::channel::content::{
    ContentType as UnifiedContentType, MediaContent, PlatformContent,
};

/// QQ message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QQMessage {
    /// Private message
    Private {
        user_id: i64,
        message: Vec<QQSegment>,
        raw_message: String,
        sender: QQSender,
    },
    /// Group message
    Group {
        group_id: i64,
        user_id: i64,
        message: Vec<QQSegment>,
        raw_message: String,
        sender: QQSender,
        anonymous: Option<QQAnonymous>,
    },
    /// Guild message (QQ频道)
    Guild {
        guild_id: String,
        channel_id: String,
        user_id: String,
        message: Vec<QQSegment>,
        sender: QQGuildSender,
    },
}

/// QQ message segment
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QQSegment {
    /// Text message
    Text { data: QQTextData },
    /// Face/emoji
    Face { data: QQFaceData },
    /// Image
    Image { data: QQImageData },
    /// Record/voice
    Record { data: QQRecordData },
    /// Video
    Video { data: QQVideoData },
    /// At mention
    At { data: QQAtData },
    /// RPS (Rock Paper Scissors)
    Rps { data: serde_json::Value },
    /// Dice
    Dice { data: serde_json::Value },
    /// Shake/nudge
    Shake { data: serde_json::Value },
    /// Poke
    Poke { data: QQPokeData },
    /// Anonymous message
    Anonymous { data: serde_json::Value },
    /// Share/link
    Share { data: QQShareData },
    /// Contact card
    Contact { data: QQContactData },
    /// Location
    Location { data: QQLocationData },
    /// Music
    Music { data: QQMusicData },
    /// Reply to message
    Reply { data: QQReplyData },
    /// Forward message
    Forward { data: QQForwardData },
    /// Node/forward node
    Node { data: QQNodeData },
    /// XML message
    Xml { data: QQXmlData },
    /// JSON message
    Json { data: QQJsonData },
}

/// Text data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QQTextData {
    pub text: String,
}

/// Face data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QQFaceData {
    pub id: String,
}

/// Image data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QQImageData {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>, // flash, show
}

/// Record data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QQRecordData {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub magic: Option<bool>,
}

/// Video data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QQVideoData {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// At data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QQAtData {
    pub qq: String, // "all" for @everyone
}

/// Poke data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QQPokeData {
    #[serde(rename = "type")]
    pub poke_type: String,
    pub id: String,
    pub name: Option<String>,
}

/// Share data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QQShareData {
    pub url: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

/// Contact data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QQContactData {
    #[serde(rename = "type")]
    pub contact_type: String, // qq, group
    pub id: String,
}

/// Location data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QQLocationData {
    pub lat: String,
    pub lon: String,
    pub title: Option<String>,
    pub content: Option<String>,
}

/// Music data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QQMusicData {
    #[serde(rename = "type")]
    pub music_type: String, // qq, 163, xm, custom
    pub id: Option<String>,
    pub url: Option<String>,
    pub audio: Option<String>,
    pub title: Option<String>,
    pub content: Option<String>,
    pub image: Option<String>,
}

/// Reply data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QQReplyData {
    pub id: String,
}

/// Forward data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QQForwardData {
    pub id: String,
}

/// Node data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QQNodeData {
    pub id: Option<String>,
    pub name: Option<String>,
    pub uin: Option<String>,
    pub content: Option<serde_json::Value>,
}

/// XML data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QQXmlData {
    pub data: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resid: Option<i32>,
}

/// JSON data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QQJsonData {
    pub data: String,
}

/// QQ sender info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QQSender {
    pub user_id: i64,
    pub nickname: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>, // owner, admin, member
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
}

/// QQ guild sender
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QQGuildSender {
    pub tiny_id: String,
    pub nickname: String,
}

/// QQ anonymous user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QQAnonymous {
    pub id: i64,
    pub name: String,
    pub flag: String,
}

/// Parsed content result
#[derive(Debug, Clone, Default)]
pub struct ParsedContent {
    pub text: String,
    pub mentions: Vec<QQMention>,
    pub images: Vec<QQImageInfo>,
    pub replies_to: Option<String>,
    pub commands: Vec<QQCommand>,
    pub raw_segments: Vec<QQSegment>,
}

/// QQ mention
#[derive(Debug, Clone)]
pub struct QQMention {
    pub qq_id: String,
    pub is_all: bool,
}

/// QQ image info
#[derive(Debug, Clone)]
pub struct QQImageInfo {
    pub file: String,
    pub url: Option<String>,
    pub is_flash: bool,
}

/// QQ command
#[derive(Debug, Clone)]
pub struct QQCommand {
    pub name: String,
    pub args: Vec<String>,
}

/// QQ content parser
pub struct QQContentParser;

impl QQContentParser {
    /// Create a new parser
    pub fn new() -> Self {
        Self
    }

    /// Parse QQ message segments
    pub fn parse_message(&self, segments: &[QQSegment]) -> ParsedContent {
        let mut text_parts = Vec::new();
        let mut mentions = Vec::new();
        let mut images = Vec::new();
        let mut replies_to = None;
        let mut commands = Vec::new();

        for segment in segments {
            match segment {
                QQSegment::Text { data } => {
                    let text = &data.text;

                    // Parse commands (format: /command arg1 arg2)
                    if text.starts_with('/') {
                        let parts: Vec<&str> = text.split_whitespace().collect();
                        if !parts.is_empty() {
                            let cmd_name = parts[0].trim_start_matches('/').to_string();
                            let args = parts[1..].iter().map(|s| s.to_string()).collect();
                            commands.push(QQCommand {
                                name: cmd_name,
                                args,
                            });
                        }
                    }

                    text_parts.push(text.clone());
                }
                QQSegment::At { data } => {
                    let is_all = data.qq == "all";
                    mentions.push(QQMention {
                        qq_id: data.qq.clone(),
                        is_all,
                    });
                    if is_all {
                        text_parts.push("@全体成员".to_string());
                    } else {
                        text_parts.push(format!("@{}", data.qq));
                    }
                }
                QQSegment::Image { data } => {
                    images.push(QQImageInfo {
                        file: data.file.clone(),
                        url: data.url.clone(),
                        is_flash: data.r#type.as_deref() == Some("flash"),
                    });
                }
                QQSegment::Face { data } => {
                    text_parts.push(format!("[表情:{}]", data.id));
                }
                QQSegment::Reply { data } => {
                    replies_to = Some(data.id.clone());
                }
                QQSegment::Poke { data } => {
                    text_parts.push(format!("[戳一戳:{}]", data.name.as_deref().unwrap_or("戳")));
                }
                QQSegment::Share { data } => {
                    text_parts.push(format!("[分享:{}] {}", data.title, data.url));
                }
                QQSegment::Location { data } => {
                    text_parts.push(format!(
                        "[位置:{}] {}, {}",
                        data.title.as_deref().unwrap_or("位置"),
                        data.lat,
                        data.lon
                    ));
                }
                QQSegment::Music { data } => {
                    text_parts.push(format!(
                        "[音乐:{}] {}",
                        data.music_type,
                        data.title.as_deref().unwrap_or("未知歌曲")
                    ));
                }
                _ => {
                    // Other segment types
                }
            }
        }

        ParsedContent {
            text: text_parts.join(" "),
            mentions,
            images,
            replies_to,
            commands,
            raw_segments: segments.to_vec(),
        }
    }

    /// Parse raw CQ code message
    pub fn parse_cq_code(&self, raw_message: &str) -> ParsedContent {
        // CQ code format: [CQ:type,key=value,key2=value2]
        let mut text_parts = Vec::new();
        let mut mentions = Vec::new();
        let mut images = Vec::new();
        let mut replies_to = None;

        let mut remaining = raw_message;

        while let Some(start) = remaining.find("[CQ:") {
            // Add text before CQ code
            if start > 0 {
                text_parts.push(remaining[..start].to_string());
            }

            // Find end of CQ code
            if let Some(end) = remaining[start..].find(']') {
                let cq_code = &remaining[start..start + end + 1];
                remaining = &remaining[start + end + 1..];

                // Parse CQ code
                if let Some(content) = self.parse_cq_segment(cq_code) {
                    match content {
                        CQContent::Text(text) => text_parts.push(text),
                        CQContent::Mention(qq) => {
                            let is_all = qq == "all";
                            mentions.push(QQMention { qq_id: qq, is_all });
                        }
                        CQContent::Image { file, url } => {
                            images.push(QQImageInfo {
                                file,
                                url,
                                is_flash: false,
                            });
                        }
                        CQContent::Reply(id) => {
                            replies_to = Some(id);
                        }
                    }
                }
            } else {
                break;
            }
        }

        // Add remaining text
        if !remaining.is_empty() {
            text_parts.push(remaining.to_string());
        }

        ParsedContent {
            text: text_parts.join(""),
            mentions,
            images,
            replies_to,
            commands: vec![],
            raw_segments: vec![],
        }
    }

    /// Parse individual CQ segment
    fn parse_cq_segment(&self, cq_code: &str) -> Option<CQContent> {
        // Remove [CQ: and ]
        let inner = cq_code.trim_start_matches("[CQ:").trim_end_matches(']');

        // Split type and params
        let parts: Vec<&str> = inner.splitn(2, ',').collect();
        let cq_type = parts.first()?;
        let params = parts.get(1).unwrap_or(&"");

        match *cq_type {
            "at" => {
                let qq = self.extract_cq_param(params, "qq");
                qq.map(CQContent::Mention)
            }
            "image" => {
                let file = self.extract_cq_param(params, "file").unwrap_or_default();
                let url = self.extract_cq_param(params, "url");
                Some(CQContent::Image { file, url })
            }
            "face" => {
                let id = self.extract_cq_param(params, "id").unwrap_or_default();
                Some(CQContent::Text(format!("[表情:{}]", id)))
            }
            "reply" => {
                let id = self.extract_cq_param(params, "id")?;
                Some(CQContent::Reply(id))
            }
            "text" => {
                let text = self.extract_cq_param(params, "text").unwrap_or_default();
                // Unescape special characters
                let text = text
                    .replace("&#91;", "[")
                    .replace("&#93;", "]")
                    .replace("&#44;", ",");
                Some(CQContent::Text(text))
            }
            _ => None,
        }
    }

    /// Extract parameter from CQ code
    fn extract_cq_param(&self, params: &str, key: &str) -> Option<String> {
        for param in params.split(',') {
            let kv: Vec<&str> = param.splitn(2, '=').collect();
            if kv.len() == 2 && kv[0] == key {
                return Some(kv[1].to_string());
            }
        }
        None
    }

    /// Build text segment
    pub fn build_text(&self, text: impl Into<String>) -> QQSegment {
        QQSegment::Text {
            data: QQTextData { text: text.into() },
        }
    }

    /// Build at segment
    pub fn build_at(&self, qq_id: impl Into<String>) -> QQSegment {
        QQSegment::At {
            data: QQAtData { qq: qq_id.into() },
        }
    }

    /// Build @all segment
    pub fn build_at_all(&self) -> QQSegment {
        QQSegment::At {
            data: QQAtData {
                qq: "all".to_string(),
            },
        }
    }

    /// Build image segment
    pub fn build_image(&self, file: impl Into<String>, url: Option<String>) -> QQSegment {
        QQSegment::Image {
            data: QQImageData {
                file: file.into(),
                url,
                r#type: None,
            },
        }
    }

    /// Build reply segment
    pub fn build_reply(&self, message_id: impl Into<String>) -> QQSegment {
        QQSegment::Reply {
            data: QQReplyData {
                id: message_id.into(),
            },
        }
    }

    /// Build face segment
    pub fn build_face(&self, face_id: impl Into<String>) -> QQSegment {
        QQSegment::Face {
            data: QQFaceData { id: face_id.into() },
        }
    }

    /// Build share segment
    pub fn build_share(
        &self,
        url: impl Into<String>,
        title: impl Into<String>,
        content: Option<String>,
    ) -> QQSegment {
        QQSegment::Share {
            data: QQShareData {
                url: url.into(),
                title: title.into(),
                content,
                image: None,
            },
        }
    }

    /// Convert to CQ code format
    pub fn to_cq_code(&self, segments: &[QQSegment]) -> String {
        segments
            .iter()
            .map(|seg| match seg {
                QQSegment::Text { data } => {
                    // Escape special characters
                    let text = data
                        .text
                        .replace("[", "&#91;")
                        .replace("]", "&#93;")
                        .replace(",", "&#44;");
                    format!("[CQ:text,text={}]", text)
                }
                QQSegment::At { data } => format!("[CQ:at,qq={}]", data.qq),
                QQSegment::Image { data } => {
                    if let Some(url) = &data.url {
                        format!("[CQ:image,file={},url={}]", data.file, url)
                    } else {
                        format!("[CQ:image,file={}]", data.file)
                    }
                }
                QQSegment::Face { data } => format!("[CQ:face,id={}]", data.id),
                QQSegment::Reply { data } => format!("[CQ:reply,id={}]", data.id),
                QQSegment::Share { data } => {
                    format!("[CQ:share,url={},title={}]", data.url, data.title)
                }
                _ => String::new(),
            })
            .collect()
    }
}

impl Default for QQContentParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal CQ content representation
enum CQContent {
    Text(String),
    Mention(String),
    Image { file: String, url: Option<String> },
    Reply(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_text_segment() {
        let parser = QQContentParser::new();
        let segments = vec![QQSegment::Text {
            data: QQTextData {
                text: "Hello QQ".to_string(),
            },
        }];

        let content = parser.parse_message(&segments);
        assert_eq!(content.text, "Hello QQ");
    }

    #[test]
    fn test_parse_at_segment() {
        let parser = QQContentParser::new();
        let segments = vec![
            QQSegment::Text {
                data: QQTextData {
                    text: "Hello ".to_string(),
                },
            },
            QQSegment::At {
                data: QQAtData {
                    qq: "123456".to_string(),
                },
            },
        ];

        let content = parser.parse_message(&segments);
        assert_eq!(content.mentions.len(), 1);
        assert_eq!(content.mentions[0].qq_id, "123456");
    }

    #[test]
    fn test_parse_cq_code_at() {
        let parser = QQContentParser::new();
        let raw = "Hello [CQ:at,qq=123456]!";

        let content = parser.parse_cq_code(raw);
        assert_eq!(content.mentions.len(), 1);
        assert_eq!(content.mentions[0].qq_id, "123456");
    }

    #[test]
    fn test_build_text() {
        let parser = QQContentParser::new();
        let seg = parser.build_text("Test");

        match seg {
            QQSegment::Text { data } => assert_eq!(data.text, "Test"),
            _ => panic!("Expected text segment"),
        }
    }

    #[test]
    fn test_to_cq_code() {
        let parser = QQContentParser::new();
        let segments = vec![parser.build_text("Hello "), parser.build_at("123456")];

        let cq = parser.to_cq_code(&segments);
        assert!(cq.contains("[CQ:text,text=Hello ]"));
        assert!(cq.contains("[CQ:at,qq=123456]"));
    }
}

// =============================================================================
// 🟢 P0 FIX: PlatformContent trait implementation for unified content framework
// =============================================================================

impl PlatformContent for QQMessage {
    fn content_type(&self) -> UnifiedContentType {
        match self {
            QQMessage::Private { message, .. } => Self::detect_content_type(message),
            QQMessage::Group { message, .. } => Self::detect_content_type(message),
            QQMessage::Guild { message, .. } => Self::detect_content_type(message),
        }
    }

    fn extract_text(&self) -> String {
        let segments = match self {
            QQMessage::Private { message, .. } => message,
            QQMessage::Group { message, .. } => message,
            QQMessage::Guild { message, .. } => message,
        };
        let parser = QQContentParser::new();
        let parsed = parser.parse_message(segments);
        parsed.text
    }
}

impl QQMessage {
    /// Detect content type from message segments
    fn detect_content_type(segments: &[QQSegment]) -> UnifiedContentType {
        for segment in segments {
            match segment {
                QQSegment::Image { .. } => return UnifiedContentType::Image,
                QQSegment::Video { .. } => return UnifiedContentType::Video,
                QQSegment::Record { .. } => return UnifiedContentType::Audio,
                QQSegment::Location { .. } => return UnifiedContentType::Location,
                QQSegment::At { .. } => return UnifiedContentType::Text,
                QQSegment::Share { .. } => return UnifiedContentType::Rich,
                QQSegment::Contact { .. } => return UnifiedContentType::Contact,
                QQSegment::Music { .. } => return UnifiedContentType::Rich,
                QQSegment::Face { .. } => return UnifiedContentType::Sticker,
                _ => continue,
            }
        }
        UnifiedContentType::Text
    }
}

impl QQImageData {
    /// Convert to unified MediaContent
    pub fn to_media_content(&self) -> MediaContent {
        MediaContent {
            url: self.url.clone().unwrap_or_default(),
            mime_type: None,
            filename: Some(self.file.clone()),
            size: None,
            width: None,
            height: None,
            duration: None,
            caption: None,
            thumbnail: None,
        }
    }
}

impl QQVideoData {
    /// Convert to unified MediaContent
    pub fn to_media_content(&self) -> MediaContent {
        MediaContent {
            url: self.url.clone().unwrap_or_default(),
            mime_type: None,
            filename: Some(self.file.clone()),
            size: None,
            width: None,
            height: None,
            duration: None,
            caption: None,
            thumbnail: None,
        }
    }
}

impl QQRecordData {
    /// Convert to unified MediaContent
    pub fn to_media_content(&self) -> MediaContent {
        MediaContent {
            url: self.url.clone().unwrap_or_default(),
            mime_type: None,
            filename: Some(self.file.clone()),
            size: None,
            width: None,
            height: None,
            duration: None,
            caption: None,
            thumbnail: None,
        }
    }
}
