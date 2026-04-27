//! IPC Mechanism Benchmarks

use std::sync::mpsc;
use std::thread;

use beebotos_kernel::ipc::channel::{bounded, unbounded};
use beebotos_kernel::ipc::MessageQueue;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

fn bench_channel_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("channel_throughput");

    for capacity in [1, 10, 100, 1000].iter() {
        group.throughput(Throughput::Elements(*capacity as u64));

        group.bench_with_input(
            BenchmarkId::new("bounded_send_recv", capacity),
            capacity,
            |b, &capacity| {
                b.iter(|| {
                    let (tx, rx) = bounded::<i64>(capacity);
                    for i in 0..capacity {
                        tx.send(i as i64).unwrap();
                    }
                    for _ in 0..capacity {
                        black_box(rx.recv().unwrap());
                    }
                });
            },
        );
    }

    group.finish();
}

fn bench_unbounded_channel(c: &mut Criterion) {
    let mut group = c.benchmark_group("unbounded_channel");

    for message_count in [100, 1000, 10000].iter() {
        group.throughput(Throughput::Elements(*message_count as u64));

        group.bench_with_input(
            BenchmarkId::new("send_recv", message_count),
            message_count,
            |b, &count| {
                b.iter(|| {
                    let (tx, rx) = unbounded::<i64>();
                    for i in 0..count {
                        tx.send(i as i64).unwrap();
                    }
                    for _ in 0..count {
                        black_box(rx.recv().unwrap());
                    }
                });
            },
        );
    }

    group.finish();
}

fn bench_message_queue(c: &mut Criterion) {
    let mut group = c.benchmark_group("message_queue");

    for capacity in [100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("enqueue_dequeue", capacity),
            capacity,
            |b, &capacity| {
                b.iter(|| {
                    let queue = MessageQueue::<i64>::new(capacity);
                    for i in 0..capacity {
                        queue.enqueue(i as i64).unwrap();
                    }
                    for _ in 0..capacity {
                        black_box(queue.dequeue().unwrap());
                    }
                });
            },
        );
    }

    group.finish();
}

fn bench_cross_thread_channel(c: &mut Criterion) {
    let mut group = c.benchmark_group("cross_thread_channel");

    group.bench_function("single_producer_single_consumer", |b| {
        b.iter(|| {
            let (tx, rx) = bounded::<i64>(1000);

            let producer = thread::spawn(move || {
                for i in 0..1000 {
                    tx.send(i).unwrap();
                }
            });

            let consumer = thread::spawn(move || {
                for _ in 0..1000 {
                    black_box(rx.recv().unwrap());
                }
            });

            producer.join().unwrap();
            consumer.join().unwrap();
        });
    });

    group.bench_function("std_mpsc_comparison", |b| {
        b.iter(|| {
            let (tx, rx) = mpsc::channel::<i64>();

            let producer = thread::spawn(move || {
                for i in 0..1000 {
                    tx.send(i).unwrap();
                }
            });

            let consumer = thread::spawn(move || {
                for _ in 0..1000 {
                    black_box(rx.recv().unwrap());
                }
            });

            producer.join().unwrap();
            consumer.join().unwrap();
        });
    });

    group.finish();
}

fn bench_channel_try_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("channel_try_operations");

    group.bench_function("try_send_success", |b| {
        let (tx, _rx) = bounded::<i64>(10000);
        b.iter(|| {
            black_box(tx.try_send(42).unwrap());
        });
    });

    group.bench_function("try_send_failure", |b| {
        let (tx, _rx) = bounded::<i64>(1);
        tx.try_send(1).unwrap(); // Fill the channel
        b.iter(|| {
            black_box(tx.try_send(42).is_err());
        });
    });

    group.bench_function("try_recv_success", |b| {
        let (tx, rx) = bounded::<i64>(10000);
        for i in 0..10000 {
            tx.send(i).unwrap();
        }
        b.iter(|| {
            black_box(rx.try_recv().unwrap());
        });
    });

    group.bench_function("try_recv_failure", |b| {
        let (_tx, rx) = bounded::<i64>(1);
        b.iter(|| {
            black_box(rx.try_recv().is_err());
        });
    });

    group.finish();
}

criterion_group!(
    ipc_benches,
    bench_channel_throughput,
    bench_unbounded_channel,
    bench_message_queue,
    bench_cross_thread_channel,
    bench_channel_try_operations,
);
criterion_main!(ipc_benches);
