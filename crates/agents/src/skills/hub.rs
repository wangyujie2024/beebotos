//! Skills Hub

use std::sync::Arc;

use crate::error::Result;
use crate::skills::loader::LoadedSkill;
use crate::skills::registry::{SkillRegistry, Version};

/// Skill hub manages skill lifecycle, wrapping the central SkillRegistry.
pub struct SkillsHub {
    registry: Arc<SkillRegistry>,
}

#[derive(Debug, Clone)]
pub struct SkillInfo {
    pub name: String,
    pub version: Version,
    pub enabled: bool,
}

impl SkillsHub {
    pub fn new(registry: Arc<SkillRegistry>) -> Self {
        Self { registry }
    }

    pub async fn register(
        &self,
        skill: LoadedSkill,
        category: impl Into<String>,
        tags: Vec<String>,
    ) {
        self.registry.register(skill, category, tags).await;
    }

    pub async fn get(&self, name: &str) -> Option<SkillInfo> {
        self.registry.get(name).await.map(|r| SkillInfo {
            name: r.skill.name,
            version: r.skill.version,
            enabled: r.enabled,
        })
    }

    pub async fn list(&self) -> Vec<SkillInfo> {
        self.registry
            .list_all()
            .await
            .into_iter()
            .map(|r| SkillInfo {
                name: r.skill.name,
                version: r.skill.version,
                enabled: r.enabled,
            })
            .collect()
    }

    pub async fn enable(&self, name: &str) -> Result<()> {
        if self.registry.enable(name).await {
            Ok(())
        } else {
            Err(crate::error::AgentError::not_found(format!(
                "Skill {}",
                name
            )))
        }
    }

    pub async fn disable(&self, name: &str) -> Result<()> {
        if self.registry.disable(name).await {
            Ok(())
        } else {
            Err(crate::error::AgentError::not_found(format!(
                "Skill {}",
                name
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::skills::loader::{LoadedSkill, SkillManifest};

    fn dummy_skill(id: &str) -> LoadedSkill {
        LoadedSkill {
            id: id.to_string(),
            name: id.to_string(),
            version: Version::new(1, 0, 0),
            wasm_path: PathBuf::from("/dev/null/skill.wasm"),
            manifest: SkillManifest {
                id: id.to_string(),
                name: id.to_string(),
                version: Version::new(1, 0, 0),
                description: "test".to_string(),
                author: "test".to_string(),
                capabilities: vec![],
                permissions: vec![],
                entry_point: "skill.wasm".to_string(),
                license: "MIT".to_string(),
                functions: vec![],
            },
        }
    }

    #[tokio::test]
    async fn test_hub_lifecycle() {
        let registry = Arc::new(SkillRegistry::new());
        let hub = SkillsHub::new(registry);

        hub.register(dummy_skill("test_skill"), "test", vec![])
            .await;

        let info = hub.get("test_skill").await.unwrap();
        assert!(info.enabled);

        hub.disable("test_skill").await.unwrap();
        let info = hub.get("test_skill").await.unwrap();
        assert!(!info.enabled);

        hub.enable("test_skill").await.unwrap();
        let info = hub.get("test_skill").await.unwrap();
        assert!(info.enabled);
    }
}
