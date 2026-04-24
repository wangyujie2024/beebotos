//! Oracle Module

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::compat::{Address, U256};

/// Price data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceData {
    pub token: Address,
    pub price: U256,
    pub decimals: u8,
    pub timestamp: u64,
}

impl PriceData {
    /// Get price as f64
    pub fn price_as_f64(&self) -> f64 {
        let price_bytes: [u8; 32] = self.price.to_be_bytes();
        // Convert U256 to f64 (simplified, assumes 8 decimals)
        let price_u128 = u128::from_be_bytes(price_bytes[16..32].try_into().unwrap_or_default());
        price_u128 as f64 / 1e8
    }
}

/// Price feed trait
#[async_trait::async_trait]
pub trait PriceFeed: Send + Sync {
    /// Get latest price for token
    async fn get_price(&self, token: Address) -> anyhow::Result<PriceData>;

    /// Get prices for multiple tokens
    async fn get_prices(&self, tokens: Vec<Address>) -> anyhow::Result<Vec<PriceData>>;

    /// Check if price is stale
    fn is_stale(&self, price: &PriceData, max_age_secs: u64) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now - price.timestamp > max_age_secs
    }
}

/// Aggregated oracle with multiple sources
pub struct AggregatedOracle {
    sources: Vec<Box<dyn PriceFeed>>,
    aggregators: HashMap<String, Address>,
}

impl AggregatedOracle {
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            aggregators: HashMap::new(),
        }
    }

    pub fn add_source(&mut self, source: Box<dyn PriceFeed>) {
        self.sources.push(source);
    }

    pub fn register_aggregator(&mut self, name: impl Into<String>, aggregator: Address) {
        self.aggregators.insert(name.into(), aggregator);
    }

    /// Get median price from all sources
    pub async fn get_aggregated_price(&self, token: Address) -> anyhow::Result<PriceData> {
        let mut prices = Vec::new();

        for source in &self.sources {
            if let Ok(price) = source.get_price(token).await {
                prices.push(price);
            }
        }

        if prices.is_empty() {
            return Err(anyhow::anyhow!("No price data available"));
        }

        // Sort by price and take median
        prices.sort_by(|a, b| a.price.cmp(&b.price));
        let median = prices[prices.len() / 2].clone();

        Ok(median)
    }

    pub fn get_aggregator(&self, name: &str) -> Option<Address> {
        self.aggregators.get(name).copied()
    }
}

impl Default for AggregatedOracle {
    fn default() -> Self {
        Self::new()
    }
}
