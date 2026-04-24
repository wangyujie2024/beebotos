//! Event Listener

use std::pin::Pin;
use std::task::{Context, Poll};

use alloy_provider::Provider;
use alloy_rpc_types::Log;
use tokio::sync::mpsc;
use tokio_stream::Stream;

use crate::chains::common::events::{EventFilter, EvmEvent};
use crate::{ChainError, Result};

/// Subscription type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionType {
    /// WebSocket subscription (real-time)
    WebSocket,
    /// Polling-based subscription
    Polling,
    /// HTTP streaming
    HttpStream,
}

/// Event listener
pub struct EventListener<P: Provider> {
    provider: P,
    filter: EventFilter,
    subscription_type: SubscriptionType,
}

impl<P: Provider + Clone + 'static> EventListener<P> {
    /// Create new event listener
    pub fn new(provider: P, filter: EventFilter) -> Self {
        Self {
            provider,
            filter,
            subscription_type: SubscriptionType::Polling,
        }
    }

    /// Set subscription type
    pub fn with_subscription_type(mut self, sub_type: SubscriptionType) -> Self {
        self.subscription_type = sub_type;
        self
    }

    /// Get historical logs
    pub async fn get_logs(&self) -> Result<Vec<Log>> {
        let filter = self.filter.to_alloy_filter();
        let logs = self
            .provider
            .get_logs(&filter)
            .await
            .map_err(|e| ChainError::Provider(format!("Failed to get logs: {}", e)))?;
        Ok(logs)
    }

    /// Stream events
    pub async fn stream(&self) -> Result<EventStream> {
        match self.subscription_type {
            SubscriptionType::WebSocket => self.stream_websocket().await,
            SubscriptionType::Polling => self.stream_polling().await,
            SubscriptionType::HttpStream => self.stream_http().await,
        }
    }

    /// Stream via WebSocket
    async fn stream_websocket(&self) -> Result<EventStream> {
        // WebSocket implementation would go here
        // For now, fall back to polling
        self.stream_polling().await
    }

    /// Stream via polling
    async fn stream_polling(&self) -> Result<EventStream> {
        let (tx, rx) = mpsc::channel(100);
        let provider = self.provider.clone();
        let filter = self.filter.clone();

        tokio::spawn(async move {
            let mut from_block = filter.get_from_block();

            loop {
                if let Some(block) = from_block {
                    let current_filter = filter.clone().from_block(block);

                    match provider.get_logs(&current_filter.to_alloy_filter()).await {
                        Ok(logs) => {
                            for log in logs {
                                let event: EvmEvent = log.into();
                                let block_num = event.block_number;

                                if tx.send(Ok(event)).await.is_err() {
                                    return;
                                }

                                // Update from_block to avoid duplicates
                                if let Some(next_block) = block_num.checked_add(1) {
                                    from_block = Some(next_block);
                                }
                            }
                        }
                        Err(e) => {
                            if tx
                                .send(Err(ChainError::Provider(e.to_string())))
                                .await
                                .is_err()
                            {
                                return;
                            }
                        }
                    }
                }

                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
        });

        Ok(EventStream::new(rx))
    }

    /// Stream via HTTP
    async fn stream_http(&self) -> Result<EventStream> {
        // HTTP streaming implementation
        self.stream_polling().await
    }
}

/// Event stream
pub struct EventStream {
    receiver: mpsc::Receiver<std::result::Result<EvmEvent, ChainError>>,
}

impl EventStream {
    fn new(receiver: mpsc::Receiver<std::result::Result<EvmEvent, ChainError>>) -> Self {
        Self { receiver }
    }
}

impl Stream for EventStream {
    type Item = std::result::Result<EvmEvent, ChainError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}

/// Multi-listener for listening to multiple filters
pub struct MultiListener<P: Provider> {
    listeners: Vec<EventListener<P>>,
}

impl<P: Provider + Clone + 'static> MultiListener<P> {
    pub fn new() -> Self {
        Self {
            listeners: Vec::new(),
        }
    }

    pub fn add_listener(mut self, listener: EventListener<P>) -> Self {
        self.listeners.push(listener);
        self
    }

    pub async fn get_all_logs(&self) -> Result<Vec<Log>> {
        let mut all_logs = Vec::new();

        for listener in &self.listeners {
            let logs = listener.get_logs().await?;
            all_logs.extend(logs);
        }

        // Sort by block number and log index
        all_logs.sort_by_key(|log| {
            (
                log.block_number.unwrap_or_default(),
                log.log_index.unwrap_or_default(),
            )
        });

        Ok(all_logs)
    }
}

impl<P: Provider + Clone + 'static> Default for MultiListener<P> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscription_type() {
        let sub = SubscriptionType::Polling;
        assert_eq!(sub, SubscriptionType::Polling);
    }
}
