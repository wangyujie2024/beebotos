//! Knowledge Graph
//!
//! Graph-based knowledge representation.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Knowledge graph node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: Uuid,
    pub label: String,
    pub properties: HashMap<String, serde_json::Value>,
}

impl Node {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            label: label.into(),
            properties: HashMap::new(),
        }
    }

    pub fn with_property(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }
}

/// Knowledge graph edge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: Uuid,
    pub from: Uuid,
    pub to: Uuid,
    pub relation: String,
    pub weight: f64,
}

impl Edge {
    pub fn new(from: Uuid, to: Uuid, relation: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            from,
            to,
            relation: relation.into(),
            weight: 1.0,
        }
    }
}

/// Knowledge graph
pub struct KnowledgeGraph {
    nodes: HashMap<Uuid, Node>,
    edges: Vec<Edge>,
    adjacency: HashMap<Uuid, Vec<Uuid>>,
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
            adjacency: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, node: Node) -> Uuid {
        let id = node.id;
        self.nodes.insert(id, node);
        self.adjacency.insert(id, Vec::new());
        id
    }

    pub fn add_edge(&mut self, edge: Edge) {
        let from = edge.from;
        let to = edge.to;

        self.edges.push(edge);

        if let Some(adj) = self.adjacency.get_mut(&from) {
            adj.push(to);
        }
    }

    pub fn get_node(&self, id: Uuid) -> Option<&Node> {
        self.nodes.get(&id)
    }

    pub fn find_by_label(&self, label: &str) -> Vec<&Node> {
        self.nodes.values().filter(|n| n.label == label).collect()
    }

    pub fn neighbors(&self, id: Uuid) -> Vec<&Node> {
        self.adjacency
            .get(&id)
            .map(|adj| adj.iter().filter_map(|nid| self.nodes.get(nid)).collect())
            .unwrap_or_default()
    }

    pub fn shortest_path(&self, start: Uuid, end: Uuid) -> Option<Vec<Uuid>> {
        use std::collections::VecDeque;

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut parent = HashMap::new();

        queue.push_back(start);
        visited.insert(start);

        while let Some(current) = queue.pop_front() {
            if current == end {
                // Reconstruct path
                let mut path = vec![end];
                let mut node = end;
                while let Some(&p) = parent.get(&node) {
                    path.push(p);
                    node = p;
                }
                path.reverse();
                return Some(path);
            }

            if let Some(adj) = self.adjacency.get(&current) {
                for &neighbor in adj {
                    if visited.insert(neighbor) {
                        parent.insert(neighbor, current);
                        queue.push_back(neighbor);
                    }
                }
            }
        }

        None
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_operations() {
        let mut graph = KnowledgeGraph::new();

        let n1 = graph.add_node(Node::new("Person"));
        let n2 = graph.add_node(Node::new("Place"));

        graph.add_edge(Edge::new(n1, n2, "lives_in"));

        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.edge_count(), 1);
    }

    #[test]
    fn test_shortest_path() {
        let mut graph = KnowledgeGraph::new();

        let a = graph.add_node(Node::new("A"));
        let b = graph.add_node(Node::new("B"));
        let c = graph.add_node(Node::new("C"));

        graph.add_edge(Edge::new(a, b, "to"));
        graph.add_edge(Edge::new(b, c, "to"));

        let path = graph.shortest_path(a, c);
        assert!(path.is_some());
        assert_eq!(path.unwrap().len(), 3);
    }
}
