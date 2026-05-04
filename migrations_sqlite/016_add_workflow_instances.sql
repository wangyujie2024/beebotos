-- Workflow instances persistence
-- Stores runtime state of workflow executions for durability across restarts

CREATE TABLE IF NOT EXISTS workflow_instances (
    id TEXT PRIMARY KEY NOT NULL,
    workflow_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    trigger_context TEXT NOT NULL DEFAULT '{}',
    step_states TEXT NOT NULL DEFAULT '{}',
    error_log TEXT NOT NULL DEFAULT '[]',
    started_at TEXT NOT NULL,
    completed_at TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_workflow_instances_workflow_id ON workflow_instances(workflow_id);
CREATE INDEX IF NOT EXISTS idx_workflow_instances_status ON workflow_instances(status);
CREATE INDEX IF NOT EXISTS idx_workflow_instances_started_at ON workflow_instances(started_at);
