//! Test Utilities
//!
//! Provides mock implementations and test helpers for the chain crate.

pub mod mock_provider;

pub use mock_provider::{MockProvider, MockProviderBuilder};

/// Test constants for consistent testing
pub mod constants {
    use std::str::FromStr;

    use alloy_primitives::{Address, U256};

    /// Test addresses
    pub const TEST_ADDRESS_1: &str = "0x1234567890123456789012345678901234567890";
    pub const TEST_ADDRESS_2: &str = "0x0987654321098765432109876543210987654321";
    pub const TEST_DAO_ADDRESS: &str = "0xabcdefabcdefabcdefabcdefabcdefabcdefabcd";
    pub const TEST_TOKEN_ADDRESS: &str = "0xfedcbafedcbafedcbafedcbafedcbafedcbafed";

    /// Test chain ID
    pub const TEST_CHAIN_ID: u64 = 1337;

    /// Test gas price (20 gwei)
    pub const TEST_GAS_PRICE: u128 = 20_000_000_000;

    /// Get test address 1
    pub fn address_1() -> Address {
        Address::from_str(TEST_ADDRESS_1).unwrap()
    }

    /// Get test address 2
    pub fn address_2() -> Address {
        Address::from_str(TEST_ADDRESS_2).unwrap()
    }

    /// Get test DAO address
    pub fn dao_address() -> Address {
        Address::from_str(TEST_DAO_ADDRESS).unwrap()
    }

    /// Get test token address
    pub fn token_address() -> Address {
        Address::from_str(TEST_TOKEN_ADDRESS).unwrap()
    }

    /// Test token amount (1000 tokens with 18 decimals)
    pub fn test_token_amount() -> U256 {
        U256::from(1000) * U256::from(10).pow(U256::from(18))
    }
}

/// Test assertions helpers
#[macro_export]
macro_rules! assert_result_ok {
    ($result:expr) => {
        assert!($result.is_ok(), "Expected Ok, got Err: {:?}", $result.err());
    };
}

#[macro_export]
macro_rules! assert_result_err {
    ($result:expr) => {
        assert!($result.is_err(), "Expected Err, got Ok: {:?}", $result.ok());
    };
}

#[cfg(test)]
mod tests {
    use alloy_primitives::U256;

    use super::*;

    #[test]
    fn test_constants() {
        let addr1 = constants::address_1();
        assert_eq!(
            addr1.to_string().to_lowercase(),
            constants::TEST_ADDRESS_1.to_lowercase()
        );

        let amount = constants::test_token_amount();
        assert_eq!(
            amount,
            U256::from(1000) * U256::from(10).pow(U256::from(18))
        );
    }
}
