//! WASM Execution Engine
//!
//! WebAssembly runtime using wasmtime 34.0

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use tracing::{debug, info};
use wasmtime::{Config, Engine, Module, Store};

use crate::error::{KernelError, Result};
use crate::wasm::host_funcs::{HostContext, HostFunctions};
use crate::wasm::instance::WasmInstance;

/// WASM engine configuration
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Maximum memory per instance (bytes)
    pub max_memory_size: usize,
    /// Maximum execution fuel (fuel units)
    pub max_fuel: u64,
    /// Enable fuel metering
    pub fuel_metering: bool,
    /// Enable memory limits
    pub memory_limits: bool,
    /// Enable WASI
    pub wasi_enabled: bool,
    /// Enable debug info
    pub debug_info: bool,
    /// Enable parallel compilation
    pub parallel_compilation: bool,
    /// Enable cranelift optimizations
    pub optimize: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_memory_size: 128 * 1024 * 1024, // 128MB
            max_fuel: 10_000_000,               // 10M fuel units
            fuel_metering: true,
            memory_limits: true,
            wasi_enabled: true,
            debug_info: false,
            parallel_compilation: true,
            optimize: true,
        }
    }
}

impl EngineConfig {
    /// Production configuration
    pub fn production() -> Self {
        Self {
            debug_info: false,
            optimize: true,
            parallel_compilation: true,
            ..Default::default()
        }
    }

    /// Development configuration
    pub fn development() -> Self {
        Self {
            debug_info: true,
            optimize: false,
            ..Default::default()
        }
    }
}

/// Compiled WASM module cache entry
#[derive(Clone)]
struct CachedModule {
    module: Module,
    /// Timestamp when the module was compiled - reserved for cache eviction
    #[allow(dead_code)]
    _compiled_at: std::time::Instant,
    use_count: Arc<std::sync::atomic::AtomicUsize>,
}

/// WASM execution engine
pub struct WasmEngine {
    config: EngineConfig,
    engine: Engine,
    module_cache: Arc<RwLock<HashMap<String, CachedModule>>>,
}

impl WasmEngine {
    /// Create a new WASM engine
    pub fn new(config: EngineConfig) -> Result<Self> {
        let mut wasm_config = Config::new();

        // Configure fuel metering
        if config.fuel_metering {
            wasm_config.consume_fuel(true);
        }

        // Configure memory limits
        if config.memory_limits {
            wasm_config.memory_reservation(config.max_memory_size as u64);
            wasm_config.memory_guard_size(65536);
        }

        // Configure compilation
        wasm_config.parallel_compilation(config.parallel_compilation);
        wasm_config.cranelift_opt_level(if config.optimize {
            wasmtime::OptLevel::Speed
        } else {
            wasmtime::OptLevel::None
        });

        // Debug info
        wasm_config.debug_info(config.debug_info);
        wasm_config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Environment);

        // Enable WASI if needed
        if config.wasi_enabled {
            wasm_config.wasm_multi_memory(true);
        }

        let engine = Engine::new(&wasm_config)
            .map_err(|e| KernelError::internal(format!("Failed to create WASM engine: {}", e)))?;

        info!(
            "WASM engine initialized: max_memory={}MB, max_fuel={}",
            config.max_memory_size / (1024 * 1024),
            config.max_fuel
        );

        Ok(Self {
            config,
            engine,
            module_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Compile a WASM module
    pub fn compile(&self, wasm_bytes: &[u8]) -> Result<Module> {
        let start = std::time::Instant::now();

        let module = Module::new(&self.engine, wasm_bytes).map_err(|e| {
            KernelError::invalid_argument(format!("WASM compilation failed: {}", e))
        })?;

        debug!("WASM module compiled in {:?}", start.elapsed());
        Ok(module)
    }

    /// Compile and cache a module
    pub fn compile_cached(&self, name: &str, wasm_bytes: &[u8]) -> Result<Module> {
        // Check cache first
        {
            let cache = self.module_cache.read();
            if let Some(cached) = cache.get(name) {
                cached
                    .use_count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                debug!("Using cached WASM module: {}", name);
                return Ok(cached.module.clone());
            }
        }

        // Compile new module
        let module = self.compile(wasm_bytes)?;

        // Cache it
        let cached = CachedModule {
            module: module.clone(),
            _compiled_at: std::time::Instant::now(),
            use_count: Arc::new(std::sync::atomic::AtomicUsize::new(1)),
        };

        self.module_cache.write().insert(name.to_string(), cached);
        debug!("Cached WASM module: {}", name);

        Ok(module)
    }

    /// Instantiate a module with host context
    ///
    /// # wasmtime 34.0 API
    /// - Store::new(engine, data) - takes reference to Engine
    /// - Instance::new(&mut store, module, &[]) - needs mutable store reference
    pub fn instantiate(&self, module: &Module) -> Result<WasmInstance> {
        // Create store with host context
        // wasmtime 34.0: Store::new takes &Engine
        let mut store = Store::new(&self.engine, HostContext::new("default-agent"));

        // Note: Fuel metering is configured at engine level with consume_fuel(true)
        // Fuel is tracked across all store operations

        // Create instance
        let instance = wasmtime::Instance::new(&mut store, module, &[])
            .map_err(|e| KernelError::internal(format!("Failed to instantiate module: {}", e)))?;

        Ok(WasmInstance::new(store, instance))
    }

    /// Instantiate with WASI support
    ///
    /// Note: wasmtime 34.0 WASI API uses the stable component model,
    /// which requires `component::Linker` and `WasiView` trait implementation.
    /// For regular (non-component) WASM modules, use `instantiate_with_host`.
    ///
    /// This method creates an instance with host functions for
    /// BeeBotOS-specific capabilities.
    ///
    /// # Arguments
    /// * `module` - The compiled WASM module (must be a component for full
    ///   WASI)
    /// * `agent_id` - Unique identifier for the agent
    /// * `caps` - Optional capability configuration
    pub fn instantiate_wasi(
        &self,
        module: &Module,
        agent_id: &str,
        _caps: Option<&crate::wasm::wasi_ctx::WasiCapabilities>,
    ) -> Result<WasmInstance> {
        // Note: Full WASI support uses the component model:
        // - Use `wasmtime::component::Linker` instead of `wasmtime::Linker`
        // - Implement `WasiView` trait for the store data
        // - Use `wasmtime_wasi::command::add_to_linker`
        //
        // For now, we instantiate with host functions which provides
        // BeeBotOS-specific capabilities without full WASI.

        let mut store = Store::new(&self.engine, HostContext::new(agent_id));

        // Create linker and add host functions
        let mut linker = wasmtime::Linker::new(&self.engine);
        HostFunctions::add_to_linker(&mut linker)?;

        // Instantiate with linker
        let instance = linker.instantiate(&mut store, module).map_err(|e| {
            KernelError::internal(format!("Failed to instantiate WASI module: {}", e))
        })?;

        Ok(WasmInstance::new(store, instance))
    }

    /// Instantiate with WASI using standard capabilities
    ///
    /// Convenience method that uses standard capability configuration.
    pub fn instantiate_wasi_standard(
        &self,
        module: &Module,
        agent_id: &str,
    ) -> Result<WasmInstance> {
        self.instantiate_wasi(module, agent_id, None)
    }

    /// Instantiate with host functions
    pub fn instantiate_with_host(&self, module: &Module, agent_id: &str) -> Result<WasmInstance> {
        // Create store with host context
        let mut store = Store::new(&self.engine, HostContext::new(agent_id));

        // Create linker and add host functions
        let mut linker = wasmtime::Linker::new(&self.engine);
        HostFunctions::add_to_linker(&mut linker)?;

        // Instantiate with linker
        let instance = linker.instantiate(&mut store, module).map_err(|e| {
            KernelError::internal(format!("Failed to instantiate host module: {}", e))
        })?;

        Ok(WasmInstance::new(store, instance))
    }

    /// Precompile module to native code
    pub fn precompile(&self, wasm_bytes: &[u8]) -> Result<Vec<u8>> {
        let serialized = self
            .engine
            .precompile_module(wasm_bytes)
            .map_err(|e| KernelError::internal(format!("Precompilation failed: {}", e)))?;

        debug!(
            "WASM module precompiled: {} bytes -> {} bytes",
            wasm_bytes.len(),
            serialized.len()
        );

        Ok(serialized.to_vec())
    }

    /// Load precompiled module
    pub fn load_precompiled(&self, name: &str, serialized: &[u8]) -> Result<Module> {
        let module = unsafe {
            Module::deserialize(&self.engine, serialized)
                .map_err(|e| KernelError::internal(format!("Deserialization failed: {}", e)))?
        };

        // Cache it
        let cached = CachedModule {
            module: module.clone(),
            _compiled_at: std::time::Instant::now(),
            use_count: Arc::new(std::sync::atomic::AtomicUsize::new(1)),
        };

        self.module_cache.write().insert(name.to_string(), cached);

        Ok(module)
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> CacheStats {
        let cache = self.module_cache.read();

        CacheStats {
            cached_modules: cache.len(),
            total_uses: cache
                .values()
                .map(|c| c.use_count.load(std::sync::atomic::Ordering::Relaxed))
                .sum(),
        }
    }

    /// Clear module cache
    pub fn clear_cache(&self) {
        self.module_cache.write().clear();
        debug!("WASM module cache cleared");
    }

    /// Get engine reference
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Get config
    pub fn config(&self) -> &EngineConfig {
        &self.config
    }
}

/// Cache statistics
#[derive(Debug, Clone, Copy)]
pub struct CacheStats {
    /// Number of cached modules
    pub cached_modules: usize,
    /// Total cache uses
    pub total_uses: usize,
}
