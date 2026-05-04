-- 014_add_provider_metadata.sql
-- Add icon, icon_color, type_label columns to llm_providers

-- Add new columns
ALTER TABLE llm_providers ADD COLUMN icon TEXT;
ALTER TABLE llm_providers ADD COLUMN icon_color TEXT;
ALTER TABLE llm_providers ADD COLUMN type_label TEXT;

-- Update existing preset providers with their icons and labels
UPDATE llm_providers SET icon = '🤖', icon_color = '#10a37f', type_label = '内置' WHERE provider_id = 'openai';
UPDATE llm_providers SET icon = '🅰️', icon_color = '#d4a574', type_label = '内置' WHERE provider_id = 'anthropic';
UPDATE llm_providers SET icon = '🌙', icon_color = '#4f6ef7', type_label = '内置' WHERE provider_id = 'kimi';
UPDATE llm_providers SET icon = '🔍', icon_color = '#4d6bfa', type_label = '内置' WHERE provider_id = 'deepseek';
UPDATE llm_providers SET icon = '🧠', icon_color = '#3b82f6', type_label = '内置' WHERE provider_id = 'zhipu';
UPDATE llm_providers SET icon = '🦙', icon_color = '#ff6b6b', type_label = '本地' WHERE provider_id = 'ollama';
