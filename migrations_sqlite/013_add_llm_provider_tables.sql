-- 013_add_llm_provider_tables.sql
-- LLM Provider and Model management tables

CREATE TABLE IF NOT EXISTS llm_providers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_id TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    protocol TEXT NOT NULL CHECK(protocol IN ('openai-compatible', 'anthropic')),
    base_url TEXT,
    api_key_encrypted TEXT,
    enabled BOOLEAN NOT NULL DEFAULT true,
    is_default_provider BOOLEAN NOT NULL DEFAULT false,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS llm_models (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    display_name TEXT,
    is_default_model BOOLEAN NOT NULL DEFAULT false,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (provider_id) REFERENCES llm_providers(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_provider_id ON llm_providers(provider_id);
CREATE INDEX IF NOT EXISTS idx_models_provider ON llm_models(provider_id);
