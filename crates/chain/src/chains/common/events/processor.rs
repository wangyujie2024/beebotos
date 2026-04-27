//! Event Processor

use std::sync::Arc;

use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;

use crate::chains::common::events::{EventMetrics, EvmEvent, ProcessorConfig};
use crate::ChainError;

/// Event handler trait
#[async_trait::async_trait]
pub trait EventHandler: Send + Sync {
    /// Handle an event
    async fn handle(&self, event: EvmEvent) -> Result<(), ChainError>;

    /// Handle errors
    async fn handle_error(&self, error: ChainError) {
        tracing::error!("Event processing error: {}", error);
    }
}

/// Event processor
#[allow(dead_code)]
pub struct EventProcessor {
    _config: ProcessorConfig,
    _handler: Arc<dyn EventHandler>,
    metrics: Arc<RwLock<EventMetrics>>,
    tx: mpsc::Sender<EvmEvent>,
    workers: Vec<JoinHandle<()>>,
}

impl EventProcessor {
    /// Create new event processor
    pub fn new(config: ProcessorConfig, handler: Arc<dyn EventHandler>) -> Self {
        let (tx, rx) = mpsc::channel(config.channel_buffer);
        let rx = Arc::new(RwLock::new(rx));
        let metrics = Arc::new(RwLock::new(EventMetrics::default()));
        let mut workers = Vec::with_capacity(config.worker_count);

        // Spawn worker tasks
        for _ in 0..config.worker_count {
            let handler = handler.clone();
            let metrics = metrics.clone();
            let rx = rx.clone();

            let worker = tokio::spawn(async move {
                while let Some(event) = rx.write().await.recv().await {
                    let start = std::time::Instant::now();

                    match handler.handle(event).await {
                        Ok(_) => {
                            let elapsed = start.elapsed().as_millis() as f64;
                            let mut m = metrics.write().await;
                            m.events_processed += 1;
                            m.avg_processing_time_ms = (m.avg_processing_time_ms
                                * (m.events_processed - 1) as f64
                                + elapsed)
                                / m.events_processed as f64;
                        }
                        Err(e) => {
                            handler.handle_error(e).await;
                            let mut m = metrics.write().await;
                            m.errors += 1;
                        }
                    }
                }
            });

            workers.push(worker);
        }

        Self {
            _config: config,
            _handler: handler,
            metrics,
            tx,
            workers,
        }
    }

    /// Submit event for processing
    pub async fn submit(&self, event: EvmEvent) -> Result<(), ChainError> {
        self.tx
            .send(event)
            .await
            .map_err(|_| ChainError::Provider("Processor channel closed".into()))?;

        let mut metrics = self.metrics.write().await;
        metrics.events_received += 1;
        metrics.last_event_at = Some(std::time::SystemTime::now());

        Ok(())
    }

    /// Get current metrics
    pub async fn metrics(&self) -> EventMetrics {
        self.metrics.read().await.clone()
    }

    /// Shutdown processor
    pub async fn shutdown(self) {
        drop(self.tx);
        for worker in self.workers {
            let _ = worker.await;
        }
    }
}

/// Simple event handler for callbacks
pub struct CallbackHandler<F>
where
    F: Fn(EvmEvent) -> Result<(), ChainError> + Send + Sync,
{
    callback: F,
}

impl<F> CallbackHandler<F>
where
    F: Fn(EvmEvent) -> Result<(), ChainError> + Send + Sync,
{
    pub fn new(callback: F) -> Self {
        Self { callback }
    }
}

#[async_trait::async_trait]
impl<F> EventHandler for CallbackHandler<F>
where
    F: Fn(EvmEvent) -> Result<(), ChainError> + Send + Sync,
{
    async fn handle(&self, event: EvmEvent) -> Result<(), ChainError> {
        (self.callback)(event)
    }
}

/// Multi-handler that sends events to multiple handlers
pub struct MultiHandler {
    handlers: Vec<Arc<dyn EventHandler>>,
}

impl MultiHandler {
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    pub fn add_handler(mut self, handler: Arc<dyn EventHandler>) -> Self {
        self.handlers.push(handler);
        self
    }
}

#[async_trait::async_trait]
impl EventHandler for MultiHandler {
    async fn handle(&self, event: EvmEvent) -> Result<(), ChainError> {
        for handler in &self.handlers {
            if let Err(e) = handler.handle(event.clone()).await {
                handler.handle_error(e).await;
            }
        }
        Ok(())
    }
}

impl Default for MultiHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Event handler that filters events before processing
pub struct FilteredHandler<F, H>
where
    F: Fn(&EvmEvent) -> bool + Send + Sync,
    H: EventHandler,
{
    filter: F,
    handler: Arc<H>,
}

impl<F, H> FilteredHandler<F, H>
where
    F: Fn(&EvmEvent) -> bool + Send + Sync,
    H: EventHandler,
{
    pub fn new(filter: F, handler: Arc<H>) -> Self {
        Self { filter, handler }
    }
}

#[async_trait::async_trait]
impl<F, H> EventHandler for FilteredHandler<F, H>
where
    F: Fn(&EvmEvent) -> bool + Send + Sync,
    H: EventHandler,
{
    async fn handle(&self, event: EvmEvent) -> Result<(), ChainError> {
        if (self.filter)(&event) {
            self.handler.handle(event).await
        } else {
            Ok(())
        }
    }
}

/// Rate-limited event handler
pub struct RateLimitedHandler<H: EventHandler> {
    handler: Arc<H>,
    rate_limiter: tokio::sync::Semaphore,
}

impl<H: EventHandler> RateLimitedHandler<H> {
    pub fn new(handler: Arc<H>, max_concurrent: usize) -> Self {
        Self {
            handler,
            rate_limiter: tokio::sync::Semaphore::new(max_concurrent),
        }
    }
}

#[async_trait::async_trait]
impl<H: EventHandler> EventHandler for RateLimitedHandler<H> {
    async fn handle(&self, event: EvmEvent) -> Result<(), ChainError> {
        let _permit = self
            .rate_limiter
            .acquire()
            .await
            .map_err(|_| ChainError::Provider("Rate limiter closed".into()))?;

        self.handler.handle(event).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestHandler;

    #[async_trait::async_trait]
    impl EventHandler for TestHandler {
        async fn handle(&self, _event: EvmEvent) -> Result<(), ChainError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_callback_handler() {
        let handler = CallbackHandler::new(|_event| Ok(()));
        let event = EvmEvent {
            address: alloy_primitives::Address::ZERO,
            topics: vec![],
            data: vec![],
            block_number: 1,
            block_hash: alloy_primitives::B256::ZERO,
            transaction_hash: alloy_primitives::B256::ZERO,
            log_index: 0,
            transaction_index: 0,
            removed: false,
        };

        assert!(handler.handle(event).await.is_ok());
    }
}
