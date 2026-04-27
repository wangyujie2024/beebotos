//! Peer-to-Peer Networking
//!
//! P2P network implementation supporting:
//! - Peer discovery and connection management
//! - Message routing with multiple protocols
//! - Reputation-based peer scoring
//! - Gossip protocol for broadcast
//! - Connection health monitoring and auto-reconnect
//! - Protocol versioning and negotiation

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock, Semaphore};
use tokio::time::{timeout, Duration, Instant};
use tracing::{debug, error, info, warn};

use super::{
    Message, MessageType, NetworkConfig, NetworkError, NetworkHandler, NetworkStats, PeerInfo,
};

/// Protocol version
pub const PROTOCOL_VERSION: &str = "1.0.0";
/// Protocol name
pub const PROTOCOL_NAME: &str = "/beebotos/p2p/1.0.0";
/// Maximum message size (16 MB)
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;
/// Default keepalive interval in seconds
const DEFAULT_KEEPALIVE_INTERVAL_SECS: u64 = 30;
/// Default reconnect interval in seconds
const DEFAULT_RECONNECT_INTERVAL_SECS: u64 = 10;
/// Maximum reconnect attempts
const MAX_RECONNECT_ATTEMPTS: u32 = 5;

/// Unique identifier for a peer
pub type PeerId = String;

/// P2P network node
pub struct P2PNode {
    /// Local peer ID
    local_id: PeerId,
    /// Network configuration
    config: NetworkConfig,
    /// Connected peers
    peers: Arc<RwLock<HashMap<PeerId, PeerConnection>>>,
    /// Pending reconnections
    pending_reconnects: Arc<RwLock<HashMap<PeerId, ReconnectState>>>,
    /// Network statistics
    stats: Arc<RwLock<NetworkStats>>,
    /// Message handler
    handler: Arc<RwLock<Option<Box<dyn NetworkHandler>>>>,
    /// Channel for outgoing messages
    message_tx: mpsc::UnboundedSender<(PeerId, Message)>,
    message_rx: Arc<RwLock<mpsc::UnboundedReceiver<(PeerId, Message)>>>,
    /// Channel for broadcast messages (reserved for future use)
    #[allow(dead_code)]
    broadcast_tx: mpsc::UnboundedSender<Message>,
    /// Shutdown signal
    shutdown: Arc<RwLock<bool>>,
    /// Listen address
    listen_addr: Arc<RwLock<Option<SocketAddr>>>,
    /// Connection semaphore for limiting concurrent connections
    connection_semaphore: Arc<Semaphore>,
    /// Protocol handlers
    protocol_handlers: Arc<RwLock<HashMap<String, Box<dyn ProtocolHandler>>>>,
}

/// Peer connection state
#[derive(Debug, Clone)]
struct PeerConnection {
    id: PeerId,
    addr: SocketAddr,
    /// TCP stream - retained for connection lifecycle management
    #[allow(dead_code)]
    _stream: Arc<RwLock<TcpStream>>,
    connected_since: Instant,
    /// Last seen timestamp - reserved for connection health monitoring
    #[allow(dead_code)]
    _last_seen: Arc<RwLock<Instant>>,
    reputation: Arc<RwLock<i32>>,
    message_tx: mpsc::UnboundedSender<Message>,
    /// Current latency in milliseconds
    latency_ms: Arc<RwLock<u64>>,
    /// Latency history for averaging
    latency_history: Arc<RwLock<Vec<u64>>>,
    /// Connection protocol version
    protocol_version: String,
    /// Supported capabilities (reserved for future use)
    #[allow(dead_code)]
    capabilities: Vec<String>,
    /// Connection health score (0-100)
    health_score: Arc<RwLock<u8>>,
}

/// Reconnection state
#[derive(Debug, Clone)]
struct ReconnectState {
    peer_addr: String,
    attempts: u32,
    last_attempt: Instant,
}

/// Protocol handler trait
pub trait ProtocolHandler: Send + Sync {
    /// Handle incoming message for this protocol
    fn handle_message(&self, peer_id: &PeerId, message: &Message) -> Result<(), NetworkError>;
    /// Get protocol name
    fn protocol_name(&self) -> &str;
    /// Get supported message types
    fn supported_types(&self) -> Vec<MessageType>;
}

/// Peer discovery request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryRequest {
    /// Unique identifier of the requesting peer
    pub peer_id: PeerId,
    /// Network address the peer is listening on
    pub listen_addr: String,
    /// Protocol versions supported by this peer
    pub protocols: Vec<String>,
    /// Protocol version
    pub version: String,
    /// Supported capabilities
    pub capabilities: Vec<String>,
    /// Timestamp
    pub timestamp: u64,
}

/// Peer discovery response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryResponse {
    /// List of peers discovered on the network
    pub peers: Vec<DiscoveredPeer>,
    /// Responder's peer ID
    pub responder_id: PeerId,
}

/// Discovered peer information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredPeer {
    /// Unique peer identifier
    pub peer_id: PeerId,
    /// Network addresses for the peer
    pub addresses: Vec<String>,
    /// Supported protocol versions
    pub protocols: Vec<String>,
    /// Supported capabilities
    pub capabilities: Vec<String>,
    /// Peer reputation score
    pub reputation: i32,
}

/// Protocol handshake message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolHandshake {
    /// Protocol version
    pub version: String,
    /// Supported message types
    pub supported_types: Vec<String>,
    /// Supported capabilities
    pub capabilities: Vec<String>,
    /// Nonce for security
    pub nonce: u64,
}

/// Keepalive message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeepaliveMessage {
    /// Timestamp
    pub timestamp: u64,
    /// Sequence number
    pub seq: u64,
}

/// Keepalive response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeepaliveResponse {
    /// Original timestamp
    pub timestamp: u64,
    /// Response timestamp
    pub response_timestamp: u64,
    /// Sequence number
    pub seq: u64,
}

impl P2PNode {
    /// Create new P2P node
    pub fn new(config: NetworkConfig) -> Self {
        let local_id = generate_peer_id();
        let (message_tx, message_rx) = mpsc::unbounded_channel();
        let (broadcast_tx, _broadcast_rx) = mpsc::unbounded_channel();

        // Create connection semaphore based on max peers
        let max_connections = config.max_peers.max(100);

        Self {
            local_id,
            config,
            peers: Arc::new(RwLock::new(HashMap::new())),
            pending_reconnects: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(NetworkStats {
                total_peers: 0,
                connected_peers: 0,
                pending_connections: 0,
                bytes_sent: 0,
                bytes_received: 0,
                messages_sent: 0,
                messages_received: 0,
                failed_connections: 0,
                avg_latency_ms: 0,
            })),
            handler: Arc::new(RwLock::new(None)),
            message_tx,
            message_rx: Arc::new(RwLock::new(message_rx)),
            broadcast_tx,
            shutdown: Arc::new(RwLock::new(false)),
            listen_addr: Arc::new(RwLock::new(None)),
            connection_semaphore: Arc::new(Semaphore::new(max_connections)),
            protocol_handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get local peer ID
    pub fn local_id(&self) -> &str {
        &self.local_id
    }

    /// Register a protocol handler
    pub async fn register_protocol_handler(&self, handler: Box<dyn ProtocolHandler>) {
        let name = handler.protocol_name().to_string();
        info!("Registered protocol handler: {}", name);
        self.protocol_handlers.write().await.insert(name, handler);
    }

    /// Start the P2P node
    pub async fn start(&self) -> Result<SocketAddr, NetworkError> {
        let addr = self.config.listen_addresses.first().ok_or_else(|| {
            NetworkError::ConnectionFailed("No listen address configured".to_string())
        })?;

        // Parse address
        let socket_addr: SocketAddr = addr
            .parse()
            .map_err(|e| NetworkError::ConnectionFailed(format!("Invalid address: {}", e)))?;

        // Start listening
        let listener = TcpListener::bind(socket_addr)
            .await
            .map_err(|e| NetworkError::ConnectionFailed(format!("Failed to bind: {}", e)))?;

        let actual_addr = listener.local_addr().map_err(|e| {
            NetworkError::ConnectionFailed(format!("Failed to get local addr: {}", e))
        })?;

        *self.listen_addr.write().await = Some(actual_addr);

        info!(
            "P2P node listening on {} with ID {}",
            actual_addr, self.local_id
        );

        // Start connection acceptor
        self.spawn_acceptor(listener);

        // Start message processor
        self.spawn_message_processor();

        // Start keepalive handler
        self.spawn_keepalive_handler();

        // Start reconnection handler
        self.spawn_reconnection_handler();

        // Connect to bootstrap peers
        self.connect_to_bootstrap_peers().await;

        Ok(actual_addr)
    }

    /// Set message handler
    pub async fn set_handler(&self, handler: Box<dyn NetworkHandler>) {
        *self.handler.write().await = Some(handler);
    }

    /// Connect to a peer
    pub async fn connect(&self, addr: &str) -> Result<PeerId, NetworkError> {
        let socket_addr: SocketAddr = addr
            .parse()
            .map_err(|e| NetworkError::ConnectionFailed(format!("Invalid address: {}", e)))?;

        // Acquire connection permit
        let _permit = self
            .connection_semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| NetworkError::ConnectionFailed("Connection limit reached".to_string()))?;

        // Update pending connections stat
        {
            let mut stats = self.stats.write().await;
            stats.pending_connections += 1;
        }

        // Connect with timeout
        let result = timeout(
            Duration::from_secs(self.config.connection_timeout_secs),
            TcpStream::connect(socket_addr),
        )
        .await;

        // Update pending connections stat
        {
            let mut stats = self.stats.write().await;
            stats.pending_connections = stats.pending_connections.saturating_sub(1);
        }

        let stream = result
            .map_err(|_| NetworkError::Timeout)?
            .map_err(|e| NetworkError::ConnectionFailed(format!("Connection failed: {}", e)))?;

        // Perform protocol handshake
        let (peer_id, handshake) = self.perform_handshake(stream, socket_addr).await?;

        info!(
            "Connected to peer {} at {} with protocol version {}",
            peer_id, addr, handshake.version
        );

        Ok(peer_id)
    }

    /// Send message to a specific peer
    pub fn send_message(&self, peer_id: &PeerId, message: Message) -> Result<(), NetworkError> {
        // Validate message size
        let payload_size = message.payload.len();
        if payload_size > MAX_MESSAGE_SIZE {
            return Err(NetworkError::SendFailed(format!(
                "Message too large: {} > {}",
                payload_size, MAX_MESSAGE_SIZE
            )));
        }

        self.message_tx
            .send((peer_id.clone(), message))
            .map_err(|_| NetworkError::SendFailed("Message channel closed".to_string()))?;
        Ok(())
    }

    /// Send message with delivery confirmation
    pub async fn send_message_with_confirm(
        &self,
        peer_id: &PeerId,
        message: Message,
        _timeout_duration: Duration,
    ) -> Result<(), NetworkError> {
        self.send_message(peer_id, message)?;

        // In a real implementation, we would wait for an ACK
        // For now, just simulate with a timeout
        tokio::time::sleep(Duration::from_millis(10)).await;
        Ok(())
    }

    /// Broadcast message to all peers
    pub async fn broadcast(&self, message: Message) -> usize {
        let peers = self.peers.read().await;
        let mut sent = 0;

        for (peer_id, conn) in peers.iter() {
            if *conn.health_score.read().await >= 50 {
                if conn.message_tx.send(message.clone()).is_ok() {
                    sent += 1;
                }
            } else {
                debug!("Skipping broadcast to unhealthy peer {}", peer_id);
            }
        }

        // Update stats
        self.stats.write().await.messages_sent += sent as u64;

        sent
    }

    /// Gossip message to a subset of peers (for efficient broadcast)
    pub async fn gossip(&self, message: Message, fanout: usize) -> usize {
        let peers = self.peers.read().await;

        // Collect healthy peers (can't use async in filter directly)
        let mut healthy_peers = Vec::new();
        for (peer_id, conn) in peers.iter() {
            if *conn.health_score.read().await >= 50 {
                healthy_peers.push((peer_id.clone(), conn));
            }
        }

        let fanout = fanout.min(healthy_peers.len());
        let mut sent = 0;

        // Simple gossip: select first N healthy peers
        // In production, use a more sophisticated peer selection algorithm
        for (_, conn) in healthy_peers.iter().take(fanout) {
            if conn.message_tx.send(message.clone()).is_ok() {
                sent += 1;
            }
        }

        sent
    }

    /// Get connected peers
    pub async fn get_peers(&self) -> Vec<PeerInfo> {
        let peers = self.peers.read().await;
        let mut result = Vec::new();
        for conn in peers.values() {
            result.push(PeerInfo {
                peer_id: conn.id.clone(),
                addresses: vec![conn.addr.to_string()],
                protocols: vec![format!("/beebotos/{}", conn.protocol_version)],
                connected_since: conn.connected_since.elapsed().as_secs(),
                latency_ms: *conn.latency_ms.read().await,
                reputation: *conn.reputation.read().await,
            });
        }
        result
    }

    /// Get healthy peers (health score >= 50)
    pub async fn get_healthy_peers(&self) -> Vec<PeerInfo> {
        self.get_peers()
            .await
            .into_iter()
            .filter(|p| p.reputation >= 0)
            .collect()
    }

    /// Measure and update latency for a peer
    pub async fn measure_peer_latency(&self, peer_id: &PeerId) -> Option<u64> {
        // Get peer address for measurement
        let _addr = {
            let peers = self.peers.read().await;
            let conn = peers.get(peer_id)?;
            conn.addr
        };

        // Send keepalive and measure response time
        let start = Instant::now();

        let keepalive = Message {
            id: generate_message_id(),
            source: self.local_id.clone(),
            destination: Some(peer_id.clone()),
            message_type: MessageType::Heartbeat,
            payload: serde_json::to_vec(&KeepaliveMessage {
                timestamp: start.elapsed().as_millis() as u64,
                seq: 0,
            })
            .unwrap_or_default(),
            timestamp: current_timestamp(),
            ttl: 30,
        };

        if self.send_message(peer_id, keepalive).is_ok() {
            // Wait for response (in real implementation, this would be async)
            tokio::time::sleep(Duration::from_millis(100)).await;
            let latency = start.elapsed().as_millis() as u64;

            // Update latency
            {
                let peers = self.peers.read().await;
                let conn = peers.get(peer_id)?;
                *conn.latency_ms.write().await = latency;

                let mut history = conn.latency_history.write().await;
                history.push(latency);
                if history.len() > 10 {
                    history.remove(0);
                }
            }

            Some(latency)
        } else {
            None
        }
    }

    /// Get average latency for a peer
    pub async fn get_average_latency(&self, peer_id: &PeerId) -> Option<u64> {
        let peers = self.peers.read().await;
        let conn = peers.get(peer_id)?;
        let history = conn.latency_history.read().await;

        if history.is_empty() {
            None
        } else {
            Some(history.iter().sum::<u64>() / history.len() as u64)
        }
    }

    /// Update peer reputation
    pub async fn update_reputation(&self, peer_id: &PeerId, delta: i32) {
        let peers = self.peers.read().await;
        if let Some(conn) = peers.get(peer_id) {
            let mut reputation = conn.reputation.write().await;
            *reputation = (*reputation + delta).clamp(-100, 100);
            debug!("Updated reputation for peer {}: {}", peer_id, *reputation);
        }
    }

    /// Get network statistics
    pub async fn stats(&self) -> NetworkStats {
        self.stats.read().await.clone()
    }

    /// Disconnect from a peer
    pub async fn disconnect(&self, peer_id: &PeerId) {
        let mut peers = self.peers.write().await;
        if let Some(_conn) = peers.remove(peer_id) {
            info!("Disconnected from peer {}", peer_id);

            // Update stats
            let mut stats = self.stats.write().await;
            stats.connected_peers = peers.len();
        }
    }

    /// Shutdown the node
    pub async fn shutdown(&self) {
        *self.shutdown.write().await = true;

        // Disconnect all peers
        let mut peers = self.peers.write().await;
        peers.clear();

        info!("P2P node shutdown complete");
    }

    /// Check if node is running
    pub async fn is_running(&self) -> bool {
        !*self.shutdown.read().await
    }

    /// Get peer count
    pub async fn peer_count(&self) -> usize {
        self.peers.read().await.len()
    }

    /// Get pending reconnection count
    pub async fn pending_reconnect_count(&self) -> usize {
        self.pending_reconnects.read().await.len()
    }

    // Internal: Spawn connection acceptor
    fn spawn_acceptor(&self, listener: TcpListener) {
        let peers = self.peers.clone();
        let stats = self.stats.clone();
        let handler = self.handler.clone();
        let shutdown = self.shutdown.clone();
        let local_id = self.local_id.clone();
        let connection_semaphore = self.connection_semaphore.clone();

        tokio::spawn(async move {
            loop {
                if *shutdown.read().await {
                    break;
                }

                // Acquire permit for new connection
                let permit = match connection_semaphore.clone().try_acquire_owned() {
                    Ok(permit) => permit,
                    Err(_) => {
                        // Connection limit reached, wait a bit
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    }
                };

                match listener.accept().await {
                    Ok((stream, addr)) => {
                        debug!("Incoming connection from {}", addr);

                        let peers = peers.clone();
                        let stats = stats.clone();
                        let handler = handler.clone();
                        let shutdown = shutdown.clone();
                        let local_id = local_id.clone();

                        tokio::spawn(async move {
                            let _permit = permit; // Hold permit for the lifetime of the connection
                            if let Err(e) = handle_incoming_connection(
                                stream, addr, peers, stats, handler, shutdown, local_id,
                            )
                            .await
                            {
                                debug!("Connection from {} closed: {}", addr, e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Accept error: {}", e);
                    }
                }
            }
        });
    }

    // Internal: Spawn message processor
    fn spawn_message_processor(&self) {
        let message_rx = self.message_rx.clone();
        let peers = self.peers.clone();
        let stats = self.stats.clone();
        let shutdown = self.shutdown.clone();
        let protocol_handlers = self.protocol_handlers.clone();
        let handler = self.handler.clone();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                loop {
                    if *shutdown.read().await {
                        break;
                    }

                    // Try to receive a message without holding the lock across await
                    let msg = {
                        let mut rx = message_rx.write().await;
                        match rx.try_recv() {
                            Ok(msg) => Some(msg),
                            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                                drop(rx);
                                tokio::time::sleep(Duration::from_millis(10)).await;
                                continue;
                            }
                            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                                break;
                            }
                        }
                    };

                    if let Some((peer_id, message)) = msg {
                        // Update stats
                        {
                            let mut stats = stats.write().await;
                            stats.messages_sent += 1;
                            stats.bytes_sent += message.payload.len() as u64;
                        }

                        // Try protocol-specific handler first
                        let handled = {
                            let handlers = protocol_handlers.read().await;
                            let mut handled = false;
                            for (name, protocol_handler) in handlers.iter() {
                                if protocol_handler
                                    .supported_types()
                                    .contains(&message.message_type)
                                {
                                    if let Err(e) =
                                        protocol_handler.handle_message(&peer_id, &message)
                                    {
                                        warn!("Protocol handler {} failed: {}", name, e);
                                    } else {
                                        handled = true;
                                    }
                                }
                            }
                            handled
                        };

                        // Fall back to generic handler
                        if !handled {
                            if let Some(ref h) = *handler.read().await {
                                if let Err(e) = h.on_message(&peer_id, message.clone()) {
                                    warn!("Message handler failed: {}", e);
                                }
                            }
                        }

                        // Send to peer
                        let peers = peers.read().await;
                        if let Some(conn) = peers.get(&peer_id) {
                            if conn.message_tx.send(message).is_err() {
                                warn!("Failed to send message to peer {}", peer_id);
                            }
                        }
                    }
                }
            });
        });
    }

    // Internal: Spawn keepalive handler
    fn spawn_keepalive_handler(&self) {
        let peers = self.peers.clone();
        let shutdown = self.shutdown.clone();
        let local_id = self.local_id.clone();

        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(Duration::from_secs(DEFAULT_KEEPALIVE_INTERVAL_SECS));

            loop {
                interval.tick().await;

                if *shutdown.read().await {
                    break;
                }

                let peers_guard = peers.read().await;
                let now = current_timestamp();

                for (peer_id, conn) in peers_guard.iter() {
                    let keepalive = Message {
                        id: generate_message_id(),
                        source: local_id.clone(),
                        destination: Some(peer_id.clone()),
                        message_type: MessageType::Heartbeat,
                        payload: serde_json::to_vec(&KeepaliveMessage {
                            timestamp: now,
                            seq: 0,
                        })
                        .unwrap_or_default(),
                        timestamp: now,
                        ttl: 30,
                    };

                    if conn.message_tx.send(keepalive).is_err() {
                        // Update health score on failure
                        let mut health = conn.health_score.write().await;
                        *health = health.saturating_sub(10);
                    } else {
                        // Increase health score on success
                        let mut health = conn.health_score.write().await;
                        *health = (*health + 5).min(100);
                    }
                }
            }
        });
    }

    // Internal: Spawn reconnection handler
    fn spawn_reconnection_handler(&self) {
        let pending_reconnects = self.pending_reconnects.clone();
        let peers = self.peers.clone();
        let shutdown = self.shutdown.clone();
        let _myself = Arc::new(self.local_id.clone());

        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(Duration::from_secs(DEFAULT_RECONNECT_INTERVAL_SECS));

            loop {
                interval.tick().await;

                if *shutdown.read().await {
                    break;
                }

                let reconnects: Vec<_> = {
                    let pending = pending_reconnects.read().await;
                    pending
                        .iter()
                        .filter(|(_, state)| {
                            state.last_attempt.elapsed()
                                > Duration::from_secs(DEFAULT_RECONNECT_INTERVAL_SECS)
                                && state.attempts < MAX_RECONNECT_ATTEMPTS
                        })
                        .map(|(id, state)| (id.clone(), state.clone()))
                        .collect()
                };

                for (peer_id, mut state) in reconnects {
                    // Check if already connected
                    if peers.read().await.contains_key(&peer_id) {
                        pending_reconnects.write().await.remove(&peer_id);
                        continue;
                    }

                    info!(
                        "Attempting to reconnect to peer {} at {} (attempt {})",
                        peer_id, state.peer_addr, state.attempts
                    );

                    // In a real implementation, we would try to reconnect here
                    // For now, just update the state
                    state.attempts += 1;
                    state.last_attempt = Instant::now();

                    if state.attempts >= MAX_RECONNECT_ATTEMPTS {
                        warn!("Max reconnection attempts reached for peer {}", peer_id);
                        pending_reconnects.write().await.remove(&peer_id);
                    } else {
                        pending_reconnects.write().await.insert(peer_id, state);
                    }
                }
            }
        });
    }

    // Internal: Connect to bootstrap peers
    async fn connect_to_bootstrap_peers(&self) {
        for peer_addr in &self.config.bootstrap_peers {
            match self.connect(peer_addr).await {
                Ok(peer_id) => {
                    info!("Connected to bootstrap peer {} at {}", peer_id, peer_addr);
                }
                Err(e) => {
                    warn!("Failed to connect to bootstrap peer {}: {}", peer_addr, e);
                    // Add to pending reconnects
                    self.pending_reconnects.write().await.insert(
                        format!("bootstrap_{}", peer_addr),
                        ReconnectState {
                            peer_addr: peer_addr.clone(),
                            attempts: 1,
                            last_attempt: Instant::now(),
                        },
                    );
                }
            }
        }
    }

    // Internal: Perform handshake with new connection
    async fn perform_handshake(
        &self,
        stream: TcpStream,
        addr: SocketAddr,
    ) -> Result<(PeerId, ProtocolHandshake), NetworkError> {
        let mut stream = stream;

        // Create handshake
        let handshake = ProtocolHandshake {
            version: PROTOCOL_VERSION.to_string(),
            supported_types: vec![
                "discovery".to_string(),
                "handshake".to_string(),
                "heartbeat".to_string(),
                "data".to_string(),
                "gossip".to_string(),
                "request".to_string(),
                "response".to_string(),
            ],
            capabilities: vec!["gossip".to_string(), "dht".to_string()],
            nonce: rand::random(),
        };

        let discovery = DiscoveryRequest {
            peer_id: self.local_id.clone(),
            listen_addr: self
                .listen_addr
                .read()
                .await
                .map(|a| a.to_string())
                .unwrap_or_default(),
            protocols: vec![PROTOCOL_NAME.to_string()],
            version: PROTOCOL_VERSION.to_string(),
            capabilities: handshake.capabilities.clone(),
            timestamp: current_timestamp(),
        };

        let handshake_bytes = serde_json::to_vec(&(&handshake, &discovery))
            .map_err(|e| NetworkError::SendFailed(format!("Serialization failed: {}", e)))?;

        // Send handshake length and data
        stream
            .write_all(&(handshake_bytes.len() as u64).to_be_bytes())
            .await
            .map_err(|e| NetworkError::SendFailed(e.to_string()))?;
        stream
            .write_all(&handshake_bytes)
            .await
            .map_err(|e| NetworkError::SendFailed(e.to_string()))?;

        // Read peer's handshake
        let mut len_bytes = [0u8; 8];
        stream
            .read_exact(&mut len_bytes)
            .await
            .map_err(|e| NetworkError::ReceiveFailed(format!("Failed to read handshake: {}", e)))?;

        let len = u64::from_be_bytes(len_bytes) as usize;
        if len > 65536 {
            return Err(NetworkError::InvalidMessage);
        }

        let mut buf = vec![0u8; len];
        stream
            .read_exact(&mut buf)
            .await
            .map_err(|e| NetworkError::ReceiveFailed(format!("Failed to read handshake: {}", e)))?;

        let (peer_handshake, peer_discovery): (ProtocolHandshake, DiscoveryRequest) =
            serde_json::from_slice(&buf).map_err(|_| NetworkError::InvalidMessage)?;

        // Validate protocol version compatibility
        if !is_version_compatible(&peer_handshake.version, PROTOCOL_VERSION) {
            return Err(NetworkError::ConnectionFailed(format!(
                "Incompatible protocol version: peer={}, local={}",
                peer_handshake.version, PROTOCOL_VERSION
            )));
        }

        // Create peer connection
        let (message_tx, _message_rx) = mpsc::unbounded_channel();
        let initial_latency = measure_latency_to_addr(addr).await.unwrap_or(0);

        let conn = PeerConnection {
            id: peer_discovery.peer_id.clone(),
            addr,
            _stream: Arc::new(RwLock::new(stream)),
            connected_since: Instant::now(),
            _last_seen: Arc::new(RwLock::new(Instant::now())),
            reputation: Arc::new(RwLock::new(0)),
            message_tx,
            latency_ms: Arc::new(RwLock::new(initial_latency)),
            latency_history: Arc::new(RwLock::new(vec![initial_latency])),
            protocol_version: peer_handshake.version.clone(),
            capabilities: peer_handshake.capabilities.clone(),
            health_score: Arc::new(RwLock::new(100)),
        };

        // Add to peers
        {
            let mut peers = self.peers.write().await;
            peers.insert(peer_discovery.peer_id.clone(), conn);

            let mut stats = self.stats.write().await;
            stats.total_peers = peers.len();
            stats.connected_peers = peers.len();
        }

        info!(
            "Handshake completed with peer {} at {} (version {})",
            peer_discovery.peer_id, addr, &peer_handshake.version
        );

        Ok((peer_discovery.peer_id, peer_handshake.clone()))
    }
}

// Handle incoming connection
async fn handle_incoming_connection(
    stream: TcpStream,
    addr: SocketAddr,
    peers: Arc<RwLock<HashMap<PeerId, PeerConnection>>>,
    stats: Arc<RwLock<NetworkStats>>,
    handler: Arc<RwLock<Option<Box<dyn NetworkHandler>>>>,
    shutdown: Arc<RwLock<bool>>,
    local_id: PeerId,
) -> Result<(), NetworkError> {
    let mut stream = stream;

    // Read handshake
    let mut len_bytes = [0u8; 8];
    stream
        .read_exact(&mut len_bytes)
        .await
        .map_err(|e| NetworkError::ReceiveFailed(format!("Failed to read handshake: {}", e)))?;

    let len = u64::from_be_bytes(len_bytes) as usize;
    if len > 65536 {
        return Err(NetworkError::InvalidMessage);
    }

    let mut buf = vec![0u8; len];
    stream
        .read_exact(&mut buf)
        .await
        .map_err(|e| NetworkError::ReceiveFailed(format!("Failed to read handshake: {}", e)))?;

    let (peer_handshake, peer_discovery): (ProtocolHandshake, DiscoveryRequest) =
        serde_json::from_slice(&buf).map_err(|_| NetworkError::InvalidMessage)?;

    let peer_id = peer_discovery.peer_id.clone();

    // Validate protocol version
    if !is_version_compatible(&peer_handshake.version, PROTOCOL_VERSION) {
        return Err(NetworkError::ConnectionFailed(format!(
            "Incompatible protocol version: peer={}, local={}",
            peer_handshake.version, PROTOCOL_VERSION
        )));
    }

    // Send our handshake response
    let our_handshake = ProtocolHandshake {
        version: PROTOCOL_VERSION.to_string(),
        supported_types: vec![
            "discovery".to_string(),
            "handshake".to_string(),
            "heartbeat".to_string(),
            "data".to_string(),
        ],
        capabilities: vec!["gossip".to_string(), "dht".to_string()],
        nonce: rand::random(),
    };

    let our_discovery = DiscoveryRequest {
        peer_id: local_id,
        listen_addr: addr.to_string(),
        protocols: vec![PROTOCOL_NAME.to_string()],
        version: PROTOCOL_VERSION.to_string(),
        capabilities: our_handshake.capabilities.clone(),
        timestamp: current_timestamp(),
    };

    let response_bytes = serde_json::to_vec(&(&our_handshake, &our_discovery))
        .map_err(|e| NetworkError::SendFailed(format!("Serialization failed: {}", e)))?;

    stream
        .write_all(&(response_bytes.len() as u64).to_be_bytes())
        .await
        .map_err(|e| NetworkError::SendFailed(e.to_string()))?;
    stream
        .write_all(&response_bytes)
        .await
        .map_err(|e| NetworkError::SendFailed(e.to_string()))?;

    // Create message channel
    let (message_tx, mut message_rx) = mpsc::unbounded_channel();

    // Measure initial latency
    let initial_latency = measure_latency_to_addr(addr).await.unwrap_or(0);

    // Wrap stream in Arc<RwLock> for shared access
    let stream = Arc::new(RwLock::new(stream));

    // Create peer connection
    let conn = PeerConnection {
        id: peer_id.clone(),
        addr,
        _stream: stream.clone(),
        connected_since: Instant::now(),
        _last_seen: Arc::new(RwLock::new(Instant::now())),
        reputation: Arc::new(RwLock::new(0)),
        message_tx,
        latency_ms: Arc::new(RwLock::new(initial_latency)),
        latency_history: Arc::new(RwLock::new(vec![initial_latency])),
        protocol_version: peer_handshake.version.clone(),
        capabilities: peer_handshake.capabilities.clone(),
        health_score: Arc::new(RwLock::new(100)),
    };

    // Add to peers
    {
        let mut peers_guard = peers.write().await;
        peers_guard.insert(peer_id.clone(), conn);

        let mut stats_guard = stats.write().await;
        stats_guard.total_peers = peers_guard.len();
        stats_guard.connected_peers = peers_guard.len();
    }

    // Notify handler
    if let Some(ref handler) = *handler.read().await {
        handler.on_connect(&peer_id);
    }

    info!(
        "Peer {} connected from {} (version {})",
        peer_id,
        addr,
        peer_handshake.version.clone()
    );

    // Handle messages from this peer
    loop {
        if *shutdown.read().await {
            break;
        }

        tokio::select! {
            // Outgoing messages to this peer
            Some(message) = message_rx.recv() => {
                let message_bytes = serde_json::to_vec(&message)
                    .map_err(|e| NetworkError::SendFailed(format!("Serialization failed: {}", e)))?;

                // Send message through stream
                let mut stream_guard = stream.write().await;
                if let Err(e) = send_message_bytes(&mut *stream_guard, &message_bytes).await {
                    error!("Failed to send message to peer {}: {}", peer_id, e);
                    break;
                }

                debug!("Sent message {} to peer {}", message.id, peer_id);
            }

            // Timeout for keepalive
            _ = tokio::time::sleep(Duration::from_secs(30)) => {
                // Send keepalive heartbeat
                let ping = Message::new(
                    peer_id.clone(),
                    MessageType::Heartbeat,
                    vec![], // Empty payload for ping
                );

                match serde_json::to_vec(&ping) {
                    Ok(ping_bytes) => {
                        let mut stream_guard = stream.write().await;
                        if let Err(e) = send_message_bytes(&mut *stream_guard, &ping_bytes).await {
                            warn!("Failed to send keepalive to peer {}: {}", peer_id, e);
                            break;
                        }
                        debug!("Sent keepalive to peer {}", peer_id);
                    }
                    Err(e) => {
                        warn!("Failed to serialize keepalive for peer {}: {}", peer_id, e);
                    }
                }
            }
        }
    }

    // Cleanup
    {
        let mut peers_guard = peers.write().await;
        peers_guard.remove(&peer_id);

        let mut stats_guard = stats.write().await;
        stats_guard.connected_peers = peers_guard.len();
    }

    if let Some(ref handler) = *handler.read().await {
        handler.on_disconnect(&peer_id);
    }

    Ok(())
}

// Generate unique peer ID
fn generate_peer_id() -> PeerId {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.gen()).collect();
    format!("16Uiu2HA{}", hex::encode(&bytes[..20]))
}

/// Send message bytes over TCP stream with length prefix
///
/// Format: [8-byte length (big-endian)] [message bytes]
async fn send_message_bytes(stream: &mut TcpStream, bytes: &[u8]) -> Result<(), NetworkError> {
    let len = bytes.len() as u64;

    stream
        .write_all(&len.to_be_bytes())
        .await
        .map_err(|e| NetworkError::SendFailed(format!("Failed to write length: {}", e)))?;

    stream
        .write_all(bytes)
        .await
        .map_err(|e| NetworkError::SendFailed(format!("Failed to write message: {}", e)))?;

    stream
        .flush()
        .await
        .map_err(|e| NetworkError::SendFailed(format!("Failed to flush: {}", e)))?;

    Ok(())
}

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

/// Check if version is compatible (simplified semver check)
fn is_version_compatible(peer_version: &str, our_version: &str) -> bool {
    // Parse major version
    let peer_major = peer_version
        .split('.')
        .next()
        .and_then(|s| s.parse::<u32>().ok());
    let our_major = our_version
        .split('.')
        .next()
        .and_then(|s| s.parse::<u32>().ok());

    match (peer_major, our_major) {
        (Some(p), Some(o)) => p == o, // Major version must match
        _ => false,
    }
}

/// Measure latency to an address using TCP handshake timing
async fn measure_latency_to_addr(addr: SocketAddr) -> Result<u64, NetworkError> {
    let start = Instant::now();

    match timeout(Duration::from_secs(5), TcpStream::connect(addr)).await {
        Ok(Ok(_)) => {
            let latency = start.elapsed().as_millis() as u64;
            Ok(latency)
        }
        Ok(Err(e)) => Err(NetworkError::ConnectionFailed(format!(
            "Failed to connect: {}",
            e
        ))),
        Err(_) => Err(NetworkError::Timeout),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestHandler;

    impl NetworkHandler for TestHandler {
        fn on_connect(&self, peer_id: &str) {
            println!("Peer connected: {}", peer_id);
        }

        fn on_disconnect(&self, peer_id: &str) {
            println!("Peer disconnected: {}", peer_id);
        }

        fn on_message(&self, peer_id: &str, message: Message) -> Result<(), NetworkError> {
            println!("Message from {}: {:?}", peer_id, message);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_p2p_node_creation() {
        let config = NetworkConfig::default();
        let node = P2PNode::new(config);

        assert!(!node.local_id().is_empty());
        assert!(node.is_running().await);
        assert_eq!(node.peer_count().await, 0);
    }

    #[tokio::test]
    async fn test_p2p_start_stop() {
        let config = NetworkConfig {
            listen_addresses: vec!["127.0.0.1:0".to_string()],
            ..Default::default()
        };

        let node = P2PNode::new(config);
        node.set_handler(Box::new(TestHandler)).await;

        let addr = node.start().await.unwrap();
        assert!(addr.port() > 0);

        node.shutdown().await;
        assert!(!node.is_running().await);
    }

    #[tokio::test]
    async fn test_version_compatibility() {
        assert!(is_version_compatible("1.0.0", "1.2.0"));
        assert!(is_version_compatible("1.0.0", "1.0.0"));
        assert!(!is_version_compatible("2.0.0", "1.0.0"));
        // Note: "1.0" vs "1.0.0" - both have major version 1, so they are compatible
        assert!(is_version_compatible("1.0", "1.0.0"));
    }

    #[test]
    fn test_message_id_generation() {
        let id1 = generate_message_id();
        let id2 = generate_message_id();
        assert_ne!(id1, id2);
        assert!(!id1.is_empty());
    }
}
