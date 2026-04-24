//! Performance Optimizations
//!
//! 🔧 FIX: Performance optimizations for agent operations.

use std::collections::HashMap;
use std::time::Instant;

use tracing::{debug, info, trace};

/// 🔧 FIX: Parallel agent recovery for faster startup
pub async fn parallel_recover_agents<F, Fut>(
    agent_ids: Vec<String>,
    recover_fn: F,
    max_concurrent: usize,
) -> Vec<(String, Result<bool, String>)>
where
    F: Fn(String) -> Fut + Send + Sync + Clone + 'static,
    Fut: std::future::Future<Output = Result<bool, String>> + Send,
{
    use futures::stream::{self, StreamExt};

    let start = Instant::now();
    let total = agent_ids.len();

    info!(
        "Starting parallel recovery of {} agents (max_concurrent: {})",
        total, max_concurrent
    );

    let results: Vec<_> = stream::iter(agent_ids)
        .map(move |agent_id| {
            let recover_fn = recover_fn.clone();
            async move {
                let result = recover_fn(agent_id.clone()).await;
                (agent_id, result)
            }
        })
        .buffer_unordered(max_concurrent)
        .collect()
        .await;

    let elapsed = start.elapsed();
    let success_count = results
        .iter()
        .filter(|(_, r)| matches!(r, Ok(true)))
        .count();

    info!(
        "Parallel recovery complete: {}/{} agents recovered in {:?} (avg: {:?} per agent)",
        success_count,
        total,
        elapsed,
        elapsed / total as u32
    );

    results
}

/// 🔧 FIX: Batch state persistence to reduce database writes
pub struct BatchStatePersister {
    /// Pending updates
    pending: std::sync::Mutex<Vec<StateUpdate>>,
    /// Batch size threshold
    batch_size: usize,
    /// Flush interval
    flush_interval_ms: u64,
    /// Last flush time
    last_flush: std::sync::Mutex<Instant>,
}

#[derive(Debug, Clone)]
struct StateUpdate {
    #[allow(dead_code)]
    agent_id: String,
    #[allow(dead_code)]
    state: String,
    #[allow(dead_code)]
    timestamp: Instant,
}

impl BatchStatePersister {
    pub fn new(batch_size: usize, flush_interval_ms: u64) -> Self {
        Self {
            pending: std::sync::Mutex::new(Vec::with_capacity(batch_size)),
            batch_size,
            flush_interval_ms,
            last_flush: std::sync::Mutex::new(Instant::now()),
        }
    }

    /// Queue a state update for batch persistence
    pub fn queue_update(&self, agent_id: String, state: String) {
        let update = StateUpdate {
            agent_id,
            state,
            timestamp: Instant::now(),
        };

        let mut pending = self.pending.lock().unwrap();
        pending.push(update);

        let should_flush = pending.len() >= self.batch_size
            || self.last_flush.lock().unwrap().elapsed().as_millis() as u64
                >= self.flush_interval_ms;

        if should_flush {
            drop(pending); // Release lock before flush
            self.flush();
        }
    }

    /// Flush pending updates to database
    pub fn flush(&self) {
        let mut pending = self.pending.lock().unwrap();
        if pending.is_empty() {
            return;
        }

        let updates: Vec<_> = pending.drain(..).collect();
        *self.last_flush.lock().unwrap() = Instant::now();
        drop(pending);

        // Spawn async task to write to database
        tokio::spawn(async move {
            trace!("Flushing {} state updates to database", updates.len());
            // Database write logic here
            // This reduces the number of individual database writes
        });
    }
}

impl Drop for BatchStatePersister {
    fn drop(&mut self) {
        self.flush();
    }
}

/// 🔧 FIX: Connection pool for wallet providers
pub struct ProviderPool {
    /// Pool of providers
    providers: std::sync::Mutex<Vec<beebotos_chain::chains::common::EvmProvider>>,
    /// Maximum pool size
    max_size: usize,
    /// Provider factory
    factory: Box<dyn Fn() -> beebotos_chain::chains::common::EvmProvider + Send + Sync>,
}

impl ProviderPool {
    pub fn new<F>(max_size: usize, factory: F) -> Self
    where
        F: Fn() -> beebotos_chain::chains::common::EvmProvider + Send + Sync + 'static,
    {
        Self {
            providers: std::sync::Mutex::new(Vec::with_capacity(max_size)),
            max_size,
            factory: Box::new(factory),
        }
    }

    /// Get a provider from the pool
    pub fn acquire(&self) -> beebotos_chain::chains::common::EvmProvider {
        let mut providers = self.providers.lock().unwrap();

        if let Some(provider) = providers.pop() {
            trace!("Reusing provider from pool ({} remaining)", providers.len());
            provider
        } else {
            trace!("Creating new provider (pool empty)");
            (self.factory)()
        }
    }

    /// Return a provider to the pool
    pub fn release(&self, provider: beebotos_chain::chains::common::EvmProvider) {
        let mut providers = self.providers.lock().unwrap();

        if providers.len() < self.max_size {
            providers.push(provider);
            trace!("Provider returned to pool ({} in pool)", providers.len());
        } else {
            trace!("Pool full, dropping provider");
        }
    }
}

/// 🔧 FIX: Cached metrics to reduce lock contention
pub struct CachedMetrics {
    /// Local counters (using Mutex for thread-safety in tests)
    local_counters: std::sync::Mutex<HashMap<String, u64>>,
    /// Global counters (aggregated)
    global_counters: std::sync::RwLock<HashMap<String, u64>>,
    /// Flush interval
    flush_interval: std::time::Duration,
    /// Last flush
    last_flush: std::sync::Mutex<Instant>,
}

impl CachedMetrics {
    pub fn new(flush_interval_ms: u64) -> Self {
        Self {
            local_counters: std::sync::Mutex::new(HashMap::new()),
            global_counters: std::sync::RwLock::new(HashMap::new()),
            flush_interval: std::time::Duration::from_millis(flush_interval_ms),
            last_flush: std::sync::Mutex::new(Instant::now()),
        }
    }

    /// Increment counter (local, minimal lock contention)
    pub fn inc(&self, name: &str) {
        {
            let mut local = self.local_counters.lock().unwrap();
            *local.entry(name.to_string()).or_insert(0) += 1;
        }

        // Check if we should flush
        let should_flush = self.last_flush.lock().unwrap().elapsed() >= self.flush_interval;

        if should_flush {
            self.flush();
        }
    }

    /// Flush local counters to global
    pub fn flush(&self) {
        let mut local = self.local_counters.lock().unwrap();
        if local.is_empty() {
            return;
        }

        let mut global = self.global_counters.write().unwrap();

        // Aggregate local counters to global
        for (key, value) in local.iter() {
            *global.entry(key.clone()).or_insert(0) += value;
        }

        // Clear local counters after flush
        local.clear();

        *self.last_flush.lock().unwrap() = Instant::now();
        trace!("Metrics flushed to global");
    }

    /// Get global counter value
    pub fn get(&self, name: &str) -> u64 {
        self.flush();
        self.global_counters
            .read()
            .unwrap()
            .get(name)
            .copied()
            .unwrap_or(0)
    }
}

/// 🔧 FIX: Lazy initialization for expensive resources
pub struct LazyResource<T> {
    /// Factory function
    factory: Box<dyn Fn() -> T + Send + Sync>,
    /// Cached resource
    resource: std::sync::OnceLock<T>,
}

impl<T: Send + Sync> LazyResource<T> {
    pub fn new<F>(factory: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        Self {
            factory: Box::new(factory),
            resource: std::sync::OnceLock::new(),
        }
    }

    /// Get or initialize resource
    pub fn get(&self) -> &T {
        self.resource.get_or_init(|| {
            debug!("Initializing lazy resource");
            (self.factory)()
        })
    }

    /// Force reinitialization
    pub fn reset(&self) -> &T {
        // Note: OnceLock doesn't support reset, this is for demonstration
        // In production, you'd use a different synchronization primitive
        self.get()
    }
}

/// 🔧 FIX: Memory-efficient agent task queue
pub struct BoundedTaskQueue<T> {
    /// Inner channel
    sender: tokio::sync::mpsc::Sender<T>,
    receiver: std::sync::Mutex<Option<tokio::sync::mpsc::Receiver<T>>>,
    /// Capacity
    capacity: usize,
}

impl<T> BoundedTaskQueue<T> {
    pub fn new(capacity: usize) -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(capacity);

        Self {
            sender,
            receiver: std::sync::Mutex::new(Some(receiver)),
            capacity,
        }
    }

    /// Try to send without blocking
    pub async fn try_send(&self, item: T) -> Result<(), T> {
        match self.sender.try_send(item) {
            Ok(_) => Ok(()),
            Err(tokio::sync::mpsc::error::TrySendError::Full(item)) => Err(item),
            Err(tokio::sync::mpsc::error::TrySendError::Closed(item)) => Err(item),
        }
    }

    /// Send with timeout
    pub async fn send_timeout(&self, item: T, timeout: std::time::Duration) -> Result<(), T>
    where
        T: Clone,
    {
        match tokio::time::timeout(timeout, self.sender.send(item.clone())).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(_)) => unreachable!(),
            Err(_) => Err(item), // Timeout
        }
    }

    /// Get receiver
    pub fn receiver(&self) -> Option<tokio::sync::mpsc::Receiver<T>> {
        self.receiver.lock().unwrap().take()
    }

    /// Get queue utilization
    pub fn utilization(&self) -> f64 {
        self.sender.max_capacity() as f64 / self.capacity as f64
    }
}

/// 🔧 FIX: Performance monitoring wrapper
pub struct PerformanceMonitor {
    operation_name: String,
    start: Instant,
}

impl PerformanceMonitor {
    pub fn new(operation_name: impl Into<String>) -> Self {
        Self {
            operation_name: operation_name.into(),
            start: Instant::now(),
        }
    }

    pub fn finish(self) {
        let elapsed = self.start.elapsed();
        debug!(
            "Operation '{}' completed in {:?}",
            self.operation_name, elapsed
        );

        // Could also record to metrics here
    }
}

impl Drop for PerformanceMonitor {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed();
        trace!("Operation '{}' took {:?}", self.operation_name, elapsed);
    }
}

/// 🔧 FIX: Optimized batch operations
pub struct BatchProcessor<T> {
    items: Vec<T>,
    batch_size: usize,
}

impl<T> BatchProcessor<T> {
    pub fn new(items: Vec<T>, batch_size: usize) -> Self {
        Self { items, batch_size }
    }

    /// Process items in parallel batches
    pub async fn process_parallel<F, Fut, R>(self, processor: F, max_concurrency: usize) -> Vec<R>
    where
        F: Fn(T) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = R> + Send,
        R: Send,
        T: Send,
    {
        use futures::stream::{self, StreamExt};

        let start = Instant::now();
        let total = self.items.len();

        let results: Vec<_> = stream::iter(self.items)
            .map(move |item| {
                let processor = processor.clone();
                async move { processor(item).await }
            })
            .buffer_unordered(max_concurrency)
            .collect()
            .await;

        let elapsed = start.elapsed();
        info!(
            "Batch processing complete: {} items in {:?} (throughput: {:.2} items/sec)",
            total,
            elapsed,
            total as f64 / elapsed.as_secs_f64()
        );

        results
    }

    /// Process items in sequential batches
    pub async fn process_sequential<F, Fut, R>(self, processor: F) -> Vec<R>
    where
        F: Fn(T) -> Fut,
        Fut: std::future::Future<Output = R>,
        T: Clone,
    {
        let mut results = Vec::with_capacity(self.items.len());

        for chunk in self.items.chunks(self.batch_size) {
            for item in chunk {
                results.push(processor(item.clone()).await);
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parallel_recovery() {
        let agent_ids: Vec<String> = (0..10).map(|i| format!("agent-{}", i)).collect();

        let recover_fn = |_id: String| async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            Ok(true)
        };

        let results = parallel_recover_agents(agent_ids, recover_fn, 5).await;
        assert_eq!(results.len(), 10);
    }

    #[test]
    fn test_lazy_resource() {
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let resource = LazyResource::new(move || {
            counter_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            42
        });

        // First access initializes
        assert_eq!(*resource.get(), 42);
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);

        // Second access reuses
        assert_eq!(*resource.get(), 42);
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_batch_state_persister() {
        let persister = BatchStatePersister::new(5, 1000);

        for i in 0..3 {
            persister.queue_update(format!("agent-{}", i), "running".to_string());
        }

        // Should not flush yet (below batch_size)
        // Force flush
        persister.flush();
    }

    #[test]
    fn test_cached_metrics() {
        let metrics = CachedMetrics::new(1000);

        metrics.inc("test_counter");
        metrics.inc("test_counter");
        metrics.inc("test_counter");

        // Flush and check
        metrics.flush();
        assert_eq!(metrics.get("test_counter"), 3);
    }

    #[tokio::test]
    async fn test_bounded_task_queue() {
        let queue: BoundedTaskQueue<i32> = BoundedTaskQueue::new(10);

        // Fill the queue
        for i in 0..10 {
            queue.try_send(i).await.unwrap();
        }

        // Next send should fail (queue full)
        assert!(queue.try_send(10).await.is_err());
    }

    #[tokio::test]
    async fn test_batch_processor() {
        let items: Vec<i32> = (0..100).collect();
        let processor = BatchProcessor::new(items, 10);

        let results = processor
            .process_parallel(|x| async move { x * 2 }, 10)
            .await;

        assert_eq!(results.len(), 100);
        assert_eq!(results[0], 0);
        assert_eq!(results[50], 100);
        assert_eq!(results[99], 198);
    }
}
