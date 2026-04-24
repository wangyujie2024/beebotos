//! Procedural Memory
//!
//! Memory for skills, habits, and action sequences.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Procedural memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProceduralMemory {
    procedures: HashMap<String, Procedure>,
}

/// Procedure (skill/action sequence)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Procedure {
    pub id: String,
    pub name: String,
    pub description: String,
    pub steps: Vec<Step>,
    pub success_rate: f32,
    pub execution_count: u32,
    pub context_triggers: Vec<String>,
}

/// Procedure step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub id: String,
    pub action: String,
    pub expected_outcome: String,
    pub on_success: Option<String>, // Next step ID
    pub on_failure: Option<String>, // Error handling step ID
}

impl ProceduralMemory {
    pub fn new() -> Self {
        Self {
            procedures: HashMap::new(),
        }
    }

    /// Learn new procedure
    pub fn learn(&mut self, name: impl Into<String>, steps: Vec<Step>) -> String {
        let id = uuid::Uuid::new_v4().to_string();

        let procedure = Procedure {
            id: id.clone(),
            name: name.into(),
            description: String::new(),
            steps,
            success_rate: 0.5,
            execution_count: 0,
            context_triggers: vec![],
        };

        self.procedures.insert(id.clone(), procedure);
        id
    }

    /// Get procedure
    pub fn get(&self, id: &str) -> Option<&Procedure> {
        self.procedures.get(id)
    }

    /// Get mutable
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Procedure> {
        self.procedures.get_mut(id)
    }

    /// Find by name
    pub fn find_by_name(&self, name: &str) -> Option<&Procedure> {
        self.procedures.values().find(|p| p.name == name)
    }

    /// Record execution result
    pub fn record_execution(&mut self, id: &str, success: bool) {
        if let Some(proc) = self.procedures.get_mut(id) {
            proc.execution_count += 1;

            // Update success rate using exponential moving average
            let alpha = 0.1;
            let current = if success { 1.0 } else { 0.0 };
            proc.success_rate = proc.success_rate * (1.0 - alpha) + current * alpha;
        }
    }

    /// Get well-practiced procedures (high success rate and count)
    pub fn well_practiced(&self) -> Vec<&Procedure> {
        self.procedures
            .values()
            .filter(|p| p.success_rate > 0.8 && p.execution_count > 10)
            .collect()
    }

    /// List all procedures
    pub fn list_all(&self) -> Vec<&Procedure> {
        self.procedures.values().collect()
    }

    /// Search procedures by name or description
    pub fn search(&self, query: &str) -> Vec<&Procedure> {
        let query_lower = query.to_lowercase();
        self.procedures
            .values()
            .filter(|p| {
                p.name.to_lowercase().contains(&query_lower)
                    || p.description.to_lowercase().contains(&query_lower)
            })
            .collect()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.procedures.is_empty()
    }
}

impl Default for ProceduralMemory {
    fn default() -> Self {
        Self::new()
    }
}
