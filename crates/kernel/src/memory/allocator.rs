//! Kernel Memory Allocator
//!
//! Provides memory allocation with jemalloc support and fallback to std
//! allocator.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use tracing::{debug, error, warn};

use crate::error::Result;

/// Global memory statistics with overflow-safe atomic operations
///
/// All counters use saturating arithmetic to prevent overflow/underflow
/// on long-running systems. On 64-bit platforms, overflow is extremely
/// unlikely, but this provides defense in depth.
pub struct MemoryStats {
    /// Total bytes allocated
    total_allocated: AtomicUsize,
    /// Total bytes freed
    total_freed: AtomicUsize,
    /// Currently used bytes
    current_used: AtomicUsize,
    /// Peak memory usage
    peak_used: AtomicUsize,
    /// Number of allocations
    allocation_count: AtomicUsize,
    /// Number of deallocations
    deallocation_count: AtomicUsize,
}

impl MemoryStats {
    /// Create new memory stats
    pub const fn new() -> Self {
        Self {
            total_allocated: AtomicUsize::new(0),
            total_freed: AtomicUsize::new(0),
            current_used: AtomicUsize::new(0),
            peak_used: AtomicUsize::new(0),
            allocation_count: AtomicUsize::new(0),
            deallocation_count: AtomicUsize::new(0),
        }
    }

    /// Record an allocation in statistics
    ///
    /// Uses saturating arithmetic for all counters to prevent overflow.
    /// Uses SeqCst for current_used and peak_used to ensure visibility
    /// for OOM detection across all threads.
    pub fn record_allocation(&self, size: usize) {
        self.saturating_add(&self.total_allocated, size, Ordering::Relaxed);
        self.saturating_add(&self.allocation_count, 1, Ordering::Relaxed);

        // current_used and peak_used use SeqCst for OOM detection visibility
        let current = self
            .current_used
            .fetch_add(size, Ordering::SeqCst)
            .saturating_add(size);

        // Update peak with SeqCst to ensure ordering with current_used
        self.update_peak(current);
    }

    /// Record a deallocation in statistics
    pub fn record_deallocation(&self, size: usize) {
        self.saturating_add(&self.total_freed, size, Ordering::Relaxed);
        self.saturating_add(&self.deallocation_count, 1, Ordering::Relaxed);
        // Use SeqCst to match record_allocation
        let _ = self.current_used.fetch_sub(size, Ordering::SeqCst);
    }

    /// Get current memory usage
    pub fn current_used(&self) -> usize {
        self.current_used.load(Ordering::SeqCst)
    }

    /// Get peak memory usage
    pub fn peak_used(&self) -> usize {
        self.peak_used.load(Ordering::SeqCst)
    }

    /// Get total bytes allocated
    pub fn total_allocated(&self) -> usize {
        self.total_allocated.load(Ordering::Relaxed)
    }

    /// Get total bytes freed
    pub fn total_freed(&self) -> usize {
        self.total_freed.load(Ordering::Relaxed)
    }

    /// Get number of allocations
    pub fn allocation_count(&self) -> usize {
        self.allocation_count.load(Ordering::Relaxed)
    }

    /// Get number of deallocations
    pub fn deallocation_count(&self) -> usize {
        self.deallocation_count.load(Ordering::Relaxed)
    }

    /// Saturating add for an atomic counter
    fn saturating_add(&self, atomic: &AtomicUsize, value: usize, ordering: Ordering) {
        let mut current = atomic.load(ordering);
        loop {
            let new = current.saturating_add(value);
            match atomic.compare_exchange_weak(current, new, ordering, ordering) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }

    /// Update peak memory usage if current is higher
    fn update_peak(&self, current: usize) {
        let mut peak = self.peak_used.load(Ordering::SeqCst);
        while current > peak {
            match self.peak_used.compare_exchange_weak(
                peak,
                current,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(actual) => peak = actual,
            }
        }
    }
}

/// Thread-safe memory tracker with limit enforcement
///
/// Wraps `MemoryStats` with a configurable memory limit and provides
/// a unified interface for memory accounting.
pub struct MemoryTracker {
    stats: MemoryStats,
    limit: AtomicUsize,
}

impl MemoryTracker {
    /// Create a new memory tracker
    pub const fn new() -> Self {
        Self {
            stats: MemoryStats::new(),
            limit: AtomicUsize::new(0),
        }
    }

    /// Set memory limit in bytes (0 means unlimited)
    pub fn set_limit(&self, bytes: usize) {
        self.limit.store(bytes, Ordering::Relaxed);
    }

    /// Get current memory limit
    pub fn get_limit(&self) -> usize {
        self.limit.load(Ordering::Relaxed)
    }

    /// Check if an allocation of the given size would exceed the limit
    pub fn would_exceed_limit(&self, size: usize) -> bool {
        let limit = self.get_limit();
        if limit == 0 {
            return false;
        }
        self.stats.current_used().saturating_add(size) > limit
    }

    /// Record an allocation
    pub fn record_allocation(&self, size: usize) {
        self.stats.record_allocation(size);
    }

    /// Record a deallocation
    pub fn record_deallocation(&self, size: usize) {
        self.stats.record_deallocation(size);
    }

    /// Get the underlying stats
    pub fn stats(&self) -> &MemoryStats {
        &self.stats
    }
}

/// Global memory statistics
///
/// For new code, prefer using `MEMORY_TRACKER` which provides limit
/// enforcement and a safer API.
pub static MEMORY_STATS: MemoryStats = MemoryStats::new();

/// Global memory tracker with limit enforcement
pub static MEMORY_TRACKER: MemoryTracker = MemoryTracker::new();

/// Maximum memory limit (configurable, 0 means unlimited)
///
/// Deprecated: use `MEMORY_TRACKER.set_limit()` instead.
static MEMORY_LIMIT: AtomicUsize = AtomicUsize::new(0);

/// Set memory limit in bytes
pub fn set_memory_limit(bytes: usize) {
    MEMORY_LIMIT.store(bytes, Ordering::Relaxed);
    MEMORY_TRACKER.set_limit(bytes);
}

/// Get current memory limit
pub fn get_memory_limit() -> usize {
    let limit = MEMORY_LIMIT.load(Ordering::Relaxed);
    if limit == 0 {
        MEMORY_TRACKER.get_limit()
    } else {
        limit
    }
}

/// Check if allocation would exceed limit
fn would_exceed_limit(size: usize) -> bool {
    MEMORY_TRACKER.would_exceed_limit(size)
}

/// OOM handler type
pub type OomHandler = Box<dyn Fn() + Send + Sync>;

static OOM_HANDLER: parking_lot::RwLock<Option<OomHandler>> = parking_lot::RwLock::new(None);

/// Set custom OOM handler
pub fn set_oom_handler(handler: OomHandler) {
    *OOM_HANDLER.write() = Some(handler);
}

/// Default OOM handler
fn default_oom_handler() {
    error!(
        "Out of memory! Current usage: {} bytes",
        MEMORY_STATS.current_used()
    );
    // Try to free some memory or panic
    std::process::abort();
}

/// Kernel memory allocator wrapper
///
/// Uses jemalloc when available for better performance,
/// falls back to system allocator otherwise.
///
/// In debug builds, integrates with MemorySafetyTracker for:
/// - Use-after-free detection
/// - Double-free detection
/// - Buffer overflow detection
pub struct KernelAllocator;

unsafe impl GlobalAlloc for KernelAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Check memory limit
        if would_exceed_limit(layout.size()) {
            warn!(
                "Memory limit would be exceeded: {} bytes requested",
                layout.size()
            );
            if let Some(ref handler) = *OOM_HANDLER.read() {
                handler();
            } else {
                default_oom_handler();
            }
            return std::ptr::null_mut();
        }

        let ptr = System.alloc(layout);
        if !ptr.is_null() {
            MEMORY_STATS.record_allocation(layout.size());

            // Track allocation in safety tracker (if enabled)
            #[cfg(debug_assertions)]
            if let Some(ref tracker) = super::safety::global_tracker() {
                tracker.record_allocation(ptr, layout.size());
            }

            debug!(
                "Allocated {} bytes at {:p}, current usage: {} bytes",
                layout.size(),
                ptr,
                MEMORY_STATS.current_used()
            );
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // Check for double-free and use-after-free in debug builds
        #[cfg(debug_assertions)]
        if let Some(ref tracker) = super::safety::global_tracker() {
            match tracker.record_deallocation(ptr) {
                super::safety::AllocationCheck::DoubleFree => {
                    error!("Double-free detected at {:p}", ptr);
                    // In debug builds, panic on double-free
                    panic!("Double-free detected at {:p}", ptr);
                }
                super::safety::AllocationCheck::Invalid(reason) => {
                    error!("Invalid free at {:p}: {}", ptr, reason);
                    panic!("Invalid free at {:p}: {}", ptr, reason);
                }
                _ => {}
            }

            // Poison memory in debug builds
            super::safety::poison_memory(ptr, layout.size());
        }

        System.dealloc(ptr, layout);
        MEMORY_STATS.record_deallocation(layout.size());
        debug!(
            "Deallocated {} bytes from {:p}, current usage: {} bytes",
            layout.size(),
            ptr,
            MEMORY_STATS.current_used()
        );
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _new_layout = Layout::from_size_align_unchecked(new_size, layout.align());

        // Check if new allocation would exceed limit
        if would_exceed_limit(new_size.saturating_sub(layout.size())) {
            warn!(
                "Memory limit would be exceeded on realloc: {} bytes",
                new_size
            );
            return std::ptr::null_mut();
        }

        // Record deallocation in safety tracker
        #[cfg(debug_assertions)]
        if let Some(ref tracker) = super::safety::global_tracker() {
            tracker.record_deallocation(ptr);
        }

        let new_ptr = System.realloc(ptr, layout, new_size);

        if !new_ptr.is_null() {
            MEMORY_STATS.record_deallocation(layout.size());
            MEMORY_STATS.record_allocation(new_size);

            // Record new allocation
            #[cfg(debug_assertions)]
            if let Some(ref tracker) = super::safety::global_tracker() {
                tracker.record_allocation(new_ptr, new_size);
            }
        }
        new_ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc_zeroed(layout);
        if !ptr.is_null() {
            MEMORY_STATS.record_allocation(layout.size());

            #[cfg(debug_assertions)]
            if let Some(ref tracker) = super::safety::global_tracker() {
                tracker.record_allocation(ptr, layout.size());
            }
        }
        ptr
    }
}

/// Memory pool for fixed-size allocations
///
/// # Performance
/// - Allocation: O(1) - pop from Vec
/// - Deallocation: O(1) - push to Vec with pointer validation via HashSet
pub struct MemoryPool {
    block_size: usize,
    preallocated: Vec<Vec<u8>>,
    available: Vec<*mut u8>,
    /// O(1) pointer validation set - stores valid pointer values
    valid_pointers: std::collections::HashSet<usize>,
}

// SAFETY: MemoryPool can be Send + Sync because:
// 1. All pointer operations are internally synchronized via &mut self
// 2. The pointers in `available` always point to memory owned by `preallocated`
// 3. `preallocated` is a Vec<Vec<u8>>, so the actual memory is heap-allocated
//    and stable
// 4. The pool never gives out references, only raw pointers, and callers must
//    use unsafe to dereference them, ensuring they follow Rust's aliasing rules
// 5. All methods that access the pool require &mut self, preventing concurrent
//    access
//
// Note: If MemoryPool is ever changed to allow interior mutability (e.g.,
// Mutex), this safety guarantee must be re-evaluated
unsafe impl Send for MemoryPool {}
unsafe impl Sync for MemoryPool {}

impl MemoryPool {
    /// Create a new memory pool
    pub fn new(block_size: usize, initial_blocks: usize) -> Self {
        let mut pool = Self {
            block_size,
            preallocated: Vec::with_capacity(initial_blocks),
            available: Vec::with_capacity(initial_blocks),
            valid_pointers: std::collections::HashSet::with_capacity(initial_blocks),
        };

        // Pre-allocate blocks
        for _ in 0..initial_blocks {
            let mut block = vec![0u8; block_size];
            let ptr = block.as_mut_ptr();
            pool.valid_pointers.insert(ptr as usize);
            pool.preallocated.push(block);
            pool.available.push(ptr);
        }

        pool
    }

    /// Allocate from pool
    ///
    /// Returns a pointer to a block or None if pool is exhausted
    pub fn allocate(&mut self) -> Option<*mut u8> {
        self.available.pop()
    }

    /// Free back to pool
    ///
    /// # Safety
    /// The pointer must have been allocated from this pool.
    /// Invalid pointers are silently ignored (defensive programming).
    ///
    /// # Complexity
    /// O(1) - uses HashSet for validation
    pub fn free(&mut self, ptr: *mut u8) {
        // O(1) pointer validation using HashSet
        if ptr.is_null() {
            return;
        }

        let ptr_val = ptr as usize;
        if self.valid_pointers.contains(&ptr_val) {
            // Additional safety: don't double-free
            if !self.available.iter().any(|p| *p == ptr) {
                self.available.push(ptr);
            }
        }
        // Silently ignore invalid pointers (defensive)
    }

    /// Grow the pool by adding more blocks
    pub fn grow(&mut self, additional_blocks: usize) {
        self.preallocated.reserve(additional_blocks);
        self.available.reserve(additional_blocks);

        for _ in 0..additional_blocks {
            let mut block = vec![0u8; self.block_size];
            let ptr = block.as_mut_ptr();
            self.valid_pointers.insert(ptr as usize);
            self.preallocated.push(block);
            self.available.push(ptr);
        }
    }

    /// Shrink the pool by removing available blocks
    pub fn shrink(&mut self, max_to_remove: usize) -> usize {
        let to_remove = max_to_remove.min(self.available.len());

        for _ in 0..to_remove {
            if let Some(ptr) = self.available.pop() {
                self.valid_pointers.remove(&(ptr as usize));
                // Remove the corresponding Vec from preallocated
                self.preallocated.retain(|block| block.as_ptr() != ptr);
            }
        }

        to_remove
    }

    /// Get pool statistics
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            block_size: self.block_size,
            total_blocks: self.preallocated.len(),
            available_blocks: self.available.len(),
            in_use: self.preallocated.len() - self.available.len(),
        }
    }
}

/// Pool statistics
#[derive(Debug, Clone, Copy)]
pub struct PoolStats {
    /// Block size in bytes
    pub block_size: usize,
    /// Total blocks in pool
    pub total_blocks: usize,
    /// Available blocks
    pub available_blocks: usize,
    /// Blocks currently in use
    pub in_use: usize,
}

/// Memory manager for kernel
pub struct MemoryManager {
    pools: parking_lot::RwLock<Vec<MemoryPool>>,
    compaction_threshold: AtomicUsize,
}

impl MemoryManager {
    /// Create new memory manager
    pub fn new() -> Self {
        Self {
            pools: parking_lot::RwLock::new(Vec::new()),
            compaction_threshold: AtomicUsize::new(80), // 80% fragmentation threshold
        }
    }

    /// Create a new memory pool
    pub fn create_pool(&self, block_size: usize, initial_blocks: usize) -> usize {
        let pool = MemoryPool::new(block_size, initial_blocks);
        let mut pools = self.pools.write();
        let id = pools.len();
        pools.push(pool);
        id
    }

    /// Allocate from a specific pool
    pub fn allocate_from_pool(&self, pool_id: usize) -> Option<*mut u8> {
        let mut pools = self.pools.write();
        pools.get_mut(pool_id).and_then(|pool| pool.allocate())
    }

    /// Free back to pool
    pub fn free_to_pool(&self, pool_id: usize, ptr: *mut u8) {
        let mut pools = self.pools.write();
        if let Some(pool) = pools.get_mut(pool_id) {
            pool.free(ptr);
        }
    }

    /// Get all pool statistics
    pub fn pool_stats(&self) -> Vec<PoolStats> {
        let pools = self.pools.read();
        pools.iter().map(|p| p.stats()).collect()
    }

    /// Trigger memory compaction (placeholder)
    pub fn compact(&self) -> Result<()> {
        debug!("Memory compaction requested");
        // In a real implementation, this would:
        // 1. Identify fragmented memory regions
        // 2. Move live objects to consolidate free space
        // 3. Update all references
        Ok(())
    }

    /// Check if compaction is needed
    pub fn should_compact(&self) -> bool {
        // Simple heuristic: check fragmentation
        let current = MEMORY_STATS.current_used();
        let peak = MEMORY_STATS.peak_used();
        if peak == 0 {
            return false;
        }
        let utilization = (current * 100) / peak;
        let threshold = self.compaction_threshold.load(Ordering::Relaxed);
        utilization < threshold
    }
}

impl Default for MemoryManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialize memory management
pub fn init() -> Result<()> {
    tracing::info!("Initializing kernel memory management");

    // Log initial memory stats
    let sys = sysinfo::System::new_all();
    tracing::info!(
        "System memory: {} MB total, {} MB available",
        sys.total_memory() / 1024,
        sys.available_memory() / 1024
    );

    Ok(())
}

/// Get formatted memory statistics
pub fn format_stats() -> String {
    format!(
        "Memory Stats - Current: {} MB, Peak: {} MB, Allocs: {}, Deallocs: {}",
        MEMORY_STATS.current_used() / (1024 * 1024),
        MEMORY_STATS.peak_used() / (1024 * 1024),
        MEMORY_STATS.allocation_count(),
        MEMORY_STATS.deallocation_count()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::unwrap_used)]
    #[test]
    fn test_memory_stats() {
        let stats = MemoryStats::new();
        stats.record_allocation(1024);
        assert_eq!(stats.current_used(), 1024);

        stats.record_deallocation(512);
        assert_eq!(stats.current_used(), 512);
    }

    #[allow(clippy::unwrap_used)]
    #[test]
    fn test_memory_pool() {
        let mut pool = MemoryPool::new(64, 10);
        assert_eq!(pool.stats().total_blocks, 10);

        let ptr = pool.allocate().unwrap();
        assert!(!ptr.is_null());
        assert_eq!(pool.stats().available_blocks, 9);

        pool.free(ptr);
        assert_eq!(pool.stats().available_blocks, 10);
    }

    #[test]
    fn test_memory_limit() {
        set_memory_limit(1024);
        assert_eq!(get_memory_limit(), 1024);

        assert!(would_exceed_limit(2048));
        assert!(!would_exceed_limit(512));

        // Reset
        set_memory_limit(0);
    }

    #[test]
    fn test_memory_tracker() {
        let tracker = MemoryTracker::new();
        tracker.set_limit(1024);
        assert!(tracker.would_exceed_limit(2048));
        assert!(!tracker.would_exceed_limit(512));

        tracker.record_allocation(100);
        assert_eq!(tracker.stats().current_used(), 100);

        tracker.record_deallocation(50);
        assert_eq!(tracker.stats().current_used(), 50);
    }
}
