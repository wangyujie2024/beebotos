pub mod interaction;
pub mod reputation;
pub mod trust;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialAgent {
    pub id: String,
    pub name: String,
    pub public_key: String,
    pub reputation_score: f32,
    pub trust_score: f32,
    pub relationship: Relationship,
    pub last_interaction: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Relationship {
    Stranger,
    Acquaintance,
    Friend,
    CloseFriend,
    Trusted,
    Distrusted,
    Blocked,
}

impl Relationship {
    pub fn trust_baseline(&self) -> f32 {
        match self {
            Relationship::Stranger => 0.5,
            Relationship::Acquaintance => 0.6,
            Relationship::Friend => 0.75,
            Relationship::CloseFriend => 0.9,
            Relationship::Trusted => 0.95,
            Relationship::Distrusted => 0.2,
            Relationship::Blocked => 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialContext {
    pub group_id: Option<String>,
    pub conversation_history: Vec<Message>,
    pub shared_goals: Vec<String>,
    pub norms: Vec<SocialNorm>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub sender_id: String,
    pub content: String,
    pub timestamp: u64,
    pub message_type: MessageType,
    pub sentiment: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageType {
    Text,
    Action,
    Proposal,
    Agreement,
    Disagreement,
    Request,
    Offer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialNorm {
    pub norm_id: String,
    pub description: String,
    pub condition: String,
    pub expected_behavior: String,
    pub violation_consequence: String,
}

pub struct SocialCognition {
    known_agents: HashMap<String, SocialAgent>,
    interaction_history: Vec<InteractionRecord>,
    social_graph: SocialGraph,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionRecord {
    pub interaction_id: String,
    pub participant_ids: Vec<String>,
    pub interaction_type: InteractionType,
    pub outcome: InteractionOutcome,
    pub timestamp: u64,
    pub duration_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InteractionType {
    Cooperation,
    Competition,
    Communication,
    Transaction,
    Conflict,
    Negotiation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InteractionOutcome {
    Success,
    PartialSuccess,
    Failure,
    Aborted,
}

impl InteractionOutcome {
    pub fn score(&self) -> f32 {
        match self {
            InteractionOutcome::Success => 1.0,
            InteractionOutcome::PartialSuccess => 0.5,
            InteractionOutcome::Failure => 0.0,
            InteractionOutcome::Aborted => 0.2,
        }
    }
}

pub struct SocialGraph {
    nodes: HashMap<String, SocialNode>,
    edges: Vec<SocialEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialNode {
    pub agent_id: String,
    pub centrality: f32,
    pub influence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialEdge {
    pub from: String,
    pub to: String,
    pub strength: f32,
    pub edge_type: EdgeType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeType {
    Friendship,
    Professional,
    Family,
    Transactional,
    Informational,
}

impl SocialCognition {
    pub fn new() -> Self {
        Self {
            known_agents: HashMap::new(),
            interaction_history: vec![],
            social_graph: SocialGraph::new(),
        }
    }

    pub fn register_agent(&mut self, agent: SocialAgent) {
        let agent_id = agent.id.clone();
        self.known_agents.insert(agent_id.clone(), agent);
        self.social_graph.add_node(agent_id);
    }

    pub fn get_agent(&self, id: &str) -> Option<&SocialAgent> {
        self.known_agents.get(id)
    }

    pub fn record_interaction(&mut self, record: InteractionRecord) {
        self.interaction_history.push(record.clone());

        for participant in &record.participant_ids {
            if let Some(agent) = self.known_agents.get_mut(participant) {
                agent.last_interaction = Some(record.timestamp);
            }
        }

        if record.participant_ids.len() == 2 {
            self.social_graph.update_edge(
                &record.participant_ids[0],
                &record.participant_ids[1],
                record.outcome.score(),
            );
        }
    }

    pub fn get_interaction_history(&self, agent_id: &str) -> Vec<&InteractionRecord> {
        self.interaction_history
            .iter()
            .filter(|r| r.participant_ids.contains(&agent_id.to_string()))
            .collect()
    }

    pub fn calculate_social_score(&self, agent_id: &str) -> f32 {
        let agent = match self.known_agents.get(agent_id) {
            Some(a) => a,
            None => return 0.0,
        };

        let history = self.get_interaction_history(agent_id);

        if history.is_empty() {
            return agent.relationship.trust_baseline();
        }

        let recent_score: f32 = history
            .iter()
            .rev()
            .take(10)
            .map(|h| h.outcome.score())
            .sum::<f32>()
            / history.len().min(10) as f32;

        let reputation = agent.reputation_score;
        let relationship = agent.relationship.trust_baseline();

        (recent_score * 0.5) + (reputation * 0.3) + (relationship * 0.2)
    }

    pub fn find_trusted_agents(&self, threshold: f32) -> Vec<&SocialAgent> {
        self.known_agents
            .values()
            .filter(|a| {
                let score = self.calculate_social_score(&a.id);
                score >= threshold
            })
            .collect()
    }

    pub fn recommend_collaborators(&self, _task_requirements: &[String]) -> Vec<&SocialAgent> {
        let mut candidates: Vec<_> = self.known_agents.values().collect();

        candidates.sort_by(|a, b| {
            let score_a = self.calculate_social_score(&a.id);
            let score_b = self.calculate_social_score(&b.id);
            crate::utils::compare_f32(&score_b, &score_a)
        });

        candidates.into_iter().take(5).collect()
    }
}

impl SocialGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: vec![],
        }
    }

    pub fn add_node(&mut self, agent_id: String) {
        self.nodes.entry(agent_id.clone()).or_insert(SocialNode {
            agent_id,
            centrality: 0.0,
            influence: 0.0,
        });
    }

    pub fn update_edge(&mut self, from: &str, to: &str, strength: f32) {
        let existing = self
            .edges
            .iter_mut()
            .find(|e| (e.from == from && e.to == to) || (e.from == to && e.to == from));

        if let Some(edge) = existing {
            edge.strength = (edge.strength + strength) / 2.0;
        } else {
            self.edges.push(SocialEdge {
                from: from.to_string(),
                to: to.to_string(),
                strength,
                edge_type: EdgeType::Friendship,
            });
        }
    }

    pub fn calculate_centrality(&mut self) {
        for (id, node) in &mut self.nodes {
            let degree = self
                .edges
                .iter()
                .filter(|e| &e.from == id || &e.to == id)
                .count() as f32;
            node.centrality = degree;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_social_score_calculation() {
        let mut social = SocialCognition::new();

        let agent = SocialAgent {
            id: "agent1".to_string(),
            name: "Test Agent".to_string(),
            public_key: "pk".to_string(),
            reputation_score: 0.8,
            trust_score: 0.7,
            relationship: Relationship::Friend,
            last_interaction: None,
        };

        social.register_agent(agent);

        let record = InteractionRecord {
            interaction_id: "i1".to_string(),
            participant_ids: vec!["agent1".to_string(), "agent2".to_string()],
            interaction_type: InteractionType::Cooperation,
            outcome: InteractionOutcome::Success,
            timestamp: chrono::Utc::now().timestamp() as u64,
            duration_ms: 1000,
        };

        social.record_interaction(record);

        let score = social.calculate_social_score("agent1");
        assert!(score > 0.0);
    }
}
