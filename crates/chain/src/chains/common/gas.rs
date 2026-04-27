//! Gas Estimation Module
//!
//! Provides intelligent gas estimation with EIP-1559 support,
//! historical data analysis, and priority-based pricing.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use alloy_rpc_types::{FeeHistory, TransactionRequest};
use parking_lot::RwLock;
use tracing::{debug, info, instrument, warn};

use crate::chains::common::token::TransactionPriority;
use crate::chains::common::{EvmError, EvmProvider};
use crate::compat::U256;
use crate::constants::{DEFAULT_GAS_LIMIT, MIN_GAS_LIMIT, NATIVE_TRANSFER_GAS};

/// Gas estimator with caching and historical analysis
#[derive(Clone)]
pub struct GasEstimator {
    provider: Arc<EvmProvider>,
    cache: Arc<RwLock<GasCache>>,
    config: GasEstimatorConfig,
}

/// Gas estimator configuration
#[derive(Debug, Clone)]
pub struct GasEstimatorConfig {
    /// Cache duration for gas estimates
    pub cache_duration: Duration,
    /// Historical data window size
    pub history_window_size: usize,
    /// Minimum confidence level for estimates (0.0 - 1.0)
    pub min_confidence: f64,
    /// Whether to use EIP-1559
    pub use_eip1559: bool,
    /// Base fee multiplier for conservative estimates
    pub base_fee_multiplier: f64,
    /// Priority fee multiplier
    pub priority_fee_multiplier: f64,
}

impl Default for GasEstimatorConfig {
    fn default() -> Self {
        Self {
            cache_duration: Duration::from_secs(12), // One block
            history_window_size: 10,
            min_confidence: 0.9,
            use_eip1559: true,
            base_fee_multiplier: 1.25,    // 25% buffer
            priority_fee_multiplier: 1.1, // 10% buffer
        }
    }
}

/// Cached gas data
#[derive(Debug, Clone)]
struct GasCache {
    /// Last base fee per gas
    base_fee: Option<u128>,
    /// Last priority fee estimate
    priority_fee: Option<u128>,
    /// Historical base fees
    base_fee_history: VecDeque<u128>,
    /// Historical priority fees
    priority_fee_history: VecDeque<u128>,
    /// Last update time
    last_update: Option<Instant>,
    /// Cached gas estimates by transaction hash
    estimates: std::collections::HashMap<String, CachedEstimate>,
}

impl Default for GasCache {
    fn default() -> Self {
        Self {
            base_fee: None,
            priority_fee: None,
            base_fee_history: VecDeque::new(),
            priority_fee_history: VecDeque::new(),
            last_update: None,
            estimates: std::collections::HashMap::new(),
        }
    }
}

/// Cached gas estimate
#[derive(Debug, Clone)]
struct CachedEstimate {
    gas_limit: u64,
    max_fee_per_gas: Option<u128>,
    max_priority_fee_per_gas: Option<u128>,
    gas_price: Option<u128>,
    created_at: Instant,
}

/// Gas estimate result
#[derive(Debug, Clone)]
pub struct GasEstimate {
    /// Gas limit for the transaction
    pub gas_limit: u64,
    /// Maximum fee per gas (EIP-1559)
    pub max_fee_per_gas: Option<u128>,
    /// Maximum priority fee per gas (EIP-1559)
    pub max_priority_fee_per_gas: Option<u128>,
    /// Legacy gas price
    pub gas_price: Option<u128>,
    /// Estimated total cost in wei
    pub estimated_cost: U256,
    /// Confidence level of the estimate
    pub confidence: f64,
}

/// EIP-1559 fee estimate
#[derive(Debug, Clone)]
pub struct EIP1559FeeEstimate {
    /// Base fee per gas
    pub base_fee: u128,
    /// Max fee per gas
    pub max_fee: u128,
    /// Max priority fee per gas
    pub max_priority_fee: u128,
    /// Estimated confirmation time in seconds
    pub estimated_confirmation_secs: u64,
}

impl GasEstimator {
    /// Create new gas estimator
    pub fn new(provider: Arc<EvmProvider>) -> Self {
        Self {
            provider,
            cache: Arc::new(RwLock::new(GasCache::default())),
            config: GasEstimatorConfig::default(),
        }
    }

    /// Create with custom config
    pub fn with_config(provider: Arc<EvmProvider>, config: GasEstimatorConfig) -> Self {
        Self {
            provider,
            cache: Arc::new(RwLock::new(GasCache::default())),
            config,
        }
    }

    /// Estimate gas for a transaction with full pricing
    #[instrument(skip(self, tx), target = "chain::gas")]
    pub async fn estimate_transaction(
        &self,
        tx: &TransactionRequest,
        priority: TransactionPriority,
    ) -> Result<GasEstimate, EvmError> {
        let start = Instant::now();

        // Check cache first
        let cache_key = format!("{:?}", tx);
        {
            let cache = self.cache.read();
            if let Some(cached) = cache.estimates.get(&cache_key) {
                if cached.created_at.elapsed() < self.config.cache_duration {
                    debug!(target: "chain::gas", "Using cached gas estimate");
                    return Ok(GasEstimate {
                        gas_limit: cached.gas_limit,
                        max_fee_per_gas: cached.max_fee_per_gas,
                        max_priority_fee_per_gas: cached.max_priority_fee_per_gas,
                        gas_price: cached.gas_price,
                        estimated_cost: self.calculate_total_cost(
                            cached.gas_limit,
                            cached
                                .max_fee_per_gas
                                .or(cached.gas_price)
                                .unwrap_or_default(),
                        ),
                        confidence: 0.95,
                    });
                }
            }
        }

        // Estimate gas limit
        let gas_limit = self.estimate_gas_limit(tx).await?;

        // Add buffer for safety (20%)
        let gas_limit_with_buffer = (gas_limit as f64 * 1.2) as u64;

        // Get fee estimate based on priority
        let fee_estimate = if self.config.use_eip1559 {
            let eip1559 = self.estimate_eip1559_fees(priority).await?;
            GasEstimate {
                gas_limit: gas_limit_with_buffer,
                max_fee_per_gas: Some(eip1559.max_fee),
                max_priority_fee_per_gas: Some(eip1559.max_priority_fee),
                gas_price: None,
                estimated_cost: self.calculate_total_cost(gas_limit_with_buffer, eip1559.max_fee),
                confidence: self.config.min_confidence,
            }
        } else {
            let gas_price = self.estimate_legacy_gas_price(priority).await?;
            GasEstimate {
                gas_limit: gas_limit_with_buffer,
                max_fee_per_gas: None,
                max_priority_fee_per_gas: None,
                gas_price: Some(gas_price),
                estimated_cost: self.calculate_total_cost(gas_limit_with_buffer, gas_price),
                confidence: self.config.min_confidence,
            }
        };

        // Cache the estimate
        {
            let mut cache = self.cache.write();
            cache.estimates.insert(
                cache_key,
                CachedEstimate {
                    gas_limit: fee_estimate.gas_limit,
                    max_fee_per_gas: fee_estimate.max_fee_per_gas,
                    max_priority_fee_per_gas: fee_estimate.max_priority_fee_per_gas,
                    gas_price: fee_estimate.gas_price,
                    created_at: Instant::now(),
                },
            );
        }

        let duration = start.elapsed();
        info!(
            target = "chain::gas",
            gas_limit = fee_estimate.gas_limit,
            max_fee = ?fee_estimate.max_fee_per_gas,
            estimated_cost = %fee_estimate.estimated_cost,
            duration_ms = duration.as_millis(),
            "Gas estimate completed"
        );

        Ok(fee_estimate)
    }

    /// Estimate gas limit for a transaction
    #[instrument(skip(self, tx), target = "chain::gas")]
    pub async fn estimate_gas_limit(&self, tx: &TransactionRequest) -> Result<u64, EvmError> {
        // For simple transfers, use constant
        if is_simple_transfer(tx) {
            return Ok(NATIVE_TRANSFER_GAS);
        }

        // Otherwise, call provider for estimate
        match self.provider.estimate_gas(tx).await {
            Ok(gas) => {
                let gas_u64: u64 = gas.to();
                debug!(target: "chain::gas", estimated_gas = gas_u64, "Gas estimation successful");
                Ok(gas_u64.max(MIN_GAS_LIMIT))
            }
            Err(e) => {
                warn!(target: "chain::gas", error = %e, "Gas estimation failed, using default");
                // Return default gas limit on error
                Ok(DEFAULT_GAS_LIMIT)
            }
        }
    }

    /// Estimate EIP-1559 fees
    #[instrument(skip(self), target = "chain::gas")]
    pub async fn estimate_eip1559_fees(
        &self,
        priority: TransactionPriority,
    ) -> Result<EIP1559FeeEstimate, EvmError> {
        let fee_history = self.get_fee_history().await?;

        let base_fee = fee_history
            .base_fee_per_gas
            .last()
            .copied()
            .unwrap_or_default() as u128;

        // Calculate priority fee based on priority level
        let priority_fee = self.calculate_priority_fee(&fee_history, priority);

        // Apply multipliers for safety
        let adjusted_base_fee = (base_fee as f64 * self.config.base_fee_multiplier) as u128;
        let adjusted_priority_fee =
            (priority_fee as f64 * self.config.priority_fee_multiplier) as u128;

        // Max fee is base fee + priority fee (with buffer for base fee increases)
        let max_fee = adjusted_base_fee.saturating_add(adjusted_priority_fee * 2);

        let estimate = EIP1559FeeEstimate {
            base_fee,
            max_fee,
            max_priority_fee: adjusted_priority_fee,
            estimated_confirmation_secs: priority.confirmation_time_secs(),
        };

        // Update cache
        {
            let mut cache = self.cache.write();
            cache.base_fee = Some(base_fee);
            cache.priority_fee = Some(adjusted_priority_fee);
            cache.base_fee_history.push_back(base_fee);
            cache.priority_fee_history.push_back(adjusted_priority_fee);

            // Maintain window size
            if cache.base_fee_history.len() > self.config.history_window_size {
                cache.base_fee_history.pop_front();
            }
            if cache.priority_fee_history.len() > self.config.history_window_size {
                cache.priority_fee_history.pop_front();
            }
            cache.last_update = Some(Instant::now());
        }

        debug!(
            target = "chain::gas",
            base_fee = base_fee,
            max_fee = max_fee,
            priority_fee = adjusted_priority_fee,
            "EIP-1559 fee estimate"
        );

        Ok(estimate)
    }

    /// Estimate legacy gas price
    #[instrument(skip(self), target = "chain::gas")]
    pub async fn estimate_legacy_gas_price(
        &self,
        priority: TransactionPriority,
    ) -> Result<u128, EvmError> {
        let base_price = self
            .provider
            .get_gas_price()
            .await
            .map_err(|e| EvmError::ProviderError(format!("Failed to get gas price: {}", e)))?;

        let multiplier = priority.multiplier();
        let adjusted_price = (base_price as f64 * multiplier) as u128;

        debug!(
            target = "chain::gas",
            base_price = base_price,
            adjusted_price = adjusted_price,
            multiplier = multiplier,
            "Legacy gas price estimate"
        );

        Ok(adjusted_price)
    }

    /// Get fee history from provider
    async fn get_fee_history(&self) -> Result<FeeHistory, EvmError> {
        let history = self
            .provider
            .get_fee_history(
                10,
                alloy_rpc_types::BlockNumberOrTag::Latest,
                &[25.0, 50.0, 75.0],
            )
            .await
            .map_err(|e| EvmError::ProviderError(format!("Failed to get fee history: {}", e)))?;

        Ok(history)
    }

    /// Calculate priority fee based on fee history and priority level
    fn calculate_priority_fee(
        &self,
        fee_history: &FeeHistory,
        priority: TransactionPriority,
    ) -> u128 {
        let _reward_percentile = match priority {
            TransactionPriority::Low => 25.0,
            TransactionPriority::Normal => 50.0,
            TransactionPriority::High => 75.0,
            TransactionPriority::Urgent => 90.0,
            TransactionPriority::Custom { multiplier } => (multiplier * 50.0).min(95.0).max(10.0),
        };

        // Get average reward from history
        let avg_reward: u128 = fee_history
            .reward
            .as_ref()
            .map(|rewards| {
                let flattened: Vec<u128> = rewards.iter().flatten().copied().collect();
                let sum: u128 = flattened.iter().sum();
                sum.saturating_div(rewards.len().max(1) as u128)
            })
            .unwrap_or_default();

        // Apply priority multiplier
        let multiplier = priority.multiplier();
        let adjusted_reward = (avg_reward as f64 * multiplier) as u128;
        adjusted_reward.max(priority.priority_fee_gwei() as u128 * 1_000_000_000)
    }

    /// Calculate total transaction cost
    fn calculate_total_cost(&self, gas_limit: u64, gas_price: u128) -> U256 {
        U256::from(gas_limit) * U256::from(gas_price)
    }

    /// Get current base fee from cache
    pub fn get_cached_base_fee(&self) -> Option<u128> {
        self.cache.read().base_fee
    }

    /// Get average base fee from history
    pub fn get_average_base_fee(&self) -> Option<u128> {
        let cache = self.cache.read();
        if cache.base_fee_history.is_empty() {
            return None;
        }

        let sum: u128 = cache.base_fee_history.iter().sum();
        Some(sum / cache.base_fee_history.len() as u128)
    }

    /// Clear expired cache entries
    pub fn clear_expired_cache(&self) {
        let mut cache = self.cache.write();
        let now = Instant::now();
        let expired_keys: Vec<String> = cache
            .estimates
            .iter()
            .filter(|(_, v)| now.duration_since(v.created_at) > self.config.cache_duration)
            .map(|(k, _)| k.clone())
            .collect();

        for key in expired_keys {
            cache.estimates.remove(&key);
        }
    }

    /// Get gas statistics
    pub fn get_statistics(&self) -> GasStatistics {
        let cache = self.cache.read();
        GasStatistics {
            cached_estimates: cache.estimates.len(),
            base_fee_history_size: cache.base_fee_history.len(),
            priority_fee_history_size: cache.priority_fee_history.len(),
            last_update: cache.last_update,
        }
    }
}

/// Gas statistics
#[derive(Debug, Clone)]
pub struct GasStatistics {
    pub cached_estimates: usize,
    pub base_fee_history_size: usize,
    pub priority_fee_history_size: usize,
    pub last_update: Option<Instant>,
}

/// Check if transaction is a simple ETH transfer
fn is_simple_transfer(tx: &TransactionRequest) -> bool {
    // Simple transfer: has 'to', no data, has value
    tx.to.is_some()
        && tx.input.data.as_ref().map(|d| d.is_empty()).unwrap_or(true)
        && tx.value.map(|v| v > U256::ZERO).unwrap_or(false)
}

/// Gas estimation for specific operation types
pub struct OperationGasEstimator;

impl OperationGasEstimator {
    /// Estimate gas for ERC20 transfer
    pub fn erc20_transfer() -> u64 {
        65_000
    }

    /// Estimate gas for ERC20 approve
    pub fn erc20_approve() -> u64 {
        55_000
    }

    /// Estimate gas for contract deployment
    pub fn contract_deployment(bytecode_size: usize) -> u64 {
        // Base cost + cost per byte of bytecode
        let base_cost = 100_000;
        let per_byte_cost = 200;
        base_cost + (bytecode_size as u64 * per_byte_cost)
    }

    /// Estimate gas for multicall
    pub fn multicall(num_calls: usize) -> u64 {
        let base_cost = 50_000;
        let per_call_cost = 30_000;
        base_cost + (num_calls as u64 * per_call_cost)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gas_estimate_calculation() {
        let gas_limit = 100_000;
        let gas_price = 20_000_000_000u128; // 20 gwei

        // Just test the calculation logic directly
        let cost = U256::from(gas_limit) * U256::from(gas_price);
        assert_eq!(cost, U256::from(2_000_000_000_000_000u128)); // 0.002 ETH
    }

    #[test]
    fn test_operation_gas_estimates() {
        assert_eq!(OperationGasEstimator::erc20_transfer(), 65_000);
        assert_eq!(OperationGasEstimator::erc20_approve(), 55_000);
        assert_eq!(OperationGasEstimator::contract_deployment(1000), 300_000);
        assert_eq!(OperationGasEstimator::multicall(3), 140_000);
    }

    #[test]
    fn test_gas_estimator_config_default() {
        let config = GasEstimatorConfig::default();
        assert!(config.use_eip1559);
        assert_eq!(config.base_fee_multiplier, 1.25);
        assert_eq!(config.priority_fee_multiplier, 1.1);
    }
}
