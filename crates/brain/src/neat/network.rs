//! Neural Network Phenotype
//!
//! Phenotypic expression of NEAT genome.

use std::collections::HashMap;

use super::genome::{ActivationFn, Genome};

/// Neural network
#[derive(Debug, Clone)]
pub struct NeuralNetwork {
    pub layers: Vec<Layer>,
    pub connections: Vec<Connection>,
    pub node_map: HashMap<u64, usize>, // node_id -> layer index
}

/// Network layer
#[derive(Debug, Clone)]
pub struct Layer {
    pub nodes: Vec<Node>,
    pub activation: ActivationFn,
}

/// Network node
#[derive(Debug, Clone)]
pub struct Node {
    pub id: u64,
    pub value: f32,
    pub bias: f32,
}

/// Network connection
#[derive(Debug, Clone)]
pub struct Connection {
    pub from: u64,
    pub to: u64,
    pub weight: f32,
    pub enabled: bool,
}

impl NeuralNetwork {
    /// Build network from genome
    pub fn from_genome(genome: &Genome) -> Self {
        let mut layers = vec![];
        let mut node_map = HashMap::new();
        let mut node_id = 0u64;

        // Build layers
        for layer_gene in &genome.layers {
            let mut nodes = vec![];
            for _ in 0..layer_gene.size {
                nodes.push(Node {
                    id: node_id,
                    value: 0.0,
                    bias: 0.0,
                });
                node_map.insert(node_id, layers.len());
                node_id += 1;
            }

            layers.push(Layer {
                nodes,
                activation: layer_gene.activation,
            });
        }

        // Build connections
        let connections = genome
            .connections
            .iter()
            .map(|cg| Connection {
                from: cg.in_node,
                to: cg.out_node,
                weight: cg.weight,
                enabled: cg.enabled,
            })
            .collect();

        Self {
            layers,
            connections,
            node_map,
        }
    }

    /// Forward pass
    pub fn forward(&mut self, inputs: &[f32]) -> Vec<f32> {
        // Set input layer values
        if let Some(input_layer) = self.layers.first_mut() {
            for (i, val) in inputs.iter().enumerate() {
                if i < input_layer.nodes.len() {
                    input_layer.nodes[i].value = *val;
                }
            }
        }

        // Propagate through layers
        for layer_idx in 1..self.layers.len() {
            let activation = self.layers[layer_idx].activation;

            // Collect incoming connections for this layer
            let incoming: Vec<_> = self
                .connections
                .iter()
                .filter(|c| c.enabled && self.node_map.get(&c.to).copied() == Some(layer_idx))
                .cloned()
                .collect();

            // Update node values
            // Collect node values first to avoid borrow issues
            let node_ids: Vec<u64> = self.layers[layer_idx].nodes.iter().map(|n| n.id).collect();
            let mut new_values = Vec::with_capacity(node_ids.len());

            for node_id in &node_ids {
                let sum: f32 = incoming
                    .iter()
                    .filter(|c| c.to == *node_id)
                    .filter_map(|c| {
                        let from_layer = self.node_map.get(&c.from)?;
                        let from_node = self.layers[*from_layer]
                            .nodes
                            .iter()
                            .find(|n| n.id == c.from)?;
                        Some(from_node.value * c.weight)
                    })
                    .sum();

                let bias = self.layers[layer_idx]
                    .nodes
                    .iter()
                    .find(|n| n.id == *node_id)
                    .map(|n| n.bias)
                    .unwrap_or(0.0);
                new_values.push(activation.apply(sum + bias));
            }

            // Apply new values
            for (i, node) in self.layers[layer_idx].nodes.iter_mut().enumerate() {
                node.value = new_values[i];
            }
        }

        // Return output layer values
        self.layers
            .last()
            .map(|l| l.nodes.iter().map(|n| n.value).collect())
            .unwrap_or_default()
    }

    /// Get output without modifying state
    pub fn predict(&self, inputs: &[f32]) -> Vec<f32> {
        let mut net = self.clone();
        net.forward(inputs)
    }

    /// Activate network (alias for forward)
    pub fn activate(&mut self, inputs: &[f32]) -> Vec<f32> {
        self.forward(inputs)
    }
}
