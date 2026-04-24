//! Network Module
//!
//! Provides peer-to-peer networking capabilities for agent communication
//! and distributed consensus.
//!
//! ## Submodules
//!
//! - `p2p`: Peer-to-peer networking and protocol handling with:
//!   - Protocol versioning and negotiation
//!   - Connection health monitoring
//!   - Keepalive and auto-reconnect
//!   - Gossip protocol support
//!   - Protocol handler registration
//!
//! - `discovery`: DHT-based peer discovery with:
//!   - Kademlia-style K-buckets
//!   - XOR distance-based routing
//!   - Bootstrap node support
//!   - mDNS local discovery
//!   - Peer reputation tracking
//!
//! - `transport`: Network transport layer abstractions
//!
//! - `connection`: Connection pool management with:
//!   - Connection pooling with configurable limits
//!   - Load balancing strategies
//!   - Connection health monitoring
//!   - Connection multiplexing
//!
//! ## Example Usage
//!
//! ```rust,no_run
//! use beebotos_kernel::network::{
//!     ConnectionPoolConfig, ConnectionPoolManager, DiscoveryMethod, DiscoveryService,
//!     NetworkConfig, P2PNode,
//! };
//!
//! async fn setup_network() {
//!     // Create and start P2P node
//!     let config = NetworkConfig::default();
//!     let node = P2PNode::new(config);
//!     let addr = node.start().await.unwrap();
//!
//!     // Create discovery service
//!     let discovery = DiscoveryService::new(node.local_id(), DiscoveryMethod::DHT);
//!     discovery.start().await.unwrap();
//! }
//! ```

pub mod connection;
pub mod discovery;
pub mod p2p;
pub mod transport;

// Re-export main types
use std::collections::HashMap;
use std::sync::Arc;

pub use connection::{
    measure_latency, ConnectionConfig, ConnectionManager, ConnectionPool, ConnectionPoolConfig,
    ConnectionPoolManager, ConnectionState, ConnectionStats, LoadBalancingStrategy, PoolStats,
    PooledConnection,
};
pub use discovery::{
    BootstrapConfig, DhtConfig, DiscoveryEvent, DiscoveryMethod, DiscoveryService, Distance,
    KBucket, MdnsDiscovery, NodeId, PeerEntry, RoutingTable, RoutingTableStats,
};
pub use p2p::{
    DiscoveredPeer, DiscoveryRequest, DiscoveryResponse, KeepaliveMessage, KeepaliveResponse,
    P2PNode, PeerId, ProtocolHandler, ProtocolHandshake, PROTOCOL_NAME, PROTOCOL_VERSION,
};
use serde::{Deserialize, Serialize};
pub use transport::{Transport, TransportConfig, TransportProtocol};

/// Network configuration for peer-to-peer communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Addresses to listen on for incoming connections
    pub listen_addresses: Vec<String>,
    /// Bootstrap peer addresses for initial discovery
    pub bootstrap_peers: Vec<String>,
    /// Maximum number of peer connections
    pub max_peers: usize,
    /// Minimum number of peers to maintain
    pub min_peers: usize,
    /// Connection timeout in seconds
    pub connection_timeout_secs: u64,
    /// Enable relay functionality
    pub enable_relay: bool,
    /// Enable DHT for peer discovery
    pub enable_dht: bool,
    /// Enable connection pooling
    pub enable_connection_pooling: bool,
    /// Pool size (min, max)
    pub pool_size: (usize, usize),
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addresses: vec!["/ip4/0.0.0.0/tcp/4001".to_string()],
            bootstrap_peers: vec![],
            max_peers: 50,
            min_peers: 5,
            connection_timeout_secs: 30,
            enable_relay: true,
            enable_dht: true,
            enable_connection_pooling: true,
            pool_size: (5, 20),
        }
    }
}

/// Information about a connected peer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    /// Unique peer identifier
    pub peer_id: String,
    /// Network addresses for the peer
    pub addresses: Vec<String>,
    /// Supported protocol versions
    pub protocols: Vec<String>,
    /// Unix timestamp when connected
    pub connected_since: u64,
    /// Current latency in milliseconds
    pub latency_ms: u64,
    /// Reputation score (-100 to 100)
    pub reputation: i32,
}

impl PeerInfo {
    /// Check if peer is healthy (positive reputation)
    pub fn is_healthy(&self) -> bool {
        self.reputation >= 0
    }

    /// Check if peer has low latency
    pub fn has_low_latency(&self, threshold_ms: u64) -> bool {
        self.latency_ms < threshold_ms
    }
}

/// Network operation statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkStats {
    /// Total known peers
    pub total_peers: usize,
    /// Currently connected peers
    pub connected_peers: usize,
    /// Connections in progress
    pub pending_connections: usize,
    /// Total bytes sent
    pub bytes_sent: u64,
    /// Total bytes received
    pub bytes_received: u64,
    /// Total messages sent
    pub messages_sent: u64,
    /// Total messages received
    pub messages_received: u64,
    /// Total failed connections
    pub failed_connections: u64,
    /// Average connection latency
    pub avg_latency_ms: u64,
}

impl NetworkStats {
    /// Calculate messages per second (approximate)
    pub fn msg_rate(&self, window_secs: u64) -> f64 {
        if window_secs == 0 {
            return 0.0;
        }
        (self.messages_sent + self.messages_received) as f64 / window_secs as f64
    }

    /// Calculate bandwidth usage in bytes per second
    pub fn bandwidth_bps(&self, window_secs: u64) -> f64 {
        if window_secs == 0 {
            return 0.0;
        }
        (self.bytes_sent + self.bytes_received) as f64 / window_secs as f64
    }
}

/// Manages network connections and peer relationships
pub struct NetworkManager {
    /// Network configuration
    #[allow(dead_code)]
    config: NetworkConfig,
    /// Connected peers
    peers: HashMap<String, PeerInfo>,
    /// Network statistics
    stats: NetworkStats,
    /// P2P node (if enabled)
    p2p_node: Option<Arc<p2p::P2PNode>>,
    /// Discovery service (if enabled)
    discovery: Option<Arc<discovery::DiscoveryService>>,
    /// Connection pool manager (if enabled)
    pool_manager: Option<Arc<connection::ConnectionPoolManager>>,
}

impl NetworkManager {
    /// Create new network manager with configuration
    pub fn new(config: NetworkConfig) -> Self {
        Self {
            #[allow(dead_code)]
            config: config.clone(),
            peers: HashMap::new(),
            stats: NetworkStats {
                total_peers: 0,
                connected_peers: 0,
                pending_connections: 0,
                bytes_sent: 0,
                bytes_received: 0,
                messages_sent: 0,
                messages_received: 0,
                failed_connections: 0,
                avg_latency_ms: 0,
            },
            p2p_node: None,
            discovery: None,
            pool_manager: None,
        }
    }

    /// Create with P2P node
    pub fn with_p2p(mut self, p2p_node: Arc<p2p::P2PNode>) -> Self {
        self.p2p_node = Some(p2p_node);
        self
    }

    /// Create with discovery service
    pub fn with_discovery(mut self, discovery: Arc<discovery::DiscoveryService>) -> Self {
        self.discovery = Some(discovery);
        self
    }

    /// Create with pool manager
    pub fn with_pool_manager(
        mut self,
        pool_manager: Arc<connection::ConnectionPoolManager>,
    ) -> Self {
        self.pool_manager = Some(pool_manager);
        self
    }

    /// Add peer to managed list
    pub fn add_peer(&mut self, peer_id: String, info: PeerInfo) {
        self.peers.insert(peer_id, info);
        self.stats.total_peers = self.peers.len();
        self.stats.connected_peers += 1;
    }

    /// Remove peer from list
    pub fn remove_peer(&mut self, peer_id: &str) {
        if self.peers.remove(peer_id).is_some() {
            self.stats.total_peers = self.peers.len();
            self.stats.connected_peers = self.stats.connected_peers.saturating_sub(1);
        }
    }

    /// Get peer info by ID
    pub fn get_peer(&self, peer_id: &str) -> Option<&PeerInfo> {
        self.peers.get(peer_id)
    }

    /// Get all connected peers
    pub fn get_all_peers(&self) -> Vec<&PeerInfo> {
        self.peers.values().collect()
    }

    /// Get healthy peers only
    pub fn get_healthy_peers(&self) -> Vec<&PeerInfo> {
        self.peers.values().filter(|p| p.is_healthy()).collect()
    }

    /// Get peers sorted by latency
    pub fn get_peers_by_latency(&self) -> Vec<&PeerInfo> {
        let mut peers: Vec<_> = self.peers.values().collect();
        peers.sort_by_key(|p| p.latency_ms);
        peers
    }

    /// Get network statistics
    pub fn get_stats(&self) -> &NetworkStats {
        &self.stats
    }

    /// Record sent message statistics
    pub fn record_message_sent(&mut self, bytes: u64) {
        self.stats.messages_sent += 1;
        self.stats.bytes_sent += bytes;
    }

    /// Record received message statistics
    pub fn record_message_received(&mut self, bytes: u64) {
        self.stats.messages_received += 1;
        self.stats.bytes_received += bytes;
    }

    /// Record failed connection
    pub fn record_failed_connection(&mut self) {
        self.stats.failed_connections += 1;
    }

    /// Update average latency
    pub fn update_avg_latency(&mut self, latency_ms: u64) {
        // Simple running average
        let n = self.stats.connected_peers as u64;
        if n > 0 {
            self.stats.avg_latency_ms = (self.stats.avg_latency_ms * (n - 1) + latency_ms) / n;
        } else {
            self.stats.avg_latency_ms = latency_ms;
        }
    }

    /// Get P2P node reference
    pub fn p2p_node(&self) -> Option<&Arc<p2p::P2PNode>> {
        self.p2p_node.as_ref()
    }

    /// Get discovery service reference
    pub fn discovery(&self) -> Option<&Arc<discovery::DiscoveryService>> {
        self.discovery.as_ref()
    }

    /// Get pool manager reference
    pub fn pool_manager(&self) -> Option<&Arc<connection::ConnectionPoolManager>> {
        self.pool_manager.as_ref()
    }

    /// Get peer count
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Check if peer exists
    pub fn has_peer(&self, peer_id: &str) -> bool {
        self.peers.contains_key(peer_id)
    }
}

/// Network message structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique message identifier
    pub id: String,
    /// Source peer ID
    pub source: String,
    /// Destination peer ID (None for broadcast)
    pub destination: Option<String>,
    /// Type of message
    pub message_type: MessageType,
    /// Message payload data
    pub payload: Vec<u8>,
    /// Unix timestamp when message was created
    pub timestamp: u64,
    /// Time-to-live for message propagation
    pub ttl: u32,
}

impl Message {
    /// Create a new message
    pub fn new(source: impl Into<String>, message_type: MessageType, payload: Vec<u8>) -> Self {
        Self {
            id: generate_message_id(),
            source: source.into(),
            destination: None,
            message_type,
            payload,
            timestamp: current_timestamp(),
            ttl: 300, // 5 minutes default
        }
    }

    /// Set destination
    pub fn with_destination(mut self, dest: impl Into<String>) -> Self {
        self.destination = Some(dest.into());
        self
    }

    /// Set TTL
    pub fn with_ttl(mut self, ttl: u32) -> Self {
        self.ttl = ttl;
        self
    }

    /// Check if message is expired
    pub fn is_expired(&self) -> bool {
        let now = current_timestamp();
        now > self.timestamp + self.ttl as u64
    }

    /// Get payload size
    pub fn payload_size(&self) -> usize {
        self.payload.len()
    }
}

/// Types of network messages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum MessageType {
    /// Peer discovery message
    Discovery,
    /// Connection handshake
    Handshake,
    /// Keep-alive heartbeat
    Heartbeat,
    /// Application data message
    Data,
    /// Gossip protocol message
    Gossip,
    /// Request message
    Request,
    /// Response message
    Response,
    /// Control message
    Control,
    /// Error message
    Error,
}

impl std::fmt::Display for MessageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageType::Discovery => write!(f, "DISCOVERY"),
            MessageType::Handshake => write!(f, "HANDSHAKE"),
            MessageType::Heartbeat => write!(f, "HEARTBEAT"),
            MessageType::Data => write!(f, "DATA"),
            MessageType::Gossip => write!(f, "GOSSIP"),
            MessageType::Request => write!(f, "REQUEST"),
            MessageType::Response => write!(f, "RESPONSE"),
            MessageType::Control => write!(f, "CONTROL"),
            MessageType::Error => write!(f, "ERROR"),
        }
    }
}

/// Handler for network events
pub trait NetworkHandler: Send + Sync {
    /// Called when peer connects
    fn on_connect(&self, peer_id: &str);
    /// Called when peer disconnects
    fn on_disconnect(&self, peer_id: &str);
    /// Called when message is received
    fn on_message(&self, peer_id: &str, message: Message) -> Result<(), NetworkError>;
}

/// Network operation errors
#[derive(Debug, Clone)]
pub enum NetworkError {
    /// Connection establishment failed
    ConnectionFailed(String),
    /// Failed to send data
    SendFailed(String),
    /// Failed to receive data
    ReceiveFailed(String),
    /// Peer not found in peer list
    PeerNotFound,
    /// Operation timed out
    Timeout,
    /// Invalid or malformed message
    InvalidMessage,
    /// Protocol version mismatch
    ProtocolMismatch(String),
    /// Resource exhausted
    ResourceExhausted(String),
    /// Not implemented
    NotImplemented(String),
}

impl std::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkError::ConnectionFailed(e) => write!(f, "Connection failed: {}", e),
            NetworkError::SendFailed(e) => write!(f, "Send failed: {}", e),
            NetworkError::ReceiveFailed(e) => write!(f, "Receive failed: {}", e),
            NetworkError::PeerNotFound => write!(f, "Peer not found"),
            NetworkError::Timeout => write!(f, "Timeout"),
            NetworkError::InvalidMessage => write!(f, "Invalid message"),
            NetworkError::ProtocolMismatch(e) => write!(f, "Protocol mismatch: {}", e),
            NetworkError::ResourceExhausted(e) => write!(f, "Resource exhausted: {}", e),
            NetworkError::NotImplemented(e) => write!(f, "Not implemented: {}", e),
        }
    }
}

impl std::error::Error for NetworkError {}

/// Generate unique message ID
fn generate_message_id() -> String {
    use uuid::Uuid;
    Uuid::new_v4().to_string()
}

/// Get current timestamp in seconds
fn current_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_config_default() {
        let config = NetworkConfig::default();
        assert!(!config.listen_addresses.is_empty());
        assert!(config.enable_dht);
        assert!(config.enable_connection_pooling);
    }

    #[test]
    fn test_message_creation() {
        let msg = Message::new("peer1", MessageType::Data, vec![1, 2, 3]);
        assert_eq!(msg.source, "peer1");
        assert_eq!(msg.message_type, MessageType::Data);
        assert!(!msg.is_expired());
        assert_eq!(msg.payload_size(), 3);
    }

    #[test]
    fn test_network_stats() {
        let stats = NetworkStats {
            messages_sent: 100,
            messages_received: 200,
            bytes_sent: 1000,
            bytes_received: 2000,
            ..Default::default()
        };

        assert_eq!(stats.msg_rate(10), 30.0);
        assert_eq!(stats.bandwidth_bps(10), 300.0);
    }

    #[test]
    fn test_message_type_display() {
        assert_eq!(format!("{}", MessageType::Heartbeat), "HEARTBEAT");
        assert_eq!(format!("{}", MessageType::Data), "DATA");
    }

    #[test]
    fn test_network_manager() {
        let config = NetworkConfig::default();
        let mut manager = NetworkManager::new(config);

        let peer = PeerInfo {
            peer_id: "peer1".to_string(),
            addresses: vec!["127.0.0.1:4001".to_string()],
            protocols: vec!["/beebotos/1.0.0".to_string()],
            connected_since: 0,
            latency_ms: 50,
            reputation: 10,
        };

        manager.add_peer("peer1".to_string(), peer);
        assert_eq!(manager.peer_count(), 1);
        assert!(manager.has_peer("peer1"));

        let healthy = manager.get_healthy_peers();
        assert_eq!(healthy.len(), 1);

        manager.remove_peer("peer1");
        assert_eq!(manager.peer_count(), 0);
    }
}
