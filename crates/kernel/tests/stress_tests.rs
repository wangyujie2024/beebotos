//! Stress Tests
//!
//! High-load tests for memory, scheduler, IPC, and WASM runtime.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use beebotos_kernel::capabilities::{CapabilityLevel, CapabilitySet};
use beebotos_kernel::memory::{MemorySnapshot, MemoryStats};
use beebotos_kernel::scheduler::{Priority, Scheduler, SchedulerConfig, Task};

/// Stress test: Massive task submission
#[tokio::test]
async fn stress_test_massive_task_submission() {
    let config = SchedulerConfig {
        max_concurrent: 100,
        time_slice_ms: 10,
        enable_preemption: true,
        default_priority: Priority::Normal,
        num_workers: 0,
        enable_work_stealing: true,
        enable_cpu_affinity: false,
    };

    let scheduler = Arc::new(Scheduler::new(config));
    scheduler.start().await.unwrap();

    let start = Instant::now();
    let task_count = 10_000;

    // Submit many tasks
    for i in 0..task_count {
        let task = Task::new(beebotos_kernel::TaskId(i), format!("stress_task_{}", i))
            .with_priority(Priority::Normal);
        scheduler.submit(task).await.unwrap();
    }

    // Wait for queue to process
    while scheduler.queue_length().await > 0 {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let elapsed = start.elapsed();
    let throughput = task_count as f64 / elapsed.as_secs_f64();

    println!(
        "Submitted {} tasks in {:?} ({:.0} tasks/sec)",
        task_count, elapsed, throughput
    );

    // Should complete within reasonable time
    assert!(elapsed < Duration::from_secs(30));

    scheduler.stop().await;
}

/// Stress test: Memory allocation pressure
#[test]
fn stress_test_memory_pressure() {
    let stats = Arc::new(MemoryStats::new());
    let iterations = 100_000;

    let start = Instant::now();

    // Simulate heavy allocation pattern
    for i in 0..iterations {
        let size = (i % 1024 + 1) * 64; // 64B to 64KB
        stats.record_allocation(size);

        // Simulate deallocation of older allocations
        if i > 1000 {
            stats.record_deallocation(size);
        }
    }

    let elapsed = start.elapsed();
    let ops_per_sec = iterations as f64 / elapsed.as_secs_f64();

    println!(
        "Memory operations: {} in {:?} ({:.0} ops/sec)",
        iterations, elapsed, ops_per_sec
    );

    // Verify stats are consistent
    let count = stats.allocation_count();
    assert_eq!(count, iterations);
}

/// Stress test: Concurrent memory allocations
#[test]
fn stress_test_concurrent_memory() {
    let stats = Arc::new(MemoryStats::new());
    let thread_count = 16;
    let ops_per_thread = 10_000;
    let mut handles = vec![];

    let start = Instant::now();

    for t in 0..thread_count {
        let stats_clone = Arc::clone(&stats);
        handles.push(thread::spawn(move || {
            for i in 0..ops_per_thread {
                let size = ((t * ops_per_thread + i) % 512 + 1) * 128;
                stats_clone.record_allocation(size);

                // Random deallocation
                if i % 3 == 0 {
                    stats_clone.record_deallocation(size);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    let elapsed = start.elapsed();
    let total_ops = thread_count * ops_per_thread;
    let ops_per_sec = total_ops as f64 / elapsed.as_secs_f64();

    println!(
        "Concurrent memory ops: {} in {:?} ({:.0} ops/sec)",
        total_ops, elapsed, ops_per_sec
    );

    let count = stats.allocation_count();
    assert_eq!(count, total_ops);
}

/// Stress test: High-throughput IPC channels
#[test]
fn stress_test_high_throughput_channels() {
    let message_count = 1_000_000;
    let (tx, rx) = beebotos_kernel::ipc::channel::channel::<i64>(10000);

    let start = Instant::now();

    // Producer thread
    let producer = thread::spawn(move || {
        for i in 0..message_count {
            tx.send(i).unwrap();
        }
    });

    // Consumer thread
    let consumer = thread::spawn(move || {
        let mut count = 0;
        while count < message_count {
            if rx.recv().is_ok() {
                count += 1;
            }
        }
        count
    });

    producer.join().unwrap();
    let received = consumer.join().unwrap();

    let elapsed = start.elapsed();
    let throughput = message_count as f64 / elapsed.as_secs_f64();

    println!(
        "IPC throughput: {} messages in {:?} ({:.0} msg/sec)",
        message_count, elapsed, throughput
    );

    assert_eq!(received, message_count);
}

/// Stress test: Many-to-many channel communication
#[test]
fn stress_test_many_to_many_channels() {
    let producer_count = 10;
    let _consumer_count = 5;
    let messages_per_producer = 10_000;
    let (tx, rx) = beebotos_kernel::ipc::channel::channel::<usize>(1000);

    let start = Instant::now();
    let mut handles = vec![];

    // Spawn producers
    for p in 0..producer_count {
        let tx_clone = tx.clone();
        handles.push(thread::spawn(move || {
            for i in 0..messages_per_producer {
                tx_clone.send(p * messages_per_producer + i).unwrap();
            }
        }));
    }

    // Spawn single consumer (Receiver doesn't implement Clone)
    let received = Arc::new(AtomicUsize::new(0));
    let received_clone = Arc::clone(&received);
    handles.push(thread::spawn(move || {
        let mut count = 0;
        while count < producer_count * messages_per_producer {
            if rx.recv().is_ok() {
                count += 1;
                received_clone.fetch_add(1, Ordering::Relaxed);
            }
        }
    }));

    for handle in handles {
        handle.join().unwrap();
    }

    let elapsed = start.elapsed();
    let total_messages = producer_count * messages_per_producer;
    let throughput = total_messages as f64 / elapsed.as_secs_f64();

    println!(
        "Many-to-many IPC: {} messages in {:?} ({:.0} msg/sec)",
        total_messages, elapsed, throughput
    );

    assert_eq!(received.load(Ordering::Relaxed), total_messages);
}

/// Stress test: Capability verification at scale
#[test]
fn stress_test_capability_verification() {
    let caps = CapabilitySet::standard();
    let iterations = 1_000_000;

    let start = Instant::now();

    for _ in 0..iterations {
        let _ = caps.verify(CapabilityLevel::L3NetworkOut);
        let _ = caps.verify(CapabilityLevel::L5SpawnLimited);
        let _ = caps.verify(CapabilityLevel::L10SystemAdmin);
    }

    let elapsed = start.elapsed();
    let ops_per_sec = (iterations * 3) as f64 / elapsed.as_secs_f64();

    println!(
        "Capability verifications: {} in {:?} ({:.0} ops/sec)",
        iterations * 3,
        elapsed,
        ops_per_sec
    );
}

/// Stress test: Priority queue ordering
#[tokio::test]
async fn stress_test_priority_ordering() {
    use beebotos_kernel::scheduler::queue::{SchedulingAlgorithm, TaskQueue};

    let queue = TaskQueue::new(SchedulingAlgorithm::Priority);
    let task_count = 1000;

    // Insert tasks in random priority order
    for i in 0..task_count {
        let priority = match i % 5 {
            0 => Priority::RealTime,
            1 => Priority::High,
            2 => Priority::Normal,
            3 => Priority::Low,
            _ => Priority::Idle,
        };

        let task = Task::new(beebotos_kernel::TaskId(i as u64), format!("task_{}", i))
            .with_priority(priority);
        queue.enqueue(task).await;
    }

    // Dequeue and verify order (lower value = higher priority)
    let mut last_priority = Priority::RealTime;
    let mut count = 0;

    while let Some(task) = queue.dequeue().await {
        // Priority values: RealTime=0, High=1, Normal=2, Low=3, Idle=4
        // Lower value = higher priority, so we expect priority to increase
        assert!(
            task.priority.level() >= last_priority.level(),
            "Priority ordering violated: {:?} ({}) after {:?} ({})",
            task.priority,
            task.priority.level(),
            last_priority,
            last_priority.level()
        );
        last_priority = task.priority;
        count += 1;
    }

    assert_eq!(count, task_count);
}

/// Stress test: Scheduler with mixed priorities
#[tokio::test]
async fn stress_test_scheduler_mixed_priorities() {
    let config = SchedulerConfig::default();
    let scheduler = Arc::new(Scheduler::new(config));
    scheduler.start().await.unwrap();

    let priorities = vec![
        Priority::RealTime,
        Priority::High,
        Priority::Normal,
        Priority::Low,
        Priority::Idle,
    ];

    // Spawn tasks with different priorities using spawn API
    for i in 0..100 {
        let priority = priorities[i % priorities.len()];
        scheduler
            .spawn(
                format!("priority_task_{}", i),
                priority,
                beebotos_kernel::capabilities::CapabilitySet::standard(),
                async { Ok(()) },
            )
            .await
            .unwrap();
    }

    // Wait for all tasks to complete
    tokio::time::sleep(Duration::from_millis(500)).await;

    let stats = scheduler.stats().await;
    // Tasks should be completed
    assert_eq!(stats.tasks_completed, 100);

    scheduler.stop().await;
}

/// Stress test: Rapid scheduler start/stop cycles
#[tokio::test]
async fn stress_test_scheduler_cycling() {
    let config = SchedulerConfig::default();

    for i in 0..10 {
        let scheduler = Scheduler::new(config.clone());
        scheduler.start().await.unwrap();

        // Submit a few tasks
        for j in 0..10 {
            let task = Task::new(
                beebotos_kernel::TaskId(j),
                format!("cycle_{}_task_{}", i, j),
            );
            scheduler.submit(task).await.unwrap();
        }

        // Small delay
        tokio::time::sleep(Duration::from_millis(10)).await;

        scheduler.stop().await;
    }
}

/// Stress test: Memory fragmentation pattern
#[test]
fn stress_test_memory_fragmentation() {
    let stats = Arc::new(MemoryStats::new());
    let pattern_count = 1000;

    // Simulate allocation pattern that causes fragmentation
    let mut allocations: Vec<(usize, usize)> = vec![]; // (size, id)

    for i in 0..pattern_count {
        let size = if i % 2 == 0 { 64 } else { 1024 }; // Alternate small/large
        stats.record_allocation(size);
        allocations.push((size, i));

        // Free every third allocation (creates holes)
        if i % 3 == 0 && !allocations.is_empty() {
            let idx = allocations.len() / 2;
            let (size, _) = allocations.remove(idx);
            stats.record_deallocation(size);
        }
    }

    // Calculate fragmentation
    let snapshot = MemorySnapshot {
        current_used_bytes: stats.current_used(),
        peak_used_bytes: stats.peak_used(),
        total_allocated_bytes: stats.total_allocated(),
        total_freed_bytes: stats.total_freed(),
        allocation_count: stats.allocation_count(),
        deallocation_count: stats.deallocation_count(),
    };

    let fragmentation = snapshot.fragmentation_ratio();
    println!("Fragmentation ratio: {:.2}%", fragmentation * 100.0);

    // Fragmentation should be within reasonable bounds
    assert!(fragmentation >= 0.0 && fragmentation < 0.5);
}

/// Stress test: Contention on shared memory stats
#[test]
fn stress_test_memory_stats_contention() {
    let stats = Arc::new(MemoryStats::new());
    let thread_count = 32;
    let ops_per_thread = 1000;
    let mut handles = vec![];

    let start = Instant::now();

    // Many threads reading and writing stats concurrently
    for _ in 0..thread_count {
        let stats_clone = Arc::clone(&stats);
        handles.push(thread::spawn(move || {
            for i in 0..ops_per_thread {
                // Mix of reads and writes
                if i % 10 == 0 {
                    let _ = stats_clone.current_used();
                } else {
                    stats_clone.record_allocation(64);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    let elapsed = start.elapsed();
    let total_ops = thread_count * ops_per_thread;
    let ops_per_sec = total_ops as f64 / elapsed.as_secs_f64();

    println!(
        "Memory stats contention: {} ops in {:?} ({:.0} ops/sec)",
        total_ops, elapsed, ops_per_sec
    );
}

/// Performance baseline test
#[test]
fn performance_baseline() {
    let start = Instant::now();

    // Simple operations that should be fast
    let mut sum = 0u64;
    for i in 0..1_000_000 {
        sum = sum.wrapping_add(i);
    }

    let elapsed = start.elapsed();
    println!("Baseline loop: {} iterations in {:?}", 1_000_000, elapsed);

    // Sanity check
    assert!(elapsed < Duration::from_secs(1));
    assert!(sum > 0);
}
