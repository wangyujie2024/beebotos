//! 浏览器自动化 API 服务
//!
//! 与 Gateway 的浏览器自动化 API 对接

use super::client::{ApiClient, ApiError};
use crate::browser::{
    BatchOperation, BatchResult, BrowserInstance, BrowserProfile, BrowserProfileStatus,
    BrowserSandbox, BrowserStatus, ScreenshotResult,
};
use crate::browser::automation::BatchOptions;
use crate::browser::sandbox::SandboxStats;
use serde::{Deserialize, Serialize};

/// 浏览器 API 服务
#[derive(Clone)]
pub struct BrowserApiService {
    client: ApiClient,
}

impl BrowserApiService {
    pub fn new(client: ApiClient) -> Self {
        Self { client }
    }

    /// 获取浏览器状态
    pub async fn get_status(&self) -> Result<BrowserStatus, ApiError> {
        self.client.get("/browser/status").await
    }

    /// 列出浏览器配置
    pub async fn list_profiles(&self) -> Result<Vec<BrowserProfile>, ApiError> {
        self.client.get("/browser/profiles").await
    }

    /// 创建浏览器配置
    pub async fn create_profile(
        &self,
        profile: BrowserProfile,
    ) -> Result<BrowserProfile, ApiError> {
        self.client.post("/browser/profiles", &profile).await
    }

    /// 删除浏览器配置
    pub async fn delete_profile(&self, id: &str) -> Result<(), ApiError> {
        self.client
            .delete(&format!("/browser/profiles/{}", js_sys::encode_uri_component(id)))
            .await
    }

    /// 连接到浏览器
    pub async fn connect(&self, profile_id: &str) -> Result<BrowserInstance, ApiError> {
        self.client
            .post("/browser/connect", &ConnectRequest { profile_id: profile_id.to_string() })
            .await
    }

    /// 断开浏览器连接
    pub async fn disconnect(&self, instance_id: &str) -> Result<serde_json::Value, ApiError> {
        self.client
            .post(
                "/browser/disconnect",
                &DisconnectRequest { instance_id: instance_id.to_string() },
            )
            .await
    }

    /// 导航到 URL
    pub async fn navigate(&self, instance_id: &str, url: &str) -> Result<NavigationResponse, ApiError> {
        self.client
            .post(
                "/browser/navigate",
                &NavigateRequest {
                    instance_id: instance_id.to_string(),
                    url: url.to_string(),
                },
            )
            .await
    }

    /// 执行 JavaScript
    pub async fn evaluate(
        &self,
        instance_id: &str,
        script: &str,
    ) -> Result<EvaluateResponse, ApiError> {
        self.client
            .post(
                "/browser/evaluate",
                &EvaluateRequest {
                    instance_id: instance_id.to_string(),
                    script: script.to_string(),
                },
            )
            .await
    }

    /// 执行批处理操作
    pub async fn execute_batch(
        &self,
        instance_id: &str,
        batch: BatchOperation,
    ) -> Result<BatchResult, ApiError> {
        let request = BatchExecuteRequest {
            instance_id: instance_id.to_string(),
            operations: batch.operations,
            options: batch.options,
        };

        self.client.post("/browser/batch", &request).await
    }

    /// 获取截图
    pub async fn capture_screenshot(
        &self,
        instance_id: &str,
        full_page: bool,
    ) -> Result<ScreenshotResult, ApiError> {
        let request = ScreenshotRequest {
            instance_id: instance_id.to_string(),
            full_page,
            selector: None,
        };

        self.client.post("/browser/screenshot", &request).await
    }

    /// 列出沙箱
    pub async fn list_sandboxes(&self) -> Result<Vec<BrowserSandbox>, ApiError> {
        self.client.get("/browser/sandboxes").await
    }

    /// 创建沙箱
    pub async fn create_sandbox(
        &self,
        name: &str,
        profile_id: &str,
    ) -> Result<BrowserSandbox, ApiError> {
        let request = CreateSandboxRequest {
            name: name.to_string(),
            profile_id: profile_id.to_string(),
        };

        self.client.post("/browser/sandboxes", &request).await
    }

    /// 删除沙箱
    pub async fn delete_sandbox(&self, id: &str) -> Result<(), ApiError> {
        self.client.delete(&format!("/browser/sandboxes/{}", js_sys::encode_uri_component(id))).await
    }

    /// 获取沙箱统计
    pub async fn get_sandbox_stats(&self, id: &str) -> Result<SandboxStats, ApiError> {
        self.client
            .get(&format!("/browser/sandboxes/{}/stats", js_sys::encode_uri_component(id)))
            .await
    }

    /// 获取实例状态
    pub async fn get_instance_status(
        &self,
        instance_id: &str,
    ) -> Result<BrowserProfileStatus, ApiError> {
        self.client
            .get(&format!("/browser/instances/{}/status", js_sys::encode_uri_component(instance_id)))
            .await
    }
}

/// 连接请求
#[derive(Clone, Debug, Serialize, Deserialize)]
struct ConnectRequest {
    profile_id: String,
}

/// 断开请求
#[derive(Clone, Debug, Serialize, Deserialize)]
struct DisconnectRequest {
    instance_id: String,
}

/// 导航请求
#[derive(Clone, Debug, Serialize, Deserialize)]
struct NavigateRequest {
    instance_id: String,
    url: String,
}

/// 导航响应
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NavigationResponse {
    pub success: bool,
    pub url: String,
    pub title: Option<String>,
}

/// 执行请求
#[derive(Clone, Debug, Serialize, Deserialize)]
struct EvaluateRequest {
    instance_id: String,
    script: String,
}

/// 执行响应
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvaluateResponse {
    pub result: serde_json::Value,
    pub exception: Option<serde_json::Value>,
}

/// 批处理执行请求
#[derive(Clone, Debug, Serialize, Deserialize)]
struct BatchExecuteRequest {
    instance_id: String,
    operations: Vec<crate::browser::BrowserAction>,
    #[serde(flatten)]
    options: BatchOptions,
}

/// 截图请求
#[derive(Clone, Debug, Serialize, Deserialize)]
struct ScreenshotRequest {
    instance_id: String,
    full_page: bool,
    selector: Option<String>,
}

/// 创建沙箱请求
#[derive(Clone, Debug, Serialize, Deserialize)]
struct CreateSandboxRequest {
    name: String,
    profile_id: String,
}

/// 点击请求
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClickRequest {
    pub instance_id: String,
    pub selector: String,
    #[serde(default)]
    pub delay_ms: u64,
}

/// 输入请求
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InputRequest {
    pub instance_id: String,
    pub selector: String,
    pub value: String,
    #[serde(default)]
    pub clear_first: bool,
}
