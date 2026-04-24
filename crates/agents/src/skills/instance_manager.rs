//! Skill Instance Manager
//!
//! Manages skill instance lifecycle: creation, status transitions, config
//! updates, and usage tracking. Each instance is a runtime binding of a Skill
//! to an Agent with independent configuration and state.

use std::collections::HashMap;

use tokio::sync::RwLock;

/// Instance lifecycle status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceStatus {
    Pending,
    Running,
    Paused,
    Stopped,
    Error,
}

impl InstanceStatus {
    /// Check if the instance can transition to the target status
    pub fn can_transition_to(&self, target: InstanceStatus) -> bool {
        use InstanceStatus::*;
        match (*self, target) {
            // From Pending: can start or error
            (Pending, Running) | (Pending, Error) => true,
            // From Running: can pause, stop, or error
            (Running, Paused) | (Running, Stopped) | (Running, Error) => true,
            // From Paused: can resume, stop, or error
            (Paused, Running) | (Paused, Stopped) | (Paused, Error) => true,
            // From Error: can only stop (cleanup)
            (Error, Stopped) => true,
            // Same state is a no-op
            (a, b) if a == b => true,
            _ => false,
        }
    }
}

/// Usage statistics for a skill instance
#[derive(Debug, Clone, Default)]
pub struct UsageStats {
    pub total_calls: u64,
    pub successful_calls: u64,
    pub failed_calls: u64,
    pub avg_latency_ms: f64,
    /// Running sum of latencies for incremental avg calculation
    latency_sum_ms: f64,
}

impl UsageStats {
    /// Record a new execution result and update averages
    pub fn record(&mut self, success: bool, latency_ms: f64) {
        self.total_calls += 1;
        if success {
            self.successful_calls += 1;
        } else {
            self.failed_calls += 1;
        }
        self.latency_sum_ms += latency_ms;
        self.avg_latency_ms = self.latency_sum_ms / self.total_calls as f64;
    }
}

/// A running or configured skill instance
#[derive(Debug, Clone)]
pub struct SkillInstance {
    pub instance_id: String,
    pub skill_id: String,
    pub agent_id: String,
    pub config: HashMap<String, String>,
    pub status: InstanceStatus,
    pub started_at: u64,
    pub last_active: u64,
    pub usage: UsageStats,
}

/// Filter for listing instances
#[derive(Debug, Clone, Default)]
pub struct InstanceFilter {
    pub agent_id: Option<String>,
    pub skill_id: Option<String>,
    pub status: Option<InstanceStatus>,
    pub page: usize,
    pub page_size: usize,
}

/// Maximum number of instances allowed
const DEFAULT_MAX_INSTANCES: usize = 1000;

/// Instance expiration threshold in seconds (1 hour of inactivity)
const DEFAULT_EXPIRY_SECONDS: u64 = 3600;

/// Thread-safe manager for skill instances
pub struct InstanceManager {
    instances: RwLock<HashMap<String, SkillInstance>>,
    max_instances: usize,
    expiry_seconds: u64,
}

impl InstanceManager {
    pub fn new() -> Self {
        Self {
            instances: RwLock::new(HashMap::new()),
            max_instances: DEFAULT_MAX_INSTANCES,
            expiry_seconds: DEFAULT_EXPIRY_SECONDS,
        }
    }

    /// Create with custom limits
    pub fn with_limits(max_instances: usize, expiry_seconds: u64) -> Self {
        Self {
            instances: RwLock::new(HashMap::new()),
            max_instances,
            expiry_seconds,
        }
    }

    /// Create a new instance. Returns the generated instance_id.
    /// If max_instances is exceeded, the oldest inactive instance is evicted.
    pub async fn create(
        &self,
        skill_id: impl Into<String>,
        agent_id: impl Into<String>,
        config: HashMap<String, String>,
    ) -> Result<String, InstanceError> {
        let mut instances = self.instances.write().await;

        // Enforce max_instances limit by evicting oldest inactive
        if instances.len() >= self.max_instances {
            let _now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let victim = instances
                .values()
                .filter(|i| i.status != InstanceStatus::Running)
                .min_by_key(|i| i.last_active)
                .map(|i| i.instance_id.clone());
            if let Some(id) = victim {
                instances.remove(&id);
            } else {
                return Err(InstanceError::LimitExceeded(
                    self.max_instances,
                    "All instances are active; cannot evict".to_string(),
                ));
            }
        }

        let instance_id = uuid::Uuid::new_v4().to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let instance = SkillInstance {
            instance_id: instance_id.clone(),
            skill_id: skill_id.into(),
            agent_id: agent_id.into(),
            config,
            status: InstanceStatus::Pending,
            started_at: now,
            last_active: now,
            usage: UsageStats::default(),
        };

        instances.insert(instance_id.clone(), instance);
        Ok(instance_id)
    }

    /// Get an instance by ID
    pub async fn get(&self, instance_id: &str) -> Option<SkillInstance> {
        let instances = self.instances.read().await;
        instances.get(instance_id).cloned()
    }

    /// Update instance status with validation
    pub async fn update_status(
        &self,
        instance_id: &str,
        new_status: InstanceStatus,
    ) -> Result<(), InstanceError> {
        let mut instances = self.instances.write().await;
        let instance = instances
            .get_mut(instance_id)
            .ok_or_else(|| InstanceError::NotFound(instance_id.to_string()))?;

        if !instance.status.can_transition_to(new_status) {
            return Err(InstanceError::InvalidTransition {
                from: format!("{:?}", instance.status),
                to: format!("{:?}", new_status),
            });
        }

        instance.status = new_status;
        instance.last_active = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(())
    }

    /// Update instance configuration
    pub async fn update_config(
        &self,
        instance_id: &str,
        updates: HashMap<String, String>,
    ) -> Result<(), InstanceError> {
        let mut instances = self.instances.write().await;
        let instance = instances
            .get_mut(instance_id)
            .ok_or_else(|| InstanceError::NotFound(instance_id.to_string()))?;

        for (k, v) in updates {
            instance.config.insert(k, v);
        }
        instance.last_active = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(())
    }

    /// Delete an instance
    pub async fn delete(&self, instance_id: &str) -> Result<SkillInstance, InstanceError> {
        let mut instances = self.instances.write().await;
        instances
            .remove(instance_id)
            .ok_or_else(|| InstanceError::NotFound(instance_id.to_string()))
    }

    /// List instances with optional filtering and pagination.
    pub async fn list(&self, filter: &InstanceFilter) -> Vec<SkillInstance> {
        let instances = self.instances.read().await;
        let mut results: Vec<SkillInstance> = instances
            .values()
            .filter(|i| {
                filter
                    .agent_id
                    .as_ref()
                    .map_or(true, |id| &i.agent_id == id)
                    && filter
                        .skill_id
                        .as_ref()
                        .map_or(true, |id| &i.skill_id == id)
                    && filter.status.map_or(true, |s| i.status == s)
            })
            .cloned()
            .collect();

        // Apply pagination if requested
        if filter.page_size > 0 {
            let offset = filter.page * filter.page_size;
            results = results
                .into_iter()
                .skip(offset)
                .take(filter.page_size)
                .collect();
        }
        results
    }

    /// Remove instances that have been inactive longer than expiry_seconds.
    /// Returns the number of cleaned-up instances.
    pub async fn cleanup_expired(&self) -> usize {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let cutoff = now.saturating_sub(self.expiry_seconds);

        let mut instances = self.instances.write().await;
        let expired_ids: Vec<String> = instances
            .values()
            .filter(|i| i.last_active < cutoff && i.status != InstanceStatus::Running)
            .map(|i| i.instance_id.clone())
            .collect();

        for id in &expired_ids {
            instances.remove(id);
        }
        expired_ids.len()
    }

    /// Record execution result for usage stats
    pub async fn record_execution(
        &self,
        instance_id: &str,
        success: bool,
        latency_ms: f64,
    ) -> Result<(), InstanceError> {
        let mut instances = self.instances.write().await;
        let instance = instances
            .get_mut(instance_id)
            .ok_or_else(|| InstanceError::NotFound(instance_id.to_string()))?;

        instance.usage.record(success, latency_ms);
        instance.last_active = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(())
    }

    /// Count total instances
    pub async fn count(&self) -> usize {
        let instances = self.instances.read().await;
        instances.len()
    }
}

impl Default for InstanceManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Instance management errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum InstanceError {
    #[error("Instance not found: {0}")]
    NotFound(String),
    #[error("Invalid status transition from {from} to {to}")]
    InvalidTransition { from: String, to: String },
    #[error("Instance limit exceeded (max {0}): {1}")]
    LimitExceeded(usize, String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_transitions() {
        use InstanceStatus::*;
        assert!(Pending.can_transition_to(Running));
        assert!(Pending.can_transition_to(Error));
        assert!(!Pending.can_transition_to(Paused));

        assert!(Running.can_transition_to(Paused));
        assert!(Running.can_transition_to(Stopped));
        assert!(Running.can_transition_to(Error));

        assert!(Paused.can_transition_to(Running));
        assert!(Paused.can_transition_to(Stopped));

        assert!(!Error.can_transition_to(Running));
        assert!(Error.can_transition_to(Stopped));
    }

    #[tokio::test]
    async fn test_instance_lifecycle() {
        let manager = InstanceManager::new();
        let mut config = HashMap::new();
        config.insert("key".to_string(), "value".to_string());

        let id = manager.create("skill-1", "agent-1", config).await.unwrap();
        assert_eq!(manager.count().await, 1);

        let instance = manager.get(&id).await.unwrap();
        assert_eq!(instance.skill_id, "skill-1");
        assert_eq!(instance.status, InstanceStatus::Pending);

        manager
            .update_status(&id, InstanceStatus::Running)
            .await
            .unwrap();
        let instance = manager.get(&id).await.unwrap();
        assert_eq!(instance.status, InstanceStatus::Running);

        manager
            .update_status(&id, InstanceStatus::Paused)
            .await
            .unwrap();
        manager
            .update_status(&id, InstanceStatus::Running)
            .await
            .unwrap();

        manager.delete(&id).await.unwrap();
        assert_eq!(manager.count().await, 0);
    }

    #[tokio::test]
    async fn test_invalid_transition() {
        let manager = InstanceManager::new();
        let id = manager
            .create("skill-1", "agent-1", HashMap::new())
            .await
            .unwrap();

        let result = manager.update_status(&id, InstanceStatus::Paused).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_usage_stats() {
        let manager = InstanceManager::new();
        let id = manager
            .create("skill-1", "agent-1", HashMap::new())
            .await
            .unwrap();

        manager.record_execution(&id, true, 100.0).await.unwrap();
        manager.record_execution(&id, false, 200.0).await.unwrap();
        manager.record_execution(&id, true, 300.0).await.unwrap();

        let instance = manager.get(&id).await.unwrap();
        assert_eq!(instance.usage.total_calls, 3);
        assert_eq!(instance.usage.successful_calls, 2);
        assert_eq!(instance.usage.failed_calls, 1);
        assert_eq!(instance.usage.avg_latency_ms, 200.0);
    }

    #[tokio::test]
    async fn test_list_filtering() {
        let manager = InstanceManager::new();
        let id1 = manager
            .create("skill-a", "agent-1", HashMap::new())
            .await
            .unwrap();
        let id2 = manager
            .create("skill-b", "agent-1", HashMap::new())
            .await
            .unwrap();
        let _id3 = manager
            .create("skill-a", "agent-2", HashMap::new())
            .await
            .unwrap();

        manager
            .update_status(&id1, InstanceStatus::Running)
            .await
            .unwrap();
        manager
            .update_status(&id2, InstanceStatus::Running)
            .await
            .unwrap();
        manager
            .update_status(&id2, InstanceStatus::Stopped)
            .await
            .unwrap();

        let filter = InstanceFilter {
            agent_id: Some("agent-1".to_string()),
            ..Default::default()
        };
        assert_eq!(manager.list(&filter).await.len(), 2);

        let filter = InstanceFilter {
            skill_id: Some("skill-a".to_string()),
            ..Default::default()
        };
        assert_eq!(manager.list(&filter).await.len(), 2);

        let filter = InstanceFilter {
            status: Some(InstanceStatus::Running),
            ..Default::default()
        };
        assert_eq!(manager.list(&filter).await.len(), 1);
    }
}
