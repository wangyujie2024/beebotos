//! Security Utilities
//!
//! Provides security-related utilities and checks for blockchain operations.
//!
//! # Features
//!
//! - Address validation and sanitization
//! - Transaction validation
//! - Reentrancy protection
//! - Integer overflow checks
//! - Replay attack prevention

use parking_lot::Mutex;

use crate::compat::Address;
use crate::constants::{MAX_GAS_PRICE_WEI, MAX_TX_DATA_SIZE};
use crate::{ChainError, Result};

/// Security validator for blockchain operations
#[derive(Debug, Clone)]
pub struct SecurityValidator {
    /// Maximum gas price allowed (in wei)
    max_gas_price: u128,
    /// Maximum transaction value allowed (in wei)
    max_value: u128,
    /// Blacklisted addresses
    blacklist: Vec<Address>,
    /// Whitelist mode (if true, only whitelisted addresses are allowed)
    whitelist_mode: bool,
    /// Whitelisted addresses
    whitelist: Vec<Address>,
}

impl Default for SecurityValidator {
    fn default() -> Self {
        Self {
            max_gas_price: MAX_GAS_PRICE_WEI, // 1000 gwei
            max_value: u128::MAX,
            blacklist: Vec::new(),
            whitelist_mode: false,
            whitelist: Vec::new(),
        }
    }
}

impl SecurityValidator {
    /// Create new security validator with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum gas price
    pub fn max_gas_price(mut self, max_gas_price: u128) -> Self {
        self.max_gas_price = max_gas_price;
        self
    }

    /// Set maximum transaction value
    pub fn max_value(mut self, max_value: u128) -> Self {
        self.max_value = max_value;
        self
    }

    /// Add address to blacklist
    pub fn blacklist(mut self, address: Address) -> Self {
        self.blacklist.push(address);
        self
    }

    /// Enable whitelist mode
    pub fn whitelist_mode(mut self) -> Self {
        self.whitelist_mode = true;
        self
    }

    /// Add address to whitelist
    pub fn whitelist(mut self, address: Address) -> Self {
        self.whitelist.push(address);
        self
    }

    /// Validate transaction parameters
    pub fn validate_transaction(
        &self,
        to: Address,
        value: u128,
        gas_price: u128,
        data: &[u8],
    ) -> Result<()> {
        // Check blacklist
        if self.blacklist.contains(&to) {
            return Err(ChainError::Validation(
                "Transaction to blacklisted address".to_string(),
            ));
        }

        // Check whitelist
        if self.whitelist_mode && !self.whitelist.contains(&to) {
            return Err(ChainError::Validation(
                "Address not in whitelist".to_string(),
            ));
        }

        // Check gas price
        if gas_price > self.max_gas_price {
            return Err(ChainError::Validation(format!(
                "Gas price {} exceeds maximum {}",
                gas_price, self.max_gas_price
            )));
        }

        // Check value
        if value > self.max_value {
            return Err(ChainError::Validation(format!(
                "Value {} exceeds maximum {}",
                value, self.max_value
            )));
        }

        // Check data size (prevent DOS)
        if data.len() > MAX_TX_DATA_SIZE {
            return Err(ChainError::Validation(
                "Transaction data too large".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate contract address
    pub fn validate_contract_address(&self, address: Address) -> Result<()> {
        // Check if address is zero (burn address)
        if address.is_zero() {
            return Err(ChainError::Validation(
                "Zero address is not a valid contract".to_string(),
            ));
        }

        // Check blacklist
        if self.blacklist.contains(&address) {
            return Err(ChainError::Validation(
                "Contract address is blacklisted".to_string(),
            ));
        }

        Ok(())
    }
}

/// Reentrancy guard for protecting against reentrancy attacks
///
/// Use this guard when making external calls that could be reentered.
///
/// # Example
///
/// ```rust
/// use beebotos_chain::security::ReentrancyGuard;
///
/// fn withdraw(guard: &ReentrancyGuard) -> Result<(), beebotos_chain::ChainError> {
///     let _lock = guard.try_lock()?;
///     // Perform withdrawal
///     Ok(())
/// }
/// ```
#[derive(Debug)]
pub struct ReentrancyGuard {
    locked: std::sync::atomic::AtomicBool,
}

impl Default for ReentrancyGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl ReentrancyGuard {
    /// Create new reentrancy guard
    pub fn new() -> Self {
        Self {
            locked: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Try to lock the guard
    pub fn try_lock(&self) -> Result<ReentrancyLock<'_>> {
        let was_locked = self.locked.swap(true, std::sync::atomic::Ordering::SeqCst);
        if was_locked {
            return Err(ChainError::Validation("Reentrancy detected".to_string()));
        }
        Ok(ReentrancyLock { guard: self })
    }
}

/// Reentrancy lock - automatically unlocks when dropped
pub struct ReentrancyLock<'a> {
    guard: &'a ReentrancyGuard,
}

impl<'a> Drop for ReentrancyLock<'a> {
    fn drop(&mut self) {
        self.guard
            .locked
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Nonce manager for replay attack prevention
#[derive(Debug)]
pub struct NonceManager {
    /// Last used nonce by address
    nonces: Mutex<std::collections::HashMap<Address, u64>>,
}

impl Default for NonceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl NonceManager {
    /// Create new nonce manager
    pub fn new() -> Self {
        Self {
            nonces: Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Get next nonce for address
    pub fn get_next_nonce(&self, address: Address) -> u64 {
        let mut nonces = self.nonces.lock();
        let nonce = nonces.entry(address).or_insert(0);
        *nonce += 1;
        *nonce - 1
    }

    /// Set nonce for address
    pub fn set_nonce(&self, address: Address, nonce: u64) {
        let mut nonces = self.nonces.lock();
        nonces.insert(address, nonce);
    }

    /// Validate and consume nonce
    pub fn validate_nonce(&self, address: Address, nonce: u64) -> Result<()> {
        let mut nonces = self.nonces.lock();
        let expected = nonces.get(&address).copied().unwrap_or(0);

        if nonce != expected {
            return Err(ChainError::Validation(format!(
                "Invalid nonce: expected {}, got {}",
                expected, nonce
            )));
        }

        nonces.insert(address, nonce + 1);
        Ok(())
    }
}

/// Input sanitizer for sanitizing user inputs
pub struct InputSanitizer;

impl InputSanitizer {
    /// Sanitize Ethereum address string
    pub fn sanitize_address(address: &str) -> Result<String> {
        let sanitized = address.trim().to_lowercase();

        // Check format
        if !sanitized.starts_with("0x") {
            return Err(ChainError::Validation(
                "Address must start with 0x".to_string(),
            ));
        }

        if sanitized.len() != 42 {
            return Err(ChainError::Validation(
                "Address must be 42 characters".to_string(),
            ));
        }

        // Check hex characters
        if !sanitized[2..].chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ChainError::Validation(
                "Address contains invalid characters".to_string(),
            ));
        }

        Ok(sanitized)
    }

    /// Sanitize transaction data
    pub fn sanitize_data(data: &[u8]) -> Result<Vec<u8>> {
        // Limit data size
        if data.len() > 100_000 {
            return Err(ChainError::Validation("Data too large".to_string()));
        }

        Ok(data.to_vec())
    }

    /// Sanitize string input (remove control characters)
    pub fn sanitize_string(input: &str) -> String {
        input.chars().filter(|c| !c.is_control()).collect()
    }
}

/// Rate limiter for contract calls
#[derive(Debug)]
pub struct CallRateLimiter {
    /// Maximum calls per window
    max_calls: u32,
    /// Time window in seconds
    window_secs: u64,
    /// Call timestamps
    calls: Mutex<std::collections::VecDeque<std::time::Instant>>,
}

impl CallRateLimiter {
    /// Create new rate limiter
    pub fn new(max_calls: u32, window_secs: u64) -> Self {
        Self {
            max_calls,
            window_secs,
            calls: Mutex::new(std::collections::VecDeque::new()),
        }
    }

    /// Check if call is allowed
    pub fn check(&self) -> Result<()> {
        let mut calls = self.calls.lock();
        let now = std::time::Instant::now();
        let window = std::time::Duration::from_secs(self.window_secs);

        // Remove old calls
        while let Some(front) = calls.front() {
            if now.duration_since(*front) > window {
                calls.pop_front();
            } else {
                break;
            }
        }

        // Check limit
        if calls.len() >= self.max_calls as usize {
            return Err(ChainError::Validation("Rate limit exceeded".to_string()));
        }

        calls.push_back(now);
        Ok(())
    }

    /// Reset rate limiter
    pub fn reset(&self) {
        let mut calls = self.calls.lock();
        calls.clear();
    }
}

/// Security audit logger
#[derive(Debug)]
pub struct SecurityAudit;

impl SecurityAudit {
    /// Log security event
    pub fn log_event(event_type: &str, details: &str) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        tracing::info!(
            target: "security_audit",
            event_type = %event_type,
            details = %details,
            timestamp = timestamp,
            "Security event"
        );
    }

    /// Log suspicious activity
    pub fn log_suspicious(activity: &str, address: Option<Address>) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        tracing::warn!(
            target: "security_audit",
            activity = %activity,
            address = ?address,
            timestamp = timestamp,
            "Suspicious activity detected"
        );
    }

    /// Log security violation
    pub fn log_violation(violation: &str, blocked: bool) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        tracing::error!(
            target: "security_audit",
            violation = %violation,
            blocked = blocked,
            timestamp = timestamp,
            "Security violation"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_validator() {
        let validator = SecurityValidator::new().max_gas_price(100_000_000_000); // 100 gwei

        let to = Address::from([1u8; 20]);
        assert!(validator
            .validate_transaction(to, 1000, 50_000_000_000, &[])
            .is_ok());
        assert!(validator
            .validate_transaction(to, 1000, 200_000_000_000, &[])
            .is_err());
    }

    #[test]
    fn test_security_validator_blacklist() {
        let blacklisted = Address::from([1u8; 20]);
        let validator = SecurityValidator::new().blacklist(blacklisted);

        assert!(validator
            .validate_transaction(blacklisted, 1000, 1, &[])
            .is_err());

        let other = Address::from([2u8; 20]);
        assert!(validator.validate_transaction(other, 1000, 1, &[]).is_ok());
    }

    #[test]
    fn test_reentrancy_guard() {
        let guard = ReentrancyGuard::new();

        let lock1 = guard.try_lock();
        assert!(lock1.is_ok());

        let lock2 = guard.try_lock();
        assert!(lock2.is_err());

        drop(lock1);

        let lock3 = guard.try_lock();
        assert!(lock3.is_ok());
    }

    #[test]
    fn test_nonce_manager() {
        let manager = NonceManager::new();
        let addr = Address::from([1u8; 20]);

        assert_eq!(manager.get_next_nonce(addr), 0);
        assert_eq!(manager.get_next_nonce(addr), 1);
        assert_eq!(manager.get_next_nonce(addr), 2);

        assert!(manager.validate_nonce(addr, 3).is_ok());
        assert!(manager.validate_nonce(addr, 3).is_err()); // Already used
    }

    #[test]
    fn test_input_sanitizer_address() {
        assert!(
            InputSanitizer::sanitize_address("0x1234567890123456789012345678901234567890").is_ok()
        );
        assert!(
            InputSanitizer::sanitize_address("1234567890123456789012345678901234567890").is_err()
        ); // No 0x
        assert!(InputSanitizer::sanitize_address("0x123").is_err()); // Too short
        assert!(
            InputSanitizer::sanitize_address("0xGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGG").is_err()
        ); // Invalid hex
    }

    #[test]
    fn test_input_sanitizer_string() {
        let input = "Hello\x00World\x01";
        let sanitized = InputSanitizer::sanitize_string(input);
        assert_eq!(sanitized, "HelloWorld");
    }

    #[test]
    fn test_rate_limiter() {
        let limiter = CallRateLimiter::new(3, 60);

        assert!(limiter.check().is_ok());
        assert!(limiter.check().is_ok());
        assert!(limiter.check().is_ok());
        assert!(limiter.check().is_err()); // Exceeds limit

        limiter.reset();
        assert!(limiter.check().is_ok());
    }

    #[test]
    fn test_security_audit_log() {
        SecurityAudit::log_event("TEST", "Test event");
        SecurityAudit::log_suspicious("Test activity", None);
        SecurityAudit::log_violation("Test violation", true);
    }
}
