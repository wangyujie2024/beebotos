//! NeuroEvolution of Augmenting Topologies (NEAT)
//!
//! Implementation of NEAT algorithm for agent neural network evolution.
//! Based on Stanley & Miikkulainen 2002.

pub mod config;
pub mod genome;
pub mod network;
pub mod species;

use std::collections::HashMap;

use beebotos_core::AgentId;
pub use config::NeatConfig;
pub use genome::Genome;
pub use network::NeuralNetwork;
use serde::{Deserialize, Serialize};
pub use species::Species;

/// Innovation tracker for historical markings
#[derive(Debug, Clone)]
pub struct InnovationTracker {
    /// Innovation number counter
    next_innovation: usize,
    /// Connection innovations (from_node, to_node) -> innovation_number
    connection_innovations: HashMap<(usize, usize), usize>,
    /// Node innovations (connection_innovation) -> node_id
    node_innovations: HashMap<usize, usize>,
}

impl InnovationTracker {
    /// Create new innovation tracker
    pub fn new() -> Self {
        Self {
            next_innovation: 0,
            connection_innovations: HashMap::new(),
            node_innovations: HashMap::new(),
        }
    }

    /// Get or create innovation number for connection
    pub fn get_connection_innovation(&mut self, from: usize, to: usize) -> usize {
        let key = (from, to);
        if let Some(&innovation) = self.connection_innovations.get(&key) {
            innovation
        } else {
            let innovation = self.next_innovation;
            self.connection_innovations.insert(key, innovation);
            self.next_innovation += 1;
            innovation
        }
    }

    /// Get or create node ID for split connection
    pub fn get_node_innovation(&mut self, connection_innovation: usize) -> usize {
        if let Some(&node_id) = self.node_innovations.get(&connection_innovation) {
            node_id
        } else {
            let node_id = self.next_innovation + 1000; // Offset to avoid collision
            self.node_innovations.insert(connection_innovation, node_id);
            self.next_innovation += 1;
            node_id
        }
    }
}

impl Default for InnovationTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Fitness evaluation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FitnessResult {
    pub agent_id: AgentId,
    pub fitness: f32,
    pub generation: usize,
    pub metrics: HashMap<String, f32>,
}

/// NEAT population
#[derive(Debug, Clone)]
pub struct Population {
    /// Population size
    pub size: usize,
    /// Current generation
    pub generation: usize,
    /// Genomes
    pub genomes: Vec<Genome>,
    /// Species
    pub species: Vec<Species>,
    /// Innovation tracker
    pub innovations: InnovationTracker,
    /// Best fitness ever
    pub best_fitness: f32,
    /// Best genome
    pub best_genome: Option<Genome>,
}

impl Population {
    /// Create new population
    pub fn new(size: usize, input_size: usize, output_size: usize, config: &NeatConfig) -> Self {
        let innovations = InnovationTracker::new();
        let mut genomes = Vec::with_capacity(size);

        // Create initial population with minimal genomes
        for i in 0..size {
            let mut genome = Genome::new_minimal(i as u64, input_size, output_size);
            genome.mutate_weights(config);
            genomes.push(genome);
        }

        Self {
            size,
            generation: 0,
            genomes,
            species: Vec::new(),
            innovations,
            best_fitness: f32::NEG_INFINITY,
            best_genome: None,
        }
    }

    /// Speciate population
    pub fn speciate(&mut self, config: &NeatConfig) {
        // Clear existing species members
        for species in &mut self.species {
            species.members.clear();
        }

        // Assign genomes to species
        for genome in &self.genomes {
            let mut found = false;
            for species in &mut self.species {
                if species.is_compatible(genome, config) {
                    species.members.push(genome.clone());
                    found = true;
                    break;
                }
            }

            if !found {
                // Create new species
                let mut new_species = Species::new(self.species.len(), genome.clone());
                new_species.members.push(genome.clone());
                self.species.push(new_species);
            }
        }

        // Remove empty species
        self.species.retain(|s| !s.members.is_empty());

        // Update representatives
        for species in &mut self.species {
            species.update_representative();
        }
    }

    /// Evolve one generation
    pub fn evolve(&mut self, fitness_results: &[FitnessResult], config: &NeatConfig) {
        self.generation += 1;

        // Assign fitness to genomes
        for result in fitness_results {
            if let Some(genome) = self
                .genomes
                .iter_mut()
                .find(|g| g.id == result.agent_id.0.as_u128() as u64)
            {
                genome.fitness = result.fitness;
            }
        }

        // Update best fitness
        for genome in &self.genomes {
            if genome.fitness > self.best_fitness {
                self.best_fitness = genome.fitness;
                self.best_genome = Some(genome.clone());
            }
        }

        // Adjust fitness by species
        for species in &self.species {
            for member in &species.members {
                if let Some(genome) = self.genomes.iter_mut().find(|g| g.id == member.id) {
                    genome.adjusted_fitness = genome.fitness / species.members.len() as f32;
                }
            }
        }

        // Calculate offspring per species
        let total_adjusted_fitness: f32 = self.genomes.iter().map(|g| g.adjusted_fitness).sum();

        for species in &mut self.species {
            let species_fitness: f32 = species.members.iter().map(|m| m.adjusted_fitness).sum();

            species.offspring_count = if total_adjusted_fitness > 0.0 {
                ((species_fitness / total_adjusted_fitness) * self.size as f32) as usize
            } else {
                1
            }
            .max(1);
        }

        // Create new generation
        let mut new_genomes = Vec::with_capacity(self.size);

        // Keep best from each species (elitism)
        for species in &self.species {
            if let Some(best) = species
                .members
                .iter()
                .max_by(|a, b| crate::utils::compare_f32(&a.fitness, &b.fitness))
            {
                new_genomes.push(best.clone());
            }
        }

        // Breed offspring
        while new_genomes.len() < self.size {
            let species = self.select_species_proportionally();
            if let Some(offspring) = self.breed_offspring(species, config) {
                new_genomes.push(offspring);
            }
        }

        self.genomes = new_genomes;
        self.speciate(config);
    }

    /// Select species proportionally to offspring count
    fn select_species_proportionally(&self) -> &Species {
        let total: usize = self.species.iter().map(|s| s.offspring_count).sum();
        let mut idx = rand::random::<usize>() % total.max(1);

        for species in &self.species {
            if idx < species.offspring_count {
                return species;
            }
            idx -= species.offspring_count;
        }

        &self.species[0]
    }

    /// Breed offspring from species
    fn breed_offspring(&self, species: &Species, config: &NeatConfig) -> Option<Genome> {
        use rand::seq::SliceRandom;

        if species.members.len() < 2 {
            // Mutate single member
            let mut offspring = species.members[0].clone();
            offspring.mutate(config, &mut InnovationTracker::new());
            return Some(offspring);
        }

        // Select parents
        let parent1 = species.members.choose(&mut rand::thread_rng())?;
        let parent2 = species.members.choose(&mut rand::thread_rng())?;

        // Crossover
        let mut offspring = Genome::crossover(parent1, parent2);
        offspring.mutate(config, &mut InnovationTracker::new());

        Some(offspring)
    }

    /// Get population statistics
    pub fn stats(&self) -> PopulationStats {
        let fitnesses: Vec<f32> = self.genomes.iter().map(|g| g.fitness).collect();
        let avg_fitness = fitnesses.iter().sum::<f32>() / fitnesses.len() as f32;
        let min_fitness = fitnesses.iter().cloned().fold(f32::INFINITY, f32::min);
        let max_fitness = fitnesses.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

        PopulationStats {
            generation: self.generation,
            population_size: self.genomes.len(),
            species_count: self.species.len(),
            avg_fitness,
            min_fitness,
            max_fitness,
            best_fitness: self.best_fitness,
        }
    }
}

/// Population statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PopulationStats {
    pub generation: usize,
    pub population_size: usize,
    pub species_count: usize,
    pub avg_fitness: f32,
    pub min_fitness: f32,
    pub max_fitness: f32,
    pub best_fitness: f32,
}

/// Agent brain with NEAT
pub struct AgentBrain {
    pub genome: Genome,
    pub network: NeuralNetwork,
    pub fitness: f32,
}

impl AgentBrain {
    /// Create from genome
    pub fn from_genome(genome: Genome) -> Self {
        let network = NeuralNetwork::from_genome(&genome);
        Self {
            genome,
            network,
            fitness: 0.0,
        }
    }

    /// Process input through neural network
    pub fn think(&mut self, inputs: &[f32]) -> Vec<f32> {
        self.network.activate(inputs)
    }

    /// Update fitness
    pub fn update_fitness(&mut self, delta: f32) {
        self.fitness += delta;
        self.genome.fitness = self.fitness;
    }
}
