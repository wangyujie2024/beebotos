//! Channel management commands
//!
//! Manage message platform connections: Telegram, Discord, WeChat, Lark, etc.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

use crate::progress::TaskProgress;

#[derive(Parser)]
pub struct ChannelArgs {
    #[command(subcommand)]
    pub command: ChannelCommand,
}

#[derive(Subcommand)]
pub enum ChannelCommand {
    /// List all configured channels
    List {
        /// Filter by channel type
        #[arg(short, long)]
        r#type: Option<String>,
        /// Show verbose information
        #[arg(short, long)]
        verbose: bool,
    },

    /// Check channel status and health
    Status {
        /// Channel ID (optional, checks all if not provided)
        id: Option<String>,
        /// Perform deep probe
        #[arg(long)]
        probe: bool,
        /// Watch mode - refresh every N seconds
        #[arg(short, long)]
        watch: Option<u64>,
    },

    /// Query channel capabilities
    Capabilities {
        /// Channel ID
        id: String,
    },

    /// Resolve human-readable name to internal ID
    Resolve {
        /// Channel ID
        channel: String,
        /// Name to resolve
        name: String,
    },

    /// View channel logs
    Logs {
        /// Channel ID
        id: String,
        /// Follow log output
        #[arg(long)]
        follow: bool,
        /// Number of lines to show
        #[arg(short, long, default_value = "50")]
        lines: usize,
        /// Filter by log level
        #[arg(long)]
        level: Option<String>,
    },

    /// Add a new channel (interactive wizard)
    Add {
        /// Channel type
        #[arg(value_enum)]
        channel_type: ChannelType,
        /// Non-interactive mode (use --file for config)
        #[arg(long)]
        non_interactive: bool,
        /// Configuration file path
        #[arg(short = 'c', long)]
        file: Option<PathBuf>,
    },

    /// Remove a channel
    Remove {
        /// Channel ID
        id: String,
        /// Delete associated data
        #[arg(long)]
        delete_data: bool,
        /// Force removal without confirmation
        #[arg(long)]
        force: bool,
    },

    /// Login/authenticate with a channel
    Login {
        /// Channel ID
        id: String,
        /// Authentication method
        #[arg(short, long, value_enum)]
        method: Option<AuthMethod>,
    },

    /// Logout from a channel
    Logout {
        /// Channel ID
        id: String,
        /// Revoke access tokens
        #[arg(long)]
        revoke: bool,
    },

    /// Send a message through a channel
    Send {
        /// Channel ID
        #[arg(short, long)]
        channel: String,
        /// Target (user, group, or channel)
        #[arg(short = 't', long)]
        target: String,
        /// Message content
        message: String,
        /// Use a message template
        #[arg(short = 'T', long)]
        template: Option<String>,
        /// Attach files
        #[arg(short, long)]
        attachment: Vec<PathBuf>,
    },

    /// Webhook management
    #[command(subcommand)]
    Webhook(WebhookCommand),

    /// Test channel connection
    Test {
        /// Channel ID
        id: String,
        /// Test message
        #[arg(short, long, default_value = "Test message from BeeBotOS")]
        message: String,
    },
}

#[derive(Subcommand)]
pub enum WebhookCommand {
    /// Generate webhook URL for a channel
    Generate {
        /// Channel ID
        channel: String,
        /// Custom path
        #[arg(short, long)]
        path: Option<String>,
        /// Secret for webhook verification
        #[arg(short, long)]
        secret: Option<String>,
    },
    /// List webhooks for a channel
    List {
        /// Channel ID
        channel: String,
    },
    /// Delete a webhook
    Delete {
        /// Webhook ID
        id: String,
    },
    /// Test a webhook
    Test {
        /// Webhook ID
        id: String,
        /// Test payload (JSON)
        #[arg(short, long)]
        payload: Option<String>,
    },
}

#[derive(ValueEnum, Clone, Debug)]
pub enum ChannelType {
    /// Telegram Bot
    Telegram,
    /// Discord Bot
    Discord,
    /// WeChat Work (Enterprise)
    WechatWork,
    /// Personal WeChat
    WechatPersonal,
    /// Lark/Feishu
    Lark,
    /// DingTalk
    Dingtalk,
    /// Slack
    Slack,
    /// WhatsApp
    Whatsapp,
    /// LINE
    Line,
    /// Matrix
    Matrix,
    /// iMessage via BlueBubbles
    Imessage,
    /// Signal
    Signal,
    /// IRC
    Irc,
    /// Google Chat
    Googlechat,
    /// Microsoft Teams
    Teams,
    /// Twitter/X
    Twitter,
    /// QQ
    Qq,
    /// Email (SMTP/IMAP)
    Email,
    /// Custom WebSocket
    Websocket,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum AuthMethod {
    /// OAuth flow
    Oauth,
    /// API Token
    Token,
    /// QR Code scan
    Qrcode,
    /// Phone number + Code
    Phone,
    /// Webhook verification
    Webhook,
}

pub async fn execute(args: ChannelArgs) -> Result<()> {
    let client = crate::client::ApiClient::new()?;

    match args.command {
        ChannelCommand::List { r#type, verbose } => {
            let progress = TaskProgress::new("Fetching channels");
            let channels = client.list_channels(r#type.as_deref()).await?;
            progress.finish_success(Some(&format!("{} channels found", channels.len())));

            if verbose {
                println!(
                    "{:<20} {:<15} {:<15} {:<20} {:<10}",
                    "ID", "Type", "Name", "Status", "Health"
                );
                println!("{}", "-".repeat(90));
                for ch in channels {
                    println!(
                        "{:<20} {:<15} {:<15} {:<20} {:<10}",
                        ch.id, ch.r#type, ch.name, ch.status, ch.health
                    );
                }
            } else {
                println!(
                    "{:<20} {:<15} {:<15} {:<10}",
                    "ID", "Type", "Name", "Status"
                );
                println!("{}", "-".repeat(70));
                for ch in channels {
                    println!(
                        "{:<20} {:<15} {:<15} {:<10}",
                        ch.id, ch.r#type, ch.name, ch.status
                    );
                }
            }
        }

        ChannelCommand::Status { id, probe, watch } => {
            if let Some(watch_interval) = watch {
                println!("Watching channel status (Ctrl+C to exit)...");
                loop {
                    print!("\x1B[2J\x1B[1;1H"); // Clear screen
                    if let Some(ref channel_id) = id {
                        let status = client.get_channel_status(channel_id, probe).await?;
                        print_channel_status(&status);
                    } else {
                        let statuses = client.get_all_channel_status(probe).await?;
                        for status in statuses {
                            print_channel_status(&status);
                            println!();
                        }
                    }
                    tokio::time::sleep(tokio::time::Duration::from_secs(watch_interval)).await;
                }
            } else {
                let progress = TaskProgress::new("Checking channel status");
                if let Some(channel_id) = id {
                    let status = client.get_channel_status(&channel_id, probe).await?;
                    progress.finish_success(None);
                    print_channel_status(&status);
                } else {
                    let statuses = client.get_all_channel_status(probe).await?;
                    progress.finish_success(Some(&format!("{} channels checked", statuses.len())));
                    for status in statuses {
                        print_channel_status(&status);
                        println!();
                    }
                }
            }
        }

        ChannelCommand::Capabilities { id } => {
            let progress = TaskProgress::new("Querying capabilities");
            let caps = client.get_channel_capabilities(&id).await?;
            progress.finish_success(None);

            println!("Channel: {} ({})", caps.name, id);
            println!("\nSupported Features:");
            for feature in caps.features {
                println!("  ✓ {}", feature);
            }
            println!("\nContent Types:");
            for ct in caps.content_types {
                println!("  • {}", ct);
            }
            println!("\nRate Limits:");
            println!(
                "  Messages per minute: {}",
                caps.rate_limit.messages_per_minute
            );
            println!(
                "  Max message size: {} bytes",
                caps.rate_limit.max_message_size
            );
        }

        ChannelCommand::Resolve { channel, name } => {
            let progress = TaskProgress::new("Resolving name");
            let resolved = client.resolve_channel_name(&channel, &name).await?;
            progress.finish_success(None);
            println!("{} -> {}", name, resolved.id);
            if let Some(channel_type) = resolved.channel_type {
                println!("Type: {}", channel_type);
            }
        }

        ChannelCommand::Logs {
            id,
            follow,
            lines,
            level,
        } => {
            if follow {
                println!("Following logs for channel '{}' (Ctrl+C to exit)...", id);
                client.follow_channel_logs(&id, level.as_deref()).await?;
            } else {
                let progress = TaskProgress::new("Fetching logs");
                let logs = client
                    .get_channel_logs(&id, lines, level.as_deref())
                    .await?;
                progress.finish_success(Some(&format!("{} lines", logs.len())));
                for log in logs {
                    println!("[{}] {}: {}", log.timestamp, log.level, log.message);
                }
            }
        }

        ChannelCommand::Add {
            channel_type,
            non_interactive,
            file,
        } => {
            let progress = TaskProgress::new("Adding channel");

            let channel_type_str = format!("{:?}", channel_type).to_lowercase();

            let config = if non_interactive {
                if let Some(config_path) = file {
                    std::fs::read_to_string(config_path)?
                } else {
                    anyhow::bail!("--file is required in non-interactive mode");
                }
            } else {
                // Interactive wizard
                run_add_channel_wizard(channel_type).await?
            };

            let channel = client.add_channel(&channel_type_str, &config).await?;
            progress.finish_success(Some(&channel.id));
            println!("Channel ID: {}", channel.id);
            println!("Status: {}", channel.status);

            if !non_interactive {
                println!("\nNext steps:");
                println!(
                    "  1. Authenticate: beebot channel login --id {}",
                    channel.id
                );
                println!(
                    "  2. Test connection: beebot channel test --id {}",
                    channel.id
                );
            }
        }

        ChannelCommand::Remove {
            id,
            delete_data,
            force,
        } => {
            if !force {
                print!("Are you sure you want to remove channel '{}'? [y/N] ", id);
                std::io::Write::flush(&mut std::io::stdout())?;
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Cancelled.");
                    return Ok(());
                }
            }

            let progress = TaskProgress::new(format!("Removing channel {}", id));
            client.remove_channel(&id, delete_data).await?;
            progress.finish_success(None);
        }

        ChannelCommand::Login { id, method } => {
            let progress = TaskProgress::new(format!("Authenticating channel {}", id));
            let auth_url = client
                .login_channel(&id, method.map(|m| format!("{:?}", m)))
                .await?;
            progress.finish_success(None);

            if let Some(url) = auth_url {
                println!("Please open the following URL to authenticate:");
                println!("  {}", url);
                println!("\nWaiting for authentication...");
                client.wait_for_channel_auth(&id).await?;
                println!("✅ Authentication successful!");
            } else {
                println!("✅ Authentication completed");
            }
        }

        ChannelCommand::Logout { id, revoke } => {
            let progress = TaskProgress::new(format!("Logging out channel {}", id));
            client.logout_channel(&id, revoke).await?;
            progress.finish_success(None);
        }

        ChannelCommand::Send {
            channel,
            target,
            message,
            template,
            attachment,
        } => {
            let progress = TaskProgress::new("Sending message");
            let msg = client
                .send_channel_message(
                    &channel,
                    &target,
                    &message,
                    template.as_deref(),
                    &attachment,
                )
                .await?;
            progress.finish_success(Some(&msg.id));
            println!("Message sent successfully!");
            println!("Message ID: {}", msg.id);
        }

        ChannelCommand::Webhook(cmd) => match cmd {
            WebhookCommand::Generate {
                channel,
                path,
                secret,
            } => {
                let progress = TaskProgress::new("Generating webhook");
                let webhook = client
                    .generate_webhook(&channel, path.as_deref(), secret.as_deref())
                    .await?;
                progress.finish_success(None);
                println!("Webhook URL: {}", webhook.url);
                println!("Webhook ID: {}", webhook.id);
                if webhook.secret.is_some() {
                    println!("Secret: {} (save this securely)", webhook.secret.unwrap());
                }
            }
            WebhookCommand::List { channel } => {
                let progress = TaskProgress::new("Listing webhooks");
                let webhooks = client.list_webhooks(&channel).await?;
                progress.finish_success(Some(&format!("{} webhooks", webhooks.len())));
                println!("{:<20} {:<30} {:<20}", "ID", "URL", "Created");
                println!("{}", "-".repeat(80));
                for wh in webhooks {
                    println!("{:<20} {:<30} {:<20}", wh.id, wh.url, wh.created_at);
                }
            }
            WebhookCommand::Delete { id } => {
                let progress = TaskProgress::new(format!("Deleting webhook {}", id));
                client.delete_webhook(&id).await?;
                progress.finish_success(None);
            }
            WebhookCommand::Test { id, payload } => {
                let progress = TaskProgress::new("Testing webhook");
                let result = client.test_webhook(&id, payload.as_deref()).await?;
                progress.finish_success(None);
                println!("Status: {}", result.status);
                println!("Response time: {}ms", result.response_time_ms);
                if let Some(error) = result.error {
                    println!("Error: {}", error);
                }
            }
        },

        ChannelCommand::Test { id, message } => {
            let progress = TaskProgress::new("Testing channel connection");
            let result = client.test_channel(&id, &message).await?;
            progress.finish_success(None);

            if result.success {
                println!("✅ Channel test passed!");
                println!("Response time: {}ms", result.latency_ms);
                println!("Message delivered: {}", result.message_delivered);
            } else {
                println!("❌ Channel test failed!");
                if let Some(error) = result.error {
                    println!("Error: {}", error);
                }
            }
        }
    }

    Ok(())
}

fn print_channel_status(status: &ChannelStatus) {
    let health_icon = match status.health.as_str() {
        "healthy" => "🟢",
        "degraded" => "🟡",
        "unhealthy" => "🔴",
        _ => "⚪",
    };

    println!("{} Channel: {} ({})", health_icon, status.name, status.id);
    println!(
        "   Type: {} | Status: {}",
        status.channel_type, status.status
    );
    println!(
        "   Connected: {} | Last activity: {}",
        status.connected, status.last_activity
    );

    if let Some(stats) = &status.stats {
        println!(
            "   Messages (24h): {} sent, {} received",
            stats.messages_sent, stats.messages_received
        );
        println!("   Error rate: {:.2}%", stats.error_rate * 100.0);
    }
}

async fn run_add_channel_wizard(channel_type: ChannelType) -> Result<String> {
    use dialoguer::Input;

    println!("\n📝 Channel Configuration Wizard");
    println!("================================\n");

    let name: String = Input::new().with_prompt("Channel name").interact_text()?;

    let config = match channel_type {
        ChannelType::Telegram => {
            let bot_token: String = Input::new()
                .with_prompt("Bot Token (from @BotFather)")
                .interact_text()?;
            serde_json::json!({
                "name": name,
                "type": "telegram",
                "bot_token": bot_token,
            })
        }
        ChannelType::Discord => {
            let bot_token: String = Input::new().with_prompt("Bot Token").interact_text()?;
            let application_id: String = Input::new()
                .with_prompt("Application ID (optional)")
                .allow_empty(true)
                .interact_text()?;
            let mut cfg = serde_json::json!({
                "name": name,
                "type": "discord",
                "bot_token": bot_token,
            });
            if !application_id.is_empty() {
                cfg["application_id"] = application_id.into();
            }
            cfg
        }
        ChannelType::Lark => {
            let app_id: String = Input::new().with_prompt("App ID").interact_text()?;
            let app_secret: String = Input::new().with_prompt("App Secret").interact_text()?;
            serde_json::json!({
                "name": name,
                "type": "lark",
                "app_id": app_id,
                "app_secret": app_secret,
            })
        }
        ChannelType::Slack => {
            let bot_token: String = Input::new()
                .with_prompt("Bot Token (xoxb-...)")
                .interact_text()?;
            let signing_secret: String = Input::new()
                .with_prompt("Signing Secret (optional)")
                .allow_empty(true)
                .interact_text()?;
            let mut cfg = serde_json::json!({
                "name": name,
                "type": "slack",
                "bot_token": bot_token,
            });
            if !signing_secret.is_empty() {
                cfg["signing_secret"] = signing_secret.into();
            }
            cfg
        }
        _ => {
            println!(
                "Channel type {:?} requires manual configuration.",
                channel_type
            );
            println!("Please provide the configuration as a JSON string:");
            let config_str: String = Input::new()
                .with_prompt("Configuration JSON")
                .interact_text()?;
            serde_json::from_str(&config_str)?
        }
    };

    Ok(config.to_string())
}

// Import API types from client module
#[allow(unused_imports)]
use crate::client::{
    Channel, ChannelCapabilities, ChannelClient, ChannelStats, ChannelStatus, ChannelTestResult,
    LogEntry, Message as ChannelMessage, NewChannel, RateLimit, ResolvedName, Webhook,
    WebhookTestResult,
};
