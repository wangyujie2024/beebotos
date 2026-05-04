//! LLM Global Configuration HTTP Handler
//!
//! Provides read-only access to the current global LLM configuration.
//! Sensitive fields (API keys) are masked for security.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use gateway::{
    error::GatewayError,
    middleware::{require_any_role, AuthUser},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::AppState;

/// Global LLM configuration response
#[derive(Debug, Serialize)]
pub struct LlmGlobalConfigResponse {
    pub default_provider: String,
    pub fallback_chain: Vec<String>,
    pub cost_optimization: bool,
    pub max_tokens: u32,
    pub system_prompt: String,
    pub request_timeout: u64,
    pub providers: Vec<ProviderConfigResponse>,
}

/// Provider configuration (with masked API key)
#[derive(Debug, Serialize)]
pub struct ProviderConfigResponse {
    pub name: String,
    pub api_key_masked: String,
    pub model: String,
    pub base_url: String,
    pub temperature: f32,
    pub context_window: Option<u32>,
}

/// Request to update LLM provider configuration
#[derive(Debug, Deserialize)]
pub struct UpdateLlmConfigRequest {
    pub provider: String,
    pub model: String,
    pub temperature: f32,
    /// Whether to set this provider as the default. Defaults to true.
    pub set_default: Option<bool>,
}

/// Get current global LLM configuration (read-only, sensitive fields masked)
pub async fn get_llm_global_config(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Result<Json<LlmGlobalConfigResponse>, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let config = &state.config.models;

    let providers = config
        .providers
        .iter()
        .map(|(name, provider)| ProviderConfigResponse {
            name: name.clone(),
            api_key_masked: mask_api_key(provider.api_key.as_deref().unwrap_or("")),
            model: provider.model.clone().unwrap_or_default(),
            base_url: provider.base_url.clone().unwrap_or_default(),
            temperature: provider.temperature,
            context_window: provider.context_window.map(|v| v as u32),
        })
        .collect();

    Ok(Json(LlmGlobalConfigResponse {
        default_provider: config.default_provider.clone(),
        fallback_chain: config.fallback_chain.clone(),
        cost_optimization: config.cost_optimization,
        max_tokens: config.max_tokens,
        system_prompt: config.system_prompt.clone(),
        request_timeout: config.request_timeout,
        providers,
    }))
}

/// Update LLM provider configuration (model & temperature) and persist to config file
pub async fn update_llm_global_config(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<UpdateLlmConfigRequest>,
) -> Result<impl axum::response::IntoResponse, GatewayError> {
    require_any_role(&user, &["user", "admin"])?;

    let path = state
        .config_manager
        .as_ref()
        .and_then(|m| m.source_path().cloned())
        .unwrap_or_else(|| std::path::PathBuf::from("config/beebotos.toml"));

    // Read current config file
    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to read config file: {}", e)))?;

    // Parse as toml::Value for surgical modification
    let mut doc: toml::Value = toml::from_str(&content)
        .map_err(|e| GatewayError::internal(format!("Failed to parse config TOML: {}", e)))?;

    // Update the specific provider table
    if let Some(models) = doc.get_mut("models") {
        if let Some(provider) = models.get_mut(&req.provider) {
            if let Some(table) = provider.as_table_mut() {
                table.insert("model".to_string(), toml::Value::String(req.model));
                // Use string round-trip to clean up f32→f64 precision artifacts
                let temp: f64 = format!("{}", req.temperature)
                    .parse()
                    .unwrap_or(req.temperature as f64);
                table.insert(
                    "temperature".to_string(),
                    toml::Value::Float(temp),
                );
            } else {
                return Err(GatewayError::internal(format!(
                    "Provider '{}' is not a TOML table",
                    req.provider
                )));
            }
        } else {
            return Err(GatewayError::internal(format!(
                "Provider '{}' not found in config",
                req.provider
            )));
        }

        // Optionally update the default provider
        if req.set_default.unwrap_or(true) {
            if let Some(models_table) = models.as_table_mut() {
                models_table.insert(
                    "default_provider".to_string(),
                    toml::Value::String(req.provider.clone()),
                );
            }
        }
    } else {
        return Err(GatewayError::internal(
            "[models] section not found in config".to_string(),
        ));
    }

    // Serialize back to TOML
    let new_content = toml::to_string_pretty(&doc)
        .map_err(|e| GatewayError::internal(format!("Failed to serialize config: {}", e)))?;

    // Write back to file
    tokio::fs::write(&path, new_content)
        .await
        .map_err(|e| GatewayError::internal(format!("Failed to write config file: {}", e)))?;

    // Hot-reload configuration
    let reload_result = if let Some(ref manager) = state.config_manager {
        match manager.reload().await {
            Ok(changed) => {
                let msg = if changed {
                    "Config updated and hot-reloaded"
                } else {
                    "Config updated (no changes detected on reload)"
                };
                serde_json::json!({
                    "message": msg,
                    "status": "ok"
                })
            }
            Err(e) => serde_json::json!({
                "message": format!("Config saved but hot-reload failed: {}", e),
                "status": "partial"
            }),
        }
    } else {
        serde_json::json!({
            "message": "Config saved but config manager not available for reload",
            "status": "partial"
        })
    };

    Ok((StatusCode::OK, Json(reload_result)))
}

/// Mask an API key for display (show first 4 and last 4 chars)
fn mask_api_key(key: &str) -> String {
    if key.len() <= 12 {
        "****".to_string()
    } else {
        format!("{}****{}", &key[..4], &key[key.len() - 4..])
    }
}
