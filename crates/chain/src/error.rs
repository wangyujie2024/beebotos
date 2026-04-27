//! Chain Errors

use thiserror::Error;

use crate::compat::B256;

/// Chain error types
#[derive(Debug, Clone, Error, PartialEq)]
pub enum ChainError {
    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Contract error: {0}")]
    Contract(String),

    #[error("Transaction failed: {tx_hash:?} - {reason}")]
    TransactionFailed { tx_hash: B256, reason: String },

    #[error("Insufficient balance")]
    InsufficientBalance,

    #[error("Invalid address: {0}")]
    InvalidAddress(String),

    #[error("DAO error: {0}")]
    DAO(String),

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Wallet error: {0}")]
    Wallet(String),

    #[error("URL parse error: {0}")]
    UrlParse(String),

    #[error("Alloy provider error: {0}")]
    AlloyProvider(String),

    #[error("Alloy signer error: {0}")]
    AlloySigner(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Identity error: {0}")]
    Identity(String),

    #[error("Bridge error: {0}")]
    Bridge(String),

    #[error("Oracle error: {0}")]
    Oracle(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),

    #[error("Timeout error: {0}")]
    Timeout(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Transaction error: {0}")]
    Transaction(String),

    #[error("{0}")]
    Other(String),
}

/// Result type alias
pub type Result<T> = std::result::Result<T, ChainError>;

// Error conversions
impl From<alloy_contract::Error> for ChainError {
    fn from(e: alloy_contract::Error) -> Self {
        ChainError::Contract(format!("Contract error: {}", e))
    }
}

impl From<alloy_signer::Error> for ChainError {
    fn from(e: alloy_signer::Error) -> Self {
        ChainError::AlloySigner(e.to_string())
    }
}

impl From<url::ParseError> for ChainError {
    fn from(e: url::ParseError) -> Self {
        ChainError::UrlParse(e.to_string())
    }
}

impl From<serde_json::Error> for ChainError {
    fn from(e: serde_json::Error) -> Self {
        ChainError::Serialization(e.to_string())
    }
}

impl From<std::io::Error> for ChainError {
    fn from(e: std::io::Error) -> Self {
        ChainError::Connection(e.to_string())
    }
}
