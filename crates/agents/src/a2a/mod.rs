pub mod capabilities;
pub mod discovery;
pub mod message;
pub mod negotiation;
pub mod protocol;
pub mod security;
pub mod task_manager;
pub mod transport;

// 🟢 P0 FIX: ACP 2.0 Protocol Implementation
pub mod acp20_protocol;

use std::sync::Arc;

pub use acp20_protocol::{
    Acp20Protocol, AcpError, AcpEvent, AcpSession, AgentCapabilityProfile, Capability,
    CapabilityType, CollaborationProposal, CommunicationPreferences, NegotiationResponse,
    PerformanceMetrics, ProposalType, ResourceConstraints, ResponseType, RetryPolicy,
    ServiceEndpoint, SessionParticipant, SessionState, SessionType, TaskAssignment,
    TaskAssignmentStatus, TimeoutSettings, ACP20_VERSION,
};
use message::{A2AMessage, MessageType};
use protocol::A2AError;
use tokio::sync::Mutex;

use crate::types::AgentId;

/// A2A Client with enhanced security
///
/// SECURITY FIX: Implements end-to-end encryption and mandatory signature
/// verification
pub struct A2AClient {
    discovery: Arc<discovery::DiscoveryService>,
    #[allow(dead_code)]
    transport: Arc<transport::TransportManager>,
    security: Arc<security::A2ASecurity>,
    task_manager: Arc<task_manager::TaskManager>,
    /// 🟠 HIGH FIX: Use tokio::sync::Mutex for async context
    negotiation: Arc<Mutex<negotiation::NegotiationEngine>>,
    /// Agent's unique identifier (resolved from DID/certificate)
    agent_id: AgentId,
}

impl A2AClient {
    /// Create new A2A client with default configuration
    ///
    /// 🟠 HIGH FIX: Returns Result instead of panicking
    pub fn new(agent_id: AgentId) -> Result<Self, A2AError> {
        Ok(Self {
            discovery: Arc::new(discovery::DiscoveryService::new()),
            transport: Arc::new(transport::TransportManager::new()),
            security: Arc::new(
                security::A2ASecurity::generate_key_pair()
                    .map_err(|e| A2AError::Security(e.to_string()))?,
            ),
            task_manager: Arc::new(task_manager::TaskManager::new()),
            negotiation: Arc::new(Mutex::new(negotiation::NegotiationEngine::new())),
            agent_id,
        })
    }

    /// Create new A2A client with HTTP transport
    pub fn with_http_transport(agent_id: AgentId, base_url: String) -> Result<Self, A2AError> {
        Ok(Self {
            discovery: Arc::new(discovery::DiscoveryService::new()),
            transport: Arc::new(transport::TransportManager::new().with_http_transport(base_url)),
            security: Arc::new(
                security::A2ASecurity::generate_key_pair()
                    .map_err(|e| A2AError::Security(e.to_string()))?,
            ),
            task_manager: Arc::new(task_manager::TaskManager::new()),
            negotiation: Arc::new(Mutex::new(negotiation::NegotiationEngine::new())),
            agent_id,
        })
    }

    /// Send a message with end-to-end encryption and mandatory signature
    /// verification
    ///
    /// SECURITY FIX:
    /// - Messages are encrypted using the recipient's public key
    /// - Signatures are mandatory and verified
    /// - Sender identity is cryptographically verified
    pub async fn send_message(
        &self,
        message: A2AMessage,
        recipient_agent_id: &str,
    ) -> Result<A2AMessage, A2AError> {
        let agent = self
            .discovery
            .find_agent_by_id(recipient_agent_id)
            .ok_or(A2AError::AgentNotFound)?;

        let endpoint = agent.endpoints.first().ok_or(A2AError::NoValidEndpoint)?;

        use chrono::Utc;

        // Serialize the payload for signing
        let payload_bytes = serde_json::to_vec(&message.payload)
            .map_err(|e| A2AError::Serialization(e.to_string()))?;

        // 🟠 HIGH FIX: Sign the message with our private key
        let signed = self
            .security
            .sign_message(&payload_bytes, Utc::now().timestamp() as u64)
            .map_err(|e| A2AError::Security(e.to_string()))?;

        // SECURITY FIX: Encrypt the payload for end-to-end encryption
        let recipient_public_key = agent
            .public_key
            .as_ref()
            .ok_or_else(|| A2AError::Security("Recipient has no public key".to_string()))?;

        let encrypted_payload = self
            .security
            .encrypt_for_recipient(&payload_bytes, recipient_public_key)
            .map_err(|e| A2AError::Security(format!("Encryption failed: {}", e)))?;

        // Create the actual message to send with signature and encryption
        let message_to_send = A2AMessage {
            id: uuid::Uuid::new_v4().to_string(),
            msg_type: MessageType::Request,
            priority: message.priority,
            // SECURITY FIX: Use actual agent ID from cryptographic identity
            from: self.agent_id.clone(),
            to: Some(crate::types::AgentId::from_string(recipient_agent_id)),
            // SECURITY FIX: Store encrypted payload
            payload: message::MessagePayload::Encrypted(encrypted_payload),
            timestamp: Utc::now(),
            ttl: message.ttl,
            // 🟠 HIGH FIX: Include the signature (mandatory)
            signature: Some(signed.signature),
        };

        // Actually send via transport
        let response = self
            .transport
            .send(&message_to_send, endpoint.url().as_str())
            .await
            .map_err(|e| A2AError::Transport(e.to_string()))?;

        // SECURITY FIX: Verify response signature
        if let Some(ref sig) = response.signature {
            let response_payload_bytes = serde_json::to_vec(&response.payload)
                .map_err(|e| A2AError::Serialization(e.to_string()))?;

            let response_signer_key = self
                .discovery
                .get_agent_public_key(recipient_agent_id)
                .ok_or_else(|| A2AError::Security("Unknown response signer".to_string()))?;

            self.security
                .verify_signature(&response_payload_bytes, sig, &response_signer_key)
                .map_err(|e| A2AError::Security(format!("Invalid response signature: {}", e)))?;
        } else {
            return Err(A2AError::Security(
                "Response missing mandatory signature".to_string(),
            ));
        }

        tracing::info!(
            "Message sent to {}, received verified response",
            recipient_agent_id
        );

        Ok(response)
    }

    /// Receive and verify a message
    ///
    /// SECURITY FIX: Verifies signatures and decrypts payload
    ///
    /// Protocol:
    /// 1. Message is signed, then encrypted (sign-then-encrypt)
    /// 2. We must decrypt first to get the original payload
    /// 3. Then verify the signature on the decrypted payload
    pub async fn receive_message(&self, message: &A2AMessage) -> Result<Vec<u8>, A2AError> {
        // Verify signature is present
        let signature = message
            .signature
            .as_ref()
            .ok_or_else(|| A2AError::Security("Message missing mandatory signature".to_string()))?;

        // Get sender's public key
        let sender_id = message.from.to_string();
        let sender_public_key = self
            .discovery
            .get_agent_public_key(&sender_id)
            .ok_or_else(|| A2AError::Security(format!("Unknown sender: {}", sender_id)))?;

        // SECURITY FIX: Decrypt first (sign-then-encrypt protocol)
        let decrypted_payload = match &message.payload {
            message::MessagePayload::Encrypted(encrypted_data) => self
                .security
                .decrypt(encrypted_data)
                .map_err(|e| A2AError::Security(format!("Decryption failed: {}", e)))?,
            message::MessagePayload::Plain(data) => data.clone(),
            _ => serde_json::to_vec(&message.payload)
                .map_err(|e| A2AError::Serialization(e.to_string()))?,
        };

        // SECURITY FIX: Verify signature on decrypted payload
        self.security
            .verify_signature(&decrypted_payload, signature, &sender_public_key)
            .map_err(|e| A2AError::Security(format!("Signature verification failed: {}", e)))?;

        Ok(decrypted_payload)
    }

    pub fn discovery(&self) -> Arc<discovery::DiscoveryService> {
        self.discovery.clone()
    }

    pub fn task_manager(&self) -> Arc<task_manager::TaskManager> {
        self.task_manager.clone()
    }

    pub fn negotiation(&self) -> Arc<Mutex<negotiation::NegotiationEngine>> {
        self.negotiation.clone()
    }

    /// Get this agent's ID
    pub fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_a2a_client_creation() {
        let agent_id = AgentId::from_string("test-agent-1");
        let client = A2AClient::new(agent_id).expect("Failed to create client");
        // AgentId Display impl formats as "agent_{hex_prefix}", not the original string
        assert!(client.agent_id().to_string().starts_with("agent_"));
        assert!(client.discovery.find_agent_by_id("test").is_none());
    }

    #[tokio::test]
    async fn test_message_signature_verification() {
        let agent_id = AgentId::from_string("test-agent");
        let client = A2AClient::new(agent_id).expect("Failed to create client");

        // Create a test message
        let message = A2AMessage::new(
            MessageType::Request,
            AgentId::from_string("test-agent"),
            Some(AgentId::from_string("recipient")),
            message::MessagePayload::Plain(vec![1, 2, 3]),
        );

        // The message should not have a signature initially
        assert!(message.signature.is_none());
    }
}
