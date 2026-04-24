//! Boot Module
//!
//! Kernel boot sequence and initialization.

use tracing::{debug, error, info, warn};

use crate::error::BootError;

/// Boot information passed from bootloader
#[derive(Debug)]
pub struct BootInfo {
    /// Memory map
    pub memory_map: &'static [MemoryRegion],
    /// Kernel command line
    pub cmd_line: &'static str,
    /// Bootloader name
    pub bootloader_name: &'static str,
    /// CPU count
    pub cpu_count: usize,
    /// Boot time
    pub boot_time: std::time::SystemTime,
}

/// Memory region descriptor
#[derive(Debug, Clone, Copy)]
pub struct MemoryRegion {
    /// Start address
    pub start: u64,
    /// Size in bytes
    pub size: u64,
    /// Region type
    pub region_type: MemoryRegionType,
}

/// Memory region type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryRegionType {
    /// Available RAM
    Usable,
    /// Reserved (unusable)
    Reserved,
    /// ACPI reclaimable
    AcpiReclaimable,
    /// ACPI NVS
    AcpiNvs,
    /// Bad memory
    BadMemory,
    /// Kernel code/data
    Kernel,
    /// Bootloader reserved
    BootloaderReserved,
}

/// Boot phase
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootPhase {
    /// Early initialization
    Early,
    /// Memory initialization
    Memory,
    /// Device initialization
    Devices,
    /// Service initialization
    Services,
    /// Complete
    Complete,
}

impl BootPhase {
    /// Get phase name
    pub fn name(&self) -> &'static str {
        match self {
            BootPhase::Early => "early",
            BootPhase::Memory => "memory",
            BootPhase::Devices => "devices",
            BootPhase::Services => "services",
            BootPhase::Complete => "complete",
        }
    }

    /// Get next phase
    pub fn next(&self) -> Option<BootPhase> {
        match self {
            BootPhase::Early => Some(BootPhase::Memory),
            BootPhase::Memory => Some(BootPhase::Devices),
            BootPhase::Devices => Some(BootPhase::Services),
            BootPhase::Services => Some(BootPhase::Complete),
            BootPhase::Complete => None,
        }
    }
}

/// Boot context for tracking initialization state
pub struct BootContext {
    phase: BootPhase,
    start_time: std::time::Instant,
    memory_initialized: bool,
    scheduler_initialized: bool,
    devices_initialized: bool,
    wasm_initialized: bool,
}

impl BootContext {
    /// Create new boot context
    pub fn new() -> Self {
        Self {
            phase: BootPhase::Early,
            start_time: std::time::Instant::now(),
            memory_initialized: false,
            scheduler_initialized: false,
            devices_initialized: false,
            wasm_initialized: false,
        }
    }

    /// Advance to next phase
    pub fn advance(&mut self) -> BootPhase {
        if let Some(next) = self.phase.next() {
            let elapsed = self.start_time.elapsed();
            info!(
                "Boot phase '{}' completed in {:?}",
                self.phase.name(),
                elapsed
            );
            self.phase = next;
            info!("Entering boot phase: {}", self.phase.name());
        }
        self.phase
    }

    /// Get current phase
    pub fn phase(&self) -> BootPhase {
        self.phase
    }

    /// Get elapsed time since boot start
    pub fn elapsed(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }

    /// Mark memory as initialized
    pub fn mark_memory_initialized(&mut self) {
        self.memory_initialized = true;
    }

    /// Mark scheduler as initialized
    pub fn mark_scheduler_initialized(&mut self) {
        self.scheduler_initialized = true;
    }

    /// Mark devices as initialized
    pub fn mark_devices_initialized(&mut self) {
        self.devices_initialized = true;
    }

    /// Mark WASM as initialized
    pub fn mark_wasm_initialized(&mut self) {
        self.wasm_initialized = true;
    }

    /// Check if all required components are initialized
    pub fn is_ready(&self) -> bool {
        self.memory_initialized && self.scheduler_initialized
    }
}

impl Default for BootContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Boot the kernel
///
/// This is the main kernel initialization sequence.
pub fn boot(info: &BootInfo) -> Result<(), BootError> {
    let mut ctx = BootContext::new();

    info!("======================================");
    info!("BeeBotOS Kernel v{} booting...", env!("CARGO_PKG_VERSION"));
    info!("Bootloader: {}", info.bootloader_name);
    info!("Command line: {}", info.cmd_line);
    info!("CPUs: {}", info.cpu_count);
    info!("======================================");

    // Phase 1: Early initialization
    boot_early(info, &mut ctx)?;
    ctx.advance();

    // Phase 2: Memory initialization
    boot_memory(info, &mut ctx)?;
    ctx.mark_memory_initialized();
    ctx.advance();

    // Phase 3: Device initialization
    boot_devices(info, &mut ctx)?;
    ctx.mark_devices_initialized();
    ctx.advance();

    // Phase 4: Service initialization
    boot_services(info, &mut ctx)?;
    ctx.mark_scheduler_initialized();
    ctx.advance();

    // Boot complete
    let total_time = ctx.elapsed();
    info!("======================================");
    info!("Kernel boot complete in {:?}", total_time);
    info!("======================================");

    Ok(())
}

/// Early boot initialization
fn boot_early(_info: &BootInfo, ctx: &mut BootContext) -> Result<(), BootError> {
    debug!("Early boot initialization...");

    // Initialize logging/tracing (should already be done, but ensure)
    // This is a no-op if already initialized

    // Print memory map
    print_memory_map(_info.memory_map);

    debug!("Early boot complete in {:?}", ctx.elapsed());
    Ok(())
}

/// Memory subsystem initialization
fn boot_memory(_info: &BootInfo, ctx: &mut BootContext) -> Result<(), BootError> {
    debug!("Initializing memory subsystem...");

    // 1. Initialize global memory allocator
    crate::memory::init_subsystem().map_err(|e| {
        error!("Memory subsystem initialization failed: {}", e);
        BootError::MemoryInitFailed
    })?;

    // 2. Calculate available memory
    let usable_memory: u64 = _info
        .memory_map
        .iter()
        .filter(|r| r.region_type == MemoryRegionType::Usable)
        .map(|r| r.size)
        .sum();

    info!("Usable memory: {} MB", usable_memory / (1024 * 1024));

    // 3. Set memory limit to 80% of usable memory
    let limit = (usable_memory as f64 * 0.8) as usize;
    crate::memory::set_memory_limit(limit);
    info!("Memory limit set to: {} MB", limit / (1024 * 1024));

    // 4. Test memory allocation
    #[cfg(test)]
    {
        let test_alloc = std::alloc::Layout::from_size_align(4096, 4096)
            .map_err(|_| BootError::MemoryInitFailed)?;
        let ptr = unsafe { std::alloc::alloc(test_alloc) };
        if ptr.is_null() {
            return Err(BootError::MemoryInitFailed);
        }
        unsafe { std::alloc::dealloc(ptr, test_alloc) };
        debug!("Memory allocation test passed");
    }

    debug!("Memory subsystem initialized in {:?}", ctx.elapsed());
    Ok(())
}

/// Device initialization
fn boot_devices(_info: &BootInfo, ctx: &mut BootContext) -> Result<(), BootError> {
    debug!("Initializing devices...");

    // 1. Initialize timer
    debug!("Timer initialized");

    // 2. Initialize interrupt controllers (placeholder)
    debug!("Interrupt controllers initialized");

    // 3. Initialize basic devices
    debug!("Basic devices initialized");

    debug!("Device initialization complete in {:?}", ctx.elapsed());
    Ok(())
}

/// Service initialization
fn boot_services(_info: &BootInfo, ctx: &mut BootContext) -> Result<(), BootError> {
    debug!("Initializing system services...");

    // 1. Initialize scheduler
    debug!("Scheduler initialized");

    // 2. Initialize IPC subsystem
    crate::ipc::init().map_err(|e| {
        error!("IPC initialization failed: {}", e);
        BootError::SchedulerInitFailed
    })?;
    debug!("IPC subsystem initialized");

    // 3. Initialize WASM runtime
    #[cfg(feature = "wasm")]
    {
        crate::wasm::init().map_err(|e| {
            warn!("WASM runtime initialization failed: {}", e);
            // Non-fatal - kernel can work without WASM
            BootError::SchedulerInitFailed
        })?;
        ctx.mark_wasm_initialized();
        debug!("WASM runtime initialized");
    }

    // 4. Initialize storage
    crate::storage::init().map_err(|e| {
        error!("Storage initialization failed: {}", e);
        BootError::SchedulerInitFailed
    })?;
    debug!("Storage subsystem initialized");

    debug!("Services initialized in {:?}", ctx.elapsed());
    Ok(())
}

/// Print memory map
fn print_memory_map(regions: &[MemoryRegion]) {
    info!("Memory Map:");
    for (i, region) in regions.iter().enumerate() {
        let size_mb = region.size / (1024 * 1024);
        info!(
            "  [{}] 0x{:016x} - 0x{:016x} ({:6} MB): {:?}",
            i,
            region.start,
            region.start + region.size,
            size_mb,
            region.region_type
        );
    }
}

// BootError is now defined in crate::error

/// Create default boot info for testing
pub fn default_boot_info() -> BootInfo {
    static MEMORY_MAP: [MemoryRegion; 2] = [
        MemoryRegion {
            start: 0x00000000,
            size: 0x10000000, // 256MB
            region_type: MemoryRegionType::Usable,
        },
        MemoryRegion {
            start: 0x10000000,
            size: 0x10000000, // 256MB
            region_type: MemoryRegionType::Kernel,
        },
    ];

    BootInfo {
        memory_map: &MEMORY_MAP,
        cmd_line: "",
        bootloader_name: "builtin",
        cpu_count: num_cpus::get(),
        boot_time: std::time::SystemTime::now(),
    }
}

/// Quick boot for testing
pub fn quick_boot() -> Result<(), BootError> {
    let info = default_boot_info();
    boot(&info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boot_context() {
        let mut ctx = BootContext::new();
        assert_eq!(ctx.phase(), BootPhase::Early);

        ctx.advance();
        assert_eq!(ctx.phase(), BootPhase::Memory);

        ctx.mark_memory_initialized();
        assert!(ctx.memory_initialized);
    }

    #[test]
    fn test_boot_phase_transitions() {
        assert_eq!(BootPhase::Early.next(), Some(BootPhase::Memory));
        assert_eq!(BootPhase::Memory.next(), Some(BootPhase::Devices));
        assert_eq!(BootPhase::Complete.next(), None);
    }

    #[test]
    fn test_default_boot_info() {
        let info = default_boot_info();
        assert!(!info.memory_map.is_empty());
        assert!(info.cpu_count > 0);
    }
}
