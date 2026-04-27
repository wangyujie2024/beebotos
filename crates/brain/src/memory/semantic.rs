//! Semantic Memory
//!
//! Conceptual knowledge and facts network.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

/// Semantic memory (concept network)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticMemory {
    concepts: HashMap<String, Concept>,
    relations: Vec<Relation>,
}

/// Concept node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Concept {
    pub id: String,
    pub name: String,
    pub definition: String,
    pub category: String,
    pub properties: HashMap<String, PropertyValue>,
    pub related: HashSet<String>, // IDs of related concepts
    pub embedding: Option<Vec<f32>>,
}

/// Property value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PropertyValue {
    String(String),
    Number(f64),
    Boolean(bool),
    List(Vec<String>),
}

/// Relation between concepts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub from: String,
    pub to: String,
    pub relation_type: RelationType,
    pub strength: f32,
}

/// Relation types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelationType {
    IsA,
    HasA,
    PartOf,
    RelatedTo,
    Causes,
    Enables,
    Contradicts,
}

impl SemanticMemory {
    pub fn new() -> Self {
        Self {
            concepts: HashMap::new(),
            relations: vec![],
        }
    }

    /// Learn new concept
    pub fn learn_concept(
        &mut self,
        name: impl Into<String>,
        definition: impl Into<String>,
        category: impl Into<String>,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let name = name.into();

        let concept = Concept {
            id: id.clone(),
            name: name.clone(),
            definition: definition.into(),
            category: category.into(),
            properties: HashMap::new(),
            related: HashSet::new(),
            embedding: None,
        };

        self.concepts.insert(id.clone(), concept);
        id
    }

    /// Add relation
    pub fn add_relation(
        &mut self,
        from: &str,
        to: &str,
        rel_type: RelationType,
        strength: f32,
    ) -> Result<(), MemoryError> {
        if !self.concepts.contains_key(from) || !self.concepts.contains_key(to) {
            return Err(MemoryError::ConceptNotFound);
        }

        let relation = Relation {
            from: from.to_string(),
            to: to.to_string(),
            relation_type: rel_type,
            strength: strength.clamp(0.0, 1.0),
        };

        // Update related set
        if let Some(concept) = self.concepts.get_mut(from) {
            concept.related.insert(to.to_string());
        }

        self.relations.push(relation);
        Ok(())
    }

    /// Get concept by ID
    pub fn get(&self, id: &str) -> Option<&Concept> {
        self.concepts.get(id)
    }

    /// Get by name
    pub fn find_by_name(&self, name: &str) -> Option<&Concept> {
        self.concepts.values().find(|c| c.name == name)
    }

    /// Find similar concepts (by category or properties)
    pub fn find_similar(&self, concept_id: &str, _threshold: f32) -> Vec<&Concept> {
        let concept = match self.concepts.get(concept_id) {
            Some(c) => c,
            None => return vec![],
        };

        self.concepts
            .values()
            .filter(|c| {
                c.id != concept_id
                    && (c.category == concept.category || c.related.contains(concept_id))
            })
            .collect()
    }

    /// Query by category
    pub fn by_category(&self, category: &str) -> Vec<&Concept> {
        self.concepts
            .values()
            .filter(|c| c.category == category)
            .collect()
    }

    /// Infer relation (transitive closure for IsA)
    pub fn infer_relation(&self, from: &str, to: &str) -> Option<RelationType> {
        // Check direct relation
        if let Some(rel) = self.relations.iter().find(|r| r.from == from && r.to == to) {
            return Some(rel.relation_type);
        }

        // Check transitive IsA relations
        let from_concept = self.concepts.get(from)?;
        for related_id in &from_concept.related {
            if let Some(related) = self.concepts.get(related_id) {
                if related.name == to || related_id == to {
                    return Some(RelationType::RelatedTo);
                }
            }
        }

        None
    }

    /// Get concept count
    pub fn len(&self) -> usize {
        self.concepts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.concepts.is_empty()
    }
}

impl Default for SemanticMemory {
    fn default() -> Self {
        Self::new()
    }
}

/// Semantic memory errors
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MemoryError {
    ConceptNotFound,
    DuplicateConcept,
}
