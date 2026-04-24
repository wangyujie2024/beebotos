//! Memory Management Benchmarks

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::Ordering;

use beebotos_kernel::memory::slab::SlabAllocator;
use beebotos_kernel::memory::{MemorySnapshot, MemoryStats};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

fn bench_memory_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_allocation");

    for size in [64, 256, 1024, 4096, 16384].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            b.iter(|| {
                let layout = Layout::from_size_align(size, 8).unwrap();
                let ptr = unsafe { System.alloc(layout) };
                black_box(ptr);
                unsafe { System.dealloc(ptr, layout) };
            });
        });
    }

    group.finish();
}

fn bench_memory_stats_recording(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_stats");

    group.bench_function("record_allocation", |b| {
        let stats = MemoryStats::new();
        b.iter(|| {
            stats.record_allocation(black_box(1024));
        });
    });

    group.bench_function("record_deallocation", |b| {
        let stats = MemoryStats::new();
        stats.record_allocation(1024);
        b.iter(|| {
            stats.record_deallocation(black_box(1024));
        });
    });

    group.bench_function("concurrent_allocation_recording", |b| {
        use std::sync::Arc;
        use std::thread;

        let stats = Arc::new(MemoryStats::new());
        b.iter(|| {
            let mut handles = vec![];
            for _ in 0..4 {
                let stats_clone = Arc::clone(&stats);
                handles.push(thread::spawn(move || {
                    for _ in 0..100 {
                        stats_clone.record_allocation(64);
                    }
                }));
            }
            for h in handles {
                h.join().unwrap();
            }
        });
    });

    group.finish();
}

fn bench_slab_allocator(c: &mut Criterion) {
    let mut group = c.benchmark_group("slab_allocator");

    for object_size in [64, 128, 256, 512].iter() {
        let slab = SlabAllocator::new(*object_size, 10000);

        group.bench_with_input(
            BenchmarkId::new("allocate", object_size),
            object_size,
            |b, _| {
                b.iter(|| {
                    let ptr = slab.allocate();
                    black_box(ptr);
                    unsafe { slab.deallocate(ptr) };
                });
            },
        );
    }

    group.finish();
}

fn bench_memory_snapshot(c: &mut Criterion) {
    c.bench_function("snapshot_capture", |b| {
        // Pre-populate some stats
        let _stats = MemoryStats::new();
        for _ in 0..1000 {
            _stats.record_allocation(1024);
        }

        b.iter(|| {
            let snapshot = MemorySnapshot::capture();
            black_box(snapshot);
        });
    });
}

fn bench_fragmentation_calculation(c: &mut Criterion) {
    let snapshot = MemorySnapshot {
        current_used_bytes: 1100,
        peak_used_bytes: 1100,
        total_allocated_bytes: 1000,
        total_freed_bytes: 0,
        allocation_count: 10,
        deallocation_count: 0,
    };

    c.bench_function("fragmentation_calculation", |b| {
        b.iter(|| {
            let ratio = snapshot.fragmentation_ratio();
            black_box(ratio);
        });
    });
}

criterion_group!(
    memory_benches,
    bench_memory_allocation,
    bench_memory_stats_recording,
    bench_slab_allocator,
    bench_memory_snapshot,
    bench_fragmentation_calculation,
);
criterion_main!(memory_benches);
