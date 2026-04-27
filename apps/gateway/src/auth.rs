//! Authentication and Authorization
//!
//! API Key authentication support.
//! JWT authentication is provided by beebotos-gateway-lib.

use bcrypt::{hash, DEFAULT_COST};
// Use gateway-lib for JWT and error handling
use gateway::error::GatewayError;
pub use gateway::middleware::AuthUser;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// API key authentication record
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ApiKeyAuth {
    pub key_id: String,
    pub owner_id: String,
    pub scopes: Vec<String>,
}

/// API key record for database storage (SQLite compatible)
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[allow(dead_code)]
pub struct ApiKeyRecord {
    pub id: uuid::Uuid,
    pub owner_id: String,
    /// Scopes stored as JSON string in SQLite
    pub scopes: String,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub is_revoked: bool,
}

impl ApiKeyRecord {
    /// Parse scopes from JSON string
    pub fn parsed_scopes(&self) -> Vec<String> {
        serde_json::from_str(&self.scopes).unwrap_or_default()
    }
}

/// Validate API key with timing-safe comparison
///
/// Uses HMAC-SHA256 for timing-safe key comparison to prevent timing attacks.
#[allow(dead_code)]
pub async fn validate_api_key(
    db: &sqlx::SqlitePool,
    key: &str,
) -> Result<ApiKeyAuth, GatewayError> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    // Use HMAC with a secret key for timing-safe comparison
    type HmacSha256 = Hmac<Sha256>;

    // Get the HMAC key from environment (must be set at startup)
    // SEC-001 FIX: Removed hardcoded default, require explicit configuration
    let hmac_key = std::env::var("API_KEY_HMAC_SECRET").map_err(|_| {
        tracing::error!("API_KEY_HMAC_SECRET environment variable not set");
        GatewayError::service_unavailable("Auth", "API_KEY_HMAC_SECRET not configured")
    })?;

    // Compute HMAC of the provided key
    let mut mac = HmacSha256::new_from_slice(hmac_key.as_bytes())
        .map_err(|_| GatewayError::internal("HMAC initialization failed"))?;
    mac.update(key.as_bytes());
    let key_hash = hex::encode(mac.finalize().into_bytes());

    // SEC-002 FIX: Optimized query with index on key_hash
    // Query specific key hash instead of scanning all keys
    // SQLite syntax: use ?1 instead of $1 and datetime('now') instead of NOW()
    let record: Option<ApiKeyRecord> = sqlx::query_as(
        r#"SELECT id, owner_id, scopes, expires_at, is_revoked
           FROM api_keys
           WHERE key_hash = ?1
           AND is_revoked = false
           AND (expires_at IS NULL OR expires_at > datetime('now'))"#,
    )
    .bind(&key_hash)
    .fetch_optional(db)
    .await
    .map_err(|e| {
        tracing::error!("Database error during API key validation: {}", e);
        GatewayError::internal("Authentication failed")
    })?;

    match record {
        Some(record) => {
            // Update last_used_at (fire and forget, don't fail on error)
            let db_clone = db.clone();
            let record_id = record.id;
            tokio::spawn(async move {
                let _ =
                    sqlx::query("UPDATE api_keys SET last_used_at = datetime('now') WHERE id = ?1")
                        .bind(&record_id)
                        .execute(&db_clone)
                        .await;
            });

            Ok(ApiKeyAuth {
                key_id: record.id.to_string(),
                owner_id: record.owner_id.clone(),
                scopes: record.parsed_scopes(),
            })
        }
        None => {
            // Use constant-time delay to prevent timing attacks
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            Err(GatewayError::unauthorized("Invalid API key"))
        }
    }
}

/// Generate new API key
#[allow(dead_code)]
pub fn generate_api_key() -> String {
    let random_bytes: [u8; 32] = rand::random();
    format!("bee_{}", hex::encode(random_bytes))
}

/// Hash API key for storage
///
/// # Panics
/// Panics if bcrypt hashing fails, as this indicates a serious system issue
/// that should not be silently ignored.
#[allow(dead_code)]
pub fn hash_api_key(key: &str) -> String {
    // Use bcrypt for secure hashing
    // bcrypt failure is a serious error that should not be silently ignored
    hash(key, DEFAULT_COST).expect("bcrypt hashing failed - this indicates a serious system issue")
}

/// Role-based access control
#[allow(dead_code)]
pub struct Rbac;

#[allow(dead_code)]
impl Rbac {
    /// Check if user has required role
    pub fn has_role(user: &AuthUser, role: &str) -> bool {
        user.has_role(role)
    }

    /// Check if user is admin
    #[allow(dead_code)]
    pub fn is_admin(user: &AuthUser) -> bool {
        user.is_admin()
    }

    /// Check if user can manage specific agent
    #[allow(dead_code)]
    pub fn can_manage_agent(user: &AuthUser, agent_owner_id: Option<&str>) -> bool {
        user.is_admin() || agent_owner_id == Some(&user.user_id)
    }

    /// Check if API key has required scope
    #[allow(dead_code)]
    pub fn has_scope(auth: &ApiKeyAuth, scope: &str) -> bool {
        auth.scopes.contains(&scope.to_string()) || auth.scopes.contains(&"*".to_string())
    }
}

/// Check if session is revoked
#[allow(dead_code)]
async fn is_session_revoked(db: &sqlx::SqlitePool, jti: &str) -> Result<bool, GatewayError> {
    // SQLite syntax: use ?1 instead of $1 and datetime('now') instead of NOW()
    let record: (i64,) = sqlx::query_as(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM sessions
            WHERE token_jti = ?1 AND (is_revoked = true OR expires_at < datetime('now'))
        )
        "#,
    )
    .bind(jti)
    .fetch_one(db)
    .await
    .map_err(|e| GatewayError::internal(format!("Database error: {}", e)))?;

    Ok(record.0 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_generation() {
        let key = generate_api_key();
        assert!(key.starts_with("bee_"));
        assert_eq!(key.len(), 4 + 64); // "bee_" + 64 hex chars
    }

    #[test]
    fn test_role_check() {
        use gateway::middleware::{Claims, TokenType};

        let user = AuthUser {
            user_id: "user123".to_string(),
            claims: Claims {
                sub: "user123".to_string(),
                jti: "test".to_string(),
                iat: 0,
                exp: 0,
                iss: "test".to_string(),
                aud: "test".to_string(),
                token_type: TokenType::Access,
                session_id: None,
            },
            client_ip: "127.0.0.1".to_string(),
            request_id: "req-1".to_string(),
        };

        // Role checks are disabled - all authenticated users have all roles
        assert!(Rbac::has_role(&user, "user"));
        assert!(Rbac::has_role(&user, "agent_manager"));
        assert!(Rbac::has_role(&user, "admin"));
        assert!(Rbac::is_admin(&user));
    }
}
