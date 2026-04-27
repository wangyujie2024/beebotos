//! Message Router
//!
//! Routes messages between agents with rate limiting and delivery guarantees.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::mpsc;
use tracing::{debug, trace, warn};

use crate::error::{KernelError, Result};

/// Message envelope for inter-agent communication
#[derive(Debug, Clone)]
pub struct MessageEnvelope {
    /// Source agent ID
    pub source: String,
    /// Destination agent ID
    pub destination: String,
    /// Message payload
    pub payload: Vec<u8>,
    /// Message timestamp (Unix milliseconds)
    pub timestamp: u64,
    /// Message priority (0-255, lower is higher priority)
    pub priority: u8,
    /// Delivery timeout (milliseconds)
    pub timeout_ms: u64,
}

/// Rate limiter for message sending
#[derive(Debug)]
pub struct RateLimiter {
    /// Maximum messages per window
    max_messages: u32,
    /// Window size in milliseconds
    window_ms: u64,
    /// Message timestamps in current window
    timestamps: VecDeque<u64>,
}

impl RateLimiter {
    /// Create new rate limiter
    pub fn new(max_messages: u32, window_ms: u64) -> Self {
        Self {
            max_messages,
            window_ms,
            timestamps: VecDeque::new(),
        }
    }

    /// Check if a message can be sent
    pub fn allow(&mut self) -> bool {
        let now = chrono::Utc::now().timestamp_millis() as u64;

        // Remove timestamps outside the window
        while let Some(&oldest) = self.timestamps.front() {
            if now - oldest > self.window_ms {
                self.timestamps.pop_front();
            } else {
                break;
            }
        }

        // Check if we can send
        if self.timestamps.len() < self.max_messages as usize {
            self.timestamps.push_back(now);
            true
        } else {
            false
        }
    }

    /// Get current message count in window
    pub fn current_count(&self) -> usize {
        self.timestamps.len()
    }

    /// Get remaining quota
    pub fn remaining(&self) -> i32 {
        self.max_messages as i32 - self.timestamps.len() as i32
    }
}

/// Message queue for an agent
#[derive(Debug)]
pub struct AgentMailbox {
    /// Agent ID
    agent_id: String,
    /// Message sender channel
    sender: mpsc::UnboundedSender<MessageEnvelope>,
    /// Rate limiter for outgoing messages
    rate_limiter: Mutex<RateLimiter>,
    /// Message statistics
    stats: Mutex<MailboxStats>,
}

/// Mailbox statistics
#[derive(Debug, Default, Clone)]
pub struct MailboxStats {
    /// Messages sent from this mailbox
    pub messages_sent: u64,
    /// Messages received by this mailbox
    pub messages_received: u64,
    /// Messages dropped due to errors
    pub messages_dropped: u64,
    /// Bytes sent from this mailbox
    pub bytes_sent: u64,
    /// Bytes received by this mailbox
    pub bytes_received: u64,
}

impl AgentMailbox {
    /// Create new mailbox
    pub fn new(agent_id: String) -> (Self, mpsc::UnboundedReceiver<MessageEnvelope>) {
        let (sender, receiver) = mpsc::unbounded_channel();

        let mailbox = Self {
            agent_id,
            sender,
            rate_limiter: Mutex::new(RateLimiter::new(1000, 1000)), // 1000 msg/sec
            stats: Mutex::new(MailboxStats::default()),
        };

        (mailbox, receiver)
    }

    /// Send message to this agent
    pub fn deliver(&self, message: MessageEnvelope) -> Result<()> {
        let mut stats = self.stats.lock();
        stats.messages_received += 1;
        stats.bytes_received += message.payload.len() as u64;
        drop(stats);

        self.sender
            .send(message)
            .map_err(|_| KernelError::internal("Mailbox closed"))
    }

    /// Check if can send message (rate limit)
    pub fn can_send(&self) -> bool {
        self.rate_limiter.lock().allow()
    }

    /// Record sent message
    pub fn record_sent(&self, bytes: usize) {
        let mut stats = self.stats.lock();
        stats.messages_sent += 1;
        stats.bytes_sent += bytes as u64;
    }

    /// Get statistics
    pub fn stats(&self) -> MailboxStats {
        self.stats.lock().clone()
    }
}

impl Clone for AgentMailbox {
    fn clone(&self) -> Self {
        Self {
            agent_id: self.agent_id.clone(),
            sender: self.sender.clone(),
            rate_limiter: Mutex::new(RateLimiter::new(1000, 1000)),
            stats: Mutex::new(MailboxStats::default()),
        }
    }
}

/// Message router for inter-agent communication
#[derive(Debug)]
pub struct MessageRouter {
    /// Registered agent mailboxes
    mailboxes: Mutex<HashMap<String, AgentMailbox>>,
    /// Global message statistics
    global_stats: Mutex<RouterStats>,
    /// Default rate limit settings - reserved for future rate limiting
    /// implementation
    #[allow(dead_code)]
    _default_rate_limit: (u32, u64), // (max_messages, window_ms)
}

/// Router statistics
#[derive(Debug, Default, Clone)]
pub struct RouterStats {
    /// Total messages routed through this router
    pub total_messages_routed: u64,
    /// Total messages dropped due to errors
    pub total_messages_dropped: u64,
    /// Total bytes routed through this router
    pub total_bytes_routed: u64,
    /// Number of active mailboxes
    pub active_mailboxes: usize,
}

impl MessageRouter {
    /// Create new message router
    pub fn new() -> Self {
        Self {
            mailboxes: Mutex::new(HashMap::new()),
            global_stats: Mutex::new(RouterStats::default()),
            _default_rate_limit: (1000, 1000), // 1000 msg/sec
        }
    }

    /// Register an agent with the router
    pub fn register_agent(&self, agent_id: String) -> mpsc::UnboundedReceiver<MessageEnvelope> {
        let agent_id_clone = agent_id.clone();
        let (mailbox, receiver) = AgentMailbox::new(agent_id);

        let mut mailboxes = self.mailboxes.lock();
        mailboxes.insert(agent_id_clone.clone(), mailbox);

        let mut stats = self.global_stats.lock();
        stats.active_mailboxes = mailboxes.len();

        debug!("Registered agent {} with message router", agent_id_clone);
        receiver
    }

    /// Unregister an agent
    pub fn unregister_agent(&self, agent_id: &str) {
        let mut mailboxes = self.mailboxes.lock();
        mailboxes.remove(agent_id);

        let mut stats = self.global_stats.lock();
        stats.active_mailboxes = mailboxes.len();

        debug!("Unregistered agent {} from message router", agent_id);
    }

    /// Route a message to destination agent
    pub fn route(&self, message: MessageEnvelope) -> Result<()> {
        let mailboxes = self.mailboxes.lock();

        // Check rate limit for source
        if let Some(source_mailbox) = mailboxes.get(&message.source) {
            if !source_mailbox.can_send() {
                warn!("Rate limit exceeded for agent {}", message.source);
                return Err(KernelError::resource_exhausted("Rate limit exceeded"));
            }
        } else {
            return Err(KernelError::invalid_argument("Source agent not registered"));
        }

        // Route to destination
        match mailboxes.get(&message.destination) {
            Some(dest_mailbox) => {
                dest_mailbox.deliver(message.clone())?;

                // Update statistics
                let mut global_stats = self.global_stats.lock();
                global_stats.total_messages_routed += 1;
                global_stats.total_bytes_routed += message.payload.len() as u64;

                if let Some(source) = mailboxes.get(&message.source) {
                    source.record_sent(message.payload.len());
                }

                trace!(
                    "Routed message from {} to {}",
                    message.source,
                    message.destination
                );
                Ok(())
            }
            None => {
                warn!("Destination agent {} not found", message.destination);
                Err(KernelError::AgentNotFound(message.destination))
            }
        }
    }

    /// Get mailbox for an agent
    pub fn get_mailbox(&self, agent_id: &str) -> Option<AgentMailbox> {
        self.mailboxes.lock().get(agent_id).cloned()
    }

    /// Get global statistics
    pub fn stats(&self) -> RouterStats {
        self.global_stats.lock().clone()
    }

    /// Get agent statistics
    pub fn agent_stats(&self, agent_id: &str) -> Option<MailboxStats> {
        self.mailboxes.lock().get(agent_id).map(|m| m.stats())
    }

    /// List registered agents
    pub fn list_agents(&self) -> Vec<String> {
        self.mailboxes.lock().keys().cloned().collect()
    }
}

impl Default for MessageRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// Global message router instance
static MESSAGE_ROUTER: std::sync::OnceLock<Arc<MessageRouter>> = std::sync::OnceLock::new();

/// Get global message router
pub fn global_router() -> Arc<MessageRouter> {
    MESSAGE_ROUTER
        .get_or_init(|| Arc::new(MessageRouter::new()))
        .clone()
}

/// Initialize message router
pub fn init() -> Result<()> {
    let _ = global_router();
    tracing::info!("Message router initialized");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter() {
        let mut limiter = RateLimiter::new(5, 1000);

        // Should allow 5 messages
        for _ in 0..5 {
            assert!(limiter.allow());
        }

        // Should deny the 6th
        assert!(!limiter.allow());
    }

    #[test]
    fn test_message_router() {
        let router = MessageRouter::new();

        // Register two agents
        let _rx1 = router.register_agent("agent1".to_string());
        let _rx2 = router.register_agent("agent2".to_string());

        // Send message
        let message = MessageEnvelope {
            source: "agent1".to_string(),
            destination: "agent2".to_string(),
            payload: vec![1, 2, 3],
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            priority: 0,
            timeout_ms: 5000,
        };

        assert!(router.route(message).is_ok());

        // Check stats
        let stats = router.stats();
        assert_eq!(stats.total_messages_routed, 1);
        assert_eq!(stats.active_mailboxes, 2);
    }

    #[test]
    fn test_unregistered_destination() {
        let router = MessageRouter::new();

        // Register only source
        let _rx1 = router.register_agent("agent1".to_string());

        // Try to send to unregistered agent
        let message = MessageEnvelope {
            source: "agent1".to_string(),
            destination: "agent2".to_string(),
            payload: vec![1, 2, 3],
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            priority: 0,
            timeout_ms: 5000,
        };

        assert!(router.route(message).is_err());
    }
}
