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

const DEFAULT_ROLES: &str = "user";
const DEFAULT_PERMISSIONS: &str = "agentRead,agentCreate,daoVote,settingsRead";

/// Authenticated user info (non-sensitive)
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AuthUserInfo {
    pub id: String,
    #[serde(rename = "name")]
    pub username: String,
    pub email: Option<String>,
    pub avatar: Option<String>,
    pub wallet_address: Option<String>,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
}

/// DB row for users table (includes password hash)
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

/// DB row for public user queries (excludes password hash)
#[derive(Clone, Debug, sqlx::FromRow)]
struct UserPublicRow {
    id: String,
    username: String,
    email: Option<String>,
    avatar: Option<String>,
    wallet_address: Option<String>,
    roles: String,
    permissions: String,
}

impl UserPublicRow {
    fn to_auth_user_info(&self) -> AuthUserInfo {
        AuthUserInfo {
            id: self.id.clone(),
            username: self.username.clone(),
            email: self.email.clone(),
            avatar: self.avatar.clone(),
            wallet_address: self.wallet_address.clone(),
            roles: parse_comma_list(&self.roles),
            permissions: parse_comma_list(&self.permissions),
        }
    }
}

impl UserRow {
    fn to_auth_user_info(&self) -> AuthUserInfo {
        AuthUserInfo {
            id: self.id.clone(),
            username: self.username.clone(),
            email: self.email.clone(),
            avatar: self.avatar.clone(),
            wallet_address: self.wallet_address.clone(),
            roles: parse_comma_list(&self.roles),
            permissions: parse_comma_list(&self.permissions),
        }
    }
}

fn parse_comma_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
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
             VALUES (?, ?, ?, ?, ?)
             RETURNING id, username, email, password_hash, avatar, wallet_address, roles, permissions"
        )
        .bind(username)
        .bind(email.unwrap_or(""))
        .bind(password_hash)
        .bind(DEFAULT_ROLES)
        .bind(DEFAULT_PERMISSIONS)
        .fetch_one(&self.db)
        .await;

        match result {
            Ok(row) => Ok(row.to_auth_user_info()),
            Err(sqlx::Error::Database(db_err)) => {
                let msg = db_err.message().to_lowercase();
                if msg.contains("unique constraint failed") {
                    let field = if msg.contains("email") { "email" } else { "username" };
                    Err(AppError::Validation(vec![crate::error::ValidationError {
                        field: field.to_string(),
                        message: format!("{} already exists", field),
                        code: "ALREADY_EXISTS".to_string(),
                    }]))
                } else if msg.contains("not null constraint failed") {
                    let field = if msg.contains("email") { "email" } else { "username" };
                    Err(AppError::Validation(vec![crate::error::ValidationError {
                        field: field.to_string(),
                        message: format!("{} is required", field),
                        code: "REQUIRED".to_string(),
                    }]))
                } else {
                    Err(AppError::database(sqlx::Error::Database(db_err)))
                }
            }
            Err(e) => Err(AppError::database(e)),
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
        .await?;

        let row = row.ok_or_else(|| AppError::Unauthorized("Invalid credentials".to_string()))?;

        verify_password(password, &row.password_hash)
            .map_err(|_| AppError::Unauthorized("Invalid credentials".to_string()))?;

        Ok(row.to_auth_user_info())
    }

    /// Get user by ID
    pub async fn get_user_by_id(&self, id: &str) -> Result<AuthUserInfo, AppError> {
        let row = sqlx::query_as::<_, UserPublicRow>(
            "SELECT id, username, email, avatar, wallet_address, roles, permissions
             FROM users WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.db)
        .await?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    async fn create_test_db() -> SqlitePool {
        SqlitePool::connect("sqlite::memory:")
            .await
            .expect("Failed to create in-memory SQLite pool")
    }

    async fn run_migrations(pool: &SqlitePool) {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
                username TEXT UNIQUE NOT NULL,
                email TEXT NOT NULL UNIQUE,
                password_hash TEXT NOT NULL,
                avatar TEXT,
                wallet_address TEXT,
                roles TEXT NOT NULL DEFAULT 'user',
                permissions TEXT NOT NULL DEFAULT 'agentRead,agentCreate,daoVote,settingsRead'
            )"
        )
        .execute(pool)
        .await
        .expect("Failed to create users table");
    }

    #[tokio::test]
    async fn test_register_success() {
        let pool = create_test_db().await;
        run_migrations(&pool).await;
        let service = AuthService::new(pool);

        let user = service
            .register("alice", Some("alice@example.com"), "password123")
            .await
            .expect("register should succeed");

        assert_eq!(user.username, "alice");
        assert_eq!(user.email, Some("alice@example.com".to_string()));
        assert_eq!(user.roles, vec!["user"]);
    }

    #[tokio::test]
    async fn test_register_duplicate_username() {
        let pool = create_test_db().await;
        run_migrations(&pool).await;
        let service = AuthService::new(pool);

        service
            .register("bob", Some("bob@example.com"), "password123")
            .await
            .unwrap();

        let err = service
            .register("bob", Some("bob2@example.com"), "password123")
            .await
            .expect_err("duplicate username should fail");

        match err {
            AppError::Validation(errors) => {
                assert_eq!(errors[0].field, "username");
                assert!(errors[0].message.contains("already exists"));
            }
            _ => panic!("Expected validation error, got {:?}", err),
        }
    }

    #[tokio::test]
    async fn test_register_duplicate_email() {
        let pool = create_test_db().await;
        run_migrations(&pool).await;
        let service = AuthService::new(pool);

        service
            .register("bob", Some("bob@example.com"), "password123")
            .await
            .unwrap();

        let err = service
            .register("alice", Some("bob@example.com"), "password123")
            .await
            .expect_err("duplicate email should fail");

        match err {
            AppError::Validation(errors) => {
                assert_eq!(errors[0].field, "email");
                assert!(errors[0].message.contains("already exists"));
            }
            _ => panic!("Expected validation error, got {:?}", err),
        }
    }

    #[tokio::test]
    async fn test_authenticate_success() {
        let pool = create_test_db().await;
        run_migrations(&pool).await;
        let service = AuthService::new(pool);

        service
            .register("charlie", None, "securepass")
            .await
            .unwrap();

        let user = service
            .authenticate("charlie", "securepass")
            .await
            .expect("authenticate should succeed");

        assert_eq!(user.username, "charlie");
    }

    #[tokio::test]
    async fn test_authenticate_wrong_password() {
        let pool = create_test_db().await;
        run_migrations(&pool).await;
        let service = AuthService::new(pool);

        service
            .register("dave", None, "correctpass")
            .await
            .unwrap();

        let err = service
            .authenticate("dave", "wrongpass")
            .await
            .expect_err("wrong password should fail");

        match err {
            AppError::Unauthorized(msg) => {
                assert!(msg.contains("Invalid credentials"));
            }
            _ => panic!("Expected unauthorized error, got {:?}", err),
        }
    }

    #[tokio::test]
    async fn test_authenticate_nonexistent_user() {
        let pool = create_test_db().await;
        run_migrations(&pool).await;
        let service = AuthService::new(pool);

        let err = service
            .authenticate("ghost", "anypass")
            .await
            .expect_err("nonexistent user should fail");

        match err {
            AppError::Unauthorized(msg) => {
                assert!(msg.contains("Invalid credentials"));
            }
            _ => panic!("Expected unauthorized error, got {:?}", err),
        }
    }

    #[tokio::test]
    async fn test_get_user_by_id() {
        let pool = create_test_db().await;
        run_migrations(&pool).await;
        let service = AuthService::new(pool);

        let registered = service
            .register("eve", None, "password123")
            .await
            .unwrap();

        let fetched = service
            .get_user_by_id(&registered.id)
            .await
            .expect("get_user_by_id should succeed");

        assert_eq!(fetched.username, "eve");
        assert_eq!(fetched.id, registered.id);
    }

    #[tokio::test]
    async fn test_get_user_by_id_not_found() {
        let pool = create_test_db().await;
        run_migrations(&pool).await;
        let service = AuthService::new(pool);

        let err = service
            .get_user_by_id("nonexistent-id")
            .await
            .expect_err("should fail for unknown id");

        match err {
            AppError::NotFound(msg) => {
                assert!(msg.contains("User"));
            }
            _ => panic!("Expected NotFound error, got {:?}", err),
        }
    }
}
