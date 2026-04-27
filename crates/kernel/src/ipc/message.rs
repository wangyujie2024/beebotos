//! Message Queue
//!
//! Inter-process message queue implementation.

use std::collections::VecDeque;

use crate::error::Result;
use crate::task::TaskId;

/// Inter-process message
#[derive(Debug)]
pub struct Message {
    /// Sender task ID
    pub sender: TaskId,
    /// Message payload
    pub content: Vec<u8>,
    /// Message type identifier
    pub msg_type: u32,
}

/// Message queue for IPC
pub struct MessageQueue {
    queue: VecDeque<Message>,
    max_size: usize,
}

impl MessageQueue {
    /// Create new message queue with max size
    pub fn new(max_size: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            max_size,
        }
    }

    /// Send message to queue
    pub fn send(&mut self, msg: Message) -> Result<()> {
        if self.queue.len() >= self.max_size {
            return Err(crate::error::KernelError::out_of_memory());
        }
        self.queue.push_back(msg);
        Ok(())
    }

    /// Receive message from queue
    pub fn receive(&mut self) -> Result<Message> {
        self.queue
            .pop_front()
            .ok_or_else(|| crate::error::KernelError::WouldBlock)
    }
}
