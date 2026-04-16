//! Authentication service

use argon2::{
    password_hash::{
        rand_core::OsRng,
        PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
    },
    Argon2,
};
use sqlx::SqlitePool;

use crate::error::AppError;

/// Authenticated user info (non-sensitive)
#[derive(Clone, Debug, serde::Serialize)]
pub struct AuthUserInfo {
    pub id: String,
    pub username: String,
    pub email: Option<String>,
    pub avatar: Option<String>,
    pub wallet_address: Option<String>,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
}

/// DB row for users table
#[derive(Clone, Debug, sqlx::FromRow)]
struct UserRow {
    id: String,
    username: String,
    email: Option<String>,
    password_hash: String,
    avatar: Option<String>,
    wallet_address: Option<String>,
    roles: String,
    permissions: String,
}

impl UserRow {
    fn to_auth_user_info(&self) -> AuthUserInfo {
        AuthUserInfo {
            id: self.id.clone(),
            username: self.username.clone(),
            email: self.email.clone(),
            avatar: self.avatar.clone(),
            wallet_address: self.wallet_address.clone(),
            roles: self.roles.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
            permissions: self.permissions.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
        }
    }
}

/// Authentication service
#[derive(Clone)]
pub struct AuthService {
    db: SqlitePool,
}

impl AuthService {
    pub fn new(db: SqlitePool) -> Self {
        Self { db }
    }

    /// Register a new user
    pub async fn register(
        &self,
        username: &str,
        email: Option<&str>,
        password: &str,
    ) -> Result<AuthUserInfo, AppError> {
        let password_hash = hash_password(password)?;

        let result = sqlx::query_as::<_, UserRow>(
            "INSERT INTO users (username, email, password_hash, roles, permissions)
             VALUES (?, ?, ?, 'member', 'agentRead,agentCreate,daoVote,settingsRead')
             RETURNING id, username, email, password_hash, avatar, wallet_address, roles, permissions"
        )
        .bind(username)
        .bind(email)
        .bind(password_hash)
        .fetch_one(&self.db)
        .await;

        match result {
            Ok(row) => Ok(row.to_auth_user_info()),
            Err(sqlx::Error::Database(db_err)) if db_err.message().contains("UNIQUE") => {
                Err(AppError::Validation(vec![crate::error::ValidationError {
                    field: "username".to_string(),
                    message: "Username already exists".to_string(),
                    code: "ALREADY_EXISTS".to_string(),
                }]))
            }
            Err(e) => Err(AppError::Internal(format!("Failed to create user: {}", e))),
        }
    }

    /// Authenticate a user by username and password
    pub async fn authenticate(
        &self,
        username: &str,
        password: &str,
    ) -> Result<AuthUserInfo, AppError> {
        let row = sqlx::query_as::<_, UserRow>(
            "SELECT id, username, email, password_hash, avatar, wallet_address, roles, permissions
             FROM users WHERE username = ?"
        )
        .bind(username)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

        let row = row.ok_or_else(|| AppError::Internal("Invalid credentials".to_string()))?;

        verify_password(password, &row.password_hash)
            .map_err(|_| AppError::Internal("Invalid credentials".to_string()))?;

        Ok(row.to_auth_user_info())
    }

    /// Get user by ID
    pub async fn get_user_by_id(&self, id: &str) -> Result<AuthUserInfo, AppError> {
        let row = sqlx::query_as::<_, UserRow>(
            "SELECT id, username, email, password_hash, avatar, wallet_address, roles, permissions
             FROM users WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

        row.map(|r| r.to_auth_user_info())
            .ok_or_else(|| AppError::not_found("User", id))
    }
}

fn hash_password(password: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(format!("Password hashing failed: {}", e)))
}

fn verify_password(password: &str, hash: &str) -> Result<(), argon2::password_hash::Error> {
    let parsed_hash = PasswordHash::new(hash)?;
    Argon2::default().verify_password(password.as_bytes(), &parsed_hash)
}
