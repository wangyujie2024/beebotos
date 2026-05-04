//! Workflow Orchestration Module
//!
//! Provides declarative workflow definition, template resolution,
//! state management, and execution engine for BeeBotOS.
//!
//! # Architecture
//! ```text
//! WorkflowDefinition (YAML/JSON) → Template Resolution → DagWorkflow → DagScheduler
//! ```

pub mod dag_bridge;
pub mod definition;
pub mod engine;
pub mod state;
pub mod template;
pub mod trigger;

pub use definition::{
    ErrorHandler, FallbackAction, StepErrorHandler,
    TriggerDefinition, TriggerType, WorkflowDefinition, WorkflowGlobalConfig, WorkflowStep,
};
pub use dag_bridge::{to_dag_workflow, WorkflowDagExecutor};
pub use engine::{SkillStepResult, StepExecutor, StepProgressReporter, WorkflowEngine};
pub use state::{
    StepState, StepStatus, WorkflowError, WorkflowId, WorkflowInstance, WorkflowInstanceId, WorkflowStatus,
};
pub use template::{resolve_template, TemplateContext, TemplateError};
pub use trigger::{TriggerEngine, TriggerMatch};


/// Registry of loaded workflow definitions
#[derive(Debug, Clone, Default)]
pub struct WorkflowRegistry {
    workflows: std::collections::HashMap<WorkflowId, WorkflowDefinition>,
}

impl WorkflowRegistry {
    /// Create empty registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a workflow definition
    pub fn register(&mut self, def: WorkflowDefinition) {
        self.workflows.insert(def.id.clone(), def);
    }

    /// Get workflow by ID
    pub fn get(&self, id: &str) -> Option<&WorkflowDefinition> {
        self.workflows.get(id)
    }

    /// List all registered workflows
    pub fn list_all(&self) -> Vec<&WorkflowDefinition> {
        self.workflows.values().collect()
    }

    /// Remove a workflow
    pub fn remove(&mut self, id: &str) -> Option<WorkflowDefinition> {
        self.workflows.remove(id)
    }

    /// Load all YAML/JSON workflow files from a directory
    pub async fn load_from_dir(&mut self, dir: &std::path::Path) -> Result<(), WorkflowRegistryError> {
        let mut entries = match tokio::fs::read_dir(dir).await {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("Workflow directory not found: {}", e);
                return Ok(());
            }
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str());
            let is_workflow_file = ext == Some("yaml")
                || ext == Some("yml")
                || ext == Some("json");

            if is_workflow_file {
                match tokio::fs::read_to_string(&path).await {
                    Ok(content) => {
                        // Parse based on file extension
                        let parse_json = || serde_json::from_str::<WorkflowDefinition>(&content);
                        let parse_yaml = || serde_yaml::from_str::<WorkflowDefinition>(&content);

                        let parsed = if ext == Some("json") {
                            parse_json().map_err(|e| format!("{}", e))
                        } else {
                            // YAML parser can also parse JSON, so try YAML first
                            parse_yaml().map_err(|e| format!("{}", e))
                        };

                        match parsed {
                            Ok(mut def) => {
                                // OpenClaw compatibility: if id is empty, use name
                                if def.id.is_empty() {
                                    def.id = def.name.clone();
                                }
                                // If name is empty, use id
                                if def.name.is_empty() {
                                    def.name = def.id.clone();
                                }
                                // Final fallback: filename
                                if def.id.is_empty() {
                                    def.id = path
                                        .file_stem()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_string();
                                }
                                if def.name.is_empty() {
                                    def.name = def.id.clone();
                                }
                                tracing::info!("Loaded workflow: {} ({}) from {:?}", def.name, def.id, path);
                                self.register(def);
                            }
                            Err(e) => {
                                tracing::warn!("Failed to parse workflow {:?}: {}", path, e);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to read workflow {:?}: {}", path, e);
                    }
                }
            }
        }

        Ok(())
    }
}

/// Workflow registry error
#[derive(Debug, Clone)]
pub enum WorkflowRegistryError {
    Io(String),
    Parse(String),
}

impl std::fmt::Display for WorkflowRegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkflowRegistryError::Io(msg) => write!(f, "IO error: {}", msg),
            WorkflowRegistryError::Parse(msg) => write!(f, "Parse error: {}", msg),
        }
    }
}

impl std::error::Error for WorkflowRegistryError {}

#[cfg(test)]
mod tests;
