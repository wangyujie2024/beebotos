//! IRC Content Parser
//!
//! Parses and formats content from IRC messages.
//! Supports CTCP (Client-To-Client Protocol), formatting codes,
//! and IRC-specific features.

use serde::{Deserialize, Serialize};

use crate::communication::channel::content::{ContentType as UnifiedContentType, PlatformContent};

/// IRC message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IRCMessageType {
    /// Private message
    Private {
        sender: String,
        target: String,
        content: String,
    },
    /// Channel message
    Channel {
        sender: String,
        channel: String,
        content: String,
    },
    /// Notice
    Notice {
        sender: String,
        target: String,
        content: String,
    },
    /// CTCP message
    Ctcp {
        sender: String,
        target: String,
        command: String,
        params: Vec<String>,
    },
    /// CTCP reply
    CtcpReply {
        sender: String,
        target: String,
        command: String,
        response: String,
    },
    /// Action/me message
    Action {
        sender: String,
        target: String,
        action: String,
    },
}

/// IRC formatting codes
pub struct IRCFormat;

impl IRCFormat {
    /// Bold text
    pub const BOLD: char = '\x02';
    /// Color code
    pub const COLOR: char = '\x03';
    /// Italic text
    pub const ITALIC: char = '\x1D';
    /// Underline text
    pub const UNDERLINE: char = '\x1F';
    /// Reverse/italic
    pub const REVERSE: char = '\x16';
    /// Reset formatting
    pub const RESET: char = '\x0F';

    /// Color codes
    pub const WHITE: u8 = 0;
    pub const BLACK: u8 = 1;
    pub const BLUE: u8 = 2;
    pub const GREEN: u8 = 3;
    pub const RED: u8 = 4;
    pub const BROWN: u8 = 5;
    pub const PURPLE: u8 = 6;
    pub const ORANGE: u8 = 7;
    pub const YELLOW: u8 = 8;
    pub const LIGHT_GREEN: u8 = 9;
    pub const CYAN: u8 = 10;
    pub const LIGHT_CYAN: u8 = 11;
    pub const LIGHT_BLUE: u8 = 12;
    pub const PINK: u8 = 13;
    pub const GREY: u8 = 14;
    pub const LIGHT_GREY: u8 = 15;

    /// Format text as bold
    pub fn bold(text: impl AsRef<str>) -> String {
        format!("{}{}{}", Self::BOLD, text.as_ref(), Self::BOLD)
    }

    /// Format text with color
    pub fn color(text: impl AsRef<str>, fg: u8, bg: Option<u8>) -> String {
        if let Some(bg) = bg {
            format!(
                "{}{:02},{:02}{}{}",
                Self::COLOR,
                fg,
                bg,
                text.as_ref(),
                Self::RESET
            )
        } else {
            format!("{}{:02}{}{}", Self::COLOR, fg, text.as_ref(), Self::RESET)
        }
    }

    /// Format text as italic
    pub fn italic(text: impl AsRef<str>) -> String {
        format!("{}{}{}", Self::ITALIC, text.as_ref(), Self::RESET)
    }

    /// Format text as underlined
    pub fn underline(text: impl AsRef<str>) -> String {
        format!("{}{}{}", Self::UNDERLINE, text.as_ref(), Self::RESET)
    }

    /// Strip all formatting codes
    pub fn strip(text: impl AsRef<str>) -> String {
        let text = text.as_ref();
        let mut result = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                '\x02' | '\x0F' | '\x1D' | '\x1F' | '\x16' => {
                    // Skip formatting control characters
                }
                '\x03' => {
                    // Skip color code and its parameters
                    // Skip foreground (2 digits)
                    for _ in 0..2 {
                        if chars.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    // Skip comma and background if present
                    if chars.peek() == Some(&',') {
                        chars.next();
                        for _ in 0..2 {
                            if chars.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                                chars.next();
                            } else {
                                break;
                            }
                        }
                    }
                }
                _ => result.push(ch),
            }
        }

        result
    }
}

/// CTCP (Client-To-Client Protocol) commands
pub struct CTCP;

impl CTCP {
    /// CTCP delimiter
    pub const DELIM: char = '\x01';

    /// Create CTCP message
    pub fn message(command: impl AsRef<str>, params: &[impl AsRef<str>]) -> String {
        let params_str = if params.is_empty() {
            String::new()
        } else {
            format!(
                " {}",
                params
                    .iter()
                    .map(|p| p.as_ref())
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        };
        format!(
            "{}{}{}{}",
            Self::DELIM,
            command.as_ref(),
            params_str,
            Self::DELIM
        )
    }

    /// Create CTCP reply
    pub fn reply(command: impl AsRef<str>, response: impl AsRef<str>) -> String {
        format!("{}{} {}", Self::DELIM, command.as_ref(), response.as_ref())
    }

    /// Parse CTCP message
    pub fn parse(text: impl AsRef<str>) -> Option<(String, Vec<String>)> {
        let text = text.as_ref();
        if !text.starts_with(Self::DELIM) || !text.ends_with(Self::DELIM) {
            return None;
        }

        let inner = &text[1..text.len() - 1];
        let parts: Vec<&str> = inner.splitn(2, ' ').collect();

        let command = parts.first()?.to_string();
        let params = parts
            .get(1)
            .map(|p| p.split(' ').map(|s| s.to_string()).collect())
            .unwrap_or_default();

        Some((command, params))
    }

    /// Check if text is a CTCP message
    pub fn is_ctcp(text: impl AsRef<str>) -> bool {
        let text = text.as_ref();
        text.starts_with(Self::DELIM) && text.ends_with(Self::DELIM)
    }

    /// ACTION command (me)
    pub fn action(text: impl AsRef<str>) -> String {
        Self::message("ACTION", &[text.as_ref()])
    }

    /// VERSION command
    pub fn version() -> String {
        Self::message("VERSION", &[] as &[&str])
    }

    /// TIME command
    pub fn time() -> String {
        Self::message("TIME", &[] as &[&str])
    }

    /// PING command
    pub fn ping(timestamp: impl AsRef<str>) -> String {
        Self::message("PING", &[timestamp.as_ref()])
    }

    /// SOURCE command
    pub fn source() -> String {
        Self::message("SOURCE", &[] as &[&str])
    }

    /// CLIENTINFO command
    pub fn clientinfo() -> String {
        Self::message("CLIENTINFO", &[] as &[&str])
    }

    /// DCC command
    pub fn dcc(dcc_type: impl AsRef<str>, params: &[impl AsRef<str>]) -> String {
        Self::message(format!("DCC {}", dcc_type.as_ref()), params)
    }
}

/// Parsed content result
#[derive(Debug, Clone, Default)]
pub struct ParsedContent {
    pub text: String,
    pub formatting: Vec<FormatRange>,
    pub mentions: Vec<String>,
    pub urls: Vec<String>,
    pub is_ctcp: bool,
    pub ctcp_command: Option<String>,
    pub ctcp_params: Vec<String>,
    pub is_action: bool,
    pub action_text: Option<String>,
}

/// Format range
#[derive(Debug, Clone)]
pub struct FormatRange {
    pub start: usize,
    pub end: usize,
    pub format_type: FormatType,
}

/// Format types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatType {
    Bold,
    Italic,
    Underline,
    Color { fg: u8, bg: Option<u8> },
}

/// IRC content parser
pub struct IRCContentParser;

impl IRCContentParser {
    /// Create a new parser
    pub fn new() -> Self {
        Self
    }

    /// Parse IRC message content
    pub fn parse(&self, content: impl AsRef<str>) -> ParsedContent {
        let content = content.as_ref();
        let mut parsed = ParsedContent::default();

        // Check for CTCP
        if CTCP::is_ctcp(content) {
            parsed.is_ctcp = true;
            if let Some((cmd, params)) = CTCP::parse(content) {
                parsed.ctcp_command = Some(cmd.clone());
                parsed.ctcp_params = params.clone();

                // Handle ACTION specially
                if cmd == "ACTION" {
                    parsed.is_action = true;
                    parsed.action_text = params.first().cloned();
                    parsed.text = params.first().cloned().unwrap_or_default();
                } else {
                    parsed.text = format!("[CTCP {}]", cmd);
                }
            }
            return parsed;
        }

        // Parse formatting
        parsed.formatting = self.parse_formatting(content);

        // Strip formatting for plain text
        parsed.text = IRCFormat::strip(content);

        // Extract URLs
        parsed.urls = self.extract_urls(&parsed.text);

        // Extract mentions (nicknames starting with @)
        parsed.mentions = self.extract_mentions(&parsed.text);

        parsed
    }

    /// Parse formatting codes
    fn parse_formatting(&self, text: &str) -> Vec<FormatRange> {
        let mut ranges = Vec::new();
        let mut chars = text.chars().enumerate().peekable();

        let mut bold_start: Option<usize> = None;
        let mut italic_start: Option<usize> = None;
        let mut underline_start: Option<usize> = None;
        let mut color_start: Option<(usize, u8, Option<u8>)> = None;

        while let Some((idx, ch)) = chars.next() {
            match ch {
                '\x02' => {
                    // Bold
                    if let Some(start) = bold_start {
                        ranges.push(FormatRange {
                            start,
                            end: idx,
                            format_type: FormatType::Bold,
                        });
                        bold_start = None;
                    } else {
                        bold_start = Some(idx);
                    }
                }
                '\x1D' => {
                    // Italic
                    if let Some(start) = italic_start {
                        ranges.push(FormatRange {
                            start,
                            end: idx,
                            format_type: FormatType::Italic,
                        });
                        italic_start = None;
                    } else {
                        italic_start = Some(idx);
                    }
                }
                '\x1F' => {
                    // Underline
                    if let Some(start) = underline_start {
                        ranges.push(FormatRange {
                            start,
                            end: idx,
                            format_type: FormatType::Underline,
                        });
                        underline_start = None;
                    } else {
                        underline_start = Some(idx);
                    }
                }
                '\x03' => {
                    // Color
                    if let Some((start, fg, bg)) = color_start {
                        ranges.push(FormatRange {
                            start,
                            end: idx,
                            format_type: FormatType::Color { fg, bg },
                        });
                        color_start = None;
                    } else {
                        // Parse color codes
                        let mut fg = String::new();
                        let mut bg = String::new();
                        let mut parsing_bg = false;

                        while let Some(&(_, c)) = chars.peek() {
                            if c == ',' && !parsing_bg && !fg.is_empty() {
                                parsing_bg = true;
                                chars.next();
                                continue;
                            }
                            if c.is_ascii_digit() && fg.len() < 2 && !parsing_bg {
                                fg.push(c);
                                chars.next();
                            } else if c.is_ascii_digit() && bg.len() < 2 && parsing_bg {
                                bg.push(c);
                                chars.next();
                            } else {
                                break;
                            }
                        }

                        let fg_num = fg.parse().unwrap_or(0);
                        let bg_num = if bg.is_empty() {
                            None
                        } else {
                            Some(bg.parse().unwrap_or(0))
                        };
                        color_start = Some((idx, fg_num, bg_num));
                    }
                }
                '\x0F' => {
                    // Reset
                    if let Some(start) = bold_start {
                        ranges.push(FormatRange {
                            start,
                            end: idx,
                            format_type: FormatType::Bold,
                        });
                        bold_start = None;
                    }
                    if let Some(start) = italic_start {
                        ranges.push(FormatRange {
                            start,
                            end: idx,
                            format_type: FormatType::Italic,
                        });
                        italic_start = None;
                    }
                    if let Some(start) = underline_start {
                        ranges.push(FormatRange {
                            start,
                            end: idx,
                            format_type: FormatType::Underline,
                        });
                        underline_start = None;
                    }
                    if let Some((start, fg, bg)) = color_start {
                        ranges.push(FormatRange {
                            start,
                            end: idx,
                            format_type: FormatType::Color { fg, bg },
                        });
                        color_start = None;
                    }
                }
                _ => {}
            }
        }

        // Close any open formats at end of text
        if let Some(start) = bold_start {
            ranges.push(FormatRange {
                start,
                end: text.len(),
                format_type: FormatType::Bold,
            });
        }
        if let Some(start) = italic_start {
            ranges.push(FormatRange {
                start,
                end: text.len(),
                format_type: FormatType::Italic,
            });
        }
        if let Some(start) = underline_start {
            ranges.push(FormatRange {
                start,
                end: text.len(),
                format_type: FormatType::Underline,
            });
        }
        if let Some((start, fg, bg)) = color_start {
            ranges.push(FormatRange {
                start,
                end: text.len(),
                format_type: FormatType::Color { fg, bg },
            });
        }

        ranges
    }

    /// Extract URLs from text
    fn extract_urls(&self, text: &str) -> Vec<String> {
        let url_regex = regex::Regex::new(
            r"https?://[a-zA-Z0-9][-\w]*(?:\.[a-zA-Z0-9][-\w]*)+(?::\d+)?(?:/[^\s]*)?",
        )
        .ok();

        url_regex.map_or_else(Vec::new, |re| {
            re.find_iter(text).map(|m| m.as_str().to_string()).collect()
        })
    }

    /// Extract mentions (nicknames)
    fn extract_mentions(&self, text: &str) -> Vec<String> {
        // Simple mention detection - nicks at start of message or after space
        text.split_whitespace()
            .filter(|w| w.starts_with('@') && w.len() > 1)
            .map(|w| {
                w[1..]
                    .trim_end_matches(|c: char| !c.is_alphanumeric())
                    .to_string()
            })
            .collect()
    }

    /// Build formatted message
    pub fn build_formatted(&self, parts: &[FormatPart]) -> String {
        parts
            .iter()
            .map(|part| match part {
                FormatPart::Text(text) => text.clone(),
                FormatPart::Bold(text) => IRCFormat::bold(text),
                FormatPart::Italic(text) => IRCFormat::italic(text),
                FormatPart::Underline(text) => IRCFormat::underline(text),
                FormatPart::Color { text, fg, bg } => IRCFormat::color(text, *fg, *bg),
            })
            .collect()
    }

    /// Build action message
    pub fn build_action(&self, text: impl AsRef<str>) -> String {
        CTCP::action(text)
    }

    /// Build CTCP request
    pub fn build_ctcp(&self, command: impl AsRef<str>, params: &[impl AsRef<str>]) -> String {
        CTCP::message(command, params)
    }

    /// Strip all formatting
    pub fn strip_formatting(&self, text: impl AsRef<str>) -> String {
        IRCFormat::strip(text)
    }
}

impl Default for IRCContentParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Format part for building messages
#[derive(Debug, Clone)]
pub enum FormatPart {
    Text(String),
    Bold(String),
    Italic(String),
    Underline(String),
    Color {
        text: String,
        fg: u8,
        bg: Option<u8>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_irc_format_bold() {
        let formatted = IRCFormat::bold("bold text");
        assert!(formatted.contains('\x02'));
    }

    #[test]
    fn test_irc_format_color() {
        let formatted = IRCFormat::color("colored", IRCFormat::RED, Some(IRCFormat::WHITE));
        assert!(formatted.contains('\x03'));
    }

    #[test]
    fn test_irc_format_strip() {
        let formatted = format!("{}bold{} normal", IRCFormat::BOLD, IRCFormat::RESET);
        let stripped = IRCFormat::strip(&formatted);
        assert_eq!(stripped, "bold normal");
    }

    #[test]
    fn test_ctcp_action() {
        let action = CTCP::action("does something");
        assert!(action.starts_with('\x01'));
        assert!(action.contains("ACTION"));
        assert!(action.ends_with('\x01'));
    }

    #[test]
    fn test_ctcp_parse() {
        let ctcp = "\x01VERSION\x01";
        let parsed = CTCP::parse(ctcp);
        assert!(parsed.is_some());
        let (cmd, params) = parsed.unwrap();
        assert_eq!(cmd, "VERSION");
        assert!(params.is_empty());
    }

    #[test]
    fn test_ctcp_is_ctcp() {
        assert!(CTCP::is_ctcp("\x01VERSION\x01"));
        assert!(!CTCP::is_ctcp("regular message"));
    }

    #[test]
    fn test_parser_action() {
        let parser = IRCContentParser::new();
        let content = parser.parse("\x01ACTION dances\x01");
        assert!(content.is_action);
        assert_eq!(content.action_text, Some("dances".to_string()));
    }

    #[test]
    fn test_parser_formatting() {
        let parser = IRCContentParser::new();
        let content = parser.parse(format!("{}bold{} text", IRCFormat::BOLD, IRCFormat::RESET));
        assert!(!content.formatting.is_empty());
    }

    #[test]
    fn test_extract_urls() {
        let parser = IRCContentParser::new();
        let urls = parser.extract_urls("Check out https://example.com");
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0], "https://example.com");
    }

    #[test]
    fn test_extract_mentions() {
        let parser = IRCContentParser::new();
        let mentions = parser.extract_mentions("Hello @nick1 and @nick2!");
        assert_eq!(mentions.len(), 2);
    }
}

// =============================================================================
// 🟢 P0 FIX: PlatformContent trait implementation for unified content framework
// =============================================================================

impl PlatformContent for IRCMessageType {
    fn content_type(&self) -> UnifiedContentType {
        match self {
            IRCMessageType::Private { .. } => UnifiedContentType::Text,
            IRCMessageType::Channel { .. } => UnifiedContentType::Text,
            IRCMessageType::Notice { .. } => UnifiedContentType::System,
            IRCMessageType::Ctcp { .. } => UnifiedContentType::System,
            IRCMessageType::CtcpReply { .. } => UnifiedContentType::System,
            IRCMessageType::Action { .. } => UnifiedContentType::Text,
        }
    }

    fn extract_text(&self) -> String {
        match self {
            IRCMessageType::Private { content, .. } => IRCFormat::strip(content),
            IRCMessageType::Channel { content, .. } => IRCFormat::strip(content),
            IRCMessageType::Notice { content, .. } => IRCFormat::strip(content),
            IRCMessageType::Ctcp {
                command, params, ..
            } => {
                format!("[CTCP {}] {}", command, params.join(" "))
            }
            IRCMessageType::CtcpReply {
                command, response, ..
            } => {
                format!("[CTCP Reply {}] {}", command, response)
            }
            IRCMessageType::Action { action, .. } => {
                format!("* {} *", action)
            }
        }
    }
}
