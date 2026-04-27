//! IPC Mechanism Tests
//!
//! Tests for channels, message queues, shared memory, and pipes.

use std::thread;

use beebotos_kernel::ipc::channel::channel;
use beebotos_kernel::ipc::message::Message;
use beebotos_kernel::ipc::shared_memory::SharedMemoryStats;
use beebotos_kernel::ipc::{init, shared_memory_manager, MessageQueue, SharedMemory};
use beebotos_kernel::task::TaskId;

/// Test channel creation and basic send/receive
#[test]
fn test_channel_basic() {
    let (tx, rx) = channel::<i32>(10);

    // Send a message
    tx.send(42).unwrap();

    // Receive it
    let msg = rx.recv().unwrap();
    assert_eq!(msg, 42);
}

/// Test channel multiple messages
#[test]
fn test_channel_multiple_messages() {
    let (tx, rx) = channel::<String>(5);

    // Send multiple messages
    for i in 0..5 {
        tx.send(format!("message {}", i)).unwrap();
    }

    // Receive all
    for i in 0..5 {
        let msg = rx.recv().unwrap();
        assert_eq!(msg, format!("message {}", i));
    }
}

/// Test channel capacity limit
#[test]
fn test_channel_capacity() {
    let (tx, _rx) = channel::<i32>(2);

    // Fill the channel
    tx.send(1).unwrap();
    tx.send(2).unwrap();

    // Third send should block (but we use try_send to test)
    let result = tx.try_send(3);
    assert!(result.is_err());
}

/// Test channel between threads
#[test]
fn test_channel_cross_thread() {
    let (tx, rx) = channel::<String>(10);

    // Spawn producer thread
    let tx_clone = tx.clone();
    thread::spawn(move || {
        for i in 0..10 {
            tx_clone.send(format!("thread msg {}", i)).unwrap();
        }
    });

    // Receive messages
    for i in 0..10 {
        let msg = rx.recv().unwrap();
        assert_eq!(msg, format!("thread msg {}", i));
    }
}

/// Test channel try operations
#[test]
fn test_channel_try_operations() {
    let (tx, rx) = channel::<i32>(2);

    // Try receive on empty should fail
    assert!(rx.try_recv().is_err());

    // Try send should succeed
    assert!(tx.try_send(1).is_ok());
    assert!(tx.try_send(2).is_ok());

    // Try send on full should fail
    assert!(tx.try_send(3).is_err());

    // Try receive should succeed
    assert_eq!(rx.try_recv().unwrap(), 1);
    assert_eq!(rx.try_recv().unwrap(), 2);
}

/// Test message queue
#[test]
fn test_message_queue() {
    let mut queue = MessageQueue::new(100);

    // Send messages
    queue
        .send(Message {
            sender: TaskId(1),
            content: b"msg1".to_vec(),
            msg_type: 0,
        })
        .unwrap();
    queue
        .send(Message {
            sender: TaskId(1),
            content: b"msg2".to_vec(),
            msg_type: 0,
        })
        .unwrap();
    queue
        .send(Message {
            sender: TaskId(1),
            content: b"msg3".to_vec(),
            msg_type: 0,
        })
        .unwrap();

    // Receive
    let msg1 = queue.receive().unwrap();
    assert_eq!(msg1.content, b"msg1");
    let msg2 = queue.receive().unwrap();
    assert_eq!(msg2.content, b"msg2");

    // Still one left
    let msg3 = queue.receive().unwrap();
    assert_eq!(msg3.content, b"msg3");
}

/// Test message queue overflow
#[test]
fn test_message_queue_overflow() {
    let mut queue = MessageQueue::new(2);

    queue
        .send(Message {
            sender: TaskId(1),
            content: b"1".to_vec(),
            msg_type: 0,
        })
        .unwrap();
    queue
        .send(Message {
            sender: TaskId(1),
            content: b"2".to_vec(),
            msg_type: 0,
        })
        .unwrap();

    // Third should fail (bounded)
    assert!(queue
        .send(Message {
            sender: TaskId(1),
            content: b"3".to_vec(),
            msg_type: 0,
        })
        .is_err());
}

/// Test shared memory creation
#[test]
fn test_shared_memory_creation() {
    // Initialize IPC first
    let _ = init();

    let shm = SharedMemory::new(1, 4096, 100); // 4KB
    assert!(shm.is_ok());

    let shm = shm.unwrap();
    assert_eq!(shm.size(), 4096);
}

/// Test shared memory read/write
#[test]
fn test_shared_memory_read_write() {
    let shm = SharedMemory::new(1, 1024, 100).unwrap();

    // Write data
    let data = b"Hello, Shared Memory!";
    shm.write(0, data).unwrap();

    // Read it back
    let buffer = shm.read(0, data.len()).unwrap();

    assert_eq!(&buffer, data);
}

/// Test shared memory manager
#[test]
fn test_shared_memory_manager() {
    let _ = init();
    let manager = shared_memory_manager();

    // Create a shared memory region
    let id = {
        let mgr = manager.read();
        mgr.create(4096, 100)
    };

    assert!(id.is_ok());
    assert!(id.unwrap() > 0);

    // Get stats
    let stats = {
        let mgr = manager.read();
        mgr.stats()
    };

    assert_eq!(stats.region_count, 1);
}

/// Test shared memory stats
#[test]
fn test_shared_memory_stats() {
    let stats = SharedMemoryStats {
        region_count: 5,
        total_size_bytes: 20480,
        total_mappings: 3,
    };

    assert_eq!(stats.region_count, 5);
    assert_eq!(stats.total_size_bytes, 20480);
}

/// Test pipe creation and read/write
#[test]
fn test_pipe_basic() {
    let (reader, writer) = beebotos_kernel::ipc::pipe::pipe(1024);

    // Write to pipe
    let data = b"Hello, Pipe!";
    writer.write(data).unwrap();

    // Read from pipe
    let mut buffer = vec![0u8; data.len()];
    reader.read(&mut buffer).unwrap();

    assert_eq!(&buffer, data);
}

/// Test IPC initialization
#[test]
fn test_ipc_init() {
    let result = init();
    assert!(result.is_ok());

    // Second init should also succeed (idempotent)
    let result = init();
    assert!(result.is_ok());
}

/// Test concurrent channel operations
#[test]
fn test_concurrent_channels() {
    let (tx, rx) = channel::<i32>(100);
    let mut handles = vec![];

    // Spawn producers
    for i in 0..5 {
        let tx_clone = tx.clone();
        handles.push(thread::spawn(move || {
            for j in 0..20 {
                tx_clone.send(i * 100 + j).unwrap();
            }
        }));
    }

    // Spawn consumer
    let consumer = thread::spawn(move || {
        let mut count = 0;
        while count < 100 {
            if rx.recv().is_ok() {
                count += 1;
            }
        }
        count
    });

    // Wait for producers
    for handle in handles {
        handle.join().unwrap();
    }

    // Wait for consumer
    let received = consumer.join().unwrap();
    assert_eq!(received, 100);
}

/// Test channel try operations on empty/full
#[test]
fn test_channel_try_behavior() {
    let (tx, rx) = channel::<i32>(1);

    // Try receive on empty should fail
    assert!(rx.try_recv().is_err());

    // Send a message
    tx.send(42).unwrap();

    // Should receive immediately
    assert_eq!(rx.try_recv().unwrap(), 42);
}

/// Test channel drop behavior
#[test]
fn test_channel_drop() {
    let (tx, rx) = channel::<String>(10);

    // Send some messages
    tx.send("msg1".to_string()).unwrap();
    tx.send("msg2".to_string()).unwrap();

    // Drop sender
    drop(tx);

    // Can still receive remaining messages
    assert_eq!(rx.recv().unwrap(), "msg1");
    assert_eq!(rx.recv().unwrap(), "msg2");

    // Next receive should fail (channel closed)
    assert!(rx.recv().is_err());
}
