//! LLM Provider Admin API Handlers

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use gateway::error::GatewayError;
use gateway::middleware::{require_any_role, AuthUser};
use serde::{Deserialize, Serialize};

use crate::services::llm_provider_db as db;
use crate::AppState;

// ---- Request/Response DTOs ----

#[derive(Debug, Deserialize)]
pub struct CreateProviderRequest {
    pub provider_id: String,
    pub name: String,
    pub protocol: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProviderRequest {
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct AddModelRequest {
    pub name: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProviderResponse {
    pub id: i64,
    pub provider_id: String,
    pub name: String,
    pub protocol: String,
    pub base_url: Option<String>,
    pub api_key_masked: Option<String>,
    pub enabled: bool,
    pub is_default_provider: bool,
    pub icon: Option<String>,
    pub icon_color: Option<String>,
    pub type_label: Option<String>,
    pub models: Vec<ModelResponse>,
}

#[derive(Debug, Serialize)]
pub struct ModelResponse {
    pub id: i64,
    pub name: String,
    pub display_name: Option<String>,
    pub is_default_model: bool,
}

#[derive(Debug, Serialize)]
pub struct ProvidersListResponse {
    pub providers: Vec<ProviderResponse>,
}

// ---- Helper ----

fn mask_encrypted_key(encrypted: Option<&str>) -> Option<String> {
    encrypted.map(|_| "******".to_string())
}

// ---- Handlers ----

/// GET /api/v1/admin/llm/providers
pub async fn list_providers(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Result<Json<ProvidersListResponse>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    let rows = db::list_providers_with_models(&state.db)
        .await
        .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    let providers = rows
        .into_iter()
        .map(|(p, models)| ProviderResponse {
            id: p.id,
            provider_id: p.provider_id,
            name: p.name,
            protocol: p.protocol,
            base_url: p.base_url,
            api_key_masked: mask_encrypted_key(p.api_key_encrypted.as_deref()),
            enabled: p.enabled,
            is_default_provider: p.is_default_provider,
            icon: p.icon,
            icon_color: p.icon_color,
            type_label: p.type_label,
            models: models
                .into_iter()
                .map(|m| ModelResponse {
                    id: m.id,
                    name: m.name,
                    display_name: m.display_name,
                    is_default_model: m.is_default_model,
                })
                .collect(),
        })
        .collect();

    Ok(Json(ProvidersListResponse { providers }))
}

/// POST /api/v1/admin/llm/providers
pub async fn create_provider(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<CreateProviderRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    // Validate protocol
    if req.protocol != "openai-compatible" && req.protocol != "anthropic" {
        return Err(GatewayError::bad_request_field(
            "protocol must be 'openai-compatible' or 'anthropic'",
            "protocol",
        ));
    }

    // Encrypt API key if provided
    let api_key_encrypted = match req.api_key {
        Some(key) if !key.is_empty() => Some(
            state
                .encryption_service
                .encrypt(&key)
                .map_err(|e| GatewayError::internal(format!("Encryption failed: {}", e)))?,
        ),
        _ => None,
    };

    let id = db::create_provider(
        &state.db,
        &req.provider_id,
        &req.name,
        &req.protocol,
        req.base_url.as_deref(),
        api_key_encrypted.as_deref(),
    )
    .await
    .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    // Reload providers after creation
    state.llm_service.reload_providers().await.ok();

    Ok(Json(
        serde_json::json!({ "id": id, "message": "Provider created" }),
    ))
}

/// PUT /api/v1/admin/llm/providers/:id
pub async fn update_provider(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<UpdateProviderRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    // Encrypt API key if provided
    let api_key_encrypted = match req.api_key {
        Some(key) if !key.is_empty() => Some(
            state
                .encryption_service
                .encrypt(&key)
                .map_err(|e| GatewayError::internal(format!("Encryption failed: {}", e)))?,
        ),
        Some(_) => Some(String::new()), // empty string means clear
        None => None,                   // not provided means don't change
    };

    db::update_provider(
        &state.db,
        id,
        req.name.as_deref(),
        req.base_url.as_deref(),
        Some(api_key_encrypted.as_deref().unwrap_or_default()),
        req.enabled,
    )
    .await
    .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    // Reload providers after update
    state.llm_service.reload_providers().await.ok();

    Ok(Json(serde_json::json!({ "message": "Provider updated" })))
}

/// DELETE /api/v1/admin/llm/providers/:id
pub async fn delete_provider(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    db::delete_provider(&state.db, id)
        .await
        .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    // Reload providers after deletion
    state.llm_service.reload_providers().await.ok();

    Ok(Json(serde_json::json!({ "message": "Provider deleted" })))
}

/// POST /api/v1/admin/llm/providers/:id/models
pub async fn add_model(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(req): Json<AddModelRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    let model_id = db::add_model(&state.db, id, &req.name, req.display_name.as_deref())
        .await
        .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    Ok(Json(
        serde_json::json!({ "id": model_id, "message": "Model added" }),
    ))
}

/// DELETE /api/v1/admin/llm/providers/:id/models/:model_id
pub async fn delete_model(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path((_provider_id, model_id)): Path<(i64, i64)>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    db::delete_model(&state.db, model_id)
        .await
        .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    // Reload providers after model deletion
    state.llm_service.reload_providers().await.ok();

    Ok(Json(serde_json::json!({ "message": "Model deleted" })))
}

/// PUT /api/v1/admin/llm/providers/:id/default
pub async fn set_default_provider(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    db::set_default_provider(&state.db, id)
        .await
        .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    // Reload providers after setting default
    state.llm_service.reload_providers().await.ok();

    Ok(Json(
        serde_json::json!({ "message": "Default provider set" }),
    ))
}

/// PUT /api/v1/admin/llm/providers/:id/models/:model_id/default
pub async fn set_default_model(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path((provider_id, model_id)): Path<(i64, i64)>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    require_any_role(&user, &["admin"])?;

    db::set_default_model(&state.db, provider_id, model_id)
        .await
        .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    // Reload providers after setting default model
    state.llm_service.reload_providers().await.ok();

    Ok(Json(serde_json::json!({ "message": "Default model set" })))
}
