//! Blockchain & DeFi Syscall Handlers
//!
//! Implements system calls for:
//! - ExecutePayment: Execute blockchain payments
//! - BridgeToken: Cross-chain token bridging
//! - SwapToken: DEX token swapping
//! - StakeToken/UnstakeToken: Staking operations

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;
use tracing::{error, info, trace, warn};

use crate::capabilities::CapabilityLevel;
use crate::syscalls::handlers::{read_caller_memory, write_caller_memory};
use crate::syscalls::{SyscallArgs, SyscallContext, SyscallError, SyscallHandler, SyscallResult};

// Global blockchain client reference
static BLOCKCHAIN_CLIENT: RwLock<Option<Arc<dyn BlockchainClient>>> = RwLock::new(None);

/// Initialize blockchain client
pub fn init_blockchain_client(client: Arc<dyn BlockchainClient>) {
    let mut guard = BLOCKCHAIN_CLIENT.write();
    *guard = Some(client);
}

/// Get blockchain client if initialized
fn get_blockchain_client() -> Option<Arc<dyn BlockchainClient>> {
    BLOCKCHAIN_CLIENT.read().as_ref().cloned()
}

/// Blockchain client trait for kernel integration
#[async_trait]
pub trait BlockchainClient: Send + Sync {
    /// Execute a payment transaction
    async fn execute_payment(
        &self,
        from: &str,
        to: &str,
        amount: &str,
        token: Option<&str>,
    ) -> Result<String, BlockchainError>;

    /// Bridge tokens across chains
    async fn bridge_tokens(
        &self,
        from_chain: u64,
        to_chain: u64,
        token: &str,
        amount: &str,
        recipient: &str,
    ) -> Result<String, BlockchainError>;

    /// Swap tokens on DEX
    async fn swap_tokens(
        &self,
        token_in: &str,
        token_out: &str,
        amount_in: &str,
        min_amount_out: &str,
    ) -> Result<SwapResult, BlockchainError>;

    /// Stake tokens
    async fn stake_tokens(&self, token: &str, amount: &str) -> Result<String, BlockchainError>;

    /// Unstake tokens
    async fn unstake_tokens(&self, token: &str, amount: &str) -> Result<String, BlockchainError>;

    /// Query token balance
    async fn query_balance(
        &self,
        address: &str,
        token: Option<&str>,
    ) -> Result<String, BlockchainError>;
}

/// Blockchain error types
#[derive(Debug, Clone)]
pub enum BlockchainError {
    /// Insufficient funds for transaction
    InsufficientFunds,
    /// Invalid blockchain address
    InvalidAddress,
    /// Invalid transaction amount
    InvalidAmount,
    /// Transaction failed with message
    TransactionFailed(String),
    /// Network communication error
    NetworkError(String),
    /// Feature not yet implemented
    NotImplemented,
}

impl std::fmt::Display for BlockchainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockchainError::InsufficientFunds => write!(f, "Insufficient funds"),
            BlockchainError::InvalidAddress => write!(f, "Invalid address"),
            BlockchainError::InvalidAmount => write!(f, "Invalid amount"),
            BlockchainError::TransactionFailed(e) => write!(f, "Transaction failed: {}", e),
            BlockchainError::NetworkError(e) => write!(f, "Network error: {}", e),
            BlockchainError::NotImplemented => write!(f, "Feature not implemented"),
        }
    }
}

impl std::error::Error for BlockchainError {}

/// Swap result
#[derive(Debug, Clone)]
pub struct SwapResult {
    /// Transaction hash of the swap
    pub tx_hash: String,
    /// Amount of tokens received
    pub amount_out: String,
    /// Price impact as a ratio (0.01 = 1%)
    pub price_impact: f64,
}

/// Read string from caller memory
fn read_string(ctx: &SyscallContext, ptr: u64, len: usize) -> Result<String, SyscallError> {
    if ptr == 0 || len == 0 || len > 1024 {
        return Err(SyscallError::InvalidArgs);
    }

    let bytes = read_caller_memory(ctx, ptr, len).map_err(|_| SyscallError::InvalidArgs)?;

    String::from_utf8(bytes).map_err(|_| SyscallError::InvalidArgs)
}

/// Check capability level
fn check_capability(ctx: &SyscallContext, required: CapabilityLevel) -> SyscallResult {
    if ctx.capability_level < required as u8 {
        warn!(
            "Capability check failed: required {:?} (level {}), have level {}",
            required, required as u8, ctx.capability_level
        );
        return SyscallResult::Error(SyscallError::PermissionDenied);
    }
    SyscallResult::Success(0)
}

// =============================================================================
// Payment Syscalls
// =============================================================================

/// Execute payment (syscall 4)
pub struct ExecutePaymentHandler;

#[async_trait]
impl SyscallHandler for ExecutePaymentHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("ExecutePayment syscall from {}", ctx.caller_id);

        // Check capability - requires L8 for blockchain writes
        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L8ChainWriteLow) {
            return SyscallResult::Error(e);
        }

        let recipient_ptr = args.arg0;
        let recipient_len = args.arg1 as usize;
        let amount_ptr = args.arg2;
        let amount_len = args.arg3 as usize;
        let token_ptr = args.arg4;
        let token_len = args.arg5 as usize;

        // Read recipient address
        let recipient = match read_string(ctx, recipient_ptr, recipient_len) {
            Ok(s) => s,
            Err(e) => return SyscallResult::Error(e),
        };

        // Read amount
        let amount = match read_string(ctx, amount_ptr, amount_len) {
            Ok(s) => s,
            Err(e) => return SyscallResult::Error(e),
        };

        // Read optional token address (empty string means native token)
        let token = if token_ptr != 0 && token_len > 0 {
            match read_string(ctx, token_ptr, token_len) {
                Ok(s) if !s.is_empty() => Some(s),
                _ => None,
            }
        } else {
            None
        };

        trace!(
            "ExecutePayment: from={} to={} amount={} token={:?}",
            ctx.caller_id,
            recipient,
            amount,
            token
        );

        // Get blockchain client
        let client = match get_blockchain_client() {
            Some(c) => c,
            None => {
                warn!("Blockchain client not initialized");
                return SyscallResult::Error(SyscallError::NotImplemented);
            }
        };

        // Execute payment
        match client
            .execute_payment(&ctx.caller_id, &recipient, &amount, token.as_deref())
            .await
        {
            Ok(tx_hash) => {
                info!(
                    "Payment executed: from={} to={} amount={} tx={}",
                    ctx.caller_id, recipient, amount, tx_hash
                );
                // Return transaction hash as u64 handle
                let handle = hash_to_handle(&tx_hash);
                SyscallResult::Success(handle)
            }
            Err(BlockchainError::InsufficientFunds) => {
                SyscallResult::Error(SyscallError::QuotaExceeded)
            }
            Err(BlockchainError::InvalidAddress) => SyscallResult::Error(SyscallError::InvalidArgs),
            Err(BlockchainError::InvalidAmount) => SyscallResult::Error(SyscallError::InvalidArgs),
            Err(e) => {
                error!("Payment failed: {}", e);
                SyscallResult::Error(SyscallError::InternalError)
            }
        }
    }
}

// =============================================================================
// Bridge Syscalls
// =============================================================================

/// Bridge tokens across chains (syscall 19)
pub struct BridgeTokenHandler;

#[async_trait]
impl SyscallHandler for BridgeTokenHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("BridgeToken syscall from {}", ctx.caller_id);

        // Requires L8 for cross-chain operations
        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L8ChainWriteLow) {
            return SyscallResult::Error(e);
        }

        let from_chain = args.arg0 as u64;
        let to_chain = args.arg1 as u64;
        let config_ptr = args.arg2;
        let config_len = args.arg3 as usize;

        if config_ptr == 0 || config_len == 0 || config_len > 4096 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        let config_bytes = match read_caller_memory(ctx, config_ptr, config_len) {
            Ok(b) => b,
            Err(e) => return SyscallResult::Error(e),
        };

        let config: BridgeConfig = match serde_json::from_slice(&config_bytes) {
            Ok(c) => c,
            Err(e) => {
                warn!("Invalid bridge config: {}", e);
                return SyscallResult::Error(SyscallError::InvalidArgs);
            }
        };

        trace!(
            "BridgeToken: from_chain={} to_chain={} token={} amount={}",
            from_chain,
            to_chain,
            config.token,
            config.amount
        );

        let client = match get_blockchain_client() {
            Some(c) => c,
            None => return SyscallResult::Error(SyscallError::NotImplemented),
        };

        match client
            .bridge_tokens(
                from_chain,
                to_chain,
                &config.token,
                &config.amount,
                &config.recipient,
            )
            .await
        {
            Ok(tx_hash) => {
                info!(
                    "Bridge initiated: {} -> {} token={} amount={} tx={}",
                    from_chain, to_chain, config.token, config.amount, tx_hash
                );
                let handle = hash_to_handle(&tx_hash);
                SyscallResult::Success(handle)
            }
            Err(e) => {
                error!("Bridge failed: {}", e);
                SyscallResult::Error(SyscallError::InternalError)
            }
        }
    }
}

/// Bridge configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct BridgeConfig {
    token: String,
    amount: String,
    recipient: String,
}

// =============================================================================
// DEX Syscalls
// =============================================================================

/// Swap tokens on DEX (syscall 20)
pub struct SwapTokenHandler;

#[async_trait]
impl SyscallHandler for SwapTokenHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("SwapToken syscall from {}", ctx.caller_id);

        // Requires L8 for DEX operations
        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L8ChainWriteLow) {
            return SyscallResult::Error(e);
        }

        let config_ptr = args.arg0;
        let config_len = args.arg1 as usize;

        if config_ptr == 0 || config_len == 0 || config_len > 4096 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        let config_bytes = match read_caller_memory(ctx, config_ptr, config_len) {
            Ok(b) => b,
            Err(e) => return SyscallResult::Error(e),
        };

        let config: SwapConfig = match serde_json::from_slice(&config_bytes) {
            Ok(c) => c,
            Err(e) => {
                warn!("Invalid swap config: {}", e);
                return SyscallResult::Error(SyscallError::InvalidArgs);
            }
        };

        trace!(
            "SwapToken: token_in={} token_out={} amount_in={}",
            config.token_in,
            config.token_out,
            config.amount_in
        );

        let client = match get_blockchain_client() {
            Some(c) => c,
            None => return SyscallResult::Error(SyscallError::NotImplemented),
        };

        match client
            .swap_tokens(
                &config.token_in,
                &config.token_out,
                &config.amount_in,
                &config.min_amount_out,
            )
            .await
        {
            Ok(result) => {
                info!(
                    "Swap executed: {} -> {} amount_out={} impact={:.2}% tx={}",
                    config.token_in,
                    config.token_out,
                    result.amount_out,
                    result.price_impact * 100.0,
                    result.tx_hash
                );
                let handle = hash_to_handle(&result.tx_hash);
                SyscallResult::Success(handle)
            }
            Err(e) => {
                error!("Swap failed: {}", e);
                SyscallResult::Error(SyscallError::InternalError)
            }
        }
    }
}

/// Swap configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SwapConfig {
    token_in: String,
    token_out: String,
    amount_in: String,
    min_amount_out: String,
}

// =============================================================================
// Staking Syscalls
// =============================================================================

/// Stake tokens (syscall 21)
pub struct StakeTokenHandler;

#[async_trait]
impl SyscallHandler for StakeTokenHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("StakeToken syscall from {}", ctx.caller_id);

        // Requires L8 for staking
        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L8ChainWriteLow) {
            return SyscallResult::Error(e);
        }

        let token_ptr = args.arg0;
        let token_len = args.arg1 as usize;
        let amount_ptr = args.arg2;
        let amount_len = args.arg3 as usize;

        let token = match read_string(ctx, token_ptr, token_len) {
            Ok(s) => s,
            Err(e) => return SyscallResult::Error(e),
        };

        let amount = match read_string(ctx, amount_ptr, amount_len) {
            Ok(s) => s,
            Err(e) => return SyscallResult::Error(e),
        };

        trace!("StakeToken: token={} amount={}", token, amount);

        let client = match get_blockchain_client() {
            Some(c) => c,
            None => return SyscallResult::Error(SyscallError::NotImplemented),
        };

        match client.stake_tokens(&token, &amount).await {
            Ok(tx_hash) => {
                info!(
                    "Stake executed: token={} amount={} tx={}",
                    token, amount, tx_hash
                );
                let handle = hash_to_handle(&tx_hash);
                SyscallResult::Success(handle)
            }
            Err(e) => {
                error!("Stake failed: {}", e);
                SyscallResult::Error(SyscallError::InternalError)
            }
        }
    }
}

/// Unstake tokens (syscall 22)
pub struct UnstakeTokenHandler;

#[async_trait]
impl SyscallHandler for UnstakeTokenHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("UnstakeToken syscall from {}", ctx.caller_id);

        // Requires L8 for unstaking
        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L8ChainWriteLow) {
            return SyscallResult::Error(e);
        }

        let token_ptr = args.arg0;
        let token_len = args.arg1 as usize;
        let amount_ptr = args.arg2;
        let amount_len = args.arg3 as usize;

        let token = match read_string(ctx, token_ptr, token_len) {
            Ok(s) => s,
            Err(e) => return SyscallResult::Error(e),
        };

        let amount = match read_string(ctx, amount_ptr, amount_len) {
            Ok(s) => s,
            Err(e) => return SyscallResult::Error(e),
        };

        trace!("UnstakeToken: token={} amount={}", token, amount);

        let client = match get_blockchain_client() {
            Some(c) => c,
            None => return SyscallResult::Error(SyscallError::NotImplemented),
        };

        match client.unstake_tokens(&token, &amount).await {
            Ok(tx_hash) => {
                info!(
                    "Unstake executed: token={} amount={} tx={}",
                    token, amount, tx_hash
                );
                let handle = hash_to_handle(&tx_hash);
                SyscallResult::Success(handle)
            }
            Err(e) => {
                error!("Unstake failed: {}", e);
                SyscallResult::Error(SyscallError::InternalError)
            }
        }
    }
}

/// Query token balance (syscall 23)
pub struct QueryBalanceHandler;

#[async_trait]
impl SyscallHandler for QueryBalanceHandler {
    async fn handle(&self, args: SyscallArgs, ctx: &SyscallContext) -> SyscallResult {
        trace!("QueryBalance syscall from {}", ctx.caller_id);

        // Requires L7 for chain reads
        if let SyscallResult::Error(e) = check_capability(ctx, CapabilityLevel::L7ChainRead) {
            return SyscallResult::Error(e);
        }

        let address_ptr = args.arg0;
        let address_len = args.arg1 as usize;
        let token_ptr = args.arg2;
        let token_len = args.arg3 as usize;
        let buf_ptr = args.arg4;
        let buf_size = args.arg5 as usize;

        if buf_ptr == 0 || buf_size == 0 {
            return SyscallResult::Error(SyscallError::InvalidArgs);
        }

        let address = if address_ptr != 0 && address_len > 0 {
            match read_string(ctx, address_ptr, address_len) {
                Ok(s) => s,
                Err(e) => return SyscallResult::Error(e),
            }
        } else {
            // Use caller's address if not specified
            ctx.caller_id.clone()
        };

        let token = if token_ptr != 0 && token_len > 0 {
            match read_string(ctx, token_ptr, token_len) {
                Ok(s) if !s.is_empty() => Some(s),
                _ => None,
            }
        } else {
            None
        };

        let client = match get_blockchain_client() {
            Some(c) => c,
            None => return SyscallResult::Error(SyscallError::NotImplemented),
        };

        match client.query_balance(&address, token.as_deref()).await {
            Ok(balance) => {
                let balance_json = serde_json::json!({
                    "address": address,
                    "token": token,
                    "balance": balance,
                });

                let json_bytes = match serde_json::to_vec(&balance_json) {
                    Ok(b) => b,
                    Err(_) => return SyscallResult::Error(SyscallError::InternalError),
                };

                if json_bytes.len() > buf_size {
                    return SyscallResult::Error(SyscallError::InvalidArgs);
                }

                match write_caller_memory(ctx, buf_ptr, &json_bytes) {
                    Ok(len) => SyscallResult::Success(len as u64),
                    Err(e) => SyscallResult::Error(e),
                }
            }
            Err(e) => {
                error!("Balance query failed: {}", e);
                SyscallResult::Error(SyscallError::InternalError)
            }
        }
    }
}

// =============================================================================
// Utility Functions
// =============================================================================

/// Convert transaction hash to u64 handle
fn hash_to_handle(hash: &str) -> u64 {
    // Use first 8 bytes of hash as handle
    let bytes = hash.as_bytes();
    let mut handle = 0u64;
    for (i, byte) in bytes.iter().take(8).enumerate() {
        handle |= (*byte as u64) << (i * 8);
    }
    handle
}

/// Mock blockchain client for testing
pub struct MockBlockchainClient;

#[async_trait]
impl BlockchainClient for MockBlockchainClient {
    async fn execute_payment(
        &self,
        from: &str,
        to: &str,
        amount: &str,
        _token: Option<&str>,
    ) -> Result<String, BlockchainError> {
        // Mock implementation - just return a fake tx hash
        let hash = format!("0x{:064x}", rand::random::<u128>());
        info!(
            "Mock payment: {} -> {} amount={} tx={}",
            from, to, amount, hash
        );
        Ok(hash)
    }

    async fn bridge_tokens(
        &self,
        from_chain: u64,
        to_chain: u64,
        token: &str,
        amount: &str,
        recipient: &str,
    ) -> Result<String, BlockchainError> {
        let hash = format!("0x{:064x}", rand::random::<u128>());
        info!(
            "Mock bridge: {}->{} token={} amount={} recipient={} tx={}",
            from_chain, to_chain, token, amount, recipient, hash
        );
        Ok(hash)
    }

    async fn swap_tokens(
        &self,
        token_in: &str,
        token_out: &str,
        amount_in: &str,
        _min_amount_out: &str,
    ) -> Result<SwapResult, BlockchainError> {
        let hash = format!("0x{:064x}", rand::random::<u128>());
        info!(
            "Mock swap: {} -> {} amount={} tx={}",
            token_in, token_out, amount_in, hash
        );
        Ok(SwapResult {
            tx_hash: hash,
            amount_out: "1000000000000000000".to_string(), // 1 token
            price_impact: 0.001,
        })
    }

    async fn stake_tokens(&self, token: &str, amount: &str) -> Result<String, BlockchainError> {
        let hash = format!("0x{:064x}", rand::random::<u128>());
        info!("Mock stake: token={} amount={} tx={}", token, amount, hash);
        Ok(hash)
    }

    async fn unstake_tokens(&self, token: &str, amount: &str) -> Result<String, BlockchainError> {
        let hash = format!("0x{:064x}", rand::random::<u128>());
        info!(
            "Mock unstake: token={} amount={} tx={}",
            token, amount, hash
        );
        Ok(hash)
    }

    async fn query_balance(
        &self,
        _address: &str,
        token: Option<&str>,
    ) -> Result<String, BlockchainError> {
        let balance = if token.is_some() {
            "1000000000000000000000" // 1000 tokens
        } else {
            "5000000000000000000" // 5 ETH
        };
        Ok(balance.to_string())
    }
}
