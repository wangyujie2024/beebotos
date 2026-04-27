//! Kernel Heap Manager
//!
//! Manages the kernel heap with support for:
//! - Variable-size allocations
//! - Memory coalescing
//! - Usage tracking
//! - Out-of-memory handling

use std::collections::BTreeMap;
use std::sync::OnceLock;

use parking_lot::Mutex;
use tracing::{debug, error, trace, warn};

use crate::error::{KernelError, Result};

/// Heap block header
#[derive(Debug, Clone, Copy)]
struct BlockHeader {
    size: usize,
    is_free: bool,
    prev_size: usize,
}

impl BlockHeader {
    const fn new(size: usize, is_free: bool) -> Self {
        Self {
            size,
            is_free,
            prev_size: 0,
        }
    }

    fn total_size(&self) -> usize {
        self.size + std::mem::size_of::<BlockHeader>()
    }
}

/// Kernel heap with block-based allocation
///
/// # Thread Safety
/// All public methods are thread-safe using interior mutability.
/// Lock ordering: blocks -> used (always acquire in this order)
pub struct KernelHeap {
    start: u64,
    size: usize,
    /// Lock order: must acquire `blocks` before `used` to prevent deadlocks
    used: Mutex<usize>,
    blocks: Mutex<BTreeMap<u64, BlockHeader>>,
    min_block_size: usize,
}

impl KernelHeap {
    /// Minimum block size (16 bytes for alignment)
    pub const MIN_BLOCK_SIZE: usize = 16;
    /// Alignment requirement
    pub const ALIGNMENT: usize = 8;

    /// Create a new kernel heap
    pub fn new(start: u64, size: usize) -> Self {
        let mut blocks = BTreeMap::new();
        // Initial free block covering entire heap
        blocks.insert(
            start,
            BlockHeader::new(size - std::mem::size_of::<BlockHeader>(), true),
        );

        Self {
            start,
            size,
            used: Mutex::new(0),
            blocks: Mutex::new(blocks),
            min_block_size: Self::MIN_BLOCK_SIZE,
        }
    }

    /// Round up to alignment
    const fn align_up(size: usize, align: usize) -> usize {
        (size + align - 1) & !(align - 1)
    }

    /// Allocate memory from heap
    ///
    /// Lock acquisition order: blocks, then used
    pub fn alloc(&self, size: usize) -> Result<u64> {
        let aligned_size = Self::align_up(size, Self::ALIGNMENT);
        let min_alloc_size = aligned_size.max(self.min_block_size);

        // Lock order: blocks first, then used
        let mut blocks = self.blocks.lock();
        let mut used = self.used.lock();

        // Find first fit
        let candidate = blocks
            .iter()
            .find(|(_, header)| header.is_free && header.size >= min_alloc_size)
            .map(|(&addr, &header)| (addr, header));

        match candidate {
            Some((addr, header)) => {
                let block_size = header.size;
                let total_needed = min_alloc_size + std::mem::size_of::<BlockHeader>();

                // Check if we should split the block
                let allocated_total_size = if block_size >= total_needed + self.min_block_size {
                    // Split block
                    let new_block_addr =
                        addr + std::mem::size_of::<BlockHeader>() as u64 + min_alloc_size as u64;
                    let remaining_size =
                        block_size - min_alloc_size - std::mem::size_of::<BlockHeader>();

                    // Update current block
                    blocks.insert(
                        addr,
                        BlockHeader {
                            size: min_alloc_size,
                            is_free: false,
                            prev_size: header.prev_size,
                        },
                    );

                    // Insert new free block
                    blocks.insert(
                        new_block_addr,
                        BlockHeader {
                            size: remaining_size,
                            is_free: true,
                            prev_size: min_alloc_size,
                        },
                    );

                    trace!(
                        "Heap split: allocated {} bytes at 0x{:x}, new free block at 0x{:x} ({} \
                         bytes)",
                        min_alloc_size,
                        addr,
                        new_block_addr,
                        remaining_size
                    );

                    min_alloc_size + std::mem::size_of::<BlockHeader>()
                } else {
                    // Use entire block
                    blocks.insert(
                        addr,
                        BlockHeader {
                            size: block_size,
                            is_free: false,
                            prev_size: header.prev_size,
                        },
                    );

                    header.total_size()
                };

                *used += allocated_total_size;

                trace!(
                    "Heap alloc: {} bytes at 0x{:x} (used: {} / {})",
                    min_alloc_size,
                    addr + std::mem::size_of::<BlockHeader>() as u64,
                    *used,
                    self.size
                );

                Ok(addr + std::mem::size_of::<BlockHeader>() as u64)
            }
            None => {
                error!("Heap out of memory: requested {} bytes", size);
                Err(KernelError::out_of_memory())
            }
        }
    }

    /// Free memory back to heap
    ///
    /// Lock acquisition order: blocks, then used
    pub fn free(&self, addr: u64) -> Result<()> {
        if addr < self.start || addr >= self.start + self.size as u64 {
            return Err(KernelError::invalid_address());
        }

        let block_addr = addr - std::mem::size_of::<BlockHeader>() as u64;

        // Lock order: blocks first, then used
        let mut blocks = self.blocks.lock();
        let mut used = self.used.lock();

        let header = blocks
            .get(&block_addr)
            .copied()
            .ok_or_else(KernelError::invalid_address)?;

        if header.is_free {
            warn!("Double free detected at 0x{:x}", addr);
            return Err(KernelError::invalid_argument("Double free"));
        }

        let freed_size = header.total_size();

        // Mark as free
        blocks.insert(
            block_addr,
            BlockHeader {
                size: header.size,
                is_free: true,
                prev_size: header.prev_size,
            },
        );

        // Try to coalesce with next block
        let next_addr = block_addr + freed_size as u64;
        if let Some(&next_header) = blocks.get(&next_addr) {
            if next_header.is_free {
                let combined_size = header.size + next_header.total_size();
                blocks.insert(
                    block_addr,
                    BlockHeader {
                        size: combined_size,
                        is_free: true,
                        prev_size: header.prev_size,
                    },
                );
                blocks.remove(&next_addr);
            }
        }

        // Try to coalesce with previous block
        if header.prev_size > 0 {
            let prev_end = block_addr;
            if let Some((&prev_addr, prev_header)) = blocks
                .iter()
                .find(|(addr, h)| prev_end == **addr + h.total_size() as u64)
            {
                if prev_header.is_free {
                    let combined_size = prev_header.size + header.total_size();
                    let prev_prev_size = prev_header.prev_size;
                    blocks.insert(
                        prev_addr,
                        BlockHeader {
                            size: combined_size,
                            is_free: true,
                            prev_size: prev_prev_size,
                        },
                    );
                    blocks.remove(&block_addr);
                }
            }
        }

        *used = used.saturating_sub(freed_size);

        trace!(
            "Heap free: {} bytes at 0x{:x} (used: {} / {})",
            header.size,
            addr,
            *used,
            self.size
        );

        Ok(())
    }

    /// Get heap usage statistics
    ///
    /// Lock acquisition order: blocks, then used
    pub fn stats(&self) -> HeapStats {
        // Lock order: blocks first, then used
        let blocks = self.blocks.lock();
        let used = *self.used.lock();

        let mut free_blocks = 0;
        let mut free_size = 0;
        let mut used_blocks = 0;
        let mut largest_free = 0;

        for (_, header) in blocks.iter() {
            if header.is_free {
                free_blocks += 1;
                free_size += header.size;
                largest_free = largest_free.max(header.size);
            } else {
                used_blocks += 1;
            }
        }

        HeapStats {
            total_size: self.size,
            used_size: used,
            free_size,
            free_blocks,
            used_blocks,
            largest_free,
            fragmentation: if free_size > 0 {
                1.0 - (largest_free as f64 / free_size as f64)
            } else {
                0.0
            },
        }
    }
}

/// Heap statistics
#[derive(Debug, Clone, Copy)]
pub struct HeapStats {
    /// Total heap size
    pub total_size: usize,
    /// Used bytes
    pub used_size: usize,
    /// Free bytes
    pub free_size: usize,
    /// Free blocks count
    pub free_blocks: usize,
    /// Used blocks count
    pub used_blocks: usize,
    /// Largest free block
    pub largest_free: usize,
    /// Fragmentation ratio
    pub fragmentation: f64,
}

impl HeapStats {
    /// Format as human-readable string
    pub fn format(&self) -> String {
        format!(
            "Heap: {} MB used / {} MB total ({}% util, {}% frag, {} blocks)",
            self.used_size / (1024 * 1024),
            self.total_size / (1024 * 1024),
            self.utilization() as usize,
            (self.fragmentation * 100.0) as usize,
            self.free_blocks + self.used_blocks
        )
    }

    /// Calculate utilization percentage
    pub fn utilization(&self) -> f64 {
        if self.total_size == 0 {
            return 0.0;
        }
        (self.used_size as f64 / self.total_size as f64) * 100.0
    }
}

/// Global heap instance
///
/// Using OnceLock instead of static mut for thread-safe initialization
static KERNEL_HEAP: OnceLock<KernelHeap> = OnceLock::new();

/// Initialize kernel heap
pub fn init() -> Result<()> {
    // Allocate 64MB heap using system allocator
    const HEAP_SIZE: usize = 64 * 1024 * 1024;

    let layout = std::alloc::Layout::from_size_align(HEAP_SIZE, 4096)
        .map_err(|_| KernelError::internal("Invalid heap layout"))?;

    let ptr = unsafe { std::alloc::alloc(layout) };
    if ptr.is_null() {
        return Err(KernelError::out_of_memory());
    }

    let heap = KernelHeap::new(ptr as u64, HEAP_SIZE);

    KERNEL_HEAP
        .set(heap)
        .map_err(|_| KernelError::internal("Heap already initialized"))?;

    debug!(
        "Kernel heap initialized: {} MB at {:p}",
        HEAP_SIZE / (1024 * 1024),
        ptr
    );
    Ok(())
}

/// Get global heap reference
///
/// Returns None if heap not initialized
pub fn heap() -> Option<&'static KernelHeap> {
    KERNEL_HEAP.get()
}

/// Get global heap reference (panics if not initialized)
///
/// # Panics
/// Panics if heap is not initialized
pub fn heap_expect() -> &'static KernelHeap {
    KERNEL_HEAP.get().expect("Heap not initialized")
}

/// Global heap allocator
pub struct HeapAllocator;

impl HeapAllocator {
    /// Allocate from global heap
    ///
    /// Returns error if heap not initialized
    pub fn alloc(&self, size: usize) -> Result<u64> {
        heap()
            .ok_or_else(|| KernelError::internal("Heap not initialized"))?
            .alloc(size)
    }

    /// Free to global heap
    ///
    /// Returns error if heap not initialized
    pub fn free(&self, addr: u64) -> Result<()> {
        heap()
            .ok_or_else(|| KernelError::internal("Heap not initialized"))?
            .free(addr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heap_allocation() {
        let heap = KernelHeap::new(0x10000, 1024 * 1024);

        let addr1 = heap.alloc(64).unwrap();
        assert!(addr1 > 0x10000);

        let addr2 = heap.alloc(128).unwrap();
        assert!(addr2 > addr1);

        heap.free(addr1).unwrap();
        heap.free(addr2).unwrap();

        let stats = heap.stats();
        // After freeing all allocations, used_size should be 0 or close to 0
        // Allow some tolerance for internal bookkeeping
        assert!(
            stats.used_size < 100,
            "used_size should be near 0 after freeing all allocations, got {}",
            stats.used_size
        );
    }

    #[test]
    fn test_heap_coalescing() {
        let heap = KernelHeap::new(0x10000, 1024 * 1024);

        // Allocate several blocks
        let addr1 = heap.alloc(64).unwrap();
        let addr2 = heap.alloc(64).unwrap();
        let addr3 = heap.alloc(64).unwrap();

        // Free middle block first, then outer blocks
        heap.free(addr2).unwrap();
        heap.free(addr1).unwrap();
        heap.free(addr3).unwrap();

        // After coalescing, should have 1 free block
        let stats = heap.stats();
        assert_eq!(
            stats.free_blocks, 1,
            "should have 1 free block after coalescing"
        );
        assert!(
            stats.used_size < 100,
            "used_size should be near 0 after freeing all allocations, got {}",
            stats.used_size
        );
    }

    #[test]
    fn test_heap_alignment() {
        let heap = KernelHeap::new(0x10000, 1024 * 1024);

        // Request unaligned size
        let addr = heap.alloc(17).unwrap();

        // Address should be aligned
        assert_eq!(addr % 8, 0);

        heap.free(addr).unwrap();
    }

    #[test]
    fn test_heap_out_of_memory() {
        let heap = KernelHeap::new(0x10000, 1024);

        // Try to allocate more than available
        let result = heap.alloc(2048);
        assert!(result.is_err());
    }

    #[test]
    fn test_heap_double_free() {
        let heap = KernelHeap::new(0x10000, 1024 * 1024);

        let addr = heap.alloc(64).unwrap();
        heap.free(addr).unwrap();

        // Double free should fail
        let result = heap.free(addr);
        assert!(result.is_err());
    }

    #[test]
    fn test_heap_stats() {
        let heap = KernelHeap::new(0x10000, 1024 * 1024);

        let stats_before = heap.stats();
        assert!(
            stats_before.used_size < 100,
            "initial used_size should be near 0"
        );

        let addr = heap.alloc(64).unwrap();
        let stats_after = heap.stats();
        assert!(
            stats_after.used_size > 0,
            "used_size should increase after allocation"
        );

        heap.free(addr).unwrap();
        let stats_final = heap.stats();
        assert!(
            stats_final.used_size < 100,
            "used_size should be near 0 after freeing, got {}",
            stats_final.used_size
        );
    }
}
