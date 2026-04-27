use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerceptualInput {
    pub modality: SensoryModality,
    pub raw_data: Vec<u8>,
    pub timestamp: u64,
    pub source: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SensoryModality {
    Visual,
    Auditory,
    Text,
    Numeric,
    Temporal,
    Spatial,
    Social,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerceptualFeature {
    pub feature_type: FeatureType,
    pub value: FeatureValue,
    pub salience: f32,
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeatureType {
    Color,
    Shape,
    Texture,
    Motion,
    Pattern,
    Entity,
    Relation,
    Emotion,
    Intent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FeatureValue {
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Vector(Vec<f64>),
    BoundingBox { x: f64, y: f64, w: f64, h: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Percept {
    pub id: String,
    pub features: Vec<PerceptualFeature>,
    pub raw_input_id: String,
    pub interpretation: String,
    pub confidence: f32,
    pub attention_weight: f32,
}

pub struct PerceptualSystem {
    feature_extractors: HashMap<SensoryModality, Box<dyn FeatureExtractor>>,
    attention_threshold: f32,
}

pub trait FeatureExtractor: Send + Sync {
    fn extract_features(&self, input: &PerceptualInput) -> Vec<PerceptualFeature>;
    fn modality(&self) -> SensoryModality;
}

impl PerceptualSystem {
    pub fn new() -> Self {
        Self {
            feature_extractors: HashMap::new(),
            attention_threshold: 0.5,
        }
    }

    pub fn register_extractor(&mut self, extractor: Box<dyn FeatureExtractor>) {
        self.feature_extractors
            .insert(extractor.modality(), extractor);
    }

    pub fn process_input(&self, input: PerceptualInput) -> Option<Percept> {
        if input.confidence < self.attention_threshold {
            return None;
        }

        let features = if let Some(extractor) = self.feature_extractors.get(&input.modality) {
            extractor.extract_features(&input)
        } else {
            vec![]
        };

        let avg_confidence = if features.is_empty() {
            input.confidence
        } else {
            features.iter().map(|f| f.confidence).sum::<f32>() / features.len() as f32
        };

        Some(Percept {
            id: uuid::Uuid::new_v4().to_string(),
            features,
            raw_input_id: input.source.clone(),
            interpretation: String::new(),
            confidence: avg_confidence,
            attention_weight: input.confidence,
        })
    }

    pub fn set_attention_threshold(&mut self, threshold: f32) {
        self.attention_threshold = threshold.clamp(0.0, 1.0);
    }
}

pub struct TextFeatureExtractor;

impl FeatureExtractor for TextFeatureExtractor {
    fn extract_features(&self, input: &PerceptualInput) -> Vec<PerceptualFeature> {
        let text = String::from_utf8_lossy(&input.raw_data);
        let mut features = vec![];

        features.push(PerceptualFeature {
            feature_type: FeatureType::Pattern,
            value: FeatureValue::Integer(text.len() as i64),
            salience: 0.5,
            confidence: input.confidence,
        });

        let word_count = text.split_whitespace().count() as i64;
        features.push(PerceptualFeature {
            feature_type: FeatureType::Entity,
            value: FeatureValue::Integer(word_count),
            salience: 0.7,
            confidence: input.confidence,
        });

        features
    }

    fn modality(&self) -> SensoryModality {
        SensoryModality::Text
    }
}

#[allow(dead_code)]
pub struct NumericFeatureExtractor;

impl FeatureExtractor for NumericFeatureExtractor {
    fn extract_features(&self, input: &PerceptualInput) -> Vec<PerceptualFeature> {
        let mut features = vec![];

        if let Ok(text) = String::from_utf8(input.raw_data.clone()) {
            if let Ok(value) = text.parse::<f64>() {
                features.push(PerceptualFeature {
                    feature_type: FeatureType::Pattern,
                    value: FeatureValue::Float(value),
                    salience: 1.0,
                    confidence: input.confidence,
                });

                let magnitude = value.abs().log10().abs() as f32;
                features.push(PerceptualFeature {
                    feature_type: FeatureType::Relation,
                    value: FeatureValue::Float(magnitude as f64),
                    salience: 0.6,
                    confidence: input.confidence,
                });
            }
        }

        features
    }

    fn modality(&self) -> SensoryModality {
        SensoryModality::Numeric
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct SceneUnderstanding {
    pub percepts: Vec<Percept>,
    pub spatial_relations: Vec<SpatialRelation>,
    pub temporal_context: TemporalContext,
    pub overall_confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct SpatialRelation {
    pub from_entity: String,
    pub to_entity: String,
    pub relation_type: RelationType,
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum RelationType {
    LeftOf,
    RightOf,
    Above,
    Below,
    Inside,
    Outside,
    Near,
    Far,
    Connected,
    Disconnected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct TemporalContext {
    pub timestamp: u64,
    pub sequence_position: u32,
    pub duration_since_last: Option<u64>,
    pub predicted_next: Option<String>,
}

#[allow(dead_code)]
pub struct SceneAnalyzer;

#[allow(dead_code)]
impl SceneAnalyzer {
    pub fn analyze_scene(percepts: &[Percept]) -> SceneUnderstanding {
        let mut spatial_relations = vec![];

        for (i, p1) in percepts.iter().enumerate() {
            for p2 in percepts.iter().skip(i + 1) {
                if let Some(relation) = Self::infer_relation(p1, p2) {
                    spatial_relations.push(relation);
                }
            }
        }

        let overall_confidence = if percepts.is_empty() {
            0.0
        } else {
            percepts.iter().map(|p| p.confidence).sum::<f32>() / percepts.len() as f32
        };

        SceneUnderstanding {
            percepts: percepts.to_vec(),
            spatial_relations,
            temporal_context: TemporalContext {
                timestamp: chrono::Utc::now().timestamp() as u64,
                sequence_position: 0,
                duration_since_last: None,
                predicted_next: None,
            },
            overall_confidence,
        }
    }

    fn infer_relation(_p1: &Percept, _p2: &Percept) -> Option<SpatialRelation> {
        None
    }
}
