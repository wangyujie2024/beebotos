//! Skill Composition Registry
//!
//! Manages persisted composition definitions loaded from `data/compositions/*.yaml`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::definition::CompositionDefinition;

/// Registry of loaded composition definitions
#[derive(Debug, Clone, Default)]
pub struct CompositionRegistry {
    compositions: HashMap<String, CompositionDefinition>,
    base_dir: Option<PathBuf>,
}

/// Registry error
#[derive(Debug, Clone)]
pub enum RegistryError {
    Io(String),
    Parse(String),
    AlreadyExists(String),
    NotFound(String),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::Io(msg) => write!(f, "IO error: {}", msg),
            RegistryError::Parse(msg) => write!(f, "Parse error: {}", msg),
            RegistryError::AlreadyExists(id) => write!(f, "Composition '{}' already exists", id),
            RegistryError::NotFound(id) => write!(f, "Composition '{}' not found", id),
        }
    }
}

impl std::error::Error for RegistryError {}

impl CompositionRegistry {
    /// Create an empty registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a registry tied to a base directory for persistence
    pub fn with_dir(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            compositions: HashMap::new(),
            base_dir: Some(base_dir.into()),
        }
    }

    /// Register a composition definition (in-memory only)
    pub fn register(&mut self, def: CompositionDefinition) {
        self.compositions.insert(def.id.clone(), def);
    }

    /// Get composition by ID
    pub fn get(&self, id: &str) -> Option<&CompositionDefinition> {
        self.compositions.get(id)
    }

    /// Get mutable reference
    pub fn get_mut(&mut self, id: &str) -> Option<&mut CompositionDefinition> {
        self.compositions.get_mut(id)
    }

    /// List all registered compositions
    pub fn list_all(&self) -> Vec<&CompositionDefinition> {
        self.compositions.values().collect()
    }

    /// Remove a composition (returns the removed definition if any)
    pub fn remove(&mut self, id: &str) -> Option<CompositionDefinition> {
        self.compositions.remove(id)
    }

    /// Create a new composition and persist it
    pub async fn create(
        &mut self,
        mut def: CompositionDefinition,
    ) -> Result<(), RegistryError> {
        if self.compositions.contains_key(&def.id) {
            return Err(RegistryError::AlreadyExists(def.id));
        }

        let now = chrono::Utc::now().to_rfc3339();
        if def.created_at.is_empty() {
            def.created_at = now.clone();
        }
        def.updated_at = now;

        // Persist to file
        if let Some(ref dir) = self.base_dir {
            tokio::fs::create_dir_all(dir).await.map_err(|e| {
                RegistryError::Io(format!("Failed to create compositions dir: {}", e))
            })?;
            let path = dir.join(format!("{}.yaml", def.id));
            let yaml = serde_yaml::to_string(&def).map_err(|e| {
                RegistryError::Parse(format!("Failed to serialize composition: {}", e))
            })?;
            tokio::fs::write(&path, yaml).await.map_err(|e| {
                RegistryError::Io(format!("Failed to write composition file: {}", e))
            })?;
        }

        self.compositions.insert(def.id.clone(), def);
        Ok(())
    }

    /// Update an existing composition and persist it
    pub async fn update(
        &mut self,
        mut def: CompositionDefinition,
    ) -> Result<(), RegistryError> {
        if !self.compositions.contains_key(&def.id) {
            return Err(RegistryError::NotFound(def.id));
        }

        def.updated_at = chrono::Utc::now().to_rfc3339();

        // Persist to file
        if let Some(ref dir) = self.base_dir {
            let path = dir.join(format!("{}.yaml", def.id));
            let yaml = serde_yaml::to_string(&def).map_err(|e| {
                RegistryError::Parse(format!("Failed to serialize composition: {}", e))
            })?;
            tokio::fs::write(&path, yaml).await.map_err(|e| {
                RegistryError::Io(format!("Failed to write composition file: {}", e))
            })?;
        }

        self.compositions.insert(def.id.clone(), def);
        Ok(())
    }

    /// Delete a composition and its persisted file
    pub async fn delete(&mut self, id: &str) -> Result<(), RegistryError> {
        if self.compositions.remove(id).is_none() {
            return Err(RegistryError::NotFound(id.to_string()));
        }

        if let Some(ref dir) = self.base_dir {
            let path = dir.join(format!("{}.yaml", id));
            if path.exists() {
                tokio::fs::remove_file(&path).await.map_err(|e| {
                    RegistryError::Io(format!("Failed to delete composition file: {}", e))
                })?;
            }
        }

        Ok(())
    }

    /// Load all YAML composition files from a directory
    pub async fn load_from_dir(&mut self, dir: &Path) -> Result<(), RegistryError> {
        let mut entries = match tokio::fs::read_dir(dir).await {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("Composition directory not found: {}", e);
                return Ok(());
            }
        };

        loop {
            match entries.next_entry().await {
                Ok(Some(entry)) => {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("yaml")
                        || path.extension().and_then(|e| e.to_str()) == Some("yml")
                    {
                        match tokio::fs::read_to_string(&path).await {
                            Ok(content) => {
                                match serde_yaml::from_str::<CompositionDefinition>(&content) {
                                    Ok(mut def) => {
                                        if def.id.is_empty() {
                                            def.id = path
                                                .file_stem()
                                                .unwrap_or_default()
                                                .to_string_lossy()
                                                .to_string();
                                        }
                                        tracing::info!(
                                            "Loaded composition: {} ({}) from {:?}",
                                            def.name,
                                            def.id,
                                            path
                                        );
                                        self.compositions.insert(def.id.clone(), def);
                                    }
                                    Err(e) => {
                                        tracing::warn!("Failed to parse composition {:?}: {}", path, e);
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Failed to read composition {:?}: {}", path, e);
                            }
                        }
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    tracing::error!("Failed to read directory entry from {:?}: {}", dir, e);
                    break;
                }
            }
        }

        self.base_dir = Some(dir.to_path_buf());
        Ok(())
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::definition::{CompositionConfig, PipelineStepDef, InputMappingDef};

    #[tokio::test]
    async fn test_registry_crud() {
        let temp_dir = std::env::temp_dir().join(format!(
            "beebotos_composition_test_{}",
            std::process::id()
        ));
        tokio::fs::create_dir_all(&temp_dir).await.unwrap();

        let mut registry = CompositionRegistry::with_dir(&temp_dir);

        let def = CompositionDefinition {
            id: "test_pipe".to_string(),
            name: "Test Pipeline".to_string(),
            description: "A test".to_string(),
            config: CompositionConfig::Pipeline {
                steps: vec![PipelineStepDef {
                    skill_id: "skill_a".to_string(),
                    input_mapping: InputMappingDef::PassThrough,
                    output_schema: None,
                }],
            },
            tags: vec!["test".to_string()],
            created_at: String::new(),
            updated_at: String::new(),
        };

        // Create
        registry.create(def.clone()).await.unwrap();
        assert!(registry.get("test_pipe").is_some());

        // List
        let all = registry.list_all();
        assert_eq!(all.len(), 1);

        // Delete
        registry.delete("test_pipe").await.unwrap();
        assert!(registry.get("test_pipe").is_none());

        // Cleanup
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }

    #[tokio::test]
    async fn test_registry_load_from_dir() {
        let temp_dir = std::env::temp_dir().join(format!(
            "beebotos_composition_load_test_{}",
            std::process::id()
        ));
        tokio::fs::create_dir_all(&temp_dir).await.unwrap();

        let def = CompositionDefinition {
            id: "loaded_pipe".to_string(),
            name: "Loaded Pipeline".to_string(),
            description: "Loaded from dir".to_string(),
            config: CompositionConfig::Pipeline {
                steps: vec![PipelineStepDef {
                    skill_id: "skill_b".to_string(),
                    input_mapping: InputMappingDef::PassThrough,
                    output_schema: None,
                }],
            },
            tags: vec![],
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        };

        let yaml = serde_yaml::to_string(&def).unwrap();
        tokio::fs::write(temp_dir.join("loaded_pipe.yaml"), yaml)
            .await
            .unwrap();

        let mut registry = CompositionRegistry::new();
        registry.load_from_dir(&temp_dir).await.unwrap();

        assert!(registry.get("loaded_pipe").is_some());

        // Cleanup
        let _ = tokio::fs::remove_dir_all(&temp_dir).await;
    }
}
