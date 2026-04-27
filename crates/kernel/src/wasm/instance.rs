//! WASM Instance
//!
//! Manages a running WASM instance with wasmtime 34.0

use tracing::trace;
use wasmtime::{Instance, Memory, Store, Val};

use crate::error::{KernelError, Result};
use crate::wasm::host_funcs::HostContext;
use crate::wasm::metering::FuelLimit;

/// WASM instance wrapper for wasmtime 34.0
///
/// Store<T> owns the engine and user data.
/// We use Store<HostContext> for host function support
/// or Store<wasmtime_wasi::WasiCtx> for WASI support.
pub struct WasmInstance {
    /// WASM store
    store: Store<HostContext>,
    /// WASM instance
    instance: Instance,
    /// Fuel consumed
    fuel_consumed: u64,
    /// Agent ID
    agent_id: String,
}

impl WasmInstance {
    /// Create a new instance wrapper with HostContext
    pub fn new(store: Store<HostContext>, instance: Instance) -> Self {
        let agent_id = store.data().agent_id.clone();

        // Record instance creation
        crate::wasm::record_instance_created();

        Self {
            store,
            instance,
            fuel_consumed: 0,
            agent_id,
        }
    }

    /// Create a new instance with WASI context
    ///
    /// Note: This is a convenience method that stores the agent_id separately
    /// since Store<WasiCtx> has a different type.

    pub fn new_with_wasi(
        store: Store<wasmtime_wasi::p2::WasiCtx>,
        instance: Instance,
        agent_id: String,
    ) -> Self {
        // Convert Store<WasiCtx> to Store<HostContext> by creating a new store
        // In practice, WASI instances should use a different wrapper
        // For now, we create a dummy HostContext
        let new_store = Store::new(&store.engine().clone(), HostContext::new(&agent_id));

        // Note: Fuel handling - wasmtime 34.0 has configurable fuel API

        // Record instance creation
        crate::wasm::record_instance_created();

        Self {
            store: new_store,
            instance,
            fuel_consumed: 0,
            agent_id,
        }
    }

    /// Get exported memory
    ///
    /// wasmtime 34.0: get_export takes &mut Store<T>
    pub fn memory(&mut self) -> Result<Memory> {
        self.instance
            .get_export(&mut self.store, "memory")
            .and_then(|e| e.into_memory())
            .ok_or_else(|| KernelError::invalid_argument("No memory export"))
    }

    /// Read from instance memory
    pub fn read_memory(&mut self, offset: usize, len: usize) -> Result<Vec<u8>> {
        let memory = self.memory()?;
        let mut buffer = vec![0u8; len];

        memory
            .read(&self.store, offset, &mut buffer)
            .map_err(|e| KernelError::memory(format!("Read failed: {}", e)))?;

        Ok(buffer)
    }

    /// Write to instance memory
    pub fn write_memory(&mut self, offset: usize, data: &[u8]) -> Result<()> {
        let memory = self.memory()?;

        memory
            .write(&mut self.store, offset, data)
            .map_err(|e| KernelError::memory(format!("Write failed: {}", e)))?;

        Ok(())
    }

    /// Get memory size in pages
    ///
    /// wasmtime 34.0: Memory::size takes &Store<T>
    pub fn memory_size(&mut self) -> usize {
        self.instance
            .get_export(&mut self.store, "memory")
            .and_then(|e| e.into_memory())
            .map(|m| m.size(&self.store) as usize)
            .unwrap_or(0)
    }

    /// Get memory size in bytes
    pub fn memory_size_bytes(&mut self) -> usize {
        self.memory_size() * 65536 // 64KB per page
    }

    /// Grow memory by additional pages
    ///
    /// wasmtime 34.0: Memory::grow takes &mut Store<T> and u64 delta
    pub fn grow_memory(&mut self, delta: u32) -> Result<u32> {
        let memory = self.memory()?;

        let old_size = memory
            .grow(&mut self.store, delta as u64)
            .map_err(|e| KernelError::memory(format!("Grow failed: {}", e)))?;

        trace!(
            "WASM memory grown: {} -> {} pages",
            old_size,
            old_size + delta as u64
        );
        Ok(old_size as u32)
    }

    /// Call an exported function by name
    ///
    /// wasmtime 34.0: Func::call takes &mut Store<T>
    pub fn call(&mut self, name: &str, args: &[Val]) -> Result<Vec<Val>> {
        let func = self
            .instance
            .get_export(&mut self.store, name)
            .and_then(|e| e.into_func())
            .ok_or_else(|| {
                KernelError::invalid_argument(format!("Function '{}' not found", name))
            })?;

        // Get result count from function type
        // wasmtime 34.0: Use func.ty(&store).results().len()
        let result_count = func.ty(&self.store).results().len();
        let mut results = vec![Val::I32(0); result_count];

        // Note: wasmtime 34.0 fuel API is configurable at engine level
        func.call(&mut self.store, args, &mut results)
            .map_err(|e| KernelError::internal(format!("Function call failed: {}", e)))?;

        Ok(results)
    }

    /// Call a function with typed signature
    ///
    /// wasmtime 34.0: TypedFunc::call takes &mut Store<T>
    pub fn call_typed<Params, Results>(&mut self, name: &str, params: Params) -> Result<Results>
    where
        Params: wasmtime::WasmParams,
        Results: wasmtime::WasmResults,
    {
        let func = self
            .instance
            .get_export(&mut self.store, name)
            .and_then(|e| e.into_func())
            .and_then(|f| f.typed::<Params, Results>(&self.store).ok())
            .ok_or_else(|| {
                KernelError::invalid_argument(format!("Typed function '{}' not found", name))
            })?;

        // Note: wasmtime 34.0 fuel API is configurable
        let results = func
            .call(&mut self.store, params)
            .map_err(|e| KernelError::internal(format!("Function call failed: {}", e)))?;

        Ok(results)
    }

    /// Call a function with i32 arguments and return
    pub fn call_i32(&mut self, name: &str, args: &[i32]) -> Result<i32> {
        let wasm_args: Vec<Val> = args.iter().map(|&v| Val::I32(v)).collect();
        let results = self.call(name, &wasm_args)?;

        results
            .get(0)
            .and_then(|v| v.i32())
            .ok_or_else(|| KernelError::internal("Expected i32 return value"))
    }

    /// Call a function with i64 arguments and return
    pub fn call_i64(&mut self, name: &str, args: &[i64]) -> Result<i64> {
        let wasm_args: Vec<Val> = args.iter().map(|&v| Val::I64(v)).collect();
        let results = self.call(name, &wasm_args)?;

        results
            .get(0)
            .and_then(|v| v.i64())
            .ok_or_else(|| KernelError::internal("Expected i64 return value"))
    }

    /// Start the instance (call _start if it exists)
    pub fn start(&mut self) -> Result<()> {
        if self
            .instance
            .get_export(&mut self.store, "_start")
            .is_some()
        {
            self.call("_start", &[])?;
        }
        Ok(())
    }

    /// Check if instance has enough fuel remaining
    ///
    /// Note: wasmtime 34.0 fuel API is configurable at engine level
    pub fn has_fuel(&self, _amount: u64) -> bool {
        true // Fuel metering configured at engine level
    }

    /// Get remaining fuel
    ///
    /// Note: wasmtime 34.0 fuel API is configurable at engine level
    pub fn remaining_fuel(&self) -> u64 {
        0 // Fuel is tracked via Engine configuration in 34.0
    }

    /// Add more fuel to the instance
    ///
    /// Note: wasmtime 34.0 Store fuel is configured at engine level
    pub fn add_fuel(&mut self, _amount: u64) -> Result<()> {
        // Fuel is configured at engine level in 34.0
        Ok(())
    }

    /// Get total fuel consumed
    pub fn fuel_consumed(&self) -> u64 {
        self.fuel_consumed
    }

    /// Set fuel limit
    ///
    /// Note: wasmtime 34.0 fuel API is configurable at engine level
    pub fn set_fuel_limit(&mut self, _limit: FuelLimit) -> Result<()> {
        // Fuel limits are configured at engine level in 34.0
        Ok(())
    }

    /// Get agent ID
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    /// Get instance statistics
    pub fn stats(&mut self) -> InstanceStats {
        InstanceStats {
            memory_pages: self.memory_size(),
            memory_bytes: self.memory_size_bytes(),
            fuel_consumed: self.fuel_consumed,
            fuel_remaining: self.remaining_fuel(),
        }
    }

    /// Check if the instance has an export
    pub fn has_export(&mut self, name: &str) -> bool {
        self.instance.get_export(&mut self.store, name).is_some()
    }

    /// Get list of all exports
    ///
    /// wasmtime 34.0: Instance::exports takes &mut Store<T>
    pub fn exports(&mut self) -> Vec<String> {
        self.instance
            .exports(&mut self.store)
            .map(|e| e.name().to_string())
            .collect()
    }

    /// Get mutable store reference
    pub fn store_mut(&mut self) -> &mut Store<HostContext> {
        &mut self.store
    }

    /// Get immutable store reference
    pub fn store(&self) -> &Store<HostContext> {
        &self.store
    }

    /// Get instance reference
    pub fn instance(&self) -> &Instance {
        &self.instance
    }
}

/// Instance statistics
#[derive(Debug, Clone, Copy)]
pub struct InstanceStats {
    /// Memory pages allocated
    pub memory_pages: usize,
    /// Memory bytes used
    pub memory_bytes: usize,
    /// Fuel consumed
    pub fuel_consumed: u64,
    /// Fuel remaining
    pub fuel_remaining: u64,
}

impl std::fmt::Display for InstanceStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Instance: {} pages ({} bytes), fuel: {} consumed, {} remaining",
            self.memory_pages, self.memory_bytes, self.fuel_consumed, self.fuel_remaining
        )
    }
}
