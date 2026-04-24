use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::capabilities::AgentCapability;
// use std::net::SocketAddr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub capabilities: Vec<AgentCapability>,
    pub endpoints: Vec<Endpoint>,
    pub authentication: AuthenticationMethod,
    pub metadata: HashMap<String, String>,
    /// Public key for E2E encryption
    pub public_key: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    pub protocol: Protocol,
    pub address: String,
    pub port: u16,
}

impl Endpoint {
    /// Get the URL for this endpoint
    pub fn url(&self) -> String {
        let scheme = match self.protocol {
            Protocol::Http => "http",
            Protocol::Https => "https",
            Protocol::WebSocket => "ws",
            Protocol::WebSocketSecure => "wss",
            Protocol::Grpc => "grpc",
        };
        format!("{}://{}:{}", scheme, self.address, self.port)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Protocol {
    Http,
    Https,
    WebSocket,
    WebSocketSecure,
    Grpc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthenticationMethod {
    None,
    ApiKey { header: String },
    OAuth2 { scopes: Vec<String> },
    Jwt { algorithm: String },
}

pub struct DiscoveryService {
    agents: HashMap<String, AgentCard>,
    capabilities_index: HashMap<String, Vec<String>>,
}

impl DiscoveryService {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
            capabilities_index: HashMap::new(),
        }
    }

    pub fn register_agent(&mut self, card: AgentCard) {
        for cap in &card.capabilities {
            self.capabilities_index
                .entry(cap.name.clone())
                .or_default()
                .push(card.id.clone());
        }
        self.agents.insert(card.id.clone(), card);
    }

    pub fn find_agent_by_id(&self, id: &str) -> Option<&AgentCard> {
        self.agents.get(id)
    }

    pub fn find_agents_by_capability(&self, capability_name: &str) -> Vec<&AgentCard> {
        self.capabilities_index
            .get(capability_name)
            .map(|ids| ids.iter().filter_map(|id| self.agents.get(id)).collect())
            .unwrap_or_default()
    }

    pub fn search_agents(&self, query: &str) -> Vec<&AgentCard> {
        self.agents
            .values()
            .filter(|agent| {
                agent.name.contains(query)
                    || agent.description.contains(query)
                    || agent.capabilities.iter().any(|c| c.name.contains(query))
            })
            .collect()
    }

    pub fn unregister_agent(&mut self, id: &str) {
        if let Some(agent) = self.agents.remove(id) {
            for cap in &agent.capabilities {
                if let Some(ids) = self.capabilities_index.get_mut(&cap.name) {
                    ids.retain(|agent_id| agent_id != id);
                }
            }
        }
    }

    /// Get the public key for an agent
    ///
    /// SECURITY FIX: Returns the agent's public key for E2E encryption
    pub fn get_agent_public_key(&self, agent_id: &str) -> Option<Vec<u8>> {
        self.agents
            .get(agent_id)
            .and_then(|agent| agent.public_key.clone())
    }
}
