//! Chain Module Integration Tests
//!
//! Integration tests for the chain module using mock providers.

use alloy_primitives::{Address, U256};
use beebotos_chain::compat::B256;
use beebotos_chain::config::ChainConfig;
use beebotos_chain::wallet::{HDWallet, Wallet};
use validator::Validate;

/// Test chain configuration creation
#[test]
fn test_chain_config_creation() {
    let config = ChainConfig::new("https://rpc.example.com", 1337).unwrap();

    assert_eq!(config.rpc_url, "https://rpc.example.com");
    assert_eq!(config.chain_id, 1337);
    assert_eq!(config.confirmation_blocks, 1);
    assert_eq!(config.gas_limit, 30_000_000);
}

/// Test chain config validation
#[test]
fn test_chain_config_validation() {
    // Valid config
    let valid = ChainConfig::new("https://rpc.example.com", 1337);
    assert!(valid.is_ok());

    // Invalid - empty RPC URL
    let invalid_rpc = ChainConfig::new("", 1337);
    assert!(invalid_rpc.is_err());

    // Invalid - chain_id = 0
    let config = ChainConfig {
        rpc_url: "https://rpc.example.com".to_string(),
        chain_id: 0, // Invalid
        confirmation_blocks: 1,
        gas_limit: 30_000_000,
        dao_address: None,
        treasury_address: None,
        token_address: None,
        identity_registry_address: None,
        multicall_address: None,
    };
    assert!(config.validate().is_err());
}

/// Test chain config builder pattern
#[test]
fn test_chain_config_builder() {
    let config = ChainConfig::new("https://rpc.example.com", 1337)
        .unwrap()
        .with_dao_address("0x1234567890123456789012345678901234567890")
        .with_token_address("0xabcdefabcdefabcdefabcdefabcdefabcdefabcd");

    assert_eq!(
        config.dao_address,
        Some("0x1234567890123456789012345678901234567890".to_string())
    );
    assert_eq!(
        config.token_address,
        Some("0xabcdefabcdefabcdefabcdefabcdefabcdefabcd".to_string())
    );
}

/// Test address parsing from config
#[test]
fn test_config_address_parsing() {
    let config = ChainConfig::new("https://rpc.example.com", 1337)
        .unwrap()
        .with_dao_address("0x1234567890123456789012345678901234567890");

    let dao_addr = config.get_dao_address();
    assert!(dao_addr.is_ok());

    // Missing address should error
    let treasury_addr = config.get_treasury_address();
    assert!(treasury_addr.is_err());
}

/// Test wallet creation
#[test]
fn test_wallet_creation() {
    // Random wallet
    let wallet = Wallet::random();
    let address = wallet.address();
    assert!(!address.is_zero());
}

/// Test HD wallet from mnemonic
#[test]
fn test_hd_wallet() {
    let test_mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon \
                         abandon abandon about";
    let hd_wallet = HDWallet::from_mnemonic(test_mnemonic).unwrap();

    // Derive first account
    let account = hd_wallet.derive_account(0, Some("Test Account".to_string()));
    assert!(account.is_ok());

    let account = account.unwrap();
    assert!(!account.address.is_zero());
    assert_eq!(account.index, 0);
    assert_eq!(account.name, Some("Test Account".to_string()));
}

/// Test mnemonic generation
#[test]
fn test_mnemonic_generation() {
    let mnemonic12 = HDWallet::generate_mnemonic(12).unwrap();
    let words12: Vec<&str> = mnemonic12.split_whitespace().collect();
    assert_eq!(words12.len(), 12);

    let mnemonic24 = HDWallet::generate_mnemonic(24).unwrap();
    let words24: Vec<&str> = mnemonic24.split_whitespace().collect();
    assert_eq!(words24.len(), 24);
}

/// Test HD wallet derivation paths
#[test]
fn test_derivation_paths() {
    let test_mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon \
                         abandon abandon about";
    let wallet = HDWallet::from_mnemonic(test_mnemonic).unwrap();

    // Derive multiple accounts
    let account0 = wallet.derive_account(0, None).unwrap();
    let account1 = wallet.derive_account(1, None).unwrap();

    // Different indices should produce different addresses
    assert_ne!(account0.address, account1.address);

    // Check derivation paths
    assert!(account0.derivation_path.contains("m/44'/60'/0'/0/0"));
    assert!(account1.derivation_path.contains("m/44'/60'/0'/0/1"));
}

/// Test wallet signing (would need async runtime)
///
/// Note: Actual signing tests require an async runtime and are
/// included in the unit tests within the wallet module.
#[test]
fn test_wallet_signing_sync() {
    // Just verify the wallet structure
    let wallet = Wallet::random();
    assert!(!wallet.address().is_zero());
}

/// Test transaction hash types
#[test]
fn test_transaction_hash() {
    let hash = B256::ZERO;
    assert_eq!(hash.as_slice(), &[0u8; 32]);

    let hash = B256::from([1u8; 32]);
    assert_eq!(hash.as_slice(), &[1u8; 32]);
}

/// Test address comparison
#[test]
fn test_address_comparison() {
    let addr1 = Address::from([1u8; 20]);
    let addr2 = Address::from([1u8; 20]);
    let addr3 = Address::from([2u8; 20]);

    assert_eq!(addr1, addr2);
    assert_ne!(addr1, addr3);
}

/// Test U256 arithmetic
#[test]
fn test_u256_arithmetic() {
    let a = U256::from(100);
    let b = U256::from(50);

    assert_eq!(a + b, U256::from(150));
    assert_eq!(a - b, U256::from(50));
    assert_eq!(a * b, U256::from(5000));
    assert_eq!(a / b, U256::from(2));
}

/// Test chain config environment variable loading
///
/// Note: This test would need environment variables set to run properly
#[test]
#[ignore = "Requires environment variables"]
fn test_chain_config_from_env() {
    // std::env::set_var("CHAIN_RPC_URL", "https://rpc.testnet.monad.xyz");
    // std::env::set_var("CHAIN_ID", "10143");

    let _config = ChainConfig::from_env();
    // Assert based on expected values
}

/// Test config validation with all addresses
#[test]
fn test_config_with_all_addresses() {
    let config = ChainConfig::new("https://rpc.example.com", 1337)
        .unwrap()
        .with_dao_address("0x1234567890123456789012345678901234567890")
        .with_treasury_address("0x0987654321098765432109876543210987654321")
        .with_token_address("0xabcdefabcdefabcdefabcdefabcdefabcdefabcd")
        .with_identity_registry_address("0xfedcbafedcbafedcbafedcbafedcbafedcbafedc")
        .with_multicall_address("0x1111111111111111111111111111111111111111");

    // All addresses should parse correctly
    assert!(config.get_dao_address().is_ok());
    assert!(config.get_treasury_address().is_ok());
    assert!(config.get_token_address().is_ok());
    assert!(config.get_identity_registry_address().is_ok());
    assert!(config.get_multicall_address().is_ok());

    // Validation should pass
    assert!(config.validate_contract_addresses().is_ok());
}

/// Test invalid address handling
#[test]
fn test_invalid_address_handling() {
    let config = ChainConfig::new("https://rpc.example.com", 1337)
        .unwrap()
        .with_dao_address("not_a_valid_address");

    // Address parsing should fail
    assert!(config.get_dao_address().is_err());
}

/// Test Monad testnet config
#[test]
fn test_monad_testnet_config() {
    // This might fail if environment variables are not set correctly
    // but we can at least verify the method exists
    let _config = ChainConfig::monad_testnet();
    // Don't assert on result as it depends on env vars
}

/// Test local devnet config
#[test]
fn test_local_config() {
    let config = ChainConfig::local();

    assert_eq!(config.rpc_url, "http://localhost:8545");
    assert_eq!(config.chain_id, 1337);
    assert_eq!(config.confirmation_blocks, 0);
}

/// Test gas limit validation
#[test]
fn test_gas_limit_validation() {
    // Gas limit too low
    let config = ChainConfig {
        rpc_url: "https://rpc.example.com".to_string(),
        chain_id: 1337,
        confirmation_blocks: 1,
        gas_limit: 1000, // Too low
        dao_address: None,
        treasury_address: None,
        token_address: None,
        identity_registry_address: None,
        multicall_address: None,
    };
    assert!(config.validate().is_err());

    // Gas limit too high
    let config = ChainConfig {
        gas_limit: 100_000_000, // Too high
        ..config
    };
    assert!(config.validate().is_err());
}
