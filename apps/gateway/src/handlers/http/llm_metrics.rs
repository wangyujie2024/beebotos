//! LLM Metrics Handler
//!
//! Provides endpoints for monitoring LLM service metrics.

use std::sync::Arc;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::AppState;

/// Get LLM service metrics
pub async fn get_llm_metrics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let summary = state.llm_service.get_metrics_summary();
    let provider_status = state.llm_service.get_provider_status().await;

    // Calculate success rate
    let total = summary.total_requests;
    let success_rate = if total > 0 {
        (summary.successful_requests as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    // Get latency percentiles
    let (p50, p95, p99) = state.llm_service.metrics().latency_percentiles().await;
    let avg_latency = state.llm_service.metrics().average_latency_ms().await;

    Json(json!({
        "summary": {
            "total_requests": summary.total_requests,
            "successful_requests": summary.successful_requests,
            "failed_requests": summary.failed_requests,
            "success_rate_percent": success_rate,
        },
        "tokens": {
            "total_tokens": summary.total_tokens,
            "input_tokens": summary.input_tokens,
            "output_tokens": summary.output_tokens,
        },
        "latency": {
            "average_ms": avg_latency,
            "p50_ms": p50,
            "p95_ms": p95,
            "p99_ms": p99,
        },
        "providers": provider_status.iter().map(|(name, healthy, failures)| {
            json!({
                "name": name,
                "healthy": healthy,
                "consecutive_failures": failures,
            })
        }).collect::<Vec<_>>(),
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}

/// Get LLM health status
pub async fn get_llm_health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.llm_service.health_check().await {
        Ok(_) => {
            let provider_status = state.llm_service.get_provider_status().await;

            Json(json!({
                "status": "healthy",
                "providers": provider_status.iter().map(|(name, healthy, failures)| {
                    json!({
                        "name": name,
                        "healthy": healthy,
                        "consecutive_failures": failures,
                    })
                }).collect::<Vec<_>>(),
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }))
        }
        Err(e) => Json(json!({
            "status": "unhealthy",
            "error": e.to_string(),
            "providers": [],
            "timestamp": chrono::Utc::now().to_rfc3339(),
        })),
    }
}

/// Reset LLM metrics (admin only)
pub async fn reset_llm_metrics(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Note: In production, this should require admin authentication
    // For now, we just return the metrics before reset
    let summary = state.llm_service.get_metrics_summary();

    Json(json!({
        "message": "Metrics reset is not implemented for security reasons",
        "previous_summary": summary,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}
