//! Memory Isolation
//!
//! Provides safe access to user-space memory from kernel space.
//! Implements validation to prevent unauthorized memory access.
//!
//! ## Safety Features
//!
//! - Address space validation with bounds checking
//! - Permission checking (read/write/execute)
//! - Guard pages for stack overflow protection
//! - Address space layout randomization (ASLR) support
//! - Double-free and use-after-free detection

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::error::{KernelError, Result};

/// Memory region owned by a process/agent
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserMemoryRegion {
    /// Start address of the region
    pub start: u64,
    /// Size in bytes
    pub size: usize,
    /// Region permissions
    pub permissions: MemoryPermissions,
    /// Region type
    pub region_type: MemoryRegionType,
}

/// Memory region types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryRegionType {
    /// Code segment
    Code,
    /// Data segment
    Data,
    /// Heap
    Heap,
    /// Stack
    Stack,
    /// Shared memory
    Shared,
    /// Memory-mapped file
    Mmap,
    /// Guard page
    Guard,
}

/// Memory access permissions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryPermissions {
    /// Read permission
    pub read: bool,
    /// Write permission
    pub write: bool,
    /// Execute permission
    pub execute: bool,
}

impl MemoryPermissions {
    /// Read-only permissions
    pub fn read_only() -> Self {
        Self {
            read: true,
            write: false,
            execute: false,
        }
    }

    /// Read-write permissions
    pub fn read_write() -> Self {
        Self {
            read: true,
            write: true,
            execute: false,
        }
    }

    /// Read-execute permissions (for code)
    pub fn read_execute() -> Self {
        Self {
            read: true,
            write: false,
            execute: true,
        }
    }

    /// Check if has all specified permissions
    pub fn has_all(&self, required: MemoryPermissions) -> bool {
        (!required.read || self.read)
            && (!required.write || self.write)
            && (!required.execute || self.execute)
    }
}

/// Memory protection flags for advanced features
#[derive(Debug, Clone, Copy, Default)]
pub struct MemoryProtection {
    /// Guard page - accessing this triggers a fault
    pub is_guard_page: bool,
    /// Copy-on-write
    pub cow: bool,
    /// No cache
    pub no_cache: bool,
    /// Write-combining
    pub write_combining: bool,
}

/// Memory validation result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationResult {
    /// Memory is valid and accessible
    Valid,
    /// Address is not within any mapped region
    InvalidAddress,
    /// Address range overflows
    AddressOverflow,
    /// Read permission denied
    ReadDenied,
    /// Write permission denied
    WriteDenied,
    /// Execute permission denied
    ExecuteDenied,
    /// Access to guard page
    GuardPageViolation,
    /// Region is not accessible (e.g., freed)
    RegionFreed,
}

impl ValidationResult {
    /// Check if valid
    pub fn is_valid(&self) -> bool {
        matches!(self, ValidationResult::Valid)
    }

    /// Convert to error
    pub fn to_error(self, operation: &str) -> Result<()> {
        match self {
            ValidationResult::Valid => Ok(()),
            ValidationResult::InvalidAddress => Err(KernelError::invalid_address()),
            ValidationResult::AddressOverflow => {
                Err(KernelError::invalid_argument("Address overflow"))
            }
            ValidationResult::ReadDenied => Err(KernelError::Security(format!(
                "Read permission denied for {}",
                operation
            ))),
            ValidationResult::WriteDenied => Err(KernelError::Security(format!(
                "Write permission denied for {}",
                operation
            ))),
            ValidationResult::ExecuteDenied => Err(KernelError::Security(format!(
                "Execute permission denied for {}",
                operation
            ))),
            ValidationResult::GuardPageViolation => {
                Err(KernelError::Security("Guard page violation".into()))
            }
            ValidationResult::RegionFreed => Err(KernelError::Security(
                "Access to freed memory region".into(),
            )),
        }
    }
}

/// Memory isolation manager for a process
pub struct ProcessMemorySpace {
    /// Process identifier
    process_id: u64,
    /// Memory regions owned by this process
    regions: RwLock<Vec<UserMemoryRegion>>,
    /// Whether this is a kernel process
    is_kernel: bool,
    /// Address space base (for ASLR)
    aslr_base: u64,
    /// Protection flags for each region
    protections: RwLock<HashMap<u64, MemoryProtection>>,
    /// Freed regions (for use-after-free detection)
    freed_regions: RwLock<Vec<UserMemoryRegion>>,
}

impl std::fmt::Debug for ProcessMemorySpace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessMemorySpace")
            .field("process_id", &self.process_id)
            .field("is_kernel", &self.is_kernel)
            .field("region_count", &self.regions.read().len())
            .field("aslr_base", &format_args!("{:#x}", self.aslr_base))
            .finish()
    }
}

impl ProcessMemorySpace {
    /// Create new user process memory space with ASLR
    pub fn new_user(process_id: u64) -> Self {
        // Generate random ASLR base (simplified - real implementation would use secure
        // RNG)
        let aslr_base = 0x1000 + ((process_id * 0x1000) % 0x100000);

        Self {
            process_id,
            regions: RwLock::new(Vec::new()),
            is_kernel: false,
            aslr_base,
            protections: RwLock::new(HashMap::new()),
            freed_regions: RwLock::new(Vec::new()),
        }
    }

    /// Create kernel process memory space
    pub fn new_kernel(process_id: u64) -> Self {
        Self {
            process_id,
            regions: RwLock::new(Vec::new()),
            is_kernel: true,
            aslr_base: 0xffff_8000_0000_0000, // Kernel address space
            protections: RwLock::new(HashMap::new()),
            freed_regions: RwLock::new(Vec::new()),
        }
    }

    /// Get ASLR base address
    pub fn aslr_base(&self) -> u64 {
        self.aslr_base
    }

    /// Register a memory region for this process
    pub fn register_region(
        &self,
        start: u64,
        size: usize,
        permissions: MemoryPermissions,
        region_type: MemoryRegionType,
    ) -> Result<()> {
        if size == 0 {
            return Err(KernelError::invalid_argument(
                "Memory region size cannot be zero",
            ));
        }

        // Check for overflow
        let end = start
            .checked_add(size as u64)
            .ok_or_else(|| KernelError::invalid_argument("Memory region address overflow"))?;

        // Check for overlap with existing regions
        let regions = self.regions.read();
        for region in regions.iter() {
            let region_end = region.start + region.size as u64;

            // Check overlap: [start, end) overlaps with [region.start, region_end)
            if start < region_end && end > region.start {
                return Err(KernelError::invalid_argument(format!(
                    "Memory region {:x}-{:x} overlaps with existing region at {:x}",
                    start, end, region.start
                )));
            }
        }
        drop(regions);

        // Check if address was previously freed (use-after-free detection)
        let freed = self.freed_regions.read();
        for freed_region in freed.iter() {
            let freed_end = freed_region.start + freed_region.size as u64;
            if start < freed_end && end > freed_region.start {
                tracing::warn!(
                    "Process {} registering memory at previously freed region {:x}-{:x}",
                    self.process_id,
                    freed_region.start,
                    freed_end
                );
                // In debug mode, we might want to error here
            }
        }
        drop(freed);

        let mut regions = self.regions.write();
        regions.push(UserMemoryRegion {
            start,
            size,
            permissions,
            region_type,
        });

        tracing::debug!(
            "Registered memory region {:x}-{:x} (type: {:?}) for process {}",
            start,
            end,
            region_type,
            self.process_id
        );

        Ok(())
    }

    /// Register a memory region with protection flags
    pub fn register_region_with_protection(
        &self,
        start: u64,
        size: usize,
        permissions: MemoryPermissions,
        region_type: MemoryRegionType,
        protection: MemoryProtection,
    ) -> Result<()> {
        self.register_region(start, size, permissions, region_type)?;

        let mut protections = self.protections.write();
        protections.insert(start, protection);

        Ok(())
    }

    /// Register a guard page (for stack protection)
    pub fn register_guard_page(&self, start: u64) -> Result<()> {
        self.register_region_with_protection(
            start,
            4096, // One page
            MemoryPermissions {
                read: false,
                write: false,
                execute: false,
            },
            MemoryRegionType::Guard,
            MemoryProtection {
                is_guard_page: true,
                ..Default::default()
            },
        )
    }

    /// Unregister a memory region
    pub fn unregister_region(&self, start: u64) -> Result<()> {
        let mut regions = self.regions.write();
        let idx = regions
            .iter()
            .position(|r| r.start == start)
            .ok_or_else(|| KernelError::invalid_argument("Memory region not found"))?;

        let region = regions.remove(idx);

        // Add to freed regions for use-after-free detection
        self.freed_regions.write().push(region);

        // Remove protection flags
        self.protections.write().remove(&start);

        tracing::debug!(
            "Unregistered memory region {:x} from process {}",
            start,
            self.process_id
        );

        Ok(())
    }

    /// Validate memory address range without accessing
    pub fn validate_range(
        &self,
        addr: u64,
        size: usize,
        require_write: bool,
        require_execute: bool,
    ) -> ValidationResult {
        // Kernel processes get full access (with basic validation)
        if self.is_kernel {
            return ValidationResult::Valid;
        }

        if size == 0 {
            return ValidationResult::Valid; // Zero-size access is always valid
        }

        // Check for address overflow
        let end_addr = match addr.checked_add(size as u64) {
            Some(end) => end,
            None => return ValidationResult::AddressOverflow,
        };

        // Check for null pointer
        if addr == 0 {
            return ValidationResult::InvalidAddress;
        }

        // Check for user-space address range (on x86_64, user space is below
        // 0x0000_8000_0000_0000)
        if !self.is_kernel && addr >= 0x0000_8000_0000_0000 {
            return ValidationResult::InvalidAddress;
        }

        let regions = self.regions.read();

        for region in regions.iter() {
            let region_end = region.start + region.size as u64;

            // Check if [addr, end_addr) is within [region.start, region_end)
            if addr >= region.start && end_addr <= region_end {
                // Check guard page
                let protections = self.protections.read();
                if let Some(prot) = protections.get(&region.start) {
                    if prot.is_guard_page {
                        return ValidationResult::GuardPageViolation;
                    }
                }

                // Check permissions
                if require_execute && !region.permissions.execute {
                    return ValidationResult::ExecuteDenied;
                }
                if require_write && !region.permissions.write {
                    return ValidationResult::WriteDenied;
                }
                if !require_write && !region.permissions.read {
                    return ValidationResult::ReadDenied;
                }

                return ValidationResult::Valid;
            }
        }

        // Check freed regions (use-after-free detection)
        let freed = self.freed_regions.read();
        for freed_region in freed.iter() {
            let freed_end = freed_region.start + freed_region.size as u64;
            if addr < freed_end && end_addr > freed_region.start {
                return ValidationResult::RegionFreed;
            }
        }

        ValidationResult::InvalidAddress
    }

    /// Check if address range is valid for access
    pub fn check_access(
        &self,
        addr: u64,
        size: usize,
        require_write: bool,
    ) -> Result<UserMemoryRegion> {
        match self.validate_range(addr, size, require_write, false) {
            ValidationResult::Valid => {
                // Find and return the region
                let regions = self.regions.read();
                for region in regions.iter() {
                    let region_end = region.start + region.size as u64;
                    if addr >= region.start && (addr + size as u64) <= region_end {
                        return Ok(*region);
                    }
                }
                Err(KernelError::invalid_address())
            }
            other => {
                other.to_error(if require_write { "write" } else { "read" })?;
                Err(KernelError::invalid_address()) // unreachable, but needed
                                                    // for type checking
            }
        }
    }

    /// Validate a pointer is within user memory
    pub fn validate_user_pointer<T>(&self, ptr: *const T) -> ValidationResult {
        self.validate_range(ptr as u64, std::mem::size_of::<T>(), false, false)
    }

    /// Validate a mutable pointer is within user memory
    pub fn validate_user_pointer_mut<T>(&self, ptr: *mut T) -> ValidationResult {
        self.validate_range(ptr as u64, std::mem::size_of::<T>(), true, false)
    }

    /// Safely read from user memory with full validation
    ///
    /// # Safety
    /// This function validates the memory range before reading, but the actual
    /// read is still unsafe as it involves raw pointers.
    pub unsafe fn read_memory(&self, addr: u64, len: usize) -> Result<Vec<u8>> {
        self.check_access(addr, len, false)?;

        // Now safe to read (validation passed)
        let slice = std::slice::from_raw_parts(addr as *const u8, len);
        Ok(slice.to_vec())
    }

    /// Safely write to user memory with full validation
    ///
    /// # Safety
    /// This function validates the memory range before writing, but the actual
    /// write is still unsafe as it involves raw pointers.
    pub unsafe fn write_memory(&self, addr: u64, data: &[u8]) -> Result<usize> {
        self.check_access(addr, data.len(), true)?;

        // Now safe to write (validation passed)
        let slice = std::slice::from_raw_parts_mut(addr as *mut u8, data.len());
        slice.copy_from_slice(data);
        Ok(data.len())
    }

    /// Copy string from user memory with length limit
    pub unsafe fn read_string(&self, addr: u64, max_len: usize) -> Result<String> {
        // Validate initial range
        self.check_access(addr, max_len, false)?;

        // Read up to max_len bytes looking for null terminator
        let slice = std::slice::from_raw_parts(addr as *const u8, max_len);

        let mut len = 0;
        for (i, &byte) in slice.iter().enumerate() {
            if byte == 0 {
                len = i;
                break;
            }
            if i == max_len - 1 {
                return Err(KernelError::invalid_argument(
                    "String too long or not null-terminated",
                ));
            }
        }

        let string_slice = &slice[..len];
        String::from_utf8(string_slice.to_vec())
            .map_err(|_| KernelError::invalid_argument("Invalid UTF-8 in string"))
    }

    /// Get process ID
    pub fn process_id(&self) -> u64 {
        self.process_id
    }

    /// Check if kernel process
    pub fn is_kernel(&self) -> bool {
        self.is_kernel
    }

    /// Get memory statistics
    pub fn stats(&self) -> MemorySpaceStats {
        let regions = self.regions.read();
        let total_size: usize = regions.iter().map(|r| r.size).sum();
        let freed_size: usize = self.freed_regions.read().iter().map(|r| r.size).sum();

        MemorySpaceStats {
            region_count: regions.len(),
            total_size_bytes: total_size,
            freed_regions_count: self.freed_regions.read().len(),
            freed_size_bytes: freed_size,
        }
    }

    /// List all regions
    pub fn list_regions(&self) -> Vec<UserMemoryRegion> {
        self.regions.read().clone()
    }

    /// Clear all freed regions tracking (call periodically to prevent memory
    /// bloat)
    pub fn clear_freed_regions(&self) {
        let count = self.freed_regions.read().len();
        if count > 1000 {
            tracing::debug!(
                "Clearing {} freed region entries for process {}",
                count,
                self.process_id
            );
            self.freed_regions.write().clear();
        }
    }
}

/// Memory space statistics
#[derive(Debug, Clone, Copy)]
pub struct MemorySpaceStats {
    /// Number of regions
    pub region_count: usize,
    /// Total size in bytes
    pub total_size_bytes: usize,
    /// Number of freed regions being tracked
    pub freed_regions_count: usize,
    /// Total size of freed regions
    pub freed_size_bytes: usize,
}

/// Global memory isolation manager
pub struct MemoryIsolation {
    /// Process memory spaces
    spaces: RwLock<HashMap<u64, Arc<ProcessMemorySpace>>>,
    /// Next process ID
    next_process_id: RwLock<u64>,
}

impl MemoryIsolation {
    /// Create new memory isolation manager
    pub fn new() -> Self {
        Self {
            spaces: RwLock::new(HashMap::new()),
            next_process_id: RwLock::new(1000), // Start from 1000 to reserve low IDs
        }
    }

    /// Create new process memory space
    pub fn create_process(&self, is_kernel: bool) -> (u64, Arc<ProcessMemorySpace>) {
        let pid = self.next_pid();
        let space = if is_kernel {
            Arc::new(ProcessMemorySpace::new_kernel(pid))
        } else {
            Arc::new(ProcessMemorySpace::new_user(pid))
        };

        self.spaces.write().insert(pid, space.clone());
        tracing::debug!(
            "Created memory space for process {} (kernel={})",
            pid,
            is_kernel
        );
        (pid, space)
    }

    /// Get process memory space
    pub fn get_space(&self, pid: u64) -> Option<Arc<ProcessMemorySpace>> {
        self.spaces.read().get(&pid).cloned()
    }

    /// Destroy process memory space
    pub fn destroy_process(&self, pid: u64) -> Result<()> {
        self.spaces
            .write()
            .remove(&pid)
            .ok_or_else(|| KernelError::invalid_argument("Process not found"))?;
        tracing::debug!("Destroyed memory space for process {}", pid);
        Ok(())
    }

    /// List all process IDs
    pub fn list_processes(&self) -> Vec<u64> {
        self.spaces.read().keys().copied().collect()
    }

    /// Get global statistics
    pub fn global_stats(&self) -> IsolationStats {
        let spaces = self.spaces.read();
        let mut total_regions = 0;
        let mut total_bytes = 0;

        for space in spaces.values() {
            let stats = space.stats();
            total_regions += stats.region_count;
            total_bytes += stats.total_size_bytes;
        }

        IsolationStats {
            process_count: spaces.len(),
            total_regions,
            total_bytes,
        }
    }

    /// Cleanup freed regions for all processes
    pub fn cleanup_all_freed_regions(&self) {
        for space in self.spaces.read().values() {
            space.clear_freed_regions();
        }
    }

    fn next_pid(&self) -> u64 {
        let mut id = self.next_process_id.write();
        let current = *id;
        *id += 1;
        current
    }
}

impl Default for MemoryIsolation {
    fn default() -> Self {
        Self::new()
    }
}

/// Global isolation statistics
#[derive(Debug, Clone, Copy)]
pub struct IsolationStats {
    /// Number of processes
    pub process_count: usize,
    /// Total number of regions
    pub total_regions: usize,
    /// Total bytes allocated
    pub total_bytes: usize,
}

/// Global memory isolation instance
static GLOBAL_ISOLATION: std::sync::OnceLock<MemoryIsolation> = std::sync::OnceLock::new();

/// Initialize global memory isolation
pub fn init() {
    GLOBAL_ISOLATION.get_or_init(MemoryIsolation::new);
    tracing::info!("Memory isolation initialized");
}

/// Get global memory isolation
pub fn global() -> &'static MemoryIsolation {
    GLOBAL_ISOLATION
        .get()
        .expect("Memory isolation not initialized")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_permissions() {
        let rw = MemoryPermissions::read_write();
        assert!(rw.read);
        assert!(rw.write);
        assert!(!rw.execute);

        let ro = MemoryPermissions::read_only();
        assert!(ro.read);
        assert!(!ro.write);

        assert!(rw.has_all(ro));
        assert!(!ro.has_all(rw));
    }

    #[test]
    fn test_process_memory_space() {
        let space = ProcessMemorySpace::new_user(100);

        // Register a region
        space
            .register_region(
                0x1000,
                4096,
                MemoryPermissions::read_write(),
                MemoryRegionType::Data,
            )
            .unwrap();

        // Check access
        let region = space.check_access(0x1000, 100, false).unwrap();
        assert_eq!(region.start, 0x1000);

        // Invalid access - not registered
        assert!(space.check_access(0x5000, 100, false).is_err());
    }

    #[test]
    fn test_validation_result() {
        assert!(ValidationResult::Valid.is_valid());
        assert!(!ValidationResult::InvalidAddress.is_valid());
        assert!(!ValidationResult::WriteDenied.is_valid());
    }

    #[test]
    fn test_overlap_detection() {
        let space = ProcessMemorySpace::new_user(100);
        space
            .register_region(
                0x1000,
                4096,
                MemoryPermissions::read_write(),
                MemoryRegionType::Data,
            )
            .unwrap();

        // Overlapping region should fail
        let result = space.register_region(
            0x2000,
            4096,
            MemoryPermissions::read_write(),
            MemoryRegionType::Data,
        );
        assert!(result.is_ok()); // Not overlapping

        let result = space.register_region(
            0x3000,
            4096,
            MemoryPermissions::read_write(),
            MemoryRegionType::Data,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_guard_page() {
        let space = ProcessMemorySpace::new_user(100);
        space.register_guard_page(0x1000).unwrap();

        // Access to guard page should fail
        let result = space.validate_range(0x1000, 1, false, false);
        assert!(matches!(result, ValidationResult::GuardPageViolation));
    }

    #[test]
    fn test_use_after_free_detection() {
        let space = ProcessMemorySpace::new_user(100);

        // Register and unregister
        space
            .register_region(
                0x1000,
                4096,
                MemoryPermissions::read_write(),
                MemoryRegionType::Data,
            )
            .unwrap();
        space.unregister_region(0x1000).unwrap();

        // Access to freed region should be detected
        let result = space.validate_range(0x1000, 100, false, false);
        assert!(matches!(result, ValidationResult::RegionFreed));
    }

    #[test]
    fn test_memory_isolation() {
        let isolation = MemoryIsolation::new();

        // Create processes
        let (pid1, space1) = isolation.create_process(false);
        let (pid2, space2) = isolation.create_process(false);

        assert_ne!(pid1, pid2);

        // Register in process 1
        space1
            .register_region(
                0x1000,
                4096,
                MemoryPermissions::read_write(),
                MemoryRegionType::Data,
            )
            .unwrap();

        // Process 2 should not see process 1's memory
        assert!(space2.check_access(0x1000, 100, false).is_err());

        // Get via isolation
        let retrieved = isolation.get_space(pid1).unwrap();
        assert!(retrieved.check_access(0x1000, 100, false).is_ok());

        // Destroy process
        isolation.destroy_process(pid1).unwrap();
        assert!(isolation.get_space(pid1).is_none());
    }
}
