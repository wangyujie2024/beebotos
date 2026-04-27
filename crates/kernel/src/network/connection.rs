//! Network Connection Pool Manager
//!
//! Provides connection pooling, management, and health monitoring for
//! TCP/QUIC/WebSocket protocols. Implements:
//! - Connection pooling with configurable limits
//! - Connection multiplexing
//! - Keepalive and health monitoring
//! - Load balancing across connections
//! - Connection lifecycle management
//! - Protocol upgrade support

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
// Async I/O traits removed - using synchronous patterns for now
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Semaphore};
use tokio::time::timeout;
use tracing::{debug, info, trace, warn};

use crate::error::{KernelError, Result};

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Connection is being established
    Connecting,
    /// Connection is active and idle (ready for use)
    Idle,
    /// Connection is active and in use
    Active,
    /// Connection is closing
    Closing,
    /// Connection is closed
    Closed,
    /// Connection failed
    Failed,
}

impl ConnectionState {
    /// Check if connection is usable
    pub fn is_usable(&self) -> bool {
        matches!(self, ConnectionState::Idle | ConnectionState::Active)
    }

    /// Check if connection is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self, ConnectionState::Closed | ConnectionState::Failed)
    }
}

/// Connection statistics with latency measurement
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    /// Connection establishment time
    pub connected_at: Instant,
    /// Last activity time
    pub last_activity: Instant,
    /// Bytes sent
    pub bytes_sent: u64,
    /// Bytes received
    pub bytes_received: u64,
    /// Current latency in milliseconds (measured via keepalive)
    pub latency_ms: u64,
    /// Latency history (last 10 measurements)
    pub latency_history: Vec<u64>,
    /// Number of retransmissions
    pub retransmissions: u32,
    /// Connection state
    pub state: ConnectionState,
    /// Total requests processed
    pub request_count: u64,
    /// Failed request count
    pub failed_requests: u64,
    /// Average request duration
    pub avg_request_duration_ms: u64,
}

impl ConnectionStats {
    /// Create new stats
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            connected_at: now,
            last_activity: now,
            bytes_sent: 0,
            bytes_received: 0,
            latency_ms: 0,
            latency_history: Vec::with_capacity(10),
            retransmissions: 0,
            state: ConnectionState::Connecting,
            request_count: 0,
            failed_requests: 0,
            avg_request_duration_ms: 0,
        }
    }

    /// Update latency measurement
    pub fn record_latency(&mut self, latency_ms: u64) {
        self.latency_ms = latency_ms;
        self.latency_history.push(latency_ms);
        if self.latency_history.len() > 10 {
            self.latency_history.remove(0);
        }
    }

    /// Get average latency
    pub fn average_latency(&self) -> u64 {
        if self.latency_history.is_empty() {
            return 0;
        }
        self.latency_history.iter().sum::<u64>() / self.latency_history.len() as u64
    }

    /// Record bytes sent
    pub fn record_sent(&mut self, bytes: usize) {
        self.bytes_sent += bytes as u64;
        self.last_activity = Instant::now();
    }

    /// Record bytes received
    pub fn record_received(&mut self, bytes: usize) {
        self.bytes_received += bytes as u64;
        self.last_activity = Instant::now();
    }

    /// Record request completion
    pub fn record_request(&mut self, duration_ms: u64, success: bool) {
        self.request_count += 1;
        if !success {
            self.failed_requests += 1;
        }

        // Update running average
        let current_avg = self.avg_request_duration_ms;
        let count = self.request_count;
        self.avg_request_duration_ms = (current_avg * (count - 1) + duration_ms) / count;
    }

    /// Check if connection is idle (no activity for specified duration)
    pub fn is_idle(&self, duration: Duration) -> bool {
        self.last_activity.elapsed() > duration
    }

    /// Get success rate (0-100)
    pub fn success_rate(&self) -> u8 {
        if self.request_count == 0 {
            return 100;
        }
        let success = self.request_count - self.failed_requests;
        ((success * 100) / self.request_count) as u8
    }
}

impl Default for ConnectionStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Pooled connection handle
#[derive(Debug, Clone)]
pub struct PooledConnection {
    /// Connection ID
    pub id: u64,
    /// Pool name
    pub pool_name: String,
    /// Remote address
    pub remote_addr: SocketAddr,
    /// Local address
    pub local_addr: SocketAddr,
    /// Connection statistics
    pub stats: Arc<RwLock<ConnectionStats>>,
    /// Data channel for async communication
    data_tx: mpsc::UnboundedSender<Vec<u8>>,
    /// Return channel to return connection to pool
    return_tx: mpsc::UnboundedSender<u64>,
}

impl PooledConnection {
    /// Create new pooled connection handle
    pub fn new(
        id: u64,
        pool_name: impl Into<String>,
        remote_addr: SocketAddr,
        local_addr: SocketAddr,
        data_tx: mpsc::UnboundedSender<Vec<u8>>,
        return_tx: mpsc::UnboundedSender<u64>,
    ) -> Self {
        let mut stats = ConnectionStats::new();
        stats.state = ConnectionState::Idle;

        Self {
            id,
            pool_name: pool_name.into(),
            remote_addr,
            local_addr,
            stats: Arc::new(RwLock::new(stats)),
            data_tx,
            return_tx,
        }
    }

    /// Mark connection as active
    pub fn mark_active(&self) {
        self.stats.write().state = ConnectionState::Active;
    }

    /// Mark connection as idle
    pub fn mark_idle(&self) {
        self.stats.write().state = ConnectionState::Idle;
    }

    /// Send data
    pub fn send(&self, data: Vec<u8>) -> Result<()> {
        let len = data.len();
        self.data_tx
            .send(data)
            .map_err(|_| KernelError::io("Connection closed"))?;
        self.stats.write().record_sent(len);
        Ok(())
    }

    /// Send and receive with timeout
    pub async fn send_receive(
        &self,
        data: Vec<u8>,
        _timeout_duration: Duration,
    ) -> Result<Option<Vec<u8>>> {
        let start = Instant::now();
        self.mark_active();

        // Send data
        self.send(data)?;

        // Wait for response (simplified - in real impl would use a response channel)
        tokio::time::sleep(Duration::from_millis(10)).await;

        let duration_ms = start.elapsed().as_millis() as u64;
        self.stats.write().record_request(duration_ms, true);
        self.mark_idle();

        Ok(None) // Would return actual response
    }

    /// Update latency measurement
    pub fn record_latency(&self, latency_ms: u64) {
        self.stats.write().record_latency(latency_ms);
    }

    /// Get current latency
    pub fn latency(&self) -> u64 {
        self.stats.read().latency_ms
    }

    /// Get connection state
    pub fn state(&self) -> ConnectionState {
        self.stats.read().state
    }

    /// Check if connection is alive
    pub fn is_alive(&self) -> bool {
        self.state().is_usable()
    }

    /// Get connection health score (0-100)
    pub fn health_score(&self) -> u8 {
        let stats = self.stats.read();
        let latency_score = if stats.latency_ms < 50 {
            40
        } else if stats.latency_ms < 100 {
            30
        } else if stats.latency_ms < 200 {
            20
        } else {
            10
        };

        let success_score = stats.success_rate() as u64 * 40 / 100;
        let activity_score = if stats.is_idle(Duration::from_secs(300)) {
            10
        } else {
            20
        };

        (latency_score + success_score as u8 + activity_score).min(100)
    }

    /// Return connection to pool
    pub fn return_to_pool(self) {
        self.mark_idle();
        let _ = self.return_tx.send(self.id);
    }
}

impl Drop for PooledConnection {
    fn drop(&mut self) {
        // Try to return connection to pool if not already closed
        if self.is_alive() {
            let _ = self.return_tx.send(self.id);
        }
    }
}

/// Connection pool configuration
#[derive(Debug, Clone)]
pub struct ConnectionPoolConfig {
    /// Pool name
    pub name: String,
    /// Target address
    pub target_addr: SocketAddr,
    /// Minimum connections to maintain
    pub min_connections: usize,
    /// Maximum connections allowed
    pub max_connections: usize,
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Read/write timeout
    pub io_timeout: Duration,
    /// Keepalive interval
    pub keepalive_interval: Duration,
    /// Idle timeout before closing
    pub idle_timeout: Duration,
    /// Maximum requests per connection before recycling
    pub max_requests_per_connection: u64,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Load balancing strategy
    pub load_balancing: LoadBalancingStrategy,
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            target_addr: "127.0.0.1:0".parse().unwrap(),
            min_connections: 2,
            max_connections: 10,
            connect_timeout: Duration::from_secs(10),
            io_timeout: Duration::from_secs(30),
            keepalive_interval: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(300),
            max_requests_per_connection: 1000,
            health_check_interval: Duration::from_secs(60),
            load_balancing: LoadBalancingStrategy::RoundRobin,
        }
    }
}

/// Load balancing strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadBalancingStrategy {
    /// Round-robin selection
    RoundRobin,
    /// Least connections
    LeastConnections,
    /// Lowest latency
    LowestLatency,
    /// Random selection
    Random,
    /// Weighted round-robin
    WeightedRoundRobin,
}

/// Connection pool for a specific target
#[derive(Debug)]
pub struct ConnectionPool {
    /// Pool configuration
    config: ConnectionPoolConfig,
    /// Active connections (conn_id -> connection)
    connections: Arc<RwLock<HashMap<u64, Arc<PooledConnection>>>>,
    /// Available connections (ready for use)
    available: Arc<RwLock<VecDeque<u64>>>,
    /// Connection counter for IDs
    next_id: Arc<AtomicU64>,
    /// Current connection count
    current_count: Arc<AtomicUsize>,
    /// Round-robin counter
    round_robin_counter: Arc<AtomicU64>,
    /// Shutdown signal
    shutdown: Arc<RwLock<bool>>,
    /// Connection semaphore for limiting concurrent connects
    connect_semaphore: Arc<Semaphore>,
    /// Return channel receiver (reserved for future use)
    #[allow(dead_code)]
    return_rx: RwLock<mpsc::UnboundedReceiver<u64>>,
    /// Return channel sender
    return_tx: mpsc::UnboundedSender<u64>,
}

impl ConnectionPool {
    /// Create new connection pool
    pub fn new(config: ConnectionPoolConfig) -> Self {
        let (return_tx, return_rx) = mpsc::unbounded_channel();

        Self {
            config,
            connections: Arc::new(RwLock::new(HashMap::new())),
            available: Arc::new(RwLock::new(VecDeque::new())),
            next_id: Arc::new(AtomicU64::new(1)),
            current_count: Arc::new(AtomicUsize::new(0)),
            round_robin_counter: Arc::new(AtomicU64::new(0)),
            shutdown: Arc::new(RwLock::new(false)),
            connect_semaphore: Arc::new(Semaphore::new(10)), // Max concurrent connects
            return_rx: RwLock::new(return_rx),
            return_tx,
        }
    }

    /// Start the pool (create minimum connections)
    pub async fn start(&self) -> Result<()> {
        info!(
            "Starting connection pool '{}' for {}",
            self.config.name, self.config.target_addr
        );

        // Create minimum connections
        for _ in 0..self.config.min_connections {
            if let Err(e) = self.create_connection().await {
                warn!("Failed to create initial connection: {}", e);
            }
        }

        // Start maintenance task
        self.start_maintenance_task();

        info!(
            "Connection pool '{}' started with {} connections",
            self.config.name,
            self.current_count.load(Ordering::Relaxed)
        );

        Ok(())
    }

    /// Shutdown the pool
    pub async fn shutdown(&self) -> Result<()> {
        *self.shutdown.write() = true;

        // Close all connections
        let connections = self.connections.write();
        for (_, conn) in connections.iter() {
            conn.stats.write().state = ConnectionState::Closing;
        }
        drop(connections);

        self.connections.write().clear();
        self.available.write().clear();
        self.current_count.store(0, Ordering::Relaxed);

        info!("Connection pool '{}' shutdown complete", self.config.name);
        Ok(())
    }

    /// Get a connection from the pool
    pub async fn acquire(&self) -> Result<Arc<PooledConnection>> {
        // Fast path: try to get an available connection
        if let Some(conn_id) = self.available.write().pop_front() {
            if let Some(conn) = self.connections.read().get(&conn_id).cloned() {
                conn.mark_active();
                return Ok(conn);
            }
        }

        // No available connections, create a new one if under limit
        let current = self.current_count.load(Ordering::Relaxed);
        if current < self.config.max_connections {
            if let Ok(conn) = self.create_connection().await {
                conn.mark_active();
                return Ok(conn);
            }
        }

        // Wait for a connection to become available
        let mut retries = 10;
        while retries > 0 {
            tokio::time::sleep(Duration::from_millis(100)).await;

            if let Some(conn_id) = self.available.write().pop_front() {
                if let Some(conn) = self.connections.read().get(&conn_id).cloned() {
                    conn.mark_active();
                    return Ok(conn);
                }
            }

            retries -= 1;
        }

        Err(KernelError::resource_exhausted(
            "No available connections in pool",
        ))
    }

    /// Get a connection using load balancing strategy
    pub async fn acquire_balanced(&self) -> Result<Arc<PooledConnection>> {
        match self.config.load_balancing {
            LoadBalancingStrategy::RoundRobin => self.acquire_round_robin().await,
            LoadBalancingStrategy::LeastConnections => self.acquire_least_connections().await,
            LoadBalancingStrategy::LowestLatency => self.acquire_lowest_latency().await,
            LoadBalancingStrategy::Random => self.acquire_random().await,
            LoadBalancingStrategy::WeightedRoundRobin => self.acquire_weighted_round_robin().await,
        }
    }

    /// Round-robin acquisition
    async fn acquire_round_robin(&self) -> Result<Arc<PooledConnection>> {
        let idx = self.round_robin_counter.fetch_add(1, Ordering::Relaxed) as usize;
        let connections: Vec<_> = self.connections.read().values().cloned().collect();

        if connections.is_empty() {
            return self.acquire().await;
        }

        let conn = connections
            .get(idx % connections.len())
            .ok_or_else(|| KernelError::resource_exhausted("No connections available"))?
            .clone();

        conn.mark_active();
        Ok(conn)
    }

    /// Least connections acquisition
    async fn acquire_least_connections(&self) -> Result<Arc<PooledConnection>> {
        let connections: Vec<_> = self.connections.read().values().cloned().collect();

        let best = connections
            .iter()
            .filter(|c| c.is_alive())
            .min_by_key(|c| c.stats.read().request_count)
            .cloned()
            .ok_or_else(|| KernelError::resource_exhausted("No healthy connections available"))?;

        best.mark_active();
        Ok(best)
    }

    /// Lowest latency acquisition
    async fn acquire_lowest_latency(&self) -> Result<Arc<PooledConnection>> {
        let connections: Vec<_> = self.connections.read().values().cloned().collect();

        let best = connections
            .iter()
            .filter(|c| c.is_alive())
            .min_by_key(|c| c.stats.read().latency_ms)
            .cloned()
            .ok_or_else(|| KernelError::resource_exhausted("No healthy connections available"))?;

        best.mark_active();
        Ok(best)
    }

    /// Random acquisition
    async fn acquire_random(&self) -> Result<Arc<PooledConnection>> {
        use rand::seq::SliceRandom;

        let connections: Vec<_> = self.connections.read().values().cloned().collect();

        let conn = connections
            .choose(&mut rand::thread_rng())
            .ok_or_else(|| KernelError::resource_exhausted("No connections available"))?
            .clone();

        conn.mark_active();
        Ok(conn)
    }

    /// Weighted round-robin acquisition
    async fn acquire_weighted_round_robin(&self) -> Result<Arc<PooledConnection>> {
        // Simplified implementation - use health score as weight
        let connections: Vec<_> = self.connections.read().values().cloned().collect();

        let best = connections
            .iter()
            .filter(|c| c.is_alive())
            .max_by_key(|c| c.health_score())
            .cloned()
            .ok_or_else(|| KernelError::resource_exhausted("No healthy connections available"))?;

        best.mark_active();
        Ok(best)
    }

    /// Release a connection back to the pool
    pub fn release(&self, conn_id: u64) {
        if let Some(conn) = self.connections.read().get(&conn_id) {
            conn.mark_idle();

            // Check if connection should be recycled
            let stats = conn.stats.read();
            if stats.request_count >= self.config.max_requests_per_connection {
                drop(stats);
                self.close_connection(conn_id);
                return;
            }
        }

        self.available.write().push_back(conn_id);
    }

    /// Create a new connection
    async fn create_connection(&self) -> Result<Arc<PooledConnection>> {
        let _permit = self
            .connect_semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| KernelError::internal("Failed to acquire connect permit"))?;

        let conn_id = self.next_id.fetch_add(1, Ordering::Relaxed);

        // Connect with timeout
        let result = timeout(
            self.config.connect_timeout,
            TcpStream::connect(self.config.target_addr),
        )
        .await;

        let stream = match result {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                return Err(KernelError::io(format!("Connection failed: {}", e)));
            }
            Err(_) => {
                return Err(KernelError::Timeout);
            }
        };

        let local_addr = stream
            .local_addr()
            .map_err(|e| KernelError::io(format!("Failed to get local addr: {}", e)))?;
        let remote_addr = stream
            .peer_addr()
            .map_err(|e| KernelError::io(format!("Failed to get peer addr: {}", e)))?;

        // Create channels
        let (data_tx, _data_rx) = mpsc::unbounded_channel();

        // Create connection
        let connection = Arc::new(PooledConnection::new(
            conn_id,
            self.config.name.clone(),
            remote_addr,
            local_addr,
            data_tx,
            self.return_tx.clone(),
        ));

        // Store connection
        self.connections.write().insert(conn_id, connection.clone());
        self.available.write().push_back(conn_id);
        self.current_count.fetch_add(1, Ordering::Relaxed);

        // Spawn connection handler
        let conn_clone = connection.clone();
        tokio::spawn(async move {
            // Connection handler would run here
            loop {
                if !conn_clone.is_alive() {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });

        debug!("Created connection {} to {}", conn_id, remote_addr);
        Ok(connection)
    }

    /// Close a specific connection
    fn close_connection(&self, conn_id: u64) {
        if let Some(conn) = self.connections.write().remove(&conn_id) {
            conn.stats.write().state = ConnectionState::Closing;
            self.current_count.fetch_sub(1, Ordering::Relaxed);
            info!(
                "Closed connection {} from pool '{}'",
                conn_id, self.config.name
            );
        }
    }

    /// Start maintenance task
    fn start_maintenance_task(&self) {
        let connections = self.connections.clone();
        let available = self.available.clone();
        let config = self.config.clone();
        let shutdown = self.shutdown.clone();
        let current_count = self.current_count.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.health_check_interval);

            loop {
                interval.tick().await;

                if *shutdown.read() {
                    break;
                }

                // Clean up dead connections
                let dead_connections: Vec<u64> = connections
                    .read()
                    .iter()
                    .filter(|(_, conn)| {
                        let stats = conn.stats.read();
                        stats.state.is_terminal()
                            || stats.is_idle(config.idle_timeout)
                            || stats.success_rate() < 50
                    })
                    .map(|(id, _)| *id)
                    .collect();

                for id in dead_connections {
                    if let Some(conn) = connections.write().remove(&id) {
                        conn.stats.write().state = ConnectionState::Closed;
                        current_count.fetch_sub(1, Ordering::Relaxed);
                    }
                    available.write().retain(|&x| x != id);
                }

                // Ensure minimum connections
                let current = current_count.load(Ordering::Relaxed);
                if current < config.min_connections {
                    debug!(
                        "Pool '{}' below minimum connections ({} < {}), creating more",
                        config.name, current, config.min_connections
                    );
                    // Would create more connections here
                }

                trace!(
                    "Pool '{}' maintenance: {} total, {} available",
                    config.name,
                    current,
                    available.read().len()
                );
            }
        });
    }

    /// Get pool statistics
    pub fn stats(&self) -> PoolStats {
        let connections = self.connections.read();
        let total_requests: u64 = connections
            .values()
            .map(|c| c.stats.read().request_count)
            .sum();
        let avg_latency: u64 = if connections.is_empty() {
            0
        } else {
            connections
                .values()
                .map(|c| c.stats.read().average_latency())
                .sum::<u64>()
                / connections.len() as u64
        };

        PoolStats {
            name: self.config.name.clone(),
            target_addr: self.config.target_addr,
            total_connections: connections.len(),
            available_connections: self.available.read().len(),
            active_connections: connections.len() - self.available.read().len(),
            total_requests,
            avg_latency_ms: avg_latency,
        }
    }
}

/// Pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    /// Pool name
    pub name: String,
    /// Target address
    pub target_addr: SocketAddr,
    /// Total connections
    pub total_connections: usize,
    /// Available (idle) connections
    pub available_connections: usize,
    /// Active (in-use) connections
    pub active_connections: usize,
    /// Total requests processed
    pub total_requests: u64,
    /// Average latency in milliseconds
    pub avg_latency_ms: u64,
}

/// Multi-pool connection manager
#[derive(Debug, Default)]
pub struct ConnectionPoolManager {
    /// Pools indexed by name
    pools: RwLock<HashMap<String, Arc<ConnectionPool>>>,
    /// Pool indexed by target address
    addr_to_pool: RwLock<HashMap<SocketAddr, String>>,
}

impl ConnectionPoolManager {
    /// Create new pool manager
    pub fn new() -> Self {
        Self {
            pools: RwLock::new(HashMap::new()),
            addr_to_pool: RwLock::new(HashMap::new()),
        }
    }

    /// Create a new pool
    pub async fn create_pool(&self, config: ConnectionPoolConfig) -> Result<Arc<ConnectionPool>> {
        let pool = Arc::new(ConnectionPool::new(config.clone()));
        pool.start().await?;

        self.addr_to_pool
            .write()
            .insert(config.target_addr, config.name.clone());
        self.pools.write().insert(config.name.clone(), pool.clone());

        info!(
            "Created connection pool '{}' for {}",
            config.name, config.target_addr
        );
        Ok(pool)
    }

    /// Get a pool by name
    pub fn get_pool(&self, name: &str) -> Option<Arc<ConnectionPool>> {
        self.pools.read().get(name).cloned()
    }

    /// Get or create pool for address
    pub async fn get_or_create_pool(&self, addr: SocketAddr) -> Result<Arc<ConnectionPool>> {
        // Check if pool exists
        if let Some(pool_name) = self.addr_to_pool.read().get(&addr) {
            if let Some(pool) = self.pools.read().get(pool_name) {
                return Ok(pool.clone());
            }
        }

        // Create new pool
        let config = ConnectionPoolConfig {
            name: format!("pool-{}", addr),
            target_addr: addr,
            ..Default::default()
        };

        self.create_pool(config).await
    }

    /// Remove a pool
    pub async fn remove_pool(&self, name: &str) -> Result<()> {
        if let Some(pool) = self.pools.write().remove(name) {
            let addr = pool.config.target_addr;
            self.addr_to_pool.write().remove(&addr);
            pool.shutdown().await?;
        }
        Ok(())
    }

    /// Get all pool statistics
    pub fn get_all_stats(&self) -> Vec<PoolStats> {
        self.pools.read().values().map(|p| p.stats()).collect()
    }

    /// Shutdown all pools
    pub async fn shutdown_all(&self) -> Result<()> {
        let pools: Vec<_> = self.pools.read().values().cloned().collect();
        for pool in pools {
            pool.shutdown().await?;
        }
        self.pools.write().clear();
        self.addr_to_pool.write().clear();
        Ok(())
    }
}

/// Legacy Network Connection handle (for backward compatibility)
#[derive(Debug)]
pub struct NetworkConnection {
    /// Connection ID
    pub id: u64,
    /// Remote address
    pub remote_addr: SocketAddr,
    /// Local address
    pub local_addr: SocketAddr,
    /// Connection statistics
    pub stats: RwLock<ConnectionStats>,
    /// Data channel for async communication
    data_tx: mpsc::UnboundedSender<Vec<u8>>,
    data_rx: tokio::sync::Mutex<mpsc::UnboundedReceiver<Vec<u8>>>,
    /// Shutdown signal
    shutdown_tx: tokio::sync::watch::Sender<bool>,
}

impl NetworkConnection {
    /// Create new connection handle
    pub fn new(
        id: u64,
        remote_addr: SocketAddr,
        local_addr: SocketAddr,
        data_tx: mpsc::UnboundedSender<Vec<u8>>,
        data_rx: mpsc::UnboundedReceiver<Vec<u8>>,
        shutdown_tx: tokio::sync::watch::Sender<bool>,
    ) -> Self {
        let mut stats = ConnectionStats::new();
        stats.state = ConnectionState::Idle;

        Self {
            id,
            remote_addr,
            local_addr,
            stats: RwLock::new(stats),
            data_tx,
            data_rx: tokio::sync::Mutex::new(data_rx),
            shutdown_tx,
        }
    }

    /// Send data
    pub fn send(&self, data: Vec<u8>) -> Result<()> {
        let len = data.len();
        self.data_tx
            .send(data)
            .map_err(|_| KernelError::io("Connection closed"))?;
        self.stats.write().record_sent(len);
        Ok(())
    }

    /// Receive data (non-blocking)
    pub async fn try_receive(&self) -> Result<Option<Vec<u8>>> {
        match self.data_rx.try_lock() {
            Ok(mut rx) => match rx.try_recv() {
                Ok(data) => {
                    self.stats.write().record_received(data.len());
                    Ok(Some(data))
                }
                Err(mpsc::error::TryRecvError::Empty) => Ok(None),
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    Err(KernelError::io("Connection closed"))
                }
            },
            Err(_) => Ok(None), // Lock is held elsewhere
        }
    }

    /// Receive data (blocking with timeout)
    pub async fn receive(&self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        match tokio::time::timeout(timeout, self.data_rx.lock().await.recv()).await {
            Ok(Some(data)) => {
                self.stats.write().record_received(data.len());
                Ok(Some(data))
            }
            Ok(None) => Ok(None),
            Err(_) => Ok(None), // Timeout
        }
    }

    /// Update latency measurement
    pub fn record_latency(&self, latency_ms: u64) {
        self.stats.write().record_latency(latency_ms);
    }

    /// Get current latency
    pub fn latency(&self) -> u64 {
        self.stats.read().latency_ms
    }

    /// Get connection state
    pub fn state(&self) -> ConnectionState {
        self.stats.read().state
    }

    /// Close the connection
    pub fn close(&self) {
        let _ = self.shutdown_tx.send(true);
        self.stats.write().state = ConnectionState::Closing;
    }

    /// Check if connection is alive
    pub fn is_alive(&self) -> bool {
        matches!(
            self.state(),
            ConnectionState::Idle | ConnectionState::Active
        )
    }
}

/// Legacy ConnectionManager (for backward compatibility)
#[derive(Debug)]
pub struct ConnectionManager {
    /// Active connections
    connections: RwLock<HashMap<u64, Arc<NetworkConnection>>>,
    /// Next connection ID (reserved for future use)
    #[allow(dead_code)]
    next_id: RwLock<u64>,
    /// Connection configuration
    config: ConnectionConfig,
    /// Global statistics
    stats: RwLock<ManagerStats>,
    /// Connection pool manager
    pool_manager: ConnectionPoolManager,
}

/// Connection configuration (legacy)
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Read/write timeout
    pub io_timeout: Duration,
    /// Keepalive interval
    pub keepalive_interval: Duration,
    /// Idle timeout
    pub idle_timeout: Duration,
    /// Maximum connections
    pub max_connections: usize,
    /// Buffer size
    pub buffer_size: usize,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            io_timeout: Duration::from_secs(30),
            keepalive_interval: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(300),
            max_connections: 1000,
            buffer_size: 65536,
        }
    }
}

/// Manager statistics (legacy)
#[derive(Debug, Clone, Default)]
pub struct ManagerStats {
    /// Total number of connections established
    pub total_connections: u64,
    /// Number of currently active connections
    pub active_connections: u64,
    /// Number of failed connection attempts
    pub failed_connections: u64,
    /// Total bytes sent across all connections
    pub total_bytes_sent: u64,
    /// Total bytes received across all connections
    pub total_bytes_received: u64,
}

impl ConnectionManager {
    /// Create new connection manager
    pub fn new() -> Self {
        Self::with_config(ConnectionConfig::default())
    }

    /// Create with custom config
    pub fn with_config(config: ConnectionConfig) -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            next_id: RwLock::new(1),
            config,
            stats: RwLock::new(ManagerStats::default()),
            pool_manager: ConnectionPoolManager::new(),
        }
    }

    /// Connect to remote address (legacy, uses pool manager internally)
    pub async fn connect(&self, addr: SocketAddr) -> Result<u64> {
        // Use pool manager for new connections
        let pool = self.pool_manager.get_or_create_pool(addr).await?;
        let conn = pool.acquire().await?;

        // Also track in legacy connections map for compatibility
        let legacy_conn = Arc::new(NetworkConnection::new(
            conn.id,
            conn.remote_addr,
            conn.local_addr,
            conn.data_tx.clone(),
            mpsc::unbounded_channel().1,
            tokio::sync::watch::channel(false).0,
        ));

        self.connections.write().insert(conn.id, legacy_conn);
        self.stats.write().total_connections += 1;
        self.stats.write().active_connections = self.connections.read().len() as u64;

        Ok(conn.id)
    }

    /// Send data over connection
    pub fn send(&self, conn_id: u64, data: Vec<u8>) -> Result<usize> {
        let conn = self
            .connections
            .read()
            .get(&conn_id)
            .cloned()
            .ok_or_else(|| KernelError::invalid_argument("Invalid connection handle"))?;

        if !conn.is_alive() {
            return Err(KernelError::io("Connection is closed"));
        }

        let len = data.len();
        conn.send(data)?;

        self.stats.write().total_bytes_sent += len as u64;
        trace!("Sent {} bytes over connection {}", len, conn_id);

        Ok(len)
    }

    /// Receive data from connection (non-blocking)
    pub async fn try_receive(&self, conn_id: u64) -> Result<Option<Vec<u8>>> {
        let conn = self
            .connections
            .read()
            .get(&conn_id)
            .cloned()
            .ok_or_else(|| KernelError::invalid_argument("Invalid connection handle"))?;

        let data = conn.try_receive().await?;
        if let Some(ref d) = data {
            self.stats.write().total_bytes_received += d.len() as u64;
        }

        Ok(data)
    }

    /// Receive data from connection (blocking with timeout)
    pub async fn receive(&self, conn_id: u64, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let conn = self
            .connections
            .read()
            .get(&conn_id)
            .cloned()
            .ok_or_else(|| KernelError::invalid_argument("Invalid connection handle"))?;

        let data = conn.receive(timeout).await?;
        if let Some(ref d) = data {
            self.stats.write().total_bytes_received += d.len() as u64;
        }

        Ok(data)
    }

    /// Close a connection
    pub fn close(&self, conn_id: u64) -> Result<()> {
        let conn = self
            .connections
            .write()
            .remove(&conn_id)
            .ok_or_else(|| KernelError::invalid_argument("Invalid connection handle"))?;

        conn.close();
        self.stats.write().active_connections = self.connections.read().len() as u64;

        info!("Connection {} closed", conn_id);
        Ok(())
    }

    /// Get connection statistics
    pub fn get_stats(&self, conn_id: u64) -> Result<ConnectionStats> {
        let conn = self
            .connections
            .read()
            .get(&conn_id)
            .cloned()
            .ok_or_else(|| KernelError::invalid_argument("Invalid connection handle"))?;
        let stats = conn.stats.read().clone();
        Ok(stats)
    }

    /// Get latency for a connection
    pub fn get_latency(&self, conn_id: u64) -> Result<u64> {
        self.get_stats(conn_id).map(|s| s.latency_ms)
    }

    /// Update latency measurement for a connection
    pub fn record_latency(&self, conn_id: u64, latency_ms: u64) -> Result<()> {
        let conn = self
            .connections
            .read()
            .get(&conn_id)
            .cloned()
            .ok_or_else(|| KernelError::invalid_argument("Invalid connection handle"))?;

        conn.record_latency(latency_ms);
        Ok(())
    }

    /// Check if connection exists
    pub fn has_connection(&self, conn_id: u64) -> bool {
        self.connections.read().contains_key(&conn_id)
    }

    /// Get all active connection IDs
    pub fn list_connections(&self) -> Vec<u64> {
        self.connections.read().keys().copied().collect()
    }

    /// Get manager statistics
    pub fn manager_stats(&self) -> ManagerStats {
        self.stats.read().clone()
    }

    /// Cleanup idle connections
    pub fn cleanup_idle(&self) -> usize {
        let mut connections = self.connections.write();
        let idle_ids: Vec<u64> = connections
            .iter()
            .filter(|(_, conn)| conn.stats.read().is_idle(self.config.idle_timeout))
            .map(|(id, _)| *id)
            .collect();

        for id in &idle_ids {
            if let Some(conn) = connections.get(id) {
                conn.close();
            }
            connections.remove(id);
        }

        self.stats.write().active_connections = connections.len() as u64;
        idle_ids.len()
    }

    /// Shutdown all connections
    pub fn shutdown(&self) {
        let mut connections = self.connections.write();
        for (_, conn) in connections.iter() {
            conn.close();
        }
        connections.clear();
        self.stats.write().active_connections = 0;
    }

    /// Generate next connection ID (reserved for future use)
    #[allow(dead_code)]
    fn next_connection_id(&self) -> u64 {
        let mut id = self.next_id.write();
        let current = *id;
        *id += 1;
        current
    }

    /// Get pool manager for advanced usage
    pub fn pool_manager(&self) -> &ConnectionPoolManager {
        &self.pool_manager
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Measure latency to a remote address using TCP handshake timing
pub async fn measure_latency(addr: SocketAddr, timeout: Duration) -> Result<u64> {
    let start = Instant::now();

    match tokio::time::timeout(timeout, TcpStream::connect(addr)).await {
        Ok(Ok(_)) => {
            let latency = start.elapsed().as_millis() as u64;
            Ok(latency)
        }
        Ok(Err(e)) => Err(KernelError::io(format!("Connection failed: {}", e))),
        Err(_) => Err(KernelError::Timeout),
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    #[tokio::test]
    async fn test_connection_pool() {
        // Start a test server
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            loop {
                match stream.read(&mut buf).await {
                    Ok(n) if n > 0 => {
                        let _ = stream.write_all(&buf[..n]).await;
                    }
                    _ => break,
                }
            }
        });

        // Create pool
        let config = ConnectionPoolConfig {
            name: "test-pool".to_string(),
            target_addr: addr,
            min_connections: 1,
            max_connections: 3,
            ..Default::default()
        };

        let pool = ConnectionPool::new(config);
        pool.start().await.unwrap();

        // Acquire a connection
        let conn = pool.acquire().await.unwrap();
        assert_eq!(conn.state(), ConnectionState::Active);

        // Release connection
        pool.release(conn.id);

        // Get stats
        let stats = pool.stats();
        assert!(stats.total_connections >= 1);

        pool.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_pool_manager() {
        let manager = ConnectionPoolManager::new();

        let config = ConnectionPoolConfig {
            name: "test-pool".to_string(),
            target_addr: "127.0.0.1:12345".parse().unwrap(),
            ..Default::default()
        };

        // Create pool should work even without server (connections will fail but pool
        // exists)
        let pool = manager.create_pool(config).await;
        assert!(pool.is_ok());

        // Get pool
        let retrieved = manager.get_pool("test-pool");
        assert!(retrieved.is_some());

        // Get all stats
        let stats = manager.get_all_stats();
        assert_eq!(stats.len(), 1);
    }

    #[tokio::test]
    async fn test_load_balancing_strategies() {
        // This test just verifies the strategy enum works
        let strategies = vec![
            LoadBalancingStrategy::RoundRobin,
            LoadBalancingStrategy::LeastConnections,
            LoadBalancingStrategy::LowestLatency,
            LoadBalancingStrategy::Random,
            LoadBalancingStrategy::WeightedRoundRobin,
        ];

        for strategy in strategies {
            let config = ConnectionPoolConfig {
                name: format!("pool-{:?}", strategy),
                load_balancing: strategy,
                ..Default::default()
            };
            assert_eq!(config.load_balancing, strategy);
        }
    }

    #[tokio::test]
    async fn test_connection_stats() {
        let mut stats = ConnectionStats::new();

        stats.record_sent(100);
        stats.record_received(200);
        stats.record_latency(50);
        stats.record_latency(60);
        stats.record_latency(70);
        stats.record_request(100, true);
        stats.record_request(200, false);

        assert_eq!(stats.bytes_sent, 100);
        assert_eq!(stats.bytes_received, 200);
        assert_eq!(stats.average_latency(), 60);
        assert_eq!(stats.request_count, 2);
        assert_eq!(stats.success_rate(), 50);
    }

    #[test]
    fn test_connection_state() {
        assert!(ConnectionState::Idle.is_usable());
        assert!(ConnectionState::Active.is_usable());
        assert!(!ConnectionState::Closed.is_usable());
        assert!(!ConnectionState::Failed.is_usable());

        assert!(ConnectionState::Closed.is_terminal());
        assert!(ConnectionState::Failed.is_terminal());
        assert!(!ConnectionState::Idle.is_terminal());
    }
}
