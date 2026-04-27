//! Ontology
//!
//! Concept hierarchies and relationships.

#![allow(dead_code)]

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Concept in ontology
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Concept {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub parent: Option<Uuid>,
    pub properties: HashMap<String, PropertyType>,
}

/// Property types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PropertyType {
    String,
    Number,
    Boolean,
    Reference(Uuid),
    List(Box<PropertyType>),
}

/// Ontology
pub struct Ontology {
    concepts: HashMap<Uuid, Concept>,
    name_index: HashMap<String, Uuid>,
}

impl Ontology {
    pub fn new() -> Self {
        Self {
            concepts: HashMap::new(),
            name_index: HashMap::new(),
        }
    }

    pub fn add_concept(&mut self, concept: Concept) -> Uuid {
        let id = concept.id;
        self.name_index.insert(concept.name.clone(), id);
        self.concepts.insert(id, concept);
        id
    }

    pub fn get_concept(&self, id: Uuid) -> Option<&Concept> {
        self.concepts.get(&id)
    }

    pub fn find_by_name(&self, name: &str) -> Option<&Concept> {
        self.name_index
            .get(name)
            .and_then(|id| self.concepts.get(id))
    }

    pub fn is_a(&self, child: Uuid, ancestor: Uuid) -> bool {
        let mut current = Some(child);

        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            current = self.concepts.get(&id).and_then(|c| c.parent);
        }

        false
    }

    pub fn get_ancestors(&self, id: Uuid) -> Vec<&Concept> {
        let mut ancestors = Vec::new();
        let mut current = self.concepts.get(&id).and_then(|c| c.parent);

        while let Some(pid) = current {
            if let Some(concept) = self.concepts.get(&pid) {
                ancestors.push(concept);
                current = concept.parent;
            } else {
                break;
            }
        }

        ancestors
    }

    pub fn get_children(&self, parent_id: Uuid) -> Vec<&Concept> {
        self.concepts
            .values()
            .filter(|c| c.parent == Some(parent_id))
            .collect()
    }
}

impl Default for Ontology {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ontology_hierarchy() {
        let mut ontology = Ontology::new();

        let entity = ontology.add_concept(Concept {
            id: Uuid::new_v4(),
            name: "Entity".to_string(),
            description: None,
            parent: None,
            properties: HashMap::new(),
        });

        let person = ontology.add_concept(Concept {
            id: Uuid::new_v4(),
            name: "Person".to_string(),
            description: None,
            parent: Some(entity),
            properties: HashMap::new(),
        });

        assert!(ontology.is_a(person, entity));
    }
}
