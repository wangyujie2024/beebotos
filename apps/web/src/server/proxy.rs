//! API 代理模块
//!
//! 将 /api/* 请求转发到后端 Gateway

use axum::{
    body::{Body, Bytes},
    extract::{Request, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use std::time::Duration;

/// 代理状态
#[derive(Clone)]
pub struct ProxyState {
    /// HTTP 客户端
    pub client: reqwest::Client,
    /// Gateway 基础 URL
    pub gateway_url: String,
    /// 是否转发 Host 头
    pub forward_host: bool,
}

impl ProxyState {
    /// 创建新的代理状态
    pub fn new(gateway_url: String, timeout_secs: u64, forward_host: bool) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .connect_timeout(Duration::from_secs(10))
            .pool_idle_timeout(Duration::from_secs(300))
            .pool_max_idle_per_host(32)
            .build()?;

        Ok(Self {
            client,
            gateway_url,
            forward_host,
        })
    }
}

/// API 代理处理器
pub async fn proxy_handler(
    State(state): State<ProxyState>,
    mut req: Request,
) -> Result<impl IntoResponse, StatusCode> {
    // 构建目标 URL - 添加 /api 前缀
    let path = req.uri().path();
    let query = req.uri().query().map(|q| format!("?{}", q)).unwrap_or_default();
    let target_url = format!("{}/api{}{}", state.gateway_url, path, query);

    tracing::debug!("Proxying {} {} -> {}", req.method(), path, target_url);

    // 提取请求体
    let body_bytes = extract_body(&mut req).await.map_err(|e| {
        tracing::error!("Failed to extract request body: {}", e);
        StatusCode::BAD_REQUEST
    })?;

    // 构建代理请求
    let method = reqwest::Method::from_bytes(req.method().as_str().as_bytes())
        .map_err(|_| {
            tracing::error!("Invalid HTTP method: {}", req.method());
            StatusCode::BAD_REQUEST
        })?;
    let mut proxy_req = state.client.request(method, &target_url);

    // 复制请求头
    let mut headers = HeaderMap::new();
    for (name, value) in req.headers() {
        if should_forward_header(name) {
            headers.insert(name.clone(), value.clone());
        }
    }

    // 添加 X-Forwarded 头
    if let Some(host) = req.headers().get("host") {
        if state.forward_host {
            headers.insert("host", host.clone());
        }
        headers.insert("X-Forwarded-Host", host.clone());
    }

    headers.insert("X-Forwarded-Proto", HeaderValue::from_static("http"));

    // 设置请求头
    for (name, value) in &headers {
        let name_str = name.as_str();
        if let Ok(value_str) = value.to_str() {
            proxy_req = proxy_req.header(name_str, value_str);
        }
    }

    // 添加请求体
    if !body_bytes.is_empty() {
        proxy_req = proxy_req.body(body_bytes.to_vec());
    }

    // 发送请求
    let response = proxy_req.send().await.map_err(|e| {
        tracing::error!("Proxy request failed: {}", e);
        StatusCode::BAD_GATEWAY
    })?;

    // 构建响应
    let status = StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::OK);
    let mut builder = Response::builder().status(status);

    // 复制响应头（跳过内容相关头，由 axum 自动处理）
    for (name, value) in response.headers() {
        let name_lower = name.as_str().to_lowercase();
        // 跳过这些头，避免与 axum 的 Body 处理冲突
        if name_lower == "content-length" || name_lower == "transfer-encoding" {
            continue;
        }
        if let Ok(name) = HeaderName::from_bytes(name.as_str().as_bytes()) {
            if let Ok(value) = HeaderValue::from_bytes(value.as_bytes()) {
                builder = builder.header(name, value);
            }
        }
    }

    // 获取响应体
    let body_bytes = response.bytes().await.map_err(|e| {
        tracing::error!("Failed to read response body: {}", e);
        StatusCode::BAD_GATEWAY
    })?;

    Ok(builder
        .body(Body::from(body_bytes))
        .unwrap_or_else(|_| Response::new(Body::empty())))
}

/// 提取请求体
async fn extract_body(req: &mut Request) -> anyhow::Result<Bytes> {
    let body = std::mem::take(req.body_mut());
    let bytes = axum::body::to_bytes(body, usize::MAX).await?;
    Ok(bytes)
}

/// 判断是否应该转发该请求头
fn should_forward_header(name: &HeaderName) -> bool {
    // 跳过 hop-by-hop 头
    let hop_by_hop = [
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailers",
        "transfer-encoding",
        "upgrade",
    ];

    let name_str = name.as_str().to_lowercase();
    !hop_by_hop.contains(&name_str.as_str())
}

/// 健康检查处理器
pub async fn health_check() -> impl IntoResponse {
    axum::Json(serde_json::json!({
        "status": "ok",
        "service": "beebotos-web-server",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_state_creation() {
        let state = ProxyState::new(
            "http://localhost:3000".to_string(),
            30,
            false,
        );
        assert!(state.is_ok());
    }

    #[test]
    fn test_should_forward_header() {
        assert!(!should_forward_header(&HeaderName::from_static("connection")));
        assert!(!should_forward_header(&HeaderName::from_static("upgrade")));
        assert!(should_forward_header(&HeaderName::from_static("content-type")));
        assert!(should_forward_header(&HeaderName::from_static("authorization")));
    }
}
