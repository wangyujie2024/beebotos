pub mod divergence;
pub mod ideation;
pub mod synthesis;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreativeProcess {
    pub process_id: String,
    pub problem_statement: String,
    pub current_stage: CreativeStage,
    pub ideas: Vec<Idea>,
    pub selected_solution: Option<Solution>,
    pub evaluation_criteria: Vec<Criterion>,
    pub start_time: u64,
    pub completion_time: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CreativeStage {
    Preparation,
    Incubation,
    Illumination,
    Verification,
    Implementation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Idea {
    pub id: String,
    pub description: String,
    pub novelty_score: f32,
    pub feasibility_score: f32,
    pub usefulness_score: f32,
    pub synthesis_score: f32,
    pub generated_at: u64,
    pub inspiration_sources: Vec<String>,
    pub tags: Vec<String>,
}

impl Idea {
    pub fn overall_score(&self) -> f32 {
        (self.novelty_score * 0.3)
            + (self.feasibility_score * 0.3)
            + (self.usefulness_score * 0.25)
            + (self.synthesis_score * 0.15)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Solution {
    pub idea_id: String,
    pub refined_description: String,
    pub implementation_plan: Vec<ActionStep>,
    pub resource_requirements: ResourceRequirements,
    pub risks: Vec<Risk>,
    pub expected_outcomes: Vec<ExpectedOutcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionStep {
    pub step_number: u32,
    pub description: String,
    pub estimated_duration_hours: f32,
    pub dependencies: Vec<u32>,
    pub required_resources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequirements {
    pub compute_units: u32,
    pub memory_gb: f32,
    pub storage_gb: f32,
    pub budget: f32,
    pub personnel: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Risk {
    pub description: String,
    pub probability: f32,
    pub impact: RiskImpact,
    pub mitigation_strategy: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskImpact {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedOutcome {
    pub description: String,
    pub metric: String,
    pub target_value: f32,
    pub measurement_method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Criterion {
    pub name: String,
    pub weight: f32,
    pub description: String,
    pub scoring_rubric: String,
}

pub struct CreativeEngine {
    processes: HashMap<String, CreativeProcess>,
    idea_bank: Vec<Idea>,
    creativity_parameters: CreativityParameters,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreativityParameters {
    pub divergence_factor: f32,
    pub convergence_threshold: f32,
    pub novelty_weight: f32,
    pub feasibility_weight: f32,
    pub max_ideas_per_session: u32,
    pub incubation_time_seconds: u32,
}

impl Default for CreativityParameters {
    fn default() -> Self {
        Self {
            divergence_factor: 0.7,
            convergence_threshold: 0.6,
            novelty_weight: 0.3,
            feasibility_weight: 0.3,
            max_ideas_per_session: 20,
            incubation_time_seconds: 300,
        }
    }
}

impl CreativeEngine {
    pub fn new() -> Self {
        Self {
            processes: HashMap::new(),
            idea_bank: vec![],
            creativity_parameters: CreativityParameters::default(),
        }
    }

    pub fn start_creative_process(&mut self, problem_statement: String) -> String {
        let process_id = uuid::Uuid::new_v4().to_string();

        let process = CreativeProcess {
            process_id: process_id.clone(),
            problem_statement,
            current_stage: CreativeStage::Preparation,
            ideas: vec![],
            selected_solution: None,
            evaluation_criteria: vec![],
            start_time: chrono::Utc::now().timestamp() as u64,
            completion_time: None,
        };

        self.processes.insert(process_id.clone(), process);
        process_id
    }

    pub fn generate_ideas(&mut self, process_id: &str, count: u32) -> Vec<Idea> {
        let process = match self.processes.get_mut(process_id) {
            Some(p) => p,
            None => return vec![],
        };

        process.current_stage = CreativeStage::Illumination;

        let mut new_ideas = vec![];
        let count = count.min(self.creativity_parameters.max_ideas_per_session);

        for i in 0..count {
            let idea = Idea {
                id: uuid::Uuid::new_v4().to_string(),
                description: format!("Idea {} for: {}", i + 1, process.problem_statement),
                novelty_score: rand::random::<f32>(),
                feasibility_score: rand::random::<f32>(),
                usefulness_score: rand::random::<f32>(),
                synthesis_score: rand::random::<f32>(),
                generated_at: chrono::Utc::now().timestamp() as u64,
                inspiration_sources: vec![],
                tags: vec![],
            };

            new_ideas.push(idea.clone());
            process.ideas.push(idea.clone());
            self.idea_bank.push(idea);
        }

        new_ideas
    }

    pub fn evaluate_ideas(&self, process_id: &str) -> Vec<(String, f32)> {
        let process = match self.processes.get(process_id) {
            Some(p) => p,
            None => return vec![],
        };

        let mut scores: Vec<(String, f32)> = process
            .ideas
            .iter()
            .map(|idea| (idea.id.clone(), idea.overall_score()))
            .collect();

        scores.sort_by(|a, b| crate::utils::compare_f32(&b.1, &a.1));
        scores
    }

    pub fn select_solution(&mut self, process_id: &str, idea_id: &str) -> Option<Solution> {
        let process = match self.processes.get_mut(process_id) {
            Some(p) => p,
            None => return None,
        };

        let idea = process.ideas.iter().find(|i| i.id == idea_id)?;

        let solution = Solution {
            idea_id: idea_id.to_string(),
            refined_description: idea.description.clone(),
            implementation_plan: vec![],
            resource_requirements: ResourceRequirements {
                compute_units: 100,
                memory_gb: 8.0,
                storage_gb: 100.0,
                budget: 1000.0,
                personnel: vec![],
            },
            risks: vec![],
            expected_outcomes: vec![],
        };

        process.selected_solution = Some(solution.clone());
        process.current_stage = CreativeStage::Implementation;
        process.completion_time = Some(chrono::Utc::now().timestamp() as u64);

        Some(solution)
    }

    pub fn get_process(&self, process_id: &str) -> Option<&CreativeProcess> {
        self.processes.get(process_id)
    }

    pub fn search_idea_bank(&self, query: &str) -> Vec<&Idea> {
        self.idea_bank
            .iter()
            .filter(|idea| {
                idea.description.contains(query) || idea.tags.iter().any(|t| t.contains(query))
            })
            .collect()
    }

    pub fn combine_ideas(&self, idea_ids: &[String]) -> Option<Idea> {
        let selected: Vec<&Idea> = self
            .idea_bank
            .iter()
            .filter(|i| idea_ids.contains(&i.id))
            .collect();

        if selected.is_empty() {
            return None;
        }

        let avg_novelty =
            selected.iter().map(|i| i.novelty_score).sum::<f32>() / selected.len() as f32;
        let avg_feasibility =
            selected.iter().map(|i| i.feasibility_score).sum::<f32>() / selected.len() as f32;
        let avg_usefulness =
            selected.iter().map(|i| i.usefulness_score).sum::<f32>() / selected.len() as f32;

        Some(Idea {
            id: uuid::Uuid::new_v4().to_string(),
            description: format!("Combined idea from {} sources", selected.len()),
            novelty_score: (avg_novelty * 1.1).min(1.0),
            feasibility_score: avg_feasibility,
            usefulness_score: (avg_usefulness * 1.05).min(1.0),
            synthesis_score: 0.9,
            generated_at: chrono::Utc::now().timestamp() as u64,
            inspiration_sources: idea_ids.to_vec(),
            tags: vec!["synthesis".to_string()],
        })
    }
}

pub struct BrainstormingSession {
    participants: Vec<String>,
    #[allow(dead_code)]
    rules: Vec<String>,
    ideas_generated: Vec<Idea>,
    current_round: u32,
    max_rounds: u32,
}

impl BrainstormingSession {
    pub fn new(participants: Vec<String>) -> Self {
        Self {
            participants,
            rules: vec![
                "Defer judgment".to_string(),
                "Encourage wild ideas".to_string(),
                "Build on others' ideas".to_string(),
                "Stay focused on topic".to_string(),
                "One conversation at a time".to_string(),
                "Be visual".to_string(),
                "Go for quantity".to_string(),
            ],
            ideas_generated: vec![],
            current_round: 0,
            max_rounds: 3,
        }
    }

    pub fn next_round(&mut self) -> bool {
        if self.current_round < self.max_rounds {
            self.current_round += 1;
            true
        } else {
            false
        }
    }

    pub fn submit_idea(&mut self, description: String, participant: &str) -> Idea {
        let idea = Idea {
            id: uuid::Uuid::new_v4().to_string(),
            description,
            novelty_score: 0.5 + (rand::random::<f32>() * 0.5),
            feasibility_score: 0.5 + (rand::random::<f32>() * 0.5),
            usefulness_score: 0.5 + (rand::random::<f32>() * 0.5),
            synthesis_score: 0.5,
            generated_at: chrono::Utc::now().timestamp() as u64,
            inspiration_sources: vec![participant.to_string()],
            tags: vec![format!("round_{}", self.current_round)],
        };

        self.ideas_generated.push(idea.clone());
        idea
    }

    pub fn get_statistics(&self) -> BrainstormingStats {
        let total_ideas = self.ideas_generated.len() as u32;
        let ideas_per_participant = if self.participants.is_empty() {
            0.0
        } else {
            total_ideas as f32 / self.participants.len() as f32
        };

        let avg_novelty = if self.ideas_generated.is_empty() {
            0.0
        } else {
            self.ideas_generated
                .iter()
                .map(|i| i.novelty_score)
                .sum::<f32>()
                / total_ideas as f32
        };

        BrainstormingStats {
            total_ideas,
            ideas_per_participant,
            avg_novelty,
            current_round: self.current_round,
            is_complete: self.current_round >= self.max_rounds,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BrainstormingStats {
    pub total_ideas: u32,
    pub ideas_per_participant: f32,
    pub avg_novelty: f32,
    pub current_round: u32,
    pub is_complete: bool,
}
