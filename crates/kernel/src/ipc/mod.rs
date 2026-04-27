//! Inter-Process Communication
//!
//! IPC mechanisms for process communication.

pub mod channel;
pub mod message;
pub mod pipe;
pub mod router;
pub mod shared_memory;

use std::sync::Arc;

pub use channel::IpcChannel;
pub use message::MessageQueue;
use parking_lot::RwLock;
pub use shared_memory::{SharedMemory, SharedMemoryManager, SharedMemoryStats};

use crate::error::Result;

/// Global shared memory manager
static SHARED_MEMORY_MANAGER: std::sync::OnceLock<Arc<RwLock<SharedMemoryManager>>> =
    std::sync::OnceLock::new();

/// Initialize IPC subsystem
pub fn init() -> Result<()> {
    tracing::info!("Initializing IPC");

    // Initialize shared memory manager
    SHARED_MEMORY_MANAGER.get_or_init(|| Arc::new(RwLock::new(SharedMemoryManager::new())));

    Ok(())
}

/// Get global shared memory manager
pub fn shared_memory_manager() -> Arc<RwLock<SharedMemoryManager>> {
    SHARED_MEMORY_MANAGER
        .get()
        .expect("IPC not initialized")
        .clone()
}
