//! LLM Provider Admin API Service

use super::client::{ApiClient, ApiError};
use serde::{Deserialize, Serialize};

/// LLM Provider (admin view with full fields)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmProvider {
    pub id: i64,
    pub provider_id: String,
    pub name: String,
    pub protocol: String,
    pub base_url: Option<String>,
    pub api_key_masked: Option<String>,
    pub enabled: bool,
    pub is_default_provider: bool,
    pub models: Vec<LlmModel>,
}

/// LLM Model
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmModel {
    pub id: i64,
    pub name: String,
    pub display_name: Option<String>,
    pub is_default_model: bool,
}

/// Response from listing providers
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProvidersResponse {
    pub providers: Vec<LlmProvider>,
}

/// Request to create a provider
#[derive(Clone, Debug, Serialize)]
pub struct CreateProviderRequest {
    pub provider_id: String,
    pub name: String,
    pub protocol: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
}

/// Request to update a provider
#[derive(Clone, Debug, Serialize)]
pub struct UpdateProviderRequest {
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub enabled: Option<bool>,
}

/// Request to add a model
#[derive(Clone, Debug, Serialize)]
pub struct AddModelRequest {
    pub name: String,
    pub display_name: Option<String>,
}

/// LLM Provider Admin Service
#[derive(Clone)]
pub struct LlmProviderService {
    client: ApiClient,
}

impl LlmProviderService {
    pub fn new(client: ApiClient) -> Self {
        Self { client }
    }

    /// List all providers with their models
    pub async fn list_providers(&self) -> Result<ProvidersResponse, ApiError> {
        self.client.get("/admin/llm/providers").await
    }

    /// Create a new provider
    pub async fn create_provider(
        &self,
        req: CreateProviderRequest,
    ) -> Result<serde_json::Value, ApiError> {
        self.client.post("/admin/llm/providers", &req).await
    }

    /// Update a provider
    pub async fn update_provider(
        &self,
        id: i64,
        req: UpdateProviderRequest,
    ) -> Result<serde_json::Value, ApiError> {
        self.client
            .put(&format!("/admin/llm/providers/{}", id), &req)
            .await
    }

    /// Delete a provider
    pub async fn delete_provider(&self, id: i64) -> Result<(), ApiError> {
        self.client
            .delete(&format!("/admin/llm/providers/{}", id))
            .await
    }

    /// Add a model to a provider
    pub async fn add_model(
        &self,
        provider_id: i64,
        req: AddModelRequest,
    ) -> Result<serde_json::Value, ApiError> {
        self.client
            .post(
                &format!("/admin/llm/providers/{}/models", provider_id),
                &req,
            )
            .await
    }

    /// Delete a model
    pub async fn delete_model(
        &self,
        provider_id: i64,
        model_id: i64,
    ) -> Result<(), ApiError> {
        self.client
            .delete(&format!(
                "/admin/llm/providers/{}/models/{}",
                provider_id, model_id
            ))
            .await
    }

    /// Set default provider
    pub async fn set_default_provider(&self, id: i64) -> Result<serde_json::Value, ApiError> {
        self.client
            .put(
                &format!("/admin/llm/providers/{}/default", id),
                &serde_json::json!({}),
            )
            .await
    }

    /// Set default model for a provider
    pub async fn set_default_model(
        &self,
        provider_id: i64,
        model_id: i64,
    ) -> Result<serde_json::Value, ApiError> {
        self.client
            .put(
                &format!(
                    "/admin/llm/providers/{}/models/{}/default",
                    provider_id, model_id
                ),
                &serde_json::json!({}),
            )
            .await
    }
}
