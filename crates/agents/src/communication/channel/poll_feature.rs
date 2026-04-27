//! Cross-Platform Poll/Voting Feature
//!
//! Provides a unified polling mechanism across multiple communication
//! platforms. Supports WhatsApp, Discord, Teams, and other platforms with
//! interactive messages.
//!
//! # Features
//! - Multi-option polls
//! - Time-limited voting
//! - Anonymous vs public voting
//! - Real-time results
//! - Cross-platform compatibility

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info};
use uuid::Uuid;

use crate::communication::{Message, PlatformType};
use crate::error::{AgentError, Result};

/// Poll/Vote manager for cross-platform polling
pub struct PollManager {
    /// Active polls
    polls: Arc<RwLock<HashMap<String, Poll>>>,
    /// Poll results storage
    results: Arc<RwLock<HashMap<String, PollResult>>>,
}

/// A poll/vote
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Poll {
    /// Unique poll ID
    pub id: String,
    /// Poll question
    pub question: String,
    /// Poll options
    pub options: Vec<PollOption>,
    /// Platform where the poll is hosted
    pub platform: PlatformType,
    /// Channel/Chat ID
    pub channel_id: String,
    /// Creator user ID
    pub creator_id: String,
    /// Poll configuration
    pub config: PollConfig,
    /// Poll status
    pub status: PollStatus,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Expiration timestamp (optional)
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Platform-specific message ID
    pub platform_message_id: Option<String>,
}

/// Poll option
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollOption {
    /// Option ID (usually 1, 2, 3...)
    pub id: String,
    /// Option text
    pub text: String,
    /// Emoji/icon for the option (optional)
    pub emoji: Option<String>,
}

/// Poll configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollConfig {
    /// Allow multiple selections
    pub allow_multiple: bool,
    /// Anonymous voting (don't show who voted)
    pub anonymous: bool,
    /// Show live results
    pub show_results: bool,
    /// Allow changing vote
    pub allow_change: bool,
    /// Minimum votes required
    pub min_votes: Option<u32>,
    /// Maximum votes allowed per user
    pub max_votes_per_user: Option<u32>,
}

impl Default for PollConfig {
    fn default() -> Self {
        Self {
            allow_multiple: false,
            anonymous: true,
            show_results: true,
            allow_change: false,
            min_votes: None,
            max_votes_per_user: Some(1),
        }
    }
}

/// Poll status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PollStatus {
    /// Poll is active and accepting votes
    Active,
    /// Poll is paused
    Paused,
    /// Poll has ended
    Closed,
    /// Poll was cancelled
    Cancelled,
}

/// A vote cast by a user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    /// Poll ID
    pub poll_id: String,
    /// User ID who cast the vote
    pub user_id: String,
    /// Selected option IDs
    pub option_ids: Vec<String>,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Poll results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollResult {
    /// Poll ID
    pub poll_id: String,
    /// Total votes cast
    pub total_votes: u32,
    /// Unique voters
    pub unique_voters: u32,
    /// Votes per option
    pub option_votes: HashMap<String, u32>,
    /// Detailed votes (if not anonymous)
    pub votes: Vec<Vote>,
    /// Winning option(s)
    pub winners: Vec<String>,
}

/// Platform-specific poll formatter
pub trait PollFormatter: Send + Sync {
    /// Format poll for display
    fn format_poll(&self, poll: &Poll, results: Option<&PollResult>) -> String;
    /// Format poll for interactive message (buttons, etc.)
    fn format_interactive(&self, poll: &Poll) -> serde_json::Value;
    /// Parse vote from platform message
    fn parse_vote(&self, message: &Message) -> Option<VoteIntent>;
}

/// Vote intent (parsed from user message)
#[derive(Debug, Clone)]
pub struct VoteIntent {
    pub poll_id: String,
    pub option_id: String,
    pub user_id: String,
}

impl PollManager {
    /// Create a new poll manager
    pub fn new() -> Self {
        Self {
            polls: Arc::new(RwLock::new(HashMap::new())),
            results: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new poll
    pub async fn create_poll(
        &self,
        question: impl Into<String>,
        options: Vec<String>,
        platform: PlatformType,
        channel_id: impl Into<String>,
        creator_id: impl Into<String>,
        config: PollConfig,
        duration_minutes: Option<u32>,
    ) -> Result<Poll> {
        let poll_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now();

        let poll_options: Vec<PollOption> = options
            .into_iter()
            .enumerate()
            .map(|(i, text)| PollOption {
                id: (i + 1).to_string(),
                text,
                emoji: Self::number_to_emoji(i + 1),
            })
            .collect();

        let expires_at = duration_minutes.map(|m| now + chrono::Duration::minutes(m as i64));

        let poll = Poll {
            id: poll_id.clone(),
            question: question.into(),
            options: poll_options,
            platform,
            channel_id: channel_id.into(),
            creator_id: creator_id.into(),
            config,
            status: PollStatus::Active,
            created_at: now,
            expires_at,
            platform_message_id: None,
        };

        // Store poll
        {
            let mut polls = self.polls.write().await;
            polls.insert(poll_id.clone(), poll.clone());
        }

        // Initialize empty results
        {
            let mut results = self.results.write().await;
            let option_votes: HashMap<String, u32> =
                poll.options.iter().map(|o| (o.id.clone(), 0)).collect();

            results.insert(
                poll_id,
                PollResult {
                    poll_id: poll.id.clone(),
                    total_votes: 0,
                    unique_voters: 0,
                    option_votes,
                    votes: vec![],
                    winners: vec![],
                },
            );
        }

        info!("Created poll: {} on {:?}", poll.id, platform);
        Ok(poll)
    }

    /// Cast a vote
    pub async fn cast_vote(&self, vote: Vote) -> Result<()> {
        let poll_id = vote.poll_id.clone();
        let user_id = vote.user_id.clone();
        let option_ids = vote.option_ids.clone();
        let _allow_multiple;
        let allow_change;

        {
            let mut polls = self.polls.write().await;
            let poll = polls
                .get_mut(&poll_id)
                .ok_or_else(|| AgentError::not_found(format!("Poll {} not found", poll_id)))?;

            // Check if poll is active
            if poll.status != PollStatus::Active {
                return Err(AgentError::platform("Poll is not active"));
            }

            // Check expiration
            if let Some(expires) = poll.expires_at {
                if chrono::Utc::now() > expires {
                    poll.status = PollStatus::Closed;
                    return Err(AgentError::platform("Poll has expired"));
                }
            }

            // Validate options
            let valid_option_ids: std::collections::HashSet<String> =
                poll.options.iter().map(|o| o.id.clone()).collect();

            for option_id in &option_ids {
                if !valid_option_ids.contains(option_id) {
                    return Err(AgentError::platform(format!(
                        "Invalid option: {}",
                        option_id
                    )));
                }
            }

            // Check vote limits
            if !poll.config.allow_multiple && option_ids.len() > 1 {
                return Err(AgentError::platform(
                    "Multiple selections not allowed".to_string(),
                ));
            }

            _allow_multiple = poll.config.allow_multiple;
            allow_change = poll.config.allow_change;
        }

        // Update results
        {
            let mut results = self.results.write().await;
            let result = results
                .get_mut(&poll_id)
                .ok_or_else(|| AgentError::not_found("Poll results not found"))?;

            // Check if user already voted and if changes are allowed
            let existing_vote = result.votes.iter().find(|v| v.user_id == user_id);

            if let Some(existing) = existing_vote {
                if !allow_change {
                    return Err(AgentError::platform(
                        "Changing votes not allowed".to_string(),
                    ));
                }

                // Remove old votes from count
                for option_id in &existing.option_ids {
                    if let Some(count) = result.option_votes.get_mut(option_id) {
                        *count = count.saturating_sub(1);
                    }
                }

                // Remove old vote
                result.votes.retain(|v| v.user_id != user_id);
                result.unique_voters = result.unique_voters.saturating_sub(1);
            }

            // Add new votes
            for option_id in &option_ids {
                if let Some(count) = result.option_votes.get_mut(option_id) {
                    *count += 1;
                }
            }

            result.votes.push(vote);
            result.total_votes += 1;
            result.unique_voters += 1;

            // Update winners
            let max_votes = result.option_votes.values().max().copied().unwrap_or(0);
            if max_votes > 0 {
                result.winners = result
                    .option_votes
                    .iter()
                    .filter(|(_, &v)| v == max_votes)
                    .map(|(k, _)| k.clone())
                    .collect();
            }
        }

        debug!("Vote cast for poll: {}", poll_id);
        Ok(())
    }

    /// Get poll by ID
    pub async fn get_poll(&self, poll_id: &str) -> Option<Poll> {
        let polls = self.polls.read().await;
        polls.get(poll_id).cloned()
    }

    /// Get poll results
    pub async fn get_results(&self, poll_id: &str) -> Option<PollResult> {
        let results = self.results.read().await;
        results.get(poll_id).cloned()
    }

    /// Close a poll
    pub async fn close_poll(&self, poll_id: &str) -> Result<()> {
        let mut polls = self.polls.write().await;
        let poll = polls
            .get_mut(poll_id)
            .ok_or_else(|| AgentError::not_found(format!("Poll {} not found", poll_id)))?;

        poll.status = PollStatus::Closed;
        info!("Poll {} closed", poll_id);
        Ok(())
    }

    /// Cancel a poll
    pub async fn cancel_poll(&self, poll_id: &str) -> Result<()> {
        let mut polls = self.polls.write().await;
        let poll = polls
            .get_mut(poll_id)
            .ok_or_else(|| AgentError::not_found(format!("Poll {} not found", poll_id)))?;

        poll.status = PollStatus::Cancelled;
        info!("Poll {} cancelled", poll_id);
        Ok(())
    }

    /// List active polls
    pub async fn list_active_polls(&self) -> Vec<Poll> {
        let polls = self.polls.read().await;
        polls
            .values()
            .filter(|p| {
                p.status == PollStatus::Active
                    && p.expires_at.map_or(true, |e| chrono::Utc::now() < e)
            })
            .cloned()
            .collect()
    }

    /// List polls in a channel
    pub async fn list_channel_polls(&self, channel_id: &str) -> Vec<Poll> {
        let polls = self.polls.read().await;
        polls
            .values()
            .filter(|p| p.channel_id == channel_id)
            .cloned()
            .collect()
    }

    /// Delete a poll
    pub async fn delete_poll(&self, poll_id: &str) -> Result<()> {
        {
            let mut polls = self.polls.write().await;
            polls.remove(poll_id);
        }
        {
            let mut results = self.results.write().await;
            results.remove(poll_id);
        }
        info!("Poll {} deleted", poll_id);
        Ok(())
    }

    /// Check and close expired polls
    pub async fn check_expired_polls(&self) -> Vec<String> {
        let mut closed = Vec::new();
        let now = chrono::Utc::now();

        let mut polls = self.polls.write().await;
        for (id, poll) in polls.iter_mut() {
            if poll.status == PollStatus::Active {
                if let Some(expires) = poll.expires_at {
                    if now > expires {
                        poll.status = PollStatus::Closed;
                        closed.push(id.clone());
                        info!("Poll {} auto-closed (expired)", id);
                    }
                }
            }
        }

        closed
    }

    /// Get formatter for platform
    pub fn get_formatter(&self, platform: PlatformType) -> Box<dyn PollFormatter> {
        match platform {
            PlatformType::Discord => Box::new(DiscordPollFormatter),
            PlatformType::WhatsApp => Box::new(WhatsAppPollFormatter),
            PlatformType::Telegram => Box::new(TelegramPollFormatter),
            PlatformType::Teams => Box::new(TeamsPollFormatter),
            PlatformType::Line => Box::new(LinePollFormatter),
            _ => Box::new(DefaultPollFormatter),
        }
    }

    /// Convert number to emoji
    fn number_to_emoji(n: usize) -> Option<String> {
        let emojis = ["1️⃣", "2️⃣", "3️⃣", "4️⃣", "5️⃣", "6️⃣", "7️⃣", "8️⃣", "9️⃣", "🔟"];
        emojis.get(n - 1).map(|e| e.to_string())
    }
}

impl Default for PollManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Default poll formatter
pub struct DefaultPollFormatter;

impl PollFormatter for DefaultPollFormatter {
    fn format_poll(&self, poll: &Poll, results: Option<&PollResult>) -> String {
        let mut output = format!("📊 **{}**\n\n", poll.question);

        for option in &poll.options {
            let emoji = option.emoji.as_deref().unwrap_or("•");
            if let Some(result) = results {
                let votes = result.option_votes.get(&option.id).copied().unwrap_or(0);
                let percentage = if result.total_votes > 0 {
                    (votes as f32 / result.total_votes as f32 * 100.0) as u32
                } else {
                    0
                };
                let bar = "█".repeat((percentage / 10) as usize);
                output.push_str(&format!(
                    "{} {} - {} votes ({}%) {}\n",
                    emoji, option.text, votes, percentage, bar
                ));
            } else {
                output.push_str(&format!("{} {}\n", emoji, option.text));
            }
        }

        if let Some(result) = results {
            output.push_str(&format!(
                "\nTotal votes: {} | Voters: {}",
                result.total_votes, result.unique_voters
            ));
        }

        output
    }

    fn format_interactive(&self, poll: &Poll) -> serde_json::Value {
        serde_json::json!({
            "text": self.format_poll(poll, None),
            "poll_id": poll.id
        })
    }

    fn parse_vote(&self, message: &Message) -> Option<VoteIntent> {
        // Simple parsing: look for "vote <number>" or just a number
        let content = message.content.trim();

        // Try to parse as just a number
        if let Ok(num) = content.parse::<usize>() {
            return Some(VoteIntent {
                poll_id: String::new(), // Need to determine from context
                option_id: num.to_string(),
                user_id: String::new(), // Need to get from message metadata
            });
        }

        // Try "vote <number>" pattern
        if content.to_lowercase().starts_with("vote ") {
            if let Ok(num) = content[5..].trim().parse::<usize>() {
                return Some(VoteIntent {
                    poll_id: String::new(),
                    option_id: num.to_string(),
                    user_id: String::new(),
                });
            }
        }

        None
    }
}

/// Discord poll formatter
pub struct DiscordPollFormatter;

impl PollFormatter for DiscordPollFormatter {
    fn format_poll(&self, poll: &Poll, results: Option<&PollResult>) -> String {
        DefaultPollFormatter.format_poll(poll, results)
    }

    fn format_interactive(&self, poll: &Poll) -> serde_json::Value {
        // Discord uses embeds and reactions
        let mut fields = Vec::new();

        for option in &poll.options {
            let emoji = option.emoji.as_deref().unwrap_or("🔘");
            fields.push(serde_json::json!({
                "name": format!("{} {}", emoji, option.text),
                "value": format!("React with {} to vote", option.id),
                "inline": false
            }));
        }

        serde_json::json!({
            "embeds": [{
                "title": format!("📊 {}", poll.question),
                "fields": fields,
                "footer": {
                    "text": format!("Poll ID: {}", poll.id)
                }
            }]
        })
    }

    fn parse_vote(&self, _message: &Message) -> Option<VoteIntent> {
        // Discord uses reactions, not text messages
        None
    }
}

/// WhatsApp poll formatter
pub struct WhatsAppPollFormatter;

impl PollFormatter for WhatsAppPollFormatter {
    fn format_poll(&self, poll: &Poll, results: Option<&PollResult>) -> String {
        // WhatsApp uses simple text format
        let mut output = format!("📊 *{}*\n\n", poll.question);

        for option in &poll.options {
            if let Some(result) = results {
                let votes = result.option_votes.get(&option.id).copied().unwrap_or(0);
                output.push_str(&format!(
                    "{} {} - {} votes\n",
                    option.id, option.text, votes
                ));
            } else {
                output.push_str(&format!("{} {}\n", option.id, option.text));
            }
        }

        output.push_str("\nReply with the option number to vote!");
        output
    }

    fn format_interactive(&self, poll: &Poll) -> serde_json::Value {
        // WhatsApp supports interactive messages with buttons
        let options: Vec<_> = poll
            .options
            .iter()
            .map(|o| {
                serde_json::json!({
                    "type": "reply",
                    "reply": {
                        "id": o.id,
                        "title": format!("{} {}", o.emoji.as_deref().unwrap_or(""), o.text)
                    }
                })
            })
            .collect();

        serde_json::json!({
            "messaging_product": "whatsapp",
            "recipient_type": "individual",
            "type": "interactive",
            "interactive": {
                "type": "button",
                "body": {
                    "text": poll.question
                },
                "action": {
                    "buttons": options
                }
            }
        })
    }

    fn parse_vote(&self, message: &Message) -> Option<VoteIntent> {
        DefaultPollFormatter.parse_vote(message)
    }
}

/// Telegram poll formatter
pub struct TelegramPollFormatter;

impl PollFormatter for TelegramPollFormatter {
    fn format_poll(&self, poll: &Poll, results: Option<&PollResult>) -> String {
        DefaultPollFormatter.format_poll(poll, results)
    }

    fn format_interactive(&self, _poll: &Poll) -> serde_json::Value {
        // Telegram has native poll support, handled separately
        serde_json::json!({})
    }

    fn parse_vote(&self, message: &Message) -> Option<VoteIntent> {
        DefaultPollFormatter.parse_vote(message)
    }
}

/// Teams poll formatter
pub struct TeamsPollFormatter;

impl PollFormatter for TeamsPollFormatter {
    fn format_poll(&self, poll: &Poll, results: Option<&PollResult>) -> String {
        DefaultPollFormatter.format_poll(poll, results)
    }

    fn format_interactive(&self, poll: &Poll) -> serde_json::Value {
        // Teams uses Adaptive Cards
        let actions: Vec<_> = poll
            .options
            .iter()
            .map(|o| {
                serde_json::json!({
                    "type": "Action.Submit",
                    "title": format!("{} {}", o.emoji.as_deref().unwrap_or(""), o.text),
                    "data": {
                        "poll_id": poll.id,
                        "option_id": o.id
                    }
                })
            })
            .collect();

        serde_json::json!({
            "type": "AdaptiveCard",
            "version": "1.3",
            "body": [
                {
                    "type": "TextBlock",
                    "text": poll.question,
                    "weight": "Bolder",
                    "size": "Medium"
                }
            ],
            "actions": actions
        })
    }

    fn parse_vote(&self, _message: &Message) -> Option<VoteIntent> {
        // Teams uses Adaptive Card submissions, message parameter unused
        let _ = _message;
        None
    }
}

/// LINE poll formatter
pub struct LinePollFormatter;

impl PollFormatter for LinePollFormatter {
    fn format_poll(&self, poll: &Poll, results: Option<&PollResult>) -> String {
        DefaultPollFormatter.format_poll(poll, results)
    }

    fn format_interactive(&self, poll: &Poll) -> serde_json::Value {
        // LINE uses template messages
        let actions: Vec<_> = poll
            .options
            .iter()
            .map(|o| {
                serde_json::json!({
                    "type": "message",
                    "label": format!("{} {}", o.emoji.as_deref().unwrap_or(""), o.text),
                    "text": o.id.clone()
                })
            })
            .collect();

        serde_json::json!({
            "type": "template",
            "altText": poll.question,
            "template": {
                "type": "buttons",
                "text": poll.question,
                "actions": actions
            }
        })
    }

    fn parse_vote(&self, message: &Message) -> Option<VoteIntent> {
        DefaultPollFormatter.parse_vote(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_poll_creation() {
        let manager = PollManager::new();
        let poll = manager
            .create_poll(
                "Favorite color?",
                vec!["Red".to_string(), "Blue".to_string(), "Green".to_string()],
                PlatformType::Discord,
                "channel-1",
                "user-1",
                PollConfig::default(),
                Some(60),
            )
            .await
            .unwrap();

        assert_eq!(poll.question, "Favorite color?");
        assert_eq!(poll.options.len(), 3);
        assert_eq!(poll.status, PollStatus::Active);
    }

    #[tokio::test]
    async fn test_voting() {
        let manager = PollManager::new();
        let poll = manager
            .create_poll(
                "Favorite color?",
                vec!["Red".to_string(), "Blue".to_string()],
                PlatformType::Discord,
                "channel-1",
                "user-1",
                PollConfig::default(),
                None,
            )
            .await
            .unwrap();

        let vote = Vote {
            poll_id: poll.id.clone(),
            user_id: "voter-1".to_string(),
            option_ids: vec!["1".to_string()],
            timestamp: chrono::Utc::now(),
        };

        manager.cast_vote(vote).await.unwrap();

        let results = manager.get_results(&poll.id).await.unwrap();
        assert_eq!(results.total_votes, 1);
        assert_eq!(results.option_votes.get("1").copied().unwrap_or(0), 1);
    }

    #[test]
    fn test_poll_formatting() {
        let formatter = DefaultPollFormatter;
        let poll = Poll {
            id: "test".to_string(),
            question: "Test?".to_string(),
            options: vec![
                PollOption {
                    id: "1".to_string(),
                    text: "Option 1".to_string(),
                    emoji: Some("1️⃣".to_string()),
                },
                PollOption {
                    id: "2".to_string(),
                    text: "Option 2".to_string(),
                    emoji: Some("2️⃣".to_string()),
                },
            ],
            platform: PlatformType::Discord,
            channel_id: "test".to_string(),
            creator_id: "test".to_string(),
            config: PollConfig::default(),
            status: PollStatus::Active,
            created_at: chrono::Utc::now(),
            expires_at: None,
            platform_message_id: None,
        };

        let formatted = formatter.format_poll(&poll, None);
        assert!(formatted.contains("Test?"));
        assert!(formatted.contains("Option 1"));
    }
}
