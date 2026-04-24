//! Memory System Benchmarks

use beebotos_brain::memory::{MemoryIndex, MemoryQuery, ShortTermMemory};
use beebotos_brain::SocialBrainApi;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

/// Benchmark short-term memory operations
fn bench_stm_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("short_term_memory");

    // Benchmark store operation
    group.bench_function("store", |b| {
        let mut stm = ShortTermMemory::new();
        let mut counter = 0;
        b.iter(|| {
            stm.push(&format!("Content {}", black_box(counter)));
            counter += 1;
        });
    });

    // Benchmark retrieve operation
    group.bench_function("retrieve", |b| {
        let mut stm = ShortTermMemory::new();
        // Pre-populate
        for i in 0..100 {
            stm.push(&format!("Test content {}", i));
        }

        b.iter(|| {
            stm.retrieve(black_box("Test"));
        });
    });

    group.finish();
}

/// Benchmark memory index operations
fn bench_memory_index(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_index");

    // Benchmark index addition with varying document sizes
    for size in [100, 1000, 10000].iter() {
        group.bench_with_input(BenchmarkId::new("add", size), size, |b, &size| {
            let mut index = MemoryIndex::new();
            let mut counter = 0;
            b.iter(|| {
                index.add(
                    &format!("id{}", counter),
                    &format!("This is test content number {}", counter % size),
                );
                counter += 1;
            });
        });
    }

    // Benchmark search with varying index sizes
    for size in [100, 1000, 10000].iter() {
        group.bench_with_input(BenchmarkId::new("search", size), size, |b, &size| {
            let mut index = MemoryIndex::new();
            // Pre-populate
            for i in 0..size {
                index.add(
                    &format!("id{}", i),
                    &format!("The quick brown fox jumps over the lazy dog number {}", i),
                );
            }

            b.iter(|| {
                index.search(black_box("quick brown"));
            });
        });
    }

    group.finish();
}

/// Benchmark API-level memory operations
fn bench_api_memory(c: &mut Criterion) {
    let mut group = c.benchmark_group("api_memory");

    // Benchmark store_memory
    group.bench_function("store_memory", |b| {
        let mut api = SocialBrainApi::new();
        let mut counter = 0;
        b.iter(|| {
            api.store_memory(&format!("Memory content {}", black_box(counter)), 0.7)
                .unwrap();
            counter += 1;
        });
    });

    // Benchmark query_memory
    group.bench_function("query_memory", |b| {
        let mut api = SocialBrainApi::new();
        // Pre-populate
        for i in 0..100 {
            api.store_memory(&format!("Test memory item {}", i), 0.6)
                .unwrap();
        }

        b.iter(|| {
            let query = MemoryQuery::new(black_box("memory"));
            api.query_memory(&query).unwrap();
        });
    });

    group.finish();
}

/// Benchmark memory consolidation
fn bench_memory_consolidation(c: &mut Criterion) {
    c.bench_function("consolidate_memories", |b| {
        let mut api = SocialBrainApi::new();
        // Pre-populate with many memories
        for i in 0..1000 {
            api.store_memory(&format!("Content to consolidate {}", i), 0.8)
                .unwrap();
        }

        b.iter(|| {
            api.consolidate_memories().unwrap();
        });
    });
}

criterion_group!(
    memory_benches,
    bench_stm_operations,
    bench_memory_index,
    bench_api_memory,
    bench_memory_consolidation
);
criterion_main!(memory_benches);
