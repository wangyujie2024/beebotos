//! Memory Safety Verification
//!
//! Production-ready memory safety checks:
//! - Use-after-free detection
//! - Buffer overflow protection
//! - Memory leak detection
//! - Double-free prevention
//! - Address sanitization helpers

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use tracing::{error, info, trace, warn};

/// Memory block metadata for tracking
#[derive(Debug, Clone)]
pub struct MemoryBlock {
    /// Allocation address
    pub addr: usize,
    /// Size in bytes
    pub size: usize,
    /// Allocation timestamp
    pub allocated_at: std::time::Instant,
    /// Allocation backtrace (for debugging)
    pub backtrace: Option<String>,
    /// Whether the block is currently allocated
    pub is_allocated: bool,
    /// Allocation ID (for tracking)
    pub allocation_id: u64,
}

/// Memory safety tracker
pub struct MemorySafetyTracker {
    /// Currently allocated blocks
    active_allocations: Arc<RwLock<HashMap<usize, MemoryBlock>>>,
    /// Freed blocks (for use-after-free detection)
    freed_blocks: Arc<Mutex<HashSet<usize>>>,
    /// Allocation counter
    allocation_counter: AtomicU64,
    /// Statistics
    stats: Arc<RwLock<SafetyStats>>,
    /// Enable expensive checks
    paranoid_mode: bool,
}

/// Safety statistics
#[derive(Debug, Clone, Default)]
pub struct SafetyStats {
    /// Total allocations tracked
    pub total_allocations: u64,
    /// Total deallocations tracked
    pub total_deallocations: u64,
    /// Double free errors detected
    pub double_frees_detected: u64,
    /// Use after free errors detected
    pub use_after_free_detected: u64,
    /// Invalid free errors detected
    pub invalid_frees_detected: u64,
    /// Potential memory leaks
    pub potential_leaks: u64,
}

impl MemorySafetyTracker {
    /// Create new safety tracker
    pub fn new(paranoid_mode: bool) -> Self {
        Self {
            active_allocations: Arc::new(RwLock::new(HashMap::new())),
            freed_blocks: Arc::new(Mutex::new(HashSet::new())),
            allocation_counter: AtomicU64::new(1),
            stats: Arc::new(RwLock::new(SafetyStats::default())),
            paranoid_mode,
        }
    }

    /// Record a new allocation
    pub fn record_allocation(&self, ptr: *mut u8, size: usize) {
        if ptr.is_null() {
            return;
        }

        let addr = ptr as usize;
        let id = self.allocation_counter.fetch_add(1, Ordering::SeqCst);

        let block = MemoryBlock {
            addr,
            size,
            allocated_at: std::time::Instant::now(),
            backtrace: if self.paranoid_mode {
                Some(std::backtrace::Backtrace::capture().to_string())
            } else {
                None
            },
            is_allocated: true,
            allocation_id: id,
        };

        // Remove from freed set (in case of reuse)
        self.freed_blocks.lock().remove(&addr);

        // Record active allocation
        self.active_allocations.write().insert(addr, block);

        // Update stats
        self.stats.write().total_allocations += 1;

        trace!("Recorded allocation {:p} (id={}) size={}", ptr, id, size);
    }

    /// Record a deallocation
    pub fn record_deallocation(&self, ptr: *mut u8) -> AllocationCheck {
        if ptr.is_null() {
            return AllocationCheck::Invalid("null pointer");
        }

        let addr = ptr as usize;

        // Check for double-free
        if self.freed_blocks.lock().contains(&addr) {
            self.stats.write().double_frees_detected += 1;
            error!("Double-free detected at {:p}", ptr);
            return AllocationCheck::DoubleFree;
        }

        // Check if this is a valid active allocation
        let mut allocations = self.active_allocations.write();

        if let Some(block) = allocations.remove(&addr) {
            // Valid free
            drop(allocations);

            self.freed_blocks.lock().insert(addr);
            self.stats.write().total_deallocations += 1;

            trace!(
                "Recorded deallocation {:p} (id={})",
                ptr,
                block.allocation_id
            );

            AllocationCheck::Valid(block)
        } else {
            // Invalid free (not an active allocation)
            self.stats.write().invalid_frees_detected += 1;
            error!(
                "Invalid free detected at {:p} (not an active allocation)",
                ptr
            );
            AllocationCheck::Invalid("not an active allocation")
        }
    }

    /// Check if pointer is valid for access
    pub fn check_access(&self, ptr: *const u8, size: usize) -> AccessCheck {
        if ptr.is_null() {
            return AccessCheck::Invalid("null pointer");
        }

        let addr = ptr as usize;

        // Check if this was a freed block (use-after-free)
        if self.freed_blocks.lock().contains(&addr) {
            self.stats.write().use_after_free_detected += 1;
            error!("Use-after-free detected at {:p}", ptr);
            return AccessCheck::UseAfterFree;
        }

        // Check if pointer is within a valid allocation
        let allocations = self.active_allocations.read();

        for (block_addr, block) in allocations.iter() {
            if addr >= *block_addr && addr < block_addr + block.size {
                // Check bounds
                let end_addr = addr + size;
                if end_addr > block_addr + block.size {
                    return AccessCheck::BufferOverflow {
                        allocation_size: block.size,
                        access_offset: addr - block_addr,
                        access_size: size,
                    };
                }
                return AccessCheck::Valid(block.clone());
            }
        }

        // Unknown pointer - could be stack, static, or invalid
        AccessCheck::Unknown
    }

    /// Get memory leak report
    pub fn leak_report(&self) -> Vec<MemoryBlock> {
        let allocations = self.active_allocations.read();
        let mut leaks = Vec::new();

        for block in allocations.values() {
            // Consider it a potential leak if allocated for > 5 minutes
            if block.allocated_at.elapsed().as_secs() > 300 {
                leaks.push(block.clone());
            }
        }

        self.stats.write().potential_leaks = leaks.len() as u64;
        leaks
    }

    /// Get statistics
    pub fn stats(&self) -> SafetyStats {
        self.stats.read().clone()
    }

    /// Print leak report
    pub fn print_leak_report(&self) {
        let leaks = self.leak_report();

        if leaks.is_empty() {
            info!("No memory leaks detected");
            return;
        }

        warn!("=== Memory Leak Report ===");
        warn!("Found {} potential leaks:", leaks.len());

        for (i, block) in leaks.iter().enumerate() {
            warn!(
                "  [{}] Address: {:p}, Size: {} bytes, Age: {:?}",
                i + 1,
                block.addr as *const u8,
                block.size,
                block.allocated_at.elapsed()
            );

            if let Some(ref bt) = block.backtrace {
                warn!("      Allocation backtrace:\n{}", bt);
            }
        }
    }

    /// Clear freed block tracking (to prevent memory growth)
    pub fn clear_freed_cache(&self) {
        self.freed_blocks.lock().clear();
    }
}

/// Result of allocation check
#[derive(Debug)]
pub enum AllocationCheck {
    /// Valid allocation
    Valid(MemoryBlock),
    /// Double free detected
    DoubleFree,
    /// Invalid allocation
    Invalid(&'static str),
}

/// Result of access check
#[derive(Debug)]
pub enum AccessCheck {
    /// Valid access
    Valid(MemoryBlock),
    /// Use after free detected
    UseAfterFree,
    /// Buffer overflow detected
    BufferOverflow {
        /// Allocation size
        allocation_size: usize,
        /// Access offset
        access_offset: usize,
        /// Access size
        access_size: usize,
    },
    /// Invalid access
    Invalid(&'static str),
    /// Unknown status
    Unknown,
}

/// Guard for memory operations
pub struct MemoryGuard {
    tracker: Arc<MemorySafetyTracker>,
}

impl MemoryGuard {
    /// Create new memory guard
    pub fn new(tracker: Arc<MemorySafetyTracker>) -> Self {
        Self { tracker }
    }

    /// Safe wrapper for allocation
    pub unsafe fn allocate(&self, layout: std::alloc::Layout) -> *mut u8 {
        let ptr = std::alloc::alloc(layout);
        self.tracker.record_allocation(ptr, layout.size());
        ptr
    }

    /// Safe wrapper for deallocation
    pub unsafe fn deallocate(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        match self.tracker.record_deallocation(ptr) {
            AllocationCheck::Valid(_) => {
                std::alloc::dealloc(ptr, layout);
            }
            AllocationCheck::DoubleFree => {
                panic!("Double-free detected at {:p}", ptr);
            }
            AllocationCheck::Invalid(reason) => {
                panic!("Invalid free at {:p}: {}", ptr, reason);
            }
        }
    }

    /// Check pointer before access
    pub fn check_read(&self, ptr: *const u8, size: usize) {
        match self.tracker.check_access(ptr, size) {
            AccessCheck::Valid(_) | AccessCheck::Unknown => {}
            AccessCheck::UseAfterFree => {
                panic!("Use-after-free detected at {:p}", ptr);
            }
            AccessCheck::BufferOverflow {
                allocation_size,
                access_offset,
                access_size,
            } => {
                panic!(
                    "Buffer overflow: accessing {} bytes at offset {} from allocation of {} bytes",
                    access_size, access_offset, allocation_size
                );
            }
            AccessCheck::Invalid(reason) => {
                panic!("Invalid memory access at {:p}: {}", ptr, reason);
            }
        }
    }
}

/// Canaries for buffer overflow detection
pub struct CanaryGuard {
    value: u64,
}

impl CanaryGuard {
    /// Create new canary with random value
    pub fn new() -> Self {
        Self {
            value: rand::random::<u64>(),
        }
    }

    /// Write canary to memory
    pub unsafe fn write(&self, ptr: *mut u64) {
        ptr.write(self.value);
    }

    /// Verify canary
    pub unsafe fn verify(&self, ptr: *const u64) -> bool {
        ptr.read() == self.value
    }
}

impl Default for CanaryGuard {
    fn default() -> Self {
        Self::new()
    }
}

/// Poison value for freed memory
pub const POISON_BYTE: u8 = 0xDE;

/// Poison memory after free (debug builds)
pub unsafe fn poison_memory(ptr: *mut u8, size: usize) {
    std::ptr::write_bytes(ptr, POISON_BYTE, size);
}

/// Global safety tracker (optional)
static GLOBAL_TRACKER: std::sync::OnceLock<Arc<MemorySafetyTracker>> = std::sync::OnceLock::new();

/// Initialize global safety tracker
pub fn init_global_tracker(paranoid_mode: bool) {
    let _ = GLOBAL_TRACKER.set(Arc::new(MemorySafetyTracker::new(paranoid_mode)));
}

/// Get global tracker
pub fn global_tracker() -> Option<Arc<MemorySafetyTracker>> {
    GLOBAL_TRACKER.get().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocation_tracking() {
        let tracker = MemorySafetyTracker::new(false);

        let ptr = 0x1000 as *mut u8;
        tracker.record_allocation(ptr, 1024);

        assert_eq!(tracker.stats().total_allocations, 1);

        let check = tracker.record_deallocation(ptr);
        assert!(matches!(check, AllocationCheck::Valid(_)));
        assert_eq!(tracker.stats().total_deallocations, 1);
    }

    #[test]
    fn test_double_free_detection() {
        let tracker = MemorySafetyTracker::new(false);

        let ptr = 0x1000 as *mut u8;
        tracker.record_allocation(ptr, 1024);
        tracker.record_deallocation(ptr);

        let check = tracker.record_deallocation(ptr);
        assert!(matches!(check, AllocationCheck::DoubleFree));
        assert_eq!(tracker.stats().double_frees_detected, 1);
    }

    #[test]
    fn test_use_after_free_detection() {
        let tracker = MemorySafetyTracker::new(false);

        let ptr = 0x1000 as *mut u8;
        tracker.record_allocation(ptr, 1024);
        tracker.record_deallocation(ptr);

        let check = tracker.check_access(ptr, 1);
        assert!(matches!(check, AccessCheck::UseAfterFree));
        assert_eq!(tracker.stats().use_after_free_detected, 1);
    }

    #[test]
    fn test_buffer_overflow_detection() {
        let tracker = MemorySafetyTracker::new(false);

        let ptr = 0x1000 as *mut u8;
        tracker.record_allocation(ptr, 1024);

        // Access within bounds
        let check = tracker.check_access(ptr, 512);
        assert!(matches!(check, AccessCheck::Valid(_)));

        // Access beyond bounds
        let check = tracker.check_access(ptr, 2048);
        assert!(matches!(check, AccessCheck::BufferOverflow { .. }));
    }

    #[test]
    fn test_canary_guard() {
        let canary = CanaryGuard::new();

        unsafe {
            let mut value: u64 = 0;
            canary.write(&mut value);
            assert!(canary.verify(&value));

            // Corrupt the value
            value = 0xDEADBEEF;
            assert!(!canary.verify(&value));
        }
    }
}
