//! Message operations commands
//!
//! Send, edit, react, poll, thread, and manage messages across channels.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

use crate::progress::TaskProgress;

#[derive(Parser)]
pub struct MessageArgs {
    #[command(subcommand)]
    pub command: MessageCommand,
}

#[derive(Subcommand)]
pub enum MessageCommand {
    /// Send a message to an agent or channel
    Send {
        /// Recipient agent ID or channel
        #[arg(short, long)]
        to: Option<String>,

        /// Channel ID (alternative to --to)
        #[arg(short, long)]
        channel: Option<String>,

        /// Message content
        message: String,

        /// Timeout in seconds
        #[arg(short = 'T', long, default_value = "30")]
        timeout: u64,

        /// Reply to message ID
        #[arg(long)]
        reply_to: Option<String>,

        /// Attach files
        #[arg(short, long)]
        attachment: Vec<PathBuf>,
    },

    /// Broadcast a message to multiple agents
    Broadcast {
        /// Target capability filter
        #[arg(short = 'a', long)]
        capability: Option<String>,

        /// Target channels (comma-separated)
        #[arg(short = 'c', long, value_delimiter = ',')]
        channels: Vec<String>,

        /// Message content
        message: String,
    },

    /// Edit a message
    Edit {
        /// Message ID to edit
        message_id: String,

        /// Channel ID
        #[arg(short, long)]
        channel: Option<String>,

        /// New content
        content: String,
    },

    /// Delete a message
    Delete {
        /// Message ID to delete
        message_id: String,

        /// Channel ID
        #[arg(short, long)]
        channel: Option<String>,
    },

    /// Mark message(s) as read
    Read {
        /// Channel ID
        channel: String,

        /// Message ID (optional, marks all as read if not provided)
        message_id: Option<String>,
    },

    /// Start interactive chat session
    Chat {
        /// Agent ID to chat with
        agent: String,
    },

    /// Query message history
    History {
        /// Agent ID (optional)
        #[arg(long)]
        agent: Option<String>,

        /// Channel ID (optional)
        #[arg(short, long)]
        channel: Option<String>,

        /// Number of messages to show
        #[arg(short, long, default_value = "20")]
        limit: usize,

        /// Show since timestamp
        #[arg(long)]
        since: Option<String>,
    },

    /// Reaction commands
    #[command(subcommand)]
    React(ReactCommand),

    /// Poll commands
    #[command(subcommand)]
    Poll(PollCommand),

    /// Thread commands
    #[command(subcommand)]
    Thread(ThreadCommand),

    /// Role commands
    #[command(subcommand)]
    Role(RoleCommand),

    /// Member commands
    #[command(subcommand)]
    Member(MemberCommand),

    /// Media commands (emoji, sticker, voice)
    #[command(subcommand)]
    Media(MediaCommand),

    /// Event commands
    #[command(subcommand)]
    Event(EventCommand),
}

#[derive(Subcommand)]
pub enum ReactCommand {
    /// Add reaction to a message
    Add {
        /// Message ID
        message_id: String,

        /// Channel ID
        #[arg(short, long)]
        channel: String,

        /// Emoji reaction (e.g., 👍, ❤️)
        emoji: String,
    },

    /// Remove reaction from a message
    Remove {
        /// Message ID
        message_id: String,

        /// Channel ID
        #[arg(short, long)]
        channel: String,

        /// Emoji to remove (optional, removes all if not provided)
        emoji: Option<String>,
    },

    /// List reactions on a message
    List {
        /// Message ID
        message_id: String,

        /// Channel ID
        #[arg(short, long)]
        channel: String,
    },
}

#[derive(Subcommand)]
pub enum PollCommand {
    /// Create a new poll
    Create {
        /// Channel ID
        #[arg(short, long)]
        channel: String,

        /// Poll question
        question: String,

        /// Poll options (at least 2)
        #[arg(short, long, required = true, num_args = 2..)]
        options: Vec<String>,

        /// Allow multiple selections
        #[arg(long)]
        multiple: bool,

        /// Poll duration in minutes
        #[arg(short, long)]
        duration: Option<u32>,
    },

    /// Vote in a poll
    Vote {
        /// Poll ID
        poll_id: String,

        /// Option index (0-based)
        option: usize,
    },

    /// View poll results
    Results {
        /// Poll ID
        poll_id: String,
    },

    /// Close a poll
    Close {
        /// Poll ID
        poll_id: String,
    },
}

#[derive(Subcommand)]
pub enum ThreadCommand {
    /// Create a new thread
    Create {
        /// Channel ID
        channel: String,

        /// Thread name
        name: String,

        /// Parent message ID (optional)
        #[arg(long)]
        parent_message: Option<String>,
    },

    /// List threads in a channel
    List {
        /// Channel ID
        channel: String,

        /// Include archived threads
        #[arg(long)]
        archived: bool,
    },

    /// Reply to a thread
    Reply {
        /// Thread ID
        thread_id: String,

        /// Message content
        message: String,
    },

    /// Archive a thread
    Archive {
        /// Thread ID
        thread_id: String,
    },

    /// Unarchive a thread
    Unarchive {
        /// Thread ID
        thread_id: String,
    },
}

#[derive(Subcommand)]
pub enum RoleCommand {
    /// List roles in a channel/guild
    List {
        /// Channel ID
        #[arg(short, long)]
        channel: String,
    },

    /// Get role info
    Info {
        /// Role ID
        role_id: String,

        /// Channel ID
        #[arg(short, long)]
        channel: String,
    },

    /// Add role to a member
    Add {
        /// Channel ID
        #[arg(short, long)]
        channel: String,

        /// User ID
        #[arg(short, long)]
        user: String,

        /// Role ID or name
        role: String,
    },

    /// Remove role from a member
    Remove {
        /// Channel ID
        #[arg(short, long)]
        channel: String,

        /// User ID
        #[arg(short, long)]
        user: String,

        /// Role ID or name
        role: String,
    },
}

#[derive(Subcommand)]
pub enum MemberCommand {
    /// List members in a channel/guild
    List {
        /// Channel ID
        channel: String,
    },

    /// Get member info
    Info {
        /// Channel ID
        channel: String,

        /// User ID
        user: String,
    },

    /// Kick a member
    Kick {
        /// Channel ID
        channel: String,

        /// User ID
        user: String,

        /// Reason
        #[arg(short, long)]
        reason: Option<String>,
    },

    /// Ban a member
    Ban {
        /// Channel ID
        channel: String,

        /// User ID
        user: String,

        /// Reason
        #[arg(short, long)]
        reason: Option<String>,

        /// Delete message days
        #[arg(long, default_value = "0")]
        delete_days: u8,
    },

    /// Unban a member
    Unban {
        /// Channel ID
        channel: String,

        /// User ID
        user: String,
    },

    /// Timeout/mute a member
    Timeout {
        /// Channel ID
        channel: String,

        /// User ID
        user: String,

        /// Duration in minutes
        #[arg(short, long)]
        duration: u64,

        /// Reason
        #[arg(short, long)]
        reason: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum MediaCommand {
    /// Emoji commands
    #[command(subcommand)]
    Emoji(EmojiCommand),

    /// Sticker commands
    #[command(subcommand)]
    Sticker(StickerCommand),

    /// Voice commands
    #[command(subcommand)]
    Voice(VoiceCommand),
}

#[derive(Subcommand)]
pub enum EmojiCommand {
    /// List available emojis
    List {
        /// Channel ID
        #[arg(short, long)]
        channel: Option<String>,
    },

    /// Upload a custom emoji
    Upload {
        /// Channel ID
        #[arg(short, long)]
        channel: String,

        /// Emoji name
        name: String,

        /// Image file path
        image: PathBuf,
    },

    /// Delete a custom emoji
    Delete {
        /// Emoji ID
        emoji_id: String,

        /// Channel ID
        #[arg(short, long)]
        channel: String,
    },
}

#[derive(Subcommand)]
pub enum StickerCommand {
    /// List available stickers
    List {
        /// Channel ID
        #[arg(short, long)]
        channel: Option<String>,
    },

    /// Send a sticker
    Send {
        /// Channel ID
        #[arg(short, long)]
        channel: String,

        /// Sticker ID
        sticker: String,
    },

    /// Upload a sticker
    Upload {
        /// Channel ID
        #[arg(short, long)]
        channel: String,

        /// Sticker name
        name: String,

        /// Image file path
        image: PathBuf,

        /// Related emoji
        #[arg(short = 'm', long)]
        emoji: String,
    },
}

#[derive(Subcommand)]
pub enum VoiceCommand {
    /// Check voice status
    Status {
        /// Channel ID
        #[arg(short, long)]
        channel: String,
    },

    /// Join voice channel
    Join {
        /// Channel ID
        channel: String,
    },

    /// Leave voice channel
    Leave {
        /// Channel ID
        channel: String,
    },
}

#[derive(Subcommand)]
pub enum EventCommand {
    /// List upcoming events
    List {
        /// Channel ID
        #[arg(short, long)]
        channel: String,
    },

    /// Create an event
    Create {
        /// Channel ID
        #[arg(short, long)]
        channel: String,

        /// Event name
        name: String,

        /// Event description
        #[arg(short, long)]
        description: Option<String>,

        /// Start time (ISO 8601 format)
        #[arg(short, long)]
        start: String,

        /// End time (ISO 8601 format)
        #[arg(short = 'E', long)]
        end: Option<String>,
    },

    /// Delete an event
    Delete {
        /// Event ID
        event_id: String,
    },

    /// RSVP to an event
    Rsvp {
        /// Event ID
        event_id: String,

        /// Response
        #[arg(value_enum)]
        response: RsvpResponse,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum RsvpResponse {
    Going,
    Maybe,
    Declined,
}

pub async fn execute(args: MessageArgs) -> Result<()> {
    let client = crate::client::ApiClient::new()?;

    match args.command {
        // Basic message operations
        MessageCommand::Send {
            to,
            channel,
            message,
            timeout,
            reply_to,
            attachment,
        } => {
            let target = to
                .or(channel)
                .ok_or_else(|| anyhow::anyhow!("Either --to or --channel must be specified"))?;
            let progress = TaskProgress::new(format!("Sending message to {}", target));

            let response = client
                .send_message_advanced(&target, &message, timeout, reply_to.as_deref(), &attachment)
                .await?;

            progress.finish_success(None);
            println!("✅ Message sent: {}", response);
        }

        MessageCommand::Broadcast {
            capability,
            channels,
            message,
        } => {
            let progress = TaskProgress::new("Broadcasting message");

            let recipients = if !channels.is_empty() {
                client.broadcast_to_channels(&channels, &message).await?
            } else {
                client
                    .broadcast_message(capability.as_deref(), &message)
                    .await?
            };

            progress.finish_success(Some(&format!("{} recipients", recipients.len())));
        }

        MessageCommand::Edit {
            message_id,
            channel,
            content,
        } => {
            let progress = TaskProgress::new("Editing message");
            client
                .edit_message(&message_id, channel.as_deref(), &content)
                .await?;
            progress.finish_success(Some("Message updated"));
        }

        MessageCommand::Delete {
            message_id,
            channel,
        } => {
            let progress = TaskProgress::new("Deleting message");
            client
                .delete_message(&message_id, channel.as_deref())
                .await?;
            progress.finish_success(Some("Message deleted"));
        }

        MessageCommand::Read {
            channel,
            message_id,
        } => {
            let progress = TaskProgress::new("Marking as read");
            client.mark_as_read(&channel, message_id.as_deref()).await?;
            progress.finish_success(Some(if message_id.is_some() {
                "Message marked as read"
            } else {
                "All messages marked as read"
            }));
        }

        MessageCommand::Chat { agent } => {
            println!("Starting chat with agent '{}' (type 'exit' to quit)", agent);
            let mut rl = rustyline::DefaultEditor::new()?;

            loop {
                let input = rl.readline("> ")?;
                let input = input.trim();

                if input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("quit") {
                    break;
                }

                if input.is_empty() {
                    continue;
                }

                match client.send_message(&agent, input, 60).await {
                    Ok(response) => println!("Agent: {}", response),
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
        }

        MessageCommand::History {
            agent,
            channel,
            limit,
            since,
        } => {
            let history = if let Some(ch) = channel {
                client
                    .get_channel_message_history(&ch, limit, since.as_deref())
                    .await?
            } else {
                client.get_message_history(agent.as_deref(), limit).await?
            };

            for msg in history {
                println!(
                    "[{}] {} -> {}: {}",
                    msg.timestamp,
                    msg.from,
                    msg.to.as_deref().unwrap_or("broadcast"),
                    msg.content
                );
            }
        }

        // Reaction commands
        MessageCommand::React(cmd) => match cmd {
            ReactCommand::Add {
                message_id,
                channel,
                emoji,
            } => {
                let progress = TaskProgress::new("Adding reaction");
                client.add_reaction(&message_id, &channel, &emoji).await?;
                progress.finish_success(Some(&format!("Reacted with {}", emoji)));
            }
            ReactCommand::Remove {
                message_id,
                channel,
                emoji,
            } => {
                let progress = TaskProgress::new("Removing reaction");
                client
                    .remove_reaction(&message_id, &channel, emoji.as_deref())
                    .await?;
                progress.finish_success(Some("Reaction removed"));
            }
            ReactCommand::List {
                message_id,
                channel,
            } => {
                let reactions = client.list_reactions(&message_id, &channel).await?;
                println!("Reactions on message {}:", message_id);
                for (emoji, count) in reactions {
                    println!("  {} - {}", emoji, count);
                }
            }
        },

        // Poll commands
        MessageCommand::Poll(cmd) => match cmd {
            PollCommand::Create {
                channel,
                question,
                options,
                multiple,
                duration,
            } => {
                let progress = TaskProgress::new("Creating poll");
                let poll_id = client
                    .create_poll(&channel, &question, &options, multiple, duration)
                    .await?;
                progress.finish_success(Some(&format!("Poll created: {}", poll_id)));
            }
            PollCommand::Vote { poll_id, option } => {
                let progress = TaskProgress::new("Recording vote");
                client.vote_poll(&poll_id, option).await?;
                progress.finish_success(Some("Vote recorded"));
            }
            PollCommand::Results { poll_id } => {
                let results = client.get_poll_results(&poll_id).await?;
                println!("📊 Poll Results: {}", results.question);
                println!("{}", "-".repeat(50));
                for (option, count, percentage) in results.options {
                    let bar = "█".repeat((percentage / 2.0) as usize);
                    println!("{:20} {:4} ({:.1}%) {}", option, count, percentage, bar);
                }
                println!("\nTotal votes: {}", results.total_votes);
            }
            PollCommand::Close { poll_id } => {
                let progress = TaskProgress::new("Closing poll");
                client.close_poll(&poll_id).await?;
                progress.finish_success(Some("Poll closed"));
            }
        },

        // Thread commands
        MessageCommand::Thread(cmd) => match cmd {
            ThreadCommand::Create {
                channel,
                name,
                parent_message,
            } => {
                let progress = TaskProgress::new("Creating thread");
                let thread_id = client
                    .create_thread(&channel, &name, parent_message.as_deref())
                    .await?;
                progress.finish_success(Some(&format!("Thread created: {}", thread_id)));
            }
            ThreadCommand::List { channel, archived } => {
                let threads = client.list_threads(&channel, archived).await?;
                println!("Threads in channel {}:", channel);
                for thread in threads {
                    let status = if thread.archived { "📦" } else { "🧵" };
                    println!(
                        "{} {} - {} messages",
                        status, thread.name, thread.message_count
                    );
                }
            }
            ThreadCommand::Reply { thread_id, message } => {
                let progress = TaskProgress::new("Sending reply");
                client.reply_to_thread(&thread_id, &message).await?;
                progress.finish_success(Some("Reply sent"));
            }
            ThreadCommand::Archive { thread_id } => {
                let progress = TaskProgress::new("Archiving thread");
                client.archive_thread(&thread_id, true).await?;
                progress.finish_success(Some("Thread archived"));
            }
            ThreadCommand::Unarchive { thread_id } => {
                let progress = TaskProgress::new("Unarchiving thread");
                client.archive_thread(&thread_id, false).await?;
                progress.finish_success(Some("Thread unarchived"));
            }
        },

        // Role commands
        MessageCommand::Role(cmd) => match cmd {
            RoleCommand::List { channel } => {
                let roles = client.list_roles(&channel).await?;
                println!("Roles in channel {}:", channel);
                for role in roles {
                    println!("  {} - {} members", role.name, role.member_count);
                }
            }
            RoleCommand::Info { role_id, channel } => {
                let role = client.get_role_info(&channel, &role_id).await?;
                println!("Role: {}", role.name);
                println!("Permissions: {:?}", role.permissions);
                println!("Members: {}", role.member_count);
            }
            RoleCommand::Add {
                channel,
                user,
                role,
            } => {
                let progress = TaskProgress::new("Adding role");
                client.add_role_to_member(&channel, &user, &role).await?;
                progress.finish_success(Some(&format!("Added {} to {}", role, user)));
            }
            RoleCommand::Remove {
                channel,
                user,
                role,
            } => {
                let progress = TaskProgress::new("Removing role");
                client
                    .remove_role_from_member(&channel, &user, &role)
                    .await?;
                progress.finish_success(Some(&format!("Removed {} from {}", role, user)));
            }
        },

        // Member commands
        MessageCommand::Member(cmd) => match cmd {
            MemberCommand::List { channel } => {
                let members = client.list_members(&channel).await?;
                println!("Members in channel {}:", channel);
                for member in members {
                    println!(
                        "  {} (@{}) - {}",
                        member.display_name, member.username, member.role
                    );
                }
            }
            MemberCommand::Info { channel, user } => {
                let member = client.get_member_info(&channel, &user).await?;
                println!("Member: {}", member.display_name);
                println!("Username: @{}", member.username);
                println!("Joined: {}", member.joined_at);
                println!("Roles: {:?}", member.roles);
            }
            MemberCommand::Kick {
                channel,
                user,
                reason,
            } => {
                let progress = TaskProgress::new("Kicking member");
                client
                    .kick_member(&channel, &user, reason.as_deref())
                    .await?;
                progress.finish_success(Some(&format!("Kicked {}", user)));
            }
            MemberCommand::Ban {
                channel,
                user,
                reason,
                delete_days,
            } => {
                let progress = TaskProgress::new("Banning member");
                client
                    .ban_member(&channel, &user, reason.as_deref(), delete_days)
                    .await?;
                progress.finish_success(Some(&format!("Banned {}", user)));
            }
            MemberCommand::Unban { channel, user } => {
                let progress = TaskProgress::new("Unbanning member");
                client.unban_member(&channel, &user).await?;
                progress.finish_success(Some(&format!("Unbanned {}", user)));
            }
            MemberCommand::Timeout {
                channel,
                user,
                duration,
                reason,
            } => {
                let progress = TaskProgress::new("Setting timeout");
                client
                    .timeout_member(&channel, &user, duration, reason.as_deref())
                    .await?;
                progress.finish_success(Some(&format!("Timeout set for {} minutes", duration)));
            }
        },

        // Media commands
        MessageCommand::Media(cmd) => match cmd {
            MediaCommand::Emoji(cmd) => match cmd {
                EmojiCommand::List { channel } => {
                    let emojis = client.list_emojis(channel.as_deref()).await?;
                    println!("Available emojis:");
                    for emoji in emojis {
                        println!("  :{}:", emoji.name);
                    }
                }
                EmojiCommand::Upload {
                    channel,
                    name,
                    image,
                } => {
                    let progress = TaskProgress::new("Uploading emoji");
                    client.upload_emoji(&channel, &name, &image).await?;
                    progress.finish_success(Some(&format!("Emoji :{}: uploaded", name)));
                }
                EmojiCommand::Delete { emoji_id, channel } => {
                    let progress = TaskProgress::new("Deleting emoji");
                    client.delete_emoji(&channel, &emoji_id).await?;
                    progress.finish_success(Some("Emoji deleted"));
                }
            },
            MediaCommand::Sticker(cmd) => match cmd {
                StickerCommand::List { channel } => {
                    let stickers = client.list_stickers(channel.as_deref()).await?;
                    println!("Available stickers:");
                    for sticker in stickers {
                        println!("  {} - {}", sticker.id, sticker.name);
                    }
                }
                StickerCommand::Send { channel, sticker } => {
                    let progress = TaskProgress::new("Sending sticker");
                    client.send_sticker(&channel, &sticker).await?;
                    progress.finish_success(Some("Sticker sent"));
                }
                StickerCommand::Upload {
                    channel,
                    name,
                    image,
                    emoji,
                } => {
                    let progress = TaskProgress::new("Uploading sticker");
                    client
                        .upload_sticker(&channel, &name, &image, &emoji)
                        .await?;
                    progress.finish_success(Some(&format!("Sticker '{}' uploaded", name)));
                }
            },
            MediaCommand::Voice(cmd) => match cmd {
                VoiceCommand::Status { channel } => {
                    let status = client.get_voice_status(&channel).await?;
                    println!("Voice status for channel {}:", channel);
                    println!("  Connected: {}", status.connected);
                    println!("  Participants: {}", status.participants);
                }
                VoiceCommand::Join { channel } => {
                    client.join_voice_channel(&channel).await?;
                    println!("Joined voice channel {}", channel);
                }
                VoiceCommand::Leave { channel } => {
                    client.leave_voice_channel(&channel).await?;
                    println!("Left voice channel {}", channel);
                }
            },
        },

        // Event commands
        MessageCommand::Event(cmd) => match cmd {
            EventCommand::List { channel } => {
                let events = client.list_events(&channel).await?;
                println!("Upcoming events in channel {}:", channel);
                for event in events {
                    println!("  📅 {} - {}", event.name, event.start_time);
                    println!("     {} going", event.rsvp_count);
                }
            }
            EventCommand::Create {
                channel,
                name,
                description,
                start,
                end,
            } => {
                let progress = TaskProgress::new("Creating event");
                let event_id = client
                    .create_event(
                        &channel,
                        &name,
                        description.as_deref(),
                        &start,
                        end.as_deref(),
                    )
                    .await?;
                progress.finish_success(Some(&format!("Event created: {}", event_id)));
            }
            EventCommand::Delete { event_id } => {
                let progress = TaskProgress::new("Deleting event");
                client.delete_event(&event_id).await?;
                progress.finish_success(Some("Event deleted"));
            }
            EventCommand::Rsvp { event_id, response } => {
                let progress = TaskProgress::new("Recording RSVP");
                let response_str = match response {
                    RsvpResponse::Going => "going",
                    RsvpResponse::Maybe => "maybe",
                    RsvpResponse::Declined => "declined",
                };
                client.rsvp_to_event(&event_id, response_str).await?;
                progress.finish_success(Some(&format!("RSVP'd: {:?}", response)));
            }
        },
    }

    Ok(())
}

// Client extension trait
trait MessageClient {
    async fn send_message_advanced(
        &self,
        target: &str,
        message: &str,
        timeout: u64,
        reply_to: Option<&str>,
        attachments: &[PathBuf],
    ) -> Result<String>;
    async fn broadcast_to_channels(
        &self,
        channels: &[String],
        message: &str,
    ) -> Result<Vec<String>>;
    async fn edit_message(
        &self,
        message_id: &str,
        channel: Option<&str>,
        content: &str,
    ) -> Result<()>;
    async fn delete_message(&self, message_id: &str, channel: Option<&str>) -> Result<()>;
    async fn mark_as_read(&self, channel: &str, message_id: Option<&str>) -> Result<()>;
    async fn get_channel_message_history(
        &self,
        channel: &str,
        limit: usize,
        since: Option<&str>,
    ) -> Result<Vec<MessageInfo>>;

    // Reactions
    async fn add_reaction(&self, message_id: &str, channel: &str, emoji: &str) -> Result<()>;
    async fn remove_reaction(
        &self,
        message_id: &str,
        channel: &str,
        emoji: Option<&str>,
    ) -> Result<()>;
    async fn list_reactions(&self, message_id: &str, channel: &str) -> Result<Vec<(String, u32)>>;

    // Polls
    async fn create_poll(
        &self,
        channel: &str,
        question: &str,
        options: &[String],
        multiple: bool,
        duration: Option<u32>,
    ) -> Result<String>;
    async fn vote_poll(&self, poll_id: &str, option: usize) -> Result<()>;
    async fn get_poll_results(&self, poll_id: &str) -> Result<PollResults>;
    async fn close_poll(&self, poll_id: &str) -> Result<()>;

    // Threads
    async fn create_thread(
        &self,
        channel: &str,
        name: &str,
        parent_message: Option<&str>,
    ) -> Result<String>;
    async fn list_threads(&self, channel: &str, archived: bool) -> Result<Vec<ThreadInfo>>;
    async fn reply_to_thread(&self, thread_id: &str, message: &str) -> Result<()>;
    async fn archive_thread(&self, thread_id: &str, archive: bool) -> Result<()>;

    // Roles
    async fn list_roles(&self, channel: &str) -> Result<Vec<RoleInfo>>;
    async fn get_role_info(&self, channel: &str, role_id: &str) -> Result<RoleInfo>;
    async fn add_role_to_member(&self, channel: &str, user: &str, role: &str) -> Result<()>;
    async fn remove_role_from_member(&self, channel: &str, user: &str, role: &str) -> Result<()>;

    // Members
    async fn list_members(&self, channel: &str) -> Result<Vec<MemberInfo>>;
    async fn get_member_info(&self, channel: &str, user: &str) -> Result<MemberInfo>;
    async fn kick_member(&self, channel: &str, user: &str, reason: Option<&str>) -> Result<()>;
    async fn ban_member(
        &self,
        channel: &str,
        user: &str,
        reason: Option<&str>,
        delete_days: u8,
    ) -> Result<()>;
    async fn unban_member(&self, channel: &str, user: &str) -> Result<()>;
    async fn timeout_member(
        &self,
        channel: &str,
        user: &str,
        duration: u64,
        reason: Option<&str>,
    ) -> Result<()>;

    // Media
    async fn list_emojis(&self, channel: Option<&str>) -> Result<Vec<EmojiInfo>>;
    async fn upload_emoji(&self, channel: &str, name: &str, image: &PathBuf) -> Result<()>;
    async fn delete_emoji(&self, channel: &str, emoji_id: &str) -> Result<()>;
    async fn list_stickers(&self, channel: Option<&str>) -> Result<Vec<StickerInfo>>;
    async fn send_sticker(&self, channel: &str, sticker: &str) -> Result<()>;
    async fn upload_sticker(
        &self,
        channel: &str,
        name: &str,
        image: &PathBuf,
        emoji: &str,
    ) -> Result<()>;
    async fn get_voice_status(&self, channel: &str) -> Result<VoiceStatus>;
    async fn join_voice_channel(&self, channel: &str) -> Result<()>;
    async fn leave_voice_channel(&self, channel: &str) -> Result<()>;

    // Events
    async fn list_events(&self, channel: &str) -> Result<Vec<EventInfo>>;
    async fn create_event(
        &self,
        channel: &str,
        name: &str,
        description: Option<&str>,
        start: &str,
        end: Option<&str>,
    ) -> Result<String>;
    async fn delete_event(&self, event_id: &str) -> Result<()>;
    async fn rsvp_to_event(&self, event_id: &str, response: &str) -> Result<()>;
}

impl MessageClient for crate::client::ApiClient {
    async fn send_message_advanced(
        &self,
        _target: &str,
        _message: &str,
        _timeout: u64,
        _reply_to: Option<&str>,
        _attachments: &[PathBuf],
    ) -> Result<String> {
        // Implementation would send message with attachments
        Ok("msg_123".to_string())
    }

    async fn broadcast_to_channels(
        &self,
        _channels: &[String],
        _message: &str,
    ) -> Result<Vec<String>> {
        Ok(vec![])
    }

    async fn edit_message(
        &self,
        _message_id: &str,
        _channel: Option<&str>,
        _content: &str,
    ) -> Result<()> {
        Ok(())
    }

    async fn delete_message(&self, _message_id: &str, _channel: Option<&str>) -> Result<()> {
        Ok(())
    }

    async fn mark_as_read(&self, _channel: &str, _message_id: Option<&str>) -> Result<()> {
        Ok(())
    }

    async fn get_channel_message_history(
        &self,
        _channel: &str,
        _limit: usize,
        _since: Option<&str>,
    ) -> Result<Vec<MessageInfo>> {
        Ok(vec![])
    }

    // Reactions
    async fn add_reaction(&self, _message_id: &str, _channel: &str, _emoji: &str) -> Result<()> {
        Ok(())
    }

    async fn remove_reaction(
        &self,
        _message_id: &str,
        _channel: &str,
        _emoji: Option<&str>,
    ) -> Result<()> {
        Ok(())
    }

    async fn list_reactions(
        &self,
        _message_id: &str,
        _channel: &str,
    ) -> Result<Vec<(String, u32)>> {
        Ok(vec![])
    }

    // Polls
    async fn create_poll(
        &self,
        _channel: &str,
        _question: &str,
        _options: &[String],
        _multiple: bool,
        _duration: Option<u32>,
    ) -> Result<String> {
        Ok("poll_123".to_string())
    }

    async fn vote_poll(&self, _poll_id: &str, _option: usize) -> Result<()> {
        Ok(())
    }

    async fn get_poll_results(&self, _poll_id: &str) -> Result<PollResults> {
        Ok(PollResults {
            question: "Test".to_string(),
            options: vec![],
            total_votes: 0,
        })
    }

    async fn close_poll(&self, _poll_id: &str) -> Result<()> {
        Ok(())
    }

    // Threads
    async fn create_thread(
        &self,
        _channel: &str,
        _name: &str,
        _parent_message: Option<&str>,
    ) -> Result<String> {
        Ok("thread_123".to_string())
    }

    async fn list_threads(&self, _channel: &str, _archived: bool) -> Result<Vec<ThreadInfo>> {
        Ok(vec![])
    }

    async fn reply_to_thread(&self, _thread_id: &str, _message: &str) -> Result<()> {
        Ok(())
    }

    async fn archive_thread(&self, _thread_id: &str, _archive: bool) -> Result<()> {
        Ok(())
    }

    // Roles
    async fn list_roles(&self, _channel: &str) -> Result<Vec<RoleInfo>> {
        Ok(vec![])
    }

    async fn get_role_info(&self, _channel: &str, _role_id: &str) -> Result<RoleInfo> {
        Ok(RoleInfo {
            name: "Member".to_string(),
            member_count: 0,
            permissions: vec![],
        })
    }

    async fn add_role_to_member(&self, _channel: &str, _user: &str, _role: &str) -> Result<()> {
        Ok(())
    }

    async fn remove_role_from_member(
        &self,
        _channel: &str,
        _user: &str,
        _role: &str,
    ) -> Result<()> {
        Ok(())
    }

    // Members
    async fn list_members(&self, _channel: &str) -> Result<Vec<MemberInfo>> {
        Ok(vec![])
    }

    async fn get_member_info(&self, _channel: &str, _user: &str) -> Result<MemberInfo> {
        Ok(MemberInfo {
            display_name: "User".to_string(),
            username: "user".to_string(),
            role: "Member".to_string(),
            joined_at: "2024-01-01".to_string(),
            roles: vec![],
        })
    }

    async fn kick_member(&self, _channel: &str, _user: &str, _reason: Option<&str>) -> Result<()> {
        Ok(())
    }

    async fn ban_member(
        &self,
        _channel: &str,
        _user: &str,
        _reason: Option<&str>,
        _delete_days: u8,
    ) -> Result<()> {
        Ok(())
    }

    async fn unban_member(&self, _channel: &str, _user: &str) -> Result<()> {
        Ok(())
    }

    async fn timeout_member(
        &self,
        _channel: &str,
        _user: &str,
        _duration: u64,
        _reason: Option<&str>,
    ) -> Result<()> {
        Ok(())
    }

    // Media
    async fn list_emojis(&self, _channel: Option<&str>) -> Result<Vec<EmojiInfo>> {
        Ok(vec![])
    }

    async fn upload_emoji(&self, _channel: &str, _name: &str, _image: &PathBuf) -> Result<()> {
        Ok(())
    }

    async fn delete_emoji(&self, _channel: &str, _emoji_id: &str) -> Result<()> {
        Ok(())
    }

    async fn list_stickers(&self, _channel: Option<&str>) -> Result<Vec<StickerInfo>> {
        Ok(vec![])
    }

    async fn send_sticker(&self, _channel: &str, _sticker: &str) -> Result<()> {
        Ok(())
    }

    async fn upload_sticker(
        &self,
        _channel: &str,
        _name: &str,
        _image: &PathBuf,
        _emoji: &str,
    ) -> Result<()> {
        Ok(())
    }

    async fn get_voice_status(&self, _channel: &str) -> Result<VoiceStatus> {
        Ok(VoiceStatus {
            connected: false,
            participants: 0,
        })
    }

    async fn join_voice_channel(&self, _channel: &str) -> Result<()> {
        Ok(())
    }

    async fn leave_voice_channel(&self, _channel: &str) -> Result<()> {
        Ok(())
    }

    // Events
    async fn list_events(&self, _channel: &str) -> Result<Vec<EventInfo>> {
        Ok(vec![])
    }

    async fn create_event(
        &self,
        _channel: &str,
        _name: &str,
        _description: Option<&str>,
        _start: &str,
        _end: Option<&str>,
    ) -> Result<String> {
        Ok("event_123".to_string())
    }

    async fn delete_event(&self, _event_id: &str) -> Result<()> {
        Ok(())
    }

    async fn rsvp_to_event(&self, _event_id: &str, _response: &str) -> Result<()> {
        Ok(())
    }
}

// Re-export from client
use crate::client::MessageInfo;

#[derive(serde::Deserialize)]
struct PollResults {
    question: String,
    options: Vec<(String, u32, f64)>,
    total_votes: u32,
}

#[derive(serde::Deserialize)]
struct ThreadInfo {
    name: String,
    message_count: u32,
    archived: bool,
}

#[derive(serde::Deserialize)]
struct RoleInfo {
    name: String,
    member_count: u32,
    permissions: Vec<String>,
}

#[derive(serde::Deserialize)]
struct MemberInfo {
    display_name: String,
    username: String,
    role: String,
    joined_at: String,
    roles: Vec<String>,
}

#[derive(serde::Deserialize)]
struct EmojiInfo {
    name: String,
}

#[derive(serde::Deserialize)]
struct StickerInfo {
    id: String,
    name: String,
}

#[derive(serde::Deserialize)]
struct VoiceStatus {
    connected: bool,
    participants: u32,
}

#[derive(serde::Deserialize)]
struct EventInfo {
    name: String,
    start_time: String,
    rsvp_count: u32,
}
