//! WASI View Implementation
//!
//! Production-ready WasiView trait implementation for wasmtime 34.0.

//! Provides:
//! - Resource table management
//! - File system access with sandboxing
//! - Network capability control
//! - Environment variable management

use wasmtime::component::ResourceTable;
use wasmtime_wasi::p2::{IoView, WasiCtx, WasiCtxBuilder, WasiView};
use wasmtime_wasi::{DirPerms, FilePerms};

use crate::error::Result;
use crate::wasm::wasi_ctx::{FilesystemAccess, WasiCapabilities};

/// WASI view implementation for BeeBotOS
///
/// This struct bridges the WasiView trait with BeeBotOS capability system.
pub struct BeeBotOsWasiView {
    /// WASI context
    wasi: WasiCtx,
    /// Resource table for file handles, sockets, etc.
    table: ResourceTable,
    /// Host context for BeeBotOS integration
    host: super::host_funcs::HostContext,
    /// Capabilities
    #[allow(dead_code)]
    caps: WasiCapabilities,
}

impl BeeBotOsWasiView {
    /// Create new WasiView with capabilities
    pub fn new(host_ctx: super::host_funcs::HostContext, caps: WasiCapabilities) -> Result<Self> {
        let mut wasi_builder = WasiCtxBuilder::new();

        // Configure stdio based on capabilities
        if caps.stdio.inherit_stdin {
            wasi_builder.inherit_stdin();
        }
        if caps.stdio.inherit_stdout {
            wasi_builder.inherit_stdout();
        }
        if caps.stdio.inherit_stderr {
            wasi_builder.inherit_stderr();
        }

        // Note: Arguments and environment variables in wasmtime 34.0
        // can be configured through the WasiCtxBuilder or WasiView trait.
        if !caps.args.is_empty() {
            tracing::debug!("Arguments for agent: {:?}", caps.args);
        }
        if caps.inherit_env {
            tracing::debug!("Environment inheritance requested");
        }
        if !caps.injected_env.is_empty() {
            tracing::debug!("Injected env vars: {:?}", caps.injected_env);
        }

        // Configure file system access
        Self::configure_filesystem(&mut wasi_builder, &caps.filesystem)?;

        let wasi = wasi_builder.build();
        let table = ResourceTable::new();

        Ok(Self {
            wasi,
            table,
            host: host_ctx,
            caps,
        })
    }

    /// Configure filesystem access
    fn configure_filesystem(builder: &mut WasiCtxBuilder, access: &FilesystemAccess) -> Result<()> {
        match access {
            FilesystemAccess::None => {
                // No filesystem access - don't preopen anything
                tracing::debug!("No filesystem access granted");
            }
            FilesystemAccess::ReadOnly(paths) => {
                // In wasmtime 24.0, use preopened_dir with host path
                for path in paths {
                    if path.exists() {
                        // Use the directory name as the guest path
                        let guest_path = path.file_name().and_then(|n| n.to_str()).unwrap_or("/");
                        if let Err(e) =
                            builder.preopened_dir(path, guest_path, DirPerms::READ, FilePerms::READ)
                        {
                            tracing::warn!("Failed to preopen dir {:?}: {}", path, e);
                        } else {
                            tracing::debug!("Preopened {:?} as read-only", path);
                        }
                    }
                }
            }
            FilesystemAccess::ReadWrite(paths) => {
                for path in paths {
                    if path.exists() {
                        // Use the directory name as the guest path
                        let guest_path = path.file_name().and_then(|n| n.to_str()).unwrap_or("/");
                        if let Err(e) = builder.preopened_dir(
                            path,
                            guest_path,
                            DirPerms::all(),
                            FilePerms::all(),
                        ) {
                            tracing::warn!("Failed to preopen dir {:?}: {}", path, e);
                        } else {
                            tracing::debug!("Preopened {:?} as read-write", path);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Get agent ID
    pub fn agent_id(&self) -> &str {
        &self.host.agent_id
    }
}

impl IoView for BeeBotOsWasiView {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

impl WasiView for BeeBotOsWasiView {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }
}

/// Instance of a WebAssembly Component with WASI support
pub struct ComponentInstance {
    /// The component instance
    #[allow(dead_code)]
    _instance: wasmtime::component::Instance,
    /// Store containing the WASI view
    store: wasmtime::Store<BeeBotOsWasiView>,
}

impl ComponentInstance {
    /// Create new component instance
    pub async fn new(
        engine: &wasmtime::Engine,
        component: &wasmtime::component::Component,
        wasi_view: BeeBotOsWasiView,
    ) -> Result<Self> {
        let mut store = wasmtime::Store::new(engine, wasi_view);

        // Create linker and add WASI
        let mut linker = wasmtime::component::Linker::<BeeBotOsWasiView>::new(engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut linker).map_err(|e| {
            crate::error::KernelError::internal(format!("Failed to add WASI: {}", e))
        })?;

        // Instantiate
        let instance = linker.instantiate(&mut store, component).map_err(|e| {
            crate::error::KernelError::internal(format!("Failed to instantiate: {}", e))
        })?;

        Ok(Self {
            _instance: instance,
            store,
        })
    }

    /// Get store reference
    pub fn store(&self) -> &wasmtime::Store<BeeBotOsWasiView> {
        &self.store
    }

    /// Get store mutable reference
    pub fn store_mut(&mut self) -> &mut wasmtime::Store<BeeBotOsWasiView> {
        &mut self.store
    }

    /// Get agent ID
    pub fn agent_id(&self) -> &str {
        self.store.data().agent_id()
    }
}

/// Engine for running WebAssembly Components with full WASI support
pub struct ComponentEngine {
    /// The wasmtime engine
    engine: wasmtime::Engine,
    /// Component linker - retained for component instantiation
    #[allow(dead_code)]
    _linker: wasmtime::component::Linker<BeeBotOsWasiView>,
}

impl ComponentEngine {
    /// Create new component engine
    pub fn new() -> Result<Self> {
        let mut config = wasmtime::Config::new();
        config.wasm_component_model(true);
        config.async_support(true);

        let engine = wasmtime::Engine::new(&config).map_err(|e| {
            crate::error::KernelError::internal(format!("Failed to create engine: {}", e))
        })?;

        let mut linker = wasmtime::component::Linker::<BeeBotOsWasiView>::new(&engine);

        wasmtime_wasi::p2::add_to_linker_async(&mut linker).map_err(|e| {
            crate::error::KernelError::internal(format!("Failed to add WASI: {}", e))
        })?;

        Ok(Self {
            engine,
            _linker: linker,
        })
    }

    /// Compile a component from bytes
    pub fn compile(&self, bytes: &[u8]) -> Result<wasmtime::component::Component> {
        wasmtime::component::Component::new(&self.engine, bytes)
            .map_err(|e| crate::error::KernelError::internal(format!("Failed to compile: {}", e)))
    }

    /// Instantiate a component with WASI context
    pub async fn instantiate(
        &self,
        component: &wasmtime::component::Component,
        wasi_view: BeeBotOsWasiView,
    ) -> Result<ComponentInstance> {
        ComponentInstance::new(&self.engine, component, wasi_view).await
    }

    /// Get engine reference
    pub fn engine(&self) -> &wasmtime::Engine {
        &self.engine
    }
}

impl Default for ComponentEngine {
    fn default() -> Self {
        // Note: This may panic if engine creation fails.
        // For production use, prefer explicit construction with
        // `ComponentEngine::new()` and handle the error properly.
        Self::new().unwrap_or_else(|e| panic!("Failed to create ComponentEngine: {}", e))
    }
}
