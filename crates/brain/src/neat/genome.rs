//! NEAT Genome
//!
//! Genetic encoding of neural networks.

use serde::{Deserialize, Serialize};

/// Genome - genetic representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Genome {
    pub id: u64,
    pub layers: Vec<LayerGene>,
    pub connections: Vec<ConnectionGene>,
    pub learning_params: LearningParams,
    pub fitness: f32,
    pub adjusted_fitness: f32,
    pub species_id: Option<u64>,
}

/// Layer gene
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerGene {
    pub layer_type: LayerType,
    pub size: usize,
    pub activation: ActivationFn,
    pub plasticity: f32,
}

/// Layer types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayerType {
    Input,
    Hidden,
    Output,
    Lstm,
    Attention,
}

/// Activation functions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivationFn {
    Sigmoid,
    Tanh,
    Relu,
    LeakyRelu,
    Swish,
}

impl ActivationFn {
    pub fn apply(&self, x: f32) -> f32 {
        match self {
            ActivationFn::Sigmoid => 1.0 / (1.0 + (-x).exp()),
            ActivationFn::Tanh => x.tanh(),
            ActivationFn::Relu => x.max(0.0),
            ActivationFn::LeakyRelu => {
                if x > 0.0 {
                    x
                } else {
                    0.01 * x
                }
            }
            ActivationFn::Swish => x * (1.0 / (1.0 + (-x).exp())),
        }
    }
}

/// Connection gene (NEAT key innovation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionGene {
    pub in_node: u64,
    pub out_node: u64,
    pub weight: f32,
    pub enabled: bool,
    pub innovation_number: u64,
    pub is_recurrent: bool,
}

/// Learning parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningParams {
    pub learning_rate: f32,
    pub discount_factor: f32,
    pub exploration_rate: f32,
}

impl Default for LearningParams {
    fn default() -> Self {
        Self {
            learning_rate: 0.01,
            discount_factor: 0.95,
            exploration_rate: 0.1,
        }
    }
}

impl Genome {
    pub fn new(id: u64, input_size: usize, output_size: usize) -> Self {
        Self::new_minimal(id, input_size, output_size)
    }

    pub fn new_minimal(id: u64, input_size: usize, output_size: usize) -> Self {
        let mut layers = vec![];
        let mut connections = vec![];

        // Input layer
        layers.push(LayerGene {
            layer_type: LayerType::Input,
            size: input_size,
            activation: ActivationFn::Relu,
            plasticity: 0.0,
        });

        // Output layer
        layers.push(LayerGene {
            layer_type: LayerType::Output,
            size: output_size,
            activation: ActivationFn::Sigmoid,
            plasticity: 0.1,
        });

        // Fully connected
        for i in 0..input_size as u64 {
            for o in 0..output_size as u64 {
                connections.push(ConnectionGene {
                    in_node: i,
                    out_node: input_size as u64 + o,
                    weight: rand::random::<f32>() * 2.0 - 1.0,
                    enabled: true,
                    innovation_number: i * output_size as u64 + o,
                    is_recurrent: false,
                });
            }
        }

        Self {
            id,
            layers,
            connections,
            learning_params: LearningParams::default(),
            fitness: 0.0,
            adjusted_fitness: 0.0,
            species_id: None,
        }
    }

    pub fn node_count(&self) -> usize {
        self.layers.iter().map(|l| l.size).sum()
    }

    pub fn enabled_connections(&self) -> Vec<&ConnectionGene> {
        self.connections.iter().filter(|c| c.enabled).collect()
    }

    /// Calculate compatibility distance between two genomes
    pub fn compatibility_distance(
        &self,
        other: &Genome,
        config: &crate::neat::config::NeatConfig,
    ) -> f32 {
        let mut disjoint = 0.0;
        let mut excess = 0.0;
        let mut weight_diff = 0.0;
        let mut matching = 0;

        let max_innovation = self
            .connections
            .iter()
            .chain(other.connections.iter())
            .map(|c| c.innovation_number)
            .max()
            .unwrap_or(0);

        for i in 0..=max_innovation {
            let c1 = self.connections.iter().find(|c| c.innovation_number == i);
            let c2 = other.connections.iter().find(|c| c.innovation_number == i);

            match (c1, c2) {
                (Some(c1), Some(c2)) => {
                    weight_diff += (c1.weight - c2.weight).abs();
                    matching += 1;
                }
                (Some(_), None) => {
                    if i < max_innovation / 2 {
                        disjoint += 1.0;
                    } else {
                        excess += 1.0;
                    }
                }
                (None, Some(_)) => {
                    if i < max_innovation / 2 {
                        disjoint += 1.0;
                    } else {
                        excess += 1.0;
                    }
                }
                (None, None) => {}
            }
        }

        let n = self.connections.len().max(other.connections.len()).max(1) as f32;
        let weight_diff = if matching > 0 {
            weight_diff / matching as f32
        } else {
            0.0
        };

        (config.excess_coefficient * excess + config.disjoint_coefficient * disjoint) / n
            + config.weight_coefficient * weight_diff
    }

    /// Mutate weights
    pub fn mutate_weights(&mut self, config: &crate::neat::config::NeatConfig) {
        for conn in &mut self.connections {
            if rand::random::<f32>() < config.weight_mutation_rate {
                if rand::random::<f32>() < 0.1 {
                    // Random new weight
                    conn.weight = rand::random::<f32>() * 2.0 - 1.0;
                } else {
                    // Perturb weight
                    conn.weight += rand::random::<f32>() * 0.4 - 0.2;
                    conn.weight = conn.weight.clamp(-1.0, 1.0);
                }
            }
        }
    }

    /// Crossover two genomes
    pub fn crossover(parent1: &Genome, parent2: &Genome) -> Genome {
        // Assume parent1 is fitter
        let mut child = parent1.clone();

        for child_conn in &mut child.connections {
            if let Some(p2_conn) = parent2
                .connections
                .iter()
                .find(|c| c.innovation_number == child_conn.innovation_number)
            {
                // Matching gene - randomly choose
                if rand::random::<bool>() {
                    child_conn.weight = p2_conn.weight;
                }
            }
        }

        child.fitness = 0.0;
        child.adjusted_fitness = 0.0;
        child.id = rand::random::<u64>();
        child
    }

    /// Mutate genome
    pub fn mutate(
        &mut self,
        config: &crate::neat::config::NeatConfig,
        innovations: &mut crate::neat::InnovationTracker,
    ) {
        self.mutate_weights(config);

        // Structural mutations
        if rand::random::<f32>() < config.add_node_probability {
            self.add_node_mutation(innovations);
        }

        if rand::random::<f32>() < config.add_connection_probability {
            self.add_connection_mutation(innovations);
        }
    }

    /// Add node mutation - split an existing connection
    fn add_node_mutation(&mut self, innovations: &mut crate::neat::InnovationTracker) {
        if self.connections.is_empty() {
            return;
        }

        // Pick random connection to split
        let idx = rand::random::<usize>() % self.connections.len();

        // Check if enabled first
        if !self.connections[idx].enabled {
            return;
        }

        // Extract values before mutable borrow
        let conn = &self.connections[idx];
        let in_node = conn.in_node;
        let out_node = conn.out_node;
        let old_weight = conn.weight;
        let innovation_number = conn.innovation_number;

        // Disable old connection
        self.connections[idx].enabled = false;

        // Create new node
        let new_node_id = innovations.get_node_innovation(innovation_number as usize) as u64;

        // Add new connections: in_node -> new_node -> out_node
        let conn1_innovation =
            innovations.get_connection_innovation(in_node as usize, new_node_id as usize) as u64;
        self.connections.push(ConnectionGene {
            in_node,
            out_node: new_node_id,
            weight: 1.0, // First connection gets weight 1.0
            enabled: true,
            innovation_number: conn1_innovation,
            is_recurrent: false,
        });

        let conn2_innovation =
            innovations.get_connection_innovation(new_node_id as usize, out_node as usize) as u64;
        self.connections.push(ConnectionGene {
            in_node: new_node_id,
            out_node,
            weight: old_weight, // Second connection inherits old weight
            enabled: true,
            innovation_number: conn2_innovation,
            is_recurrent: false,
        });
    }

    /// Add connection mutation - create new connection between unconnected
    /// nodes
    fn add_connection_mutation(&mut self, innovations: &mut crate::neat::InnovationTracker) {
        // Get all node IDs using indices instead of pointer comparison
        let mut node_ids = Vec::new();
        let mut offset = 0usize;
        for layer in &self.layers {
            for i in 0..layer.size {
                node_ids.push((offset + i) as u64);
            }
            offset += layer.size;
        }

        if node_ids.len() < 2 {
            return;
        }

        // Try to find unconnected pair
        for _ in 0..20 {
            // Limit attempts
            let from_idx = rand::random::<usize>() % node_ids.len();
            let to_idx = rand::random::<usize>() % node_ids.len();

            if from_idx == to_idx {
                continue;
            }

            let from_node = node_ids[from_idx];
            let to_node = node_ids[to_idx];

            // Check if connection already exists
            let exists = self
                .connections
                .iter()
                .any(|c| c.in_node == from_node && c.out_node == to_node);

            if !exists {
                let innovation = innovations
                    .get_connection_innovation(from_node as usize, to_node as usize)
                    as u64;
                self.connections.push(ConnectionGene {
                    in_node: from_node,
                    out_node: to_node,
                    weight: rand::random::<f32>() * 2.0 - 1.0,
                    enabled: true,
                    innovation_number: innovation,
                    is_recurrent: false,
                });
                break;
            }
        }
    }
}
