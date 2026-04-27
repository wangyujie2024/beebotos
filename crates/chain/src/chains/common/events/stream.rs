//! Event Stream Types

use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::sync::mpsc;
use tokio_stream::Stream;

use crate::chains::common::events::EvmEvent;
use crate::ChainError;

/// Event stream with additional configuration
pub struct EventStream {
    receiver: mpsc::Receiver<std::result::Result<EvmEvent, ChainError>>,
    config: EventStreamConfig,
}

impl EventStream {
    /// Create new event stream
    pub fn new(receiver: mpsc::Receiver<std::result::Result<EvmEvent, ChainError>>) -> Self {
        Self {
            receiver,
            config: EventStreamConfig::default(),
        }
    }

    /// Create with config
    pub fn with_config(
        receiver: mpsc::Receiver<std::result::Result<EvmEvent, ChainError>>,
        config: EventStreamConfig,
    ) -> Self {
        Self { receiver, config }
    }

    /// Get stream config
    pub fn config(&self) -> &EventStreamConfig {
        &self.config
    }

    /// Try to receive next event without blocking
    pub fn try_recv(&mut self) -> Option<std::result::Result<EvmEvent, ChainError>> {
        self.receiver.try_recv().ok()
    }

    /// Close the stream
    pub fn close(&mut self) {
        self.receiver.close();
    }
}

impl Stream for EventStream {
    type Item = std::result::Result<EvmEvent, ChainError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}

/// Event stream configuration
#[derive(Debug, Clone)]
pub struct EventStreamConfig {
    /// Buffer size for the channel
    pub buffer_size: usize,
    /// Timeout for receiving events
    pub recv_timeout: Option<std::time::Duration>,
    /// Auto-reconnect on error
    pub auto_reconnect: bool,
    /// Reconnect delay
    pub reconnect_delay: std::time::Duration,
    /// Maximum number of reconnection attempts
    pub max_reconnect_attempts: u32,
}

impl Default for EventStreamConfig {
    fn default() -> Self {
        Self {
            buffer_size: 1000,
            recv_timeout: None,
            auto_reconnect: true,
            reconnect_delay: std::time::Duration::from_secs(5),
            max_reconnect_attempts: 10,
        }
    }
}

impl EventStreamConfig {
    /// Set buffer size
    pub fn with_buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size;
        self
    }

    /// Set receive timeout
    pub fn with_recv_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.recv_timeout = Some(timeout);
        self
    }

    /// Set auto-reconnect
    pub fn with_auto_reconnect(mut self, enabled: bool) -> Self {
        self.auto_reconnect = enabled;
        self
    }

    /// Set reconnect delay
    pub fn with_reconnect_delay(mut self, delay: std::time::Duration) -> Self {
        self.reconnect_delay = delay;
        self
    }
}

/// Batched event stream for processing events in batches
pub struct BatchedEventStream {
    inner: EventStream,
    batch_size: usize,
    timeout: std::time::Duration,
}

impl BatchedEventStream {
    /// Create new batched stream
    pub fn new(inner: EventStream, batch_size: usize, timeout: std::time::Duration) -> Self {
        Self {
            inner,
            batch_size,
            timeout,
        }
    }

    /// Get next batch of events
    pub async fn next_batch(&mut self) -> Vec<EvmEvent> {
        let mut batch = Vec::with_capacity(self.batch_size);
        let deadline = tokio::time::Instant::now() + self.timeout;

        while batch.len() < self.batch_size {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());

            match tokio::time::timeout(remaining, self.inner.receiver.recv()).await {
                Ok(Some(Ok(event))) => batch.push(event),
                Ok(Some(Err(_))) => continue, // Skip errors
                Ok(None) => break,            // Channel closed
                Err(_) => break,              // Timeout
            }
        }

        batch
    }
}

/// Filtered event stream
pub struct FilteredEventStream<F> {
    inner: EventStream,
    filter: F,
}

impl<F> FilteredEventStream<F>
where
    F: Fn(&EvmEvent) -> bool,
{
    /// Create new filtered stream
    pub fn new(inner: EventStream, filter: F) -> Self {
        Self { inner, filter }
    }
}

impl<F> Stream for FilteredEventStream<F>
where
    F: Fn(&EvmEvent) -> bool + Unpin,
{
    type Item = std::result::Result<EvmEvent, ChainError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match self.inner.receiver.poll_recv(cx) {
                Poll::Ready(Some(Ok(event))) => {
                    if (self.filter)(&event) {
                        return Poll::Ready(Some(Ok(event)));
                    }
                    // Continue polling if filtered out
                }
                Poll::Ready(other) => return Poll::Ready(other),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Event stream transformers
pub mod transformers {
    use super::*;

    /// Transform events using a mapper function
    pub fn map<F, T>(_stream: EventStream, _mapper: F) -> MapStream<F>
    where
        F: Fn(EvmEvent) -> T,
    {
        MapStream { _mapper }
    }

    /// Placeholder for map stream
    pub struct MapStream<F> {
        _mapper: F,
    }

    impl<F> Stream for MapStream<F> {
        type Item = std::result::Result<EvmEvent, ChainError>;

        fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Pending
        }
    }

    /// Filter events
    pub fn filter<F>(stream: EventStream, filter: F) -> FilteredEventStream<F>
    where
        F: Fn(&EvmEvent) -> bool,
    {
        FilteredEventStream::new(stream, filter)
    }

    /// Batch events
    pub fn batch(
        stream: EventStream,
        size: usize,
        timeout: std::time::Duration,
    ) -> BatchedEventStream {
        BatchedEventStream::new(stream, size, timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_stream_config() {
        let config = EventStreamConfig::default()
            .with_buffer_size(500)
            .with_auto_reconnect(false);

        assert_eq!(config.buffer_size, 500);
        assert!(!config.auto_reconnect);
    }
}
