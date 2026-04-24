//! Pipe
//!
//! Unidirectional pipe for inter-process communication.
//! Uses a bounded buffer with blocking reads and writes.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};

use crate::error::{KernelError, Result};

/// Pipe reader endpoint
#[derive(Clone)]
pub struct PipeReader {
    /// Pipe inner
    inner: Arc<PipeInner>,
}

impl PipeReader {
    /// Read data from the pipe into the buffer
    ///
    /// Returns the number of bytes read. Blocks if the pipe is empty
    /// until data is available.
    pub fn read(&self, buf: &mut [u8]) -> Result<usize> {
        let mut buffer = self
            .inner
            .buffer
            .lock()
            .map_err(|_| KernelError::internal("Pipe lock poisoned"))?;

        // Wait if pipe is empty and not closed
        while buffer.is_empty()
            && !*self
                .inner
                .closed
                .lock()
                .map_err(|_| KernelError::internal("Pipe closed lock poisoned"))?
        {
            buffer = self
                .inner
                .not_empty
                .wait(buffer)
                .map_err(|_| KernelError::internal("Pipe wait failed"))?;
        }

        // Read available data
        let to_read = buf.len().min(buffer.len());
        for i in 0..to_read {
            buf[i] = buffer
                .pop_front()
                .ok_or_else(|| KernelError::internal("Pipe buffer unexpectedly empty"))?;
        }
        drop(buffer);

        // Notify waiting writers
        self.inner.not_full.notify_one();
        Ok(to_read)
    }

    /// Try to read without blocking
    ///
    /// Returns immediately with however much data is available.
    pub fn try_read(&self, buf: &mut [u8]) -> Result<usize> {
        let mut buffer = self
            .inner
            .buffer
            .lock()
            .map_err(|_| KernelError::internal("Pipe lock poisoned"))?;

        let to_read = buf.len().min(buffer.len());
        for i in 0..to_read {
            buf[i] = buffer
                .pop_front()
                .ok_or_else(|| KernelError::internal("Pipe buffer unexpectedly empty"))?;
        }
        drop(buffer);

        if to_read > 0 {
            self.inner.not_full.notify_one();
        }
        Ok(to_read)
    }
}

/// Pipe writer endpoint
#[derive(Clone)]
pub struct PipeWriter {
    /// Pipe inner
    inner: Arc<PipeInner>,
}

impl PipeWriter {
    /// Write data to the pipe
    ///
    /// Returns the number of bytes written. Blocks if the pipe is full
    /// until space is available.
    pub fn write(&self, data: &[u8]) -> Result<usize> {
        let mut buffer = self
            .inner
            .buffer
            .lock()
            .map_err(|_| KernelError::internal("Pipe lock poisoned"))?;

        // Check if pipe is closed
        if *self
            .inner
            .closed
            .lock()
            .map_err(|_| KernelError::internal("Pipe closed lock poisoned"))?
        {
            return Err(KernelError::Io("Pipe closed".into()));
        }

        let capacity = self.inner.capacity;

        // Write as much as possible
        let mut written = 0;
        for &byte in data {
            // Wait if buffer is full
            while buffer.len() >= capacity {
                buffer = self
                    .inner
                    .not_full
                    .wait(buffer)
                    .map_err(|_| KernelError::internal("Pipe wait failed"))?;

                // Check if closed while waiting
                if *self
                    .inner
                    .closed
                    .lock()
                    .map_err(|_| KernelError::internal("Pipe closed lock poisoned"))?
                {
                    return Ok(written);
                }
            }

            buffer.push_back(byte);
            written += 1;
        }
        drop(buffer);

        // Notify waiting readers
        self.inner.not_empty.notify_one();
        Ok(written)
    }

    /// Try to write without blocking
    ///
    /// Returns immediately with however much data could be written.
    pub fn try_write(&self, data: &[u8]) -> Result<usize> {
        let mut buffer = self
            .inner
            .buffer
            .lock()
            .map_err(|_| KernelError::internal("Pipe lock poisoned"))?;

        // Check if pipe is closed
        if *self
            .inner
            .closed
            .lock()
            .map_err(|_| KernelError::internal("Pipe closed lock poisoned"))?
        {
            return Err(KernelError::Io("Pipe closed".into()));
        }

        let capacity = self.inner.capacity;
        let to_write = data.len().min(capacity - buffer.len());

        for i in 0..to_write {
            buffer.push_back(data[i]);
        }
        drop(buffer);

        if to_write > 0 {
            self.inner.not_empty.notify_one();
        }
        Ok(to_write)
    }
}

// Inner pipe state shared between reader and writer
struct PipeInner {
    buffer: Mutex<VecDeque<u8>>,
    capacity: usize,
    closed: Mutex<bool>,
    not_empty: Condvar,
    not_full: Condvar,
}

/// Create a new pipe with the given buffer capacity
///
/// Returns (reader, writer) pair. Data written to the writer can be
/// read from the reader.
pub fn pipe(capacity: usize) -> (PipeReader, PipeWriter) {
    let inner = Arc::new(PipeInner {
        buffer: Mutex::new(VecDeque::with_capacity(capacity)),
        capacity,
        closed: Mutex::new(false),
        not_empty: Condvar::new(),
        not_full: Condvar::new(),
    });

    let reader = PipeReader {
        inner: inner.clone(),
    };
    let writer = PipeWriter { inner };

    (reader, writer)
}

/// Close the pipe
///
/// After closing, write operations will fail and read operations
/// will return any remaining data, then return 0.
pub fn close_pipe(reader: &PipeReader) -> Result<()> {
    let mut closed = reader
        .inner
        .closed
        .lock()
        .map_err(|_| KernelError::internal("Pipe closed lock poisoned"))?;
    *closed = true;
    drop(closed);

    // Wake up all waiters
    reader.inner.not_empty.notify_all();
    reader.inner.not_full.notify_all();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipe_basic() {
        let (mut reader, writer) = pipe(1024);

        writer.write(b"hello").unwrap();

        let mut buf = [0u8; 10];
        let n = reader.read(&mut buf).unwrap();

        assert_eq!(n, 5);
        assert_eq!(&buf[..5], b"hello");
    }

    #[test]
    fn test_pipe_try_write_read() {
        let (reader, writer) = pipe(5);

        // Fill the pipe
        assert_eq!(writer.try_write(b"hello").unwrap(), 5);

        // Should only write partial
        assert_eq!(writer.try_write(b"world").unwrap(), 0);

        // Read some data
        let mut buf = [0u8; 3];
        assert_eq!(reader.try_read(&mut buf).unwrap(), 3);
        assert_eq!(&buf, b"hel");
    }

    #[test]
    fn test_pipe_close() {
        let (reader, writer) = pipe(1024);

        writer.write(b"test").unwrap();
        close_pipe(&reader).unwrap();

        // Write should fail after close
        assert!(writer.write(b"more").is_err());
    }
}
