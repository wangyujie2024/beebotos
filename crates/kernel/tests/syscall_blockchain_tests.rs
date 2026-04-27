//! Blockchain Syscall Tests

use std::sync::Arc;

use beebotos_kernel::syscalls::blockchain::{
    init_blockchain_client, BlockchainClient, MockBlockchainClient, SwapResult,
};
use beebotos_kernel::syscalls::handlers::get_handler;
use beebotos_kernel::syscalls::sandbox::{init_sandbox_registry, SandboxRegistry};
use parking_lot::RwLock;

#[test]
fn test_syscall_registration() {
    // Verify all blockchain syscalls are registered
    assert!(
        get_handler(4).is_some(),
        "ExecutePayment (syscall 4) should be registered"
    );
    assert!(
        get_handler(19).is_some(),
        "BridgeToken (syscall 19) should be registered"
    );
    assert!(
        get_handler(20).is_some(),
        "SwapToken (syscall 20) should be registered"
    );
    assert!(
        get_handler(21).is_some(),
        "StakeToken (syscall 21) should be registered"
    );
    assert!(
        get_handler(22).is_some(),
        "UnstakeToken (syscall 22) should be registered"
    );
    assert!(
        get_handler(23).is_some(),
        "QueryBalance (syscall 23) should be registered"
    );
}

#[test]
fn test_sandbox_syscall_registration() {
    // Verify sandbox syscalls are registered
    assert!(
        get_handler(6).is_some(),
        "UpdateCapability (syscall 6) should be registered"
    );
    assert!(
        get_handler(7).is_some(),
        "EnterSandbox (syscall 7) should be registered"
    );
    assert!(
        get_handler(8).is_some(),
        "ExitSandbox (syscall 8) should be registered"
    );
    assert!(
        get_handler(34).is_some(),
        "QuerySandboxStatus (syscall 34) should be registered"
    );
}

#[tokio::test]
async fn test_mock_blockchain_client_payment() {
    let client = MockBlockchainClient;

    let result = client
        .execute_payment("0xSender", "0xRecipient", "1.5", None)
        .await;

    assert!(result.is_ok());
    let tx_hash = result.unwrap();
    assert!(tx_hash.starts_with("0x"));
    assert_eq!(tx_hash.len(), 66); // 0x + 64 hex chars
}

#[tokio::test]
async fn test_mock_blockchain_client_swap() {
    let client = MockBlockchainClient;

    let result = client
        .swap_tokens(
            "0xTokenA",
            "0xTokenB",
            "1000000000000000000",
            "900000000000000000",
        )
        .await;

    assert!(result.is_ok());
    let swap_result = result.unwrap();
    assert!(swap_result.tx_hash.starts_with("0x"));
    assert!(!swap_result.amount_out.is_empty());
}

#[tokio::test]
async fn test_mock_blockchain_client_stake() {
    let client = MockBlockchainClient;

    let result = client.stake_tokens("0xStakeToken", "1000").await;

    assert!(result.is_ok());
    let tx_hash = result.unwrap();
    assert!(tx_hash.starts_with("0x"));
}

#[tokio::test]
async fn test_mock_blockchain_client_unstake() {
    let client = MockBlockchainClient;

    let result = client.unstake_tokens("0xStakeToken", "500").await;

    assert!(result.is_ok());
    let tx_hash = result.unwrap();
    assert!(tx_hash.starts_with("0x"));
}

#[tokio::test]
async fn test_mock_blockchain_client_query_balance() {
    let client = MockBlockchainClient;

    // Test native token balance
    let result = client.query_balance("0xAddress", None).await;
    assert!(result.is_ok());
    let balance = result.unwrap();
    assert!(!balance.is_empty());

    // Test ERC-20 balance
    let result = client.query_balance("0xAddress", Some("0xToken")).await;
    assert!(result.is_ok());
    let balance = result.unwrap();
    assert!(!balance.is_empty());
}

#[tokio::test]
async fn test_mock_blockchain_client_bridge() {
    let client = MockBlockchainClient;

    let result = client
        .bridge_tokens(
            1,     // Ethereum
            10143, // Monad
            "0xToken",
            "1000",
            "0xRecipient",
        )
        .await;

    assert!(result.is_ok());
    let tx_hash = result.unwrap();
    assert!(tx_hash.starts_with("0x"));
}

#[test]
fn test_sandbox_registry() {
    let mut registry = SandboxRegistry::new();

    // Test entering sandbox
    let config = beebotos_kernel::security::sandbox::SandboxConfig::restrictive("test-agent");
    assert!(registry.enter("agent1".to_string(), config, 5).is_ok());

    // Test duplicate entry
    let config2 = beebotos_kernel::security::sandbox::SandboxConfig::standard("test-agent");
    assert!(registry.enter("agent1".to_string(), config2, 5).is_err());

    // Test sandbox status check
    assert!(registry.is_sandboxed("agent1"));
    assert!(!registry.is_sandboxed("agent2"));

    // Test syscall filtering
    assert!(registry.is_syscall_allowed("agent1", 0));
    assert!(registry.is_syscall_allowed("agent1", 1));
    assert!(!registry.is_syscall_allowed("agent1", 100));

    // Non-sandboxed agents can use any syscall
    assert!(registry.is_syscall_allowed("agent2", 100));

    // Test exiting sandbox
    let original_level = registry.exit("agent1").unwrap();
    assert_eq!(original_level, 5);

    // Test exit when not in sandbox
    assert!(registry.exit("agent1").is_err());

    // Test history
    let history = registry.get_history();
    assert_eq!(history.len(), 2); // Enter + Exit
}

#[test]
fn test_init_functions() {
    // Test blockchain client initialization
    let client = Arc::new(MockBlockchainClient);
    init_blockchain_client(client);

    // Test sandbox registry initialization
    let registry = Arc::new(RwLock::new(SandboxRegistry::new()));
    init_sandbox_registry(registry);
}
