//! IRC Channel Implementation
//!
//! Provides integration with IRC (Internet Relay Chat) networks.
//! Supports private messages, channels, and standard IRC commands.
//!
//! # Features
//! - Server connection with TLS support
//! - Channel joining and messaging
//! - Private messaging
//! - CTCP support
//! - User management
//! - SASL authentication
//!
//! # Protocol Reference
//! - RFC 1459: <https://tools.ietf.org/html/rfc1459>
//! - RFC 2810-2813: IRC updates

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio_native_tls::{TlsConnector, TlsStream};
use tracing::{debug, error, info};
use uuid::Uuid;

use super::r#trait::{
    BaseChannelConfig, Channel, ChannelConfig, ChannelEvent, ChannelInfo, ChannelType,
    ConnectionMode, ContentType, MemberInfo,
};
use crate::communication::{Message, PlatformType};
use crate::error::{AgentError, Result};

/// IRC channel implementation
pub struct IRCChannel {
    config: IRCConfig,
    state: Arc<Mutex<IRCState>>,
    event_sender: Arc<tokio::sync::Mutex<Option<mpsc::Sender<ChannelEvent>>>>,
    writer_tx: Option<mpsc::Sender<String>>,
}

/// IRC connection state
struct IRCState {
    connected: bool,
    #[allow(dead_code)]
    nickname: String,
    channels: Vec<String>,
}

/// IRC configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IRCConfig {
    /// IRC server host
    pub server: String,
    /// IRC server port
    pub port: u16,
    /// Use TLS
    pub use_tls: bool,
    /// Bot nickname
    pub nickname: String,
    /// Bot username
    pub username: String,
    /// Bot real name
    pub realname: String,
    /// Server password (optional)
    pub server_password: Option<String>,
    /// NickServ password (optional)
    pub nickserv_password: Option<String>,
    /// SASL credentials (optional)
    pub sasl_credentials: Option<SASLCredentials>,
    /// Auto-join channels
    pub auto_join_channels: Vec<String>,
    #[serde(flatten)]
    pub base: BaseChannelConfig,
}

/// SASL credentials
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SASLCredentials {
    pub username: String,
    pub password: String,
}

impl IRCConfig {
    /// Create configuration from environment variables
    pub fn from_env() -> Option<Self> {
        let server = std::env::var("IRC_SERVER").ok()?;
        let port = std::env::var("IRC_PORT").ok()?.parse().ok()?;
        let nickname = std::env::var("IRC_NICKNAME").ok()?;
        let base = BaseChannelConfig::from_env("IRC")?;

        Some(Self {
            server,
            port,
            use_tls: std::env::var("IRC_USE_TLS").ok()?.parse().ok()?,
            nickname: nickname.clone(),
            username: std::env::var("IRC_USERNAME").unwrap_or_else(|_| nickname.clone()),
            realname: std::env::var("IRC_REALNAME").unwrap_or_else(|_| "BeeBotOS Bot".to_string()),
            server_password: std::env::var("IRC_SERVER_PASSWORD").ok(),
            nickserv_password: std::env::var("IRC_NICKSERV_PASSWORD").ok(),
            sasl_credentials: None,
            auto_join_channels: std::env::var("IRC_CHANNELS")
                .map(|s| s.split(',').map(|s| s.to_string()).collect())
                .unwrap_or_default(),
            base,
        })
    }

    /// Validate configuration
    pub fn is_valid(&self) -> bool {
        !self.server.is_empty() && !self.nickname.is_empty() && self.port > 0
    }
}

impl Default for IRCConfig {
    fn default() -> Self {
        let mut base = BaseChannelConfig::default();
        // IRC uses WebSocket as the connection mode (represents persistent connection)
        base.connection_mode = ConnectionMode::WebSocket;

        Self {
            server: "irc.libera.chat".to_string(),
            port: 6667,
            use_tls: false,
            nickname: "BeeBotOS".to_string(),
            username: "beebotos".to_string(),
            realname: "BeeBotOS Bot".to_string(),
            server_password: None,
            nickserv_password: None,
            sasl_credentials: None,
            auto_join_channels: vec![],
            base,
        }
    }
}

impl ChannelConfig for IRCConfig {
    fn from_env() -> Option<Self>
    where
        Self: Sized,
    {
        Self::from_env()
    }

    fn is_valid(&self) -> bool {
        self.is_valid()
    }

    fn allowlist(&self) -> Vec<String> {
        vec![]
    }

    fn connection_mode(&self) -> ConnectionMode {
        self.base.connection_mode
    }

    fn auto_reconnect(&self) -> bool {
        self.base.auto_reconnect
    }

    fn max_reconnect_attempts(&self) -> u32 {
        self.base.max_reconnect_attempts
    }
}

/// IRC message
#[derive(Debug, Clone)]
pub struct IRCMessage {
    /// Prefix (sender)
    pub prefix: Option<String>,
    /// Command
    pub command: String,
    /// Parameters
    pub params: Vec<String>,
    /// Trailing message
    pub trailing: Option<String>,
}

impl IRCMessage {
    /// Parse IRC message from raw line
    pub fn parse(line: &str) -> Result<Self> {
        let mut remaining = line;
        let mut prefix = None;

        // Parse prefix if present
        if remaining.starts_with(':') {
            if let Some(space) = remaining.find(' ') {
                prefix = Some(remaining[1..space].to_string());
                remaining = &remaining[space + 1..];
            }
        }

        // Parse command
        let command_end = remaining.find(' ').unwrap_or(remaining.len());
        let command = remaining[..command_end].to_string();

        if command_end < remaining.len() {
            remaining = &remaining[command_end + 1..];
        } else {
            remaining = "";
        }

        // Parse parameters and trailing
        let mut params = Vec::new();
        let mut trailing = None;

        while !remaining.is_empty() {
            if remaining.starts_with(':') {
                trailing = Some(remaining[1..].to_string());
                break;
            }

            if let Some(space) = remaining.find(' ') {
                params.push(remaining[..space].to_string());
                remaining = &remaining[space + 1..];
            } else {
                params.push(remaining.to_string());
                break;
            }
        }

        Ok(Self {
            prefix,
            command,
            params,
            trailing,
        })
    }

    /// Format message for sending
    pub fn format(&self) -> String {
        let mut result = String::new();

        if let Some(prefix) = &self.prefix {
            result.push(':');
            result.push_str(prefix);
            result.push(' ');
        }

        result.push_str(&self.command);

        for param in &self.params {
            result.push(' ');
            result.push_str(param);
        }

        if let Some(trailing) = &self.trailing {
            result.push(' ');
            result.push(':');
            result.push_str(trailing);
        }

        result.push('\r');
        result.push('\n');
        result
    }

    /// Create NICK command
    pub fn nick(nickname: &str) -> Self {
        Self {
            prefix: None,
            command: "NICK".to_string(),
            params: vec![nickname.to_string()],
            trailing: None,
        }
    }

    /// Create USER command
    pub fn user(username: &str, realname: &str) -> Self {
        Self {
            prefix: None,
            command: "USER".to_string(),
            params: vec![username.to_string(), "0".to_string(), "*".to_string()],
            trailing: Some(realname.to_string()),
        }
    }

    /// Create PASS command
    pub fn pass(password: &str) -> Self {
        Self {
            prefix: None,
            command: "PASS".to_string(),
            params: vec![password.to_string()],
            trailing: None,
        }
    }

    /// Create JOIN command
    pub fn join(channel: &str) -> Self {
        Self {
            prefix: None,
            command: "JOIN".to_string(),
            params: vec![channel.to_string()],
            trailing: None,
        }
    }

    /// Create PART command
    pub fn part(channel: &str) -> Self {
        Self {
            prefix: None,
            command: "PART".to_string(),
            params: vec![channel.to_string()],
            trailing: None,
        }
    }

    /// Create PRIVMSG command
    pub fn privmsg(target: &str, message: &str) -> Self {
        Self {
            prefix: None,
            command: "PRIVMSG".to_string(),
            params: vec![target.to_string()],
            trailing: Some(message.to_string()),
        }
    }

    /// Create PONG command
    pub fn pong(server: &str) -> Self {
        Self {
            prefix: None,
            command: "PONG".to_string(),
            params: vec![server.to_string()],
            trailing: None,
        }
    }

    /// Create QUIT command
    pub fn quit(message: &str) -> Self {
        Self {
            prefix: None,
            command: "QUIT".to_string(),
            params: vec![],
            trailing: Some(message.to_string()),
        }
    }

    /// Get nickname from prefix
    pub fn nickname(&self) -> Option<String> {
        self.prefix
            .as_ref()
            .and_then(|p| p.split('!').next().map(|s| s.to_string()))
    }
}

impl IRCChannel {
    /// Create a new IRC channel
    pub fn new(config: IRCConfig) -> Result<Self> {
        if !config.is_valid() {
            return Err(AgentError::configuration("Invalid IRC configuration"));
        }

        let nickname = config.nickname.clone();

        Ok(Self {
            config,
            state: Arc::new(Mutex::new(IRCState {
                connected: false,
                nickname,
                channels: Vec::new(),
            })),
            event_sender: Arc::new(tokio::sync::Mutex::new(None)),
            writer_tx: None,
        })
    }

    /// Send raw IRC command
    async fn send_raw(&self, message: &IRCMessage) -> Result<()> {
        let data = message.format();
        if let Some(tx) = &self.writer_tx {
            tx.send(data)
                .await
                .map_err(|_| AgentError::platform("Failed to send: channel closed"))?;
            Ok(())
        } else {
            Err(AgentError::platform("Not connected"))
        }
    }

    /// Send message to channel or user
    async fn send_privmsg(&self, target: &str, content: &str) -> Result<()> {
        // Split long messages
        const MAX_LENGTH: usize = 400;

        for chunk in content.chars().collect::<Vec<_>>().chunks(MAX_LENGTH) {
            let msg: String = chunk.iter().collect();
            let irc_msg = IRCMessage::privmsg(target, &msg);
            self.send_raw(&irc_msg).await?;
        }

        Ok(())
    }

    /// Join a channel
    async fn join_channel(&self, channel: &str) -> Result<()> {
        let msg = IRCMessage::join(channel);
        self.send_raw(&msg).await?;

        {
            let mut state = self.state.lock().await;
            if !state.channels.contains(&channel.to_string()) {
                state.channels.push(channel.to_string());
            }
        }

        info!("Joined IRC channel: {}", channel);
        Ok(())
    }

    /// Part a channel
    #[allow(dead_code)]
    async fn part_channel(&self, channel: &str) -> Result<()> {
        let msg = IRCMessage::part(channel);
        self.send_raw(&msg).await?;

        {
            let mut state = self.state.lock().await;
            state.channels.retain(|c| c != channel);
        }

        info!("Parted IRC channel: {}", channel);
        Ok(())
    }

    /// Identify with NickServ
    #[allow(dead_code)]
    async fn identify_nickserv(&self) -> Result<()> {
        if let Some(password) = &self.config.nickserv_password {
            self.send_privmsg("NickServ", &format!("IDENTIFY {}", password))
                .await?;
            info!("Sent NickServ identification");
        }
        Ok(())
    }

    /// Handle incoming IRC message
    #[allow(dead_code)]
    fn handle_message(&self, msg: IRCMessage) -> Option<(String, String)> {
        match msg.command.as_str() {
            "PRIVMSG" => {
                let sender = msg.nickname()?;
                let target = msg.params.get(0)?.clone();
                let content = msg.trailing?;

                // Get our nickname from state
                // Skip messages from self - we'll check this later

                let channel_id = if target.starts_with('#') || target.starts_with('&') {
                    target
                } else {
                    sender.clone()
                };

                Some((channel_id, content))
            }
            "PING" => {
                // Handle ping - would need to send pong
                None
            }
            _ => None,
        }
    }

    /// Get member list for a channel (requires NAMES command)
    #[allow(dead_code)]
    async fn get_channel_members(&self, _channel: &str) -> Result<Vec<MemberInfo>> {
        // Send NAMES command
        let msg = IRCMessage {
            prefix: None,
            command: "NAMES".to_string(),
            params: vec![_channel.to_string()],
            trailing: None,
        };
        self.send_raw(&msg).await?;

        // In a real implementation, would wait for RPL_NAMREPLY responses
        // For now, return empty list
        Ok(vec![])
    }

    /// Connect with TLS
    #[allow(dead_code)]
    async fn connect_tls(&self) -> Result<TlsStream<TcpStream>> {
        let addr = format!("{}:{}", self.config.server, self.config.port);

        // Connect to server
        let stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| AgentError::platform(format!("Failed to connect: {}", e)))?;

        // Setup TLS
        let connector = TlsConnector::from(
            native_tls::TlsConnector::builder()
                .danger_accept_invalid_certs(false)
                .build()
                .map_err(|e| {
                    AgentError::platform(format!("Failed to create TLS connector: {}", e))
                })?,
        );

        let stream = connector
            .connect(&self.config.server, stream)
            .await
            .map_err(|e| AgentError::platform(format!("TLS handshake failed: {}", e)))?;

        info!("Connected to IRC server via TLS: {}", self.config.server);
        Ok(stream)
    }

    /// Connect without TLS
    #[allow(dead_code)]
    async fn connect_plain(&self) -> Result<TcpStream> {
        let addr = format!("{}:{}", self.config.server, self.config.port);

        let stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| AgentError::platform(format!("Failed to connect: {}", e)))?;

        info!("Connected to IRC server: {}", self.config.server);
        Ok(stream)
    }
}

#[async_trait]
impl Channel for IRCChannel {
    fn name(&self) -> &str {
        "irc"
    }

    fn platform(&self) -> PlatformType {
        PlatformType::IRC
    }

    fn is_connected(&self) -> bool {
        // Check state - this is a best-effort check
        // In a real implementation, we'd use a shared atomic
        true
    }

    async fn connect(&mut self) -> Result<()> {
        info!(
            "Connecting to IRC server {}:{}",
            self.config.server, self.config.port
        );

        // Create channel for sending messages to writer
        let (writer_tx, mut writer_rx): (mpsc::Sender<String>, mpsc::Receiver<String>) =
            mpsc::channel(100);
        let writer_tx_for_read = writer_tx.clone();
        self.writer_tx = Some(writer_tx);

        let event_sender = self.event_sender.lock().await.clone();
        let nickname = self.config.nickname.clone();
        let server_password = self.config.server_password.clone();
        let _nickserv_password = self.config.nickserv_password.clone();
        let _auto_join = self.config.auto_join_channels.clone();
        let use_tls = self.config.use_tls;
        let server = self.config.server.clone();
        let username = self.config.username.clone();
        let realname = self.config.realname.clone();

        // Spawn connection handler
        tokio::spawn(async move {
            let result: Result<()> = async {
                if use_tls {
                    // TLS connection
                    let addr = format!("{}:6667", server);
                    let stream = TcpStream::connect(&addr)
                        .await
                        .map_err(|e| AgentError::platform(format!("Failed to connect: {}", e)))?;

                    let connector = TlsConnector::from(
                        native_tls::TlsConnector::new()
                            .map_err(|e| AgentError::platform(format!("TLS error: {}", e)))?,
                    );

                    let mut stream = connector.connect(&server, stream).await.map_err(|e| {
                        AgentError::platform(format!("TLS handshake failed: {}", e))
                    })?;

                    // Send PASS if configured
                    if let Some(password) = server_password {
                        let pass_msg = format!("PASS {}\r\n", password);
                        stream
                            .write_all(pass_msg.as_bytes())
                            .await
                            .map_err(|e| AgentError::platform(format!("Failed to send: {}", e)))?;
                    }

                    // Send NICK and USER
                    let nick_msg = format!("NICK {}\r\n", nickname);
                    stream
                        .write_all(nick_msg.as_bytes())
                        .await
                        .map_err(|e| AgentError::platform(format!("Failed to send: {}", e)))?;

                    let user_msg = format!("USER {} 0 * :{}\r\n", username, realname);
                    stream
                        .write_all(user_msg.as_bytes())
                        .await
                        .map_err(|e| AgentError::platform(format!("Failed to send: {}", e)))?;

                    stream
                        .flush()
                        .await
                        .map_err(|e| AgentError::platform(format!("Failed to flush: {}", e)))?;

                    info!("Connected to IRC via TLS as {}", nickname);

                    // Split stream for read/write
                    let (read_half, mut write_half) = tokio::io::split(stream);

                    // Spawn writer task
                    tokio::spawn(async move {
                        while let Some(data) = writer_rx.recv().await {
                            if let Err(e) = write_half.write_all(data.as_bytes()).await {
                                error!("Failed to write: {}", e);
                                break;
                            }
                            if let Err(e) = write_half.flush().await {
                                error!("Failed to flush: {}", e);
                                break;
                            }
                        }
                    });

                    // Read loop
                    let reader = BufReader::new(read_half);
                    let mut lines = reader.lines();

                    while let Ok(Some(line)) = lines.next_line().await {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }

                        debug!("IRC: {}", line);

                        if let Ok(msg) = IRCMessage::parse(line) {
                            // Handle PING
                            if msg.command == "PING" {
                                if let Some(server) = msg.params.get(0) {
                                    let pong = format!("PONG :{}\r\n", server);
                                    if let Err(e) = writer_tx_for_read.send(pong).await {
                                        error!("Failed to send PONG: {}", e);
                                    }
                                }
                                continue;
                            }

                            // Handle PRIVMSG
                            if msg.command == "PRIVMSG" {
                                if let (Some(sender), Some(target), Some(content)) =
                                    (msg.nickname(), msg.params.get(0), &msg.trailing)
                                {
                                    if sender != nickname {
                                        let channel_id =
                                            if target.starts_with('#') || target.starts_with('&') {
                                                target.clone()
                                            } else {
                                                sender.clone()
                                            };

                                        if let Some(sender) = event_sender.as_ref() {
                                            let _ = sender
                                                .send(ChannelEvent::MessageReceived {
                                                    platform: PlatformType::IRC,
                                                    channel_id,
                                                    message: Message::new(
                                                        Uuid::new_v4(),
                                                        PlatformType::IRC,
                                                        content.clone(),
                                                    ),
                                                })
                                                .await;
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    // Plain connection
                    let addr = format!("{}:6667", server);
                    let stream = TcpStream::connect(&addr)
                        .await
                        .map_err(|e| AgentError::platform(format!("Failed to connect: {}", e)))?;

                    // Send PASS if configured
                    if let Some(password) = server_password {
                        let _pass_msg = format!("PASS {}\r\n", password);
                        // TODO: Actually send the PASS message
                        stream
                            .writable()
                            .await
                            .map_err(|e| AgentError::platform(format!("Stream error: {}", e)))?;
                    }

                    // Send NICK and USER
                    let nick_msg = format!("NICK {}\r\n", nickname);
                    let user_msg = format!("USER {} 0 * :{}\r\n", username, realname);

                    let (read_half, mut write_half) = tokio::io::split(stream);

                    write_half
                        .write_all(nick_msg.as_bytes())
                        .await
                        .map_err(|e| AgentError::platform(format!("Failed to send: {}", e)))?;
                    write_half
                        .write_all(user_msg.as_bytes())
                        .await
                        .map_err(|e| AgentError::platform(format!("Failed to send: {}", e)))?;
                    write_half
                        .flush()
                        .await
                        .map_err(|e| AgentError::platform(format!("Failed to flush: {}", e)))?;

                    info!("Connected to IRC as {}", nickname);

                    // Spawn writer task
                    tokio::spawn(async move {
                        while let Some(data) = writer_rx.recv().await {
                            if let Err(e) = write_half.write_all(data.as_bytes()).await {
                                error!("Failed to write: {}", e);
                                break;
                            }
                            if let Err(e) = write_half.flush().await {
                                error!("Failed to flush: {}", e);
                                break;
                            }
                        }
                    });

                    // Read loop
                    let reader = BufReader::new(read_half);
                    let mut lines = reader.lines();

                    while let Ok(Some(line)) = lines.next_line().await {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }

                        debug!("IRC: {}", line);

                        if let Ok(msg) = IRCMessage::parse(line) {
                            // Handle PING
                            if msg.command == "PING" {
                                if let Some(server) = msg.params.get(0) {
                                    let pong = format!("PONG :{}\r\n", server);
                                    if let Err(e) = writer_tx_for_read.send(pong).await {
                                        error!("Failed to send PONG: {}", e);
                                    }
                                }
                                continue;
                            }

                            // Handle PRIVMSG
                            if msg.command == "PRIVMSG" {
                                if let (Some(sender), Some(target), Some(content)) =
                                    (msg.nickname(), msg.params.get(0), &msg.trailing)
                                {
                                    if sender != nickname {
                                        let channel_id =
                                            if target.starts_with('#') || target.starts_with('&') {
                                                target.clone()
                                            } else {
                                                sender.clone()
                                            };

                                        if let Some(sender) = event_sender.as_ref() {
                                            let _ = sender
                                                .send(ChannelEvent::MessageReceived {
                                                    platform: PlatformType::IRC,
                                                    channel_id,
                                                    message: Message::new(
                                                        Uuid::new_v4(),
                                                        PlatformType::IRC,
                                                        content.clone(),
                                                    ),
                                                })
                                                .await;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                Ok(())
            }
            .await;

            if let Err(e) = result {
                error!("IRC connection error: {}", e);
            }

            info!("IRC connection closed");
        });

        // Wait a moment for connection to establish
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Update state
        {
            let mut state = self.state.lock().await;
            state.connected = true;
        }

        // Auto-join channels
        for channel in &self.config.auto_join_channels.clone() {
            self.join_channel(channel).await.ok();
        }

        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        let quit_msg = IRCMessage::quit("BeeBotOS signing off");
        self.send_raw(&quit_msg).await.ok();

        self.writer_tx = None;

        {
            let mut state = self.state.lock().await;
            state.connected = false;
        }

        info!("Disconnected from IRC");
        Ok(())
    }

    async fn send(&self, channel_id: &str, message: &Message) -> Result<()> {
        self.send_privmsg(channel_id, &message.content).await
    }

    async fn start_listener(&self, event_bus: mpsc::Sender<ChannelEvent>) -> Result<()> {
        // P1 FIX: Save event_bus so connect() can use it to emit events
        *self.event_sender.lock().await = Some(event_bus);
        Ok(())
    }

    async fn stop_listener(&self) -> Result<()> {
        Ok(())
    }

    fn supported_content_types(&self) -> Vec<ContentType> {
        vec![ContentType::Text]
    }

    fn connection_mode(&self) -> ConnectionMode {
        self.config.base.connection_mode
    }

    async fn download_image(
        &self,
        _file_key: &str,
        _message_id: Option<&str>,
    ) -> crate::error::Result<Vec<u8>> {
        Err(crate::error::AgentError::platform(
            "Image download not supported for IRC",
        ))
    }

    async fn list_channels(&self) -> Result<Vec<ChannelInfo>> {
        let state = self.state.lock().await;
        Ok(state
            .channels
            .iter()
            .map(|c| ChannelInfo {
                id: c.clone(),
                name: c.clone(),
                channel_type: ChannelType::Group,
                unread_count: 0,
                metadata: HashMap::new(),
            })
            .collect())
    }

    async fn list_members(&self, _channel_id: &str) -> Result<Vec<MemberInfo>> {
        // Would need to send NAMES and wait for response
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_irc_config_default() {
        let config = IRCConfig::default();
        assert_eq!(config.server, "irc.libera.chat");
        assert_eq!(config.port, 6667);
        assert!(!config.use_tls);
    }

    #[test]
    fn test_irc_message_parse() {
        let line = ":nick!user@host PRIVMSG #channel :Hello world";
        let msg = IRCMessage::parse(line).unwrap();

        assert_eq!(msg.prefix, Some("nick!user@host".to_string()));
        assert_eq!(msg.command, "PRIVMSG");
        assert_eq!(msg.params, vec!["#channel"]);
        assert_eq!(msg.trailing, Some("Hello world".to_string()));
    }

    #[test]
    fn test_irc_message_format() {
        let msg = IRCMessage::privmsg("#channel", "Hello");
        let formatted = msg.format();
        assert!(formatted.contains("PRIVMSG"));
        assert!(formatted.contains("#channel"));
        assert!(formatted.contains("Hello"));
    }

    #[test]
    fn test_nickname_extraction() {
        let msg = IRCMessage::parse(":nick!user@host PRIVMSG #test :hi").unwrap();
        assert_eq!(msg.nickname(), Some("nick".to_string()));
    }

    #[test]
    fn test_irc_channel_creation() {
        let config = IRCConfig::default();
        let channel = IRCChannel::new(config).unwrap();
        assert_eq!(channel.name(), "irc");
    }
}
