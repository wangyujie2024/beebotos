//! Macros for Chain Types
//!
//! Provides declarative macros to reduce boilerplate when defining
//! chain-specific Block, Transaction, and Event types.

/// Macro to generate `From<Block>` implementation for chain-specific block
/// types
#[macro_export]
macro_rules! define_block_from {
    (
        $block_type:ty,
        $number:ident, $hash:ident, $parent_hash:ident, $timestamp:ident,
        $gas_limit:ident, $gas_used:ident, $transactions:ident, $validator:ident
        $(, $field:ident: $field_val:expr )* $(,)?
    ) => {
        impl From<alloy_rpc_types::Block> for $block_type {
            fn from(block: alloy_rpc_types::Block) -> Self {
                Self {
                    number: block.header.number,
                    hash: format!("{:?}", block.header.hash),
                    parent_hash: format!("{:?}", block.header.parent_hash),
                    timestamp: block.header.timestamp,
                    gas_limit: block.header.gas_limit,
                    gas_used: block.header.gas_used,
                    transactions: block.transactions.hashes().map(|h| format!("{:?}", h)).collect(),
                    validator: format!("{:?}", block.header.beneficiary),
                    $( $field: $field_val, )*
                }
            }
        }
    };
}

/// Macro to generate `From<Transaction>` implementation for chain-specific
/// transaction types
#[macro_export]
macro_rules! define_transaction_from {
    (
        $tx_type:ty,
        $hash:ident, $from:ident, $to:ident, $val:ident,
        $gas_price:ident, $gas_limit:ident, $nonce:ident, $data:ident
        $(, $field:ident: $field_val:expr )* $(,)?
    ) => {
        impl From<alloy_rpc_types::Transaction> for $tx_type {
            fn from(tx: alloy_rpc_types::Transaction) -> Self {
                use alloy_consensus::Transaction as _;
                Self {
                    hash: format!("{:?}", tx.inner.tx_hash()),
                    from: format!("{:?}", tx.from),
                    to: tx.to().map(|a| format!("{:?}", a)),
                    value: tx.value().to_string(),
                    gas_price: tx.gas_price().map(|p| p.to_string()),
                    gas_limit: tx.gas_limit(),
                    nonce: tx.nonce(),
                    data: format!("0x{}", hex::encode(tx.input().as_ref())),
                    status: None,
                    block_number: tx.block_number,
                    block_hash: tx.block_hash.map(|h| format!("{:?}", h)),
                    gas_used: None,
                    effective_gas_price: None,
                    $( $field: $field_val, )*
                }
            }
        }
    };
}

/// Macro to generate `From<TransactionReceipt>` implementation
#[macro_export]
macro_rules! define_receipt_from {
    (
        $tx_type:ty
        $(, $field:ident: $field_val:expr )* $(,)?
    ) => {
        impl From<alloy_rpc_types::TransactionReceipt> for $tx_type {
            fn from(receipt: alloy_rpc_types::TransactionReceipt) -> Self {
                Self {
                    hash: format!("{:?}", receipt.transaction_hash),
                    from: format!("{:?}", receipt.from),
                    to: receipt.to.map(|a| format!("{:?}", a)),
                    value: "0".to_string(),
                    gas_price: None,
                    gas_limit: 0,
                    nonce: 0,
                    data: String::new(),
                    status: Some(receipt.status()),
                    block_number: receipt.block_number,
                    block_hash: receipt.block_hash.map(|h| format!("{:?}", h)),
                    gas_used: Some(receipt.gas_used as u64),
                    effective_gas_price: Some(receipt.effective_gas_price.to_string()),
                    $( $field: $field_val, )*
                }
            }
        }
    };
}

/// Macro to generate `From<Log>` implementation for chain-specific event types
#[macro_export]
macro_rules! define_event_from {
    (
        $event_type:ty
        $(, $field:ident: $field_val:expr )* $(,)?
    ) => {
        impl From<alloy_rpc_types::Log> for $event_type {
            fn from(log: alloy_rpc_types::Log) -> Self {
                Self {
                    address: format!("{:?}", log.address()),
                    topics: log.topics().iter().map(|t| format!("{:?}", t)).collect(),
                    data: format!("0x{}", hex::encode(log.data().data.clone().as_ref())),
                    block_number: log.block_number.unwrap_or_default(),
                    transaction_hash: format!("{:?}", log.transaction_hash.unwrap_or_default()),
                    log_index: log.log_index.unwrap_or_default(),
                    $( $field: $field_val, )*
                }
            }
        }
    };
}

/// Macro to define a complete set of chain types with all From implementations
#[macro_export]
macro_rules! define_chain_types {
    (
        block: $block_name:ident { $( $block_field:ident: $block_type:ty = $block_default:expr ),* $(,)? },
        transaction: $tx_name:ident { $( $tx_field:ident: $tx_field_type:ty = $tx_default:expr ),* $(,)? },
        event: $event_name:ident { $( $event_field:ident: $event_field_type:ty = $event_default:expr ),* $(,)? }
    ) => {
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub struct $block_name {
            pub number: u64,
            pub hash: String,
            pub parent_hash: String,
            pub timestamp: u64,
            pub gas_limit: u64,
            pub gas_used: u64,
            pub transactions: Vec<String>,
            pub validator: String,
            $( pub $block_field: $block_type, )*
        }

        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub struct $tx_name {
            pub hash: String,
            pub from: String,
            pub to: Option<String>,
            pub value: String,
            pub gas_price: Option<String>,
            pub gas_limit: u64,
            pub nonce: u64,
            pub data: String,
            pub status: Option<bool>,
            pub block_number: Option<u64>,
            pub block_hash: Option<String>,
            pub gas_used: Option<u64>,
            pub effective_gas_price: Option<String>,
            pub tx_type: u8,
            $( pub $tx_field: $tx_field_type, )*
        }

        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub struct $event_name {
            pub address: String,
            pub topics: Vec<String>,
            pub data: String,
            pub block_number: u64,
            pub transaction_hash: String,
            pub log_index: u64,
            pub finalized: bool,
            $( pub $event_field: $event_field_type, )*
        }
    };

    (
        block: $block_name:ident { $( $block_field:ident: $block_type:ty ),* $(,)? },
        transaction: $tx_name:ident { $( $tx_field:ident: $tx_field_type:ty ),* $(,)? },
        event: $event_name:ident { $( $event_field:ident: $event_field_type:ty ),* $(,)? }
    ) => {
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub struct $block_name {
            pub number: u64,
            pub hash: String,
            pub parent_hash: String,
            pub timestamp: u64,
            pub gas_limit: u64,
            pub gas_used: u64,
            pub transactions: Vec<String>,
            pub validator: String,
            $( pub $block_field: $block_type, )*
        }

        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub struct $tx_name {
            pub hash: String,
            pub from: String,
            pub to: Option<String>,
            pub value: String,
            pub gas_price: Option<String>,
            pub gas_limit: u64,
            pub nonce: u64,
            pub data: String,
            pub status: Option<bool>,
            pub block_number: Option<u64>,
            pub block_hash: Option<String>,
            pub gas_used: Option<u64>,
            pub effective_gas_price: Option<String>,
            pub tx_type: u8,
            $( pub $tx_field: $tx_field_type, )*
        }

        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub struct $event_name {
            pub address: String,
            pub topics: Vec<String>,
            pub data: String,
            pub block_number: u64,
            pub transaction_hash: String,
            pub log_index: u64,
            pub finalized: bool,
            $( pub $event_field: $event_field_type, )*
        }
    };

    (
        block: $block_name:ident,
        transaction: $tx_name:ident,
        event: $event_name:ident
    ) => {
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub struct $block_name {
            pub number: u64,
            pub hash: String,
            pub parent_hash: String,
            pub timestamp: u64,
            pub gas_limit: u64,
            pub gas_used: u64,
            pub transactions: Vec<String>,
            pub validator: String,
        }

        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub struct $tx_name {
            pub hash: String,
            pub from: String,
            pub to: Option<String>,
            pub value: String,
            pub gas_price: Option<String>,
            pub gas_limit: u64,
            pub nonce: u64,
            pub data: String,
            pub status: Option<bool>,
            pub block_number: Option<u64>,
            pub block_hash: Option<String>,
            pub gas_used: Option<u64>,
            pub effective_gas_price: Option<String>,
            pub tx_type: u8,
        }

        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
        pub struct $event_name {
            pub address: String,
            pub topics: Vec<String>,
            pub data: String,
            pub block_number: u64,
            pub transaction_hash: String,
            pub log_index: u64,
            pub finalized: bool,
        }
    };
}
