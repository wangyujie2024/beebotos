//! Watch command for BeeBotOS CLI
//!
//! Provides real-time streaming of agents, blocks, events, and tasks via
//! WebSocket.

use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use futures::StreamExt;

use crate::websocket::WebSocketClient;

/// Watch command arguments
#[derive(Parser)]
pub struct WatchArgs {
    /// Resource to watch (agents, blocks, events, tasks)
    pub resource: String,

    /// Filter by agent ID
    #[arg(long)]
    pub agent: Option<String>,

    /// Output format
    #[arg(short, long, default_value = "pretty")]
    pub format: String,

    /// WebSocket endpoint (overrides default)
    #[arg(long)]
    pub ws_url: Option<String>,
}

/// Execute watch command
pub async fn execute(args: WatchArgs) -> Result<()> {
    // Get API key from environment
    let api_key = std::env::var("BEEBOTOS_API_KEY").unwrap_or_else(|_| {
        eprintln!("{}", "Warning: BEEBOTOS_API_KEY not set".yellow());
        String::new()
    });

    // Get WebSocket URL
    let ws_url = args
        .ws_url
        .or_else(|| std::env::var("BEEBOTOS_WS_URL").ok())
        .or_else(|| {
            std::env::var("BEEBOTOS_API_URL").ok().map(|url| {
                if url.starts_with("https://") {
                    url.replace("https://", "wss://") + "/ws"
                } else if url.starts_with("http://") {
                    url.replace("http://", "ws://") + "/ws"
                } else {
                    format!("ws://{}/ws", url)
                }
            })
        })
        .unwrap_or_else(|| "ws://localhost:8080/ws".to_string());

    println!("{} Connecting to WebSocket at {}...", "→".blue(), ws_url);

    match args.resource.as_str() {
        "agents" => watch_agents(&ws_url, &api_key, &args.format).await,
        "blocks" => watch_blocks(&ws_url, &api_key, &args.format).await,
        "events" => watch_events(&ws_url, &api_key, args.agent.as_deref(), &args.format).await,
        "tasks" => watch_tasks(&ws_url, &api_key, args.agent.as_deref(), &args.format).await,
        _ => {
            eprintln!("{} Unknown resource: {}", "✗".red(), args.resource);
            eprintln!("Available resources: agents, blocks, events, tasks");
            std::process::exit(1);
        }
    }
}

/// Watch agents for status changes
async fn watch_agents(ws_url: &str, api_key: &str, format: &str) -> Result<()> {
    println!(
        "{} Watching agent status changes (Ctrl+C to exit)...",
        "ℹ".blue()
    );
    println!();

    let mut client = WebSocketClient::new(ws_url, api_key);
    client.connect().await?;

    let mut stream = client.watch_agents().await?;

    while let Some(result) = stream.next().await {
        match result {
            Ok(update) => match format {
                "json" => {
                    println!("{}", serde_json::to_string(&update)?);
                }
                _ => {
                    let time = &update.timestamp;
                    let agent = &update.agent_id[..8.min(update.agent_id.len())];
                    let old = &update.old_status;
                    let new = &update.new_status;

                    let status_color = match new.as_str() {
                        "active" | "running" => "green",
                        "error" | "failed" => "red",
                        "idle" | "stopped" => "yellow",
                        _ => "white",
                    };

                    println!(
                        "[{}] Agent {}: {} → {}",
                        time.dimmed(),
                        agent.cyan(),
                        old.dimmed(),
                        new.color(status_color)
                    );
                }
            },
            Err(e) => {
                eprintln!("{} Error: {}", "✗".red(), e);
            }
        }
    }

    println!("\n{} Disconnected", "ℹ".blue());
    Ok(())
}

/// Watch blockchain blocks
async fn watch_blocks(ws_url: &str, api_key: &str, format: &str) -> Result<()> {
    println!(
        "{} Watching blockchain blocks (Ctrl+C to exit)...",
        "ℹ".blue()
    );
    println!();

    let mut client = WebSocketClient::new(ws_url, api_key);
    client.connect().await?;

    let mut stream = client.watch_blocks().await?;

    while let Some(result) = stream.next().await {
        match result {
            Ok(block) => match format {
                "json" => {
                    println!("{}", serde_json::to_string(&block)?);
                }
                _ => {
                    println!(
                        "Block {}: {} transactions, gas used {}",
                        block.number.to_string().cyan(),
                        block.tx_count.to_string().yellow(),
                        block.gas_used.to_string().dimmed()
                    );
                }
            },
            Err(e) => {
                eprintln!("{} Error: {}", "✗".red(), e);
            }
        }
    }

    println!("\n{} Disconnected", "ℹ".blue());
    Ok(())
}

/// Watch events
async fn watch_events(
    ws_url: &str,
    api_key: &str,
    agent_id: Option<&str>,
    format: &str,
) -> Result<()> {
    if let Some(id) = agent_id {
        println!("{} Watching events for agent {}...", "ℹ".blue(), id.cyan());
    } else {
        println!("{} Watching all events...", "ℹ".blue());
    }
    println!("{} Press Ctrl+C to exit", "ℹ".blue());
    println!();

    let mut client = WebSocketClient::new(ws_url, api_key);
    client.connect().await?;

    let mut stream = client.watch_events(agent_id).await?;

    while let Some(result) = stream.next().await {
        match result {
            Ok(event) => match format {
                "json" => {
                    println!("{}", serde_json::to_string(&event)?);
                }
                _ => {
                    println!(
                        "[{}] {}: {:?}",
                        event.timestamp.dimmed(),
                        event.event_type.cyan(),
                        event.data
                    );
                }
            },
            Err(e) => {
                eprintln!("{} Error: {}", "✗".red(), e);
            }
        }
    }

    println!("\n{} Disconnected", "ℹ".blue());
    Ok(())
}

/// Watch tasks
async fn watch_tasks(
    ws_url: &str,
    api_key: &str,
    agent_id: Option<&str>,
    format: &str,
) -> Result<()> {
    if let Some(id) = agent_id {
        println!("{} Watching tasks for agent {}...", "ℹ".blue(), id.cyan());
    } else {
        println!("{} Watching all tasks...", "ℹ".blue());
    }
    println!("{} Press Ctrl+C to exit", "ℹ".blue());
    println!();

    let mut client = WebSocketClient::new(ws_url, api_key);
    client.connect().await?;

    let mut stream = client.watch_tasks(agent_id).await?;

    while let Some(result) = stream.next().await {
        match result {
            Ok(task) => match format {
                "json" => {
                    println!("{}", serde_json::to_string(&task)?);
                }
                _ => {
                    let status_color = match task.status.as_str() {
                        "completed" => "green",
                        "failed" | "error" => "red",
                        "running" => "blue",
                        "pending" => "yellow",
                        _ => "white",
                    };

                    println!(
                        "[{}] Task {}: {} (agent: {})",
                        task.timestamp.dimmed(),
                        task.id[..8.min(task.id.len())].to_string().cyan(),
                        task.status.color(status_color),
                        task.agent_id[..8.min(task.agent_id.len())]
                            .to_string()
                            .dimmed()
                    );
                }
            },
            Err(e) => {
                eprintln!("{} Error: {}", "✗".red(), e);
            }
        }
    }

    println!("\n{} Disconnected", "ℹ".blue());
    Ok(())
}
