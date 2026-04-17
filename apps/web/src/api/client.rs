//! Advanced API Client with retry, caching, and request deduplication

use crate::api::TokenRefreshResponse;
use gloo_net::http::Response;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

const DEFAULT_TIMEOUT_MS: u32 = 30000;
const DEFAULT_RETRY_ATTEMPTS: u32 = 3;
const DEFAULT_RETRY_DELAY_MS: u32 = 1000;

/// API Client configuration
#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub base_url: String,
    pub timeout_ms: u32,
    pub retry_attempts: u32,
    pub retry_delay_ms: u32,
    pub enable_caching: bool,
    pub cache_ttl_secs: u64,
    pub enable_deduplication: bool,
    /// Enable CSRF token validation
    pub enable_csrf: bool,
    /// CSRF token for state-changing requests
    pub csrf_token: Option<String>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            base_url: "/api/v1".to_string(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            retry_attempts: DEFAULT_RETRY_ATTEMPTS,
            retry_delay_ms: DEFAULT_RETRY_DELAY_MS,
            enable_caching: true,
            cache_ttl_secs: 300, // 5 minutes
            enable_deduplication: true,
            enable_csrf: true,
            csrf_token: None,
        }
    }
}

impl ClientConfig {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            ..Default::default()
        }
    }

    pub fn with_timeout(mut self, ms: u32) -> Self {
        self.timeout_ms = ms;
        self
    }

    pub fn with_retry(mut self, attempts: u32, delay_ms: u32) -> Self {
        self.retry_attempts = attempts;
        self.retry_delay_ms = delay_ms;
        self
    }

    pub fn with_caching(mut self, enabled: bool, ttl_secs: u64) -> Self {
        self.enable_caching = enabled;
        self.cache_ttl_secs = ttl_secs;
        self
    }
}

/// Request interceptor trait
pub trait RequestInterceptor: 'static {
    fn intercept(&self, request: RequestBuilder) -> RequestBuilder;
}

/// Response interceptor trait
pub trait ResponseInterceptor: 'static {
    fn intercept(&self, response: &ApiResponse) -> Result<(), ApiError>;
}

/// Cache entry
#[derive(Clone, Debug)]
struct CacheEntry {
    data: Vec<u8>,
    timestamp: web_time::Instant,
}

/// In-flight request tracker for deduplication
#[derive(Clone, Debug)]
struct InFlightRequest {
    timestamp: web_time::Instant,
}

/// Request builder wrapper
/// Note: This is a simplified version that stores request parameters
/// and builds the actual request on demand
#[derive(Clone)]
pub struct RequestBuilder {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
}

impl RequestBuilder {
    pub fn new(method: &str, url: &str) -> Self {
        Self {
            method: method.to_string(),
            url: url.to_string(),
            headers: Vec::new(),
            body: None,
        }
    }

    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.headers.push((key.to_string(), value.to_string()));
        self
    }

    pub fn json<T: serde::Serialize>(mut self, body: &T) -> Result<Self, ApiError> {
        self.body =
            Some(serde_json::to_vec(body).map_err(|e| ApiError::Serialization(e.to_string()))?);
        Ok(self)
    }

    pub fn build(&self) -> gloo_net::http::RequestBuilder {
        let mut builder = match self.method.as_str() {
            "GET" => gloo_net::http::Request::get(&self.url),
            "POST" => gloo_net::http::Request::post(&self.url),
            "PUT" => gloo_net::http::Request::put(&self.url),
            "DELETE" => gloo_net::http::Request::delete(&self.url),
            "PATCH" => gloo_net::http::Request::patch(&self.url),
            _ => gloo_net::http::Request::get(&self.url),
        };

        for (key, value) in &self.headers {
            builder = builder.header(key, value);
        }

        builder
    }

    pub async fn send(&self) -> Result<Response, gloo_net::Error> {
        let mut builder = self.build();
        if let Some(body) = &self.body {
            builder = builder.header("Content-Type", "application/json");
            let req = builder.body(body.clone())?;
            req.send().await
        } else {
            builder.send().await
        }
    }
}

/// API Response wrapper
#[derive(Debug, Clone)]
pub struct ApiResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl ApiResponse {
    pub async fn from_response(response: Response) -> Result<Self, ApiError> {
        let status = response.status();

        // Extract headers
        let headers = HashMap::new();
        // Note: gloo_net doesn't provide easy header iteration
        // This is a simplified version

        let body = response
            .binary()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        Ok(Self {
            status,
            headers,
            body,
        })
    }

    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, ApiError> {
        serde_json::from_slice(&self.body).map_err(|e| ApiError::Serialization(e.to_string()))
    }

    pub fn text(&self) -> Result<String, ApiError> {
        String::from_utf8(self.body.clone()).map_err(|e| ApiError::Serialization(e.to_string()))
    }

    pub fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }
}

/// Advanced API Client
#[derive(Clone)]
pub struct ApiClient {
    config: ClientConfig,
    auth_token: Arc<RwLock<Option<String>>>,
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    in_flight: Arc<RwLock<HashMap<String, InFlightRequest>>>,
    request_interceptors: Arc<RwLock<Vec<Box<dyn RequestInterceptor + Send + Sync>>>>,
    response_interceptors: Arc<RwLock<Vec<Box<dyn ResponseInterceptor + Send + Sync>>>>,
}

impl ApiClient {
    pub fn new(config: ClientConfig) -> Self {
        Self {
            config,
            auth_token: Arc::new(RwLock::new(None)),
            cache: Arc::new(RwLock::new(HashMap::new())),
            in_flight: Arc::new(RwLock::new(HashMap::new())),
            request_interceptors: Arc::new(RwLock::new(Vec::new())),
            response_interceptors: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn default_client() -> Self {
        Self::new(ClientConfig::default())
    }

    /// Set authentication token
    pub fn set_auth_token(&self, token: Option<String>) {
        *self.auth_token.write() = token;
    }

    /// Refresh access token using refresh token
    ///
    /// # Security
    /// - Uses POST to /auth/refresh endpoint
    /// - Requires valid refresh token
    /// - Returns new access token and optionally new refresh token (rotation)
    pub async fn refresh_token(
        &self,
        refresh_token: &str,
    ) -> Result<TokenRefreshResponse, ApiError> {
        let request =
            RequestBuilder::new("POST", &format!("{}/auth/refresh", self.config.base_url))
                .header("Content-Type", "application/json")
                .json(&serde_json::json!({
                    "refresh_token": refresh_token
                }))
                .map_err(|e| ApiError::Serialization(e.to_string()))?;

        let response = request
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        let api_response = ApiResponse::from_response(response).await?;

        if api_response.is_success() {
            api_response.json()
        } else if api_response.status == 401 {
            Err(ApiError::Unauthorized)
        } else {
            Err(ApiError::from_response(&api_response))
        }
    }

    /// Add request interceptor
    pub fn add_request_interceptor<I: RequestInterceptor + Send + Sync>(&self, interceptor: I) {
        self.request_interceptors
            .write()
            .push(Box::new(interceptor));
    }

    /// Add response interceptor
    pub fn add_response_interceptor<I: ResponseInterceptor + Send + Sync>(&self, interceptor: I) {
        self.response_interceptors
            .write()
            .push(Box::new(interceptor));
    }

    /// Clear cache
    pub fn clear_cache(&self) {
        self.cache.write().clear();
    }

    /// Invalidate cache entry
    pub fn invalidate_cache(&self, key: &str) {
        self.cache.write().remove(key);
    }

    /// Check if request is in-flight (deduplication)
    fn is_in_flight(&self, key: &str) -> bool {
        let in_flight = self.in_flight.read();
        if let Some(req) = in_flight.get(key) {
            // Check if request is stale (>30 seconds)
            req.timestamp.elapsed().as_secs() < 30
        } else {
            false
        }
    }

    /// Mark request as in-flight
    fn mark_in_flight(&self, key: &str) {
        self.in_flight.write().insert(
            key.to_string(),
            InFlightRequest {
                timestamp: web_time::Instant::now(),
            },
        );
    }

    /// Remove from in-flight
    fn remove_in_flight(&self, key: &str) {
        self.in_flight.write().remove(key);
    }

    /// Check cache for valid entry
    fn get_cached(&self, key: &str) -> Option<Vec<u8>> {
        if !self.config.enable_caching {
            return None;
        }

        let cache = self.cache.read();
        cache.get(key).and_then(|entry| {
            if entry.timestamp.elapsed().as_secs() < self.config.cache_ttl_secs {
                Some(entry.data.clone())
            } else {
                None
            }
        })
    }

    /// Store in cache
    fn store_cache(&self, key: &str, data: Vec<u8>) {
        if self.config.enable_caching {
            self.cache.write().insert(
                key.to_string(),
                CacheEntry {
                    data,
                    timestamp: web_time::Instant::now(),
                },
            );
        }
    }

    /// Build request with interceptors and security headers
    ///
    /// # Security Headers Added
    /// - Authorization: Bearer token for authenticated requests
    /// - X-CSRF-Token: CSRF token for state-changing requests
    /// - Origin: Origin header for CSRF protection
    /// - X-Requested-With: XMLHttpRequest to identify AJAX requests
    fn build_request(&self, method: &str, path: &str) -> RequestBuilder {
        let url = format!("{}{}", self.config.base_url, path);

        let mut request = RequestBuilder::new(method, &url);

        // Add security headers
        request = request.header("X-Requested-With", "XMLHttpRequest");

        // Add Origin header for CSRF protection
        if let Some(window) = web_sys::window() {
            if let Ok(origin) = window.location().origin() {
                request = request.header("Origin", &origin);
            }
        }

        // Add auth header if available
        if let Some(token) = self.auth_token.read().clone() {
            request = request.header("Authorization", &format!("Bearer {}", token));
        }

        // Add CSRF token for state-changing methods
        if self.config.enable_csrf && is_state_changing_method(method) {
            if let Some(csrf_token) = &self.config.csrf_token {
                request = request.header("X-CSRF-Token", csrf_token);
            }
        }

        // Apply request interceptors
        for interceptor in self.request_interceptors.read().iter() {
            request = interceptor.intercept(request);
        }

        request
    }

    /// Generate and set new CSRF token
    pub fn generate_csrf_token(&mut self) -> String {
        // Generate random token using web crypto API
        let token = generate_secure_random_token(32);
        self.config.csrf_token = Some(token.clone());
        token
    }

    /// Set CSRF token from external source (e.g., meta tag)
    pub fn set_csrf_token(&mut self, token: String) {
        self.config.csrf_token = Some(token);
    }

    /// Execute request with retry logic
    /// Note: For now, retry is disabled due to RequestBuilder not being Clone
    async fn execute_with_retry(&self, request: RequestBuilder) -> Result<ApiResponse, ApiError> {
        // TODO: Implement retry by rebuilding the request from stored parameters
        self.execute_once(request).await
    }

    /// Execute single request
    async fn execute_once(&self, request: RequestBuilder) -> Result<ApiResponse, ApiError> {
        let response = request
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        let api_response = ApiResponse::from_response(response).await?;

        // Apply response interceptors
        for interceptor in self.response_interceptors.read().iter() {
            interceptor.intercept(&api_response)?;
        }

        Ok(api_response)
    }

    // ==================== Public HTTP Methods ====================

    /// GET request with caching support
    pub async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        let cache_key = format!("GET:{}", path);

        // Check cache
        if let Some(cached) = self.get_cached(&cache_key) {
            return serde_json::from_slice(&cached)
                .map_err(|e| ApiError::Serialization(e.to_string()));
        }

        // Check deduplication
        if self.config.enable_deduplication && self.is_in_flight(&cache_key) {
            // Wait for in-flight request to complete
            for _ in 0..50 {
                // Max 5 seconds wait
                gloo_timers::future::TimeoutFuture::new(100).await;
                if let Some(cached) = self.get_cached(&cache_key) {
                    return serde_json::from_slice(&cached)
                        .map_err(|e| ApiError::Serialization(e.to_string()));
                }
                if !self.is_in_flight(&cache_key) {
                    break;
                }
            }
        }

        self.mark_in_flight(&cache_key);

        let request = self.build_request("GET", path);
        let result = self.execute_with_retry(request).await;

        self.remove_in_flight(&cache_key);

        match result {
            Ok(response) => {
                if response.is_success() {
                    let data = response.body.clone();
                    self.store_cache(&cache_key, data);
                    response.json()
                } else {
                    Err(ApiError::from_response(&response))
                }
            }
            Err(e) => Err(e),
        }
    }

    /// POST request (no caching, clears related cache)
    pub async fn post<T: serde::de::DeserializeOwned, B: serde::Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        // Invalidate cache for this resource
        self.invalidate_cache(&format!("GET:{}", path));

        let request = self.build_request("POST", path).json(body)?;

        let response = self.execute_with_retry(request).await?;

        if response.is_success() {
            response.json()
        } else {
            Err(ApiError::from_response(&response))
        }
    }

    /// PUT request
    pub async fn put<T: serde::de::DeserializeOwned, B: serde::Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        self.invalidate_cache(&format!("GET:{}", path));

        let request = self.build_request("PUT", path).json(body)?;

        let response = self.execute_with_retry(request).await?;

        if response.is_success() {
            response.json()
        } else {
            Err(ApiError::from_response(&response))
        }
    }

    /// DELETE request
    pub async fn delete(&self, path: &str) -> Result<(), ApiError> {
        self.invalidate_cache(&format!("GET:{}", path));

        let request = self.build_request("DELETE", path);
        let response = self.execute_with_retry(request).await?;

        if response.is_success() || response.status == 204 {
            Ok(())
        } else {
            Err(ApiError::from_response(&response))
        }
    }
}

impl Default for ApiClient {
    fn default() -> Self {
        Self::default_client()
    }
}

/// Backend error response structure
#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct ErrorResponse {
    pub success: bool,
    pub error: ErrorDetail,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
}

/// API Error types
#[derive(Debug, Clone, PartialEq)]
pub enum ApiError {
    Network(String),
    Serialization(String),
    NotFound,
    Unauthorized,
    Forbidden,
    ClientError(u16, String),
    ServerError(u16, String),
    Timeout,
    Cancelled,
    Unknown,
}

impl ApiError {
    fn from_response(response: &ApiResponse) -> Self {
        let status = response.status;
        // Try to parse backend error message
        let message = if let Ok(err_resp) = serde_json::from_slice::<ErrorResponse>(&response.body) {
            err_resp.error.message
        } else {
            String::new()
        };

        match status {
            401 => ApiError::Unauthorized,
            403 => ApiError::Forbidden,
            404 => ApiError::NotFound,
            400..=499 => ApiError::ClientError(status, message),
            500..=599 => ApiError::ServerError(status, message),
            _ => ApiError::Unknown,
        }
    }

    /// Check if error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            ApiError::Network(_) | ApiError::ServerError(_, _) | ApiError::Timeout | ApiError::Unknown
        )
    }

    /// Get user-friendly error message
    pub fn user_message(&self) -> String {
        match self {
            ApiError::Network(msg) => format!("Network error: {}", msg),
            ApiError::Unauthorized => "Please log in again".to_string(),
            ApiError::Forbidden => "You don't have permission to do this".to_string(),
            ApiError::NotFound => "Resource not found".to_string(),
            ApiError::Timeout => "Request timed out, please try again".to_string(),
            ApiError::ServerError(code, msg) => {
                if msg.is_empty() {
                    format!("Server error ({}), please try again later", code)
                } else {
                    msg.clone()
                }
            }
            ApiError::ClientError(code, msg) => {
                if msg.is_empty() {
                    format!("Request error ({}), please check your input", code)
                } else {
                    msg.clone()
                }
            }
            ApiError::Serialization(msg) => format!("Data error: {}", msg),
            ApiError::Cancelled => "Request was cancelled".to_string(),
            ApiError::Unknown => "An unexpected error occurred".to_string(),
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.user_message())
    }
}

impl std::error::Error for ApiError {}

/// Check if HTTP method is state-changing (requires CSRF protection)
fn is_state_changing_method(method: &str) -> bool {
    matches!(
        method.to_uppercase().as_str(),
        "POST" | "PUT" | "PATCH" | "DELETE"
    )
}

/// Generate secure random token using web crypto API
fn generate_secure_random_token(length: usize) -> String {
    // Try to use web crypto API for secure random
    if let Some(window) = web_sys::window() {
        if let Ok(crypto) = window.crypto() {
            let mut buffer = vec![0u8; length];
            if crypto.get_random_values_with_u8_array(&mut buffer).is_ok() {
                // Convert to hex string
                return buffer.iter().map(|b| format!("{:02x}", b)).collect();
            }
        }
    }

    // Fallback to simple random (less secure, but works in tests)
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| format!("{:02x}", rng.gen::<u8>()))
        .collect()
}

/// Secure logging helper - filters sensitive data using simple string matching
/// Note: This is a simplified version without regex dependency
pub fn sanitize_for_log(input: &str) -> String {
    let mut result = input.to_string();

    // Simple string-based redaction patterns
    let redactions = [
        ("\"password\":\"", "\"password\":\"***REDACTED***\""),
        ("\"token\":\"", "\"token\":\"***REDACTED***\""),
        (
            "\"refresh_token\":\"",
            "\"refresh_token\":\"***REDACTED***\"",
        ),
        ("\"api_key\":\"", "\"api_key\":\"***REDACTED***\""),
        ("\"secret\":\"", "\"secret\":\"***REDACTED***\""),
        ("\"private_key\":\"", "\"private_key\":\"***REDACTED***\""),
        ("Bearer ", "Bearer ***REDACTED***"),
    ];

    // Simple redaction - look for the key and redact the value
    for (key, replacement) in &redactions {
        if let Some(pos) = result.find(key) {
            // Find the start of the value (after the key)
            let value_start = pos + key.len();
            if value_start < result.len() {
                // Find the end of the value (next quote)
                if let Some(quote_pos) = result[value_start..].find('"') {
                    let value_end = value_start + quote_pos + 1;
                    // Replace the entire key-value pair
                    result.replace_range(pos..value_end, replacement);
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_config() {
        let config = ClientConfig::new("https://api.example.com")
            .with_timeout(5000)
            .with_retry(5, 500);

        assert_eq!(config.base_url, "https://api.example.com");
        assert_eq!(config.timeout_ms, 5000);
        assert_eq!(config.retry_attempts, 5);
        assert_eq!(config.retry_delay_ms, 500);
    }

    #[test]
    fn test_api_error_classification() {
        assert!(ApiError::Network("test".to_string()).is_retryable());
        assert!(ApiError::ServerError(500, "error".to_string()).is_retryable());
        assert!(!ApiError::ClientError(400, "error".to_string()).is_retryable());
        assert!(!ApiError::Unauthorized.is_retryable());
    }

    #[test]
    fn test_is_state_changing_method() {
        assert!(is_state_changing_method("POST"));
        assert!(is_state_changing_method("PUT"));
        assert!(is_state_changing_method("PATCH"));
        assert!(is_state_changing_method("DELETE"));
        assert!(!is_state_changing_method("GET"));
        assert!(!is_state_changing_method("HEAD"));
        assert!(!is_state_changing_method("OPTIONS"));
    }

    #[test]
    fn test_sanitize_for_log() {
        let input = r#"{"username":"admin","password":"secret123","token":"abc.def.ghi"}"#;
        let sanitized = sanitize_for_log(input);
        assert!(!sanitized.contains("secret123"));
        assert!(!sanitized.contains("abc.def.ghi"));
        assert!(sanitized.contains("***REDACTED***"));
        assert!(sanitized.contains("admin")); // non-sensitive should remain
    }
}
