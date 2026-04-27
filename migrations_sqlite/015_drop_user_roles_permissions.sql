-- Migration: Drop roles and permissions columns from users table
-- Role-based access control has been removed from the system

-- SQLite does not support DROP COLUMN directly, so we need to recreate the table
-- Step 1: Create new table without roles/permissions
CREATE TABLE IF NOT EXISTS users_new (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    username TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    avatar TEXT,
    wallet_address TEXT,
    is_active INTEGER DEFAULT 1,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now'))
);

-- Step 2: Copy data from old table
INSERT INTO users_new (id, username, email, password_hash, avatar, wallet_address, is_active, created_at, updated_at)
SELECT id, username, email, password_hash, avatar, wallet_address, is_active, created_at, updated_at
FROM users;

-- Step 3: Drop old table
DROP TABLE users;

-- Step 4: Rename new table
ALTER TABLE users_new RENAME TO users;

-- Step 5: Recreate indexes
CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);

-- Step 6: Recreate trigger
CREATE TRIGGER IF NOT EXISTS update_users_updated_at AFTER UPDATE ON users
BEGIN
    UPDATE users SET updated_at = datetime('now') WHERE id = NEW.id;
END;
