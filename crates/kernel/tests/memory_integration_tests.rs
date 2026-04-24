//! Memory Management Integration Tests
//!
//! Tests for memory allocator, safety tracker, isolation, and VM management.

use std::alloc::{GlobalAlloc, Layout};
use std::sync::Arc;

use beebotos_kernel::memory::isolation::{MemoryIsolation, MemoryPermissions, ProcessMemorySpace};
use beebotos_kernel::memory::safety::{MemoryGuard, MemorySafetyTracker};
use beebotos_kernel::memory::slab::SlabAllocator;
use beebotos_kernel::memory::{
    allocator, check_pressure, MemoryConfig, MemoryPressure, MemoryRegion, MemoryRegionType,
    MemorySnapshot, MemoryStats,
};

/// Test memory statistics tracking
#[test]
fn test_memory_stats_tracking() {
    let stats = MemoryStats::new();

    // Record some allocations
    stats.record_allocation(1024);
    stats.record_allocation(2048);

    assert_eq!(stats.current_used(), 3072);
    assert_eq!(stats.allocation_count(), 2);

    // Record deallocations
    stats.record_deallocation(1024);
    assert_eq!(stats.current_used(), 2048);
    assert_eq!(stats.deallocation_count(), 1);

    // Check peak tracking
    assert!(stats.peak_used() >= 3072);
}

/// Test memory snapshot capture
#[test]
fn test_memory_snapshot() {
    let snapshot = MemorySnapshot::capture();

    // Verify basic properties
    assert!(snapshot.current_used_bytes <= snapshot.peak_used_bytes);
    assert!(snapshot.allocation_count >= snapshot.deallocation_count);

    // Test formatting
    let formatted = snapshot.format();
    assert!(formatted.contains("Memory:"));
    assert!(formatted.contains("MB"));
}

/// Test fragmentation calculation
#[test]
fn test_fragmentation_calculation() {
    let snapshot = MemorySnapshot {
        current_used_bytes: 1100,
        peak_used_bytes: 1100,
        total_allocated_bytes: 1000,
        total_freed_bytes: 0,
        allocation_count: 10,
        deallocation_count: 0,
    };

    // Fragmentation = (tracked - actual) / actual
    // = (1100 - 1000) / 1000 = 0.1 = 10%
    let ratio = snapshot.fragmentation_ratio();
    assert!((ratio - 0.1).abs() < 0.01);
}

/// Test memory pressure levels
#[test]
fn test_memory_pressure_levels() {
    // Save original limit
    let original_limit = allocator::get_memory_limit();

    // Set a small limit for testing
    allocator::set_memory_limit(100 * 1024 * 1024); // 100MB

    // Check that we get a valid pressure reading
    let pressure = check_pressure();
    match pressure {
        MemoryPressure::Normal
        | MemoryPressure::Elevated
        | MemoryPressure::Critical
        | MemoryPressure::OutOfMemory => {}
    }

    // Restore original limit
    allocator::set_memory_limit(original_limit);
}

/// Test memory region creation
#[test]
fn test_memory_region() {
    let region = MemoryRegion {
        start: 0x1000,
        size: 4096,
        region_type: MemoryRegionType::Usable,
    };

    assert_eq!(region.start, 0x1000);
    assert_eq!(region.size, 4096);
    assert!(matches!(region.region_type, MemoryRegionType::Usable));
}

/// Test memory configuration defaults
#[test]
fn test_memory_config_defaults() {
    let config = MemoryConfig::default();

    assert_eq!(config.initial_heap_size, 64 * 1024 * 1024); // 64MB
    assert!(!config.huge_pages);
    assert_eq!(config.wasm_memory_limit, 128 * 1024 * 1024); // 128MB
}

/// Test memory safety tracker
#[test]
fn test_memory_safety_tracker() {
    let tracker = MemorySafetyTracker::new(false);

    // Record an allocation - takes pointer and size
    let ptr = 0x1000 as *mut u8;
    tracker.record_allocation(ptr, 1024);

    // Verify allocation is tracked using check_access method
    assert!(matches!(
        tracker.check_access(ptr, 512),
        beebotos_kernel::memory::safety::AccessCheck::Valid(_)
    ));
    // Check beyond allocated region - unknown pointers return Unknown (not Invalid)
    let ptr2 = 0x2000 as *const u8;
    assert!(matches!(
        tracker.check_access(ptr2, 1),
        beebotos_kernel::memory::safety::AccessCheck::Unknown
    ));
}

/// Test memory guard creation
#[test]
fn test_memory_guard() {
    // MemoryGuard::new takes Arc<MemorySafetyTracker>, not address/size
    let tracker = Arc::new(MemorySafetyTracker::new(false));
    let guard = MemoryGuard::new(tracker);

    // The guard doesn't have protects method, just verify it can be created
    // MemoryGuard is used for RAII-style safety tracking
    drop(guard);
}

/// Test memory isolation
#[test]
fn test_memory_isolation() {
    let _isolation = MemoryIsolation::new();

    // Create a process space - use new_user instead of new
    let space = ProcessMemorySpace::new_user(1);
    assert_eq!(space.process_id(), 1);

    // Create a user memory region - MemoryPermissions is a struct with fields
    let region = beebotos_kernel::memory::isolation::UserMemoryRegion {
        start: 0x1000,
        size: 4096,
        permissions: MemoryPermissions::read_write(),
        region_type: beebotos_kernel::memory::isolation::MemoryRegionType::Data,
    };

    // Check permissions using has_all method
    assert!(region.permissions.has_all(MemoryPermissions::read_only()));
    assert!(region.permissions.has_all(MemoryPermissions::read_write()));
}

/// Test slab allocator
#[test]
fn test_slab_allocator() {
    let slab = SlabAllocator::new(); // No arguments needed

    // Allocate some objects using alloc with size
    let ptr1 = slab.alloc(64).unwrap();
    let ptr2 = slab.alloc(64).unwrap();

    assert!(!ptr1.is_null());
    assert!(!ptr2.is_null());
    assert_ne!(ptr1, ptr2);

    // Free and reallocate using free with size
    unsafe { slab.free(ptr1, 64) };
    let ptr3 = slab.alloc(64).unwrap();
    // May or may not reuse the freed slot depending on implementation
    assert!(!ptr3.is_null());
}

/// Test memory pool operations
#[test]
fn test_memory_pool() {
    use beebotos_kernel::memory::MemoryPool;

    let mut pool = MemoryPool::new(1024, 10); // 1KB blocks, 10 of them

    // Allocate from pool
    let block = pool.allocate();
    assert!(block.is_some());

    // Allocate all blocks
    let mut blocks = Vec::new();
    for _ in 0..10 {
        if let Some(block) = pool.allocate() {
            blocks.push(block);
        }
    }

    // Pool should be exhausted
    assert!(pool.allocate().is_none());

    // Free one and allocate again
    if let Some(block) = blocks.pop() {
        pool.free(block);
        assert!(pool.allocate().is_some());
    }
}

/// Test concurrent memory allocations
#[test]
fn test_concurrent_allocations() {
    let stats = Arc::new(MemoryStats::new());
    let mut handles = vec![];

    // Spawn threads that record allocations
    for i in 0..10 {
        let stats_clone = Arc::clone(&stats);
        handles.push(std::thread::spawn(move || {
            for _ in 0..100 {
                stats_clone.record_allocation((i + 1) * 64);
            }
        }));
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify total allocations using method instead of field access
    assert_eq!(stats.allocation_count(), 1000);
}

/// Test memory limit enforcement
#[test]
fn test_memory_limit() {
    // Set a limit
    allocator::set_memory_limit(1024 * 1024); // 1MB

    // Get current limit
    let limit = allocator::get_memory_limit();
    assert_eq!(limit, 1024 * 1024);

    // Reset to 0 (no limit)
    allocator::set_memory_limit(0);
    assert_eq!(allocator::get_memory_limit(), 0);
}

/// Test virtual memory region
#[test]
fn test_virtual_memory_region() {
    use beebotos_kernel::memory::vm::{RegionBacking, RegionFlags, VirtualRegion};

    let region = VirtualRegion {
        start: 0x1000,
        size: 4096,
        flags: RegionFlags::read_write(),
        backing: RegionBacking::Anonymous,
        name: "test".to_string(),
    };

    // Check flags using field access instead of contains method
    assert!(region.flags.readable);
    assert!(region.flags.writable);
    assert!(!region.flags.executable);
}

/// Test kernel allocator wrapper
#[test]
fn test_kernel_allocator() {
    use beebotos_kernel::memory::KernelAllocator;

    // KernelAllocator is a unit struct, no constructor needed
    let allocator = KernelAllocator;

    // Allocate some memory
    let layout = Layout::from_size_align(1024, 8).unwrap();
    let ptr = unsafe { allocator.alloc(layout) };

    assert!(!ptr.is_null());

    // Write to memory
    unsafe {
        std::ptr::write_bytes(ptr, 0xAB, 1024);
    }

    // Free the memory
    unsafe { allocator.dealloc(ptr, layout) };
}

/// Test memory statistics formatting
#[test]
fn test_memory_stats_format() {
    let formatted = allocator::format_stats();

    // Should contain key metrics
    assert!(formatted.contains("Memory Stats"));
    assert!(formatted.contains("Current:"));
    assert!(formatted.contains("Peak:"));
}

/// Test memory manager singleton
#[test]
fn test_memory_manager_singleton() {
    use beebotos_kernel::memory::manager;

    let manager1 = manager();
    let manager2 = manager();

    // Both should point to the same instance
    assert!(std::ptr::eq(manager1, manager2));
}
