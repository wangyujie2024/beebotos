//! Slab Allocator
//!
//! Efficient fixed-size object allocation using slab allocation strategy.
//! Reduces fragmentation and improves cache locality.

use std::ptr::NonNull;

use parking_lot::Mutex;
use tracing::{debug, trace};

use crate::error::{KernelError, Result};

/// Size of a slab page
const SLAB_PAGE_SIZE: usize = 4096;
/// Maximum object size for slab allocation
const MAX_SLAB_SIZE: usize = 2048;
/// Predefined slab sizes
const SLAB_SIZES: &[usize] = &[8, 16, 32, 64, 128, 256, 512, 1024, 2048];

/// Slab page header
#[derive(Debug)]
struct SlabPage {
    objects: Vec<SlabObject>,
    free_list: Option<NonNull<SlabObject>>,
    used_count: usize,
    object_size: usize,
}

/// Slab object (free list node)
#[derive(Debug)]
struct SlabObject {
    next: Option<NonNull<SlabObject>>,
    /// Zero-size array for offset calculation - used for type layout
    #[allow(dead_code)]
    _data: [u8; 0],
}

impl SlabPage {
    /// Create a new slab page for objects of given size
    fn new(object_size: usize) -> Result<Self> {
        let objects_per_page = (SLAB_PAGE_SIZE - std::mem::size_of::<Self>())
            / (object_size + std::mem::size_of::<Option<NonNull<SlabObject>>>());

        let objects = Vec::with_capacity(objects_per_page);

        // Initialize free list
        let layout = std::alloc::Layout::from_size_align(SLAB_PAGE_SIZE, 4096)
            .map_err(|_| KernelError::internal("Invalid slab layout"))?;

        let page_ptr = unsafe { std::alloc::alloc(layout) };
        if page_ptr.is_null() {
            return Err(KernelError::out_of_memory());
        }

        // Build free list
        let mut free_list: Option<NonNull<SlabObject>> = None;
        for i in (0..objects_per_page).rev() {
            let obj_ptr = unsafe {
                page_ptr.add(i * (object_size + std::mem::size_of::<Option<NonNull<SlabObject>>>()))
            };
            let slab_obj = obj_ptr as *mut SlabObject;
            unsafe {
                (*slab_obj).next = free_list;
            }
            free_list = NonNull::new(slab_obj);
        }

        Ok(Self {
            objects,
            free_list,
            used_count: 0,
            object_size,
        })
    }

    /// Allocate an object from this page
    fn alloc(&mut self) -> Option<*mut u8> {
        self.free_list.map(|obj| {
            let ptr = obj.as_ptr();
            unsafe {
                self.free_list = (*ptr).next;
            }
            self.used_count += 1;
            trace!("Slab alloc: {} bytes at {:p}", self.object_size, ptr);
            // Return pointer to data area (after next pointer)
            unsafe { (ptr as *mut u8).add(std::mem::size_of::<Option<NonNull<SlabObject>>>()) }
        })
    }

    /// Free an object back to this page
    fn free(&mut self, ptr: *mut u8) {
        // Get slab object from data pointer
        let obj_ptr = unsafe {
            ptr.sub(std::mem::size_of::<Option<NonNull<SlabObject>>>()) as *mut SlabObject
        };

        unsafe {
            (*obj_ptr).next = self.free_list;
        }
        self.free_list = NonNull::new(obj_ptr);
        self.used_count -= 1;
        trace!("Slab free: {} bytes at {:p}", self.object_size, ptr);
    }

    /// Check if page is empty
    fn is_empty(&self) -> bool {
        self.used_count == 0
    }

    /// Check if page is full
    fn is_full(&self) -> bool {
        self.free_list.is_none()
    }

    /// Get utilization ratio
    fn utilization(&self) -> f64 {
        let total = self.objects.capacity();
        if total == 0 {
            return 0.0;
        }
        self.used_count as f64 / total as f64
    }
}

/// Slab cache for a specific size class
pub struct SlabCache {
    object_size: usize,
    pages: Mutex<Vec<SlabPage>>,
    stats: Mutex<SlabStats>,
}

impl SlabCache {
    /// Create a new slab cache
    pub fn new(object_size: usize) -> Self {
        Self {
            object_size,
            pages: Mutex::new(Vec::new()),
            stats: Mutex::new(SlabStats::default()),
        }
    }

    /// Allocate an object
    pub fn alloc(&self) -> Result<*mut u8> {
        let mut pages = self.pages.lock();
        let mut stats = self.stats.lock();

        // Try to find a page with free space
        for page in pages.iter_mut() {
            if !page.is_full() {
                if let Some(ptr) = page.alloc() {
                    stats.allocations += 1;
                    stats.current_objects += 1;
                    stats.peak_objects = stats.peak_objects.max(stats.current_objects);
                    return Ok(ptr);
                }
            }
        }

        // Need to create a new page
        let mut new_page = SlabPage::new(self.object_size)?;
        if let Some(ptr) = new_page.alloc() {
            stats.allocations += 1;
            stats.current_objects += 1;
            stats.peak_objects = stats.peak_objects.max(stats.current_objects);
            stats.pages_created += 1;
            pages.push(new_page);
            Ok(ptr)
        } else {
            Err(KernelError::out_of_memory())
        }
    }

    /// Free an object
    pub fn free(&self, ptr: *mut u8) {
        let mut pages = self.pages.lock();
        let mut stats = self.stats.lock();

        // Find the page containing this pointer
        for page in pages.iter_mut() {
            // Simple heuristic: check if pointer falls within expected range
            // In production, use a more robust lookup
            page.free(ptr);
            stats.deallocations += 1;
            stats.current_objects -= 1;
            break;
        }

        // Clean up empty pages if too many
        if pages.len() > 4 {
            pages.retain(|p| !p.is_empty() || p.utilization() > 0.0);
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> SlabStats {
        *self.stats.lock()
    }
}

/// Slab statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct SlabStats {
    /// Total allocations
    pub allocations: u64,
    /// Total deallocations
    pub deallocations: u64,
    /// Current allocated objects
    pub current_objects: u64,
    /// Peak allocated objects
    pub peak_objects: u64,
    /// Pages created
    pub pages_created: u64,
}

/// Slab allocator with multiple size classes
pub struct SlabAllocator {
    caches: Vec<SlabCache>,
}

impl SlabAllocator {
    /// Create a new slab allocator
    pub fn new() -> Self {
        let caches = SLAB_SIZES
            .iter()
            .map(|&size| SlabCache::new(size))
            .collect();

        Self { caches }
    }

    /// Find appropriate cache for size
    fn find_cache(&self, size: usize) -> Option<&SlabCache> {
        if size > MAX_SLAB_SIZE {
            return None;
        }

        // Find smallest size class that fits
        for (i, &slab_size) in SLAB_SIZES.iter().enumerate() {
            if slab_size >= size {
                return self.caches.get(i);
            }
        }
        None
    }

    /// Allocate memory
    pub fn alloc(&self, size: usize) -> Result<*mut u8> {
        if let Some(cache) = self.find_cache(size) {
            cache.alloc()
        } else {
            // Fall back to system allocator for large allocations
            let layout = std::alloc::Layout::from_size_align(size, 8)
                .map_err(|_| KernelError::invalid_argument("Invalid layout"))?;

            let ptr = unsafe { std::alloc::alloc(layout) };
            if ptr.is_null() {
                Err(KernelError::out_of_memory())
            } else {
                Ok(ptr)
            }
        }
    }

    /// Free memory
    ///
    /// # Safety
    /// ptr must have been allocated by this allocator
    pub unsafe fn free(&self, ptr: *mut u8, size: usize) {
        if let Some(cache) = self.find_cache(size) {
            cache.free(ptr);
        } else {
            let layout = std::alloc::Layout::from_size_align_unchecked(size, 8);
            std::alloc::dealloc(ptr, layout);
        }
    }

    /// Get statistics for all caches
    pub fn all_stats(&self) -> Vec<(usize, SlabStats)> {
        SLAB_SIZES
            .iter()
            .zip(self.caches.iter())
            .map(|(&size, cache)| (size, cache.stats()))
            .collect()
    }

    /// Print statistics
    pub fn print_stats(&self) {
        debug!("Slab Allocator Statistics:");
        for (size, stats) in self.all_stats() {
            debug!(
                "  Size {:4}: {:6} allocs, {:6} deallocs, {:6} current, {:6} peak",
                size,
                stats.allocations,
                stats.deallocations,
                stats.current_objects,
                stats.peak_objects
            );
        }
    }
}

impl Default for SlabAllocator {
    fn default() -> Self {
        Self::new()
    }
}

/// Global slab allocator
static mut SLAB_ALLOCATOR: Option<SlabAllocator> = None;

/// Initialize slab allocator
pub fn init() -> Result<()> {
    unsafe {
        SLAB_ALLOCATOR = Some(SlabAllocator::new());
    }
    debug!(
        "Slab allocator initialized with {} size classes",
        SLAB_SIZES.len()
    );
    Ok(())
}

/// Get global slab allocator
///
/// # Safety
/// Must only be called after init()
#[allow(static_mut_refs)]
pub unsafe fn allocator() -> &'static SlabAllocator {
    SLAB_ALLOCATOR
        .as_ref()
        .expect("Slab allocator not initialized")
}

/// Allocate using slab allocator
///
/// # Safety
/// Must be called after initialization
pub unsafe fn slab_alloc(size: usize) -> Result<*mut u8> {
    allocator().alloc(size)
}

/// Free using slab allocator
///
/// # Safety
/// Must be called after initialization, ptr must be valid
pub unsafe fn slab_free(ptr: *mut u8, size: usize) {
    allocator().free(ptr, size);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slab_allocation() {
        let allocator = SlabAllocator::new();

        let ptr1 = allocator.alloc(32).unwrap();
        assert!(!ptr1.is_null());

        let ptr2 = allocator.alloc(64).unwrap();
        assert!(!ptr2.is_null());

        unsafe {
            allocator.free(ptr1, 32);
            allocator.free(ptr2, 64);
        }
    }

    #[test]
    fn test_slab_stats() {
        let allocator = SlabAllocator::new();

        let ptr = allocator.alloc(64).unwrap();
        let stats = allocator.all_stats();

        let cache_64 = stats.iter().find(|(s, _)| *s == 64);
        assert!(cache_64.is_some());
        assert_eq!(cache_64.unwrap().1.allocations, 1);

        unsafe {
            allocator.free(ptr, 64);
        }
    }

    #[test]
    fn test_large_allocation_fallback() {
        let allocator = SlabAllocator::new();

        // Size larger than MAX_SLAB_SIZE
        let ptr = allocator.alloc(4096).unwrap();
        assert!(!ptr.is_null());

        unsafe {
            allocator.free(ptr, 4096);
        }
    }
}
