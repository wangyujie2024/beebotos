//! Metrics and Monitoring
//!
//! Provides Prometheus-compatible metrics for monitoring chain operations.
//!
//! # Usage
//!
//! ```rust
//! use std::time::Duration;
//!
//! use beebotos_chain::metrics::{ContractMetrics, MetricsCollector};
//!
//! let metrics = MetricsCollector::new();
//! metrics.record_transaction("transfer", true, Some(Duration::from_secs(1)));
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use metrics::{counter, gauge, histogram, Counter, Gauge, Histogram};
use parking_lot::RwLock;

/// Chain operation types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationType {
    Transaction,
    ContractCall,
    Query,
    EstimateGas,
    GetBalance,
    GetBlock,
}

impl std::fmt::Display for OperationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationType::Transaction => write!(f, "transaction"),
            OperationType::ContractCall => write!(f, "contract_call"),
            OperationType::Query => write!(f, "query"),
            OperationType::EstimateGas => write!(f, "estimate_gas"),
            OperationType::GetBalance => write!(f, "get_balance"),
            OperationType::GetBlock => write!(f, "get_block"),
        }
    }
}

/// Chain metrics collector
#[derive(Debug, Clone)]
pub struct MetricsCollector {
    inner: Arc<MetricsCollectorInner>,
}

#[derive(Debug)]
#[allow(dead_code)]
struct MetricsCollectorInner {
    /// Total operations counter
    operations_total: Counter,
    /// Successful operations counter
    operations_success: Counter,
    /// Failed operations counter
    operations_failed: Counter,
    /// Operation duration histogram
    operation_duration: Histogram,
    /// Gas price gauge
    gas_price: Gauge,
    /// Block number gauge
    block_number: Gauge,
    /// Pending transactions gauge
    pending_transactions: Gauge,
    /// Wallet balances by address
    wallet_balances: RwLock<HashMap<String, Gauge>>,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new() -> Self {
        let operations_total = counter!("chain_operations_total", "type" => "all");
        let operations_success = counter!("chain_operations_success_total");
        let operations_failed = counter!("chain_operations_failed_total");
        let operation_duration = histogram!("chain_operation_duration_seconds");
        let gas_price = gauge!("chain_gas_price_gwei");
        let block_number = gauge!("chain_block_number");
        let pending_transactions = gauge!("chain_pending_transactions");

        Self {
            inner: Arc::new(MetricsCollectorInner {
                operations_total,
                operations_success,
                operations_failed,
                operation_duration,
                gas_price,
                block_number,
                pending_transactions,
                wallet_balances: RwLock::new(HashMap::new()),
            }),
        }
    }

    /// Record an operation
    pub fn record_operation(&self, op_type: OperationType, success: bool, duration: Duration) {
        let op_str = op_type.to_string();
        counter!("chain_operations_total", "type" => op_str.clone()).increment(1);

        if success {
            self.inner.operations_success.increment(1);
            counter!("chain_operations_success_total", "type" => op_str).increment(1);
        } else {
            self.inner.operations_failed.increment(1);
            counter!("chain_operations_failed_total", "type" => op_str).increment(1);
        }

        self.inner.operation_duration.record(duration.as_secs_f64());
    }

    /// Record a transaction
    pub fn record_transaction(&self, tx_type: &str, success: bool, duration: Option<Duration>) {
        counter!("chain_transactions_total", "type" => tx_type.to_string()).increment(1);

        if success {
            counter!("chain_transactions_success_total", "type" => tx_type.to_string())
                .increment(1);
        } else {
            counter!("chain_transactions_failed_total", "type" => tx_type.to_string()).increment(1);
        }

        if let Some(d) = duration {
            histogram!("chain_transaction_duration_seconds", "type" => tx_type.to_string())
                .record(d.as_secs_f64());
        }
    }

    /// Update gas price
    pub fn set_gas_price(&self, gas_price_gwei: f64) {
        self.inner.gas_price.set(gas_price_gwei);
    }

    /// Update block number
    pub fn set_block_number(&self, block: u64) {
        self.inner.block_number.set(block as f64);
    }

    /// Update pending transactions count
    pub fn set_pending_transactions(&self, count: usize) {
        self.inner.pending_transactions.set(count as f64);
    }

    /// Update wallet balance
    pub fn set_wallet_balance(&self, address: &str, balance_eth: f64) {
        let mut balances = self.inner.wallet_balances.write();
        let gauge = balances.entry(address.to_string()).or_insert_with(
            || gauge!("chain_wallet_balance_eth", "address" => address.to_string()),
        );
        gauge.set(balance_eth);
    }

    /// Record RPC call
    pub fn record_rpc_call(&self, method: &str, success: bool, latency_ms: f64) {
        let status = if success { "success" } else { "error" };
        counter!("chain_rpc_calls_total", "method" => method.to_string(), "status" => status)
            .increment(1);
        histogram!("chain_rpc_latency_ms", "method" => method.to_string()).record(latency_ms);
    }

    /// Record contract interaction
    pub fn record_contract_interaction(&self, contract: &str, function: &str, success: bool) {
        let status = if success { "success" } else { "error" };
        counter!(
            "chain_contract_interactions_total",
            "contract" => contract.to_string(),
            "function" => function.to_string(),
            "status" => status
        )
        .increment(1);
    }

    /// Get current operation counts
    pub fn get_operation_stats(&self) -> OperationStats {
        // Note: In a real implementation, you'd read from the metrics registry
        // This is a simplified version
        OperationStats {
            total: 0,
            success: 0,
            failed: 0,
        }
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Operation statistics
#[derive(Debug, Clone, Copy)]
pub struct OperationStats {
    pub total: u64,
    pub success: u64,
    pub failed: u64,
}

/// Contract-specific metrics
#[derive(Debug, Clone)]
pub struct ContractMetrics {
    contract_name: String,
    collector: MetricsCollector,
}

impl ContractMetrics {
    /// Create new contract metrics
    pub fn new(contract_name: impl Into<String>, collector: MetricsCollector) -> Self {
        Self {
            contract_name: contract_name.into(),
            collector,
        }
    }

    /// Record function call
    pub fn record_function_call(&self, function: &str, success: bool, duration: Duration) {
        self.collector
            .record_contract_interaction(&self.contract_name, function, success);

        histogram!(
            "chain_contract_call_duration_seconds",
            "contract" => self.contract_name.clone(),
            "function" => function.to_string()
        )
        .record(duration.as_secs_f64());
    }

    /// Record gas usage
    pub fn record_gas_usage(&self, function: &str, gas_used: u64) {
        histogram!(
            "chain_contract_gas_used",
            "contract" => self.contract_name.clone(),
            "function" => function.to_string()
        )
        .record(gas_used as f64);
    }
}

/// Event subscription metrics
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct EventMetrics {
    collector: MetricsCollector,
}

impl EventMetrics {
    /// Create new event metrics
    pub fn new(collector: MetricsCollector) -> Self {
        Self { collector }
    }

    /// Record event received
    pub fn record_event(&self, event_type: &str, block_number: u64) {
        counter!(
            "chain_events_received_total",
            "type" => event_type.to_string()
        )
        .increment(1);

        gauge!(
            "chain_last_event_block",
            "type" => event_type.to_string()
        )
        .set(block_number as f64);
    }

    /// Record subscription reconnect
    pub fn record_reconnect(&self, subscription_type: &str) {
        counter!(
            "chain_event_reconnects_total",
            "type" => subscription_type.to_string()
        )
        .increment(1);
    }
}

/// Wallet metrics
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WalletMetrics {
    collector: MetricsCollector,
}

impl WalletMetrics {
    /// Create new wallet metrics
    pub fn new(collector: MetricsCollector) -> Self {
        Self { collector }
    }

    /// Record transaction sent
    pub fn record_transaction_sent(&self, address: &str, value_eth: f64) {
        counter!(
            "chain_wallet_transactions_sent_total",
            "address" => address.to_string()
        )
        .increment(1);

        histogram!(
            "chain_wallet_transaction_value_eth",
            "address" => address.to_string()
        )
        .record(value_eth);
    }

    /// Record nonce
    pub fn set_nonce(&self, address: &str, nonce: u64) {
        gauge!(
            "chain_wallet_nonce",
            "address" => address.to_string()
        )
        .set(nonce as f64);
    }
}

/// Initialize Prometheus exporter
#[cfg(feature = "prometheus")]
pub fn init_prometheus_exporter(
    bind_addr: &str,
) -> Result<metrics_exporter_prometheus::PrometheusHandle, Box<dyn std::error::Error>> {
    let builder = metrics_exporter_prometheus::PrometheusBuilder::new();
    let handle = builder.install_recorder()?;

    // Set up HTTP server for metrics endpoint
    // Note: In production, you'd integrate this with your HTTP server
    println!(
        "Prometheus metrics available at http://{}/metrics",
        bind_addr
    );

    Ok(handle)
}

/// Metrics middleware for wrapping operations
pub async fn timed_operation<T, E, F, Fut>(
    op_type: OperationType,
    metrics: &MetricsCollector,
    f: F,
) -> Result<T, E>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let start = std::time::Instant::now();
    let result = f().await;
    let duration = start.elapsed();

    metrics.record_operation(op_type, result.is_ok(), duration);

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operation_type_display() {
        assert_eq!(OperationType::Transaction.to_string(), "transaction");
        assert_eq!(OperationType::ContractCall.to_string(), "contract_call");
        assert_eq!(OperationType::Query.to_string(), "query");
    }

    #[test]
    fn test_metrics_collector_creation() {
        let metrics = MetricsCollector::new();
        // Just verify it doesn't panic
        metrics.set_gas_price(20.0);
        metrics.set_block_number(1000);
        metrics.set_pending_transactions(5);
    }

    #[test]
    fn test_contract_metrics() {
        let collector = MetricsCollector::new();
        let contract = ContractMetrics::new("TestContract", collector);

        contract.record_function_call("transfer", true, Duration::from_secs(1));
        contract.record_gas_usage("transfer", 21000);
    }

    #[test]
    fn test_wallet_metrics() {
        let collector = MetricsCollector::new();
        let wallet = WalletMetrics::new(collector);

        wallet.record_transaction_sent("0x1234...", 1.5);
        wallet.set_nonce("0x1234...", 42);
    }

    #[test]
    fn test_event_metrics() {
        let collector = MetricsCollector::new();
        let events = EventMetrics::new(collector);

        events.record_event("Transfer", 1000);
        events.record_reconnect("websocket");
    }
}
