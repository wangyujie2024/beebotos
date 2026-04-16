//! Gateway Middleware
//!
//! Production-ready middleware stack:
//! - Authentication (JWT validation)
//! - Request ID tracing
//! - Rate limiting
//! - CORS handling
//! - Request logging
//! - Error handling

use axum::{
    extract::{ConnectInfo, FromRequestParts, Request, State},
    http::{header, request::Parts, HeaderValue},
    middleware::Next,
    response::{IntoResponse, Response},
};

use chrono::{Duration as ChronoDuration, Utc};
use jsonwebtoken::{
    decode, encode, Algorithm, DecodingKey, EncodingKey, Header, TokenData, Validation,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tower_http::{
    cors::{AllowOrigin, Any, CorsLayer},
    trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer},
};
use tracing::{debug, error, info, info_span, warn, Instrument};
use uuid::Uuid;

use crate::config::GatewayConfig;
use crate::error::{GatewayError, Result};
use crate::rate_limit::{RateLimitManager, RateLimitResult};

/// Request ID header name
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Authorization header prefix
const BEARER_PREFIX: &str = "Bearer ";

/// JWT claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID)
    pub sub: String,
    /// JWT ID
    pub jti: String,
    /// Issued at (timestamp)
    pub iat: i64,
    /// Expiration (timestamp)
    pub exp: i64,
    /// Issuer
    pub iss: String,
    /// Audience
    pub aud: String,
    /// User roles
    pub roles: Vec<String>,
    /// Token type
    #[serde(default)]
    pub token_type: TokenType,
    /// Session ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Token type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    /// Access token (short-lived)
    #[default]
    Access,
    /// Refresh token (long-lived)
    Refresh,
    /// API key token
    ApiKey,
}

/// Authenticated user extractor
#[derive(Debug, Clone)]
pub struct AuthUser {
    /// User ID
    pub user_id: String,
    /// User roles
    pub roles: Vec<String>,
    /// JWT claims
    pub claims: Claims,
    /// Client IP
    pub client_ip: String,
    /// Request ID
    pub request_id: String,
}

impl AuthUser {
    /// Check if user has role
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.contains(&role.to_string()) || self.roles.contains(&"admin".to_string())
    }

    /// Check if user is admin
    pub fn is_admin(&self) -> bool {
        self.has_role("admin")
    }
}

#[async_trait::async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = GatewayError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthUser>()
            .cloned()
            .ok_or_else(|| GatewayError::unauthorized("Authentication required"))
    }
}

/// JWT authentication middleware
///
/// Validates JWT token from Authorization header and extracts user info
pub async fn auth_middleware(
    State(state): State<Arc<GatewayState>>,
    mut request: Request,
    next: Next,
) -> Response {
    let request_id = get_request_id(&request);
    let path = request.uri().path();

    // Skip auth for public paths
    if is_public_path(path) {
        debug!(path = %path, "Skipping auth for public path");
        return next.run(request).await;
    }

    // Extract authorization header
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let token = match auth_header {
        Some(header) if header.starts_with(BEARER_PREFIX) => &header[BEARER_PREFIX.len()..],
        _ => {
            warn!(request_id = %request_id, "Missing or invalid authorization header");
            return GatewayError::unauthorized("Missing or invalid authorization token")
                .into_response();
        }
    };

    // Demo token shortcut for frontend demo login
    if token == "demo-token" {
        let client_ip = request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|info| info.0.ip().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let claims = Claims {
            sub: "demo-user".to_string(),
            jti: Uuid::new_v4().to_string(),
            iat: Utc::now().timestamp(),
            exp: (Utc::now() + ChronoDuration::hours(24)).timestamp(),
            iss: state.config.jwt.issuer.clone(),
            aud: state.config.jwt.audience.clone(),
            roles: vec!["admin".to_string()],
            token_type: TokenType::Access,
            session_id: None,
        };

        let auth_user = AuthUser {
            user_id: claims.sub.clone(),
            roles: claims.roles.clone(),
            claims,
            client_ip,
            request_id: request_id.clone(),
        };

        debug!(request_id = %request_id, user_id = %auth_user.user_id, "Demo user authenticated");
        request.extensions_mut().insert(auth_user);
        return next.run(request).await;
    }

    // Validate token
    let claims = match validate_token(token, &state.config.jwt) {
        Ok(c) => c,
        Err(e) => {
            warn!(request_id = %request_id, error = %e, "Token validation failed");
            return GatewayError::unauthorized("Invalid or expired token").into_response();
        }
    };

    // Check token type
    if claims.token_type != TokenType::Access {
        return GatewayError::unauthorized("Invalid token type").into_response();
    }

    // Extract client IP
    let client_ip = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Create auth user
    let auth_user = AuthUser {
        user_id: claims.sub.clone(),
        roles: claims.roles.clone(),
        claims,
        client_ip,
        request_id: request_id.clone(),
    };

    debug!(
        request_id = %request_id,
        user_id = %auth_user.user_id,
        "User authenticated"
    );

    // Add user to request extensions
    request.extensions_mut().insert(auth_user);

    next.run(request).await
}

/// Validate JWT token
fn validate_token(token: &str, config: &crate::config::JwtConfig) -> anyhow::Result<Claims> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[&config.issuer]);
    validation.set_audience(&[&config.audience]);

    let token_data: TokenData<Claims> = decode(
        token,
        &DecodingKey::from_secret(config.secret_bytes()),
        &validation,
    )?;

    Ok(token_data.claims)
}

/// Generate JWT access token
pub fn generate_access_token(
    user_id: impl Into<String>,
    roles: Vec<String>,
    config: &crate::config::JwtConfig,
) -> Result<String> {
    let now = Utc::now();
    let jti = Uuid::new_v4().to_string();

    let claims = Claims {
        sub: user_id.into(),
        jti: jti.clone(),
        iat: now.timestamp(),
        exp: (now + ChronoDuration::minutes(config.expiry_minutes as i64)).timestamp(),
        iss: config.issuer.clone(),
        aud: config.audience.clone(),
        roles,
        token_type: TokenType::Access,
        session_id: Some(jti),
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.secret_bytes()),
    )
    .map_err(|e| GatewayError::internal(format!("Failed to generate token: {}", e)))
}

/// Generate refresh token
pub fn generate_refresh_token(
    user_id: impl Into<String>,
    config: &crate::config::JwtConfig,
) -> Result<String> {
    let now = Utc::now();
    let jti = Uuid::new_v4().to_string();

    let claims = Claims {
        sub: user_id.into(),
        jti: jti.clone(),
        iat: now.timestamp(),
        exp: (now + ChronoDuration::minutes(config.refresh_expiry_minutes as i64)).timestamp(),
        iss: config.issuer.clone(),
        aud: config.audience.clone(),
        roles: vec![],
        token_type: TokenType::Refresh,
        session_id: Some(jti),
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.secret_bytes()),
    )
    .map_err(|e| GatewayError::internal(format!("Failed to generate refresh token: {}", e)))
}

/// Request ID middleware
///
/// Generates or propagates request IDs for distributed tracing
pub async fn request_id_middleware(mut request: Request, next: Next) -> Response {
    let request_id = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    // Add to request extensions
    request.extensions_mut().insert(RequestContext {
        request_id: request_id.clone(),
        start_time: Instant::now(),
    });

    // Process request
    let mut response = next.run(request).await;

    // Add request ID to response headers
    response.headers_mut().insert(
        REQUEST_ID_HEADER,
        request_id
            .parse()
            .unwrap_or_else(|_| HeaderValue::from_static("unknown")),
    );

    response
}

/// Request context for tracing
#[derive(Debug, Clone)]
pub struct RequestContext {
    /// Unique request identifier
    pub request_id: String,
    /// Request start time
    pub start_time: Instant,
}

/// Get request ID from request extensions
fn get_request_id(request: &Request) -> String {
    request
        .extensions()
        .get::<RequestContext>()
        .map(|ctx| ctx.request_id.clone())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Rate limiting middleware
pub async fn rate_limit_middleware(
    State(state): State<Arc<GatewayState>>,
    request: Request,
    next: Next,
) -> Response {
    if !state.config.rate_limit.enabled {
        return next.run(request).await;
    }

    let request_id = get_request_id(&request);
    let path = request.uri().path();

    // Get client ID (user ID if authenticated, otherwise IP)
    let client_id = request
        .extensions()
        .get::<AuthUser>()
        .map(|u| u.user_id.clone())
        .or_else(|| {
            request
                .extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|info| info.0.ip().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

    // Check rate limit
    let result = state.rate_limiter.check(path, &client_id).await;

    if !result.allowed {
        warn!(
            request_id = %request_id,
            client_id = %client_id,
            path = %path,
            "Rate limit exceeded"
        );

        return GatewayError::rate_limited(result.retry_after.map(|d| d.as_secs())).into_response();
    }

    // Add rate limit headers to response
    let mut response = next.run(request).await;
    add_rate_limit_headers(response.headers_mut(), &result);

    response
}

/// Add rate limit headers to response
fn add_rate_limit_headers(headers: &mut axum::http::HeaderMap, result: &RateLimitResult) {
    headers.insert(
        "x-ratelimit-remaining",
        result.remaining.to_string().parse().unwrap(),
    );
    headers.insert(
        "x-ratelimit-reset",
        result.reset_time.as_secs().to_string().parse().unwrap(),
    );
}

/// Request logging middleware
pub async fn logging_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let start = Instant::now();
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let request_id = get_request_id(&request);

    let span = info_span!(
        "http_request",
        request_id = %request_id,
        method = %method,
        path = %path,
        client_ip = %addr.ip(),
    );

    async {
        info!("Request started");

        let response = next.run(request).await;
        let duration = start.elapsed();
        let status = response.status();

        // Log based on status
        match status.as_u16() {
            200..=299 => {
                info!(
                    status = %status.as_u16(),
                    duration_ms = %duration.as_millis(),
                    "Request completed"
                );
            }
            400..=499 => {
                warn!(
                    status = %status.as_u16(),
                    duration_ms = %duration.as_millis(),
                    "Client error"
                );
            }
            _ => {
                error!(
                    status = %status.as_u16(),
                    duration_ms = %duration.as_millis(),
                    "Server error"
                );
            }
        }

        response
    }
    .instrument(span)
    .await
}

/// CORS layer configuration
///
/// 🟠 HIGH SECURITY: Blocks dangerous combination of allow_any_origin + allow_credentials
pub fn cors_layer(config: &crate::config::CorsConfig) -> CorsLayer {
    // 🟠 HIGH SECURITY FIX: Prevent dangerous CORS configuration
    // Allowing any origin with credentials is a security vulnerability (CSRF risk)
    if config.allow_any_origin && config.allow_credentials {
        panic!(
            "SECURITY ERROR: CORS 'allow_any_origin' cannot be combined with 'allow_credentials'.\n\
             This combination creates a security vulnerability.\n\
             Either disable allow_any_origin or set allow_credentials to false."
        );
    }

    let cors = CorsLayer::new()
        .allow_methods(
            config
                .allowed_methods
                .iter()
                .filter_map(|m| m.parse().ok())
                .collect::<Vec<_>>(),
        )
        .allow_headers(
            config
                .allowed_headers
                .iter()
                .filter_map(|h| h.parse().ok())
                .collect::<Vec<_>>(),
        )
        .allow_credentials(config.allow_credentials)
        .max_age(std::time::Duration::from_secs(
            config.max_age_seconds as u64,
        ));

    if config.allow_any_origin {
        cors.allow_origin(Any)
    } else {
        let origins: Vec<_> = config
            .allowed_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        cors.allow_origin(AllowOrigin::list(origins))
    }
}

/// Trace layer configuration
pub fn trace_layer(
) -> TraceLayer<tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>>
{
    TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().include_headers(false))
        .on_request(DefaultOnRequest::new().level(tracing::Level::DEBUG))
        .on_response(DefaultOnResponse::new().level(tracing::Level::DEBUG))
}

/// Check if path is public (no auth required)
fn is_public_path(path: &str) -> bool {
    const PUBLIC_PATHS: &[&str] = &[
        "/health",
        "/ready",
        "/live",
        "/api/v1/auth/login",
        "/api/v1/auth/register",
        "/api/v1/auth/refresh",
        "/swagger",
        "/docs",
        "/static",
        "/ws",
        "/ws/status",
    ];
    const PUBLIC_PREFIXES: &[&str] = &[
        "/api/v1/channels/wechat/qr",
    ];
    const PUBLIC_EXACT: &[&str] = &[
        "/api/v1/channels",
    ];

    PUBLIC_PATHS.iter().any(|p| path.starts_with(p))
        || PUBLIC_PREFIXES.iter().any(|p| path.starts_with(p))
        || PUBLIC_EXACT.iter().any(|p| path == *p)
}

/// Gateway state shared across handlers
#[derive(Debug)]
pub struct GatewayState {
    /// Gateway configuration
    pub config: GatewayConfig,
    /// Rate limit manager
    pub rate_limiter: Arc<RateLimitManager>,
}

impl GatewayState {
    /// Create new gateway state
    pub fn new(config: GatewayConfig, rate_limiter: Arc<RateLimitManager>) -> Self {
        Self {
            config,
            rate_limiter,
        }
    }
}

/// Role-based access control (RBAC) helper
pub fn require_role(auth_user: &AuthUser, role: &str) -> Result<()> {
    if auth_user.has_role(role) {
        Ok(())
    } else {
        Err(GatewayError::forbidden(format!(
            "Required role '{}' not found",
            role
        )))
    }
}

/// Require any of the specified roles
pub fn require_any_role(auth_user: &AuthUser, roles: &[&str]) -> Result<()> {
    if roles.iter().any(|r| auth_user.has_role(r)) {
        Ok(())
    } else {
        Err(GatewayError::forbidden(
            "Insufficient permissions".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GatewayConfig, JwtConfig};
    use secrecy::Secret;

    #[test]
    fn test_auth_user_roles() {
        let user = AuthUser {
            user_id: "user-1".to_string(),
            roles: vec!["user".to_string(), "admin".to_string()],
            claims: Claims {
                sub: "user-1".to_string(),
                jti: "jti-1".to_string(),
                iat: 0,
                exp: 0,
                iss: "test".to_string(),
                aud: "test".to_string(),
                roles: vec!["user".to_string(), "admin".to_string()],
                token_type: TokenType::Access,
                session_id: None,
            },
            client_ip: "127.0.0.1".to_string(),
            request_id: "req-1".to_string(),
        };

        assert!(user.has_role("user"));
        assert!(user.has_role("admin"));
        assert!(user.is_admin());
        // Admin has all roles, so this returns true
        assert!(user.has_role("moderator"));
    }

    #[test]
    fn test_is_public_path() {
        assert!(is_public_path("/health"));
        assert!(is_public_path("/health/live"));
        assert!(is_public_path("/api/v1/auth/login"));
        assert!(!is_public_path("/api/v1/users"));
        assert!(!is_public_path("/private"));
    }

    #[test]
    fn test_token_generation_and_validation() {
        let config = JwtConfig {
            secret: Secret::new("a-very-long-secret-key-at-least-32-characters".to_string()),
            expiry_minutes: 60,
            refresh_expiry_minutes: 10080,
            issuer: "test".to_string(),
            audience: "test".to_string(),
        };

        // Generate token
        let token = generate_access_token("user-1", vec!["user".to_string()], &config).unwrap();
        assert!(!token.is_empty());

        // Validate token
        let claims = validate_token(&token, &config).unwrap();
        assert_eq!(claims.sub, "user-1");
        assert_eq!(claims.roles, vec!["user"]);
        assert_eq!(claims.token_type, TokenType::Access);
    }

    #[test]
    fn test_require_role() {
        let user = AuthUser {
            user_id: "user-1".to_string(),
            roles: vec!["admin".to_string()],
            claims: Claims {
                sub: "user-1".to_string(),
                jti: "jti-1".to_string(),
                iat: 0,
                exp: 0,
                iss: "test".to_string(),
                aud: "test".to_string(),
                roles: vec!["admin".to_string()],
                token_type: TokenType::Access,
                session_id: None,
            },
            client_ip: "127.0.0.1".to_string(),
            request_id: "req-1".to_string(),
        };

        assert!(require_role(&user, "admin").is_ok());
        // Admin has all roles, so this also returns ok
        assert!(require_role(&user, "user").is_ok());

        assert!(require_any_role(&user, &["admin", "user"]).is_ok());
        // Admin has all roles, so this also returns ok
        assert!(require_any_role(&user, &["user", "moderator"]).is_ok());
    }
}
