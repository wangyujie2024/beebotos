//! WASM Host Functions
//!
//! Provides host functions that WASM modules can call.
//! Updated for wasmtime 34.0 API

use tracing::debug;
use wasmtime::{Caller, Linker};

use crate::error::Result;

/// Host function context passed to WASM instances
#[derive(Debug, Clone)]
pub struct HostContext {
    /// Agent ID for this instance
    pub agent_id: String,
    /// Memory limit
    pub memory_limit: usize,
    /// Fuel consumed
    pub fuel_consumed: u64,
}

impl HostContext {
    /// Create new host context
    pub fn new(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            memory_limit: 128 * 1024 * 1024,
            fuel_consumed: 0,
        }
    }
}

/// Collection of host functions
///
/// wasmtime 34.0: Host functions are added to Linker with store context
pub struct HostFunctions;

impl HostFunctions {
    /// Add all host functions to a linker
    ///
    /// wasmtime 34.0: Linker::func_wrap creates functions with closure
    /// The closure receives Caller<T> as first parameter
    pub fn add_to_linker(linker: &mut Linker<HostContext>) -> Result<()> {
        // Log function: log(ptr: i32, len: i32)
        linker
            .func_wrap(
                "beebotos",
                "log",
                |mut caller: Caller<'_, HostContext>, ptr: i32, len: i32| {
                    // Read message from memory
                    let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory())
                    else {
                        tracing::warn!("WASM module does not export memory");
                        return;
                    };

                    let mut buffer = vec![0u8; len as usize];
                    if memory.read(&caller, ptr as usize, &mut buffer).is_ok() {
                        let msg = String::from_utf8_lossy(&buffer);
                        tracing::info!("[WASM] {}", msg);
                    }
                },
            )
            .map_err(|e| {
                crate::error::KernelError::internal(format!("Failed to define log: {}", e))
            })?;

        // Print function: print(ptr: i32, len: i32)
        linker
            .func_wrap(
                "beebotos",
                "print",
                |mut caller: Caller<'_, HostContext>, ptr: i32, len: i32| {
                    let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory())
                    else {
                        tracing::warn!("WASM module does not export memory");
                        return;
                    };

                    let mut buffer = vec![0u8; len as usize];
                    if memory.read(&caller, ptr as usize, &mut buffer).is_ok() {
                        let msg = String::from_utf8_lossy(&buffer);
                        print!("{}", msg);
                    }
                },
            )
            .map_err(|e| {
                crate::error::KernelError::internal(format!("Failed to define print: {}", e))
            })?;

        // Time function: time() -> i64 (returns timestamp in milliseconds)
        linker
            .func_wrap(
                "beebotos",
                "time",
                |_caller: Caller<'_, HostContext>| -> i64 {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as i64
                },
            )
            .map_err(|e| {
                crate::error::KernelError::internal(format!("Failed to define time: {}", e))
            })?;

        // Random function: random() -> i64
        linker
            .func_wrap(
                "beebotos",
                "random",
                |_caller: Caller<'_, HostContext>| -> i64 {
                    use std::collections::hash_map::RandomState;
                    use std::hash::{BuildHasher, Hasher};

                    RandomState::new().build_hasher().finish() as i64
                },
            )
            .map_err(|e| {
                crate::error::KernelError::internal(format!("Failed to define random: {}", e))
            })?;

        // Agent ID function: agent_id(ptr: i32, max_len: i32) -> i32
        linker
            .func_wrap(
                "beebotos",
                "agent_id",
                |mut caller: Caller<'_, HostContext>, ptr: i32, max_len: i32| -> i32 {
                    let agent_id = caller.data().agent_id.clone();
                    let bytes = agent_id.as_bytes();
                    let len = bytes.len().min(max_len as usize);

                    if let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory())
                    {
                        let _ = memory.write(&mut caller, ptr as usize, &bytes[..len]);
                    }
                    len as i32
                },
            )
            .map_err(|e| {
                crate::error::KernelError::internal(format!("Failed to define agent_id: {}", e))
            })?;

        // Memory remaining function: memory_remaining() -> i64
        linker
            .func_wrap(
                "beebotos",
                "memory_remaining",
                |caller: Caller<'_, HostContext>| -> i64 {
                    // Return remaining memory in bytes
                    caller.data().memory_limit as i64
                },
            )
            .map_err(|e| {
                crate::error::KernelError::internal(format!(
                    "Failed to define memory_remaining: {}",
                    e
                ))
            })?;

        debug!("Host functions registered in linker");
        Ok(())
    }
}

// Re-export WASI context creation functions and types
pub use crate::wasm::wasi_ctx::{
    create_restricted_wasi_context, create_wasi_context, create_wasi_context_with_caps,
    FilesystemAccess, StdioConfig, WasiCapabilities, WasiHostContext,
};
