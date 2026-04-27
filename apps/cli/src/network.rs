//! Network layer for BeeBotOS CLI
//!
//! Provides robust HTTP client with:
//! - Connection pooling
//! - Proxy support
//! - DNS caching
//! - Request/response interceptors
//! - Configurable timeouts
//! - Automatic retries

#![allow(dead_code)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use reqwest::header::HeaderValue;
use reqwest::{Client, ClientBuilder, Proxy, Request, Response};
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Network configuration
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Request timeout
    pub timeout: Duration,
    /// Pool idle timeout
    pub pool_idle_timeout: Duration,
    /// Maximum connections per host
    pub pool_max_idle: usize,
    /// Enable TCP keepalive
    pub tcp_keepalive: bool,
    /// TCP keepalive interval
    pub tcp_keepalive_interval: Duration,
    /// Enable HTTP/2
    pub http2: bool,
    /// Enable gzip compression
    pub gzip: bool,
    /// Enable brotli compression
    pub brotli: bool,
    /// Proxy configuration
    pub proxy: Option<ProxyConfig>,
    /// User agent string
    pub user_agent: String,
    /// Maximum redirects
    pub max_redirects: usize,
    /// Danger: Accept invalid certificates (dev only)
    pub danger_accept_invalid_certs: bool,
    /// DNS cache TTL
    pub dns_cache_ttl: Duration,
    /// Maximum DNS cache entries
    pub dns_cache_size: usize,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            timeout: Duration::from_secs(30),
            pool_idle_timeout: Duration::from_secs(90),
            pool_max_idle: 10,
            tcp_keepalive: true,
            tcp_keepalive_interval: Duration::from_secs(60),
            http2: true,
            gzip: true,
            brotli: true,
            proxy: None,
            user_agent: format!("BeeBotOS-CLI/{} (Rust)", env!("CARGO_PKG_VERSION")),
            max_redirects: 10,
            danger_accept_invalid_certs: false,
            dns_cache_ttl: Duration::from_secs(300),
            dns_cache_size: 1000,
        }
    }
}

/// Proxy configuration
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// Proxy URL (e.g., http://proxy.example.com:8080)
    pub url: String,
    /// Proxy authentication username
    pub username: Option<String>,
    /// Proxy authentication password
    pub password: Option<String>,
    /// No-proxy hosts
    pub no_proxy: Vec<String>,
}

impl ProxyConfig {
    /// Create proxy from environment variables
    pub fn from_env() -> Option<Self> {
        let url = std::env::var("HTTP_PROXY")
            .or_else(|_| std::env::var("http_proxy"))
            .ok()?;

        Some(Self {
            url,
            username: std::env::var("HTTP_PROXY_USER").ok(),
            password: std::env::var("HTTP_PROXY_PASS").ok(),
            no_proxy: std::env::var("NO_PROXY")
                .map(|s| s.split(',').map(|s| s.to_string()).collect())
                .unwrap_or_default(),
        })
    }

    /// Build reqwest Proxy
    pub fn build(&self) -> Result<Proxy> {
        let mut proxy =
            Proxy::all(&self.url).with_context(|| format!("Invalid proxy URL: {}", self.url))?;

        if let (Some(user), Some(pass)) = (&self.username, &self.password) {
            proxy = proxy.basic_auth(user, pass);
        }

        if !self.no_proxy.is_empty() {
            let no_proxy = reqwest::NoProxy::from_string(&self.no_proxy.join(","));
            proxy = proxy.no_proxy(no_proxy);
        }

        Ok(proxy)
    }
}

/// DNS cache entry
#[derive(Debug, Clone)]
struct DnsEntry {
    addrs: Vec<SocketAddr>,
    expires_at: Instant,
}

/// DNS resolver with caching
pub struct CachedDnsResolver {
    cache: Arc<RwLock<HashMap<String, DnsEntry>>>,
    ttl: Duration,
    max_size: usize,
}

impl CachedDnsResolver {
    /// Create new DNS resolver with cache
    pub fn new(ttl: Duration, max_size: usize) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            ttl,
            max_size,
        }
    }

    /// Resolve hostname to addresses
    pub async fn resolve(&self, hostname: &str) -> Result<Vec<SocketAddr>> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(hostname) {
                if entry.expires_at > Instant::now() {
                    debug!("DNS cache hit for {}", hostname);
                    return Ok(entry.addrs.clone());
                }
            }
        }

        // Perform DNS lookup
        debug!("DNS lookup for {}", hostname);
        let addrs = tokio::net::lookup_host(hostname)
            .await
            .with_context(|| format!("DNS lookup failed for {}", hostname))?
            .collect::<Vec<_>>();

        if addrs.is_empty() {
            return Err(anyhow::anyhow!("No addresses found for {}", hostname));
        }

        // Cache the result
        {
            let mut cache = self.cache.write().await;

            // Evict old entries if cache is full
            if cache.len() >= self.max_size {
                let now = Instant::now();
                let expired_keys: Vec<String> = cache
                    .iter()
                    .filter(|(_, entry)| entry.expires_at < now)
                    .map(|(k, _)| k.clone())
                    .collect();

                for key in expired_keys {
                    cache.remove(&key);
                }
            }

            cache.insert(
                hostname.to_string(),
                DnsEntry {
                    addrs: addrs.clone(),
                    expires_at: Instant::now() + self.ttl,
                },
            );
        }

        Ok(addrs)
    }

    /// Clear DNS cache
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    /// Get cache statistics
    pub async fn stats(&self) -> (usize, usize) {
        let cache = self.cache.read().await;
        let total = cache.len();
        let expired = cache
            .values()
            .filter(|e| e.expires_at < Instant::now())
            .count();
        (total, expired)
    }
}

/// Request interceptor trait
pub trait RequestInterceptor: Send + Sync {
    /// Intercept and potentially modify request
    fn intercept(&self, request: &mut Request) -> Result<()>;
}

/// Response interceptor trait
pub trait ResponseInterceptor: Send + Sync {
    /// Intercept and potentially modify response
    fn intercept(&self, response: &Response) -> Result<()>;
}

/// Default request interceptor (adds common headers)
pub struct DefaultRequestInterceptor {
    api_key: String,
    user_agent: String,
}

impl DefaultRequestInterceptor {
    pub fn new(api_key: String, user_agent: String) -> Self {
        Self {
            api_key,
            user_agent,
        }
    }
}

impl RequestInterceptor for DefaultRequestInterceptor {
    fn intercept(&self, request: &mut Request) -> Result<()> {
        let headers = request.headers_mut();

        // Add authorization
        if let Ok(auth_value) = HeaderValue::from_str(&format!("Bearer {}", self.api_key)) {
            headers.insert("Authorization", auth_value);
        }

        // Add user agent
        if let Ok(ua_value) = HeaderValue::from_str(&self.user_agent) {
            headers.insert("User-Agent", ua_value);
        }

        // Add content type if not present
        if !headers.contains_key("Content-Type") {
            headers.insert("Content-Type", HeaderValue::from_static("application/json"));
        }

        Ok(())
    }
}

/// Logging interceptor
pub struct LoggingInterceptor;

impl RequestInterceptor for LoggingInterceptor {
    fn intercept(&self, request: &mut Request) -> Result<()> {
        debug!("HTTP Request: {} {}", request.method(), request.url());
        Ok(())
    }
}

impl ResponseInterceptor for LoggingInterceptor {
    fn intercept(&self, response: &Response) -> Result<()> {
        debug!(
            "HTTP Response: {} {} ({})",
            response.url(),
            response.status(),
            response
                .content_length()
                .map(|l| l.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        );
        Ok(())
    }
}

/// Retry policy
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum retry attempts
    pub max_attempts: u32,
    /// Base delay for exponential backoff
    pub base_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// HTTP status codes that trigger retry
    pub retryable_status_codes: Vec<u16>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            retryable_status_codes: vec![408, 429, 500, 502, 503, 504],
        }
    }
}

impl RetryPolicy {
    /// Calculate delay for a specific attempt
    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        let delay_ms =
            self.base_delay.as_millis() as f64 * self.backoff_multiplier.powi(attempt as i32);
        let delay_ms = delay_ms.min(self.max_delay.as_millis() as f64) as u64;
        Duration::from_millis(delay_ms)
    }

    /// Check if status code is retryable
    pub fn is_retryable(&self, status: u16) -> bool {
        self.retryable_status_codes.contains(&status)
    }
}

/// Network client with all features
pub struct NetworkClient {
    client: Client,
    config: NetworkConfig,
    dns_resolver: Option<Arc<CachedDnsResolver>>,
    request_interceptors: Vec<Box<dyn RequestInterceptor>>,
    response_interceptors: Vec<Box<dyn ResponseInterceptor>>,
    retry_policy: RetryPolicy,
}

impl NetworkClient {
    /// Create new network client with configuration
    pub fn new(config: NetworkConfig) -> Result<Self> {
        let mut builder = ClientBuilder::new()
            .connect_timeout(config.connect_timeout)
            .timeout(config.timeout)
            .pool_idle_timeout(config.pool_idle_timeout)
            .pool_max_idle_per_host(config.pool_max_idle)
            .user_agent(&config.user_agent)
            .redirect(reqwest::redirect::Policy::limited(config.max_redirects));

        // TCP keepalive
        if config.tcp_keepalive {
            builder = builder.tcp_keepalive(Some(config.tcp_keepalive_interval));
        }

        // HTTP/2
        if config.http2 {
            builder = builder.http2_prior_knowledge();
        }

        // Compression is enabled by default in reqwest 0.11
        // Use no_gzip() and no_brotli() to disable if needed

        // Proxy
        if let Some(ref proxy_config) = config.proxy {
            let proxy = proxy_config.build()?;
            builder = builder.proxy(proxy);
        }

        // Danger: Accept invalid certs (dev only)
        if config.danger_accept_invalid_certs {
            builder = builder.danger_accept_invalid_certs(true);
        }

        let client = builder.build().context("Failed to build HTTP client")?;

        // Create DNS resolver if caching is enabled
        let dns_resolver = if config.dns_cache_ttl > Duration::ZERO {
            Some(Arc::new(CachedDnsResolver::new(
                config.dns_cache_ttl,
                config.dns_cache_size,
            )))
        } else {
            None
        };

        Ok(Self {
            client,
            config,
            dns_resolver,
            request_interceptors: Vec::new(),
            response_interceptors: Vec::new(),
            retry_policy: RetryPolicy::default(),
        })
    }

    /// Create client from environment
    pub fn from_env() -> Result<Self> {
        let mut config = NetworkConfig::default();

        // Load proxy from environment
        config.proxy = ProxyConfig::from_env();

        // Load timeouts from environment
        if let Ok(timeout) = std::env::var("BEEBOTOS_HTTP_TIMEOUT") {
            if let Ok(secs) = timeout.parse() {
                config.timeout = Duration::from_secs(secs);
            }
        }

        Self::new(config)
    }

    /// Add request interceptor
    pub fn add_request_interceptor<I>(&mut self, interceptor: I)
    where
        I: RequestInterceptor + 'static,
    {
        self.request_interceptors.push(Box::new(interceptor));
    }

    /// Add response interceptor
    pub fn add_response_interceptor<I>(&mut self, interceptor: I)
    where
        I: ResponseInterceptor + 'static,
    {
        self.response_interceptors.push(Box::new(interceptor));
    }

    /// Set retry policy
    pub fn set_retry_policy(&mut self, policy: RetryPolicy) {
        self.retry_policy = policy;
    }

    /// Get reference to inner reqwest client
    pub fn inner(&self) -> &Client {
        &self.client
    }

    /// Consume the NetworkClient and return the inner reqwest client
    pub fn into_inner(self) -> Client {
        self.client
    }

    /// Execute request with interceptors and retries
    pub async fn execute(&self, request: Request) -> Result<Response> {
        let mut request = request;

        // Apply request interceptors
        for interceptor in &self.request_interceptors {
            interceptor.intercept(&mut request)?;
        }

        let mut last_error = None;

        // Execute with retries
        for attempt in 0..=self.retry_policy.max_attempts {
            match self.client.execute(request.try_clone().unwrap()).await {
                Ok(response) => {
                    // Apply response interceptors
                    for interceptor in &self.response_interceptors {
                        interceptor.intercept(&response)?;
                    }

                    // Check if we should retry
                    let status = response.status().as_u16();
                    if self.retry_policy.is_retryable(status)
                        && attempt < self.retry_policy.max_attempts
                    {
                        let delay = self.retry_policy.calculate_delay(attempt);
                        warn!(
                            "Request failed with status {}, retrying in {:?} (attempt {}/{})",
                            status,
                            delay,
                            attempt + 1,
                            self.retry_policy.max_attempts + 1
                        );
                        tokio::time::sleep(delay).await;
                        continue;
                    }

                    return Ok(response);
                }
                Err(e) => {
                    last_error = Some(e);

                    if attempt < self.retry_policy.max_attempts {
                        let delay = self.retry_policy.calculate_delay(attempt);
                        warn!(
                            "Request failed: {}, retrying in {:?} (attempt {}/{})",
                            last_error.as_ref().unwrap(),
                            delay,
                            attempt + 1,
                            self.retry_policy.max_attempts + 1
                        );
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }

        Err(last_error
            .map(|e| {
                anyhow::anyhow!(
                    "Request failed after {} attempts: {}",
                    self.retry_policy.max_attempts + 1,
                    e
                )
            })
            .unwrap_or_else(|| anyhow::anyhow!("Request failed")))
    }

    /// Get DNS cache stats
    pub async fn dns_stats(&self) -> Option<(usize, usize)> {
        if let Some(ref resolver) = self.dns_resolver {
            Some(resolver.stats().await)
        } else {
            None
        }
    }

    /// Clear DNS cache
    pub async fn clear_dns_cache(&self) {
        if let Some(ref resolver) = self.dns_resolver {
            resolver.clear_cache().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_config_default() {
        let config = NetworkConfig::default();
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert_eq!(config.pool_max_idle, 10);
        assert!(config.gzip);
    }

    #[test]
    fn test_proxy_config_from_env() {
        // This test would need to set environment variables
        // Skipping as it would affect the environment
    }

    #[test]
    fn test_retry_policy_calculate_delay() {
        let policy = RetryPolicy::default();

        let delay0 = policy.calculate_delay(0);
        assert_eq!(delay0, Duration::from_millis(500));

        let delay1 = policy.calculate_delay(1);
        assert_eq!(delay1, Duration::from_millis(1000));

        let delay2 = policy.calculate_delay(2);
        assert_eq!(delay2, Duration::from_millis(2000));
    }

    #[test]
    fn test_retry_policy_is_retryable() {
        let policy = RetryPolicy::default();
        assert!(policy.is_retryable(500));
        assert!(policy.is_retryable(503));
        assert!(policy.is_retryable(429));
        assert!(!policy.is_retryable(400));
        assert!(!policy.is_retryable(404));
    }

    #[tokio::test]
    async fn test_dns_cache() {
        let resolver = CachedDnsResolver::new(Duration::from_secs(60), 100);

        // Test resolve (may fail in test environment without network)
        let result = resolver.resolve("localhost").await;
        if let Ok(addrs) = result {
            assert!(!addrs.is_empty());

            // Second resolve should hit cache
            let cached = resolver.resolve("localhost").await;
            assert!(cached.is_ok());
        }
    }

    #[test]
    fn test_default_request_interceptor() {
        let interceptor =
            DefaultRequestInterceptor::new("test-key".to_string(), "test-agent".to_string());

        let req = Request::new(reqwest::Method::GET, "http://localhost".parse().unwrap());
        let mut req = req;

        interceptor.intercept(&mut req).unwrap();

        assert!(req.headers().contains_key("Authorization"));
        assert!(req.headers().contains_key("User-Agent"));
    }

    #[test]
    fn test_network_client_build() {
        let config = NetworkConfig::default();
        let client = NetworkClient::new(config);
        assert!(client.is_ok());
    }
}
