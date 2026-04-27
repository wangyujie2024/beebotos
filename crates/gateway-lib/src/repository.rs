//! Repository Pattern
//!
//! Provides abstraction layer for database access.

use std::fmt::Debug;

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use tracing::{debug, error, instrument};

use crate::error::{GatewayError, Result};

/// Entity trait - all entities must implement this
pub trait Entity: Send + Sync + Debug + Clone + Serialize + DeserializeOwned + 'static {
    type Id: Send + Sync + Debug + Clone + ToString + 'static;
    fn id(&self) -> &Self::Id;
    fn entity_name() -> &'static str;
    fn field_names() -> Vec<&'static str>;
}

/// Pagination parameters
#[derive(Debug, Clone, Default)]
pub struct Pagination {
    pub page: usize,
    pub per_page: usize,
}

impl Pagination {
    pub fn new(page: usize, per_page: usize) -> Self {
        Self { page, per_page }
    }
    pub fn offset(&self) -> usize {
        (self.page.saturating_sub(1)) * self.per_page
    }
    pub fn limit(&self) -> usize {
        self.per_page
    }
}

/// Query filter
#[derive(Debug, Clone, Default)]
pub struct QueryFilter {
    pub conditions: Vec<FilterCondition>,
    pub order_by: Option<(String, SortOrder)>,
    pub pagination: Option<Pagination>,
}

/// Filter condition
#[derive(Debug, Clone)]
pub struct FilterCondition {
    pub field: String,
    pub operator: FilterOperator,
    pub value: FilterValue,
}

/// Filter operators
#[derive(Debug, Clone)]
pub enum FilterOperator {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    Like,
    In,
    IsNull,
    IsNotNull,
}

/// Filter values
#[derive(Debug, Clone)]
pub enum FilterValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    VecString(Vec<String>),
    Null,
}

/// Sort order
#[derive(Debug, Clone)]
pub enum SortOrder {
    Asc,
    Desc,
}

/// Query result with pagination
#[derive(Debug, Clone)]
pub struct PaginatedResult<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub page: usize,
    pub per_page: usize,
    pub total_pages: usize,
}

/// Repository trait
#[async_trait]
pub trait Repository<T: Entity>: Send + Sync {
    async fn find_by_id(&self, id: &T::Id) -> Result<Option<T>>;
    async fn find_all(&self) -> Result<Vec<T>>;
    async fn find_paginated(&self, pagination: Pagination) -> Result<PaginatedResult<T>>;
    async fn find_by_filter(&self, filter: QueryFilter) -> Result<PaginatedResult<T>>;
    async fn save(&self, entity: &T) -> Result<T>;
    async fn insert(&self, entity: &T) -> Result<T>;
    async fn update(&self, entity: &T) -> Result<T>;
    async fn delete(&self, id: &T::Id) -> Result<bool>;
    async fn delete_by_filter(&self, filter: QueryFilter) -> Result<u64>;
    async fn count(&self) -> Result<i64>;
    async fn count_by_filter(&self, filter: QueryFilter) -> Result<i64>;
    async fn exists(&self, id: &T::Id) -> Result<bool>;
}

/// SQLite repository implementation
pub struct PgRepository<T: Entity> {
    pool: SqlitePool,
    table_name: String,
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Entity> PgRepository<T> {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            table_name: T::entity_name().to_string(),
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn with_table_name(pool: SqlitePool, table_name: impl Into<String>) -> Self {
        Self {
            pool,
            table_name: table_name.into(),
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    fn build_select_query(&self) -> String {
        format!("SELECT * FROM {} WHERE id = ?1", self.table_name)
    }

    fn build_delete_query(&self) -> String {
        format!("DELETE FROM {} WHERE id = ?1", self.table_name)
    }
}

#[async_trait]
impl<T: Entity> Repository<T> for PgRepository<T> {
    #[instrument(skip(self), fields(entity_id = %id.to_string()))]
    async fn find_by_id(&self, id: &T::Id) -> Result<Option<T>> {
        let query = self.build_select_query();
        let id_str = id.to_string();

        debug!(query = %query, id = %id_str, "Finding entity by ID");

        let row = sqlx::query(&query)
            .bind(&id_str)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                error!(error = %e, "Database error in find_by_id");
                GatewayError::internal(format!("Database error: {}", e))
            })?;

        match row {
            Some(row) => {
                let json: serde_json::Value = row
                    .try_get("data")
                    .or_else(|_| row.try_get("json"))
                    .map_err(|e| GatewayError::internal(format!("Failed to deserialize: {}", e)))?;

                let entity: T = serde_json::from_value(json).map_err(|e| {
                    GatewayError::internal(format!("Failed to parse entity: {}", e))
                })?;

                Ok(Some(entity))
            }
            None => Ok(None),
        }
    }

    async fn find_all(&self) -> Result<Vec<T>> {
        let query = format!("SELECT * FROM {} ORDER BY id", self.table_name);

        let rows = sqlx::query(&query)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

        let mut entities = Vec::new();
        for row in rows {
            let json: serde_json::Value = row
                .try_get("data")
                .map_err(|e| GatewayError::internal(format!("Failed to deserialize: {}", e)))?;

            let entity: T = serde_json::from_value(json)
                .map_err(|e| GatewayError::internal(format!("Failed to parse entity: {}", e)))?;

            entities.push(entity);
        }

        Ok(entities)
    }

    async fn find_paginated(&self, pagination: Pagination) -> Result<PaginatedResult<T>> {
        let total = self.count().await?;

        let query = format!(
            "SELECT * FROM {} ORDER BY id LIMIT {} OFFSET {}",
            self.table_name,
            pagination.limit(),
            pagination.offset()
        );

        let rows = sqlx::query(&query)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

        let mut items = Vec::new();
        for row in rows {
            let json: serde_json::Value = row
                .try_get("data")
                .map_err(|e| GatewayError::internal(format!("Failed to deserialize: {}", e)))?;

            let entity: T = serde_json::from_value(json)
                .map_err(|e| GatewayError::internal(format!("Failed to parse entity: {}", e)))?;

            items.push(entity);
        }

        let total_pages = ((total as f64) / (pagination.per_page as f64)).ceil() as usize;

        Ok(PaginatedResult {
            items,
            total: total as usize,
            page: pagination.page,
            per_page: pagination.per_page,
            total_pages,
        })
    }

    async fn find_by_filter(&self, _filter: QueryFilter) -> Result<PaginatedResult<T>> {
        // Simplified implementation
        self.find_paginated(Pagination::default()).await
    }

    async fn save(&self, entity: &T) -> Result<T> {
        if self.exists(entity.id()).await? {
            self.update(entity).await
        } else {
            self.insert(entity).await
        }
    }

    async fn insert(&self, entity: &T) -> Result<T> {
        let query = format!(
            "INSERT INTO {} (id, data, created_at, updated_at) VALUES (?1, ?2, datetime('now'), \
             datetime('now'))",
            self.table_name
        );

        let data = serde_json::to_value(entity)
            .map_err(|e| GatewayError::internal(format!("Failed to serialize entity: {}", e)))?;

        sqlx::query(&query)
            .bind(entity.id().to_string())
            .bind(data)
            .execute(&self.pool)
            .await
            .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

        // Return the entity as SQLite doesn't support RETURNING * the same way
        Ok(entity.clone())
    }

    async fn update(&self, entity: &T) -> Result<T> {
        let query = format!(
            "UPDATE {} SET data = ?2, updated_at = datetime('now') WHERE id = ?1",
            self.table_name
        );

        let data = serde_json::to_value(entity)
            .map_err(|e| GatewayError::internal(format!("Failed to serialize entity: {}", e)))?;

        sqlx::query(&query)
            .bind(entity.id().to_string())
            .bind(data)
            .execute(&self.pool)
            .await
            .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

        // Return the entity as SQLite doesn't support RETURNING * the same way
        Ok(entity.clone())
    }

    async fn delete(&self, id: &T::Id) -> Result<bool> {
        let query = self.build_delete_query();

        let result = sqlx::query(&query)
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    async fn delete_by_filter(&self, _filter: QueryFilter) -> Result<u64> {
        // Simplified - would build DELETE with WHERE clause
        Ok(0)
    }

    async fn count(&self) -> Result<i64> {
        let query = format!("SELECT COUNT(*) FROM {}", self.table_name);

        let count: i64 = sqlx::query_scalar(&query)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

        Ok(count)
    }

    async fn count_by_filter(&self, _filter: QueryFilter) -> Result<i64> {
        // Simplified
        self.count().await
    }

    async fn exists(&self, id: &T::Id) -> Result<bool> {
        let query = format!(
            "SELECT EXISTS(SELECT 1 FROM {} WHERE id = ?1)",
            self.table_name
        );

        let exists: bool = sqlx::query_scalar(&query)
            .bind(id.to_string())
            .fetch_one(&self.pool)
            .await
            .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

        Ok(exists)
    }
}

/// Mock repository for testing
#[cfg(test)]
use std::collections::HashMap;

#[cfg(test)]
pub struct MockRepository<T: Entity> {
    data: std::sync::Mutex<HashMap<String, T>>,
}

#[cfg(test)]
impl<T: Entity> MockRepository<T> {
    pub fn new() -> Self {
        Self {
            data: std::sync::Mutex::new(HashMap::new()),
        }
    }
}

#[cfg(test)]
#[async_trait]
impl<T: Entity> Repository<T> for MockRepository<T> {
    async fn find_by_id(&self, id: &T::Id) -> Result<Option<T>> {
        let data = self.data.lock().unwrap();
        Ok(data.get(&id.to_string()).cloned())
    }

    async fn find_all(&self) -> Result<Vec<T>> {
        let data = self.data.lock().unwrap();
        Ok(data.values().cloned().collect())
    }

    async fn find_paginated(&self, pagination: Pagination) -> Result<PaginatedResult<T>> {
        let all = self.find_all().await?;
        let total = all.len();
        let offset = pagination.offset();
        let limit = pagination.limit();

        let items: Vec<T> = all.into_iter().skip(offset).take(limit).collect();

        let total_pages = ((total as f64) / (pagination.per_page as f64)).ceil() as usize;

        Ok(PaginatedResult {
            items,
            total,
            page: pagination.page,
            per_page: pagination.per_page,
            total_pages,
        })
    }

    async fn find_by_filter(&self, _filter: QueryFilter) -> Result<PaginatedResult<T>> {
        self.find_paginated(Pagination::default()).await
    }

    async fn save(&self, entity: &T) -> Result<T> {
        self.insert(entity).await
    }

    async fn insert(&self, entity: &T) -> Result<T> {
        let mut data = self.data.lock().unwrap();
        data.insert(entity.id().to_string(), entity.clone());
        Ok(entity.clone())
    }

    async fn update(&self, entity: &T) -> Result<T> {
        self.insert(entity).await
    }

    async fn delete(&self, id: &T::Id) -> Result<bool> {
        let mut data = self.data.lock().unwrap();
        Ok(data.remove(&id.to_string()).is_some())
    }

    async fn delete_by_filter(&self, _filter: QueryFilter) -> Result<u64> {
        Ok(0)
    }

    async fn count(&self) -> Result<i64> {
        let data = self.data.lock().unwrap();
        Ok(data.len() as i64)
    }

    async fn count_by_filter(&self, _filter: QueryFilter) -> Result<i64> {
        self.count().await
    }

    async fn exists(&self, id: &T::Id) -> Result<bool> {
        let data = self.data.lock().unwrap();
        Ok(data.contains_key(&id.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde::Deserialize;

    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestEntity {
        id: String,
        name: String,
    }

    impl Entity for TestEntity {
        type Id = String;
        fn id(&self) -> &Self::Id {
            &self.id
        }
        fn entity_name() -> &'static str {
            "test_entities"
        }
        fn field_names() -> Vec<&'static str> {
            vec!["id", "name"]
        }
    }

    #[test]
    fn test_pagination() {
        let pagination = Pagination::new(2, 10);
        assert_eq!(pagination.offset(), 10);
        assert_eq!(pagination.limit(), 10);
    }
}
