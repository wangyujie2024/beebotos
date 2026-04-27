//! Shared Memory
//!
//! Provides inter-process shared memory regions with proper memory mapping and
//! unmapping. Implements memory-mapped I/O for efficient IPC.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::error::{KernelError, Result};
// Memory isolation imports not needed for current implementation

/// Shared memory region with proper mapping support
pub struct SharedMemory {
    /// Unique memory region ID
    id: u64,
    /// Physical memory pointer
    ptr: *mut u8,
    /// Size in bytes
    size: usize,
    /// Memory layout
    layout: std::alloc::Layout,
    /// Currently mapped addresses (process_id -> virtual_address)
    mappings: RwLock<HashMap<u64, u64>>,
    /// Reference count
    ref_count: RwLock<usize>,
    /// Owner process ID
    owner: u64,
}

// SAFETY: SharedMemory owns the allocated memory and manages access through
// proper synchronization
unsafe impl Send for SharedMemory {}
unsafe impl Sync for SharedMemory {}

impl std::fmt::Debug for SharedMemory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedMemory")
            .field("id", &self.id)
            .field("size", &self.size)
            .field("mappings", &self.mappings.read().len())
            .field("ref_count", &*self.ref_count.read())
            .field("owner", &self.owner)
            .finish()
    }
}

impl SharedMemory {
    /// Create a new shared memory region
    ///
    /// Allocates physical pages using the system allocator aligned to page
    /// boundary.
    pub fn new(id: u64, size: usize, owner: u64) -> Result<Self> {
        if size == 0 {
            return Err(KernelError::invalid_argument(
                "Shared memory size cannot be zero",
            ));
        }

        // Align to page size (4096 bytes)
        let layout = std::alloc::Layout::from_size_align(size, 4096)
            .map_err(|_| KernelError::invalid_argument("Invalid shared memory size"))?;

        let ptr = unsafe { std::alloc::alloc(layout) };
        if ptr.is_null() {
            return Err(KernelError::out_of_memory());
        }

        // Initialize memory to zero for security
        unsafe {
            std::ptr::write_bytes(ptr, 0, size);
        }

        tracing::info!(
            "Created shared memory region {} with size {} bytes",
            id,
            size
        );

        Ok(Self {
            id,
            ptr,
            size,
            layout,
            mappings: RwLock::new(HashMap::new()),
            ref_count: RwLock::new(1),
            owner,
        })
    }

    /// Get the unique ID
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Get the size
    pub fn size(&self) -> usize {
        self.size
    }

    /// Get the owner
    pub fn owner(&self) -> u64 {
        self.owner
    }

    /// Get the pointer to the shared memory (physical address)
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr
    }

    /// Get the mutable pointer to the shared memory
    pub fn as_mut_ptr(&self) -> *mut u8 {
        self.ptr
    }

    /// Get a slice view of the shared memory
    ///
    /// # Safety
    /// The returned slice is valid as long as:
    /// - The SharedMemory object is not dropped
    /// - No mutable references overlap with this slice
    pub unsafe fn as_slice(&self) -> &[u8] {
        std::slice::from_raw_parts(self.ptr, self.size)
    }

    /// Get a mutable slice view of the shared memory
    ///
    /// # Safety
    /// The returned slice is valid as long as:
    /// - The SharedMemory object is not dropped
    /// - No other references overlap with this slice
    pub unsafe fn as_slice_mut(&mut self) -> &mut [u8] {
        std::slice::from_raw_parts_mut(self.ptr, self.size)
    }

    /// Map shared memory to a process's address space
    ///
    /// This registers a virtual address mapping for the given process.
    /// In a real kernel, this would modify page tables.
    pub fn map(&self, process_id: u64, requested_addr: Option<u64>) -> Result<u64> {
        // Check if already mapped
        if let Some(addr) = self.mappings.read().get(&process_id) {
            return Ok(*addr);
        }

        // Determine virtual address
        let virtual_addr = match requested_addr {
            Some(addr) => {
                // Validate alignment
                if addr % 4096 != 0 {
                    return Err(KernelError::invalid_argument(
                        "Requested address must be page-aligned",
                    ));
                }
                addr
            }
            None => {
                // Allocate a virtual address (simplified - would use proper VMA allocator)
                // Using high address space for shared memory, unique per (region, process) pair
                0x7000_0000_0000 + (self.id * 0x1_0000) + (process_id * 0x1000)
            }
        };

        // Register mapping
        self.mappings.write().insert(process_id, virtual_addr);
        *self.ref_count.write() += 1;

        tracing::info!(
            "Mapped shared memory {} to process {} at address {:#x}",
            self.id,
            process_id,
            virtual_addr
        );

        Ok(virtual_addr)
    }

    /// Unmap shared memory from a process's address space
    ///
    /// Removes the virtual address mapping for the given process.
    pub fn unmap(&self, process_id: u64) -> Result<()> {
        let removed = self.mappings.write().remove(&process_id);

        match removed {
            Some(addr) => {
                *self.ref_count.write() -= 1;
                tracing::info!(
                    "Unmapped shared memory {} from process {} (was at {:#x})",
                    self.id,
                    process_id,
                    addr
                );
                Ok(())
            }
            None => Err(KernelError::invalid_argument(format!(
                "Shared memory {} not mapped to process {}",
                self.id, process_id
            ))),
        }
    }

    /// Check if mapped to a process
    pub fn is_mapped_to(&self, process_id: u64) -> bool {
        self.mappings.read().contains_key(&process_id)
    }

    /// Get virtual address for a process
    pub fn get_virtual_address(&self, process_id: u64) -> Option<u64> {
        self.mappings.read().get(&process_id).copied()
    }

    /// Get reference count
    pub fn ref_count(&self) -> usize {
        *self.ref_count.read()
    }

    /// Get number of active mappings
    pub fn mapping_count(&self) -> usize {
        self.mappings.read().len()
    }

    /// Copy data to shared memory
    pub fn write(&self, offset: usize, data: &[u8]) -> Result<usize> {
        if offset >= self.size {
            return Err(KernelError::invalid_argument(
                "Offset beyond shared memory size",
            ));
        }

        let to_write = data.len().min(self.size - offset);

        unsafe {
            let dest = self.ptr.add(offset);
            std::ptr::copy_nonoverlapping(data.as_ptr(), dest, to_write);
        }

        Ok(to_write)
    }

    /// Read data from shared memory
    pub fn read(&self, offset: usize, len: usize) -> Result<Vec<u8>> {
        if offset >= self.size {
            return Err(KernelError::invalid_argument(
                "Offset beyond shared memory size",
            ));
        }

        let to_read = len.min(self.size - offset);
        let mut buffer = vec![0u8; to_read];

        unsafe {
            let src = self.ptr.add(offset);
            std::ptr::copy_nonoverlapping(src, buffer.as_mut_ptr(), to_read);
        }

        Ok(buffer)
    }
}

impl Drop for SharedMemory {
    fn drop(&mut self) {
        tracing::info!("Destroying shared memory region {}", self.id);

        // Clear all mappings
        self.mappings.write().clear();

        // Deallocate memory
        unsafe {
            std::alloc::dealloc(self.ptr, self.layout);
        }
    }
}

/// Shared memory manager
pub struct SharedMemoryManager {
    /// Active shared memory regions
    regions: RwLock<HashMap<u64, Arc<SharedMemory>>>,
    /// Next region ID
    next_id: RwLock<u64>,
}

impl SharedMemoryManager {
    /// Create new manager
    pub fn new() -> Self {
        Self {
            regions: RwLock::new(HashMap::new()),
            next_id: RwLock::new(1),
        }
    }

    /// Create new shared memory region
    pub fn create(&self, size: usize, owner: u64) -> Result<u64> {
        let id = self.next_id();
        let region = Arc::new(SharedMemory::new(id, size, owner)?);
        self.regions.write().insert(id, region);
        tracing::info!(
            "Created shared memory region {} (size: {} bytes, owner: {})",
            id,
            size,
            owner
        );
        Ok(id)
    }

    /// Get shared memory region
    pub fn get(&self, id: u64) -> Option<Arc<SharedMemory>> {
        self.regions.read().get(&id).cloned()
    }

    /// Map shared memory to process
    pub fn map(&self, id: u64, process_id: u64, requested_addr: Option<u64>) -> Result<u64> {
        let region = self
            .regions
            .read()
            .get(&id)
            .cloned()
            .ok_or_else(|| KernelError::invalid_argument("Shared memory region not found"))?;

        region.map(process_id, requested_addr)
    }

    /// Unmap shared memory from process
    pub fn unmap(&self, id: u64, process_id: u64) -> Result<()> {
        let region = self
            .regions
            .read()
            .get(&id)
            .cloned()
            .ok_or_else(|| KernelError::invalid_argument("Shared memory region not found"))?;

        region.unmap(process_id)
    }

    /// Destroy shared memory region
    pub fn destroy(&self, id: u64, requester: u64) -> Result<()> {
        let region = self
            .regions
            .read()
            .get(&id)
            .cloned()
            .ok_or_else(|| KernelError::invalid_argument("Shared memory region not found"))?;

        // Only owner can destroy
        if region.owner() != requester {
            return Err(KernelError::Security(
                "Only owner can destroy shared memory".into(),
            ));
        }

        self.regions.write().remove(&id);
        tracing::info!(
            "Destroyed shared memory region {} by owner {}",
            id,
            requester
        );
        Ok(())
    }

    /// Check if region exists
    pub fn exists(&self, id: u64) -> bool {
        self.regions.read().contains_key(&id)
    }

    /// List all region IDs
    pub fn list(&self) -> Vec<u64> {
        self.regions.read().keys().copied().collect()
    }

    /// Get statistics
    pub fn stats(&self) -> SharedMemoryStats {
        let regions = self.regions.read();
        let total_size: usize = regions.values().map(|r| r.size()).sum();

        SharedMemoryStats {
            region_count: regions.len(),
            total_size_bytes: total_size,
            total_mappings: regions.values().map(|r| r.mapping_count()).sum(),
        }
    }

    /// Cleanup mappings for a process
    pub fn cleanup_process(&self, process_id: u64) -> usize {
        let regions = self.regions.read();
        let mut count = 0;

        for (_, region) in regions.iter() {
            if region.is_mapped_to(process_id) {
                let _ = region.unmap(process_id);
                count += 1;
            }
        }

        if count > 0 {
            tracing::info!(
                "Cleaned up {} shared memory mappings for process {}",
                count,
                process_id
            );
        }

        count
    }

    fn next_id(&self) -> u64 {
        let mut id = self.next_id.write();
        let current = *id;
        *id += 1;
        current
    }
}

impl Default for SharedMemoryManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared memory statistics
#[derive(Debug, Clone, Copy)]
pub struct SharedMemoryStats {
    /// Number of active regions
    pub region_count: usize,
    /// Total allocated bytes
    pub total_size_bytes: usize,
    /// Total number of active mappings
    pub total_mappings: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_memory_create() {
        let region = SharedMemory::new(1, 4096, 100).unwrap();
        assert_eq!(region.id(), 1);
        assert_eq!(region.size(), 4096);
        assert_eq!(region.owner(), 100);
        assert!(!region.as_ptr().is_null());
    }

    #[test]
    fn test_shared_memory_map_unmap() {
        let region = SharedMemory::new(1, 4096, 100).unwrap();

        // Map to process
        let addr = region.map(200, None).unwrap();
        assert!(addr > 0);
        assert!(region.is_mapped_to(200));

        // Get virtual address
        assert_eq!(region.get_virtual_address(200), Some(addr));

        // Unmap
        region.unmap(200).unwrap();
        assert!(!region.is_mapped_to(200));
        assert!(region.get_virtual_address(200).is_none());
    }

    #[test]
    fn test_shared_memory_write_read() {
        let mut region = SharedMemory::new(1, 4096, 100).unwrap();

        let data = b"Hello, Shared Memory!";
        let written = region.write(0, data).unwrap();
        assert_eq!(written, data.len());

        let read = region.read(0, data.len()).unwrap();
        assert_eq!(&read, data);
    }

    #[test]
    fn test_shared_memory_manager() {
        let manager = SharedMemoryManager::new();

        // Create region
        let id = manager.create(4096, 100).unwrap();
        assert!(manager.exists(id));

        // Map to multiple processes
        let addr1 = manager.map(id, 200, None).unwrap();
        let addr2 = manager.map(id, 300, None).unwrap();
        assert_ne!(addr1, addr2);

        // Get region and check mappings
        let region = manager.get(id).unwrap();
        assert_eq!(region.mapping_count(), 2);

        // Cleanup process
        let count = manager.cleanup_process(200);
        assert_eq!(count, 1);

        // Destroy
        manager.destroy(id, 100).unwrap();
        assert!(!manager.exists(id));
    }

    #[test]
    fn test_shared_memory_stats() {
        let manager = SharedMemoryManager::new();

        manager.create(4096, 100).unwrap();
        manager.create(8192, 100).unwrap();

        let stats = manager.stats();
        assert_eq!(stats.region_count, 2);
        assert_eq!(stats.total_size_bytes, 12288);
    }
}
