//! Platform/Channel Adapters
//!
//! Adapters for various communication platforms and channels.

// CODE QUALITY FIX: Unified error types
pub mod error;

// 统一 Channel Trait 和新实现
pub mod lark_unified;
pub mod lark_ws_impl;
pub mod message_pipeline;
pub mod r#trait;

// 🟢 P1 FIX: Generic WebSocket client for all channels
pub mod websocket_client;

// 会话管理
pub mod session_manager;

// 新的统一 Channel 实现
pub mod dingtalk_channel;
pub mod discord_channel;
pub mod google_chat_channel;
pub mod imessage_channel;
pub mod irc_channel;
pub mod line_channel;
pub mod matrix_channel;
pub mod qq_channel;
pub mod signal_channel;
pub mod slack_channel;
pub mod teams_channel;
pub mod telegram_channel;
pub mod twitter_channel;
pub mod wechat_channel;
pub mod wechat_factory;
pub mod whatsapp_channel;

// 消息模板系统
pub mod message_template;
pub mod message_templates;

// 频道管理器
pub mod channel_manager;

// 🟢 P0 FIX: Unified content framework
pub mod content;

// Mattermost 支持
pub mod mattermost_channel;
pub mod mattermost_content;

// 🟢 P0 FIX: Personal WeChat support via OpenClaw/iLink protocol
pub mod ilink_client;
pub mod personal_wechat_channel;
pub mod personal_wechat_factory;
pub mod webchat_channel;
pub mod webchat_factory;

// 平台内容解析器（被 media/attachment.rs 等模块引用）
pub mod dingtalk_content;
pub mod discord_content;
pub mod google_chat_content;
pub mod imessage_content;
pub mod irc_content;
pub mod lark_content;
pub mod line_content;
pub mod matrix_content;
pub mod poll_feature;
pub mod qq_content;
pub mod signal_cli;
pub mod signal_content;
pub mod slack_content;
pub mod teams_content;
pub mod telegram_content;
pub mod twitter_content;
pub mod wechat_content;
pub mod whatsapp_content;

// 🟢 P0 FIX: Channel extension traits (pinning, editing, moderation)
pub mod channel_extensions;

// 🟢 P0 FIX: Export unified content types
// 🟢 P0 FIX: Export channel extension traits
pub use channel_extensions::{
    BannedUserInfo, ChannelCapabilities, ChannelCapability, ChannelHistoryStats,
    DeletedMessageInfo, EditableChannel, MessageEditHistory, MessageHistoryTracker,
    ModeratedChannel, PinnableChannel, PinnedMessage,
};
// 导出统一 Channel Trait（从 trait.rs）
// 导出频道管理器
pub use channel_manager::{
    ChannelHealthMonitor, ChannelManager, ChannelManagerConfig, ChannelRegistration, ChannelRouter,
    ChannelStatus,
};
pub use content::{
    create_metadata, extract_text_with_format, parse_content_type, ButtonAction, CardButton,
    CardContent, ContactContent, ContentBuilder, ContentParser, ContentType, EntityType,
    LocationContent, MediaContent, PlatformContent, RichContent, StickerContent, TextContent,
    TextEntity, TextFormat,
};
// 导出 Channel Factories
pub use dingtalk_channel::DingTalkChannelFactory;
// 导出新的统一 Channel 实现
pub use dingtalk_channel::{DingTalkChannel, DingTalkChannelConfig};
pub use discord_channel::{DiscordChannel, DiscordChannelConfig, DiscordChannelFactory};
// 导出内容解析器
pub use discord_content::DiscordContentParser;
// CODE QUALITY FIX: Export unified error types
pub use error::{ChannelError as UnifiedChannelError, ChannelResult as UnifiedChannelResult};
pub use google_chat_channel::{GoogleChatChannel, GoogleChatConfig as GoogleChatChannelConfig};
// 导出 Google Chat 内容解析器
pub use google_chat_content::GoogleChatContentParser;
// 🟢 P0 FIX: Export Personal WeChat channel and iLink client
pub use ilink_client::{BotSession, ILinkClient, ILinkConfig, WeChatMessage};
pub use imessage_channel::{IMessageChannel, IMessageChannelConfig};
pub use imessage_content::{IMessageContent, IMessageContentFormatter, IMessageContentParser};
pub use irc_channel::{IRCChannel, IRCConfig, SASLCredentials};
// 导出 IRC 内容解析器
pub use irc_content::{IRCContentParser, IRCFormat, IRCMessageType, CTCP};
pub use lark_unified::LarkChannelFactory;
// 导出飞书新实现
pub use lark_unified::{LarkChannel, LarkConfig};
pub use lark_ws_impl::LarkWebSocketClient;
pub use line_channel::{LineChannel, LineConfig as LineChannelConfig};
// 导出 LINE 内容解析器
pub use line_content::LineContentParser;
pub use matrix_channel::{MatrixChannel, MatrixChannelConfig, MatrixCredential};
// 🟢 P0 FIX: Export Mattermost channel
pub use mattermost_channel::{
    MattermostChannelClient as MattermostChannel, MattermostChannelConfig,
    MattermostChannelFactory, MattermostChannelInfo, MattermostPost, MattermostUser, PostEdit,
};
pub use message_pipeline::{
    ChannelResponse, MessageContext, MessagePipeline, MessageProcessor, PipelineConfig,
    ProcessedMessage, ResponseType,
};
// 🔧 P1 FIX: Export new message template system
pub use message_template::{
    MessageTemplate as NewMessageTemplate, TemplateBuilder, TemplateManager,
};
pub use message_templates::{built_in as template_built_in, MessageTemplate, TemplateEngine};
pub use personal_wechat_channel::{PersonalWeChatChannel, PersonalWeChatConfig};
// 🟢 P0 FIX: Export Personal WeChat factory
pub use personal_wechat_factory::PersonalWeChatFactory;
// 导出投票功能
pub use poll_feature::{
    Poll, PollConfig, PollFormatter, PollManager, PollOption, PollResult, PollStatus, Vote,
    VoteIntent,
};
pub use qq_channel::{QQChannel, QQConfig};
// 导出 QQ 内容解析器
pub use qq_content::{QQContentParser, QQMessage, QQSegment};
pub use r#trait::{
    BaseChannelConfig, Channel, ChannelConfig, ChannelEvent, ChannelFactory, ChannelInfo,
    ChannelType, ConnectionMode, MemberInfo, MemberRole,
};
pub use signal_channel::{SignalChannel, SignalChannelConfig};
pub use signal_cli::{SignalCliConfig, SignalCliManager, SignalCliState, SignalEvent};
pub use signal_content::{SignalContent, SignalContentParser};
pub use slack_channel::{SlackChannel, SlackChannelConfig, SlackChannelFactory};
pub use slack_content::SlackContentParser;
pub use teams_channel::{TeamsChannel, TeamsChannelConfig};
pub use teams_content::TeamsContentParser;
pub use telegram_channel::{
    TelegramChannel, TelegramChannelFactory, TelegramConfig as TelegramChannelConfig,
};
pub use twitter_channel::{TwitterChannel, TwitterChannelConfig};
pub use twitter_content::TwitterContentParser;
// 🟢 P0 FIX: Export WebChat channel and factory
pub use webchat_channel::{WebChatChannel, WebChatConfig};
pub use webchat_factory::WebChatFactory;
// 🟢 P1 FIX: Export WebSocket client types
pub use websocket_client::{
    utils::{build_ws_url, http_to_ws_url, parse_close_code},
    WebSocketClient, WebSocketConfig, WebSocketHandler, WsConnectionState,
};
pub use wechat_channel::{WeChatChannel, WeChatChannelConfig};
pub use wechat_content::{WeChatContent, WeChatContentParser};
pub use wechat_factory::WeChatFactory;
pub use whatsapp_channel::{WhatsAppChannel, WhatsAppChannelConfig};
pub use whatsapp_content::{WhatsAppContent, WhatsAppContentParser};

// Re-export old adapter types for backward compatibility
pub use super::PlatformAdapter;
// Re-export for backward compatibility
pub use super::{Message, PlatformType};
pub use crate::error::{AgentError as ChannelError, Result as ChannelResult};

/// Platform configuration for backward compatibility
#[derive(Debug, Clone, Default)]
pub struct PlatformConfig {
    pub bot_token: Option<String>,
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    pub webhook_url: Option<String>,
    pub webhook_secret: Option<String>,
    pub app_id: Option<String>,
    pub app_secret: Option<String>,
    pub encryption_key: Option<String>,
    pub other: std::collections::HashMap<String, String>,
}
