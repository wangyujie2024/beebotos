use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::progress::TaskProgress;

#[derive(Parser)]
pub struct BrainArgs {
    #[command(subcommand)]
    pub command: BrainCommand,
}

#[derive(Subcommand)]
pub enum BrainCommand {
    /// Show brain status
    Status,

    /// Query memory
    Memory {
        #[command(subcommand)]
        action: MemoryAction,
    },

    /// Manage emotions
    Emotion {
        #[command(subcommand)]
        action: EmotionAction,
    },

    /// Evolution operations
    Evolve {
        /// Agent ID to evolve
        #[arg(long)]
        agent: String,

        /// Number of generations
        #[arg(short, long, default_value = "10")]
        generations: u32,
    },
}

#[derive(Subcommand)]
pub enum MemoryAction {
    /// Store a memory
    Store {
        /// Agent ID
        #[arg(long)]
        agent: String,

        /// Memory content
        #[arg(short, long)]
        content: String,

        /// Memory type
        #[arg(short, long, default_value = "episodic")]
        memory_type: String,

        /// Importance (0-1)
        #[arg(short, long, default_value = "0.5")]
        importance: f32,
    },

    /// Retrieve memories
    Retrieve {
        /// Agent ID
        #[arg(long)]
        agent: String,

        /// Query
        query: String,

        /// Number of results
        #[arg(short, long, default_value = "5")]
        limit: usize,
    },

    /// Consolidate memories
    Consolidate {
        /// Agent ID
        #[arg(long)]
        agent: String,
    },
}

#[derive(Subcommand)]
pub enum EmotionAction {
    /// Get current emotional state
    Get {
        /// Agent ID
        #[arg(long)]
        agent: String,
    },

    /// Set emotional state
    Set {
        /// Agent ID
        #[arg(long)]
        agent: String,

        /// Pleasure value (-1 to 1)
        #[arg(long)]
        pleasure: f32,

        /// Arousal value (-1 to 1)
        #[arg(long)]
        arousal: f32,

        /// Dominance value (-1 to 1)
        #[arg(long)]
        dominance: f32,
    },
}

pub async fn execute(args: BrainArgs) -> Result<()> {
    let client = crate::client::ApiClient::new()?;

    match args.command {
        BrainCommand::Status => {
            let progress = TaskProgress::new("Fetching brain status");
            let status = client.get_brain_status().await?;
            progress.finish_success(None);
            println!("Social Brain Status:");
            println!(
                "  Memory Usage: {} / {}",
                status.memory_used, status.memory_total
            );
            println!("  Active Agents: {}", status.active_agents);
            println!("  Evolution Queue: {}", status.evolution_queue);
        }

        BrainCommand::Memory { action } => match action {
            MemoryAction::Store {
                agent,
                content,
                memory_type,
                importance,
            } => {
                let progress = TaskProgress::new("Storing memory");
                client
                    .store_memory(&agent, &content, &memory_type, importance)
                    .await?;
                progress.finish_success(None);
            }
            MemoryAction::Retrieve {
                agent,
                query,
                limit,
            } => {
                let progress = TaskProgress::new("Retrieving memories");
                let memories = client.retrieve_memories(&agent, &query, limit).await?;
                progress.finish_success(Some(&format!("{} memories", memories.len())));
                for (i, memory) in memories.iter().enumerate() {
                    println!(
                        "{}. [{}] {} (relevance: {:.2})",
                        i + 1,
                        memory.memory_type,
                        memory.content,
                        memory.relevance
                    );
                }
            }
            MemoryAction::Consolidate { agent } => {
                let progress = TaskProgress::new(format!("Consolidating memories for {}", agent));
                client.consolidate_memories(&agent).await?;
                progress.finish_success(None);
            }
        },

        BrainCommand::Emotion { action } => match action {
            EmotionAction::Get { agent } => {
                let progress = TaskProgress::new("Fetching emotion state");
                let state = client.get_emotion_state(&agent).await?;
                progress.finish_success(None);
                println!("Emotional State for agent '{}':", agent);
                println!("  Pleasure:   {:.2}", state.pleasure);
                println!("  Arousal:    {:.2}", state.arousal);
                println!("  Dominance:  {:.2}", state.dominance);
            }
            EmotionAction::Set {
                agent,
                pleasure,
                arousal,
                dominance,
            } => {
                let progress = TaskProgress::new("Setting emotion state");
                client
                    .set_emotion_state(&agent, pleasure, arousal, dominance)
                    .await?;
                progress.finish_success(None);
            }
        },

        BrainCommand::Evolve { agent, generations } => {
            let progress =
                TaskProgress::new(format!("Evolving {} ({} generations)", agent, generations));
            let result = client.evolve_agent(&agent, generations).await?;
            progress.finish_success(Some(&format!("fitness: {:.4}", result.fitness)));
            println!("  Generations: {}", result.generations);
        }
    }

    Ok(())
}
