//! WASM Memory Management
//!
//! Handles WASM linear memory:
//! - Page allocation
//! - Memory limits
//! - Shared memory support
//! - Memory mapping

use tracing::trace;

use crate::error::{KernelError, Result};

/// WASM page size (64KB)
pub const PAGE_SIZE: usize = 64 * 1024;

/// Maximum memory pages (4GB / 64KB = 65536 pages)
pub const MAX_PAGES: u32 = 65536;

/// Default initial memory pages (1MB)
pub const DEFAULT_INITIAL_PAGES: u32 = 16;

/// Default maximum memory pages (128MB)
pub const DEFAULT_MAX_PAGES: u32 = 2048;

/// Memory configuration
#[derive(Debug, Clone, Copy)]
pub struct MemoryConfig {
    /// Initial memory pages
    pub initial_pages: u32,
    /// Maximum memory pages (None = unlimited)
    pub max_pages: Option<u32>,
    /// Enable shared memory
    pub shared: bool,
    /// Enable 64-bit memory (memory64 proposal)
    pub memory64: bool,
}

impl MemoryConfig {
    /// Create new memory configuration
    pub fn new(initial_pages: u32, max_pages: Option<u32>) -> Self {
        Self {
            initial_pages,
            max_pages,
            shared: false,
            memory64: false,
        }
    }

    /// Conservative configuration for untrusted code
    pub fn conservative() -> Self {
        Self {
            initial_pages: 1,
            max_pages: Some(256), // 16MB max
            shared: false,
            memory64: false,
        }
    }

    /// Standard configuration
    pub fn standard() -> Self {
        Self {
            initial_pages: DEFAULT_INITIAL_PAGES,
            max_pages: Some(DEFAULT_MAX_PAGES),
            shared: false,
            memory64: false,
        }
    }

    /// Relaxed configuration for trusted code
    pub fn relaxed() -> Self {
        Self {
            initial_pages: DEFAULT_INITIAL_PAGES,
            max_pages: Some(MAX_PAGES),
            shared: false,
            memory64: false,
        }
    }

    /// Get initial size in bytes
    pub fn initial_size(&self) -> usize {
        self.initial_pages as usize * PAGE_SIZE
    }

    /// Get maximum size in bytes
    pub fn max_size(&self) -> Option<usize> {
        self.max_pages.map(|p| p as usize * PAGE_SIZE)
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        if self.initial_pages > MAX_PAGES {
            return Err(KernelError::invalid_argument(format!(
                "Initial pages {} exceeds maximum {}",
                self.initial_pages, MAX_PAGES
            )));
        }

        if let Some(max) = self.max_pages {
            if max > MAX_PAGES {
                return Err(KernelError::invalid_argument(format!(
                    "Max pages {} exceeds maximum {}",
                    max, MAX_PAGES
                )));
            }
            if max < self.initial_pages {
                return Err(KernelError::invalid_argument(
                    "Max pages cannot be less than initial pages",
                ));
            }
        }

        Ok(())
    }
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self::standard()
    }
}

/// Memory statistics
#[derive(Debug, Clone, Copy)]
pub struct MemoryStats {
    /// Current pages allocated
    pub current_pages: u32,
    /// Current size in bytes
    pub current_size: usize,
    /// Maximum pages allowed
    pub max_pages: Option<u32>,
    /// Maximum size allowed
    pub max_size: Option<usize>,
    /// Number of grows
    pub grow_count: u64,
    /// Number of shrinks
    pub shrink_count: u64,
}

impl MemoryStats {
    /// Format as human-readable string
    pub fn format(&self) -> String {
        let current_mb = self.current_size / (1024 * 1024);
        let max_str = self
            .max_size
            .map(|s| format!("{} MB", s / (1024 * 1024)))
            .unwrap_or_else(|| "unlimited".to_string());

        format!(
            "Memory: {} pages ({} MB) / {} (max)",
            self.current_pages, current_mb, max_str
        )
    }

    /// Calculate utilization
    pub fn utilization(&self) -> f64 {
        match self.max_pages {
            Some(max) if max > 0 => self.current_pages as f64 / max as f64,
            _ => 0.0,
        }
    }

    /// Check if memory is near limit
    pub fn is_near_limit(&self, threshold: f64) -> bool {
        self.utilization() >= threshold
    }
}

/// Memory guard for bounds checking
pub struct MemoryGuard {
    base: *mut u8,
    size: usize,
}

impl MemoryGuard {
    /// Create new memory guard
    ///
    /// # Safety
    /// Base pointer must be valid for the given size
    pub unsafe fn new(base: *mut u8, size: usize) -> Self {
        Self { base, size }
    }

    /// Check if range is valid
    pub fn check_range(&self, offset: usize, len: usize) -> bool {
        offset.saturating_add(len) <= self.size
    }

    /// Get pointer to offset
    ///
    /// # Safety
    /// Caller must ensure range is valid
    pub unsafe fn ptr(&self, offset: usize) -> *mut u8 {
        self.base.add(offset)
    }

    /// Read bytes from memory
    pub fn read(&self, offset: usize, len: usize) -> Option<Vec<u8>> {
        if !self.check_range(offset, len) {
            return None;
        }

        let mut buffer = vec![0u8; len];
        unsafe {
            std::ptr::copy_nonoverlapping(self.ptr(offset), buffer.as_mut_ptr(), len);
        }
        Some(buffer)
    }

    /// Write bytes to memory
    pub fn write(&self, offset: usize, data: &[u8]) -> Option<()> {
        if !self.check_range(offset, data.len()) {
            return None;
        }

        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), self.ptr(offset), data.len());
        }
        Some(())
    }
}

// SAFETY: MemoryGuard doesn't implement Send/Sync by default because it
// contains a raw pointer, but it's safe to send between threads if the
// underlying memory is properly synchronized (which it is in wasmtime).
unsafe impl Send for MemoryGuard {}
unsafe impl Sync for MemoryGuard {}

/// Memory manager for multiple WASM instances
pub struct MemoryManager {
    total_limit: usize,
    used: std::sync::atomic::AtomicUsize,
}

impl MemoryManager {
    /// Create new memory manager
    pub fn new(total_limit: usize) -> Self {
        Self {
            total_limit,
            used: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Allocate memory pages
    pub fn allocate_pages(&self, pages: u32) -> Result<()> {
        let bytes = pages as usize * PAGE_SIZE;
        let current = self
            .used
            .fetch_add(bytes, std::sync::atomic::Ordering::SeqCst);

        if current + bytes > self.total_limit {
            // Rollback
            self.used
                .fetch_sub(bytes, std::sync::atomic::Ordering::SeqCst);
            return Err(KernelError::out_of_memory());
        }

        trace!(
            "Allocated {} pages, total used: {} MB",
            pages,
            (current + bytes) / (1024 * 1024)
        );
        Ok(())
    }

    /// Free memory pages
    pub fn free_pages(&self, pages: u32) {
        let bytes = pages as usize * PAGE_SIZE;
        self.used
            .fetch_sub(bytes, std::sync::atomic::Ordering::SeqCst);
        trace!("Freed {} pages", pages);
    }

    /// Get current usage
    pub fn used(&self) -> usize {
        self.used.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get available memory
    pub fn available(&self) -> usize {
        self.total_limit.saturating_sub(self.used())
    }

    /// Get utilization ratio
    pub fn utilization(&self) -> f64 {
        if self.total_limit == 0 {
            return 0.0;
        }
        self.used() as f64 / self.total_limit as f64
    }
}

/// Page allocation strategy
#[derive(Debug, Clone, Copy)]
pub enum PageStrategy {
    /// Allocate pages on demand
    OnDemand,
    /// Preallocate all pages
    Preallocate,
    /// Use memory mapping
    Mmap,
}

/// Memory allocation request
#[derive(Debug, Clone)]
pub struct AllocationRequest {
    /// Initial pages to allocate
    pub initial_pages: u32,
    /// Maximum pages allowed
    pub max_pages: Option<u32>,
    /// Page allocation strategy
    pub strategy: PageStrategy,
}

/// Convert pages to human-readable string
pub fn format_pages(pages: u32) -> String {
    let bytes = pages as usize * PAGE_SIZE;
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{} MB", bytes / (1024 * 1024))
    } else if bytes >= 1024 {
        format!("{} KB", bytes / 1024)
    } else {
        format!("{} bytes", bytes)
    }
}

/// Calculate required pages for size
pub fn pages_for_size(size: usize) -> u32 {
    ((size + PAGE_SIZE - 1) / PAGE_SIZE) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_config() {
        let config = MemoryConfig::standard();
        assert_eq!(
            config.initial_size(),
            DEFAULT_INITIAL_PAGES as usize * PAGE_SIZE
        );

        assert!(config.validate().is_ok());

        let bad_config = MemoryConfig::new(MAX_PAGES + 1, None);
        assert!(bad_config.validate().is_err());
    }

    #[test]
    fn test_format_pages() {
        assert_eq!(format_pages(1), "64 KB");
        assert_eq!(format_pages(16), "1 MB"); // 16 * 64KB = 1024KB = 1MB
        assert_eq!(format_pages(256), "16 MB");
    }

    #[test]
    fn test_pages_for_size() {
        assert_eq!(pages_for_size(0), 0);
        assert_eq!(pages_for_size(1), 1);
        assert_eq!(pages_for_size(PAGE_SIZE), 1);
        assert_eq!(pages_for_size(PAGE_SIZE + 1), 2);
    }

    #[test]
    fn test_memory_manager() {
        let manager = MemoryManager::new(1024 * 1024 * 1024); // 1GB

        assert!(manager.allocate_pages(16).is_ok());
        assert_eq!(manager.used(), 16 * PAGE_SIZE);

        manager.free_pages(8);
        assert_eq!(manager.used(), 8 * PAGE_SIZE);
    }
}
