//! Chain Configuration
//!
//! Chain configuration with validation using the validator crate.

use serde::{Deserialize, Serialize};
use tracing::{error, info, instrument, warn};
use validator::{Validate, ValidationError};

use crate::compat::Address;
use crate::constants::{
    DEFAULT_GAS_LIMIT, ETH_ADDRESS_LENGTH, FAST_CONFIRMATION_BLOCKS, MAX_GAS_LIMIT, MIN_GAS_LIMIT,
    NO_CONFIRMATION_BLOCKS,
};
use crate::{ChainError, Result};

/// Custom validation for Ethereum addresses
fn validate_ethereum_address(address: &str) -> std::result::Result<(), ValidationError> {
    if address.is_empty() {
        return Ok(()); // Empty is allowed (optional field)
    }

    // Check if it's a valid Ethereum address
    if !address.starts_with("0x") {
        return Err(ValidationError::new("address_must_start_with_0x"));
    }

    if address.len() != ETH_ADDRESS_LENGTH {
        return Err(ValidationError::new("address_must_be_42_chars"));
    }

    // Check if all characters after 0x are valid hex
    let hex_part = &address[2..];
    if !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ValidationError::new("address_must_be_hex"));
    }

    Ok(())
}

/// Custom validation for RPC URL
fn validate_rpc_url(url: &str) -> std::result::Result<(), ValidationError> {
    if url.is_empty() {
        return Err(ValidationError::new("rpc_url_required"));
    }

    // Check if it's a valid URL
    if !url.starts_with("http://")
        && !url.starts_with("https://")
        && !url.starts_with("ws://")
        && !url.starts_with("wss://")
    {
        return Err(ValidationError::new("rpc_url_must_be_http_or_ws"));
    }

    // Basic URL parsing check
    match url::Url::parse(url) {
        Ok(_) => Ok(()),
        Err(_) => Err(ValidationError::new("rpc_url_invalid")),
    }
}

/// Chain configuration with validation
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ChainConfig {
    /// RPC URL for the blockchain node
    #[validate(custom(function = "validate_rpc_url"))]
    pub rpc_url: String,

    /// Chain ID (must be positive)
    #[validate(range(min = 1, max = 999999999))]
    pub chain_id: u64,

    /// Number of confirmation blocks to wait
    #[validate(range(min = 0, max = 1000))]
    pub confirmation_blocks: u64,

    /// Gas limit for transactions
    #[validate(range(min = 21000, max = MAX_GAS_LIMIT))]
    pub gas_limit: u64,

    /// DAO contract address (optional)
    #[validate(custom(function = "validate_ethereum_address"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dao_address: Option<String>,

    /// Treasury contract address (optional)
    #[validate(custom(function = "validate_ethereum_address"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub treasury_address: Option<String>,

    /// Token contract address (optional)
    #[validate(custom(function = "validate_ethereum_address"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_address: Option<String>,

    /// Identity registry contract address (optional)
    #[validate(custom(function = "validate_ethereum_address"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_registry_address: Option<String>,

    /// Multicall contract address (optional)
    #[validate(custom(function = "validate_ethereum_address"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multicall_address: Option<String>,
}

impl ChainConfig {
    /// Create new chain config with validation
    #[instrument(skip(rpc_url), target = "chain::config")]
    pub fn new(rpc_url: impl Into<String>, chain_id: u64) -> Result<Self> {
        let config = Self {
            rpc_url: rpc_url.into(),
            chain_id,
            confirmation_blocks: FAST_CONFIRMATION_BLOCKS,
            gas_limit: DEFAULT_GAS_LIMIT,
            dao_address: None,
            treasury_address: None,
            token_address: None,
            identity_registry_address: None,
            multicall_address: None,
        };

        config.validate().map_err(|e| {
            ChainError::InvalidConfig(format!("Configuration validation failed: {}", e))
        })?;

        info!(
            target: "chain::config",
            chain_id = chain_id,
            "ChainConfig created successfully"
        );

        Ok(config)
    }

    /// Monad mainnet config
    pub fn monad_mainnet() -> Self {
        Self {
            rpc_url: "https://rpc.monad.xyz".to_string(),
            chain_id: 10_143,
            confirmation_blocks: FAST_CONFIRMATION_BLOCKS,
            gas_limit: DEFAULT_GAS_LIMIT,
            dao_address: None,
            treasury_address: None,
            token_address: None,
            identity_registry_address: None,
            multicall_address: None,
        }
    }

    /// Monad testnet config
    pub fn monad_testnet() -> Result<Self> {
        let config = Self {
            rpc_url: "https://rpc.testnet.monad.xyz".to_string(),
            chain_id: 10_143,
            confirmation_blocks: FAST_CONFIRMATION_BLOCKS,
            gas_limit: DEFAULT_GAS_LIMIT,
            dao_address: std::env::var("DAO_ADDRESS").ok(),
            treasury_address: std::env::var("TREASURY_ADDRESS").ok(),
            token_address: std::env::var("TOKEN_ADDRESS").ok(),
            identity_registry_address: std::env::var("IDENTITY_REGISTRY_ADDRESS").ok(),
            multicall_address: std::env::var("MULTICALL_ADDRESS").ok(),
        };

        config.validate().map_err(|e| {
            ChainError::InvalidConfig(format!("Configuration validation failed: {}", e))
        })?;
        Ok(config)
    }

    /// Local devnet config
    pub fn local() -> Self {
        Self {
            rpc_url: "http://localhost:8545".to_string(),
            chain_id: 1337,
            confirmation_blocks: NO_CONFIRMATION_BLOCKS,
            gas_limit: DEFAULT_GAS_LIMIT,
            dao_address: None,
            treasury_address: None,
            token_address: None,
            identity_registry_address: None,
            multicall_address: None,
        }
    }

    /// Load configuration from environment variables with validation
    #[instrument(target = "chain::config")]
    pub fn from_env() -> Result<Self> {
        use std::env;

        let rpc_url = env::var("CHAIN_RPC_URL")
            .unwrap_or_else(|_| "https://rpc.testnet.monad.xyz".to_string());

        let chain_id = env::var("CHAIN_ID")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10_143);

        let confirmation_blocks = env::var("CONFIRMATION_BLOCKS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);

        let gas_limit = env::var("GAS_LIMIT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_GAS_LIMIT);

        // Validate gas_limit range
        if gas_limit < MIN_GAS_LIMIT || gas_limit > MAX_GAS_LIMIT {
            warn!(
                target: "chain::config",
                gas_limit = gas_limit,
                "Gas limit out of reasonable range, using default"
            );
        }

        let config = Self {
            rpc_url,
            chain_id,
            confirmation_blocks,
            gas_limit,
            dao_address: env::var("DAO_ADDRESS").ok(),
            treasury_address: env::var("TREASURY_ADDRESS").ok(),
            token_address: env::var("TOKEN_ADDRESS").ok(),
            identity_registry_address: env::var("IDENTITY_REGISTRY_ADDRESS").ok(),
            multicall_address: env::var("MULTICALL_ADDRESS").ok(),
        };

        // Validate the configuration
        config.validate().map_err(|e| {
            error!(
                target: "chain::config",
                error = %e,
                "Configuration validation failed"
            );
            ChainError::InvalidConfig(format!("Configuration validation failed: {}", e))
        })?;

        info!(
            target: "chain::config",
            rpc_url = %config.rpc_url,
            chain_id = config.chain_id,
            dao_address = ?config.dao_address,
            treasury_address = ?config.treasury_address,
            token_address = ?config.token_address,
            identity_registry_address = ?config.identity_registry_address,
            multicall_address = ?config.multicall_address,
            "Configuration loaded from environment successfully"
        );

        Ok(config)
    }

    /// Get DAO address as Address type
    pub fn get_dao_address(&self) -> Result<Address> {
        self.parse_address(&self.dao_address, "DAO_ADDRESS")
    }

    /// Get treasury address as Address type
    pub fn get_treasury_address(&self) -> Result<Address> {
        self.parse_address(&self.treasury_address, "TREASURY_ADDRESS")
    }

    /// Get token address as Address type
    pub fn get_token_address(&self) -> Result<Address> {
        self.parse_address(&self.token_address, "TOKEN_ADDRESS")
    }

    /// Get identity registry address as Address type
    pub fn get_identity_registry_address(&self) -> Result<Address> {
        self.parse_address(&self.identity_registry_address, "IDENTITY_REGISTRY_ADDRESS")
    }

    /// Get multicall address as Address type
    pub fn get_multicall_address(&self) -> Result<Address> {
        self.parse_address(&self.multicall_address, "MULTICALL_ADDRESS")
    }

    /// Parse address string to Address type
    fn parse_address(&self, addr_str: &Option<String>, name: &str) -> Result<Address> {
        let addr = addr_str
            .as_ref()
            .ok_or_else(|| ChainError::InvalidConfig(format!("{} not configured", name)))?;

        addr.parse::<Address>()
            .map_err(|_| ChainError::InvalidAddress(format!("Invalid {}: {}", name, addr)))
    }

    /// Set DAO address
    pub fn with_dao_address(mut self, address: impl Into<String>) -> Self {
        self.dao_address = Some(address.into());
        self
    }

    /// Set treasury address
    pub fn with_treasury_address(mut self, address: impl Into<String>) -> Self {
        self.treasury_address = Some(address.into());
        self
    }

    /// Set token address
    pub fn with_token_address(mut self, address: impl Into<String>) -> Self {
        self.token_address = Some(address.into());
        self
    }

    /// Set identity registry address
    pub fn with_identity_registry_address(mut self, address: impl Into<String>) -> Self {
        self.identity_registry_address = Some(address.into());
        self
    }

    /// Set multicall address
    pub fn with_multicall_address(mut self, address: impl Into<String>) -> Self {
        self.multicall_address = Some(address.into());
        self
    }

    /// Check if all required contract addresses are configured
    #[instrument(skip(self), target = "chain::config")]
    pub fn validate_contract_addresses(&self) -> Result<()> {
        let mut missing = Vec::new();

        if self.dao_address.is_none() {
            missing.push("DAO_ADDRESS");
        }
        if self.treasury_address.is_none() {
            missing.push("TREASURY_ADDRESS");
        }
        if self.token_address.is_none() {
            missing.push("TOKEN_ADDRESS");
        }
        if self.identity_registry_address.is_none() {
            missing.push("IDENTITY_REGISTRY_ADDRESS");
        }

        if !missing.is_empty() {
            let msg = format!("Missing contract addresses: {}", missing.join(", "));
            error!(target: "chain::config", "{}", msg);
            return Err(ChainError::InvalidConfig(msg));
        }

        info!(target: "chain::config", "All contract addresses configured");
        Ok(())
    }

    /// Validate and log configuration
    #[instrument(skip(self), target = "chain::config")]
    pub fn validate_and_log(&self) -> Result<()> {
        self.validate().map_err(|e| {
            ChainError::InvalidConfig(format!("Configuration validation failed: {}", e))
        })?;

        info!(
            target: "chain::config",
            rpc_url = %self.rpc_url,
            chain_id = self.chain_id,
            confirmation_blocks = self.confirmation_blocks,
            gas_limit = self.gas_limit,
            "Configuration validation passed"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chain_config_validation() {
        let config = ChainConfig {
            rpc_url: "https://rpc.example.com".to_string(),
            chain_id: 1337,
            confirmation_blocks: FAST_CONFIRMATION_BLOCKS,
            gas_limit: DEFAULT_GAS_LIMIT,
            dao_address: Some("0x1234567890123456789012345678901234567890".to_string()),
            treasury_address: None,
            token_address: None,
            identity_registry_address: None,
            multicall_address: None,
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_chain_config_invalid_rpc_url() {
        let config = ChainConfig {
            rpc_url: "invalid_url".to_string(),
            chain_id: 1337,
            confirmation_blocks: FAST_CONFIRMATION_BLOCKS,
            gas_limit: DEFAULT_GAS_LIMIT,
            dao_address: None,
            treasury_address: None,
            token_address: None,
            identity_registry_address: None,
            multicall_address: None,
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_chain_config_invalid_chain_id() {
        let config = ChainConfig {
            rpc_url: "https://rpc.example.com".to_string(),
            chain_id: 0, // Invalid
            confirmation_blocks: FAST_CONFIRMATION_BLOCKS,
            gas_limit: DEFAULT_GAS_LIMIT,
            dao_address: None,
            treasury_address: None,
            token_address: None,
            identity_registry_address: None,
            multicall_address: None,
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_chain_config_invalid_gas_limit() {
        let config = ChainConfig {
            rpc_url: "https://rpc.example.com".to_string(),
            chain_id: 1337,
            confirmation_blocks: FAST_CONFIRMATION_BLOCKS,
            gas_limit: 1000, // Too low
            dao_address: None,
            treasury_address: None,
            token_address: None,
            identity_registry_address: None,
            multicall_address: None,
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_address_validation() {
        assert!(validate_ethereum_address("").is_ok());
        assert!(validate_ethereum_address("0x1234567890123456789012345678901234567890").is_ok());
        assert!(validate_ethereum_address("0xABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCD").is_ok());

        assert!(validate_ethereum_address("0x123").is_err());
        assert!(validate_ethereum_address("1234567890123456789012345678901234567890").is_err());
        assert!(validate_ethereum_address("0xGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGG").is_err());
    }

    #[test]
    fn test_rpc_url_validation() {
        assert!(validate_rpc_url("https://rpc.example.com").is_ok());
        assert!(validate_rpc_url("http://localhost:8545").is_ok());
        assert!(validate_rpc_url("wss://ws.example.com").is_ok());

        assert!(validate_rpc_url("").is_err());
        assert!(validate_rpc_url("ftp://invalid.com").is_err());
        assert!(validate_rpc_url("not_a_url").is_err());
    }

    #[test]
    fn test_builder_pattern() {
        let config = ChainConfig::new("https://rpc.example.com", 1337)
            .unwrap()
            .with_dao_address("0x1234567890123456789012345678901234567890")
            .with_token_address("0xABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCD");

        assert_eq!(
            config.dao_address,
            Some("0x1234567890123456789012345678901234567890".to_string())
        );
        assert_eq!(
            config.token_address,
            Some("0xABCDEFABCDEFABCDEFABCDEFABCDEFABCDEFABCD".to_string())
        );
    }
}
