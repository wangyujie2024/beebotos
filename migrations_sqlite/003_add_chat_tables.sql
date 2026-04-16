-- Migration: Add unified chat tables (SQLite version)
-- Supports webchat, personal_wechat, lark, dingtalk, qq, feishu, etc.

-- Unified chat sessions table: isolated by user_id, channel distinguishes source
CREATE TABLE chat_sessions (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    user_id TEXT NOT NULL,
    channel TEXT NOT NULL DEFAULT 'webchat',
    title TEXT NOT NULL DEFAULT 'New Chat',
    is_pinned INTEGER DEFAULT 0,
    is_archived INTEGER DEFAULT 0,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX idx_chat_sessions_user ON chat_sessions(user_id);
CREATE INDEX idx_chat_sessions_user_updated ON chat_sessions(user_id, updated_at);
CREATE INDEX idx_chat_sessions_channel ON chat_sessions(channel);

-- Unified chat messages table
CREATE TABLE chat_messages (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    session_id TEXT NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
    role TEXT NOT NULL,             -- 'user', 'assistant', 'system'
    content TEXT NOT NULL,
    metadata TEXT DEFAULT '{}',     -- JSON: platform, sender_id, message_id, has_image, etc.
    token_usage TEXT,               -- JSON: prompt_tokens, completion_tokens, model, etc.
    created_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX idx_chat_messages_session ON chat_messages(session_id);
CREATE INDEX idx_chat_messages_created ON chat_messages(created_at);

-- Trigger: update session timestamp on modification
CREATE TRIGGER update_chat_sessions_updated_at AFTER UPDATE ON chat_sessions
BEGIN
    UPDATE chat_sessions SET updated_at = datetime('now') WHERE id = NEW.id;
END;
