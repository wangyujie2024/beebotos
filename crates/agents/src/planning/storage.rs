//! Plan Storage Module
//!
//! ARCHITECTURE FIX: Provides persistence interface for plans with TTL support.
//! This allows plans to be stored beyond process lifetime and recovered after
//! restarts.

use std::collections::HashMap;

use async_trait::async_trait;

use super::{Plan, PlanId};

/// Plan storage result type
pub type PlanStorageResult<T> = Result<T, PlanStorageError>;

/// Plan storage errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum PlanStorageError {
    #[error("Storage backend error: {0}")]
    BackendError(String),

    #[error("Plan not found: {0}")]
    NotFound(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Storage quota exceeded")]
    QuotaExceeded,

    #[error("TTL expired")]
    TtlExpired,
}

/// Plan storage interface
///
/// ARCHITECTURE FIX: Abstract storage interface allows multiple backends:
/// - In-memory (for testing/single-node)
/// - File-based (for simple deployments)
/// - Database (for production/HA deployments)
#[async_trait]
pub trait PlanStorage: Send + Sync {
    /// Store a plan
    async fn store(&self, plan: &Plan) -> PlanStorageResult<()>;

    /// Retrieve a plan by ID
    async fn retrieve(&self, plan_id: &PlanId) -> PlanStorageResult<Plan>;

    /// Delete a plan
    async fn delete(&self, plan_id: &PlanId) -> PlanStorageResult<()>;

    /// List all plans (with optional filter)
    async fn list(&self, filter: Option<PlanFilter>) -> PlanStorageResult<Vec<Plan>>;

    /// Clean up expired plans based on TTL
    async fn cleanup_expired(&self) -> PlanStorageResult<usize>;

    /// Get storage statistics
    async fn stats(&self) -> PlanStorageResult<StorageStats>;
}

/// Filter for listing plans
#[derive(Debug, Clone, Default)]
pub struct PlanFilter {
    /// Filter by status
    pub status: Option<super::PlanStatus>,
    /// Filter by agent ID
    pub agent_id: Option<String>,
    /// Only plans created after this time
    pub created_after: Option<chrono::DateTime<chrono::Utc>>,
    /// Only plans created before this time
    pub created_before: Option<chrono::DateTime<chrono::Utc>>,
    /// Maximum number of results
    pub limit: Option<usize>,
}

/// Storage statistics
#[derive(Debug, Clone, Default)]
pub struct StorageStats {
    /// Total number of plans stored
    pub total_plans: usize,
    /// Number of active plans
    pub active_plans: usize,
    /// Number of expired plans
    pub expired_plans: usize,
    /// Storage size in bytes (approximate)
    pub storage_size_bytes: usize,
}

/// In-memory plan storage implementation
///
/// ARCHITECTURE FIX: Simple in-memory storage with TTL cleanup.
/// For production, consider using a persistent backend.
pub struct InMemoryPlanStorage {
    plans: tokio::sync::RwLock<HashMap<PlanId, Plan>>,
    max_plans: usize,
}

impl InMemoryPlanStorage {
    /// Create new in-memory storage with capacity limit
    pub fn new(max_plans: usize) -> Self {
        Self {
            plans: tokio::sync::RwLock::new(HashMap::new()),
            max_plans,
        }
    }

    /// Create with default capacity (10000 plans)
    pub fn default() -> Self {
        Self::new(10000)
    }
}

#[async_trait]
impl PlanStorage for InMemoryPlanStorage {
    async fn store(&self, plan: &Plan) -> PlanStorageResult<()> {
        let mut plans = self.plans.write().await;

        // Check capacity
        if plans.len() >= self.max_plans && !plans.contains_key(&plan.id) {
            return Err(PlanStorageError::QuotaExceeded);
        }

        plans.insert(plan.id.clone(), plan.clone());
        Ok(())
    }

    async fn retrieve(&self, plan_id: &PlanId) -> PlanStorageResult<Plan> {
        let mut plans = self.plans.write().await;

        if let Some(mut plan) = plans.get(plan_id).cloned() {
            // Check if expired
            if plan.is_expired() {
                plans.remove(plan_id);
                return Err(PlanStorageError::TtlExpired);
            }

            // Update last accessed time
            plan.touch();
            plans.insert(plan_id.clone(), plan.clone());

            Ok(plan)
        } else {
            Err(PlanStorageError::NotFound(plan_id.to_string()))
        }
    }

    async fn delete(&self, plan_id: &PlanId) -> PlanStorageResult<()> {
        let mut plans = self.plans.write().await;
        plans.remove(plan_id);
        Ok(())
    }

    async fn list(&self, filter: Option<PlanFilter>) -> PlanStorageResult<Vec<Plan>> {
        let plans = self.plans.read().await;

        let mut results: Vec<Plan> = plans.values().cloned().collect();

        // Apply filters
        if let Some(filter) = filter {
            if let Some(status) = filter.status {
                results.retain(|p| p.status == status);
            }
            if let Some(created_after) = filter.created_after {
                results.retain(|p| p.created_at >= created_after);
            }
            if let Some(created_before) = filter.created_before {
                results.retain(|p| p.created_at <= created_before);
            }
            if let Some(limit) = filter.limit {
                results.truncate(limit);
            }
        }

        Ok(results)
    }

    async fn cleanup_expired(&self) -> PlanStorageResult<usize> {
        let mut plans = self.plans.write().await;
        let expired: Vec<PlanId> = plans
            .values()
            .filter(|p| p.is_expired())
            .map(|p| p.id.clone())
            .collect();

        let count = expired.len();
        for id in expired {
            plans.remove(&id);
        }

        Ok(count)
    }

    async fn stats(&self) -> PlanStorageResult<StorageStats> {
        let plans = self.plans.read().await;

        let total_plans = plans.len();
        let active_plans = plans.values().filter(|p| p.status.is_active()).count();
        let expired_plans = plans.values().filter(|p| p.is_expired()).count();

        // Approximate size
        let storage_size_bytes = plans
            .values()
            .map(|p| serde_json::to_string(p).map(|s| s.len()).unwrap_or(0))
            .sum();

        Ok(StorageStats {
            total_plans,
            active_plans,
            expired_plans,
            storage_size_bytes,
        })
    }
}

/// File-based plan storage
///
/// ARCHITECTURE FIX: Persistent storage using filesystem.
/// Each plan is stored as a separate JSON file.
pub struct FilePlanStorage {
    base_path: std::path::PathBuf,
    #[allow(dead_code)]
    max_plans: usize,
}

impl FilePlanStorage {
    /// Create new file-based storage
    pub fn new(base_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
            max_plans: 10000,
        }
    }

    /// Get file path for a plan
    fn plan_path(&self, plan_id: &PlanId) -> std::path::PathBuf {
        self.base_path.join(format!("{}.json", plan_id))
    }
}

#[async_trait]
impl PlanStorage for FilePlanStorage {
    async fn store(&self, plan: &Plan) -> PlanStorageResult<()> {
        // Ensure directory exists
        tokio::fs::create_dir_all(&self.base_path)
            .await
            .map_err(|e| PlanStorageError::BackendError(e.to_string()))?;

        let path = self.plan_path(&plan.id);
        let json = serde_json::to_string_pretty(plan)
            .map_err(|e| PlanStorageError::SerializationError(e.to_string()))?;

        tokio::fs::write(&path, json)
            .await
            .map_err(|e| PlanStorageError::BackendError(e.to_string()))?;

        Ok(())
    }

    async fn retrieve(&self, plan_id: &PlanId) -> PlanStorageResult<Plan> {
        let path = self.plan_path(plan_id);

        let json = tokio::fs::read_to_string(&path)
            .await
            .map_err(|_| PlanStorageError::NotFound(plan_id.to_string()))?;

        let plan: Plan = serde_json::from_str(&json)
            .map_err(|e| PlanStorageError::SerializationError(e.to_string()))?;

        if plan.is_expired() {
            let _ = tokio::fs::remove_file(&path).await;
            return Err(PlanStorageError::TtlExpired);
        }

        Ok(plan)
    }

    async fn delete(&self, plan_id: &PlanId) -> PlanStorageResult<()> {
        let path = self.plan_path(plan_id);
        tokio::fs::remove_file(&path)
            .await
            .map_err(|e| PlanStorageError::BackendError(e.to_string()))?;
        Ok(())
    }

    async fn list(&self, filter: Option<PlanFilter>) -> PlanStorageResult<Vec<Plan>> {
        let mut plans = Vec::new();

        let mut entries = tokio::fs::read_dir(&self.base_path)
            .await
            .map_err(|e| PlanStorageError::BackendError(e.to_string()))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| PlanStorageError::BackendError(e.to_string()))?
        {
            if let Some(ext) = entry.path().extension() {
                if ext == "json" {
                    if let Ok(json) = tokio::fs::read_to_string(entry.path()).await {
                        if let Ok(plan) = serde_json::from_str::<Plan>(&json) {
                            plans.push(plan);
                        }
                    }
                }
            }
        }

        // Apply filters
        if let Some(filter) = filter {
            if let Some(status) = filter.status {
                plans.retain(|p| p.status == status);
            }
            if let Some(created_after) = filter.created_after {
                plans.retain(|p| p.created_at >= created_after);
            }
            if let Some(created_before) = filter.created_before {
                plans.retain(|p| p.created_at <= created_before);
            }
            if let Some(limit) = filter.limit {
                plans.truncate(limit);
            }
        }

        Ok(plans)
    }

    async fn cleanup_expired(&self) -> PlanStorageResult<usize> {
        let mut count = 0;

        let mut entries = tokio::fs::read_dir(&self.base_path)
            .await
            .map_err(|e| PlanStorageError::BackendError(e.to_string()))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| PlanStorageError::BackendError(e.to_string()))?
        {
            if let Some(ext) = entry.path().extension() {
                if ext == "json" {
                    if let Ok(json) = tokio::fs::read_to_string(entry.path()).await {
                        if let Ok(plan) = serde_json::from_str::<Plan>(&json) {
                            if plan.is_expired() {
                                let _ = tokio::fs::remove_file(entry.path()).await;
                                count += 1;
                            }
                        }
                    }
                }
            }
        }

        Ok(count)
    }

    async fn stats(&self) -> PlanStorageResult<StorageStats> {
        let mut total_plans = 0;
        let mut active_plans = 0;
        let mut expired_plans = 0;
        let mut storage_size_bytes = 0;

        let mut entries = tokio::fs::read_dir(&self.base_path)
            .await
            .map_err(|e| PlanStorageError::BackendError(e.to_string()))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| PlanStorageError::BackendError(e.to_string()))?
        {
            if let Some(ext) = entry.path().extension() {
                if ext == "json" {
                    if let Ok(metadata) = entry.metadata().await {
                        storage_size_bytes += metadata.len() as usize;
                    }
                    if let Ok(json) = tokio::fs::read_to_string(entry.path()).await {
                        if let Ok(plan) = serde_json::from_str::<Plan>(&json) {
                            total_plans += 1;
                            if plan.status.is_active() {
                                active_plans += 1;
                            }
                            if plan.is_expired() {
                                expired_plans += 1;
                            }
                        }
                    }
                }
            }
        }

        Ok(StorageStats {
            total_plans,
            active_plans,
            expired_plans,
            storage_size_bytes,
        })
    }
}
