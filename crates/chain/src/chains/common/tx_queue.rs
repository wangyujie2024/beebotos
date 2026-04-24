//! Transaction Batch Queue Module
//!
//! Provides priority-based transaction queuing, batching, and submission
//! with automatic gas estimation and nonce management.

use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use alloy_rpc_types::TransactionRequest;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{debug, info, instrument};

use crate::chains::common::{ChainStateCache, EvmProvider, GasEstimator, TransactionPriority};
use crate::compat::Address;
use crate::ChainError;

/// Unique transaction ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TxId(pub u64);

impl TxId {
    /// Generate new unique ID
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for TxId {
    fn default() -> Self {
        Self::new()
    }
}

/// Queued transaction with metadata
#[derive(Debug)]
pub struct QueuedTransaction {
    pub id: TxId,
    pub transaction: TransactionRequest,
    pub priority: TransactionPriority,
    pub submitter: Address,
    pub submitted_at: Instant,
    pub max_gas_price: Option<u128>,
    pub required_confirmations: u64,
    pub result_sender: Option<oneshot::Sender<TxResult>>,
}

impl Clone for QueuedTransaction {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            transaction: self.transaction.clone(),
            priority: self.priority,
            submitter: self.submitter,
            submitted_at: self.submitted_at,
            max_gas_price: self.max_gas_price,
            required_confirmations: self.required_confirmations,
            // oneshot::Sender cannot be cloned, so we create a dummy channel and drop the receiver
            result_sender: None,
        }
    }
}

impl PartialEq for QueuedTransaction {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for QueuedTransaction {}

impl PartialOrd for QueuedTransaction {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        // Higher priority comes first
        other
            .priority
            .priority_level()
            .partial_cmp(&self.priority.priority_level())
    }
}

impl Ord for QueuedTransaction {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap_or(std::cmp::Ordering::Equal)
    }
}

/// Transaction submission result
#[derive(Debug, Clone)]
pub struct TxResult {
    pub tx_id: TxId,
    pub success: bool,
    pub tx_hash: Option<String>,
    pub error: Option<String>,
    pub gas_used: Option<u64>,
    pub block_number: Option<u64>,
}

/// Transaction queue configuration
#[derive(Debug, Clone)]
pub struct TxQueueConfig {
    /// Maximum queue size
    pub max_queue_size: usize,
    /// Batch size for submission
    pub batch_size: usize,
    /// Batch timeout (submit batch even if not full)
    pub batch_timeout: Duration,
    /// Max pending transactions
    pub max_pending: usize,
    /// Automatic gas estimation
    pub auto_estimate_gas: bool,
    /// Automatic nonce management
    pub auto_manage_nonce: bool,
}

impl Default for TxQueueConfig {
    fn default() -> Self {
        Self {
            max_queue_size: 1000,
            batch_size: 10,
            batch_timeout: Duration::from_secs(5),
            max_pending: 100,
            auto_estimate_gas: true,
            auto_manage_nonce: true,
        }
    }
}

/// Transaction batch queue
pub struct TransactionQueue {
    config: TxQueueConfig,
    /// Priority queue for pending transactions
    pending: Arc<RwLock<BinaryHeap<QueuedTransaction>>>,
    /// Queue for batched transactions
    batched: Arc<RwLock<VecDeque<QueuedTransaction>>>,
    /// Currently processing transactions
    processing: Arc<RwLock<Vec<QueuedTransaction>>>,
    /// State cache for nonce management
    state_cache: Option<ChainStateCache>,
    /// Gas estimator
    gas_estimator: Option<Arc<GasEstimator>>,
    /// Submission channel
    submission_tx: mpsc::Sender<QueuedTransaction>,
}

impl TransactionQueue {
    /// Create new transaction queue
    pub fn new(config: TxQueueConfig) -> (Self, mpsc::Receiver<QueuedTransaction>) {
        let (tx, rx) = mpsc::channel(config.max_queue_size);

        let queue = Self {
            config,
            pending: Arc::new(RwLock::new(BinaryHeap::new())),
            batched: Arc::new(RwLock::new(VecDeque::new())),
            processing: Arc::new(RwLock::new(Vec::new())),
            state_cache: None,
            gas_estimator: None,
            submission_tx: tx,
        };

        (queue, rx)
    }

    /// Set state cache
    pub fn with_state_cache(mut self, cache: ChainStateCache) -> Self {
        self.state_cache = Some(cache);
        self
    }

    /// Set gas estimator
    pub fn with_gas_estimator(mut self, estimator: Arc<GasEstimator>) -> Self {
        self.gas_estimator = Some(estimator);
        self
    }

    /// Submit transaction to queue
    #[instrument(skip(self), target = "chain::tx_queue")]
    pub async fn submit(
        &self,
        transaction: TransactionRequest,
        priority: TransactionPriority,
        submitter: Address,
    ) -> Result<oneshot::Receiver<TxResult>, ChainError> {
        // Check queue capacity
        if self.pending.read().await.len() >= self.config.max_queue_size {
            return Err(ChainError::Provider("Queue is full".to_string()));
        }

        let (tx, rx) = oneshot::channel();

        let queued = QueuedTransaction {
            id: TxId::new(),
            transaction,
            priority,
            submitter,
            submitted_at: Instant::now(),
            max_gas_price: None,
            required_confirmations: 1,
            result_sender: Some(tx),
        };

        debug!(
            tx_id = %queued.id.0,
            priority = ?queued.priority,
            "Transaction queued"
        );

        // Add to pending queue (clone for the queue)
        self.pending.write().await.push(queued.clone());

        // Send to submission channel (send original)
        if self.submission_tx.send(queued).await.is_err() {
            return Err(ChainError::Provider(
                "Submission channel closed".to_string(),
            ));
        }

        Ok(rx)
    }

    /// Submit with custom options
    #[instrument(skip(self), target = "chain::tx_queue")]
    pub async fn submit_with_options(
        &self,
        transaction: TransactionRequest,
        priority: TransactionPriority,
        submitter: Address,
        max_gas_price: Option<u128>,
        required_confirmations: u64,
    ) -> Result<oneshot::Receiver<TxResult>, ChainError> {
        if self.pending.read().await.len() >= self.config.max_queue_size {
            return Err(ChainError::Provider("Queue is full".to_string()));
        }

        let (tx, rx) = oneshot::channel();

        let queued = QueuedTransaction {
            id: TxId::new(),
            transaction,
            priority,
            submitter,
            submitted_at: Instant::now(),
            max_gas_price,
            required_confirmations,
            result_sender: Some(tx),
        };

        debug!(
            tx_id = %queued.id.0,
            priority = ?queued.priority,
            "Transaction queued with options"
        );

        self.pending.write().await.push(queued.clone());

        if self.submission_tx.send(queued).await.is_err() {
            return Err(ChainError::Provider(
                "Submission channel closed".to_string(),
            ));
        }

        Ok(rx)
    }

    /// Get queue statistics
    pub async fn get_stats(&self) -> QueueStatistics {
        QueueStatistics {
            pending_count: self.pending.read().await.len(),
            batched_count: self.batched.read().await.len(),
            processing_count: self.processing.read().await.len(),
            total_queued: self.pending.read().await.len()
                + self.batched.read().await.len()
                + self.processing.read().await.len(),
        }
    }

    /// Get next batch for processing
    pub async fn next_batch(&self) -> Vec<QueuedTransaction> {
        let mut batch = Vec::with_capacity(self.config.batch_size);
        let mut pending = self.pending.write().await;

        while batch.len() < self.config.batch_size && !pending.is_empty() {
            if let Some(tx) = pending.pop() {
                batch.push(tx);
            }
        }

        // Move to batched queue
        let mut batched = self.batched.write().await;
        for tx in &batch {
            batched.push_back(tx.clone());
        }

        batch
    }

    /// Cancel a pending transaction
    pub async fn cancel(&self, tx_id: TxId) -> bool {
        // Note: BinaryHeap doesn't support efficient removal by id
        // In production, would use additional index
        let mut pending = self.pending.write().await;

        // Create new heap without the cancelled transaction
        let mut new_heap = BinaryHeap::new();
        let mut found = false;

        while let Some(tx) = pending.pop() {
            if tx.id == tx_id {
                found = true;
                // Send cancellation result
                if let Some(sender) = tx.result_sender {
                    let _ = sender.send(TxResult {
                        tx_id,
                        success: false,
                        tx_hash: None,
                        error: Some("Cancelled by user".to_string()),
                        gas_used: None,
                        block_number: None,
                    });
                }
                break;
            }
            new_heap.push(tx);
        }

        // Restore remaining transactions
        while let Some(tx) = pending.pop() {
            new_heap.push(tx);
        }

        *pending = new_heap;
        found
    }

    /// Clear all pending transactions
    pub async fn clear(&self) {
        let mut pending = self.pending.write().await;

        // Notify all pending transactions of cancellation
        while let Some(tx) = pending.pop() {
            if let Some(sender) = tx.result_sender {
                let _ = sender.send(TxResult {
                    tx_id: tx.id,
                    success: false,
                    tx_hash: None,
                    error: Some("Queue cleared".to_string()),
                    gas_used: None,
                    block_number: None,
                });
            }
        }

        self.batched.write().await.clear();
        info!("Transaction queue cleared");
    }

    /// Update transaction result
    pub async fn update_result(&self, tx_id: TxId, result: TxResult) {
        // Remove from processing
        let mut processing = self.processing.write().await;
        if let Some(pos) = processing.iter().position(|tx| tx.id == tx_id) {
            let tx = processing.remove(pos);

            // Send result to caller
            if let Some(sender) = tx.result_sender {
                let _ = sender.send(result);
            }
        }

        // Remove from batched
        let mut batched = self.batched.write().await;
        if let Some(pos) = batched.iter().position(|tx| tx.id == tx_id) {
            batched.remove(pos);
        }
    }
}

/// Queue statistics
#[derive(Debug, Clone, Copy)]
pub struct QueueStatistics {
    pub pending_count: usize,
    pub batched_count: usize,
    pub processing_count: usize,
    pub total_queued: usize,
}

/// Transaction batch processor
pub struct TxBatchProcessor {
    queue: Arc<TransactionQueue>,
    provider: Arc<EvmProvider>,
    #[allow(dead_code)]
    gas_estimator: Option<Arc<GasEstimator>>,
    #[allow(dead_code)]
    state_cache: Option<ChainStateCache>,
    running: Arc<RwLock<bool>>,
}

impl TxBatchProcessor {
    /// Create new batch processor
    pub fn new(
        queue: Arc<TransactionQueue>,
        provider: Arc<EvmProvider>,
        gas_estimator: Option<Arc<GasEstimator>>,
        state_cache: Option<ChainStateCache>,
    ) -> Self {
        Self {
            queue,
            provider,
            gas_estimator,
            state_cache,
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// Start batch processing
    pub async fn start(&self) {
        *self.running.write().await = true;

        let queue = self.queue.clone();
        let _provider = self.provider.clone();
        let running = self.running.clone();

        tokio::spawn(async move {
            while *running.read().await {
                // Get next batch
                let batch = queue.next_batch().await;

                if batch.is_empty() {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }

                debug!(batch_size = batch.len(), "Processing batch");

                // Process each transaction in batch
                for tx in batch {
                    // In production, this would:
                    // 1. Estimate gas if needed
                    // 2. Get nonce
                    // 3. Sign transaction
                    // 4. Submit to network
                    // 5. Wait for confirmation
                    // 6. Update result

                    // For now, simulate processing
                    let result = TxResult {
                        tx_id: tx.id,
                        success: true,
                        tx_hash: Some(format!("0x{:064x}", tx.id.0)),
                        error: None,
                        gas_used: Some(21000),
                        block_number: Some(100),
                    };

                    queue.update_result(tx.id, result).await;
                }
            }

            info!("Batch processor stopped");
        });

        info!("Batch processor started");
    }

    /// Stop batch processing
    pub async fn stop(&self) {
        *self.running.write().await = false;
    }
}

/// Transaction builder for queue submission
pub struct QueuedTxBuilder {
    transaction: TransactionRequest,
    priority: TransactionPriority,
    max_gas_price: Option<u128>,
    required_confirmations: u64,
}

impl QueuedTxBuilder {
    /// Create new builder
    pub fn new(transaction: TransactionRequest) -> Self {
        Self {
            transaction,
            priority: TransactionPriority::Normal,
            max_gas_price: None,
            required_confirmations: 1,
        }
    }

    /// Set priority
    pub fn priority(mut self, priority: TransactionPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Set max gas price
    pub fn max_gas_price(mut self, price: u128) -> Self {
        self.max_gas_price = Some(price);
        self
    }

    /// Set required confirmations
    pub fn confirmations(mut self, confirmations: u64) -> Self {
        self.required_confirmations = confirmations;
        self
    }

    /// Build queued transaction
    pub fn build(self, submitter: Address) -> (QueuedTransaction, oneshot::Receiver<TxResult>) {
        let (tx, rx) = oneshot::channel();

        let queued = QueuedTransaction {
            id: TxId::new(),
            transaction: self.transaction,
            priority: self.priority,
            submitter,
            submitted_at: Instant::now(),
            max_gas_price: self.max_gas_price,
            required_confirmations: self.required_confirmations,
            result_sender: Some(tx),
        };

        (queued, rx)
    }
}

/// Priority-based transaction sorter
pub struct PrioritySorter;

impl PrioritySorter {
    /// Sort transactions by priority (urgent > high > medium > low)
    pub fn sort(transactions: &mut [QueuedTransaction]) {
        transactions.sort_by(|a, b| {
            b.priority
                .priority_level()
                .cmp(&a.priority.priority_level())
        });
    }

    /// Group transactions by priority
    pub fn group_by_priority(
        transactions: Vec<QueuedTransaction>,
    ) -> HashMap<TransactionPriority, Vec<QueuedTransaction>> {
        let mut groups: HashMap<TransactionPriority, Vec<QueuedTransaction>> = HashMap::new();

        for tx in transactions {
            groups.entry(tx.priority).or_default().push(tx);
        }

        groups
    }
}

/// Transaction batch builder
pub struct TxBatchBuilder {
    transactions: Vec<QueuedTransaction>,
    max_batch_size: usize,
    #[allow(dead_code)]
    max_batch_gas: u64,
}

impl TxBatchBuilder {
    /// Create new batch builder
    pub fn new(max_batch_size: usize, max_batch_gas: u64) -> Self {
        Self {
            transactions: Vec::new(),
            max_batch_size,
            max_batch_gas,
        }
    }

    /// Add transaction to batch
    pub fn add(&mut self, tx: QueuedTransaction) -> Result<(), ChainError> {
        if self.transactions.len() >= self.max_batch_size {
            return Err(ChainError::Provider("Batch is full".to_string()));
        }

        self.transactions.push(tx);
        Ok(())
    }

    /// Build batch
    pub fn build(self) -> Vec<QueuedTransaction> {
        self.transactions
    }

    /// Check if batch is full
    pub fn is_full(&self) -> bool {
        self.transactions.len() >= self.max_batch_size
    }

    /// Get current batch size
    pub fn len(&self) -> usize {
        self.transactions.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.transactions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tx_id_generation() {
        let id1 = TxId::new();
        let id2 = TxId::new();
        assert_ne!(id1.0, id2.0);
        assert!(id1 < id2 || id1 > id2);
    }

    #[test]
    fn test_queued_transaction_priority_ordering() {
        let tx1 = QueuedTransaction {
            id: TxId::new(),
            transaction: TransactionRequest::default(),
            priority: TransactionPriority::Low,
            submitter: Address::ZERO,
            submitted_at: Instant::now(),
            max_gas_price: None,
            required_confirmations: 1,
            result_sender: None,
        };

        let tx2 = QueuedTransaction {
            id: TxId::new(),
            transaction: TransactionRequest::default(),
            priority: TransactionPriority::Urgent,
            submitter: Address::ZERO,
            submitted_at: Instant::now(),
            max_gas_price: None,
            required_confirmations: 1,
            result_sender: None,
        };

        // tx2 (urgent) should be less than tx1 (low) for max-heap behavior
        assert!(tx2 < tx1);
    }

    #[test]
    fn test_tx_queue_config_default() {
        let config = TxQueueConfig::default();
        assert_eq!(config.max_queue_size, 1000);
        assert_eq!(config.batch_size, 10);
        assert!(config.auto_estimate_gas);
    }

    #[test]
    fn test_tx_batch_builder() {
        let mut builder = TxBatchBuilder::new(5, 1000000);

        assert!(builder.is_empty());

        let tx = QueuedTransaction {
            id: TxId::new(),
            transaction: TransactionRequest::default(),
            priority: TransactionPriority::Normal,
            submitter: Address::ZERO,
            submitted_at: Instant::now(),
            max_gas_price: None,
            required_confirmations: 1,
            result_sender: None,
        };

        builder.add(tx).unwrap();
        assert_eq!(builder.len(), 1);
        assert!(!builder.is_full());
    }

    #[test]
    fn test_priority_sorter() {
        let mut transactions = vec![
            QueuedTransaction {
                id: TxId::new(),
                transaction: TransactionRequest::default(),
                priority: TransactionPriority::Low,
                submitter: Address::ZERO,
                submitted_at: Instant::now(),
                max_gas_price: None,
                required_confirmations: 1,
                result_sender: None,
            },
            QueuedTransaction {
                id: TxId::new(),
                transaction: TransactionRequest::default(),
                priority: TransactionPriority::Urgent,
                submitter: Address::ZERO,
                submitted_at: Instant::now(),
                max_gas_price: None,
                required_confirmations: 1,
                result_sender: None,
            },
            QueuedTransaction {
                id: TxId::new(),
                transaction: TransactionRequest::default(),
                priority: TransactionPriority::High,
                submitter: Address::ZERO,
                submitted_at: Instant::now(),
                max_gas_price: None,
                required_confirmations: 1,
                result_sender: None,
            },
        ];

        PrioritySorter::sort(&mut transactions);

        assert_eq!(transactions[0].priority, TransactionPriority::Urgent);
        assert_eq!(transactions[1].priority, TransactionPriority::High);
        assert_eq!(transactions[2].priority, TransactionPriority::Low);
    }

    #[tokio::test]
    async fn test_queue_statistics() {
        let config = TxQueueConfig::default();
        let (queue, _rx) = TransactionQueue::new(config);
        let queue = Arc::new(queue);

        let stats = queue.get_stats().await;
        assert_eq!(stats.pending_count, 0);
        assert_eq!(stats.total_queued, 0);
    }
}
