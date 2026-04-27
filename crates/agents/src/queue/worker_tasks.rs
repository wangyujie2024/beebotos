//! Worker Task Functions
//!
//! RELIABILITY FIX: Extracted worker task logic into standalone functions
//! that can be called both for initial spawn and for restart after panic.
//!
//! This allows the supervisor to restart workers without needing a reference
//! to the QueueManager.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use ::tracing::{debug, info, warn};
use beebotos_core::event::Event;
use beebotos_core::types::Timestamp;
use tokio::sync::{mpsc, Mutex, RwLock, Semaphore};
use tokio::task::JoinHandle;

use super::{QueueStats, QueueTask, TaskProcessor};
use crate::events::AgentEventBus;
#[allow(unused_imports)]
use crate::session::SessionKey;

/// RELIABILITY FIX: Spawn a main queue worker task
///
/// This function can be called both for initial spawn and for restart after
/// panic.
pub fn spawn_main_worker_task(
    rx: Arc<Mutex<mpsc::Receiver<QueueTask>>>,
    stats: Arc<RwLock<QueueStats>>,
    shutdown: Arc<tokio::sync::Notify>,
    event_bus: Option<AgentEventBus>,
    processor: Arc<dyn TaskProcessor>,
    restart_count: u32,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if restart_count > 0 {
            info!("Main queue worker restarted (restart #{})", restart_count);
        } else {
            info!("Main queue worker started");
        }

        loop {
            tokio::select! {
                _ = shutdown.notified() => {
                    info!("Main queue worker shutting down");
                    break;
                }
                task = async {
                    // Acquire lock inside the async block to allow cancellation
                    let mut rx = rx.lock().await;
                    rx.recv().await
                } => {
                    match task {
                        Some(task) => {
                            debug!("Main queue processing task {}", task.id);

                            // Publish event
                            if let Some(ref bus) = event_bus {
                                bus.emit(Event::TaskStarted {
                                    task_id: task.id.clone(),
                                    agent_id: None,
                                    timestamp: Timestamp::now(),
                                }).await;
                            }

                            // Process task
                            let result = processor.process(task.clone()).await;

                            // Update stats
                            {
                                let mut s = stats.write().await;
                                if result.success {
                                    s.main_queue_processed += 1;
                                } else {
                                    s.main_queue_failed += 1;
                                }
                            }

                            // Publish completion event
                            if let Some(ref bus) = event_bus {
                                bus.emit(Event::TaskCompleted {
                                    task_id: task.id.clone(),
                                    agent_id: None,
                                    success: result.success,
                                    timestamp: Timestamp::now(),
                                }).await;
                            }

                            debug!("Main queue completed task {}: success={}", task.id, result.success);
                        }
                        None => {
                            // Channel closed, exit
                            info!("Main queue channel closed, exiting");
                            break;
                        }
                    }
                }
            }
        }
    })
}

/// RELIABILITY FIX: Spawn a cron queue worker task
///
/// This function can be called both for initial spawn and for restart after
/// panic.
pub fn spawn_cron_worker_task(
    rx: Arc<Mutex<mpsc::Receiver<QueueTask>>>,
    stats: Arc<RwLock<QueueStats>>,
    shutdown: Arc<tokio::sync::Notify>,
    event_bus: Option<AgentEventBus>,
    processor: Arc<dyn TaskProcessor>,
    restart_count: u32,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if restart_count > 0 {
            info!("Cron queue worker restarted (restart #{})", restart_count);
        } else {
            info!("Cron queue worker started");
        }

        loop {
            tokio::select! {
                _ = shutdown.notified() => {
                    info!("Cron queue worker shutting down");
                    break;
                }
                task = async {
                    let mut rx = rx.lock().await;
                    rx.recv().await
                } => {
                    match task {
                        Some(task) => {
                            debug!("Cron queue processing task {}", task.id);

                            if let Some(ref bus) = event_bus {
                                bus.emit(Event::TaskStarted {
                                    task_id: task.id.clone(),
                                    agent_id: None,
                                    timestamp: Timestamp::now(),
                                }).await;
                            }

                            let result = processor.process(task.clone()).await;

                            {
                                let mut s = stats.write().await;
                                if result.success {
                                    s.cron_queue_processed += 1;
                                } else {
                                    s.cron_queue_failed += 1;
                                }
                            }

                            if let Some(ref bus) = event_bus {
                                bus.emit(Event::TaskCompleted {
                                    task_id: task.id.clone(),
                                    agent_id: None,
                                    success: result.success,
                                    timestamp: Timestamp::now(),
                                }).await;
                            }

                            debug!("Cron queue completed task {}: success={}", task.id, result.success);
                        }
                        None => {
                            info!("Cron queue channel closed, exiting");
                            break;
                        }
                    }
                }
            }
        }
    })
}

/// RELIABILITY FIX: Spawn a subagent queue worker task
///
/// This function can be called both for initial spawn and for restart after
/// panic.
///
/// CODE QUALITY FIX: Added queue_depth_counter parameter for accurate
/// queue depth tracking used by auto-scaling.
pub fn spawn_subagent_worker_task(
    rx: Arc<Mutex<mpsc::Receiver<QueueTask>>>,
    semaphore: Arc<Semaphore>,
    stats: Arc<RwLock<QueueStats>>,
    shutdown: Arc<tokio::sync::Notify>,
    event_bus: Option<AgentEventBus>,
    processor: Arc<dyn TaskProcessor>,
    worker_id: usize,
    restart_count: u32,
    queue_depth_counter: Option<Arc<AtomicUsize>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if restart_count > 0 {
            info!(
                "Subagent queue worker {} restarted (restart #{})",
                worker_id, restart_count
            );
        } else {
            info!("Subagent queue worker {} started", worker_id);
        }

        loop {
            tokio::select! {
                _ = shutdown.notified() => {
                    info!("Subagent queue worker {} shutting down", worker_id);
                    break;
                }
                task = async {
                    let mut rx = rx.lock().await;
                    rx.recv().await
                } => {
                    match task {
                        Some(task) => {
                            // CODE QUALITY FIX: Decrement queue depth counter
                            if let Some(ref counter) = queue_depth_counter {
                                counter.fetch_sub(1, Ordering::SeqCst);
                            }

                            let _permit = match semaphore.acquire().await {
                                Ok(p) => p,
                                Err(_) => {
                                    warn!("Subagent worker {}: semaphore closed", worker_id);
                                    break;
                                }
                            };

                            debug!("Subagent worker {} processing task {}", worker_id, task.id);

                            if let Some(ref bus) = event_bus {
                                bus.emit(Event::TaskStarted {
                                    task_id: task.id.clone(),
                                    agent_id: None,
                                    timestamp: Timestamp::now(),
                                }).await;
                            }

                            let result = processor.process(task.clone()).await;

                            {
                                let mut s = stats.write().await;
                                if result.success {
                                    s.subagent_queue_processed += 1;
                                } else {
                                    s.subagent_queue_failed += 1;
                                }
                            }

                            if let Some(ref bus) = event_bus {
                                bus.emit(Event::TaskCompleted {
                                    task_id: task.id.clone(),
                                    agent_id: None,
                                    success: result.success,
                                    timestamp: Timestamp::now(),
                                }).await;
                            }

                            debug!("Subagent worker {} completed task {}: success={}", worker_id, task.id, result.success);
                        }
                        None => {
                            info!("Subagent queue worker {}: channel closed, exiting", worker_id);
                            break;
                        }
                    }
                }
            }
        }
    })
}

/// RELIABILITY FIX: Spawn a nested queue worker task
///
/// This function can be called both for initial spawn and for restart after
/// panic.
pub fn spawn_nested_worker_task(
    rx: Arc<Mutex<mpsc::Receiver<QueueTask>>>,
    active: Arc<RwLock<HashMap<String, usize>>>,
    stats: Arc<RwLock<QueueStats>>,
    shutdown: Arc<tokio::sync::Notify>,
    event_bus: Option<AgentEventBus>,
    processor: Arc<dyn TaskProcessor>,
    restart_count: u32,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if restart_count > 0 {
            info!("Nested queue worker restarted (restart #{})", restart_count);
        } else {
            info!("Nested queue worker started");
        }

        loop {
            tokio::select! {
                _ = shutdown.notified() => {
                    info!("Nested queue worker shutting down");
                    break;
                }
                task = async {
                    let mut rx = rx.lock().await;
                    rx.recv().await
                } => {
                    match task {
                        Some(task) => {
                            let session_key = task.session_key.to_string();

                            // Track nesting depth
                            {
                                let mut act = active.write().await;
                                let depth = act.get(&session_key).copied().unwrap_or(0);
                                if depth >= 5 {
                                    warn!("Max nesting depth exceeded for session {}", session_key);
                                    continue;
                                }
                                act.insert(session_key.clone(), depth + 1);
                            }

                            debug!("Nested queue processing task {} at depth {}", task.id, {
                                active.read().await.get(&session_key).copied().unwrap_or(0)
                            });

                            if let Some(ref bus) = event_bus {
                                bus.emit(Event::TaskStarted {
                                    task_id: task.id.clone(),
                                    agent_id: None,
                                    timestamp: Timestamp::now(),
                                }).await;
                            }

                            let result = processor.process(task.clone()).await;

                            // Decrement nesting depth
                            {
                                let mut act = active.write().await;
                                let new_depth = act.get(&session_key).copied().unwrap_or(1) - 1;
                                if new_depth == 0 {
                                    act.remove(&session_key);
                                } else {
                                    act.insert(session_key, new_depth);
                                }
                            }

                            {
                                let mut s = stats.write().await;
                                if result.success {
                                    s.nested_queue_processed += 1;
                                } else {
                                    s.nested_queue_failed += 1;
                                }
                            }

                            if let Some(ref bus) = event_bus {
                                bus.emit(Event::TaskCompleted {
                                    task_id: task.id.clone(),
                                    agent_id: None,
                                    success: result.success,
                                    timestamp: Timestamp::now(),
                                }).await;
                            }

                            debug!("Nested queue completed task {}: success={}", task.id, result.success);
                        }
                        None => {
                            info!("Nested queue channel closed, exiting");
                            break;
                        }
                    }
                }
            }
        }
    })
}
