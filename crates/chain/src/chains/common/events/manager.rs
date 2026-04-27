//! Event Manager Module
//!
//! Provides a centralized event management system with subscription handling,
//! automatic reconnection, and event routing.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use alloy_primitives::B256;
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, instrument, warn};

use crate::chains::common::events::{EventFilter, EventProcessor, EvmEvent, SubscriptionType};
use crate::chains::common::EvmProvider;
use crate::{ChainError, Result};

/// Unique subscription ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriptionId(pub u64);

impl SubscriptionId {
    /// Generate new unique ID
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for SubscriptionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Subscription configuration
#[derive(Debug, Clone)]
pub struct Subscription {
    pub id: SubscriptionId,
    pub filter: EventFilter,
    pub subscription_type: SubscriptionType,
    pub channel: mpsc::Sender<Result<EvmEvent>>,
    pub config: SubscriptionConfig,
}

/// Subscription-specific configuration
#[derive(Debug, Clone)]
pub struct SubscriptionConfig {
    /// Auto-reconnect on disconnect
    pub auto_reconnect: bool,
    /// Reconnect delay
    pub reconnect_delay: Duration,
    /// Maximum reconnection attempts (0 = unlimited)
    pub max_reconnect_attempts: u32,
    /// Buffer size for event channel
    pub buffer_size: usize,
}

impl Default for SubscriptionConfig {
    fn default() -> Self {
        Self {
            auto_reconnect: true,
            reconnect_delay: Duration::from_secs(5),
            max_reconnect_attempts: 10,
            buffer_size: 1000,
        }
    }
}

/// Event manager for handling multiple subscriptions
pub struct EventManager {
    provider: Arc<EvmProvider>,
    subscriptions: Arc<RwLock<HashMap<SubscriptionId, Subscription>>>,
    handles: Arc<RwLock<HashMap<SubscriptionId, JoinHandle<()>>>>,
    processor: Option<EventProcessor>,
}

impl EventManager {
    /// Create new event manager
    pub fn new(provider: Arc<EvmProvider>) -> Self {
        Self {
            provider,
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            handles: Arc::new(RwLock::new(HashMap::new())),
            processor: None,
        }
    }

    /// Set event processor
    pub fn with_processor(mut self, processor: EventProcessor) -> Self {
        self.processor = Some(processor);
        self
    }

    /// Subscribe to events
    #[instrument(skip(self, handler), target = "chain::events")]
    pub async fn subscribe<F>(
        &self,
        filter: EventFilter,
        config: SubscriptionConfig,
        mut handler: F,
    ) -> Result<SubscriptionId>
    where
        F: FnMut(EvmEvent) + Send + 'static,
    {
        let id = SubscriptionId::new();
        let (tx, mut rx) = mpsc::channel(config.buffer_size);

        let subscription = Subscription {
            id,
            filter: filter.clone(),
            subscription_type: SubscriptionType::Polling, // Default to polling
            channel: tx,
            config: config.clone(),
        };

        // Store subscription
        self.subscriptions.write().await.insert(id, subscription);

        // Start event handler task
        let handle = tokio::spawn(async move {
            while let Some(result) = rx.recv().await {
                match result {
                    Ok(event) => {
                        handler(event);
                    }
                    Err(e) => {
                        warn!(error = %e, "Event handler received error");
                    }
                }
            }
        });

        self.handles.write().await.insert(id, handle);

        // Start polling task for this subscription
        self.start_polling_task(id, filter, config).await?;

        info!(subscription_id = %id.0, "Event subscription created");
        Ok(id)
    }

    /// Subscribe with async handler
    #[instrument(skip(self, handler), target = "chain::events")]
    pub async fn subscribe_async<F, Fut>(
        &self,
        filter: EventFilter,
        config: SubscriptionConfig,
        mut handler: F,
    ) -> Result<SubscriptionId>
    where
        F: FnMut(EvmEvent) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send,
    {
        let id = SubscriptionId::new();
        let (tx, mut rx) = mpsc::channel(config.buffer_size);

        let subscription = Subscription {
            id,
            filter: filter.clone(),
            subscription_type: SubscriptionType::Polling,
            channel: tx,
            config: config.clone(),
        };

        self.subscriptions.write().await.insert(id, subscription);

        // Start async event handler task
        let handle = tokio::spawn(async move {
            while let Some(result) = rx.recv().await {
                match result {
                    Ok(event) => {
                        handler(event).await;
                    }
                    Err(e) => {
                        warn!(error = %e, "Event handler received error");
                    }
                }
            }
        });

        self.handles.write().await.insert(id, handle);
        self.start_polling_task(id, filter, config).await?;

        info!(subscription_id = %id.0, "Async event subscription created");
        Ok(id)
    }

    /// Start polling task for a subscription
    async fn start_polling_task(
        &self,
        id: SubscriptionId,
        filter: EventFilter,
        config: SubscriptionConfig,
    ) -> Result<()> {
        let provider = self.provider.clone();
        let subscriptions = self.subscriptions.clone();
        let mut reconnect_attempts = 0;

        tokio::spawn(async move {
            let mut from_block = filter.get_from_block();

            loop {
                // Check if subscription still exists
                if !subscriptions.read().await.contains_key(&id) {
                    debug!(subscription_id = %id.0, "Subscription removed, stopping poll task");
                    break;
                }

                if let Some(block) = from_block {
                    let current_filter = filter.clone().from_block(block);

                    match provider.get_logs(&current_filter.to_alloy_filter()).await {
                        Ok(logs) => {
                            reconnect_attempts = 0; // Reset on success

                            for log in logs {
                                let event: EvmEvent = log.into();
                                let block_num = event.block_number;

                                // Send to subscription channel
                                if let Some(sub) = subscriptions.read().await.get(&id) {
                                    if sub.channel.send(Ok(event)).await.is_err() {
                                        warn!(subscription_id = %id.0, "Failed to send event, channel closed");
                                        return;
                                    }
                                }

                                // Update from_block
                                if let Some(next_block) = block_num.checked_add(1) {
                                    from_block = Some(next_block);
                                }
                            }
                        }
                        Err(e) => {
                            error!(error = %e, subscription_id = %id.0, "Failed to poll events");

                            // Handle reconnection
                            if config.auto_reconnect {
                                reconnect_attempts += 1;

                                if config.max_reconnect_attempts > 0
                                    && reconnect_attempts >= config.max_reconnect_attempts
                                {
                                    error!(
                                        subscription_id = %id.0,
                                        attempts = reconnect_attempts,
                                        "Max reconnection attempts reached"
                                    );
                                    break;
                                }

                                tokio::time::sleep(config.reconnect_delay).await;
                                continue;
                            } else {
                                break;
                            }
                        }
                    }
                }

                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });

        Ok(())
    }

    /// Unsubscribe from events
    #[instrument(skip(self), target = "chain::events")]
    pub async fn unsubscribe(&self, id: SubscriptionId) -> Result<()> {
        // Remove subscription
        self.subscriptions.write().await.remove(&id);

        // Abort the handler task
        if let Some(handle) = self.handles.write().await.remove(&id) {
            handle.abort();
        }

        info!(subscription_id = %id.0, "Event subscription removed");
        Ok(())
    }

    /// Get all active subscriptions
    pub async fn active_subscriptions(&self) -> Vec<SubscriptionId> {
        self.subscriptions.read().await.keys().copied().collect()
    }

    /// Get subscription count
    pub async fn subscription_count(&self) -> usize {
        self.subscriptions.read().await.len()
    }

    /// Update filter for existing subscription
    pub async fn update_filter(&self, id: SubscriptionId, new_filter: EventFilter) -> Result<()> {
        let mut subs = self.subscriptions.write().await;
        if let Some(sub) = subs.get_mut(&id) {
            sub.filter = new_filter;
            Ok(())
        } else {
            Err(ChainError::Provider("Subscription not found".to_string()))
        }
    }

    /// Shutdown all subscriptions
    pub async fn shutdown(&self) {
        let ids: Vec<_> = self.subscriptions.read().await.keys().copied().collect();

        for id in ids {
            let _ = self.unsubscribe(id).await;
        }

        info!("Event manager shut down");
    }
}

/// Multi-chain event manager
pub struct MultiChainEventManager {
    managers: Arc<RwLock<HashMap<u64, Arc<EventManager>>>>,
}

impl MultiChainEventManager {
    /// Create new multi-chain event manager
    pub fn new() -> Self {
        Self {
            managers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a chain's event manager
    pub async fn register_chain(&self, chain_id: u64, manager: Arc<EventManager>) {
        self.managers.write().await.insert(chain_id, manager);
        info!(chain_id = chain_id, "Registered event manager for chain");
    }

    /// Subscribe to events on a specific chain
    pub async fn subscribe<F>(
        &self,
        chain_id: u64,
        filter: EventFilter,
        config: SubscriptionConfig,
        handler: F,
    ) -> Result<SubscriptionId>
    where
        F: FnMut(EvmEvent) + Send + 'static,
    {
        let managers = self.managers.read().await;
        let manager = managers.get(&chain_id).ok_or_else(|| {
            ChainError::Provider(format!("No event manager for chain {}", chain_id))
        })?;

        manager.subscribe(filter, config, handler).await
    }

    /// Unsubscribe from a chain
    pub async fn unsubscribe(&self, chain_id: u64, id: SubscriptionId) -> Result<()> {
        let managers = self.managers.read().await;
        let manager = managers.get(&chain_id).ok_or_else(|| {
            ChainError::Provider(format!("No event manager for chain {}", chain_id))
        })?;

        manager.unsubscribe(id).await
    }

    /// Get all chains with event managers
    pub async fn chains(&self) -> Vec<u64> {
        self.managers.read().await.keys().copied().collect()
    }
}

impl Default for MultiChainEventManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Event router for routing events to different handlers
pub struct EventRouter {
    routes: Arc<RwLock<Vec<(B256, mpsc::Sender<EvmEvent>)>>>,
}

impl EventRouter {
    /// Create new event router
    pub fn new() -> Self {
        Self {
            routes: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register a route for a specific event signature
    pub async fn register_route(&self, event_signature: B256, sender: mpsc::Sender<EvmEvent>) {
        self.routes.write().await.push((event_signature, sender));
    }

    /// Route an event to appropriate handlers
    pub async fn route(&self, event: EvmEvent) {
        if let Some(signature) = event.signature() {
            let routes = self.routes.read().await;
            for (sig, sender) in routes.iter() {
                if sig == signature {
                    let _ = sender.send(event.clone()).await;
                }
            }
        }
    }

    /// Remove all routes for a specific signature
    pub async fn remove_routes(&self, event_signature: B256) {
        let mut routes = self.routes.write().await;
        routes.retain(|(sig, _)| sig != &event_signature);
    }
}

impl Default for EventRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscription_id() {
        let id1 = SubscriptionId::new();
        let id2 = SubscriptionId::new();
        assert_ne!(id1.0, id2.0);
    }

    #[test]
    fn test_subscription_config_default() {
        let config = SubscriptionConfig::default();
        assert!(config.auto_reconnect);
        assert_eq!(config.max_reconnect_attempts, 10);
        assert_eq!(config.buffer_size, 1000);
    }

    #[tokio::test]
    async fn test_event_router() {
        let router = EventRouter::new();
        let (tx, mut rx) = mpsc::channel(10);
        let sig = B256::ZERO;

        router.register_route(sig, tx).await;

        // Create a test event
        let event = EvmEvent {
            address: alloy_primitives::Address::ZERO,
            topics: vec![sig],
            data: vec![],
            block_number: 1,
            block_hash: B256::ZERO,
            transaction_hash: B256::ZERO,
            log_index: 0,
            transaction_index: 0,
            removed: false,
        };

        router.route(event.clone()).await;

        let received = rx.recv().await;
        assert!(received.is_some());
    }
}
