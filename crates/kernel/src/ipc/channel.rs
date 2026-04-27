//! IPC Channel
//!
//! Thread-safe inter-process communication channel with bounded capacity.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};

use crate::error::{KernelError, Result};

/// IPC channel sender endpoint
#[derive(Clone)]
pub struct Sender<T> {
    /// Channel inner
    inner: Arc<ChannelInner<T>>,
}

impl<T> Sender<T> {
    /// Send a message through the channel
    ///
    /// Blocks if the channel is full until space is available.
    pub fn send(&self, msg: T) -> Result<()> {
        let mut buffer = self
            .inner
            .buffer
            .lock()
            .map_err(|_| KernelError::internal("Channel lock poisoned"))?;

        // Wait if buffer is full
        while buffer.len() >= self.inner.capacity {
            buffer = self
                .inner
                .not_full
                .wait(buffer)
                .map_err(|_| KernelError::internal("Channel wait failed"))?;
        }

        buffer.push_back(msg);
        drop(buffer);

        // Notify waiting receivers
        self.inner.not_empty.notify_one();
        Ok(())
    }

    /// Try to send without blocking
    ///
    /// Returns Err(WouldBlock) if the channel is full.
    pub fn try_send(&self, msg: T) -> Result<()> {
        let mut buffer = self
            .inner
            .buffer
            .lock()
            .map_err(|_| KernelError::internal("Channel lock poisoned"))?;

        if buffer.len() >= self.inner.capacity {
            return Err(KernelError::WouldBlock);
        }

        buffer.push_back(msg);
        drop(buffer);

        self.inner.not_empty.notify_one();
        Ok(())
    }
}

/// IPC channel receiver endpoint
pub struct Receiver<T> {
    /// Channel inner
    inner: Arc<ChannelInner<T>>,
}

impl<T> Receiver<T> {
    /// Receive a message from the channel
    ///
    /// Blocks if the channel is empty until a message is available.
    /// Returns Err(ChannelClosed) if all senders have been dropped.
    pub fn recv(&self) -> Result<T> {
        let mut buffer = self
            .inner
            .buffer
            .lock()
            .map_err(|_| KernelError::internal("Channel lock poisoned"))?;

        // Wait if buffer is empty and channel is not closed
        loop {
            // Check if there are messages
            if let Some(msg) = buffer.pop_front() {
                drop(buffer);
                // Notify waiting senders
                self.inner.not_full.notify_one();
                return Ok(msg);
            }

            // Check if all senders have been dropped (only receiver holds Arc)
            // Arc::strong_count returns the number of strong references
            // If it's 1, only the receiver holds the Arc, meaning all senders are dropped
            if Arc::strong_count(&self.inner) == 1 {
                return Err(KernelError::Io("Channel closed".into()));
            }

            // Wait for messages
            buffer = self
                .inner
                .not_empty
                .wait(buffer)
                .map_err(|_| KernelError::internal("Channel wait failed"))?;
        }
    }

    /// Try to receive without blocking
    ///
    /// Returns Err(WouldBlock) if the channel is empty.
    pub fn try_recv(&self) -> Result<T> {
        let mut buffer = self
            .inner
            .buffer
            .lock()
            .map_err(|_| KernelError::internal("Channel lock poisoned"))?;

        let msg = buffer.pop_front().ok_or_else(|| KernelError::WouldBlock)?;
        drop(buffer);

        self.inner.not_full.notify_one();
        Ok(msg)
    }
}

// Inner channel state shared between sender and receiver
struct ChannelInner<T> {
    buffer: Mutex<VecDeque<T>>,
    capacity: usize,
    not_empty: Condvar,
    not_full: Condvar,
}

/// Create a new IPC channel with the given capacity
///
/// Returns (sender, receiver) pair. The channel is bounded and will block
/// senders when full, and block receivers when empty.
pub fn channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(ChannelInner {
        buffer: Mutex::new(VecDeque::with_capacity(capacity)),
        capacity,
        not_empty: Condvar::new(),
        not_full: Condvar::new(),
    });

    let sender = Sender {
        inner: inner.clone(),
    };
    let receiver = Receiver { inner };

    (sender, receiver)
}

/// Legacy IPC channel struct for backward compatibility
///
/// Note: Use `channel()` function for new code as it provides better ergonomics
/// with separate Sender/Receiver endpoints.
pub struct IpcChannel<T> {
    inner: Arc<ChannelInner<T>>,
}

impl<T> IpcChannel<T> {
    /// Create a new IPC channel
    pub fn new(capacity: usize) -> Self {
        let inner = Arc::new(ChannelInner {
            buffer: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
            not_empty: Condvar::new(),
            not_full: Condvar::new(),
        });
        Self { inner }
    }

    /// Send a message through the channel
    ///
    /// Blocks if the channel is full.
    pub fn send(&self, msg: T) -> Result<()> {
        let sender = Sender {
            inner: self.inner.clone(),
        };
        sender.send(msg)
    }

    /// Receive a message from the channel
    ///
    /// Blocks if the channel is empty.
    pub fn recv(&self) -> Result<T> {
        let receiver = Receiver {
            inner: self.inner.clone(),
        };
        receiver.recv()
    }

    /// Split the channel into sender and receiver endpoints
    pub fn split(self) -> (Sender<T>, Receiver<T>) {
        let sender = Sender {
            inner: self.inner.clone(),
        };
        let receiver = Receiver { inner: self.inner };
        (sender, receiver)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_basic() {
        let (tx, rx) = channel::<i32>(10);

        tx.send(42).unwrap();
        tx.send(24).unwrap();

        assert_eq!(rx.recv().unwrap(), 42);
        assert_eq!(rx.recv().unwrap(), 24);
    }

    #[test]
    fn test_channel_try_send_recv() {
        let (tx, rx) = channel::<i32>(2);

        // Fill the channel
        tx.try_send(1).unwrap();
        tx.try_send(2).unwrap();

        // Should fail - channel full
        assert!(tx.try_send(3).is_err());

        // Empty the channel
        assert_eq!(rx.try_recv().unwrap(), 1);
        assert_eq!(rx.try_recv().unwrap(), 2);

        // Should fail - channel empty
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn test_ipc_channel() {
        let channel = IpcChannel::<String>::new(10);

        channel.send("hello".to_string()).unwrap();
        channel.send("world".to_string()).unwrap();

        assert_eq!(channel.recv().unwrap(), "hello");
        assert_eq!(channel.recv().unwrap(), "world");
    }

    #[test]
    fn test_channel_split() {
        let channel = IpcChannel::<i32>::new(5);
        let (tx, rx) = channel.split();

        tx.send(100).unwrap();
        assert_eq!(rx.recv().unwrap(), 100);
    }
}
