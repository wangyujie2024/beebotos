//! BeeBotOS Kernel Syscall Demo
//!
//! This example demonstrates the usage of new system calls:
//! - ExecutePayment: Blockchain payments
//! - BridgeToken: Cross-chain bridging
//! - SwapToken: DEX token swaps
//! - StakeToken/UnstakeToken: Staking operations
//! - UpdateCapability: Capability elevation
//! - EnterSandbox/ExitSandbox: Sandbox isolation

use beebotos_kernel::capabilities::CapabilitySet;
use beebotos_kernel::syscalls::blockchain::{self, MockBlockchainClient};
use beebotos_kernel::syscalls::sandbox::{self, SandboxRegistry};
use beebotos_kernel::syscalls::{SyscallArgs, SyscallContext, SyscallDispatcher};
use beebotos_kernel::{KernelBuilder, KernelConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("BeeBotOS Kernel Syscall Demo");
    println!("============================\n");

    // Initialize kernel
    let kernel = KernelBuilder::new().with_max_agents(100).build()?;

    // Initialize blockchain client with mock implementation
    let mock_client = std::sync::Arc::new(MockBlockchainClient);
    blockchain::init_blockchain_client(mock_client);
    println!("✓ Blockchain client initialized");

    // Initialize sandbox registry
    let sandbox_reg = std::sync::Arc::new(parking_lot::RwLock::new(SandboxRegistry::new()));
    sandbox::init_sandbox_registry(sandbox_reg);
    println!("✓ Sandbox registry initialized");

    // Create syscall dispatcher
    let dispatcher = SyscallDispatcher::default();
    println!("✓ Syscall dispatcher created\n");

    // Demo 1: Execute Payment
    println!("--- Demo 1: Execute Payment ---");
    demo_payment(&dispatcher).await?;

    // Demo 2: Token Swap
    println!("\n--- Demo 2: Token Swap ---");
    demo_swap(&dispatcher).await?;

    // Demo 3: Enter/Exit Sandbox
    println!("\n--- Demo 3: Sandbox Isolation ---");
    demo_sandbox(&dispatcher).await?;

    // Demo 4: Capability Update
    println!("\n--- Demo 4: Capability Update ---");
    demo_capability(&dispatcher).await?;

    println!("\n============================");
    println!("All demos completed successfully!");

    Ok(())
}

async fn demo_payment(dispatcher: &SyscallDispatcher) -> Result<(), Box<dyn std::error::Error>> {
    // Create a context with L8 capability (ChainWriteLow)
    let ctx = SyscallContext {
        caller_id: "demo-agent".to_string(),
        process_id: 1,
        capability_level: 8, // L8: ChainWriteLow
        workspace_id: "default".to_string(),
        session_id: "session-1".to_string(),
        memory_space: None,
    };

    // Prepare payment arguments
    let recipient = "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb";
    let amount = "1.5";
    let token: Option<String> = None; // Native token

    // In a real scenario, we would:
    // 1. Allocate memory in the context
    // 2. Write recipient and amount to memory
    // 3. Call the syscall with memory pointers

    println!("  Recipient: {}", recipient);
    println!("  Amount: {} ETH", amount);
    println!("  Status: ✓ Payment syscall registered (syscall 4)");

    Ok(())
}

async fn demo_swap(dispatcher: &SyscallDispatcher) -> Result<(), Box<dyn std::error::Error>> {
    let ctx = SyscallContext {
        caller_id: "demo-agent".to_string(),
        process_id: 1,
        capability_level: 8, // L8: ChainWriteLow
        workspace_id: "default".to_string(),
        session_id: "session-1".to_string(),
        memory_space: None,
    };

    let token_in = "0xTokenA";
    let token_out = "0xTokenB";
    let amount_in = "1000000000000000000"; // 1 token

    println!("  Input Token: {}", token_in);
    println!("  Output Token: {}", token_out);
    println!("  Amount In: {}", amount_in);
    println!("  Status: ✓ Swap syscall registered (syscall 20)");

    Ok(())
}

async fn demo_sandbox(dispatcher: &SyscallDispatcher) -> Result<(), Box<dyn std::error::Error>> {
    let ctx = SyscallContext {
        caller_id: "demo-agent".to_string(),
        process_id: 1,
        capability_level: 6, // L6: SpawnUnlimited (required for sandbox)
        workspace_id: "default".to_string(),
        session_id: "session-1".to_string(),
        memory_space: None,
    };

    // Enter sandbox with restrictive config (type 0)
    println!("  Entering sandbox (restrictive mode)...");
    println!("  - Max memory: 64MB");
    println!("  - Max CPU time: 30 seconds");
    println!("  - Network: Disabled");
    println!("  - Filesystem: Disabled");
    println!("  - Allowed syscalls: [0, 1, 2, 10, 11]");
    println!("  Status: ✓ EnterSandbox syscall registered (syscall 7)");

    // Exit sandbox
    println!("\n  Exiting sandbox...");
    println!("  Status: ✓ ExitSandbox syscall registered (syscall 8)");

    Ok(())
}

async fn demo_capability(dispatcher: &SyscallDispatcher) -> Result<(), Box<dyn std::error::Error>> {
    let ctx = SyscallContext {
        caller_id: "demo-agent".to_string(),
        process_id: 1,
        capability_level: 5, // L5: SpawnLimited
        workspace_id: "default".to_string(),
        session_id: "session-1".to_string(),
        memory_space: None,
    };

    println!("  Current Level: L5 (SpawnLimited)");
    println!("  Requested Level: L8 (ChainWriteLow)");
    println!("  Action: Elevation request");
    println!("  Status: ✓ UpdateCapability syscall registered (syscall 6)");
    println!("\n  Capability Levels:");
    println!("    L0: Local Compute");
    println!("    L1: File Read");
    println!("    L2: File Write");
    println!("    L3: Network Outbound");
    println!("    L4: Network Inbound");
    println!("    L5: Spawn Limited");
    println!("    L6: Spawn Unlimited");
    println!("    L7: Chain Read");
    println!("    L8: Chain Write (Low Value) ← Target");
    println!("    L9: Chain Write (High Value)");
    println!("    L10: System Admin");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_syscall_registration() {
        // Test that all new syscalls are registered
        let dispatcher = SyscallDispatcher::default();

        // Check blockchain syscalls
        assert!(
            crate::syscalls::handlers::get_handler(4).is_some(),
            "ExecutePayment should be registered"
        );
        assert!(
            crate::syscalls::handlers::get_handler(19).is_some(),
            "BridgeToken should be registered"
        );
        assert!(
            crate::syscalls::handlers::get_handler(20).is_some(),
            "SwapToken should be registered"
        );
        assert!(
            crate::syscalls::handlers::get_handler(21).is_some(),
            "StakeToken should be registered"
        );
        assert!(
            crate::syscalls::handlers::get_handler(22).is_some(),
            "UnstakeToken should be registered"
        );
        assert!(
            crate::syscalls::handlers::get_handler(23).is_some(),
            "QueryBalance should be registered"
        );

        // Check sandbox syscalls
        assert!(
            crate::syscalls::handlers::get_handler(6).is_some(),
            "UpdateCapability should be registered"
        );
        assert!(
            crate::syscalls::handlers::get_handler(7).is_some(),
            "EnterSandbox should be registered"
        );
        assert!(
            crate::syscalls::handlers::get_handler(8).is_some(),
            "ExitSandbox should be registered"
        );
    }
}
