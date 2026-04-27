//! User-Channel binding models and multi-instance channel identifiers

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::communication::channel::ConnectionMode;
use crate::communication::PlatformType;

/// Status of a user channel binding
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelBindingStatus {
    Active,
    Paused,
    Error,
}

/// A binding between a user and a specific platform instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserChannelBinding {
    pub id: String,
    pub user_id: String,
    pub platform: PlatformType,
    pub instance_name: String,
    pub platform_user_id: Option<String>,
    pub status: ChannelBindingStatus,
    pub webhook_path: Option<String>,
}

/// Unified platform credentials.  Stored encrypted in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserChannelConfig {
    pub platform: PlatformType,
    pub connection_mode: ConnectionMode,
    pub credentials: PlatformCredentials,
}

/// Per-platform credential variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "platform", rename_all = "snake_case")]
pub enum PlatformCredentials {
    Lark {
        app_id: String,
        app_secret: String,
        encrypt_key: Option<String>,
    },
    WeChat {
        app_id: String,
        app_secret: String,
        token: String,
    },
    DingTalk {
        app_key: String,
        app_secret: String,
        robot_code: Option<String>,
    },
    Slack {
        bot_token: String,
        app_token: Option<String>,
        signing_secret: Option<String>,
    },
    Telegram {
        bot_token: String,
    },
    Discord {
        bot_token: String,
        application_id: String,
    },
    WhatsApp {
        api_key: String,
        phone_number_id: String,
        business_account_id: Option<String>,
    },
    Teams {
        app_id: String,
        app_password: String,
    },
    WebChat,
    Generic {
        fields: HashMap<String, String>,
    },
}

/// Multi-instance channel identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChannelInstanceId {
    pub user_id: String,
    pub platform: PlatformType,
    pub instance_name: String,
}

impl ChannelInstanceId {
    pub fn new(
        user_id: impl Into<String>,
        platform: PlatformType,
        instance_name: impl Into<String>,
    ) -> Self {
        Self {
            user_id: user_id.into(),
            platform,
            instance_name: instance_name.into(),
        }
    }
}

/// Reference to a channel instance for lookups.
#[derive(Debug, Clone)]
pub struct ChannelInstanceRef {
    pub id: ChannelInstanceId,
    pub user_channel_id: String,
}
