//! Generic Mempool for EVM Chains
//!
//! Manages pending transactions before they are mined.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use alloy_rpc_types::Transaction;
use tokio::sync::RwLock;

/// Transaction entry with metadata
#[derive(Debug, Clone)]
pub struct MempoolEntry {
    pub transaction: Transaction,
    pub submitted_at: Instant,
    pub retry_count: u32,
}

impl MempoolEntry {
    pub fn new(transaction: Transaction) -> Self {
        Self {
            transaction,
            submitted_at: Instant::now(),
            retry_count: 0,
        }
    }

    pub fn age(&self) -> Duration {
        self.submitted_at.elapsed()
    }
}

/// Mempool for pending transactions
pub struct Mempool {
    txs: RwLock<HashMap<alloy_primitives::B256, MempoolEntry>>,
    max_size: usize,
    max_age: Duration,
}

impl Mempool {
    /// Create new mempool with default limits
    pub fn new() -> Self {
        Self::with_limits(10_000, Duration::from_secs(3600))
    }

    /// Create mempool with custom limits
    pub fn with_limits(max_size: usize, max_age: Duration) -> Self {
        Self {
            txs: RwLock::new(HashMap::new()),
            max_size,
            max_age,
        }
    }

    /// Add transaction to mempool
    pub async fn add(&self, hash: alloy_primitives::B256, tx: Transaction) {
        let mut txs = self.txs.write().await;

        // Evict old transactions if at capacity
        if txs.len() >= self.max_size {
            Self::evict_oldest(&mut txs);
        }

        let entry = MempoolEntry::new(tx);
        txs.insert(hash, entry);
    }

    /// Get transaction from mempool
    pub async fn get(&self, hash: &alloy_primitives::B256) -> Option<MempoolEntry> {
        let txs = self.txs.read().await;
        txs.get(hash).cloned()
    }

    /// Remove transaction from mempool
    pub async fn remove(&self, hash: &alloy_primitives::B256) -> Option<MempoolEntry> {
        let mut txs = self.txs.write().await;
        txs.remove(hash)
    }

    /// Check if mempool contains transaction
    pub async fn contains(&self, hash: &alloy_primitives::B256) -> bool {
        let txs = self.txs.read().await;
        txs.contains_key(hash)
    }

    /// Get mempool size
    pub async fn len(&self) -> usize {
        let txs = self.txs.read().await;
        txs.len()
    }

    /// Check if mempool is empty
    pub async fn is_empty(&self) -> bool {
        let txs = self.txs.read().await;
        txs.is_empty()
    }

    /// Clear all transactions
    pub async fn clear(&self) {
        let mut txs = self.txs.write().await;
        txs.clear();
    }

    /// Get all pending transactions
    pub async fn get_all(&self) -> Vec<(alloy_primitives::B256, MempoolEntry)> {
        let txs = self.txs.read().await;
        txs.iter().map(|(k, v)| (*k, v.clone())).collect()
    }

    /// Clean up old transactions
    pub async fn cleanup(&self) -> usize {
        let mut txs = self.txs.write().await;
        let before = txs.len();

        let to_remove: Vec<_> = txs
            .iter()
            .filter(|(_, entry)| entry.age() > self.max_age)
            .map(|(hash, _)| *hash)
            .collect();

        for hash in to_remove {
            txs.remove(&hash);
        }

        before - txs.len()
    }

    /// Increment retry count for a transaction
    pub async fn increment_retry(&self, hash: &alloy_primitives::B256) -> Option<u32> {
        let mut txs = self.txs.write().await;
        if let Some(entry) = txs.get_mut(hash) {
            entry.retry_count += 1;
            Some(entry.retry_count)
        } else {
            None
        }
    }

    /// Evict oldest transaction when at capacity
    fn evict_oldest(txs: &mut HashMap<alloy_primitives::B256, MempoolEntry>) {
        if let Some(oldest) = txs
            .iter()
            .min_by_key(|(_, entry)| entry.submitted_at)
            .map(|(hash, _)| *hash)
        {
            txs.remove(&oldest);
        }
    }
}

impl Default for Mempool {
    fn default() -> Self {
        Self::new()
    }
}

/// Mempool monitor for tracking transaction status
pub struct MempoolMonitor {
    mempool: Mempool,
    poll_interval: Duration,
}

impl MempoolMonitor {
    pub fn new(mempool: Mempool) -> Self {
        Self {
            mempool,
            poll_interval: Duration::from_secs(1),
        }
    }

    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Start monitoring mempool
    pub async fn start_monitoring<F>(&self, mut callback: F)
    where
        F: FnMut(Vec<(alloy_primitives::B256, MempoolEntry)>),
    {
        loop {
            let txs = self.mempool.get_all().await;
            callback(txs);
            tokio::time::sleep(self.poll_interval).await;
        }
    }
}

/// Mempool stats
#[derive(Debug, Clone, Default)]
pub struct MempoolStats {
    pub total_transactions: usize,
    pub oldest_transaction_secs: u64,
    pub average_retries: f64,
}

impl Mempool {
    /// Get mempool statistics
    pub async fn stats(&self) -> MempoolStats {
        let txs = self.txs.read().await;

        let total = txs.len();
        if total == 0 {
            return MempoolStats::default();
        }

        let oldest = txs.values().map(|e| e.age().as_secs()).max().unwrap_or(0);

        let avg_retries: f64 =
            txs.values().map(|e| e.retry_count as f64).sum::<f64>() / total as f64;

        MempoolStats {
            total_transactions: total,
            oldest_transaction_secs: oldest,
            average_retries: avg_retries,
        }
    }
}

#[cfg(test)]
mod tests {
    use alloy_consensus::TxEnvelope;

    use super::*;

    #[tokio::test]
    async fn test_mempool_add_remove() {
        let mempool = Mempool::new();
        let hash = alloy_primitives::B256::ZERO;

        // Create a minimal transaction using TxEnvelope::Legacy
        let signed_tx = alloy_consensus::Signed::new_unchecked(
            alloy_consensus::TxLegacy {
                chain_id: Some(1),
                nonce: 0,
                gas_price: 0,
                gas_limit: 0,
                to: alloy_primitives::TxKind::Create,
                value: alloy_primitives::U256::ZERO,
                input: alloy_primitives::Bytes::new(),
            },
            alloy_primitives::PrimitiveSignature::new(
                alloy_primitives::U256::ZERO,
                alloy_primitives::U256::ZERO,
                false, // y_parity as bool
            ),
            hash, // tx hash as B256
        );

        let tx = Transaction {
            inner: TxEnvelope::Legacy(signed_tx),
            from: alloy_primitives::Address::ZERO,
            block_hash: None,
            block_number: None,
            transaction_index: None,
            effective_gas_price: None,
        };

        mempool.add(hash, tx.clone()).await;
        assert_eq!(mempool.len().await, 1);
        assert!(mempool.contains(&hash).await);

        let entry = mempool.get(&hash).await;
        assert!(entry.is_some());

        mempool.remove(&hash).await;
        assert_eq!(mempool.len().await, 0);
    }

    #[tokio::test]
    async fn test_mempool_cleanup() {
        let mempool = Mempool::with_limits(100, Duration::from_millis(1));
        let hash = alloy_primitives::B256::ZERO;

        // Create a minimal transaction using TxEnvelope::Legacy
        let signed_tx = alloy_consensus::Signed::new_unchecked(
            alloy_consensus::TxLegacy {
                chain_id: Some(1),
                nonce: 0,
                gas_price: 0,
                gas_limit: 0,
                to: alloy_primitives::TxKind::Create,
                value: alloy_primitives::U256::ZERO,
                input: alloy_primitives::Bytes::new(),
            },
            alloy_primitives::PrimitiveSignature::new(
                alloy_primitives::U256::ZERO,
                alloy_primitives::U256::ZERO,
                false, // y_parity as bool
            ),
            hash, // tx hash as B256
        );

        let tx = Transaction {
            inner: TxEnvelope::Legacy(signed_tx),
            from: alloy_primitives::Address::ZERO,
            block_hash: None,
            block_number: None,
            transaction_index: None,
            effective_gas_price: None,
        };

        mempool.add(hash, tx).await;
        assert_eq!(mempool.len().await, 1);

        // Wait for expiry
        tokio::time::sleep(Duration::from_millis(10)).await;

        let removed = mempool.cleanup().await;
        assert_eq!(removed, 1);
        assert!(mempool.is_empty().await);
    }
}
