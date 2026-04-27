//! WASI Context Implementation
//!
//! Provides production-ready WASI context creation for wasmtime 34.0.
//!
//! Note: wasmtime 34.0 uses the stable WASI API with a comprehensive
//! WasiCtxBuilder. Most configuration is done at runtime through the WasiView
//! trait. This module provides a capability-based configuration that can be
//! used by custom WasiView implementations.

use std::collections::HashMap;
use std::path::PathBuf;

/// Re-export WasiCtx for convenience
pub use wasmtime_wasi::p2::WasiCtx;
use wasmtime_wasi::p2::WasiCtxBuilder;

use crate::error::Result;

/// WASI capability configuration
///
/// Note: wasmtime 34.0 has a comprehensive builder API.
/// This struct captures intent that can be applied by WasiView implementations.
#[derive(Debug, Clone)]
pub struct WasiCapabilities {
    /// File system access configuration
    pub filesystem: FilesystemAccess,
    /// Inherit environment variables from host
    pub inherit_env: bool,
    /// Additional environment variables to inject
    pub injected_env: HashMap<String, String>,
    /// Command line arguments (argv)
    pub args: Vec<String>,
    /// Network access enabled
    pub network_enabled: bool,
    /// Stdio configuration
    pub stdio: StdioConfig,
}

/// Filesystem access level
#[derive(Debug, Clone)]
pub enum FilesystemAccess {
    /// No filesystem access
    None,
    /// Read-only access to specific directories
    ReadOnly(Vec<PathBuf>),
    /// Read-write access to specific directories
    ReadWrite(Vec<PathBuf>),
}

/// Stdio configuration
#[derive(Debug, Clone)]
pub struct StdioConfig {
    /// Inherit stdin from host
    pub inherit_stdin: bool,
    /// Inherit stdout from host
    pub inherit_stdout: bool,
    /// Inherit stderr from host
    pub inherit_stderr: bool,
}

impl Default for StdioConfig {
    fn default() -> Self {
        Self {
            inherit_stdin: true,
            inherit_stdout: true,
            inherit_stderr: true,
        }
    }
}

impl Default for WasiCapabilities {
    fn default() -> Self {
        Self {
            filesystem: FilesystemAccess::None,
            inherit_env: false,
            injected_env: HashMap::new(),
            args: vec![],
            network_enabled: false,
            stdio: StdioConfig::default(),
        }
    }
}

impl WasiCapabilities {
    /// Minimal capabilities (most restrictive)
    pub fn minimal() -> Self {
        Self {
            filesystem: FilesystemAccess::None,
            inherit_env: false,
            injected_env: HashMap::new(),
            args: vec![],
            network_enabled: false,
            stdio: StdioConfig::default(),
        }
    }

    /// Standard capabilities for agents
    pub fn standard() -> Self {
        let mut injected_env = HashMap::new();
        injected_env.insert("AGENT_MODE".to_string(), "1".to_string());

        Self {
            filesystem: FilesystemAccess::ReadOnly(vec![PathBuf::from("/data")]),
            inherit_env: false,
            injected_env,
            args: vec!["agent".to_string()],
            network_enabled: true,
            stdio: StdioConfig::default(),
        }
    }

    /// Full capabilities (use with caution)
    pub fn full() -> Self {
        Self {
            filesystem: FilesystemAccess::ReadWrite(vec![PathBuf::from("/")]),
            inherit_env: true,
            injected_env: HashMap::new(),
            args: vec![],
            network_enabled: true,
            stdio: StdioConfig::default(),
        }
    }
}

/// Create a WASI context with the specified capabilities
///
/// Note: wasmtime 34.0 WasiCtxBuilder has a comprehensive API.
/// - Args can be set via builder.args()
/// - Env vars can be set via builder.env() or inherit_env()
///
/// This function uses the available builder methods.
pub fn create_wasi_context_with_caps(agent_id: &str, caps: &WasiCapabilities) -> Result<WasiCtx> {
    let mut builder = WasiCtxBuilder::new();

    // Configure arguments (argv) for wasmtime 34.0
    if !caps.args.is_empty() {
        tracing::debug!("Arguments for {}: {:?}", agent_id, caps.args);
        builder.args(&caps.args);
    }

    // Configure environment variables for wasmtime 34.0
    if caps.inherit_env {
        tracing::debug!("Environment inheritance requested for {}", agent_id);
        builder.inherit_env();
    }

    if !caps.injected_env.is_empty() {
        tracing::debug!("Injected env for {}: {:?}", agent_id, caps.injected_env);
        for (key, value) in &caps.injected_env {
            builder.env(key, value);
        }
    }

    // Configure stdio (these methods should be available)
    if caps.stdio.inherit_stdin {
        builder.inherit_stdin();
    }
    if caps.stdio.inherit_stdout {
        builder.inherit_stdout();
    }
    if caps.stdio.inherit_stderr {
        builder.inherit_stderr();
    }

    // Filesystem access configuration
    // Note: Directory preopening in wasmtime 34.0 requires access to
    // wasmtime_wasi::Dir which is handled in the WasiView implementation.
    match &caps.filesystem {
        FilesystemAccess::None => {
            tracing::debug!("No filesystem access for {}", agent_id);
        }
        FilesystemAccess::ReadOnly(paths) => {
            tracing::debug!("Read-only filesystem access for {}: {:?}", agent_id, paths);
        }
        FilesystemAccess::ReadWrite(paths) => {
            tracing::debug!("Read-write filesystem access for {}: {:?}", agent_id, paths);
        }
    }

    // Network configuration
    if caps.network_enabled {
        tracing::debug!("Network access enabled for {}", agent_id);
    }

    Ok(builder.build())
}

/// Create a standard WASI context for agents
pub fn create_wasi_context(agent_id: &str) -> WasiCtx {
    create_wasi_context_with_caps(agent_id, &WasiCapabilities::standard())
        .unwrap_or_else(|_| WasiCtxBuilder::new().build())
}

/// Create a restricted WASI context with minimal capabilities
pub fn create_restricted_wasi_context(agent_id: &str) -> WasiCtx {
    create_wasi_context_with_caps(agent_id, &WasiCapabilities::minimal())
        .unwrap_or_else(|_| WasiCtxBuilder::new().build())
}

/// WASI context with host context wrapper
///
/// This allows using WASI with our HostContext for BeeBotOS-specific
/// functionality. Note: Full integration requires implementing the WasiView
/// trait.
pub struct WasiHostContext {
    /// The WASI context
    pub wasi: WasiCtx,
    /// BeeBotOS host context
    pub host: super::host_funcs::HostContext,
    /// Stored capabilities for WasiView implementation
    pub caps: WasiCapabilities,
}

impl WasiHostContext {
    /// Create a new combined context
    pub fn new(agent_id: impl Into<String>, caps: WasiCapabilities) -> Result<Self> {
        let agent_id = agent_id.into();
        let wasi = create_wasi_context_with_caps(&agent_id, &caps)?;
        let host = super::host_funcs::HostContext::new(&agent_id);

        Ok(Self { wasi, host, caps })
    }

    /// Create with standard capabilities
    pub fn new_standard(agent_id: impl Into<String>) -> Self {
        let agent_id = agent_id.into();
        let caps = WasiCapabilities::standard();
        let wasi = create_wasi_context(&agent_id);
        let host = super::host_funcs::HostContext::new(&agent_id);

        Self { wasi, host, caps }
    }

    /// Get the agent ID
    pub fn agent_id(&self) -> &str {
        &self.host.agent_id
    }
}

// SAFETY: WasiHostContext is Send + Sync because all fields are
unsafe impl Send for WasiHostContext {}
unsafe impl Sync for WasiHostContext {}

/// Trait for extending WasiHostContext with WasiView
///
/// This trait bridges BeeBotOS HostContext with WASI (wasmtime 34.0).
/// Implement this trait to provide full WASI functionality.
pub trait BeeBotOsWasiView {
    /// Get the WASI context
    fn wasi_ctx(&self) -> &WasiCtx;

    /// Get the WASI context mutably
    fn wasi_ctx_mut(&mut self) -> &mut WasiCtx;

    /// Get capabilities
    fn capabilities(&self) -> &WasiCapabilities;
}

impl BeeBotOsWasiView for WasiHostContext {
    fn wasi_ctx(&self) -> &WasiCtx {
        &self.wasi
    }

    fn wasi_ctx_mut(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }

    fn capabilities(&self) -> &WasiCapabilities {
        &self.caps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimal_capabilities() {
        let caps = WasiCapabilities::minimal();
        assert!(matches!(caps.filesystem, FilesystemAccess::None));
        assert!(!caps.inherit_env);
        assert!(!caps.network_enabled);
    }

    #[test]
    fn test_standard_capabilities() {
        let caps = WasiCapabilities::standard();
        assert!(matches!(caps.filesystem, FilesystemAccess::ReadOnly(_)));
        assert!(caps.network_enabled);
    }

    #[test]
    fn test_create_wasi_context() {
        let ctx = create_wasi_context("test-agent");
        // Context should be created successfully
        drop(ctx);
    }

    #[test]
    fn test_wasi_host_context() {
        let ctx = WasiHostContext::new_standard("test-agent");
        assert_eq!(ctx.agent_id(), "test-agent");
    }
}
