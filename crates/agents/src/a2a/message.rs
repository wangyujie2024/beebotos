//! A2A Message definitions

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::AgentId;

/// A2A message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2AMessage {
    pub id: String,
    pub msg_type: MessageType,
    pub priority: MessagePriority,
    pub from: AgentId,
    pub to: Option<AgentId>, // None = broadcast
    pub payload: MessagePayload,
    /// 🟡 MEDIUM FIX: Use DateTime<Utc> instead of u64
    #[serde(with = "chrono::serde::ts_seconds")]
    pub timestamp: DateTime<Utc>,
    pub ttl: Option<u64>, // Time to live
    pub signature: Option<Vec<u8>>,
}

impl A2AMessage {
    /// Create new message
    pub fn new(
        msg_type: MessageType,
        from: AgentId,
        to: Option<AgentId>,
        payload: MessagePayload,
    ) -> Self {
        Self {
            id: uuid(),
            msg_type,
            priority: MessagePriority::Normal,
            from,
            to,
            payload,
            timestamp: Utc::now(),
            ttl: None,
            signature: None,
        }
    }

    /// Set priority
    pub fn with_priority(mut self, priority: MessagePriority) -> Self {
        self.priority = priority;
        self
    }

    /// Set TTL
    pub fn with_ttl(mut self, ttl: u64) -> Self {
        self.ttl = Some(ttl);
        self
    }

    /// Check if expired
    pub fn is_expired(&self) -> bool {
        self.ttl.map_or(false, |ttl| {
            let elapsed = Utc::now()
                .signed_duration_since(self.timestamp)
                .num_seconds() as u64;
            elapsed > ttl
        })
    }

    /// Sign message
    pub fn sign(mut self, signature: impl Into<Vec<u8>>) -> Self {
        self.signature = Some(signature.into());
        self
    }
}

/// Message type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    Ping,
    Pong,
    Request,
    Response,
    Event,
    Task,
    TaskResult,
    Negotiate,
    NegotiateResponse,
    Handshake,
    Error,
}

/// Message priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessagePriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

impl MessagePriority {
    /// Get as number
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }
}

/// Message payload
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessagePayload {
    Ping {
        nonce: u64,
    },
    Pong {
        nonce: u64,
    },
    Request {
        action: String,
        params: HashMap<String, serde_json::Value>,
    },
    Response {
        success: bool,
        data: Option<serde_json::Value>,
        error: Option<String>,
    },
    Event {
        event_type: String,
        data: serde_json::Value,
    },
    Task {
        task_id: String,
        description: String,
        requirements: Vec<String>,
        reward: Option<u64>,
    },
    TaskResult {
        task_id: String,
        result: serde_json::Value,
    },
    Negotiate {
        offer: NegotiationOffer,
    },
    NegotiateResponse {
        accepted: bool,
        counter_offer: Option<NegotiationOffer>,
    },
    Handshake {
        capabilities: Vec<String>,
        public_key: String,
    },
    Error {
        code: u16,
        message: String,
    },
    /// Encrypted payload for end-to-end encryption
    ///
    /// SECURITY FIX: Contains encrypted data that can only be decrypted by the
    /// recipient
    Encrypted(super::security::EncryptedMessage),
    /// Plain binary data
    Plain(Vec<u8>),
}

/// Negotiation offer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegotiationOffer {
    pub service: String,
    pub price: u64,
    pub terms: Vec<String>,
    pub valid_until: u64,
}

/// Message envelope for transport
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEnvelope {
    pub version: String,
    pub message: A2AMessage,
    pub routing: RoutingInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingInfo {
    pub hops: Vec<String>,
    pub encryption: Option<String>,
}

fn uuid() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 16] = rng.gen();
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:\
         02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}
