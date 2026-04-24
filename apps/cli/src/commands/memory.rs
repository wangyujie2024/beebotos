//! Memory management commands
//!
//! Vector memory retrieval and management: STM (Short-Term), LTM (Long-Term),
//! EM (Episodic)

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

use crate::progress::TaskProgress;

#[derive(Parser)]
pub struct MemoryArgs {
    #[command(subcommand)]
    pub command: MemoryCommand,
}

#[derive(Subcommand)]
pub enum MemoryCommand {
    /// Show memory index status
    Status {
        /// Filter by Agent ID
        #[arg(short, long)]
        agent: Option<String>,
        /// Show detailed statistics
        #[arg(short, long)]
        verbose: bool,
    },

    /// Rebuild memory index
    Index {
        /// Force full rebuild
        #[arg(long)]
        force: bool,
        /// Agent ID
        #[arg(short, long)]
        agent: Option<String>,
        /// Incremental indexing
        #[arg(long)]
        incremental: bool,
    },

    /// Search memories semantically
    Search {
        /// Search query
        #[arg(short, long)]
        query: String,
        /// Agent ID filter
        #[arg(short, long)]
        agent: Option<String>,
        /// Limit results
        #[arg(short, long, default_value = "10")]
        limit: usize,
        /// Memory type filter
        #[arg(long, value_enum)]
        r#type: Option<MemoryType>,
        /// Minimum similarity score (0-1)
        #[arg(long, default_value = "0.7")]
        threshold: f32,
        /// Include metadata
        #[arg(long)]
        metadata: bool,
    },

    /// Add a new memory
    Add {
        /// Memory content
        content: String,
        /// Agent ID
        #[arg(short, long)]
        agent: Option<String>,
        /// Memory type
        #[arg(long, value_enum, default_value = "explicit")]
        r#type: MemoryType,
        /// Tags
        #[arg(long)]
        tag: Vec<String>,
        /// Importance level (1-10)
        #[arg(short, long, default_value = "5")]
        importance: u8,
        /// Source/context
        #[arg(long)]
        source: Option<String>,
    },

    /// Get memory by ID
    Get {
        /// Memory ID
        id: String,
        /// Include full content
        #[arg(long)]
        full: bool,
    },

    /// Delete a memory
    Delete {
        /// Memory ID
        id: String,
        /// Permanent delete (skip trash)
        #[arg(long)]
        permanent: bool,
        /// Force without confirmation
        #[arg(long)]
        force: bool,
    },

    /// Update existing memory
    Update {
        /// Memory ID
        id: String,
        /// New content
        #[arg(short, long)]
        content: Option<String>,
        /// New importance
        #[arg(short, long)]
        importance: Option<u8>,
        /// Add tags
        #[arg(long)]
        add_tag: Vec<String>,
        /// Remove tags
        #[arg(long)]
        remove_tag: Vec<String>,
    },

    /// List memories with filters
    List {
        /// Agent ID filter
        #[arg(short, long)]
        agent: Option<String>,
        /// Memory type filter
        #[arg(long, value_enum)]
        r#type: Option<MemoryType>,
        /// Tag filter
        #[arg(long)]
        tag: Option<String>,
        /// Limit results
        #[arg(short, long, default_value = "20")]
        limit: usize,
        /// Show recent only
        #[arg(long)]
        recent: bool,
    },

    /// Export memories to file
    Export {
        /// Agent ID (or "all" for all agents)
        agent: String,
        /// Output file path
        #[arg(short, long)]
        output: PathBuf,
        /// Export format
        #[arg(short, long, value_enum, default_value = "json")]
        format: ExportFormat,
        /// Encrypt export
        #[arg(long)]
        encrypt: bool,
        /// Memory type filter
        #[arg(long, value_enum)]
        r#type: Option<MemoryType>,
        /// Date range start
        #[arg(long)]
        from: Option<String>,
        /// Date range end
        #[arg(long)]
        to: Option<String>,
    },

    /// Import memories from file
    Import {
        /// Input file path
        path: PathBuf,
        /// Target Agent ID
        #[arg(short, long)]
        agent: Option<String>,
        /// Merge strategy
        #[arg(long, value_enum, default_value = "merge")]
        strategy: MergeStrategy,
        /// Dry run (preview only)
        #[arg(long)]
        dry_run: bool,
    },

    /// Memory consolidation (STM -> LTM)
    Consolidate {
        /// Agent ID
        #[arg(short, long)]
        agent: Option<String>,
        /// Dry run (show what would be consolidated)
        #[arg(long)]
        dry_run: bool,
        /// Force consolidation
        #[arg(long)]
        force: bool,
    },

    /// Forget memories (adaptive forgetting)
    Forget {
        /// Agent ID
        #[arg(short, long)]
        agent: Option<String>,
        /// Forgetting strategy
        #[arg(long, value_enum, default_value = "adaptive")]
        strategy: ForgetStrategy,
        /// Max age in days
        #[arg(long)]
        older_than: Option<u32>,
        /// Dry run
        #[arg(long)]
        dry_run: bool,
    },

    /// Memory statistics and analytics
    Stats {
        /// Agent ID
        #[arg(short, long)]
        agent: Option<String>,
        /// Time range
        #[arg(long, value_enum, default_value = "30d")]
        range: TimeRange,
        /// Show trends
        #[arg(long)]
        trends: bool,
    },

    /// Manage memory trash/recycle bin
    #[command(subcommand)]
    Trash(TrashCommand),

    /// Semantic memory graph operations
    Graph {
        #[command(subcommand)]
        command: GraphCommand,
    },
}

#[derive(Subcommand)]
pub enum TrashCommand {
    /// List deleted memories
    List {
        /// Agent ID
        #[arg(short, long)]
        agent: Option<String>,
        /// Limit results
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },
    /// Restore a memory from trash
    Restore {
        /// Memory ID
        id: String,
    },
    /// Permanently delete from trash
    Purge {
        /// Memory ID (or "all" to empty trash)
        id: String,
        /// Force without confirmation
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum GraphCommand {
    /// Show memory graph structure
    Structure {
        /// Agent ID
        #[arg(short, long)]
        agent: Option<String>,
        /// Max depth
        #[arg(short, long, default_value = "3")]
        depth: usize,
    },
    /// Find related memories
    Related {
        /// Memory ID
        id: String,
        /// Number of related memories
        #[arg(short, long, default_value = "5")]
        limit: usize,
    },
    /// Memory clusters/topics
    Clusters {
        /// Agent ID
        #[arg(short, long)]
        agent: Option<String>,
        /// Number of clusters
        #[arg(short, long, default_value = "10")]
        n_clusters: usize,
    },
}

#[derive(ValueEnum, Clone, Debug)]
pub enum MemoryType {
    /// Short-Term Memory (working memory)
    Stm,
    /// Long-Term Memory (consolidated)
    Ltm,
    /// Episodic Memory (events/experiences)
    Em,
    /// Explicit (declarative) memory
    Explicit,
    /// Implicit (procedural) memory
    Implicit,
    /// Semantic memory (facts/concepts)
    Semantic,
    /// Procedural memory (skills)
    Procedural,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum ExportFormat {
    Json,
    Yaml,
    Markdown,
    Csv,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum MergeStrategy {
    /// Merge with existing memories
    Merge,
    /// Replace existing memories
    Replace,
    /// Skip duplicates
    Skip,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum ForgetStrategy {
    /// Adaptive forgetting based on importance and access
    Adaptive,
    /// FIFO - remove oldest first
    Fifo,
    /// LFU - remove least frequently accessed
    Lfu,
    /// Remove by age threshold
    Age,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum TimeRange {
    #[value(name = "24h")]
    _24h,
    #[value(name = "7d")]
    _7d,
    #[value(name = "30d")]
    _30d,
    #[value(name = "90d")]
    _90d,
    #[value(name = "1y")]
    _1y,
    All,
}

pub async fn execute(args: MemoryArgs) -> Result<()> {
    let client = crate::client::ApiClient::new()?;

    match args.command {
        MemoryCommand::Status { agent, verbose } => {
            let progress = TaskProgress::new("Fetching memory status");
            let status = if let Some(agent_id) = agent {
                client.get_memory_status_for_agent(&agent_id).await?
            } else {
                client.get_global_memory_status().await?
            };
            progress.finish_success(None);

            println!("🧠 Memory System Status");
            println!("=======================");
            println!("\n📊 Index Statistics:");
            println!("  Total memories: {}", status.total_memories);
            println!("  STM entries: {}", status.stm_count);
            println!("  LTM entries: {}", status.ltm_count);
            println!("  EM entries: {}", status.em_count);
            println!("  Index size: {:.2} MB", status.index_size_mb);

            if verbose {
                println!("\n📈 Storage Details:");
                println!("  Vector embeddings: {}", status.embedding_count);
                println!(
                    "  Average embedding size: {} bytes",
                    status.avg_embedding_size
                );
                println!("  Last consolidation: {}", status.last_consolidation);
                println!("  Index health: {}", status.health);

                if let Some(per_agent) = status.per_agent {
                    println!("\n👤 Per-Agent Breakdown:");
                    for (agent_id, stats) in per_agent {
                        println!(
                            "  {}: {} memories (STM: {}, LTM: {})",
                            agent_id, stats.total, stats.stm, stats.ltm
                        );
                    }
                }
            }
        }

        MemoryCommand::Index {
            force,
            agent,
            incremental,
        } => {
            let progress = TaskProgress::new("Rebuilding memory index");

            let options = IndexOptions {
                force,
                incremental,
                agent_id: agent,
            };

            let result = client.rebuild_memory_index(&options).await?;
            progress.finish_success(Some(&format!("{} memories indexed", result.indexed_count)));

            println!("\n✅ Index rebuild complete!");
            println!("  Indexed: {} memories", result.indexed_count);
            println!("  Removed: {} stale entries", result.removed_count);
            println!("  Errors: {}", result.error_count);
            println!("  Duration: {:.2}s", result.duration_secs);
        }

        MemoryCommand::Search {
            query,
            agent,
            limit,
            r#type,
            threshold,
            metadata,
        } => {
            let progress = TaskProgress::new("Searching memories");

            let search_req = SearchRequest {
                query: query.clone(),
                agent_id: agent,
                memory_type: r#type.map(|t| format!("{:?}", t)),
                limit,
                threshold,
                include_metadata: metadata,
            };

            let results = client.search_memories(&search_req).await?;
            progress.finish_success(Some(&format!("{} results", results.len())));

            if results.is_empty() {
                println!("No memories found matching query: '{}'", query);
                return Ok(());
            }

            println!("\n🔍 Search Results for: '{}'", query);
            println!("{}", "=".repeat(60));

            for (i, result) in results.iter().enumerate() {
                let icon = match result.memory_type.as_str() {
                    "stm" => "⚡",
                    "ltm" => "💾",
                    "em" => "📸",
                    _ => "📝",
                };

                println!(
                    "\n{}. {} {} (similarity: {:.2})",
                    i + 1,
                    icon,
                    result.id,
                    result.similarity
                );
                println!("   {}", truncate(&result.content, 120));

                if metadata {
                    println!(
                        "   Type: {} | Agent: {} | Created: {}",
                        result.memory_type, result.agent_id, result.created_at
                    );
                    if !result.tags.is_empty() {
                        println!("   Tags: {}", result.tags.join(", "));
                    }
                }
            }
        }

        MemoryCommand::Add {
            content,
            agent,
            r#type,
            tag,
            importance,
            source,
        } => {
            let progress = TaskProgress::new("Adding memory");

            let memory_req = CreateMemoryRequest {
                content,
                agent_id: agent,
                memory_type: format!("{:?}", r#type).to_lowercase(),
                tags: tag,
                importance: importance.min(10),
                source,
            };

            let memory = client.create_memory(&memory_req).await?;
            progress.finish_success(Some(&memory.id));

            println!("✅ Memory added successfully!");
            println!("  ID: {}", memory.id);
            println!("  Type: {}", memory.memory_type);
        }

        MemoryCommand::Get { id, full } => {
            let progress = TaskProgress::new("Fetching memory");
            let memory = client.get_memory(&id).await?;
            progress.finish_success(None);

            let icon = match memory.memory_type.as_str() {
                "stm" => "⚡",
                "ltm" => "💾",
                "em" => "📸",
                _ => "📝",
            };

            println!("{} Memory: {}", icon, memory.id);
            println!("{}", "=".repeat(50));
            println!("Type: {}", memory.memory_type);
            println!("Agent: {}", memory.agent_id);
            println!("Importance: {}/10", memory.importance);
            println!("Created: {}", memory.created_at);
            println!("Last accessed: {}", memory.last_accessed);
            println!("Access count: {}", memory.access_count);

            if !memory.tags.is_empty() {
                println!("Tags: {}", memory.tags.join(", "));
            }

            if let Some(source) = memory.source {
                println!("Source: {}", source);
            }

            println!("\nContent:");
            if full || memory.content.len() < 500 {
                println!("{}", memory.content);
            } else {
                println!("{}...", &memory.content[..500]);
                println!("\n(use --full to see complete content)");
            }

            if let Some(embedding_preview) = memory.embedding_preview {
                println!("\nEmbedding preview: [{}]", embedding_preview);
            }
        }

        MemoryCommand::Delete {
            id,
            permanent,
            force,
        } => {
            if !force && !permanent {
                print!("Move memory '{}' to trash? [y/N] ", id);
                std::io::Write::flush(&mut std::io::stdout())?;
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Cancelled.");
                    return Ok(());
                }
            }

            if !force && permanent {
                print!(
                    "⚠️  Permanently delete memory '{}'? This cannot be undone! [y/N] ",
                    id
                );
                std::io::Write::flush(&mut std::io::stdout())?;
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Cancelled.");
                    return Ok(());
                }
            }

            let progress = TaskProgress::new("Deleting memory");
            if permanent {
                client.permanently_delete_memory(&id).await?;
            } else {
                client.move_memory_to_trash(&id).await?;
            }
            progress.finish_success(None);
        }

        MemoryCommand::Update {
            id,
            content,
            importance,
            add_tag,
            remove_tag,
        } => {
            let progress = TaskProgress::new("Updating memory");

            let update = UpdateMemoryRequest {
                content,
                importance,
                add_tags: add_tag,
                remove_tags: remove_tag,
            };

            let memory = client.update_memory(&id, &update).await?;
            progress.finish_success(None);

            println!("✅ Memory updated!");
            println!("  ID: {}", memory.id);
            println!("  New version: {}", memory.version);
        }

        MemoryCommand::List {
            agent,
            r#type,
            tag,
            limit,
            recent,
        } => {
            let progress = TaskProgress::new("Listing memories");

            let filter = MemoryFilter {
                agent_id: agent,
                memory_type: r#type.map(|t| format!("{:?}", t).to_lowercase()),
                tag,
                limit,
                recent_only: recent,
            };

            let memories = client.list_memories(&filter).await?;
            progress.finish_success(Some(&format!("{} memories", memories.len())));

            println!(
                "{:<36} {:<10} {:<8} {:<20} Preview",
                "ID", "Type", "Import.", "Created"
            );
            println!("{}", "-".repeat(120));

            for m in memories {
                let preview = truncate(&m.content, 40);
                println!(
                    "{:<36} {:<10} {:<8} {:<20} {}",
                    m.id, m.memory_type, m.importance, m.created_at, preview
                );
            }
        }

        MemoryCommand::Export {
            agent,
            output,
            format,
            encrypt,
            r#type,
            from,
            to,
        } => {
            let progress = TaskProgress::new("Exporting memories");

            let export_req = ExportRequest {
                agent_id: agent,
                format: format!("{:?}", format).to_lowercase(),
                memory_type: r#type.map(|t| format!("{:?}", t).to_lowercase()),
                date_from: from,
                date_to: to,
                encrypt,
            };

            let result = client.export_memories(&export_req).await?;

            // Write to file
            std::fs::write(&output, &result.data)?;
            progress.finish_success(Some(&format!("{} memories", result.count)));

            println!(
                "✅ Exported {} memories to {}",
                result.count,
                output.display()
            );
            if encrypt {
                println!("  Encryption: Enabled");
            }
            println!("  Format: {:?}", format);
            println!("  Size: {:.2} KB", result.data.len() as f64 / 1024.0);
        }

        MemoryCommand::Import {
            path,
            agent,
            strategy,
            dry_run,
        } => {
            let progress = TaskProgress::new("Importing memories");

            let data = std::fs::read_to_string(&path)?;
            let import_req = ImportRequest {
                data,
                agent_id: agent,
                strategy: format!("{:?}", strategy).to_lowercase(),
                dry_run,
            };

            let result = client.import_memories(&import_req).await?;
            progress.finish_success(None);

            if dry_run {
                println!("📋 Dry Run Results:");
                println!("  Would import: {} memories", result.would_import);
                println!("  Would skip: {} duplicates", result.would_skip);
                println!("  Would merge: {} conflicts", result.would_merge);
            } else {
                println!("✅ Import complete!");
                println!("  Imported: {} memories", result.imported);
                println!("  Skipped: {} duplicates", result.skipped);
                println!("  Merged: {} conflicts", result.merged);
                println!("  Errors: {}", result.errors);
            }
        }

        MemoryCommand::Consolidate {
            agent,
            dry_run: _,
            force: _,
        } => {
            let progress = TaskProgress::new("Consolidating memories");

            // Use the first agent if specified, otherwise use "default"
            let agent_id = agent.as_deref().unwrap_or("default");

            client.consolidate_memories(agent_id).await?;
            progress.finish_success(None);

            println!("✅ Memory consolidation completed successfully.");
        }

        MemoryCommand::Forget {
            agent,
            strategy,
            older_than,
            dry_run,
        } => {
            let progress = TaskProgress::new("Applying forgetting strategy");

            let options = ForgetOptions {
                agent_id: agent,
                strategy: format!("{:?}", strategy).to_lowercase(),
                older_than_days: older_than,
                dry_run,
            };

            let result = client.apply_forgetting(&options).await?;
            progress.finish_success(None);

            if dry_run {
                println!("📋 Forgetting Preview:");
                println!("  Would forget: {} memories", result.would_forget);
                println!(
                    "  STM: {}, LTM: {}, EM: {}",
                    result.would_forget_stm, result.would_forget_ltm, result.would_forget_em
                );
            } else {
                println!("✅ Forgetting complete!");
                println!("  Forgotten: {} memories", result.forgotten);
                println!(
                    "  STM: {}, LTM: {}, EM: {}",
                    result.forgotten_stm, result.forgotten_ltm, result.forgotten_em
                );
                println!("  Storage freed: {:.2} MB", result.storage_freed_mb);
            }
        }

        MemoryCommand::Stats {
            agent,
            range,
            trends,
        } => {
            let progress = TaskProgress::new("Fetching memory statistics");

            let stats = if let Some(agent_id) = agent {
                client
                    .get_agent_memory_stats(&agent_id, &format!("{:?}", range))
                    .await?
            } else {
                client
                    .get_global_memory_stats(&format!("{:?}", range))
                    .await?
            };

            progress.finish_success(None);

            println!("📊 Memory Statistics");
            println!("===================");
            println!("Period: {:?}", range);
            println!("\nOverview:");
            println!("  Total memories: {}", stats.total_memories);
            println!("  New this period: {}", stats.new_memories);
            println!("  Accessed: {} times", stats.access_count);
            println!("  Consolidated: {}", stats.consolidated_count);
            println!("  Forgotten: {}", stats.forgotten_count);

            println!("\nDistribution:");
            println!("  STM: {:.1}%", stats.stm_percentage);
            println!("  LTM: {:.1}%", stats.ltm_percentage);
            println!("  EM: {:.1}%", stats.em_percentage);

            if trends {
                println!("\n📈 Trends:");
                for trend in stats.trends {
                    let icon = if trend.change > 0.0 { "📈" } else { "📉" };
                    println!("  {} {}: {:.1}% change", icon, trend.metric, trend.change);
                }
            }
        }

        MemoryCommand::Trash(cmd) => match cmd {
            TrashCommand::List { agent, limit } => {
                let progress = TaskProgress::new("Listing trashed memories");
                let memories = client
                    .list_trashed_memories(agent.as_deref(), limit)
                    .await?;
                progress.finish_success(Some(&format!("{} items", memories.len())));

                println!("🗑️  Trashed Memories");
                println!("{:<36} {:<20} Preview", "ID", "Deleted At");
                println!("{}", "-".repeat(100));
                for m in memories {
                    println!(
                        "{:<36} {:<20} {}",
                        m.id,
                        m.deleted_at,
                        truncate(&m.content, 40)
                    );
                }
            }
            TrashCommand::Restore { id } => {
                let progress = TaskProgress::new("Restoring memory");
                client.restore_memory(&id).await?;
                progress.finish_success(None);
                println!("✅ Memory '{}' restored from trash", id);
            }
            TrashCommand::Purge { id, force } => {
                if !force && id != "all" {
                    print!("⚠️  Permanently delete memory '{}'? [y/N] ", id);
                    std::io::Write::flush(&mut std::io::stdout())?;
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input)?;
                    if !input.trim().eq_ignore_ascii_case("y") {
                        println!("Cancelled.");
                        return Ok(());
                    }
                }

                let progress = TaskProgress::new("Purging memory");
                if id == "all" {
                    client.empty_trash().await?;
                    println!("✅ Trash emptied");
                } else {
                    client.purge_memory(&id).await?;
                    println!("✅ Memory '{}' permanently deleted", id);
                }
                progress.finish_success(None);
            }
        },

        MemoryCommand::Graph { command } => match command {
            GraphCommand::Structure { agent, depth } => {
                let progress = TaskProgress::new("Analyzing memory graph");
                let structure = client
                    .get_memory_graph_structure(agent.as_deref(), depth)
                    .await?;
                progress.finish_success(None);

                println!("🕸️  Memory Graph Structure");
                println!("Max depth: {}", depth);
                println!(
                    "\nNodes: {} | Edges: {}",
                    structure.node_count, structure.edge_count
                );
                println!("\nClusters:");
                for cluster in structure.clusters {
                    println!(
                        "  {}: {} nodes, density {:.2}",
                        cluster.name, cluster.node_count, cluster.density
                    );
                }
            }
            GraphCommand::Related { id, limit } => {
                let progress = TaskProgress::new("Finding related memories");
                let related = client.get_related_memories(&id, limit).await?;
                progress.finish_success(Some(&format!("{} related", related.len())));

                println!("🔗 Memories related to '{}'", id);
                for (i, mem) in related.iter().enumerate() {
                    println!(
                        "{}. {} (strength: {:.2})",
                        i + 1,
                        mem.id,
                        mem.relation_strength
                    );
                    println!("   {}", truncate(&mem.content, 80));
                }
            }
            GraphCommand::Clusters { agent, n_clusters } => {
                let progress = TaskProgress::new("Clustering memories");
                let clusters = client
                    .get_memory_clusters(agent.as_deref(), n_clusters)
                    .await?;
                progress.finish_success(Some(&format!("{} clusters", clusters.len())));

                println!("🎯 Memory Clusters");
                for (i, cluster) in clusters.iter().enumerate() {
                    println!(
                        "\nCluster {}: {} ({} memories, density: {:.2})",
                        i + 1,
                        cluster.name,
                        cluster.node_count,
                        cluster.density
                    );
                    println!("  Keywords: {}", cluster.keywords.join(", "));
                    println!("  Sample: {}", truncate(&cluster.sample_memory, 60));
                }
            }
        },
    }

    Ok(())
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

// Request/Response types
#[derive(serde::Serialize)]
struct SearchRequest {
    query: String,
    agent_id: Option<String>,
    memory_type: Option<String>,
    limit: usize,
    threshold: f32,
    include_metadata: bool,
}

#[derive(serde::Deserialize)]
struct SearchResult {
    id: String,
    content: String,
    similarity: f32,
    memory_type: String,
    agent_id: String,
    created_at: String,
    tags: Vec<String>,
}

#[derive(serde::Serialize)]
struct CreateMemoryRequest {
    content: String,
    agent_id: Option<String>,
    memory_type: String,
    tags: Vec<String>,
    importance: u8,
    source: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Memory {
    id: String,
    content: String,
    memory_type: String,
    agent_id: String,
    importance: u8,
    created_at: String,
    last_accessed: String,
    access_count: u64,
    tags: Vec<String>,
    source: Option<String>,
    version: u64,
    embedding_preview: Option<String>,
}

#[derive(serde::Serialize)]
struct UpdateMemoryRequest {
    content: Option<String>,
    importance: Option<u8>,
    add_tags: Vec<String>,
    remove_tags: Vec<String>,
}

#[derive(serde::Serialize)]
struct MemoryFilter {
    agent_id: Option<String>,
    memory_type: Option<String>,
    tag: Option<String>,
    limit: usize,
    recent_only: bool,
}

#[derive(serde::Deserialize)]
struct MemorySummary {
    id: String,
    content: String,
    memory_type: String,
    importance: u8,
    created_at: String,
}

#[derive(serde::Deserialize)]
struct MemoryStatus {
    total_memories: u64,
    stm_count: u64,
    ltm_count: u64,
    em_count: u64,
    index_size_mb: f64,
    embedding_count: u64,
    avg_embedding_size: u64,
    last_consolidation: String,
    health: String,
    per_agent: Option<Vec<(String, AgentMemoryStats)>>,
}

#[derive(serde::Deserialize)]
struct AgentMemoryStats {
    total: u64,
    stm: u64,
    ltm: u64,
}

#[derive(serde::Serialize)]
struct IndexOptions {
    force: bool,
    incremental: bool,
    agent_id: Option<String>,
}

#[derive(serde::Deserialize)]
struct IndexResult {
    indexed_count: u64,
    removed_count: u64,
    error_count: u64,
    duration_secs: f64,
}

#[derive(serde::Serialize)]
struct ExportRequest {
    agent_id: String,
    format: String,
    memory_type: Option<String>,
    date_from: Option<String>,
    date_to: Option<String>,
    encrypt: bool,
}

#[derive(serde::Deserialize)]
struct ExportResult {
    data: Vec<u8>,
    count: u64,
}

#[derive(serde::Serialize)]
struct ImportRequest {
    data: String,
    agent_id: Option<String>,
    strategy: String,
    dry_run: bool,
}

#[derive(serde::Deserialize)]
struct ImportResult {
    imported: u64,
    skipped: u64,
    merged: u64,
    errors: u64,
    would_import: u64,
    would_skip: u64,
    would_merge: u64,
}

#[derive(serde::Serialize)]
struct ConsolidateOptions {
    agent_id: Option<String>,
    dry_run: bool,
    force: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ConsolidateResult {
    #[serde(default)]
    candidate_count: u64,
    #[serde(default)]
    would_consolidate: u64,
    #[serde(default)]
    would_discard: u64,
    #[serde(default)]
    consolidated: u64,
    #[serde(default)]
    discarded: u64,
    #[serde(default)]
    summaries_generated: u64,
}

#[derive(serde::Serialize)]
struct ForgetOptions {
    agent_id: Option<String>,
    strategy: String,
    older_than_days: Option<u32>,
    dry_run: bool,
}

#[derive(serde::Deserialize)]
struct ForgetResult {
    would_forget: u64,
    would_forget_stm: u64,
    would_forget_ltm: u64,
    would_forget_em: u64,
    forgotten: u64,
    forgotten_stm: u64,
    forgotten_ltm: u64,
    forgotten_em: u64,
    storage_freed_mb: f64,
}

#[derive(serde::Deserialize)]
struct MemoryStats {
    total_memories: u64,
    new_memories: u64,
    access_count: u64,
    consolidated_count: u64,
    forgotten_count: u64,
    stm_percentage: f64,
    ltm_percentage: f64,
    em_percentage: f64,
    trends: Vec<Trend>,
}

#[derive(serde::Deserialize)]
struct Trend {
    metric: String,
    change: f64,
}

#[derive(serde::Deserialize)]
struct TrashedMemory {
    id: String,
    content: String,
    deleted_at: String,
}

#[derive(serde::Deserialize)]
struct GraphStructure {
    node_count: u64,
    edge_count: u64,
    clusters: Vec<Cluster>,
}

#[derive(serde::Deserialize)]
struct Cluster {
    name: String,
    node_count: u64,
    density: f64,
    keywords: Vec<String>,
    sample_memory: String,
}

#[derive(serde::Deserialize)]
struct RelatedMemory {
    id: String,
    content: String,
    relation_strength: f32,
}

// Client extension trait
trait MemoryClient {
    async fn get_memory_status_for_agent(&self, agent_id: &str) -> Result<MemoryStatus>;
    async fn get_global_memory_status(&self) -> Result<MemoryStatus>;
    async fn rebuild_memory_index(&self, options: &IndexOptions) -> Result<IndexResult>;
    async fn search_memories(&self, request: &SearchRequest) -> Result<Vec<SearchResult>>;
    async fn create_memory(&self, request: &CreateMemoryRequest) -> Result<Memory>;
    async fn get_memory(&self, id: &str) -> Result<Memory>;
    async fn update_memory(&self, id: &str, request: &UpdateMemoryRequest) -> Result<Memory>;
    async fn list_memories(&self, filter: &MemoryFilter) -> Result<Vec<MemorySummary>>;
    async fn move_memory_to_trash(&self, id: &str) -> Result<()>;
    async fn permanently_delete_memory(&self, id: &str) -> Result<()>;
    async fn export_memories(&self, request: &ExportRequest) -> Result<ExportResult>;
    async fn import_memories(&self, request: &ImportRequest) -> Result<ImportResult>;
    async fn consolidate_memories(&self, options: &ConsolidateOptions)
        -> Result<ConsolidateResult>;
    async fn apply_forgetting(&self, options: &ForgetOptions) -> Result<ForgetResult>;
    async fn get_agent_memory_stats(&self, agent_id: &str, range: &str) -> Result<MemoryStats>;
    async fn get_global_memory_stats(&self, range: &str) -> Result<MemoryStats>;
    async fn list_trashed_memories(
        &self,
        agent_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<TrashedMemory>>;
    async fn restore_memory(&self, id: &str) -> Result<()>;
    async fn empty_trash(&self) -> Result<()>;
    async fn purge_memory(&self, id: &str) -> Result<()>;
    async fn get_memory_graph_structure(
        &self,
        agent_id: Option<&str>,
        depth: usize,
    ) -> Result<GraphStructure>;
    async fn get_related_memories(&self, id: &str, limit: usize) -> Result<Vec<RelatedMemory>>;
    async fn get_memory_clusters(
        &self,
        agent_id: Option<&str>,
        n_clusters: usize,
    ) -> Result<Vec<Cluster>>;
}

// Stub implementations
impl MemoryClient for crate::client::ApiClient {
    async fn get_memory_status_for_agent(&self, _agent_id: &str) -> Result<MemoryStatus> {
        anyhow::bail!("Not implemented")
    }
    async fn get_global_memory_status(&self) -> Result<MemoryStatus> {
        anyhow::bail!("Not implemented")
    }
    async fn rebuild_memory_index(&self, _options: &IndexOptions) -> Result<IndexResult> {
        anyhow::bail!("Not implemented")
    }
    async fn search_memories(&self, _request: &SearchRequest) -> Result<Vec<SearchResult>> {
        Ok(vec![])
    }
    async fn create_memory(&self, _request: &CreateMemoryRequest) -> Result<Memory> {
        anyhow::bail!("Not implemented")
    }
    async fn get_memory(&self, _id: &str) -> Result<Memory> {
        anyhow::bail!("Not implemented")
    }
    async fn update_memory(&self, _id: &str, _request: &UpdateMemoryRequest) -> Result<Memory> {
        anyhow::bail!("Not implemented")
    }
    async fn list_memories(&self, _filter: &MemoryFilter) -> Result<Vec<MemorySummary>> {
        Ok(vec![])
    }
    async fn move_memory_to_trash(&self, _id: &str) -> Result<()> {
        Ok(())
    }
    async fn permanently_delete_memory(&self, _id: &str) -> Result<()> {
        Ok(())
    }
    async fn export_memories(&self, _request: &ExportRequest) -> Result<ExportResult> {
        anyhow::bail!("Not implemented")
    }
    async fn import_memories(&self, _request: &ImportRequest) -> Result<ImportResult> {
        anyhow::bail!("Not implemented")
    }
    async fn consolidate_memories(
        &self,
        _options: &ConsolidateOptions,
    ) -> Result<ConsolidateResult> {
        anyhow::bail!("Not implemented")
    }
    async fn apply_forgetting(&self, _options: &ForgetOptions) -> Result<ForgetResult> {
        anyhow::bail!("Not implemented")
    }
    async fn get_agent_memory_stats(&self, _agent_id: &str, _range: &str) -> Result<MemoryStats> {
        anyhow::bail!("Not implemented")
    }
    async fn get_global_memory_stats(&self, _range: &str) -> Result<MemoryStats> {
        anyhow::bail!("Not implemented")
    }
    async fn list_trashed_memories(
        &self,
        _agent_id: Option<&str>,
        _limit: usize,
    ) -> Result<Vec<TrashedMemory>> {
        Ok(vec![])
    }
    async fn restore_memory(&self, _id: &str) -> Result<()> {
        Ok(())
    }
    async fn empty_trash(&self) -> Result<()> {
        Ok(())
    }
    async fn purge_memory(&self, _id: &str) -> Result<()> {
        Ok(())
    }
    async fn get_memory_graph_structure(
        &self,
        _agent_id: Option<&str>,
        _depth: usize,
    ) -> Result<GraphStructure> {
        anyhow::bail!("Not implemented")
    }
    async fn get_related_memories(&self, _id: &str, _limit: usize) -> Result<Vec<RelatedMemory>> {
        Ok(vec![])
    }
    async fn get_memory_clusters(
        &self,
        _agent_id: Option<&str>,
        _n_clusters: usize,
    ) -> Result<Vec<Cluster>> {
        Ok(vec![])
    }
}
