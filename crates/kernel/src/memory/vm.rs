//! Virtual Memory Management
//!
//! Provides virtual memory address space management with:
//! - Region allocation and protection
//! - Memory mapping
//! - Page table management
//! - Address space isolation

use std::collections::BTreeMap;
use std::sync::OnceLock;

use parking_lot::RwLock;
use tracing::{debug, trace};

use crate::error::{KernelError, Result};

/// Virtual address space size (48-bit on x86_64)
pub const VIRTUAL_ADDRESS_SPACE_SIZE: u64 = 1 << 48;

/// Page size (4KB)
pub const PAGE_SIZE: usize = 4096;

/// Default user space start address
pub const USER_SPACE_START: u64 = 0x400000;

/// Default user space end address
pub const USER_SPACE_END: u64 = 0x7FFF_FFFF_FFFF;

/// Kernel space start address
pub const KERNEL_SPACE_START: u64 = 0xFFFF_8000_0000_0000;

/// Region flags for virtual memory
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegionFlags {
    /// Region can be read
    pub readable: bool,
    /// Region can be written
    pub writable: bool,
    /// Region can be executed
    pub executable: bool,
    /// Region accessible from user mode
    pub user_accessible: bool,
    /// Region is shared between processes
    pub shared: bool,
    /// Region grows downward (stack)
    pub grow_down: bool,
}

impl RegionFlags {
    /// Read-only region
    pub const fn read_only() -> Self {
        Self {
            readable: true,
            writable: false,
            executable: false,
            user_accessible: false,
            shared: false,
            grow_down: false,
        }
    }

    /// Read-write region
    pub const fn read_write() -> Self {
        Self {
            readable: true,
            writable: true,
            executable: false,
            user_accessible: false,
            shared: false,
            grow_down: false,
        }
    }

    /// Read-write-execute region (for JIT, etc.)
    pub const fn read_write_exec() -> Self {
        Self {
            readable: true,
            writable: true,
            executable: true,
            user_accessible: false,
            shared: false,
            grow_down: false,
        }
    }

    /// User-accessible region
    pub const fn user(flags: Self) -> Self {
        Self {
            user_accessible: true,
            ..flags
        }
    }

    /// Convert to page table flags
    pub fn to_page_flags(&self) -> super::PageFlags {
        super::PageFlags {
            present: true,
            writable: self.writable,
            user_accessible: self.user_accessible,
        }
    }
}

impl Default for RegionFlags {
    fn default() -> Self {
        Self::read_write()
    }
}

/// Virtual memory region
#[derive(Debug, Clone)]
pub struct VirtualRegion {
    /// Start address
    pub start: u64,
    /// Size in bytes
    pub size: usize,
    /// Region flags
    pub flags: RegionFlags,
    /// Backing type
    pub backing: RegionBacking,
    /// Region name
    pub name: String,
}

impl VirtualRegion {
    /// Check if address is within this region
    pub fn contains(&self, addr: u64) -> bool {
        addr >= self.start && addr < self.start + self.size as u64
    }

    /// Get end address
    pub fn end(&self) -> u64 {
        self.start + self.size as u64
    }

    /// Check if region overlaps with another
    pub fn overlaps(&self, other: &Self) -> bool {
        self.start < other.end() && other.start < self.end()
    }

    /// Split region at given address
    pub fn split_at(&self, addr: u64) -> Option<(Self, Self)> {
        if addr <= self.start || addr >= self.end() {
            return None;
        }

        let first_size = (addr - self.start) as usize;
        let second_size = self.size - first_size;

        let first = Self {
            start: self.start,
            size: first_size,
            flags: self.flags,
            backing: self.backing.clone(),
            name: format!("{}_lo", self.name),
        };

        let second = Self {
            start: addr,
            size: second_size,
            flags: self.flags,
            backing: self.backing.clone(),
            name: format!("{}_hi", self.name),
        };

        Some((first, second))
    }
}

/// Region backing type
#[derive(Debug, Clone)]
pub enum RegionBacking {
    /// Anonymous memory (zeroed)
    Anonymous,
    /// File-backed memory
    File {
        /// File path
        path: String,
        /// Offset within file
        offset: u64,
    },
    /// Physical memory mapping
    Physical {
        /// Physical address
        paddr: u64,
    },
    /// Shared memory
    Shared {
        /// Shared memory ID
        id: String,
    },
}

impl Default for RegionBacking {
    fn default() -> Self {
        Self::Anonymous
    }
}

/// Virtual memory address space
/// Virtual memory address space manager
///
/// # Thread Safety
/// All public methods are thread-safe using interior mutability.
///
/// # Lock Ordering
/// When acquiring multiple locks, always follow this order to prevent
/// deadlocks:
/// 1. `regions` (RwLock)
/// 2. `next_alloc` (RwLock)
///
/// Currently only `allocate()` acquires both locks.
pub struct VirtualMemory {
    /// Region map: start address -> region
    /// Lock order: acquire this first if acquiring multiple locks
    regions: RwLock<BTreeMap<u64, VirtualRegion>>,
    /// Next allocation hint
    /// Lock order: acquire after `regions` if acquiring multiple locks
    next_alloc: RwLock<u64>,
    /// Address space limits
    start_limit: u64,
    end_limit: u64,
}

impl VirtualMemory {
    /// Create a new virtual memory address space
    pub fn new() -> Self {
        Self::with_limits(USER_SPACE_START, USER_SPACE_END)
    }

    /// Create with custom limits
    pub fn with_limits(start: u64, end: u64) -> Self {
        Self {
            regions: RwLock::new(BTreeMap::new()),
            next_alloc: RwLock::new(start),
            start_limit: start,
            end_limit: end,
        }
    }

    /// Allocate a virtual memory region
    ///
    /// Lock acquisition order: regions, then next_alloc
    pub fn allocate(&self, size: usize, flags: RegionFlags, name: &str) -> Result<u64> {
        let aligned_size = Self::align_up(size, PAGE_SIZE);

        // Lock order: regions first, then next_alloc (following documented order)
        let mut regions = self.regions.write();
        let mut next_alloc = self.next_alloc.write();

        // Find a free region
        let start = self.find_free_space(&regions, aligned_size, *next_alloc)?;

        if start + aligned_size as u64 > self.end_limit {
            return Err(KernelError::out_of_memory());
        }

        let region = VirtualRegion {
            start,
            size: aligned_size,
            flags,
            backing: RegionBacking::Anonymous,
            name: name.to_string(),
        };

        regions.insert(start, region);
        *next_alloc = start + aligned_size as u64;

        trace!(
            "VM allocate: {} bytes at 0x{:x} for '{}'",
            aligned_size,
            start,
            name
        );

        Ok(start)
    }

    /// Allocate at specific address (for fixed mappings)
    pub fn allocate_at(
        &self,
        start: u64,
        size: usize,
        flags: RegionFlags,
        name: &str,
    ) -> Result<u64> {
        let aligned_size = Self::align_up(size, PAGE_SIZE);
        let aligned_start = Self::align_down(start, PAGE_SIZE as u64);

        if aligned_start < self.start_limit || aligned_start + aligned_size as u64 > self.end_limit
        {
            return Err(KernelError::invalid_address());
        }

        let mut regions = self.regions.write();

        // Check for conflicts
        if self.has_conflict(&regions, aligned_start, aligned_size) {
            return Err(KernelError::resource_exhausted(
                "Address range already in use",
            ));
        }

        let region = VirtualRegion {
            start: aligned_start,
            size: aligned_size,
            flags,
            backing: RegionBacking::Anonymous,
            name: name.to_string(),
        };

        regions.insert(aligned_start, region);

        trace!(
            "VM allocate_at: {} bytes at 0x{:x} for '{}'",
            aligned_size,
            aligned_start,
            name
        );

        Ok(aligned_start)
    }

    /// Deallocate a region
    pub fn deallocate(&self, start: u64) -> Result<()> {
        let mut regions = self.regions.write();

        if regions.remove(&start).is_none() {
            return Err(KernelError::invalid_address());
        }

        trace!("VM deallocate: 0x{:x}", start);
        Ok(())
    }

    /// Protect a region (change permissions)
    pub fn protect(&self, start: u64, new_flags: RegionFlags) -> Result<()> {
        let mut regions = self.regions.write();

        let region = regions
            .get_mut(&start)
            .ok_or_else(KernelError::invalid_address)?;

        region.flags = new_flags;

        trace!("VM protect: 0x{:x} -> {:?}", start, new_flags);
        Ok(())
    }

    /// Look up region containing address
    pub fn lookup(&self, addr: u64) -> Option<VirtualRegion> {
        let regions = self.regions.read();

        // Find region that contains this address
        regions
            .range(..=addr)
            .next_back()
            .filter(|(_, r)| r.contains(addr))
            .map(|(_, r)| r.clone())
    }

    /// Check if address is valid (within a region)
    pub fn is_valid(&self, addr: u64, size: usize, write: bool) -> bool {
        if let Some(region) = self.lookup(addr) {
            let end = addr + size as u64;
            if end > region.end() {
                return false;
            }
            if !region.flags.readable {
                return false;
            }
            if write && !region.flags.writable {
                return false;
            }
            true
        } else {
            false
        }
    }

    /// Map file into memory
    pub fn mmap_file(
        &self,
        path: &str,
        offset: u64,
        size: usize,
        flags: RegionFlags,
    ) -> Result<u64> {
        let addr = self.allocate(size, flags, &format!("mmap:{}", path))?;

        let mut regions = self.regions.write();
        if let Some(region) = regions.get_mut(&addr) {
            region.backing = RegionBacking::File {
                path: path.to_string(),
                offset,
            };
        }

        Ok(addr)
    }

    /// Map physical memory
    pub fn mmap_physical(&self, paddr: u64, size: usize, flags: RegionFlags) -> Result<u64> {
        let addr = self.allocate(size, flags, "mmap:physical")?;

        let mut regions = self.regions.write();
        if let Some(region) = regions.get_mut(&addr) {
            region.backing = RegionBacking::Physical { paddr };
        }

        Ok(addr)
    }

    /// Unmap region (alias for deallocate)
    pub fn munmap(&self, start: u64) -> Result<()> {
        self.deallocate(start)
    }

    /// Get region statistics
    pub fn stats(&self) -> VmStats {
        let regions = self.regions.read();

        let total_size: usize = regions.values().map(|r| r.size).sum();
        let region_count = regions.len();

        let (readable, writable, executable) =
            regions.values().fold((0, 0, 0), |(r, w, x), region| {
                (
                    r + if region.flags.readable {
                        region.size
                    } else {
                        0
                    },
                    w + if region.flags.writable {
                        region.size
                    } else {
                        0
                    },
                    x + if region.flags.executable {
                        region.size
                    } else {
                        0
                    },
                )
            });

        VmStats {
            total_size,
            region_count,
            readable,
            writable,
            executable,
            address_space_size: (self.end_limit - self.start_limit) as usize,
        }
    }

    /// Print memory map
    pub fn print_map(&self) {
        let regions = self.regions.read();

        debug!("Virtual Memory Map:");
        for (start, region) in regions.iter() {
            let perms = format!(
                "{}{}{}",
                if region.flags.readable { 'r' } else { '-' },
                if region.flags.writable { 'w' } else { '-' },
                if region.flags.executable { 'x' } else { '-' }
            );

            debug!(
                "  0x{:016x}-0x{:016x} {} {}",
                start,
                start + region.size as u64,
                perms,
                region.name
            );
        }
    }

    /// Find free space for allocation
    fn find_free_space(
        &self,
        regions: &BTreeMap<u64, VirtualRegion>,
        size: usize,
        hint: u64,
    ) -> Result<u64> {
        let aligned_hint = Self::align_up(hint as usize, PAGE_SIZE) as u64;

        // Try hint first
        if self.can_allocate_at(regions, aligned_hint, size) {
            return Ok(aligned_hint);
        }

        // Search for free space
        let mut current = self.start_limit;

        for (&start, region) in regions.iter() {
            if current + size as u64 <= start {
                // Found gap
                return Ok(current);
            }
            current = current.max(region.end());
            current = Self::align_up(current as usize, PAGE_SIZE) as u64;
        }

        // Check after last region
        if current + size as u64 <= self.end_limit {
            Ok(current)
        } else {
            Err(KernelError::out_of_memory())
        }
    }

    /// Check if we can allocate at specific address
    fn can_allocate_at(
        &self,
        regions: &BTreeMap<u64, VirtualRegion>,
        start: u64,
        size: usize,
    ) -> bool {
        if start < self.start_limit || start + size as u64 > self.end_limit {
            return false;
        }

        !self.has_conflict(regions, start, size)
    }

    /// Check for conflicts with existing regions
    fn has_conflict(
        &self,
        regions: &BTreeMap<u64, VirtualRegion>,
        start: u64,
        size: usize,
    ) -> bool {
        let end = start + size as u64;

        regions
            .values()
            .any(|region| region.start < end && region.end() > start)
    }

    /// Align up
    const fn align_up(size: usize, align: usize) -> usize {
        (size + align - 1) & !(align - 1)
    }

    /// Align down
    const fn align_down(addr: u64, align: u64) -> u64 {
        addr & !(align - 1)
    }
}

impl Default for VirtualMemory {
    fn default() -> Self {
        Self::new()
    }
}

/// Virtual memory statistics
#[derive(Debug, Clone, Copy)]
pub struct VmStats {
    /// Total size of all regions
    pub total_size: usize,
    /// Number of regions
    pub region_count: usize,
    /// Number of readable regions
    pub readable: usize,
    /// Number of writable regions
    pub writable: usize,
    /// Number of executable regions
    pub executable: usize,
    /// Total address space size
    pub address_space_size: usize,
}

impl VmStats {
    /// Get utilization percentage
    pub fn utilization(&self) -> f64 {
        if self.address_space_size == 0 {
            return 0.0;
        }
        (self.total_size as f64 / self.address_space_size as f64) * 100.0
    }

    /// Format as string
    pub fn format(&self) -> String {
        format!(
            "VM: {} MB / {} GB address space | {} regions | {:.6}% utilized",
            self.total_size / (1024 * 1024),
            self.address_space_size / (1024 * 1024 * 1024),
            self.region_count,
            self.utilization()
        )
    }
}

/// Global kernel VM instance
///
/// Using OnceLock instead of static mut for thread-safe initialization
static KERNEL_VM: OnceLock<VirtualMemory> = OnceLock::new();

/// Initialize virtual memory
pub fn init() -> Result<()> {
    KERNEL_VM
        .set(VirtualMemory::new())
        .map_err(|_| KernelError::internal("Virtual memory already initialized"))?;
    debug!("Virtual memory initialized");
    Ok(())
}

/// Get kernel VM reference
///
/// Returns None if VM not initialized
pub fn kernel_vm() -> Option<&'static VirtualMemory> {
    KERNEL_VM.get()
}

/// Get kernel VM reference (panics if not initialized)
///
/// # Panics
/// Panics if virtual memory is not initialized
pub fn kernel_vm_expect() -> &'static VirtualMemory {
    KERNEL_VM.get().expect("Virtual memory not initialized")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_allocate() {
        let vm = VirtualMemory::new();

        let addr1 = vm
            .allocate(4096, RegionFlags::read_write(), "test1")
            .unwrap();
        assert!(addr1 >= USER_SPACE_START);

        let addr2 = vm
            .allocate(8192, RegionFlags::read_write(), "test2")
            .unwrap();
        assert!(addr2 > addr1);

        // Check alignment
        assert_eq!(addr1 % PAGE_SIZE as u64, 0);
    }

    #[test]
    fn test_vm_lookup() {
        let vm = VirtualMemory::new();

        let addr = vm
            .allocate(4096, RegionFlags::read_write(), "test")
            .unwrap();

        assert!(vm.lookup(addr).is_some());
        assert!(vm.lookup(addr + 2048).is_some()); // Within region
        assert!(vm.lookup(addr + 5000).is_none()); // Outside region
    }

    #[test]
    fn test_vm_protect() {
        let vm = VirtualMemory::new();

        let addr = vm
            .allocate(4096, RegionFlags::read_write(), "test")
            .unwrap();

        // Change to read-only
        vm.protect(addr, RegionFlags::read_only()).unwrap();

        let region = vm.lookup(addr).unwrap();
        assert!(region.flags.readable);
        assert!(!region.flags.writable);
    }

    #[test]
    fn test_region_overlap() {
        let r1 = VirtualRegion {
            start: 0x1000,
            size: 4096,
            flags: RegionFlags::read_write(),
            backing: RegionBacking::Anonymous,
            name: "r1".to_string(),
        };

        let r2 = VirtualRegion {
            start: 0x2000,
            size: 4096,
            flags: RegionFlags::read_write(),
            backing: RegionBacking::Anonymous,
            name: "r2".to_string(),
        };

        let r3 = VirtualRegion {
            start: 0x1800,
            size: 4096,
            flags: RegionFlags::read_write(),
            backing: RegionBacking::Anonymous,
            name: "r3".to_string(),
        };

        assert!(!r1.overlaps(&r2));
        assert!(r1.overlaps(&r3));
        assert!(r3.overlaps(&r1));
    }

    #[test]
    fn test_vm_stats() {
        let vm = VirtualMemory::new();

        vm.allocate(4096, RegionFlags::read_write(), "test1")
            .unwrap();
        vm.allocate(4096, RegionFlags::read_only(), "test2")
            .unwrap();

        let stats = vm.stats();
        assert_eq!(stats.region_count, 2);
        assert!(stats.readable > 0);
        assert!(stats.writable > 0);
    }
}
