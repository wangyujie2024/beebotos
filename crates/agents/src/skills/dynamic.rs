//! Dynamic Skills

use std::collections::HashMap;

use crate::error::Result;

/// Dynamic skill loaded at runtime
#[derive(Debug)]
pub struct DynamicSkill {
    pub name: String,
    pub wasm_bytes: Vec<u8>,
    pub config: HashMap<String, String>,
}

/// Dynamic skill loader
pub struct DynamicSkillLoader {
    cache: HashMap<String, DynamicSkill>,
}

impl DynamicSkillLoader {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    pub fn load(&mut self, name: &str, wasm_bytes: Vec<u8>) -> Result<DynamicSkill> {
        let skill = DynamicSkill {
            name: name.to_string(),
            wasm_bytes,
            config: HashMap::new(),
        };

        self.cache.insert(name.to_string(), skill.clone());
        Ok(skill)
    }

    pub fn get(&self, name: &str) -> Option<&DynamicSkill> {
        self.cache.get(name)
    }

    pub fn unload(&mut self, name: &str) -> bool {
        self.cache.remove(name).is_some()
    }
}

impl Clone for DynamicSkill {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            wasm_bytes: self.wasm_bytes.clone(),
            config: self.config.clone(),
        }
    }
}

impl Default for DynamicSkillLoader {
    fn default() -> Self {
        Self::new()
    }
}
