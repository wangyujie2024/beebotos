-- Migration: Add users table for authentication and authorization (SQLite version)
-- Supports local account management with role-based access control

-- User accounts table
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    username TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    -- roles and permissions are stored as comma-delimited TEXT to match the frontend/backend contract
    roles TEXT NOT NULL DEFAULT 'member',
    permissions TEXT NOT NULL DEFAULT 'agentRead,agentCreate,daoVote,settingsRead',
    avatar TEXT,
    wallet_address TEXT,
    is_active INTEGER DEFAULT 1,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);

-- Trigger to auto-update updated_at
CREATE TRIGGER IF NOT EXISTS update_users_updated_at AFTER UPDATE ON users
BEGIN
    UPDATE users SET updated_at = datetime('now') WHERE id = NEW.id;
END;
