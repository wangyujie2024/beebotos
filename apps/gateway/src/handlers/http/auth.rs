//! Authentication HTTP handlers

use std::sync::Arc;

use axum::{extract::State, Json};
use chrono::{Duration as ChronoDuration, Utc};
use gateway::{
    error::GatewayError,
    middleware::{AuthUser, Claims, TokenType},
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    services::auth_service::{AuthService, AuthUserInfo},
    AppState,
};

/// Login request
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Register request
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: Option<String>,
    pub password: String,
}

/// Auth response
#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub user: AuthUserInfo,
}

/// Token refresh request
#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

fn get_auth_service(state: &AppState) -> Result<&AuthService, GatewayError> {
    state
        .auth_service
        .as_ref()
        .map(|arc| arc.as_ref())
        .ok_or_else(|| GatewayError::internal("Auth service not initialized"))
}

fn generate_access_token(
    user_id: &str,
    roles: Vec<String>,
    state: &AppState,
) -> Result<String, GatewayError> {
    let now = Utc::now();
    let jti = Uuid::new_v4().to_string();
    let claims = Claims {
        sub: user_id.to_string(),
        jti: jti.clone(),
        iat: now.timestamp(),
        exp: (now + ChronoDuration::hours(state.config.jwt.expiry_hours)).timestamp(),
        iss: state.config.jwt.issuer.clone(),
        aud: state.config.jwt.audience.clone(),
        roles,
        token_type: TokenType::Access,
        session_id: Some(jti),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.config.jwt.secret.expose_secret().as_bytes()),
    )
    .map_err(|e| GatewayError::internal(format!("Failed to generate token: {}", e)))
}

fn generate_refresh_token(user_id: &str, state: &AppState) -> Result<String, GatewayError> {
    let now = Utc::now();
    let jti = Uuid::new_v4().to_string();
    let claims = Claims {
        sub: user_id.to_string(),
        jti: jti.clone(),
        iat: now.timestamp(),
        exp: (now + ChronoDuration::hours(state.config.jwt.refresh_expiry_hours)).timestamp(),
        iss: state.config.jwt.issuer.clone(),
        aud: state.config.jwt.audience.clone(),
        roles: vec![],
        token_type: TokenType::Refresh,
        session_id: Some(jti),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.config.jwt.secret.expose_secret().as_bytes()),
    )
    .map_err(|e| GatewayError::internal(format!("Failed to generate refresh token: {}", e)))
}

/// POST /api/v1/auth/login
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, GatewayError> {
    let auth_service = get_auth_service(&state)?;
    let user = auth_service
        .authenticate(&req.username, &req.password)
        .await?;

    let access_token = generate_access_token(&user.id, user.roles.clone(), &state)?;
    let refresh_token = generate_refresh_token(&user.id, &state)?;

    Ok(Json(AuthResponse {
        access_token,
        refresh_token,
        expires_in: state.config.jwt.expiry_hours * 60 * 60,
        user,
    }))
}

/// POST /api/v1/auth/register
pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<AuthResponse>, GatewayError> {
    let auth_service = get_auth_service(&state)?;
    let user = auth_service
        .register(&req.username, req.email.as_deref(), &req.password)
        .await?;

    let access_token = generate_access_token(&user.id, user.roles.clone(), &state)?;
    let refresh_token = generate_refresh_token(&user.id, &state)?;

    Ok(Json(AuthResponse {
        access_token,
        refresh_token,
        expires_in: state.config.jwt.expiry_hours * 60 * 60,
        user,
    }))
}

/// POST /api/v1/auth/logout
pub async fn logout(_auth_user: AuthUser) -> Result<Json<serde_json::Value>, GatewayError> {
    // JWT is stateless; client must delete token from storage.
    // In the future, add refresh token blacklist here if needed.
    Ok(Json(serde_json::json!({ "success": true })))
}

/// POST /api/v1/auth/refresh
pub async fn refresh_token(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<serde_json::Value>, GatewayError> {
    let mut validation = Validation::default();
    validation.validate_exp = true;
    validation.set_audience(&[&state.config.jwt.audience]);
    validation.set_issuer(&[&state.config.jwt.issuer]);
    let token_data = decode::<Claims>(
        &req.refresh_token,
        &DecodingKey::from_secret(state.config.jwt.secret.expose_secret().as_bytes()),
        &validation,
    )
    .map_err(|_| GatewayError::unauthorized("Invalid refresh token"))?;

    let claims = token_data.claims;

    if claims.token_type != TokenType::Refresh {
        return Err(GatewayError::unauthorized("Invalid token type"));
    }

    let auth_service = get_auth_service(&state)?;
    let user = auth_service.get_user_by_id(&claims.sub).await?;
    let access_token = generate_access_token(&user.id, user.roles.clone(), &state)?;
    let refresh_token = generate_refresh_token(&user.id, &state)?;

    Ok(Json(serde_json::json!({
        "access_token": access_token,
        "refresh_token": refresh_token,
        "expires_in": state.config.jwt.expiry_hours * 60 * 60,
    })))
}

/// GET /api/v1/auth/me
pub async fn me(
    auth_user: AuthUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<AuthUserInfo>, GatewayError> {
    let auth_service = get_auth_service(&state)?;
    let user = auth_service.get_user_by_id(&auth_user.user_id).await?;
    Ok(Json(user))
}
