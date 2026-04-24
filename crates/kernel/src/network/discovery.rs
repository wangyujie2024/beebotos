//! Distributed Hash Table (DHT) based Peer Discovery
//!
//! This module implements Kademlia-style DHT for peer discovery with:
//! - K-buckets for peer routing table management
//! - XOR-based distance metric for peer lookup
//! - Bootstrap node support for initial network join
//! - mDNS for local network discovery
//! - Peer reputation and health tracking

use std::collections::{HashMap, HashSet, VecDeque};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::time::{Duration, Instant};
use tracing::{debug, info, warn};

use super::PeerInfo;
use crate::error::{KernelError, Result};

/// Kademlia K constant - maximum peers per bucket
const K_BUCKET_SIZE: usize = 20;
/// Kademlia ALPHA constant - concurrent lookups
const ALPHA: usize = 3;
/// Kademlia bucket count (160 bits for SHA-1 sized IDs)
const BUCKET_COUNT: usize = 160;
/// Peer refresh interval
const REFRESH_INTERVAL_SECS: u64 = 300;
/// Stale peer timeout
const STALE_TIMEOUT_SECS: u64 = 600;
/// Bootstrap retry interval (reserved for future use)
#[allow(dead_code)]
const BOOTSTRAP_RETRY_INTERVAL_SECS: u64 = 60;

/// Node ID in the DHT (160-bit identifier)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct NodeId(pub [u8; 20]);

impl NodeId {
    /// Create new random node ID
    pub fn random() -> Self {
        let mut bytes = [0u8; 20];
        for i in 0..20 {
            bytes[i] = rand::random();
        }
        Self(bytes)
    }

    /// Create from peer ID string
    pub fn from_peer_id(peer_id: &str) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        peer_id.hash(&mut hasher);
        let hash = hasher.finish();

        let mut bytes = [0u8; 20];
        bytes[0..8].copy_from_slice(&hash.to_be_bytes());
        bytes[8..16].copy_from_slice(&hash.wrapping_mul(0x9e3779b97f4a7c15).to_be_bytes());
        Self(bytes)
    }

    /// Calculate XOR distance to another node ID
    pub fn distance(&self, other: &NodeId) -> Distance {
        let mut bytes = [0u8; 20];
        for i in 0..20 {
            bytes[i] = self.0[i] ^ other.0[i];
        }
        Distance(bytes)
    }

    /// Get the bucket index for a given distance
    pub fn bucket_index(&self, distance: &Distance) -> Option<usize> {
        for (i, &byte) in distance.0.iter().enumerate() {
            if byte != 0 {
                let bit = 7 - (byte.leading_zeros() as usize);
                return Some(i * 8 + bit);
            }
        }
        None
    }

    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

/// XOR distance between two node IDs
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Distance([u8; 20]);

impl Distance {
    /// Check if distance is zero (same node)
    pub fn is_zero(&self) -> bool {
        self.0.iter().all(|&b| b == 0)
    }

    /// Get the leading zeros count
    pub fn leading_zeros(&self) -> u32 {
        let mut count = 0;
        for &byte in &self.0 {
            if byte == 0 {
                count += 8;
            } else {
                count += byte.leading_zeros();
                break;
            }
        }
        count
    }
}

/// Peer entry in the routing table
#[derive(Debug, Clone)]
pub struct PeerEntry {
    /// Node ID
    pub node_id: NodeId,
    /// Peer ID string
    pub peer_id: String,
    /// Network addresses
    pub addresses: Vec<SocketAddr>,
    /// Last seen timestamp
    pub last_seen: Instant,
    /// Last successful query
    pub last_successful_query: Option<Instant>,
    /// Failed query count
    pub failed_queries: u32,
    /// Peer reputation score
    pub reputation: i32,
    /// Supported protocols
    pub protocols: Vec<String>,
    /// Connection latency in ms
    pub latency_ms: u64,
}

impl PeerEntry {
    /// Check if peer is stale
    pub fn is_stale(&self) -> bool {
        self.last_seen.elapsed() > Duration::from_secs(STALE_TIMEOUT_SECS)
    }

    /// Check if peer is healthy
    pub fn is_healthy(&self) -> bool {
        self.failed_queries < 3 && self.reputation >= -20
    }

    /// Update last seen timestamp
    pub fn mark_seen(&mut self) {
        self.last_seen = Instant::now();
    }

    /// Record successful query
    pub fn record_success(&mut self) {
        self.last_successful_query = Some(Instant::now());
        self.failed_queries = 0;
        self.reputation = (self.reputation + 1).min(100);
    }

    /// Record failed query
    pub fn record_failure(&mut self) {
        self.failed_queries += 1;
        self.reputation = (self.reputation - 2).max(-100);
    }
}

/// K-bucket for storing peers at a specific distance range
#[derive(Debug, Clone)]
pub struct KBucket {
    /// Peers in this bucket (sorted by last seen)
    peers: VecDeque<PeerEntry>,
    /// Maximum capacity
    capacity: usize,
}

impl KBucket {
    /// Create new empty bucket
    pub fn new(capacity: usize) -> Self {
        Self {
            peers: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Check if bucket is full
    pub fn is_full(&self) -> bool {
        self.peers.len() >= self.capacity
    }

    /// Check if bucket is empty
    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }

    /// Get peer count
    pub fn len(&self) -> usize {
        self.peers.len()
    }

    /// Insert or update a peer
    pub fn insert(&mut self, entry: PeerEntry) -> Option<PeerEntry> {
        // Check if peer already exists
        if let Some(pos) = self.peers.iter().position(|p| p.node_id == entry.node_id) {
            // Move to front (most recently seen)
            // SAFETY: `pos` was just found by `position()`, so it must be valid
            let mut existing = self
                .peers
                .remove(pos)
                .expect("peer at found position must exist");
            existing.mark_seen();
            existing.addresses = entry.addresses;
            existing.latency_ms = entry.latency_ms;
            self.peers.push_front(existing);
            return None;
        }

        // If bucket is full, try to evict stale peers
        if self.is_full() {
            // Remove stale peers
            self.peers.retain(|p| !p.is_stale());

            // If still full, try to evict unhealthy peers
            if self.is_full() {
                if let Some(pos) = self.peers.iter().position(|p| !p.is_healthy()) {
                    // SAFETY: `pos` was just found by `position()`, so it must be valid
                    let evicted = self
                        .peers
                        .remove(pos)
                        .expect("peer at found position must exist");
                    self.peers.push_front(entry);
                    return Some(evicted);
                }
                // Bucket is full with healthy peers, return the new entry as rejected
                return Some(entry);
            }
        }

        self.peers.push_front(entry);
        None
    }

    /// Remove a peer by node ID
    pub fn remove(&mut self, node_id: &NodeId) -> Option<PeerEntry> {
        if let Some(pos) = self.peers.iter().position(|p| &p.node_id == node_id) {
            self.peers.remove(pos)
        } else {
            None
        }
    }

    /// Get peers in this bucket
    pub fn peers(&self) -> &VecDeque<PeerEntry> {
        &self.peers
    }

    /// Get mutable peers
    pub fn peers_mut(&mut self) -> &mut VecDeque<PeerEntry> {
        &mut self.peers
    }

    /// Find closest peers to a target
    pub fn find_closest(&self, target: &NodeId, count: usize) -> Vec<&PeerEntry> {
        let mut sorted: Vec<_> = self.peers.iter().collect();
        sorted.sort_by_key(|p| p.node_id.distance(target));
        sorted.into_iter().take(count).collect()
    }
}

/// Routing table for Kademlia DHT
#[derive(Debug)]
pub struct RoutingTable {
    /// Local node ID
    local_id: NodeId,
    /// K-buckets indexed by distance
    buckets: Vec<KBucket>,
    /// All known peers (for quick lookup)
    peer_index: HashMap<NodeId, (usize, String)>, // (bucket_index, peer_id)
}

impl RoutingTable {
    /// Create new routing table
    pub fn new(local_id: NodeId) -> Self {
        let mut buckets = Vec::with_capacity(BUCKET_COUNT);
        for _ in 0..BUCKET_COUNT {
            buckets.push(KBucket::new(K_BUCKET_SIZE));
        }

        Self {
            local_id,
            buckets,
            peer_index: HashMap::new(),
        }
    }

    /// Update a peer entry
    pub fn update_peer(&mut self, entry: PeerEntry) -> Option<PeerEntry> {
        let distance = self.local_id.distance(&entry.node_id);

        if let Some(index) = self.local_id.bucket_index(&distance) {
            // Remove from index if already exists (from old bucket)
            if let Some((old_index, _)) = self.peer_index.remove(&entry.node_id) {
                if old_index != index && old_index < self.buckets.len() {
                    // Node moved to different bucket (shouldn't happen often)
                    self.buckets[old_index].remove(&entry.node_id);
                }
            }

            let bucket = &mut self.buckets[index];
            let result = bucket.insert(entry.clone());

            if result.is_none() {
                // Insert successful, update index
                self.peer_index
                    .insert(entry.node_id.clone(), (index, entry.peer_id.clone()));
            }

            result
        } else {
            // Distance is zero (local node), don't add
            None
        }
    }

    /// Remove a peer
    pub fn remove_peer(&mut self, node_id: &NodeId) -> Option<PeerEntry> {
        if let Some((index, _)) = self.peer_index.remove(node_id) {
            self.buckets[index].remove(node_id)
        } else {
            None
        }
    }

    /// Find closest peers to a target node ID
    pub fn find_closest(&self, target: &NodeId, count: usize) -> Vec<&PeerEntry> {
        let distance = self.local_id.distance(target);

        if let Some(index) = self.local_id.bucket_index(&distance) {
            let mut results = Vec::new();

            // Start with the bucket containing the target distance
            results.extend(self.buckets[index].find_closest(target, count));

            // Search neighboring buckets if needed
            let mut offset = 1;
            while results.len() < count && (index >= offset || index + offset < BUCKET_COUNT) {
                if index >= offset {
                    results.extend(
                        self.buckets[index - offset].find_closest(target, count - results.len()),
                    );
                }
                if index + offset < BUCKET_COUNT && results.len() < count {
                    results.extend(
                        self.buckets[index + offset].find_closest(target, count - results.len()),
                    );
                }
                offset += 1;
            }

            // Sort by distance
            results.sort_by_key(|p| p.node_id.distance(target));
            results.truncate(count);
            results
        } else {
            vec![]
        }
    }

    /// Get all peers
    pub fn all_peers(&self) -> Vec<&PeerEntry> {
        let mut peers = Vec::new();
        for bucket in &self.buckets {
            peers.extend(bucket.peers());
        }
        peers
    }

    /// Get peer count
    pub fn peer_count(&self) -> usize {
        self.peer_index.len()
    }

    /// Get bucket stats
    pub fn bucket_stats(&self) -> Vec<(usize, usize)> {
        self.buckets
            .iter()
            .enumerate()
            .map(|(i, b)| (i, b.len()))
            .filter(|(_, len)| *len > 0)
            .collect()
    }

    /// Clean up stale peers
    pub fn cleanup_stale(&mut self) -> usize {
        let mut removed = 0;
        for (i, bucket) in self.buckets.iter_mut().enumerate() {
            let stale_ids: Vec<_> = bucket
                .peers()
                .iter()
                .filter(|p| p.is_stale())
                .map(|p| p.node_id.clone())
                .collect();

            for id in stale_ids {
                bucket.remove(&id);
                self.peer_index.remove(&id);
                removed += 1;
                debug!("Removed stale peer from bucket {}", i);
            }
        }
        removed
    }
}

/// Discovery method
#[derive(Debug, Clone)]
pub enum DiscoveryMethod {
    /// Static list of bootstrap nodes
    Static(Vec<SocketAddr>),
    /// DHT-based discovery
    DHT,
    /// mDNS local discovery
    MDNS,
    /// Manual peer addition
    Manual,
    /// Combined discovery (DHT + mDNS)
    Combined,
}

/// DHT configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhtConfig {
    /// Enable DHT
    pub enabled: bool,
    /// Bootstrap node addresses
    pub bootstrap_nodes: Vec<String>,
    /// K-bucket size
    pub k_bucket_size: usize,
    /// Alpha (concurrent lookups)
    pub alpha: usize,
    /// Refresh interval in seconds
    pub refresh_interval_secs: u64,
    /// Enable mDNS
    pub enable_mdns: bool,
}

impl Default for DhtConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bootstrap_nodes: vec![],
            k_bucket_size: K_BUCKET_SIZE,
            alpha: ALPHA,
            refresh_interval_secs: REFRESH_INTERVAL_SECS,
            enable_mdns: true,
        }
    }
}

/// Discovery event
#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    /// Peer discovered
    PeerDiscovered {
        /// Peer ID
        peer_id: String,
        /// Node ID
        node_id: NodeId,
        /// Peer addresses
        addresses: Vec<SocketAddr>,
    },
    /// Peer updated
    PeerUpdated {
        /// Peer ID
        peer_id: String,
        /// Peer addresses
        addresses: Vec<SocketAddr>,
    },
    /// Peer removed
    PeerRemoved {
        /// Peer ID
        peer_id: String,
        /// Node ID
        node_id: NodeId,
    },
    /// Bootstrap completed
    BootstrapCompleted {
        /// Number of peers found
        peers_found: usize,
    },
}

/// Peer discovery service with DHT support
pub struct DiscoveryService {
    /// Local node ID
    local_id: NodeId,
    /// Discovery method
    method: DiscoveryMethod,
    /// Known peers
    known_peers: Arc<Mutex<HashSet<SocketAddr>>>,
    /// Routing table for DHT
    routing_table: Arc<Mutex<RoutingTable>>,
    /// DHT configuration
    dht_config: DhtConfig,
    /// Bootstrap nodes
    bootstrap_nodes: Vec<String>,
    /// Event callbacks
    event_handlers: Arc<Mutex<Vec<Box<dyn Fn(DiscoveryEvent) + Send>>>>,
    /// Running state
    running: Arc<Mutex<bool>>,
}

impl DiscoveryService {
    /// Create new discovery service
    pub fn new(local_peer_id: &str, method: DiscoveryMethod) -> Self {
        let local_id = NodeId::from_peer_id(local_peer_id);
        let routing_table = RoutingTable::new(local_id.clone());

        Self {
            local_id,
            method,
            known_peers: Arc::new(Mutex::new(HashSet::new())),
            routing_table: Arc::new(Mutex::new(routing_table)),
            dht_config: DhtConfig::default(),
            bootstrap_nodes: vec![],
            event_handlers: Arc::new(Mutex::new(vec![])),
            running: Arc::new(Mutex::new(false)),
        }
    }

    /// Create with DHT configuration
    pub fn with_dht_config(mut self, config: DhtConfig) -> Self {
        self.dht_config = config;
        self
    }

    /// Set bootstrap nodes
    pub fn set_bootstrap_nodes(&mut self, nodes: Vec<String>) {
        self.bootstrap_nodes = nodes;
    }

    /// Add event handler
    pub fn add_event_handler<F>(&self, handler: F)
    where
        F: Fn(DiscoveryEvent) + Send + 'static,
    {
        self.event_handlers.lock().unwrap().push(Box::new(handler));
    }

    /// Emit discovery event
    fn emit_event(&self, event: DiscoveryEvent) {
        let handlers = self.event_handlers.lock().unwrap();
        for handler in handlers.iter() {
            handler(event.clone());
        }
    }

    /// Start discovery service
    pub async fn start(&self) -> Result<()> {
        *self.running.lock().unwrap() = true;

        info!("Starting discovery service with method: {:?}", self.method);

        match self.method {
            DiscoveryMethod::DHT | DiscoveryMethod::Combined => {
                if self.dht_config.enabled {
                    self.start_dht_discovery().await?;
                }
            }
            DiscoveryMethod::Static(ref addrs) => {
                for addr in addrs {
                    self.add_peer(*addr)?;
                }
            }
            _ => {}
        }

        // Start refresh task
        self.start_refresh_task();

        Ok(())
    }

    /// Stop discovery service
    pub async fn stop(&self) -> Result<()> {
        *self.running.lock().unwrap() = false;
        info!("Discovery service stopped");
        Ok(())
    }

    /// Start DHT discovery
    async fn start_dht_discovery(&self) -> Result<()> {
        info!(
            "Starting DHT discovery with {} bootstrap nodes",
            self.bootstrap_nodes.len()
        );

        // Connect to bootstrap nodes
        let mut connected = 0;
        for node_addr in &self.bootstrap_nodes {
            match self.ping_bootstrap_node(node_addr).await {
                Ok(peer_info) => {
                    self.register_peer(peer_info);
                    connected += 1;
                }
                Err(e) => {
                    warn!("Failed to connect to bootstrap node {}: {}", node_addr, e);
                }
            }
        }

        info!(
            "Connected to {}/{} bootstrap nodes",
            connected,
            self.bootstrap_nodes.len()
        );

        self.emit_event(DiscoveryEvent::BootstrapCompleted {
            peers_found: connected,
        });

        Ok(())
    }

    /// Start refresh task
    fn start_refresh_task(&self) {
        let routing_table = self.routing_table.clone();
        let running = self.running.clone();
        let interval_secs = self.dht_config.refresh_interval_secs;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));

            loop {
                interval.tick().await;

                if !*running.lock().unwrap() {
                    break;
                }

                // Clean up stale peers
                let removed = routing_table.lock().unwrap().cleanup_stale();
                if removed > 0 {
                    info!("Cleaned up {} stale peers from routing table", removed);
                }
            }
        });
    }

    /// Ping a bootstrap node
    async fn ping_bootstrap_node(&self, addr: &str) -> Result<PeerEntry> {
        // In a real implementation, this would perform an actual network ping
        // For now, create a mock peer entry
        let socket_addr: SocketAddr = addr
            .parse()
            .map_err(|e| KernelError::invalid_argument(format!("Invalid address: {}", e)))?;

        let peer_id = format!("bootstrap-{}", addr.replace([':', '.'], "-"));
        let node_id = NodeId::from_peer_id(&peer_id);

        Ok(PeerEntry {
            node_id,
            peer_id,
            addresses: vec![socket_addr],
            last_seen: Instant::now(),
            last_successful_query: Some(Instant::now()),
            failed_queries: 0,
            reputation: 50,
            protocols: vec!["/beebotos/dht/1.0.0".to_string()],
            latency_ms: 0,
        })
    }

    /// Register a peer in the routing table
    pub fn register_peer(&self, entry: PeerEntry) {
        let mut table = self.routing_table.lock().unwrap();

        let peer_id = entry.peer_id.clone();
        let node_id = entry.node_id.clone();
        let addresses = entry.addresses.clone();

        if let Some(evicted) = table.update_peer(entry) {
            debug!("Evicted peer {} from routing table", evicted.peer_id);
        }

        // Add to known peers set
        if let Ok(mut known) = self.known_peers.lock() {
            for addr in &addresses {
                known.insert(*addr);
            }
        }

        self.emit_event(DiscoveryEvent::PeerDiscovered {
            peer_id: peer_id.clone(),
            node_id,
            addresses,
        });

        debug!("Registered peer {} in routing table", peer_id);
    }

    /// Add known peer
    pub fn add_peer(&self, addr: SocketAddr) -> Result<bool> {
        let mut peers = self
            .known_peers
            .lock()
            .map_err(|_| KernelError::internal("Failed to acquire lock"))?;
        Ok(peers.insert(addr))
    }

    /// Remove known peer
    pub fn remove_peer(&self, addr: &SocketAddr) -> Result<bool> {
        let mut peers = self
            .known_peers
            .lock()
            .map_err(|_| KernelError::internal("Failed to acquire lock"))?;
        Ok(peers.remove(addr))
    }

    /// Get known peers
    pub fn get_peers(&self) -> Result<Vec<SocketAddr>> {
        let peers = self
            .known_peers
            .lock()
            .map_err(|_| KernelError::internal("Failed to acquire lock"))?;
        Ok(peers.iter().copied().collect())
    }

    /// Find peers closest to a target
    pub fn find_closest_peers(&self, target: &NodeId, count: usize) -> Vec<PeerEntry> {
        let table = self.routing_table.lock().unwrap();
        table
            .find_closest(target, count)
            .into_iter()
            .cloned()
            .collect()
    }

    /// Get random peers from routing table
    pub fn get_random_peers(&self, count: usize) -> Vec<PeerEntry> {
        use rand::seq::SliceRandom;

        let table = self.routing_table.lock().unwrap();
        let mut all_peers: Vec<_> = table
            .all_peers()
            .into_iter()
            .filter(|p| p.is_healthy())
            .cloned()
            .collect();

        all_peers.shuffle(&mut rand::thread_rng());
        all_peers.truncate(count);
        all_peers
    }

    /// Discover peers - perform DHT lookup
    pub async fn discover(&self) -> Result<Vec<PeerInfo>> {
        let table = self.routing_table.lock().unwrap();
        let peers: Vec<PeerInfo> = table
            .all_peers()
            .into_iter()
            .filter(|p| p.is_healthy())
            .map(|p| PeerInfo {
                peer_id: p.peer_id.clone(),
                addresses: p.addresses.iter().map(|a| a.to_string()).collect(),
                protocols: p.protocols.clone(),
                connected_since: p.last_seen.elapsed().as_secs(),
                latency_ms: p.latency_ms,
                reputation: p.reputation,
            })
            .collect();

        Ok(peers)
    }

    /// Perform iterative DHT lookup for a target
    pub async fn lookup(&self, target: &NodeId) -> Vec<PeerEntry> {
        let mut queried = HashSet::new();
        let closest = self.find_closest_peers(target, self.dht_config.alpha);

        for peer in &closest {
            if queried.insert(peer.node_id.clone()) {
                // In a real implementation, send FIND_NODE RPC
                debug!(
                    "Querying peer {} for target {}",
                    peer.peer_id,
                    target.to_hex()
                );
            }
        }

        closest
    }

    /// Get routing table statistics
    pub fn routing_table_stats(&self) -> RoutingTableStats {
        let table = self.routing_table.lock().unwrap();
        RoutingTableStats {
            total_peers: table.peer_count(),
            bucket_distribution: table.bucket_stats(),
        }
    }

    /// Get local node ID
    pub fn local_node_id(&self) -> &NodeId {
        &self.local_id
    }
}

impl Default for DiscoveryService {
    fn default() -> Self {
        Self::new("default-peer", DiscoveryMethod::Manual)
    }
}

/// Routing table statistics
#[derive(Debug, Clone)]
pub struct RoutingTableStats {
    /// Total number of peers in routing table
    pub total_peers: usize,
    /// Distribution of peers across buckets
    pub bucket_distribution: Vec<(usize, usize)>, // (bucket_index, peer_count)
}

/// Bootstrap node configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapConfig {
    /// List of bootstrap node addresses
    pub addresses: Vec<String>,
    /// Connection timeout in seconds
    pub timeout_secs: u64,
    /// Retry interval in seconds
    pub retry_interval_secs: u64,
    /// Maximum retry attempts
    pub max_retries: u32,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            addresses: vec![],
            timeout_secs: 30,
            retry_interval_secs: 60,
            max_retries: 3,
        }
    }
}

/// mDNS discovery service for local network
#[derive(Debug)]
pub struct MdnsDiscovery {
    /// Service name
    service_name: String,
    /// Local port (reserved for future use)
    #[allow(dead_code)]
    local_port: u16,
    /// Discovered peers
    discovered_peers: Arc<Mutex<HashMap<String, Vec<SocketAddr>>>>,
}

impl MdnsDiscovery {
    /// Create new mDNS discovery
    pub fn new(service_name: impl Into<String>, local_port: u16) -> Self {
        Self {
            service_name: service_name.into(),
            local_port,
            discovered_peers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start mDNS discovery (placeholder)
    pub async fn start(&self) -> Result<()> {
        info!("Starting mDNS discovery for service: {}", self.service_name);
        // In a real implementation, this would start an mDNS client
        Ok(())
    }

    /// Stop mDNS discovery
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping mDNS discovery");
        Ok(())
    }

    /// Get discovered peers
    pub fn get_discovered_peers(&self) -> HashMap<String, Vec<SocketAddr>> {
        self.discovered_peers.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_id_distance() {
        let id1 = NodeId([0u8; 20]);
        let id2 = NodeId([1u8; 20]);

        let distance = id1.distance(&id2);
        assert!(!distance.is_zero());

        let distance_self = id1.distance(&id1);
        assert!(distance_self.is_zero());
    }

    #[test]
    fn test_routing_table() {
        let local_id = NodeId::random();
        let mut table = RoutingTable::new(local_id);

        // Add some peers
        for i in 0..5 {
            let entry = PeerEntry {
                node_id: NodeId::random(),
                peer_id: format!("peer-{}", i),
                addresses: vec![format!("127.0.0.1:{}", 4000 + i).parse().unwrap()],
                last_seen: Instant::now(),
                last_successful_query: Some(Instant::now()),
                failed_queries: 0,
                reputation: 0,
                protocols: vec![],
                latency_ms: 10,
            };
            table.update_peer(entry);
        }

        assert_eq!(table.peer_count(), 5);

        // Find closest
        let target = NodeId::random();
        let closest = table.find_closest(&target, 3);
        assert_eq!(closest.len(), 3);
    }

    #[test]
    fn test_k_bucket() {
        let mut bucket = KBucket::new(3);

        // Add peers
        for i in 0..5 {
            let entry = PeerEntry {
                node_id: NodeId::random(),
                peer_id: format!("peer-{}", i),
                addresses: vec![],
                last_seen: Instant::now(),
                last_successful_query: None,
                failed_queries: 0,
                reputation: 0,
                protocols: vec![],
                latency_ms: 0,
            };

            if i < 3 {
                assert!(bucket.insert(entry).is_none());
            } else {
                // Bucket is full, should return the new entry
                assert!(bucket.insert(entry).is_some());
            }
        }

        assert_eq!(bucket.len(), 3);
    }

    #[test]
    fn test_discovery_service() {
        let service = DiscoveryService::new("test-peer", DiscoveryMethod::DHT);
        assert_eq!(service.routing_table.lock().unwrap().peer_count(), 0);

        // Register a peer
        let entry = PeerEntry {
            node_id: NodeId::random(),
            peer_id: "test-peer-1".to_string(),
            addresses: vec!["127.0.0.1:4001".parse().unwrap()],
            last_seen: Instant::now(),
            last_successful_query: Some(Instant::now()),
            failed_queries: 0,
            reputation: 50,
            protocols: vec!["test".to_string()],
            latency_ms: 5,
        };

        service.register_peer(entry);
        assert_eq!(service.routing_table.lock().unwrap().peer_count(), 1);
    }
}
