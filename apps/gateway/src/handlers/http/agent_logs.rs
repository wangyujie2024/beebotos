//! Agent Logs HTTP Handler
//!
//! Provides query access to agent activity logs stored in SQLite.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use gateway::error::GatewayError;
use gateway::middleware::{require_any_role, AuthUser};
use serde::{Deserialize, Serialize};

use crate::AppState;

/// Log entry response
#[derive(Debug, Serialize)]
pub struct LogEntry {
    pub id: i64,
    pub agent_id: String,
    pub level: String,
    pub message: String,
    pub source: Option<String>,
    pub timestamp: String,
}

/// Query parameters for log listing
#[derive(Debug, Deserialize)]
pub struct LogQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub level: Option<String>,
}

/// Get logs for an agent
pub async fn get_agent_logs(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(agent_id): Path<String>,
    Query(query): Query<LogQuery>,
) -> Result<Json<Vec<LogEntry>>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let limit = query.limit.unwrap_or(100).min(1000);

    let rows: Vec<LogRow> = if let Some(level) = query.level {
        sqlx::query_as(
            "SELECT id, agent_id, level, message, source, timestamp
             FROM agent_logs
             WHERE agent_id = ?1 AND level = ?2
             ORDER BY timestamp DESC
             LIMIT ?3",
        )
        .bind(&agent_id)
        .bind(level)
        .bind(limit)
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query_as(
            "SELECT id, agent_id, level, message, source, timestamp
             FROM agent_logs
             WHERE agent_id = ?1
             ORDER BY timestamp DESC
             LIMIT ?2",
        )
        .bind(&agent_id)
        .bind(limit)
        .fetch_all(&state.db)
        .await
    }
    .map_err(|e| GatewayError::internal(format!("Failed to query logs: {}", e)))?;

    let logs = rows
        .into_iter()
        .map(|r| LogEntry {
            id: r.id,
            agent_id: r.agent_id,
            level: r.level,
            message: r.message,
            source: r.source,
            timestamp: r.timestamp,
        })
        .collect();

    Ok(Json(logs))
}

#[derive(sqlx::FromRow)]
struct LogRow {
    id: i64,
    agent_id: String,
    level: String,
    message: String,
    source: Option<String>,
    timestamp: String,
}
