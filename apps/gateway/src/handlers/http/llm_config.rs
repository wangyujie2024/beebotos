//! LLM Global Configuration HTTP Handler
//!
//! Provides read-only access to the current global LLM configuration.
//! Sensitive fields (API keys) are masked for security.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use gateway::error::GatewayError;
use gateway::middleware::{require_any_role, AuthUser};
use serde::Serialize;

use crate::services::llm_provider_db as db;
use crate::AppState;

/// Global LLM configuration response
#[derive(Debug, Serialize)]
pub struct LlmGlobalConfigResponse {
    pub default_provider: String,
    pub providers: Vec<ProviderConfigResponse>,
}

/// Provider configuration (with masked API key)
#[derive(Debug, Serialize)]
pub struct ProviderConfigResponse {
    pub name: String,
    pub api_key_masked: String,
    pub model: String,
    pub base_url: String,
    pub protocol: String,
}

/// Get current global LLM configuration (read-only, sensitive fields masked)
pub async fn get_llm_global_config(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Result<Json<LlmGlobalConfigResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let providers = db::list_providers_with_models(&state.db)
        .await
        .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    let default_provider = providers
        .iter()
        .find(|(p, _)| p.is_default_provider)
        .map(|(p, _)| p.name.clone())
        .unwrap_or_else(|| "Not configured".to_string());

    let provider_responses = providers
        .into_iter()
        .map(|(provider, models)| {
            let default_model = models
                .iter()
                .find(|m| m.is_default_model)
                .map(|m| m.name.clone())
                .or_else(|| models.first().map(|m| m.name.clone()))
                .unwrap_or_default();

            ProviderConfigResponse {
                name: provider.name,
                api_key_masked: provider
                    .api_key_encrypted
                    .map(|_| "******".to_string())
                    .unwrap_or_default(),
                model: default_model,
                base_url: provider.base_url.unwrap_or_default(),
                protocol: provider.protocol,
            }
        })
        .collect();

    Ok(Json(LlmGlobalConfigResponse {
        default_provider,
        providers: provider_responses,
    }))
}
